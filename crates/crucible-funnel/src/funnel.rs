//! The orchestration: grid in, verdicts out, with the denominators attached.
//!
//! This is the module that turns [`crate::walkforward`]'s *evidence* into a
//! *conclusion*. The walk-forward runner deliberately stops at a fold table
//! and says so in its own footer; everything between that table and a verdict
//! lives here:
//!
//! 1. **Claim before you run.** Every (combo, fold) gets a registry row with
//!    status `running` and the pre-registered criteria on it, written before
//!    any bar is replayed ([`crate::registry`], rule 1). The trial count moves
//!    here, once per combo, whatever happens next.
//! 2. **Replay, in parallel, merged by identity** ([`crate::scheduler`]).
//! 3. **Screen cost-free** — the S1 question, under `FreeFills`, which is the
//!    only place §2.4 sanctions it.
//! 4. **Sweep the cost assumption** — 0 / 0.5 / 1 / 2 ticks of half-spread,
//!    mandatory, every level, every combo.
//! 5. **Run the two controls** — matched random-entry and buy-and-hold — on
//!    the same bars, through the same engine, under the same fill model.
//! 6. **Judge**, against the criteria that were written down first.
//!
//! # Why the controls are runs and not formulas
//!
//! `docs/PROJECT_PLAN.md` §7.4 bans the bare dollar figure and lists the
//! denominators a number is reported with. Two of them are backtests: *vs
//! buy-and-hold* and *vs a random-entry matched baseline*. Computing them any
//! other way — annualizing an index return, assuming a coin flip's expectation
//! is zero — compares a backtest against arithmetic, and the whole point of a
//! control is that it eats the same fees, crosses the same spread and is
//! sliced by the same folds. So they are replayed, and if one cannot be built
//! the scorecard says the control is **absent** rather than printing a zero
//! that reads like a benchmark that was beaten.
//!
//! # Why `free_fills` is refused as a config's own fill model here
//!
//! The funnel runs the free-fill screen itself, at S1, and then asks S2
//! whether the edge survives honest costs. A config that declared
//! `free_fills` as its execution assumption would make those two runs the same
//! run and the cost sweep a table of one number repeated. `crucible funnel`
//! refuses it and says so (D-0006: `FreeFills` is a screening tool, never a
//! result).

use std::ops::Range;

use crucible_core::prelude::*;
use crucible_engine::{
    BacktestParams, BacktestResult, FreeFills, ReturnStats, SpreadCrossFills, Summary, run,
};
use crucible_strategies::combo::{ComboId, Grid};
use crucible_strategies::controls::{BuyAndHold, ControlError, MatchedEpisode, RandomEntry};

use crate::registry::{Registry, RunKey, RunMetrics, RunRow, RunStatus, VerdictRow};
use crate::scheduler::run_grid_parallel;
use crate::stages::{Assessment, Criteria, Evidence, Stage, assess, render_half_ticks};
use crate::walkforward::{
    ComboWalkForward, FoldPlan, RunIdentity, RunTrace, WalkForwardError, derive_control_seed,
};

/// The named execution assumption the funnel scores under (§2.4).
///
/// `free_fills` is not representable here on purpose — see the module docs.
#[derive(Clone, Copy, Debug)]
pub struct Costs {
    /// Half-spread declared by `[execution]`, in ticks.
    pub half_spread_ticks: i64,
    /// Commission per contract per side.
    pub fee_per_contract_nano_usd: NanoUsd,
}

impl Costs {
    /// The fill model at the config's own declared half-spread.
    fn declared(self, tick: Price) -> SpreadCrossFills {
        SpreadCrossFills::from_ticks(self.half_spread_ticks, tick)
            .with_fee(self.fee_per_contract_nano_usd)
    }

    /// The fill model at one sweep level.
    ///
    /// The sweep moves the **spread** and leaves the commission alone: a
    /// broker does not stop charging because the book got tight, and a sweep
    /// that zeroed fees at its first level would be measuring two things at
    /// once.
    fn at_half_ticks(self, half_ticks: i64, tick: Price) -> SpreadCrossFills {
        SpreadCrossFills::from_tick_halves(half_ticks, tick)
            .with_fee(self.fee_per_contract_nano_usd)
    }
}

/// Everything one funnel run needs. Borrowed, because all of it belongs to the
/// caller and none of it is this crate's to own.
#[derive(Clone, Copy, Debug)]
pub struct FunnelInputs<'a> {
    /// The shared bar series, collected once (§2.6).
    pub events: &'a [MarketEvent],
    /// One trading-day key per bar — the same slice `plan` was built from
    /// (D-0071).
    pub day_keys: &'a [i64],
    /// The expanded grid.
    pub grid: &'a Grid,
    /// The fold layout.
    pub plan: &'a FoldPlan,
    /// Tick and point value.
    pub spec: &'a ContractSpec,
    /// Capital and annualization.
    pub params: &'a BacktestParams,
    /// Config hash, root seed, account.
    pub identity: &'a RunIdentity,
    /// The criteria, written before the run.
    pub criteria: &'a Criteria,
    /// The exact S0 declaration, present exactly when `criteria` declares S0.
    pub s0_spec: Option<&'a crate::s0::S0Spec>,
    /// The S0 data/window declaration, under the same presence contract.
    pub s0_data_source: Option<&'a crate::s0::S0DataSourceIdentity>,
    /// The declared execution assumption.
    pub costs: Costs,
    /// Position size, needed by the controls so they trade what the strategy
    /// trades.
    pub qty: Qty,
    /// `meta.hypothesis_family`.
    pub hypothesis_family: &'a str,
    /// Repository revision (§2.5).
    pub git_sha: &'a str,
    /// blake3 of every archived file the series was read from (§2.5).
    pub data_manifest_ids: &'a [String],
    /// Wall clock, supplied by the caller. This crate reads no clock.
    pub now: &'a str,
}

/// The **per-contract** half of a funnel run's inputs.
///
/// Measured rather than guessed at: of everything [`contract_evidence`] and its
/// three helpers read off [`FunnelInputs`], exactly two things differ between
/// contracts of one pooled run — the bar series, and the annualization derived
/// from it. The grid, the spec, the costs, the criteria, the run identity and
/// the position size are shared by every contract, and a per-contract copy of
/// any of them would be a second opinion waiting to disagree with the first.
///
/// So the split is two fields, not a parallel `FunnelInputs`. That is the whole
/// bridge C6b needs: a contract's evidence can now be produced from *its own*
/// series while every shared decision stays in one place.
///
/// `bars_per_year` is genuinely per contract and not an oversight —
/// `crucible combo` measures it from the sample, because real `ohlcv` data has
/// no bar for an interval that did not trade (D-0038, D-0039), and two
/// contracts' front windows do not contain the same number of traded minutes.
/// A pooled run that annualized every contract by the first one's factor would
/// be scaling each contract's Sharpe by another contract's trading intensity.
#[derive(Clone, Copy, Debug)]
/// It carries no `instrument` yet, deliberately. A pooled report must name
/// which contract a contribution came from, but `FunnelInputs` has no such
/// field to hand over and adding one would widen a public struct and both its
/// construction sites inside a commit whose entire claim is that nothing
/// changed. The symbol arrives in C6b-i-b, from `replay_pool`, which is where
/// real per-contract identities exist.
pub struct ContractSeries<'a> {
    /// This contract's bar series — the same slice its fold plan was built
    /// from (§2.6).
    pub events: &'a [MarketEvent],
    /// Capital and annualization for this contract.
    pub params: &'a BacktestParams,
}

/// One level of the mandatory cost-sensitivity sweep.
#[derive(Clone, Debug)]
pub struct CostLevel {
    /// Half-spread, in half-ticks (`0 / 1 / 2 / 4`).
    pub half_ticks: i64,
    /// Pooled out-of-sample statistics at that level.
    pub oos_stitched: Summary,
    /// Sufficient statistics of the very series [`CostLevel::oos_stitched`]
    /// summarises, so this level can be pooled across contracts at C6 without
    /// its curve being retained.
    ///
    /// A sibling field rather than a pair, unlike [`Control::oos_stitched`],
    /// because there is no invariant here to protect: both are produced
    /// unconditionally by one `pooled_of` call, so neither can be present
    /// without the other. The type system is worth spending where a wrong
    /// state is *representable*, and here it is not.
    pub oos_stitched_stats: ReturnStats,
}

impl CostLevel {
    /// The level as the config writes it: `"0"`, `"0.5"`, `"1"`, `"2"`.
    #[must_use]
    pub fn ticks(&self) -> String {
        render_half_ticks(self.half_ticks)
    }
}

/// Matched random-entry draws the control is the **median** of.
///
/// One draw is a sample of size one: a strategy can lose to a single coin-flip
/// benchmark by luck, and a criterion that turns on that is a coin flip
/// wearing a criterion's clothes. Sixteen draws make the median stable enough
/// to compare against and cheap enough to run on every combo — and the count
/// of draws the strategy beat is reported beside it, which is the empirical
/// p-value this control can honestly produce.
///
/// It is **not** the permutation null. That one shuffles the *real returns* in
/// blocks and asks whether the edge survives, which is a different question
/// and is S3's (`crate::stats`).
pub const RANDOM_ENTRY_DRAWS: usize = 16;

