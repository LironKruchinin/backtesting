//! Replaying a grid and cutting the result into folds.
//!
//! ## One replay per combo, not one per fold (D-0063)
//!
//! A fold here is a **metric window**, not a separate backtest. Each combo is
//! replayed once over the shared bar series and the resulting equity curve is
//! sliced. Two reasons, and one honest consequence.
//!
//! Nothing in this runner *fits* anything. Walk-forward exists here to
//! produce an out-of-sample sample, not to re-estimate parameters — parameter
//! selection on in-sample folds is the funnel's job (M3), and it selects
//! across combos, which this runner has already evaluated separately. So a
//! per-fold re-replay would compute, bar for bar, the same fills.
//!
//! And a continuous replay is what a deployed strategy actually does. Resetting
//! indicator state at each fold boundary would mean every test window opens
//! with a strategy that has forgotten the market, which is not a property of
//! the strategy — it is a property of the fold layout, and it would move the
//! numbers.
//!
//! The consequence, stated because it shows up in a printed number: a
//! round-trip opened in a training window and closed in a test window is
//! counted as a test-window trade. The mark-to-market equity series still
//! splits the *money* at the boundary correctly — each window keeps the marks
//! that happened inside it — but the trade *count* and win rate attribute the
//! whole round-trip to where it was realized. Path-dependent exits make this
//! more visible: a bracket ([`crucible_engine::bracket`]) can close a
//! training-window entry on the first bar of a test window, so the report
//! carries the protective-exit and path-sensitivity counts per combo rather
//! than leaving a reader to assume neither happened.

use crucible_core::prelude::*;
use crucible_engine::{
    AccountCapture, AccountSeries, BacktestParams, BacktestResult, ClosedTrade, DayRecord,
    EngineError, ReturnStats, Summary, WorstDayDistribution, run_capturing,
};
use crucible_strategies::combo::{ComboId, ConfigHash, Grid};

use super::folds::{Fold, FoldPlan};
use super::seed::derive_run_seed;
use super::window::RunTrace;

/// What a stored result must be able to name (CLAUDE.md §2.5), minus the
/// parts only the caller knows (git sha, data manifest ids).
#[derive(Clone, Debug)]
pub struct RunIdentity {
    /// Effective registration hash: D-0012 normally, D-0106 when S0 is declared.
    pub config_hash: ConfigHash,
    /// The config's declared `[run].seed`, root of every derived seed.
    pub root_seed: u64,
    /// The account being evaluated, if any (D-0067). Part of the run identity
    /// and of the seed lineage, because choosing an account after seeing
    /// results is a maximum over sixteen draws reported as an expectation.
    pub account_id: Option<String>,
}

/// One combo's numbers on one fold.
#[derive(Clone, Debug)]
pub struct FoldResult {
    /// Index into [`FoldPlan::folds`].
    pub fold_index: usize,
    /// `derive_seed(config_hash, root_seed, combo_index, fold_index)`.
    ///
    /// Computed and recorded even though nothing in this build consumes
    /// randomness — see [`super::seed`] for why it exists before its first
    /// consumer.
    pub seed: u64,
    /// In-sample window statistics.
    pub is: Summary,
    /// Out-of-sample window statistics.
    pub oos: Summary,
}

