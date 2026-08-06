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

/// One level of the mandatory cost-sensitivity sweep.
#[derive(Clone, Debug)]
pub struct CostLevel {
    /// Half-spread, in half-ticks (`0 / 1 / 2 / 4`).
    pub half_ticks: i64,
    /// Pooled out-of-sample statistics at that level.
    pub oos_stitched: Summary,
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
    /// Pooled out-of-sample statistics, or `None` if it could not be built.
    ///
    /// For the random-entry control this is the **median draw** by pooled
    /// out-of-sample return, not a single draw — see [`RANDOM_ENTRY_DRAWS`].
    pub oos_stitched: Option<Summary>,
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
        self.oos_stitched.as_ref().map(|s| s.total_return_pct)
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

/// One contract's contribution to a combo's [`Evidence`], plus the artifacts
/// the report needs beside it.
///
/// Four fields because [`ComboOutcome`] needs `free_fill_oos`, `sweep`,
/// `controls` and `deflated` to render, while [`assess`] needs `evidence` — so
/// the per-contract half cannot hand back evidence alone.
struct ContractEvidence {
    free_fill_oos: crucible_engine::Summary,
    sweep: Vec<CostLevel>,
    controls: [Control; 2],
    evidence: Evidence,
}

/// Produces one contract's evidence **without assessing it**.
///
/// This is the assess seam. What was missing was never that [`assess`] is a
/// separate function — it already was — but that there was no way to obtain a
/// contract's evidence on its own: the S1 screen, the cost sweep, both
/// mandatory controls and the `Evidence` construction were inline in
/// [`run_funnel`], which then assessed immediately. Block C needs N of these
/// pooled into one `Evidence` and assessed **once** (C6a-iii-b); naming the
/// boundary while it is still singular is what makes that a generalization
/// rather than a rewrite.
///
/// A literal lift: the body below is the moved lines, unaltered. `costed`
/// arrives by reference and `&costed` still coerces; `pbo` arrives as the
/// whole `Result` so `pbo.as_ref().ok()` reads exactly as it did. Those
/// choices exist so that nothing in the body had to be edited while being
/// moved — a re-associated reduction here is D-0122's defect class in the
/// function producing every Sharpe the funnel reports.
#[expect(
    clippy::too_many_arguments,
    reason = "a literal lift takes what the inline block read; bundling them               would edit the call shape in the commit whose only proof is that               nothing changed. C6a-iii-b revisits the signature when it pools."
)]
fn contract_evidence(
    inputs: &FunnelInputs<'_>,
    index: usize,
    costed: &ComboWalkForward,
    test_windows: &[std::ops::Range<usize>],
    oos_sessions: usize,
    n_trials: usize,
    trial_sharpe_dispersion: Option<f64>,
    deannualize: &dyn Fn(f64) -> f64,
    pbo: &Result<crate::stats::pbo::Pbo, crate::stats::pbo::PboUnavailable>,
) -> ContractEvidence {
    // Step 3: the S1 screen, cost-free. The only sanctioned FreeFills use.
    let free_fill_oos = pooled_of(&replay(inputs, index, &mut FreeFills), inputs, test_windows).0;

    // Step 4: the mandatory sweep.
    let sweep: Vec<CostLevel> = inputs
        .criteria
        .cost_sweep_half_ticks
        .iter()
        .map(|&half_ticks| CostLevel {
            half_ticks,
            oos_stitched: pooled_of(
                &replay(
                    inputs,
                    index,
                    &mut inputs.costs.at_half_ticks(half_ticks, inputs.spec.tick),
                ),
                inputs,
                test_windows,
            )
            .0,
        })
        .collect();

    // Step 5: the controls, under the declared costs.
    let controls = [
        random_entry_control(inputs, costed, test_windows),
        buy_and_hold_control(inputs, test_windows, costed.oos_stitched.total_return_pct),
    ];

    let evidence = Evidence {
        oos_trades: costed.oos_stitched.round_trips,
        oos_sessions,
        free_fill_return_pct: free_fill_oos.total_return_pct,
        costed_return_pct: costed.oos_stitched.total_return_pct,
        costed_sharpe: costed.oos_stitched.sharpe_naive,
        sharpe_at_kill_level: sweep
            .iter()
            .find(|l| l.half_ticks == inputs.criteria.kill_if_dead_half_ticks)
            .and_then(|l| l.oos_stitched.sharpe_naive),
        random_entry_return_pct: controls[0].return_pct(),
        buy_and_hold_return_pct: controls[1].return_pct(),
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
        deflated: costed.oos_stitched.sharpe_naive.and_then(|observed| {
            let shape = costed.oos_stitched.return_shape;
            crate::stats::deflated::deflated_sharpe(crate::stats::deflated::DeflationInputs {
                observed_sharpe: deannualize(observed),
                skew: shape.skew?,
                kurtosis: shape.kurtosis?,
                n_observations: shape.n_returns,
                n_trials,
                trial_sharpe_dispersion,
            })
        }),
        n_trials,
        pbo: pbo.as_ref().ok().map(|p| p.value),
    };
    ContractEvidence {
        free_fill_oos,
        sweep,
        controls,
        evidence,
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

    let mut combos = Vec::with_capacity(report.combos.len());
    for costed in report.combos {
        let index = costed.id.combo_index;

        let ContractEvidence {
            free_fill_oos,
            sweep,
            controls,
            evidence,
        } = contract_evidence(
            inputs,
            index,
            &costed,
            &test_windows,
            oos_sessions,
            n_trials,
            trial_sharpe_dispersion,
            &deannualize,
            &pbo,
        );
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
fn replay<M: FillModel>(inputs: &FunnelInputs<'_>, index: usize, fills: &mut M) -> BacktestResult {
    let mut strategy = inputs.grid.aligned_strategy(index);
    let mut feed = SliceFeed {
        events: inputs.events,
        at: 0,
    };
    run(&mut feed, &mut strategy, fills, inputs.spec, inputs.params).expect(
        "INVARIANT: the shared bar series is availability-ordered; the declared-cost replay \
             over the identical slice already succeeded",
    )
}

/// One replay's test windows stitched, and the sufficient statistics of the
/// series that stitch produced.
///
/// Widened rather than given a sibling, which is the opposite call from
/// [`RunTrace::pooled_with_stats`] one layer down, for the reason that made
/// that one a sibling: blast radius is the symptom, caller need is the cause.
/// `pooled` had nine callers when that call was made and has seven now — this
/// commit and the one before it each took one — and the majority of them want a
/// `Summary` alone, so widening it would have charged every uninterested call
/// site for the one interested caller. `pooled_of` has four: the S1 free-fill
/// screen, each cost-sweep level, each random-entry draw and buy-and-hold.
/// Every one of them is a series `ContractEvidence` must be able to pool across
/// contracts, so *all four* want the statistics, and a sibling here would leave
/// `pooled_of` with no callers at all.
///
/// The four take `.0` for one commit. Storing what they now receive is the next
/// one, kept separate so that a moved gate has a single candidate cause.
fn pooled_of(
    result: &BacktestResult,
    inputs: &FunnelInputs<'_>,
    test_windows: &[Range<usize>],
) -> (Summary, ReturnStats) {
    RunTrace::new(&result.equity, &result.closed_trades, &result.fee_events).pooled_with_stats(
        test_windows,
        inputs.params.initial_cash_nano_usd,
        inputs.params.bars_per_year,
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
    let mut draws: Vec<(u64, Summary)> = Vec::with_capacity(RANDOM_ENTRY_DRAWS);
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
                    events: inputs.events,
                    at: 0,
                };
                let mut fills = inputs.costs.declared(inputs.spec.tick);
                let result = run(
                    &mut feed,
                    &mut control,
                    &mut fills,
                    inputs.spec,
                    inputs.params,
                )
                .expect("INVARIANT: the shared bar series is availability-ordered");
                draws.push((seed, pooled_of(&result, inputs, test_windows).0));
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
        .filter(|(_, s)| strategy_pct > s.total_return_pct)
        .count();
    // Lower median of an even sample: the pessimistic half, so a strategy that
    // ties the middle of the null does not clear the bar on a rounding choice.
    let (seed, median) = draws.swap_remove((RANDOM_ENTRY_DRAWS - 1) / 2);
    Control {
        name,
        oos_stitched: Some(median),
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
    test_windows: &[Range<usize>],
    strategy_pct: f64,
) -> Control {
    let mut control = BuyAndHold::new(inputs.grid.max_warmup_bars(), inputs.qty);
    let mut feed = SliceFeed {
        events: inputs.events,
        at: 0,
    };
    let mut fills = inputs.costs.declared(inputs.spec.tick);
    let result = run(
        &mut feed,
        &mut control,
        &mut fills,
        inputs.spec,
        inputs.params,
    )
    .expect("INVARIANT: the shared bar series is availability-ordered");
    let oos = pooled_of(&result, inputs, test_windows).0;
    Control {
        name: "buy-and-hold",
        // One "draw", because owning the thing is not a random variable. The
        // field is still filled in so `draws_beaten / draws` reads the same
        // way for both controls rather than being a special case a reader has
        // to remember.
        draws_beaten: usize::from(strategy_pct > oos.total_return_pct),
        oos_stitched: Some(oos),
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