/// One control, run or refused.
#[derive(Clone, Debug)]
pub struct Control {
    /// `matched random-entry` or `buy-and-hold`.
    pub name: &'static str,
    /// Pooled out-of-sample statistics and the sufficient statistics of the
    /// same series, or `None` if the control could not be built.
    ///
    /// For the random-entry control this is the **median draw** by pooled
    /// out-of-sample return, not a single draw — see [`RANDOM_ENTRY_DRAWS`].
    ///
    /// One `Option` over the pair rather than two `Option`s side by side. A
    /// control is the one place in this file where the series can be legitimately
    /// absent, so two independent `Option`s would make "summary present,
    /// statistics missing" a representable state that every consumer would then
    /// have to decide what to do about — and C6 pools these across contracts,
    /// where a silently missing contribution is a smaller denominator reported
    /// as a result. The pair costs three call sites and removes the state.
    pub oos_stitched: Option<(Summary, ReturnStats)>,
    /// Why it is absent, when it is. An absent control is reported as absent
    /// and fails its criterion — it never renders as a zero.
    pub absent_because: Option<String>,
    /// The seed of the draw that produced [`Control::oos_stitched`]. `None` for
    /// the deterministic buy-and-hold control, which has nothing to draw.
    pub seed: Option<u64>,
    /// Draws taken. 1 for a deterministic control.
    pub draws: usize,
    /// Draws the strategy's pooled out-of-sample return beat.
    ///
    /// `draws_beaten / draws` is a one-sided empirical p-value against *this*
    /// null — "the same trades, at random times" — and it is the number a
    /// reader should look at rather than the median comparison alone: beating
    /// the median says the strategy is above average against chance, beating
    /// 16 of 16 says something else.
    pub draws_beaten: usize,
}

impl Control {
    /// Pooled out-of-sample return, or `None` if the control is absent.
    #[must_use]
    pub fn return_pct(&self) -> Option<f64> {
        self.oos_stitched.as_ref().map(|(s, _)| s.total_return_pct)
    }
}

/// One combo, judged.
#[derive(Clone, Debug)]
pub struct ComboOutcome {
    /// `(config hash, combo index)`.
    pub id: ComboId,
    /// `fast(period=10) slow(period=50)`.
    pub label: String,
    /// The walk-forward evidence under the declared costs, including the
    /// captured account series.
    pub costed: ComboWalkForward,
    /// The S1 screen: pooled out-of-sample under `FreeFills`.
    pub free_fill_oos: Summary,
    /// The mandatory sweep, ascending by half-spread.
    pub sweep: Vec<CostLevel>,
    /// The two mandatory controls, in a fixed order.
    pub controls: [Control; 2],
    /// Pooled out-of-sample trading days.
    pub oos_sessions: usize,
    /// What the verdict was computed from.
    /// The verdict and every criterion behind it.
    pub assessment: Assessment,
    /// This combo's deflated Sharpe, retained beside the verdict so a renderer
    /// never has to rebuild it from a second copy of the evidence. `None` is
    /// an absence, never a zero.
    pub deflated: Option<crate::stats::deflated::Deflated>,
    /// What the registry said when this combo's rows were claimed.
    pub already_finished_runs: usize,
}

/// A whole funnel run.
#[derive(Clone, Debug)]
pub struct FunnelReport {
    /// Owned predictor evidence, when S0 was declared.
    pub s0: Option<crate::s0::S0Report>,
    /// One entry per combo, in grid-index order — never rank order.
    pub combos: Vec<ComboOutcome>,
    /// The grid's PBO, or the reason there is none.
    ///
    /// A `Result` rather than an `Option` because a scorecard that cannot show
    /// the number must show why: "there wasn't one" and "it passed" have to be
    /// distinguishable on the page, which is the whole argument behind the
    /// named-holes section this replaces.
    pub pbo: Result<crate::stats::pbo::Pbo, crate::stats::pbo::PboUnavailable>,
    /// The trial count every deflated Sharpe on this report divided by, read
    /// from the registry after this run's rows were claimed.
    pub n_trials: usize,
    /// Trials charged to the family before this run.
    pub trials_before: usize,
    /// Trials charged after it. The number a deflated Sharpe will divide by.
    pub trials_after: usize,
    /// Rows claimed for the first time.
    pub runs_claimed: usize,
    /// Rows that had already finished — the dedupe hit.
    pub runs_already_done: usize,
    /// Rows found claimed-but-unfinished: crashes from an earlier run.
    pub runs_retried: usize,
}

/// Sample standard deviation of the grid's out-of-sample Sharpes.
///
/// This is the scale that turns `expected_max_z`'s dimensionless z-score into
/// Sharpe units. `None` for fewer than two measurable combos, or for a grid
/// whose Sharpes are identical: with no dispersion there is no spread for a
/// maximum to be drawn from, and `deflated_sharpe` falls back to the combo's
/// own standard error rather than being handed a zero.
fn sharpe_dispersion(combos: &[ComboWalkForward]) -> Option<f64> {
    let sharpes: Vec<f64> = combos
        .iter()
        .filter_map(|c| c.oos_stitched.sharpe_naive)
        .filter(|s| s.is_finite())
        .collect();
    if sharpes.len() < 2 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "grid sizes are bounded well below f64's exact-integer range"
    )]
    let n = sharpes.len() as f64;
    let mean = sharpes.iter().sum::<f64>() / n;
    // `d * d`, not `powi(2)` (D-0126).
    let variance = sharpes
        .iter()
        .map(|s| {
            let d = s - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1.0);
    let sd = variance.sqrt();
    (sd.is_finite() && sd > 0.0).then_some(sd)
}

/// One contract's contribution to a combo's pooled [`Evidence`], and the
/// artifacts a report renders beside it.
///
/// Two halves, worth naming because different code reads them. The
/// **sufficient statistics** — `free_fill_oos_stats`, `costed_oos_stats`,
/// `oos_trades`, and the `ReturnStats` travelling inside each [`CostLevel`]
/// and each [`Control`] — are what [`pool_contract_evidence`] folds across
/// contracts. The **summaries** are what a scorecard prints; rendering a
/// *pooled* run is C6b's problem and not this type's.
///
/// Nothing grid-level lives here: no trial count, no PBO, no deflated Sharpe.
/// Those are properties of the search rather than of a contract, and a
/// per-contract copy of one is a second opinion waiting to disagree with the
/// first. They enter once, through [`PoolingInputs`], at the single point
/// where N contributions become one [`Evidence`].
struct ContractEvidence {
    free_fill_oos: crucible_engine::Summary,
    /// Sufficient statistics of the S1 free-fill series, so the screen pools
    /// across contracts the same way everything else does.
    ///
    /// The sweep's and the controls' statistics need no field here: they travel
    /// inside [`CostLevel`] and [`Control`], beside the summaries they describe,
    /// which is where they cannot drift out of step with them.
    free_fill_oos_stats: ReturnStats,
    /// Sufficient statistics of the **costed** out-of-sample series — the one
    /// the headline Sharpe, the deflation and both control comparisons are
    /// about.
    ///
    /// Copied out of [`ComboWalkForward`] rather than borrowed from it, which
    /// is what makes this type sufficient for pooling: a pooling step still
    /// holding the walk-forward result would be one refactor away from reaching
    /// for `max_drawdown_pct`, and a drawdown over a curve stitched across
    /// contracts describes a path nobody walked (D-0119). Forty-eight bytes is
    /// what that guarantee costs.
    costed_oos_stats: ReturnStats,
    /// Round-trips this contract closed inside its out-of-sample windows,
    /// carried for the same reason and at the same price.
    oos_trades: usize,
    sweep: Vec<CostLevel>,
    controls: [Control; 2],
}

/// Produces one contract's contribution **without assessing it**.
///
/// This is the assess seam, now carrying the load it was named for. C6a-iii-a
/// lifted the S1 screen, the cost sweep and both mandatory controls out of
/// [`run_funnel`] unaltered, so that a moved gate would have exactly one
/// candidate cause. This step takes the other half of that promise: the
/// `Evidence` construction moves **out** of here and into
/// [`pool_contract_evidence`], because an `Evidence` is a statement about the
/// pool and a contract is not the pool.
///
/// The grid-level argument list went with it. The trial count, the dispersion,
/// the de-annualization and the PBO were only ever here to build a
/// per-contract `Evidence` that nothing now assesses, so the
/// `too_many_arguments` note that deferred this signature to C6a-iii-b is
/// discharged by deletion rather than by a bundle: four arguments remain, and
/// every one of them is genuinely per contract.
fn contract_evidence(
    inputs: &FunnelInputs<'_>,
    contract: ContractSeries<'_>,
    index: usize,
    costed: &ComboWalkForward,
    test_windows: &[Range<usize>],
) -> ContractEvidence {
    // Step 3: the S1 screen, cost-free. The only sanctioned FreeFills use.
    let (free_fill_oos, free_fill_oos_stats) = pooled_of(
        &replay(inputs, contract, index, &mut FreeFills),
        contract,
        test_windows,
    );

    // Step 4: the mandatory sweep.
    let sweep: Vec<CostLevel> = inputs
        .criteria
        .cost_sweep_half_ticks
        .iter()
        .map(|&half_ticks| {
            let (oos_stitched, oos_stitched_stats) = pooled_of(
                &replay(
                    inputs,
                    contract,
                    index,
                    &mut inputs.costs.at_half_ticks(half_ticks, inputs.spec.tick),
                ),
                contract,
                test_windows,
            );
            CostLevel {
                half_ticks,
                oos_stitched,
                oos_stitched_stats,
            }
        })
        .collect();

    // Step 5: the controls, under the declared costs.
    let controls = [
        random_entry_control(inputs, contract, costed, test_windows),
        buy_and_hold_control(
            inputs,
            contract,
            test_windows,
            costed.oos_stitched.total_return_pct,
        ),
    ];

    ContractEvidence {
        free_fill_oos,
        free_fill_oos_stats,
        costed_oos_stats: costed.oos_stitched_stats,
        oos_trades: costed.oos_stitched.round_trips,
        sweep,
        controls,
    }
}