/// One combo, walked forward.
#[derive(Clone, Debug)]
pub struct ComboWalkForward {
    /// `(config hash, combo index)` — the identity a registry stores.
    pub id: ComboId,
    /// `fast(period=10) slow(period=50)`, for report tables.
    pub label: String,
    /// Bars this combo alone would have needed. Kept for the §2.6 contrast:
    /// it is not what it was run with.
    pub own_warmup_bars: usize,
    /// Orders dropped because this combo was warm before the grid was.
    pub suppressed_intents: usize,
    /// Bars where `enter_long` and `enter_short` were both true while flat.
    pub conflicting_signals: usize,
    /// Per-fold in-sample and out-of-sample statistics.
    pub folds: Vec<FoldResult>,
    /// The headline: every test window pooled, and nothing else.
    pub oos_stitched: Summary,
    /// The **sufficient statistics** of the very series [`Self::oos_stitched`]
    /// summarises — carried so that a statistic pooled across *contracts* can
    /// be formed without any contract's curve being retained.
    ///
    /// It cannot be recovered from `oos_stitched` later. A [`Summary`] keeps
    /// the *shape* of its return series (`n`, skew, kurtosis) but not the
    /// undivided sums a second series can be combined into, and rebuilding
    /// them would mean keeping the curve — which at ~880 KB per combo per
    /// contract is the cost [`ReturnStats`] exists to refuse (D-0071's trade,
    /// pinned by `a_return_stats_is_forty_eight_bytes`).
    ///
    /// Inert today: nothing reads it, and it reaches no hash. Landing the
    /// carrier one commit ahead of its consumer is the inert-first ordering
    /// the rest of Block C used, and it is why adding this field must move no
    /// determinism gate.
    pub oos_stitched_stats: ReturnStats,
    /// Every training window pooled. A diagnostic, never a headline — the
    /// combos were chosen by a human looking at something, and this is the
    /// sample they looked at.
    pub is_pooled: Summary,
    /// The whole replay, warmup prefix and all — the number `crucible combo`
    /// prints. Reported alongside so the difference D-0061 described is
    /// visible rather than asserted.
    pub whole_run: Summary,
    /// Orders still pending when the series ended.
    pub cancelled_at_eof: usize,
    /// Exits produced by a protective stop or target rather than by a
    /// strategy order, over the whole replay.
    pub n_protective_exits: usize,
    /// Bars where a bracket's stop *and* target were both touched, so the exit
    /// came from `stop_first_intrabar` rather than from the data
    /// (`crucible-engine::bracket`).
    ///
    /// Whole-replay, not per fold, and deliberately so: a path-sensitive bar is
    /// a property of the *execution assumption*, not of a metric window, and
    /// the number a reader needs is "how much of this grid's PnL is a
    /// convention?". Attributing them per window becomes worth its plumbing the
    /// day a config can declare a bracket — today none can, so the honest
    /// number is one per combo.
    pub path_sensitive_bars: usize,
    /// The account-evaluation series, captured inside the mark loop rather
    /// than rebuilt afterwards (`docs/ACCOUNT_EVAL_SPEC.md` §3, D-0071).
    ///
    /// Whole-replay, because that is what the engine captures and what a
    /// day-block bootstrap will resample. The out-of-sample slice of it is
    /// [`ComboWalkForward::oos_worst_days`], cut here rather than by a
    /// consumer, because which days are out of sample is a *fold* question and
    /// the fold plan lives on this side of the seam.
    pub account: AccountSeries,
    /// The worst-day pair over the out-of-sample days only — every day whose
    /// bars fall inside a test window.
    ///
    /// The gap between the worst *close* and the worst *trough* is exactly the
    /// amount of a bad day that a daily-close model never sees, which is the
    /// endpoint fallacy D-0067 exists to quantify rather than argue about.
    pub oos_worst_days: WorstDayDistribution,
    /// Every round-trip as `(open bar, close bar, direction)`.
    ///
    /// Reduced here, while the equity curve is still alive, rather than
    /// retained: the matched random-entry control needs the *shape* of the
    /// trades and nothing else, and keeping a per-combo copy of a per-bar
    /// series to recover it later is the 4.98 GiB mistake D-0071 refused for
    /// the high-water reducer. This is O(round-trips), which is what a trade
    /// list costs anyway.
    pub round_trip_bars: Vec<(usize, usize, Side)>,
}

/// A grid, walked forward on one bar series.
#[derive(Clone, Debug)]
pub struct WalkForwardReport {
    /// One entry per combo, in grid-index order — never completion order
    /// (CLAUDE.md §2.2).
    pub combos: Vec<ComboWalkForward>,
}