/// What every contract in one pooled combo shares.
///
/// One struct rather than eight parameters, and it is the signature revision
/// [`contract_evidence`]'s `too_many_arguments` note deferred to this step: the
/// grid-level half of that argument list moved here, and the per-contract
/// function stopped taking any of it.
///
/// Everything in it is shared **by construction**, which is what makes pooling
/// something to do once rather than per contract and reconcile afterwards: PBO
/// is a property of the search (D-0109), the trial count is the registry's and
/// nobody else's (D-0083), and the deflation units come from the config
/// (D-0125). There is no per-contract variant of any of them for a pooling step
/// to invent.
struct PoolingInputs<'a> {
    /// **Distinct** out-of-sample trading days across every pooled contract —
    /// the union, never the sum (D-0114).
    ///
    /// Supplied by the caller rather than derived here, and that is the design
    /// rather than a shortcut. Sufficient statistics carry no day identity, so
    /// two contracts' `ReturnStats` cannot tell this function they traded the
    /// same Tuesday; **summing is the only thing it could do** if it derived
    /// the number, and summing is precisely the defect D-0114 exists to
    /// prevent. The union is computed where the day keys actually are, by
    /// [`crate::pooling::PooledSessions`].
    distinct_oos_sessions: usize,
    /// Trials charged to the hypothesis family, read from the registry and
    /// nowhere else, so voided runs are already excluded (D-0083).
    n_trials: usize,
    /// The grid's probability of backtest overfitting, or `None` when CSCV
    /// could not be computed.
    pbo: Option<f64>,
    /// Dispersion of the grid's out-of-sample Sharpes, already converted to
    /// per-observation units (D-0125).
    trial_sharpe_dispersion: Option<f64>,
    /// Annualized Sharpe to per-observation Sharpe, the other half of D-0125.
    deannualize: &'a dyn Fn(f64) -> f64,
    /// The sweep level the `kill_if_dead` criterion reads.
    kill_if_dead_half_ticks: i64,
    /// Declared capital. Every contract's stitched curve starts here, which is
    /// what makes a pooled final equity `initial + Σ deltas` and therefore
    /// exact in integer arithmetic (D-0127).
    initial_cash_nano_usd: NanoUsd,
    /// Annualization for the pooled Sharpe.
    bars_per_year: f64,
}

/// A series pooled across contracts, and the two numbers such a series may
/// honestly report.
///
/// Deliberately **not** a [`Summary`]. A `Summary` carries `max_drawdown_pct`,
/// and a drawdown over a curve whose seams join contracts months apart
/// describes a path no account walked (D-0119, D-0127). This type cannot
/// express one — which is `crucible_engine::sharpe_and_shape`'s argument
/// applied one level up: a computed-then-discarded drawdown is one refactor
/// away from being surfaced, and whoever does that refactor sees a field
/// sitting there looking available.
struct PooledSeries {
    /// The folded statistics themselves, retained because a deflation needs the
    /// shape of the very series its Sharpe came from and never a shape
    /// recomputed from another one.
    stats: ReturnStats,
    /// Pooled out-of-sample total return.
    total_return_pct: f64,
    /// Pooled out-of-sample naive Sharpe, absent on a series too short or too
    /// flat to have one.
    sharpe_naive: Option<f64>,
}

impl PoolingInputs<'_> {
    /// Folds N contracts' statistics for one series into the pooled answer.
    ///
    /// **In declared contract order**, which is part of the definition rather
    /// than an implementation detail (D-0127): `ReturnStats::combine` is not
    /// associative in the last bits, so a fold in completion order would be a
    /// different number — and §2.2 already requires parallel results to merge
    /// by run identity rather than by whichever finished first.
    ///
    /// The total return is formed as **final over initial**, not delta over
    /// initial. The same quantity arithmetically; not the same float
    /// operations, and the first spelling is `Summary::compute`'s — so a pool
    /// of one is bit-identical to the number that combo already reported.
    /// That identity is what keeps this step inert while D-0117 holds every
    /// pool to one contract, and it is asserted by
    /// `a_pool_of_one_reproduces_the_contracts_own_numbers`.
    ///
    /// # Panics
    /// On an empty iterator. Every call site builds it as
    /// `once(first).chain(rest)`, so a pool of nothing is unrepresentable at
    /// the call site and arriving here would be a wiring bug rather than a
    /// data condition.
    fn pooled_series(&self, series: impl Iterator<Item = ReturnStats>) -> PooledSeries {
        let stats = series.reduce(ReturnStats::combine).expect(
            "INVARIANT: pool_contract_evidence takes the first contract separately and chains \
             the rest, so every fold reaching here has at least one operand",
        );
        let initial = self.initial_cash_nano_usd;
        let total_return_pct = if initial == 0 {
            0.0
        } else {
            (nano_usd_to_f64(initial + stats.net_delta_nano_usd) / nano_usd_to_f64(initial) - 1.0)
                * 100.0
        };
        PooledSeries {
            total_return_pct,
            sharpe_naive: stats.sharpe(self.bars_per_year),
            stats,
        }
    }
}

/// N contracts' contributions, pooled into the one [`Evidence`] a combo is
/// judged on — so [`assess`] runs **once per combo**, never once per contract.
///
/// Assessing per contract and reconciling verdicts afterwards was the other
/// option, and it is not a near miss. Every criterion in [`assess`] is a
/// threshold on a pooled quantity, so a rule for combining N verdicts ("most
/// of them", "all of them", "the worst") would be a second gate, unwritten and
/// unregistered, sitting behind the pre-registered one. And it would not even
/// work: Block C exists because one contract's ~60 sessions cannot clear a
/// registered 250-session floor, and N contracts each failing that floor
/// separately is the same answer at N times the cost.
///
/// The pool is `(first, rest)` rather than one slice because a pool of nothing
/// is not a pool — [`crate::pooling::PoolingError::NoContracts`] refuses it
/// upstream, and every number here would otherwise have to be invented from an
/// empty fold. An invented zero return is exactly the "zero that reads like a
/// benchmark that was beaten" D-0075 refuses. Making emptiness unrepresentable
/// costs one `&[]` at the call site.
///
/// # What is pooled, and how
///
/// **Trades are summed; sessions are not.** A round-trip belongs to exactly one
/// contract, so two contracts' trade counts count disjoint events and adding
/// them invents nothing. A trading day belongs to no contract: ESH2024 and
/// ESM2024 trade the same Tuesday, so adding their session counts claims a
/// sample twice the size of the one that exists (D-0114). That asymmetry is
/// [`crate::pooling`]'s one arithmetic rule, and this is the function where it
/// either holds or is quietly lost.
///
/// **An absent control on ANY contract makes the pooled control absent** —
/// never "pool the ones that ran". That would compare a strategy measured over
/// N contracts against a benchmark measured over N−1: a smaller denominator
/// reported as a result, when D-0075 is already explicit that an absent control
/// **fails** its criterion rather than passing it. A sweep level missing from
/// any contract's sweep is governed by the same rule, for the same reason.
///
/// **Path statistics are not pooled at all**, and cannot be from here: what a
/// fold yields is a [`PooledSeries`], which has no drawdown to offer (D-0119).
fn pool_contract_evidence(
    first: &ContractEvidence,
    rest: &[ContractEvidence],
    shared: &PoolingInputs<'_>,
) -> Evidence {
    let contracts = || std::iter::once(first).chain(rest.iter());

    let costed = shared.pooled_series(contracts().map(|c| c.costed_oos_stats));
    let free_fill = shared.pooled_series(contracts().map(|c| c.free_fill_oos_stats));

    // `collect::<Option<Vec<_>>>` is the "any contract missing ⇒ absent" rule
    // written as code: it short-circuits to `None` on the first contract whose
    // sweep does not carry the level, rather than pooling the rest.
    let sharpe_at_kill_level = contracts()
        .map(|c| {
            c.sweep
                .iter()
                .find(|l| l.half_ticks == shared.kill_if_dead_half_ticks)
                .map(|l| l.oos_stitched_stats)
        })
        .collect::<Option<Vec<_>>>()
        .and_then(|levels| shared.pooled_series(levels.into_iter()).sharpe_naive);

    let control_return_pct = |which: usize| {
        contracts()
            .map(|c| c.controls[which].oos_stitched.as_ref().map(|(_, s)| *s))
            .collect::<Option<Vec<_>>>()
            .map(|stats| shared.pooled_series(stats.into_iter()).total_return_pct)
    };

    Evidence {
        oos_trades: contracts().map(|c| c.oos_trades).sum(),
        oos_sessions: shared.distinct_oos_sessions,
        free_fill_return_pct: free_fill.total_return_pct,
        costed_return_pct: costed.total_return_pct,
        costed_sharpe: costed.sharpe_naive,
        sharpe_at_kill_level,
        random_entry_return_pct: control_return_pct(0),
        buy_and_hold_return_pct: control_return_pct(1),
        // The permutation harness is not wired into the run path yet:
        // block A ships the harness and its acceptance test first
        // (docs/plans/m3-full.md).
        permutation_p_value: None,
        // Deflated against the registry's trial count and the shape of the
        // very series the Sharpe came from — never a shape recomputed from
        // a different series (`ReturnShape`). `None` propagates: a combo
        // with no Sharpe, or with a flat window that has no higher
        // moments, has no deflated Sharpe either, and that is an absence
        // rather than a zero.
        //
        // Pooling changes which series that is and nothing else: the shape
        // comes off the same folded statistics the Sharpe above was read
        // from, so the two cannot describe different samples.
        deflated: costed.sharpe_naive.and_then(|observed| {
            let shape = costed.stats.shape();
            crate::stats::deflated::deflated_sharpe(crate::stats::deflated::DeflationInputs {
                observed_sharpe: (shared.deannualize)(observed),
                skew: shape.skew?,
                kurtosis: shape.kurtosis?,
                n_observations: shape.n_returns,
                n_trials: shared.n_trials,
                trial_sharpe_dispersion: shared.trial_sharpe_dispersion,
            })
        }),
        n_trials: shared.n_trials,
        pbo: shared.pbo,
    }
}