/// Anything that stops a grid from being walked forward.
#[derive(Debug, PartialEq, Eq)]
pub enum WalkForwardError {
    /// The plan was laid out against a different series than the one supplied.
    PlanSeriesMismatch {
        /// Bars the plan was built from.
        plan_bars: usize,
        /// Bars supplied.
        series_bars: usize,
    },
    /// The plan is laid out behind a different warmup than the grid's.
    PlanWarmupMismatch {
        /// Warmup the plan was built with.
        plan_warmup: usize,
        /// The grid's max warmup.
        grid_warmup: usize,
    },
    /// The day keys do not describe the series being replayed.
    ///
    /// Separate from [`WalkForwardError::PlanSeriesMismatch`] because it means
    /// something more specific: the caller passed *different* keys to the fold
    /// plan and to the run. D-0071's whole device is that both consumers read
    /// one slice, and two slices is how a daily-loss breach lands on a
    /// different date in two reports.
    DayKeysMismatch {
        /// Keys supplied to this call.
        day_keys: usize,
        /// Bars supplied.
        series_bars: usize,
    },
    /// The replay itself failed.
    Engine(EngineError),
}

impl std::fmt::Display for WalkForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalkForwardError::PlanSeriesMismatch {
                plan_bars,
                series_bars,
            } => write!(
                f,
                "the fold plan was laid out over {plan_bars} bars but the series has \
                 {series_bars}; folds cut from the wrong series land on the wrong bars, \
                 silently"
            ),
            WalkForwardError::PlanWarmupMismatch {
                plan_warmup,
                grid_warmup,
            } => write!(
                f,
                "the fold plan starts behind a {plan_warmup}-bar warmup but the grid's is \
                 {grid_warmup}; folds must be laid out behind the warmup every combo shares, \
                 or a combo would be scored on bars its neighbours were not (CLAUDE.md §2.6)"
            ),
            WalkForwardError::DayKeysMismatch {
                day_keys,
                series_bars,
            } => write!(
                f,
                "{day_keys} trading-day key(s) were supplied for a {series_bars}-bar series. The \
                 keys must be the SAME slice the fold plan was built from (D-0071): two \
                 independent attributions of \"which day\" is how one breach lands on two dates"
            ),
            WalkForwardError::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WalkForwardError {}

/// Everything a grid replay needs, borrowed once instead of threaded through
/// an argument list nobody can read.
///
/// Public so [`crate::scheduler`] can hand the same borrowed state to rayon
/// without a second copy of the wiring — two constructions of "one combo's
/// run" that could drift from each other is one run.
#[derive(Clone, Copy, Debug)]
pub struct GridRun<'a, M> {
    events: &'a [MarketEvent],
    day_keys: &'a [i64],
    grid: &'a Grid,
    plan: &'a FoldPlan,
    spec: &'a ContractSpec,
    params: &'a BacktestParams,
    identity: &'a RunIdentity,
    fill_model: &'a M,
}