/// Anything that stops a funnel run.
#[derive(Debug)]
pub enum FunnelError {
    /// S0 evidence did not exactly match this run's declared grid and contract.
    InvalidS0(String),
    /// The grid could not be replayed.
    WalkForward(WalkForwardError),
    /// The registry could not be read or written. Fatal by design: a result
    /// with no row is a number nobody can reproduce (§2.5).
    Registry(crate::registry::RegistryError),
}

impl std::fmt::Display for FunnelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunnelError::InvalidS0(e) => write!(f, "invalid S0 evidence: {e}"),
            FunnelError::WalkForward(e) => write!(f, "{e}"),
            FunnelError::Registry(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FunnelError {}

impl From<WalkForwardError> for FunnelError {
    fn from(e: WalkForwardError) -> FunnelError {
        FunnelError::WalkForward(e)
    }
}

impl From<crate::registry::RegistryError> for FunnelError {
    fn from(e: crate::registry::RegistryError) -> FunnelError {
        FunnelError::Registry(e)
    }
}

fn validate_s0_report(
    inputs: &FunnelInputs<'_>,
    registry: &Registry,
    report: Option<&crate::s0::S0Report>,
) -> Result<(), FunnelError> {
    let declared = inputs.criteria.runs(Stage::S0);
    if !declared {
        if report.is_some() {
            return Err(FunnelError::InvalidS0(
                "evidence was supplied although S0 was not declared".to_owned(),
            ));
        }
        if inputs.s0_spec.is_some() || inputs.s0_data_source.is_some() {
            return Err(FunnelError::InvalidS0(
                "an S0 declaration exists although the stage is not declared".to_owned(),
            ));
        }
        return Ok(());
    }
    let spec = inputs.s0_spec.ok_or_else(|| {
        FunnelError::InvalidS0("criteria declare S0 but the declaration is absent".to_owned())
    })?;
    crate::s0::validate_s0_criteria_contract(inputs.criteria, spec)
        .map_err(FunnelError::InvalidS0)?;
    let data_source = inputs.s0_data_source.ok_or_else(|| {
        FunnelError::InvalidS0(
            "criteria declare S0 but the data/window declaration is absent".to_owned(),
        )
    })?;
    let report = report.ok_or_else(|| {
        FunnelError::InvalidS0("criteria declare S0 but its measured report is absent".to_owned())
    })?;

    let expected_registration = crate::s0::s0_run_registration_hash(
        inputs.grid.spec(),
        spec,
        inputs.spec.tick,
        data_source,
        inputs.events,
        inputs.day_keys,
        inputs.data_manifest_ids,
    )
    .map_err(|error| FunnelError::InvalidS0(error.to_string()))?;
    if inputs.identity.config_hash != expected_registration {
        return Err(FunnelError::InvalidS0(format!(
            "supplied run identity {} does not match derived registration {expected_registration}",
            inputs.identity.config_hash
        )));
    }
    report.validate().map_err(FunnelError::InvalidS0)?;
    if report.registration_hash != expected_registration.to_string() {
        return Err(FunnelError::InvalidS0(
            "report registration hash must match every run claim".to_owned(),
        ));
    }
    if report.combos.len() != inputs.grid.len() {
        return Err(FunnelError::InvalidS0(
            "report and grid must contain the same number of combos".to_owned(),
        ));
    }
    validate_registry_s0_results(
        inputs.identity,
        inputs.grid,
        spec,
        inputs.spec.tick,
        registry,
        report,
    )
    .map_err(FunnelError::InvalidS0)
}

pub(crate) fn validate_registry_s0_results(
    identity: &RunIdentity,
    grid: &Grid,
    spec: &crate::s0::S0Spec,
    tick_size: Price,
    registry: &Registry,
    report: &crate::s0::S0Report,
) -> Result<(), String> {
    for (metrics, combo) in report.combos.iter().zip(grid.iter()) {
        let key = crate::s0::s0_run_key(identity, combo.index);
        if registry.status_of(&key) != Some(RunStatus::Done) {
            return Err(format!(
                "combo {} registry S0 result is not in Done status",
                combo.index
            ));
        }
        let contract = crate::s0::S0ResultContract::for_combo(
            &identity.config_hash,
            grid.spec(),
            spec,
            tick_size,
            &combo,
        );
        contract.validate(&key, metrics)?;
        match registry.result_of(&key) {
            Some(crate::registry::PersistedRunResult::S0(persisted))
                if persisted.decision_bytes() == metrics.decision_bytes() => {}
            Some(crate::registry::PersistedRunResult::S0(_)) => {
                return Err(format!(
                    "combo {} report disagrees with its registry-owned S0 result",
                    combo.index
                ));
            }
            Some(crate::registry::PersistedRunResult::LegacyAbsent) => {
                return Err(format!(
                    "combo {} has only historical metrics:null in the registry",
                    combo.index
                ));
            }
            Some(crate::registry::PersistedRunResult::Trading(_)) => {
                return Err(format!(
                    "combo {} has trading metrics where S0 evidence is required",
                    combo.index
                ));
            }
            None => {
                return Err(format!(
                    "combo {} has no registry-owned S0 result",
                    combo.index
                ));
            }
        }
    }
    Ok(())
}

/// Runs the funnel: claim, replay, screen, sweep, control, judge, record.
///
/// # Errors
/// [`FunnelError`] if the grid cannot be replayed or the registry cannot be
/// written.
pub fn run_funnel(
    inputs: &FunnelInputs<'_>,
    registry: &mut Registry,
    s0: Option<crate::s0::S0Report>,
) -> Result<FunnelReport, FunnelError> {
    validate_s0_report(inputs, registry, s0.as_ref())?;
    let trials_before = registry.trials_for(inputs.hypothesis_family);
    let claims = claim_runs(inputs, registry)?;

    // Step 2: the declared-cost replay, in parallel, merged by grid index.
    let declared = inputs.costs.declared(inputs.spec.tick);
    let report = run_grid_parallel(
        inputs.events,
        inputs.day_keys,
        inputs.grid,
        inputs.plan,
        inputs.spec,
        inputs.params,
        inputs.identity,
        &declared,
    )?;

    let test_windows: Vec<Range<usize>> = inputs
        .plan
        .folds()
        .iter()
        .map(|f| f.test.bars.clone())
        .collect();
    let oos_sessions: usize = inputs.plan.folds().iter().map(|f| f.test.n_days()).sum();

    // ---- Block B's grid-level statistics, computed once, before any combo is
    // assessed.
    //
    // PBO is a property of the **search**, not of a combo: it asks how often
    // picking the in-sample winner would have been wrong out of sample, so
    // every combo's assessment reads the same number. Computing it inside the
    // loop would invite a per-combo variant, which is a statistic nobody can
    // interpret.
    //
    // The trial count is read from the registry and nowhere else
    // (`trials_from_registry`), so it already excludes voided runs by
    // construction (D-0083) and already includes the rows this run just
    // claimed. A local recount would be a second opinion about the
    // denominator, and the denominator is the entire point.
    let n_trials = crate::stats::deflated::trials_from_registry(registry, inputs.hypothesis_family);

    // The CSCV blocks ARE the folds. `FoldPlan` is the sole boundary authority
    // in this codebase (D-0062, D-0071); cutting a second set of blocks here
    // would be a second answer to "which observations are out of sample".
    //
    // The per-cell metric is the fold's out-of-sample **return**, not its
    // Sharpe, for two reasons. It is always defined — a fold with no trades has
    // a return of zero and no Sharpe at all, so a Sharpe matrix would make PBO
    // absent exactly on the quiet configs it is most useful for. And D-0063
    // already rebases every per-fold percentage to the config's declared
    // capital, so folds and combos are on one scale without further work.
    let fold_performance: Vec<Vec<f64>> = report
        .combos
        .iter()
        .map(|combo| {
            combo
                .folds
                .iter()
                .map(|fold| fold.oos.total_return_pct)
                .collect()
        })
        .collect();
    let pbo = crate::stats::pbo::probability_of_backtest_overfitting(&fold_performance);

    // Dispersion of the grid's out-of-sample Sharpes, which is what turns the
    // dimensionless expected-maximum z-score into Sharpe units. Absent for a
    // grid of one, where there was no search to correct for.
    let trial_sharpe_dispersion = sharpe_dispersion(&report.combos);

    // **Deflation happens in PER-OBSERVATION units, and both inputs are
    // converted here** (D-0125).
    //
    // `Summary::sharpe_naive` is ANNUALIZED — `crucible-engine::metrics`
    // multiplies by `periods_per_year.sqrt()` — while `ReturnShape::n_returns`
    // is a raw per-bar count. Bailey & Lopez de Prado's estimator requires
    // both at ONE frequency, and `DeflationInputs::observed_sharpe` says so in
    // its own doc: "already annualized or not, as long as `n_observations`
    // matches it". They did not match. At 1-minute bars the factor is
    // sqrt(525_949) ~ 725, which inflates the observed ratio far past anything
    // the standard error can absorb, so `P(true Sharpe > 0)` saturates: 1.0000
    // or 0.0000 and nothing between. A headline that looks maximally confident
    // and carries no information.
    //
    // The DISPERSION is converted by the same factor, and that is not
    // cosmetic. It is the sd of the grid's own annualized Sharpes, and it is
    // what scales the expected-maximum z-score into Sharpe units — leaving it
    // annualized while de-annualizing the observation would compare a number
    // to a benchmark 725 times too large and kill every combo instead.
    //
    // Chosen direction: convert DOWN to per-observation rather than up,
    // because `n_observations` is a count and cannot be rescaled without
    // inventing a sample size.
    let (deannualize, trial_sharpe_dispersion) =
        deflation_units(inputs.params.bars_per_year, trial_sharpe_dispersion);

    // The one contract this build replays, in the shape a pool takes. D-0117
    // refuses every `[pooling]` config, so a funnel run has exactly one — and
    // naming it `ContractSeries` here rather than reaching into `inputs` is the
    // bridge: `contract_evidence` no longer knows whether its series came from
    // a single-contract run or from one member of a pool.
    //
    // `instrument` is the config's declared universe entry, which
    // `collect_events` already narrowed to one (D-0117 again).
    let series = ContractSeries {
        events: inputs.events,
        params: inputs.params,
    };

    // The grid-level half of every combo's evidence, built once. None of it
    // varies by combo or by contract, which is the property that lets the pool
    // below be assessed once rather than reconciled.
    let shared = PoolingInputs {
        distinct_oos_sessions: oos_sessions,
        n_trials,
        pbo: pbo.as_ref().ok().map(|p| p.value),
        trial_sharpe_dispersion,
        deannualize: &deannualize,
        kill_if_dead_half_ticks: inputs.criteria.kill_if_dead_half_ticks,
        initial_cash_nano_usd: inputs.params.initial_cash_nano_usd,
        bars_per_year: inputs.params.bars_per_year,
    };

    let mut combos = Vec::with_capacity(report.combos.len());
    for costed in report.combos {
        let index = costed.id.combo_index;

        let contract = contract_evidence(inputs, series, index, &costed, &test_windows);
        // `rest` is empty at the only call site in this build, and that is
        // D-0117 holding rather than an omission: every well-formed `[pooling]`
        // config is refused, so no funnel run has a second contract to pool.
        // C6b is where the refusal lifts and this slice stops being empty.
        //
        // The single-contract path goes through the pooled one anyway, because
        // a pool of one is bit-identical to it — which is what makes C6b a
        // wiring change rather than a rewrite of the number five gates pin.
        let evidence = pool_contract_evidence(&contract, &[], &shared);
        let ContractEvidence {
            free_fill_oos,
            sweep,
            controls,
            ..
        } = contract;
        // `validate_s0_report` established exact contiguous grid identity
        // before any run was claimed, so assessment and rendering cannot pick
        // different copies of a duplicate or silently miss one.
        let s0_combo = s0
            .as_ref()
            .and_then(|report| report.combos.get(index))
            .map(|metrics| &metrics.combo);
        let assessment = assess(inputs.criteria, &evidence, s0_combo);

        combos.push(ComboOutcome {
            id: costed.id,
            label: costed.label.clone(),
            already_finished_runs: claims.already_done_per_combo[index],
            costed,
            free_fill_oos,
            sweep,
            controls,
            oos_sessions,
            deflated: evidence.deflated,
            assessment,
        });
    }

    finish_runs(inputs, registry, &combos)?;
    record_verdicts(inputs, registry, &combos)?;

    Ok(FunnelReport {
        s0,
        trials_before,
        trials_after: registry.trials_for(inputs.hypothesis_family),
        runs_claimed: claims.claimed,
        runs_already_done: claims.already_done,
        runs_retried: claims.retried,
        combos,
        pbo,
        n_trials,
    })
}

/// What claiming the grid's rows found already on disk.
struct Claims {
    claimed: usize,
    already_done: usize,
    retried: usize,
    already_done_per_combo: Vec<usize>,
}

/// Registry rule 1, applied to the whole grid **before** anything replays.
///
/// Serial and up-front rather than interleaved with the replay: the rows have
/// to exist before the work starts, and a `Registry` is a single append-only
/// handle that rayon's workers would otherwise have to queue behind. Claiming
/// 4,000 rows costs one pass over a file; interleaving would cost a mutex on
/// every combo.
fn claim_runs(inputs: &FunnelInputs<'_>, registry: &mut Registry) -> Result<Claims, FunnelError> {
    let mut claims = Claims {
        claimed: 0,
        already_done: 0,
        retried: 0,
        already_done_per_combo: vec![0; inputs.grid.len()],
    };
    for index in 0..inputs.grid.len() {
        let combo = inputs.grid.combo(index);
        for fold in inputs.plan.folds() {
            let key = run_key(inputs, index, Some(fold.index));
            let row = RunRow {
                key,
                hypothesis_family: inputs.hypothesis_family.to_owned(),
                params: combo.label(),
                fill_model: "spread_cross".to_owned(),
                git_sha: inputs.git_sha.to_owned(),
                data_manifest_ids: inputs.data_manifest_ids.to_vec(),
                started_at: inputs.now.to_owned(),
                criteria: inputs.criteria.clone(),
            };
            match registry.insert_running(&row)? {
                crate::registry::Inserted::New => claims.claimed += 1,
                crate::registry::Inserted::AlreadyDone => {
                    claims.already_done += 1;
                    claims.already_done_per_combo[index] += 1;
                }
                crate::registry::Inserted::Retrying => claims.retried += 1,
            }
        }
    }
    Ok(claims)
}

fn finish_runs(
    inputs: &FunnelInputs<'_>,
    registry: &mut Registry,
    combos: &[ComboOutcome],
) -> Result<(), FunnelError> {
    for outcome in combos {
        for fold in &outcome.costed.folds {
            let key = run_key(inputs, outcome.id.combo_index, Some(fold.fold_index));
            registry.finish(
                &key,
                RunStatus::Done,
                Some(metrics_of(&fold.oos)),
                inputs.now,
            )?;
        }
    }
    Ok(())
}

fn record_verdicts(
    inputs: &FunnelInputs<'_>,
    registry: &mut Registry,
    combos: &[ComboOutcome],
) -> Result<(), FunnelError> {
    let trials = registry.trials_for(inputs.hypothesis_family);
    for outcome in combos {
        registry.record_verdict(&VerdictRow {
            config_hash: inputs.identity.config_hash.to_string(),
            account_id: inputs.identity.account_id.clone(),
            combo_index: outcome.id.combo_index,
            hypothesis_family: inputs.hypothesis_family.to_owned(),
            decided_at: outcome.assessment.decided_at.to_string(),
            verdict: outcome.assessment.verdict,
            reasons: outcome.assessment.rendered_reasons(),
            trials_at_decision: trials,
            decided_on: inputs.now.to_owned(),
        })?;
    }
    Ok(())
}

fn run_key(inputs: &FunnelInputs<'_>, combo_index: usize, fold: Option<usize>) -> RunKey {
    let seed = crate::walkforward::derive_run_seed(
        &inputs.identity.config_hash,
        inputs.identity.root_seed,
        inputs.identity.account_id.as_deref(),
        combo_index,
        fold.unwrap_or(usize::MAX),
    );
    RunKey {
        config_hash: inputs.identity.config_hash.to_string(),
        account_id: inputs.identity.account_id.clone(),
        combo_index,
        fold,
        seed,
    }
}

fn metrics_of(s: &Summary) -> RunMetrics {
    RunMetrics {
        final_equity_nano_usd: s.final_equity_nano_usd,
        return_pct: s.total_return_pct,
        max_dd_pct: s.max_drawdown_pct,
        sharpe_naive: s.sharpe_naive,
        round_trips: s.round_trips,
        win_rate: s.win_rate,
        fees_nano_usd: s.fees_nano_usd,
    }
}

/// One extra replay of a combo under a different fill model.
///
/// The sweep and the S1 screen are separate *runs*, not adjustments to a
/// number: a half-spread changes which price every fill happened at, which
/// changes the mark-to-market path, which changes the drawdown and the Sharpe.
/// Subtracting an estimated cost from a finished equity curve would get the
/// final dollar right and every other number wrong.
fn replay<M: FillModel>(
    inputs: &FunnelInputs<'_>,
    contract: ContractSeries<'_>,
    index: usize,
    fills: &mut M,
) -> BacktestResult {
    let mut strategy = inputs.grid.aligned_strategy(index);
    let mut feed = SliceFeed {
        events: contract.events,
        at: 0,
    };
    run(
        &mut feed,
        &mut strategy,
        fills,
        inputs.spec,
        contract.params,
    )
    .expect(
        "INVARIANT: the shared bar series is availability-ordered; the declared-cost replay \
             over the identical slice already succeeded",
    )
}

/// One replay's test windows stitched, and the sufficient statistics of the
/// series that stitch produced.
///
/// Widened rather than given a sibling, which is the opposite call from
/// [`RunTrace::pooled_with_stats`] one layer down, for the reason that made
/// that one a sibling: blast radius is the symptom, **caller need is the
/// cause**.
///
/// Most callers of `pooled` want a `Summary` alone, so widening *it* would
/// charge every uninterested call site for the one interested caller. Every
/// caller of `pooled_of` produces a series that must be poolable across
/// contracts — the S1 free-fill screen, each cost-sweep level, each
/// random-entry draw, buy-and-hold — so all of them want the statistics, and a
/// sibling here would have been left with no callers at all.
///
/// Stated as the property rather than as call-site counts, and that is a
/// repair. This comment used to say "nine callers, and seven now"; a count is a
/// snapshot wearing a property's clothes, it was already one commit out of date
/// when it was read, and the number was never what decided the question. What
/// decides it is whether the majority of callers want the new return value —
/// which is checkable by reading them, and stays true as they come and go.
fn pooled_of(
    result: &BacktestResult,
    contract: ContractSeries<'_>,
    test_windows: &[Range<usize>],
) -> (Summary, ReturnStats) {
    RunTrace::new(&result.equity, &result.closed_trades, &result.fee_events).pooled_with_stats(
        test_windows,
        contract.params.initial_cash_nano_usd,
        contract.params.bars_per_year,
    )
}

/// The matched random-entry control.
///
/// Matching happens **per test window**, and the holding length matched is the
/// part of each round-trip that was *inside* that window. That is the exposure
/// the window's numbers were computed on: a round-trip opened in a training
/// window and closed in a test one is a test-window trade (D-0063), but only
/// the bars after the boundary contributed to the test window's marks, and a
/// control given the whole holding length would carry more risk than the thing
/// it benchmarks.
///
/// Per window rather than across the pooled series for the same reason:
/// placing an episode across a seam would put its exposure in a training
/// window, where the pooled curve does not look.
fn random_entry_control(
    inputs: &FunnelInputs<'_>,
    contract: ContractSeries<'_>,
    costed: &ComboWalkForward,
    test_windows: &[Range<usize>],
) -> Control {
    let name = "matched random-entry";
    let absent = |why: String| Control {
        name,
        oos_stitched: None,
        absent_because: Some(why),
        seed: None,
        draws: 0,
        draws_beaten: 0,
    };

    let per_window: Vec<(Range<usize>, Vec<MatchedEpisode>)> = test_windows
        .iter()
        .map(|w| {
            let episodes = costed
                .round_trip_bars
                .iter()
                .filter(|&&(_, close, _)| w.contains(&close))
                .map(|&(open, close, direction)| MatchedEpisode {
                    holding_bars: (close - open.max(w.start)).max(1),
                    direction,
                })
                .collect();
            (w.clone(), episodes)
        })
        .collect();

    // One stream per draw, domain-separated by its index, so the draws are
    // reproducible individually and no two of them share a schedule.
    let mut draws: Vec<(u64, Summary, ReturnStats)> = Vec::with_capacity(RANDOM_ENTRY_DRAWS);
    for i in 0..RANDOM_ENTRY_DRAWS {
        let seed = derive_control_seed(
            &inputs.identity.config_hash,
            inputs.identity.root_seed,
            inputs.identity.account_id.as_deref(),
            costed.id.combo_index,
            &format!("random_entry:{i}"),
        );
        match RandomEntry::matched_across(seed, &per_window, inputs.qty) {
            Ok(mut control) => {
                let mut feed = SliceFeed {
                    events: contract.events,
                    at: 0,
                };
                let mut fills = inputs.costs.declared(inputs.spec.tick);
                let result = run(
                    &mut feed,
                    &mut control,
                    &mut fills,
                    inputs.spec,
                    contract.params,
                )
                .expect("INVARIANT: the shared bar series is availability-ordered");
                let (summary, stats) = pooled_of(&result, contract, test_windows);
                draws.push((seed, summary, stats));
            }
            Err(ControlError::NothingToMatch) => {
                return absent(
                    "the combo closed no round-trips inside any test window, so there is no \
                     trade count, holding time or direction mix to match"
                        .to_owned(),
                );
            }
            Err(e) => return absent(e.to_string()),
        }
    }

    // Sort by return, then by seed: two draws with identical returns must not
    // be ordered by whichever the sort happened to see first (§2.2).
    draws.sort_by(|a, b| {
        a.1.total_return_pct
            .total_cmp(&b.1.total_return_pct)
            .then(a.0.cmp(&b.0))
    });
    let strategy_pct = costed.oos_stitched.total_return_pct;
    let beaten = draws
        .iter()
        .filter(|(_, s, _)| strategy_pct > s.total_return_pct)
        .count();
    // Lower median of an even sample: the pessimistic half, so a strategy that
    // ties the middle of the null does not clear the bar on a rounding choice.
    let (seed, median, median_stats) = draws.swap_remove((RANDOM_ENTRY_DRAWS - 1) / 2);
    Control {
        name,
        oos_stitched: Some((median, median_stats)),
        absent_because: None,
        seed: Some(seed),
        draws: RANDOM_ENTRY_DRAWS,
        draws_beaten: beaten,
    }
}

/// The buy-and-hold control: own the instrument from the bar the evaluation
/// window opens at.
///
/// Sliced to the same test windows as everything else, so what it reports is
/// "what owning the thing during the out-of-sample windows would have paid" —
/// not what owning it for the whole series would have.
fn buy_and_hold_control(
    inputs: &FunnelInputs<'_>,
    contract: ContractSeries<'_>,
    test_windows: &[Range<usize>],
    strategy_pct: f64,
) -> Control {
    let mut control = BuyAndHold::new(inputs.grid.max_warmup_bars(), inputs.qty);
    let mut feed = SliceFeed {
        events: contract.events,
        at: 0,
    };
    let mut fills = inputs.costs.declared(inputs.spec.tick);
    let result = run(
        &mut feed,
        &mut control,
        &mut fills,
        inputs.spec,
        contract.params,
    )
    .expect("INVARIANT: the shared bar series is availability-ordered");
    let (oos, oos_stats) = pooled_of(&result, contract, test_windows);
    Control {
        name: "buy-and-hold",
        // One "draw", because owning the thing is not a random variable. The
        // field is still filled in so `draws_beaten / draws` reads the same
        // way for both controls rather than being a special case a reader has
        // to remember.
        draws_beaten: usize::from(strategy_pct > oos.total_return_pct),
        oos_stitched: Some((oos, oos_stats)),
        absent_because: None,
        seed: None,
        draws: 1,
    }
}

/// Replays a pre-collected bar series — the same one every combo saw (§2.6).
struct SliceFeed<'a> {
    events: &'a [MarketEvent],
    at: usize,
}

impl Feed for SliceFeed<'_> {
    fn next_event(&mut self) -> Option<MarketEvent> {
        let event = self.events.get(self.at)?.clone();
        self.at += 1;
        Some(event)
    }
}

/// The combos this run did not kill.
///
/// In grid-index order, like everything else here: a survivor list sorted by
/// out-of-sample performance is a selection step wearing a report's clothes.
#[must_use]
pub fn survivors(report: &FunnelReport) -> Vec<&ComboOutcome> {
    report
        .combos
        .iter()
        .filter(|c| c.assessment.verdict != crate::Verdict::Kill)
        .collect()
}

/// Converts a grid's ANNUALIZED Sharpes into the per-observation units a
/// deflated Sharpe requires (D-0125).
///
/// Returns the conversion to apply to an observed Sharpe, and the converted
/// trial dispersion. Extracted as a named seam because the defect it replaces
/// lived in the *wiring* rather than in the estimator: `stats::deflated` was
/// always correct given matching inputs, and a unit test of the formula passes
/// on the broken build. The control has to sit where the units are decided.
///
/// Both are divided by `sqrt(bars_per_year)`. Converting DOWN rather than up,
/// because `n_observations` is a count and cannot be rescaled without
/// inventing a sample size.
fn deflation_units(
    bars_per_year: f64,
    trial_sharpe_dispersion: Option<f64>,
) -> (impl Fn(f64) -> f64, Option<f64>) {
    let annualization = bars_per_year.sqrt();
    (
        move |annualized: f64| annualized / annualization,
        trial_sharpe_dispersion.map(|sd| sd / annualization),
    )
}