impl<'a, M: FillModel + Clone> GridRun<'a, M> {
    /// Validates that the plan, the keys and the grid all describe `events`.
    ///
    /// Every check here is a wiring bug that would otherwise be silent: folds
    /// cut from the wrong series land on the wrong bars, and day keys that are
    /// not the plan's own put a breach on the wrong date (D-0071).
    ///
    /// # Errors
    /// [`WalkForwardError`] naming which of the three disagrees.
    #[expect(
        clippy::too_many_arguments,
        reason = "this is the argument list the checks exist to reconcile; bundling it would \
                  hide the very mismatch the constructor is for"
    )]
    pub fn new(
        events: &'a [MarketEvent],
        day_keys: &'a [i64],
        grid: &'a Grid,
        plan: &'a FoldPlan,
        spec: &'a ContractSpec,
        params: &'a BacktestParams,
        identity: &'a RunIdentity,
        fill_model: &'a M,
    ) -> Result<GridRun<'a, M>, WalkForwardError> {
        if plan.n_bars() != events.len() {
            return Err(WalkForwardError::PlanSeriesMismatch {
                plan_bars: plan.n_bars(),
                series_bars: events.len(),
            });
        }
        if plan.warmup_bars() != grid.max_warmup_bars() {
            return Err(WalkForwardError::PlanWarmupMismatch {
                plan_warmup: plan.warmup_bars(),
                grid_warmup: grid.max_warmup_bars(),
            });
        }
        if day_keys.len() != events.len() {
            return Err(WalkForwardError::DayKeysMismatch {
                day_keys: day_keys.len(),
                series_bars: events.len(),
            });
        }
        Ok(GridRun {
            events,
            day_keys,
            grid,
            plan,
            spec,
            params,
            identity,
            fill_model,
        })
    }

    /// Combos in the grid.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.grid.len()
    }

    /// Whether the grid is empty. Present because clippy asks for it beside
    /// [`GridRun::len`]; a grid with no combos never reaches here.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.grid.is_empty()
    }

    /// Replays one combo and cuts it into every fold.
    ///
    /// # Errors
    /// [`WalkForwardError::Engine`] if the feed violates the engine's ordering
    /// contract, or if the day keys do not describe the series.
    pub fn one_combo(&self, index: usize) -> Result<ComboWalkForward, WalkForwardError> {
        let combo = self.grid.combo(index);
        let mut strategy = self.grid.aligned_strategy(index);
        let mut feed = SliceFeed {
            events: self.events,
            at: 0,
        };
        let mut fills = self.fill_model.clone();
        // The mark grain comes off the bars themselves rather than from a
        // parameter: it is stamped on every result derived from these series
        // (spec §3.3.2, bar-close marks are a lower bound on P(breach)), and a
        // parameter is a second place for it to be wrong.
        let MarketEvent::Bar(first) = self
            .events
            .first()
            .expect("INVARIANT: an empty series is refused before a plan can be built over it");
        let mut capture =
            AccountCapture::new(self.day_keys, self.params.initial_cash_nano_usd, first.tf);
        let result: BacktestResult = run_capturing(
            &mut feed,
            &mut strategy,
            &mut fills,
            self.spec,
            self.params,
            &mut capture,
        )
        .map_err(WalkForwardError::Engine)?;
        let account = capture.finish();

        let trace = RunTrace::new(&result.equity, &result.closed_trades, &result.fee_events);
        let cash = self.params.initial_cash_nano_usd;
        let per_year = self.params.bars_per_year;
        let plan = self.plan;
        let identity = self.identity;

        let folds: Vec<FoldResult> = plan
            .folds()
            .iter()
            .map(|fold: &Fold| FoldResult {
                fold_index: fold.index,
                seed: derive_run_seed(
                    &identity.config_hash,
                    identity.root_seed,
                    identity.account_id.as_deref(),
                    index,
                    fold.index,
                ),
                is: trace.window(fold.train.bars.clone(), cash, per_year),
                oos: trace.window(fold.test.bars.clone(), cash, per_year),
            })
            .collect();

        let test_windows: Vec<_> = plan.folds().iter().map(|f| f.test.bars.clone()).collect();
        // `pooled` is `pooled_with_stats(..).0`, so taking both changes no
        // arithmetic — the Summary is produced by the same call it always was.
        let (oos_stitched, oos_stitched_stats) =
            trace.pooled_with_stats(&test_windows, cash, per_year);
        // Training windows overlap between folds under both schemes, so they
        // are pooled as the union rather than concatenated — a bar that
        // appears in three folds' training samples is still one bar.
        let is_pooled = trace.pooled(
            &union_of(plan.folds().iter().map(|f| f.train.bars.clone())),
            cash,
            per_year,
        );

        let oos_worst_days =
            WorstDayDistribution::from_days(&out_of_sample_days(&account.days, &test_windows));
        let round_trip_bars = round_trip_bars(&result.equity, &result.closed_trades);

        Ok(ComboWalkForward {
            id: ComboId {
                config: identity.config_hash,
                combo_index: index,
            },
            label: combo.label(),
            own_warmup_bars: combo.own_warmup_bars(),
            suppressed_intents: strategy.suppressed_intents(),
            conflicting_signals: strategy.inner().conflicting_signals(),
            folds,
            oos_stitched,
            oos_stitched_stats,
            is_pooled,
            whole_run: result.summary.clone(),
            cancelled_at_eof: result.cancelled_at_eof,
            n_protective_exits: result.n_protective_exits,
            path_sensitive_bars: result.path_sensitive_bars,
            account,
            oos_worst_days,
            round_trip_bars,
        })
    }
}