#[cfg(test)]
mod deflation_units_tests {
    use super::deflation_units;

    /// One minute bars: 525,949 per year, so the factor is ~725.
    #[test]
    fn both_inputs_are_divided_by_the_annualization_factor() {
        let bpy = 525_949.0_f64;
        let (deannualize, dispersion) = deflation_units(bpy, Some(7.25));
        let expected = bpy.sqrt();
        assert!(
            (deannualize(expected) - 1.0).abs() < 1e-12,
            "the observed ratio is converted"
        );
        assert!(
            (dispersion.expect("some") - 7.25 / expected).abs() < 1e-12,
            "and the DISPERSION by the same factor — leaving it annualized would              compare a converted observation against a benchmark {expected:.0}x too large              and kill every combo instead of passing every one"
        );
    }

    /// The factor is real at trading frequencies — a conversion that did
    /// nothing would satisfy an invariance test but not this.
    #[test]
    fn the_conversion_is_not_the_identity_at_bar_frequencies() {
        for bpy in [525_949.0_f64, 8_765.8, 252.0] {
            let (deannualize, _) = deflation_units(bpy, None);
            assert!(
                (deannualize(1.0) - 1.0).abs() > 1e-6,
                "bars_per_year {bpy} must actually rescale, or the fix is a no-op"
            );
        }
    }