/// Round-trips as `(open bar, close bar, direction)`.
///
/// A [`crucible_engine::ClosedTrade`] is stamped with the `avail_ts` of the
/// fills that opened and closed it, and every fill lands on an event that also
/// produced an equity point, so the bar index is a lookup rather than a search
/// — the same join [`RunTrace`] does, for the same reason. A trade whose stamp
/// is past the end of the curve cannot happen and is dropped rather than
/// panicking (again as `RunTrace` does): losing one entry from a control's
/// match list is a smaller wrong than aborting a research run over an
/// impossible branch.
fn round_trip_bars(equity: &[(Ts, NanoUsd)], trades: &[ClosedTrade]) -> Vec<(usize, usize, Side)> {
    let index_of = |ts: Ts| equity.partition_point(|&(t, _)| t < ts);
    trades
        .iter()
        .filter_map(|t| {
            let (open, close) = (index_of(t.opened_ts), index_of(t.closed_ts));
            (close < equity.len()).then_some((open, close, t.direction))
        })
        .collect()
}

/// The captured days that fall inside a test window.
///
/// Membership is decided by the day's **first** bar, and that is exact rather
/// than approximate: fold boundaries are trading days (D-0062) and the day
/// keys the folds were cut on are the same slice the capture was fed
/// (D-0071), so a captured day lies wholly inside one window or wholly outside
/// every one. If those two ever stopped being the same slice this would become
/// an approximation — which is why `GridRun::new` refuses a key slice that is
/// not the plan's length.
fn out_of_sample_days(
    days: &[DayRecord],
    test_windows: &[std::ops::Range<usize>],
) -> Vec<DayRecord> {
    days.iter()
        .filter(|day| test_windows.iter().any(|w| w.contains(&day.bars.start)))
        .cloned()
        .collect()
}

/// Replays every combo in `grid` on `events` and cuts each result into
/// `plan`'s folds, one combo at a time.
///
/// `day_keys` is one trading-day key per bar — **the same slice `plan` was
/// built from** (D-0071). It reaches the engine's account-evaluation capture
/// unchanged, so fold attribution and day slicing cannot disagree about which
/// session a loss belongs to.
///
/// `fill_model` is cloned per combo, so a stateful fill model (M4's queue
/// simulator) starts each run from the same state rather than inheriting the
/// previous combo's book.
///
/// [`crate::scheduler::run_grid_parallel`] computes the identical report
/// across rayon's pool, and `the_parallel_scheduler_agrees_with_the_serial_one`
/// asserts they agree bit for bit.
///
/// # Errors
/// [`WalkForwardError`] if the plan, the keys or the grid do not describe the
/// series, or if the feed violates the engine's ordering contract.
#[expect(
    clippy::too_many_arguments,
    reason = "the seven inputs are what a run IS (§2.5); a bundle struct would only move the \
              list, and `GridRun::new` is that struct"
)]
pub fn run_grid<M: FillModel + Clone>(
    events: &[MarketEvent],
    day_keys: &[i64],
    grid: &Grid,
    plan: &FoldPlan,
    spec: &ContractSpec,
    params: &BacktestParams,
    identity: &RunIdentity,
    fill_model: &M,
) -> Result<WalkForwardReport, WalkForwardError> {
    let run = GridRun::new(
        events, day_keys, grid, plan, spec, params, identity, fill_model,
    )?;
    let mut combos = Vec::with_capacity(run.len());
    for index in 0..run.len() {
        combos.push(run.one_combo(index)?);
    }
    Ok(WalkForwardReport { combos })
}

/// Merges sorted, possibly overlapping bar ranges into disjoint ones.
fn union_of(ranges: impl Iterator<Item = std::ops::Range<usize>>) -> Vec<std::ops::Range<usize>> {
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for r in ranges {
        if r.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Replays a pre-collected bar series, exactly as `crucible combo` does.
///
/// Collecting once and replaying the same slice per combo is what makes "the
/// same bars for every combo" (§2.6) literal: there is one series in memory,
/// and folds cut from it land on the same bars for every combo by
/// construction.
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