    /// Absent dispersion stays absent: a grid of one had no search to correct
    /// for, and converting `None` must not manufacture a benchmark.
    #[test]
    fn an_absent_dispersion_is_not_invented() {
        let (_, dispersion) = deflation_units(525_949.0, None);
        assert!(dispersion.is_none());
    }
}

#[cfg(test)]
mod pooling_tests {
    use super::*;
    use crucible_engine::Summary;

    /// Declared capital: $100,000, so a $1,000 move is exactly 1.00 %.
    const CASH: NanoUsd = 100_000_000_000_000;
    /// Daily annualization, so the Sharpes below are readable numbers.
    const BPY: f64 = 252.0;
    /// The sweep level `kill_if_dead` names in these fixtures.
    const KILL: i64 = 2;

    /// An equity curve from per-bar **dollar** steps, opening at [`CASH`].
    ///
    /// The same shape `RunTrace::pooled_with_stats` hands to `Summary::compute`
    /// and `ReturnStats::of`, which is what makes the statistics below the ones
    /// a real contract would actually contribute.
    fn curve(steps: &[i64]) -> Vec<(Ts, NanoUsd)> {
        let mut level = CASH;
        let mut out = vec![(Ts(0), level)];
        for (i, step) in steps.iter().enumerate() {
            level += step * 1_000_000_000;
            out.push((Ts(i64::try_from(i).expect("small test index") + 1), level));
        }
        out
    }

    /// A series' `Summary` and its `ReturnStats`, from one curve — exactly as
    /// `pooled_of` produces them, so a test comparing one against the other is
    /// comparing the two things the funnel really holds.
    fn both(steps: &[i64]) -> (Summary, ReturnStats) {
        let c = curve(steps);
        (Summary::compute(&c, &[], 0, BPY), ReturnStats::of(&c))
    }

    /// The six series one contract contributes, deliberately all different.
    ///
    /// A pooling step that read the free-fill statistics where it meant the
    /// costed ones — the likeliest defect in a function of this shape — would
    /// produce a number that still looked entirely plausible. Distinct series
    /// are what make that mistake visible instead.
    struct Series<'a> {
        costed: &'a [i64],
        free_fill: &'a [i64],
        level_zero: &'a [i64],
        kill_level: &'a [i64],
        random_entry: &'a [i64],
        buy_and_hold: &'a [i64],
    }

    fn contract(series: &Series<'_>, oos_trades: usize) -> ContractEvidence {
        let (free_fill_oos, free_fill_oos_stats) = both(series.free_fill);
        let (_, costed_oos_stats) = both(series.costed);
        let control = |name: &'static str, steps: &[i64]| Control {
            name,
            oos_stitched: Some(both(steps)),
            absent_because: None,
            seed: None,
            draws: 1,
            draws_beaten: 0,
        };
        let level = |half_ticks: i64, steps: &[i64]| {
            let (oos_stitched, oos_stitched_stats) = both(steps);
            CostLevel {
                half_ticks,
                oos_stitched,
                oos_stitched_stats,
            }
        };
        ContractEvidence {
            free_fill_oos,
            free_fill_oos_stats,
            costed_oos_stats,
            oos_trades,
            sweep: vec![level(0, series.level_zero), level(KILL, series.kill_level)],
            controls: [
                control("matched random-entry", series.random_entry),
                control("buy-and-hold", series.buy_and_hold),
            ],
        }
    }

    /// Contract A. Every series moves, and none of them moves like another.
    fn series_a() -> Series<'static> {
        Series {
            costed: &[400, -150, 260, -90, 510, -220, 330, 120, -60, 900],
            free_fill: &[520, -110, 300, -40, 610, -170, 380, 160, -20, 970],
            level_zero: &[560, -100, 320, -30, 640, -160, 400, 170, -10, 990],
            kill_level: &[310, -190, 210, -140, 430, -280, 260, 80, -110, 810],
            random_entry: &[120, 340, -260, 80, -410, 190, 270, -130, 60, 210],
            buy_and_hold: &[-80, 250, 410, -320, 170, 90, -240, 380, -50, 140],
        }
    }

    /// Contract B: a different length and a different spread, so pooling and
    /// averaging cannot coincide — `ReturnStats::sharpe`'s own control makes
    /// the same demand of its fixture, for the same reason.
    fn series_b() -> Series<'static> {
        Series {
            costed: &[-1_100, 1_700, -300, 850, -1_250, 2_100, -400],
            free_fill: &[-900, 1_850, -220, 960, -1_040, 2_280, -310],
            level_zero: &[-860, 1_900, -200, 1_000, -1_000, 2_320, -290],
            kill_level: &[-1_300, 1_520, -390, 720, -1_430, 1_960, -520],
            random_entry: &[700, -1_400, 260, -880, 1_310, -520, 940],
            buy_and_hold: &[-460, 620, -1_150, 1_480, -270, 830, -1_090],
        }
    }

    /// The grid-level half, fixed. `deannualize` is the real D-0125 conversion
    /// rather than the identity, so the deflation is exercised in the units it
    /// is actually computed in.
    fn shared<'a>(deannualize: &'a dyn Fn(f64) -> f64, sessions: usize) -> PoolingInputs<'a> {
        PoolingInputs {
            distinct_oos_sessions: sessions,
            n_trials: 40,
            pbo: Some(0.35),
            trial_sharpe_dispersion: Some(0.02),
            deannualize,
            kill_if_dead_half_ticks: KILL,
            initial_cash_nano_usd: CASH,
            bars_per_year: BPY,
        }
    }

    fn deannualizer() -> impl Fn(f64) -> f64 {
        |annualized: f64| annualized / BPY.sqrt()
    }

    /// **The inertness proof.** A pool of one must reproduce, BIT-for-bit, the
    /// numbers that contract's own `Summary`s already report — because those
    /// are what every funnel gate on `main` pins today, and D-0117 holds every
    /// pool in this build to exactly one contract.
    ///
    /// Deliberately labelled inertness and **not** correctness. N=1 is the case
    /// where a fold is the identity and a summing bug cannot show, so with
    /// respect to everything pooling actually does this test is satisfied by
    /// construction. The tests below it are the ones that can fail.
    ///
    /// What would this do if the thing it looks for were absent? On a flat
    /// fixture every field would be zero or `None` and the comparison would
    /// hold for nothing — so the expected values are asserted live first.
    #[test]
    fn a_pool_of_one_reproduces_the_contracts_own_numbers() {
        let a = series_a();
        let (costed, _) = both(a.costed);
        let (kill, _) = both(a.kill_level);
        let (random_entry, _) = both(a.random_entry);
        let (buy_and_hold, _) = both(a.buy_and_hold);
        let one = contract(&a, 17);

        assert!(
            costed.total_return_pct != 0.0
                && costed.sharpe_naive.is_some()
                && kill.sharpe_naive.is_some(),
            "the fixture must produce live numbers, or this test is vacuous"
        );

        let d = deannualizer();
        let pooled = pool_contract_evidence(&one, &[], &shared(&d, 63));

        let bits = f64::to_bits;
        assert_eq!(
            bits(pooled.costed_return_pct),
            bits(costed.total_return_pct),
            "the pooled costed return must be the contract's own, to the bit"
        );
        assert_eq!(
            bits(pooled.free_fill_return_pct),
            bits(one.free_fill_oos.total_return_pct)
        );
        assert_eq!(
            pooled.costed_sharpe.map(bits),
            costed.sharpe_naive.map(bits),
            "and the pooled Sharpe likewise"
        );
        assert_eq!(
            pooled.sharpe_at_kill_level.map(bits),
            kill.sharpe_naive.map(bits)
        );
        assert_eq!(
            pooled.random_entry_return_pct.map(bits),
            Some(bits(random_entry.total_return_pct))
        );
        assert_eq!(
            pooled.buy_and_hold_return_pct.map(bits),
            Some(bits(buy_and_hold.total_return_pct))
        );
        assert_eq!(pooled.oos_trades, 17);
        assert_eq!(pooled.oos_sessions, 63);
    }

    /// **The asymmetry, demonstrated rather than asserted** (D-0114).
    ///
    /// Trades track the contracts: 17 and 11 pool to 28. Sessions do not track
    /// them at all — the same two contracts pooled under two different declared
    /// session counts report those two counts, which is what "the caller
    /// supplies the union" means operationally. A future `ContractEvidence`
    /// that grew a session field, and a pooling step that summed it, would fail
    /// the second half of this the day it was written.
    #[test]
    fn trades_are_summed_and_sessions_are_not_derived_from_the_contracts() {
        let (a, b) = (contract(&series_a(), 17), contract(&series_b(), 11));
        let d = deannualizer();

        let pooled = pool_contract_evidence(&a, std::slice::from_ref(&b), &shared(&d, 96));
        assert_eq!(
            pooled.oos_trades, 28,
            "17 + 11: round-trips are disjoint events"
        );
        assert_eq!(pooled.oos_sessions, 96);

        // Same contracts, different declared union. Sessions follow the
        // caller; trades do not move, because they genuinely are the
        // contracts'.
        let again = pool_contract_evidence(&a, std::slice::from_ref(&b), &shared(&d, 141));
        assert_eq!(again.oos_sessions, 141);
        assert_eq!(again.oos_trades, 28);
    }

    /// **The pooled total return, hand-derived** — and distinguishable from the
    /// plausible wrong answer.
    ///
    /// Contract A's costed steps sum to +$2,000 and B's to +$1,600, on the
    /// $100,000 of declared capital both start from (D-0127). The pooled
    /// curve's final equity is `100,000 + 2,000 + 1,600 = 103,600`, so the
    /// pooled return is exactly **+3.60 %**.
    ///
    /// COMPOUNDING the two contracts' own returns — `1.02 × 1.016 − 1` =
    /// 3.632 % — is the wrong answer a reasonable implementation reaches for,
    /// and it is 0.032 points away, so this test tells them apart. The
    /// contracts do not compound: they are two windows measured from the same
    /// capital, not one account run twice.
    #[test]
    fn the_pooled_return_is_the_delta_sum_over_declared_capital() {
        let (sa, sb) = (series_a(), series_b());
        assert_eq!(
            sa.costed.iter().sum::<i64>(),
            2_000,
            "A's steps sum to $2,000"
        );
        assert_eq!(
            sb.costed.iter().sum::<i64>(),
            1_600,
            "B's steps sum to $1,600"
        );

        let (a, b) = (contract(&sa, 17), contract(&sb, 11));
        let d = deannualizer();
        let pooled = pool_contract_evidence(&a, std::slice::from_ref(&b), &shared(&d, 96));

        assert!(
            (pooled.costed_return_pct - 3.60).abs() < 1e-9,
            "expected +3.60 %, got {}",
            pooled.costed_return_pct
        );
        let compounded = (1.02_f64 * 1.016 - 1.0) * 100.0;
        assert!(
            (pooled.costed_return_pct - compounded).abs() > 0.01,
            "the delta sum and the compounded product must be distinguishable \
             here, or this test cannot see the difference: {} vs {compounded}",
            pooled.costed_return_pct
        );
    }

    /// **Each pooled number reads its own series.** Three channels — the costed
    /// run, the S1 free-fill screen and the kill-level sweep entry — are pooled
    /// from three different sets of statistics, and the fixture makes all three
    /// differ, so a copy-paste feeding one of them the wrong field could not
    /// pass.
    ///
    /// The Sharpe is checked against `combine(..).sharpe(..)`, which IS the
    /// definition (D-0127) — and that is the point. This proves the *wiring*
    /// reaches the right statistics; `ReturnStats`' own tests prove the
    /// definition is the right arithmetic. Splitting it that way is what keeps
    /// neither test tautological.
    #[test]
    fn every_pooled_channel_reads_its_own_statistics() {
        let (sa, sb) = (series_a(), series_b());
        let (a, b) = (contract(&sa, 17), contract(&sb, 11));
        let d = deannualizer();
        let pooled = pool_contract_evidence(&a, std::slice::from_ref(&b), &shared(&d, 96));

        let combined = |x: &[i64], y: &[i64]| {
            ReturnStats::combine(ReturnStats::of(&curve(x)), ReturnStats::of(&curve(y)))
        };
        let costed = combined(sa.costed, sb.costed);
        let kill = combined(sa.kill_level, sb.kill_level);

        assert_eq!(
            pooled.costed_sharpe.map(f64::to_bits),
            costed.sharpe(BPY).map(f64::to_bits),
            "the costed Sharpe is the fold of the costed statistics"
        );
        assert_eq!(
            pooled.sharpe_at_kill_level.map(f64::to_bits),
            kill.sharpe(BPY).map(f64::to_bits),
            "and the kill level's is the fold of the kill level's"
        );

        // All three channels must be mutually distinguishable, or the
        // assertions above would hold even if the function read one series
        // three times over.
        let free_fill = combined(sa.free_fill, sb.free_fill).net_delta_nano_usd;
        assert!(
            costed.net_delta_nano_usd != kill.net_delta_nano_usd
                && costed.net_delta_nano_usd != free_fill,
            "the fixture's three channels must genuinely differ"
        );
        assert!(
            (pooled.free_fill_return_pct - pooled.costed_return_pct).abs() > 1e-9,
            "the S1 screen and the costed run must not report one number"
        );
    }

    /// **An absent control on ANY contract makes the pooled control absent**,
    /// with its converse in the same test — without which a function that
    /// always returned `None` would pass.
    ///
    /// And the two controls are independent: an absent random-entry must not
    /// take buy-and-hold down with it, or a scorecard would report two holes
    /// where the run produced one.
    #[test]
    fn an_absent_control_on_any_contract_makes_the_pooled_control_absent() {
        let (sa, sb) = (series_a(), series_b());
        let (a, mut b) = (contract(&sa, 17), contract(&sb, 11));
        let d = deannualizer();
        let facts = shared(&d, 96);

        // The converse first: both present, both pooled.
        let present = pool_contract_evidence(&a, std::slice::from_ref(&b), &facts);
        assert!(
            present.random_entry_return_pct.is_some() && present.buy_and_hold_return_pct.is_some(),
            "with every contract's control built, both must pool — otherwise the \
             absence below proves nothing"
        );

        b.controls[0].oos_stitched = None;
        b.controls[0].absent_because = Some("no round-trip to match".to_owned());
        let partial = pool_contract_evidence(&a, std::slice::from_ref(&b), &facts);
        assert!(
            partial.random_entry_return_pct.is_none(),
            "pooling the contracts that DID run would benchmark N contracts \
             against N-1 (D-0075)"
        );
        assert!(
            partial.buy_and_hold_return_pct.is_some(),
            "the other control still ran on every contract and must survive"
        );
    }

    /// **A sweep level missing from any contract makes its pooled Sharpe
    /// absent**, on the same terms and with the same converse.
    ///
    /// `None` here is not a pass: `assess` reads `sharpe_at_kill_level` as the
    /// kill criterion, and an absent value fails it rather than clearing it.
    #[test]
    fn a_kill_level_missing_from_any_contract_makes_the_pooled_sharpe_absent() {
        let (sa, sb) = (series_a(), series_b());
        let (a, mut b) = (contract(&sa, 17), contract(&sb, 11));
        let d = deannualizer();
        let facts = shared(&d, 96);

        let present = pool_contract_evidence(&a, std::slice::from_ref(&b), &facts);
        assert!(
            present.sharpe_at_kill_level.is_some(),
            "with the level on every contract it must pool, or the absence \
             below proves nothing"
        );

        b.sweep.retain(|l| l.half_ticks != KILL);
        let partial = pool_contract_evidence(&a, std::slice::from_ref(&b), &facts);
        assert!(
            partial.sharpe_at_kill_level.is_none(),
            "a level one contract never swept cannot be pooled across all of them"
        );
    }

    /// The grid-level fields pass through untouched, and the deflation reads
    /// the shape of the **pooled** series rather than of any one contract's.
    ///
    /// The shape check is the one that matters. Deflating a pooled Sharpe with
    /// a single contract's skew and kurtosis would correct one number using
    /// another number's sample, which §9 already calls a wrong result rather
    /// than an approximation.
    #[test]
    fn the_deflation_reads_the_pooled_shape_and_the_grid_fields_pass_through() {
        let (sa, sb) = (series_a(), series_b());
        let (a, b) = (contract(&sa, 17), contract(&sb, 11));
        let d = deannualizer();
        let pooled = pool_contract_evidence(&a, std::slice::from_ref(&b), &shared(&d, 96));

        assert_eq!(pooled.n_trials, 40);
        assert_eq!(pooled.pbo, Some(0.35));
        assert_eq!(pooled.permutation_p_value, None);

        let deflated = pooled.deflated.expect("the fixture deflates");
        let a_only = ReturnStats::of(&curve(sa.costed));
        let combined = ReturnStats::combine(a_only, ReturnStats::of(&curve(sb.costed)));

        // Ten steps and seven: 17 pooled returns, which is neither contract's.
        assert_eq!(combined.shape().n_returns, 17);
        assert_eq!(a_only.shape().n_returns, 10);

        // `Deflated` does not echo its observation count, so the shape it used
        // is established the only way it can be — by recomputing the estimator
        // both ways and seeing which one the funnel produced.
        let observed = (d)(pooled.costed_sharpe.expect("the fixture has a Sharpe"));
        let deflate_with = |stats: ReturnStats| {
            let shape = stats.shape();
            crate::stats::deflated::deflated_sharpe(crate::stats::deflated::DeflationInputs {
                observed_sharpe: observed,
                skew: shape.skew.expect("the fixture has moments"),
                kurtosis: shape.kurtosis.expect("the fixture has moments"),
                n_observations: shape.n_returns,
                n_trials: 40,
                trial_sharpe_dispersion: Some(0.02),
            })
            .expect("the fixture deflates")
        };
        let from_pool = deflate_with(combined);
        let from_a_alone = deflate_with(a_only);

        assert_eq!(
            deflated.standard_error.to_bits(),
            from_pool.standard_error.to_bits(),
            "the deflation must read the POOLED shape"
        );
        assert_eq!(deflated.dsr.to_bits(), from_pool.dsr.to_bits());
        // The converse: had it read one contract's shape, this comparison would
        // have caught it — so the agreement above is evidence rather than a
        // coincidence of a fixture where the two happen to match.
        assert!(
            (from_pool.standard_error - from_a_alone.standard_error).abs() > 1e-9,
            "pooled and single-contract shapes must give different standard \
             errors here: {} vs {}",
            from_pool.standard_error,
            from_a_alone.standard_error
        );
    }
}
