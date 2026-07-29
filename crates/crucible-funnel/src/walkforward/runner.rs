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
//! whole round-trip to where it was realized. Path-dependent exits (M2's
//! stops/targets work) will make this more visible, and the report says so.

use crucible_core::prelude::*;
use crucible_engine::{BacktestParams, BacktestResult, EngineError, Summary, run};
use crucible_strategies::combo::{ComboId, ConfigHash, Grid};

use super::folds::{Fold, FoldPlan};
use super::seed::derive_seed;
use super::window::RunTrace;

/// What a stored result must be able to name (CLAUDE.md §2.5), minus the
/// parts only the caller knows (git sha, data manifest ids).
#[derive(Clone, Copy, Debug)]
pub struct RunIdentity {
    /// blake3 over the config's canonical form (D-0012).
    pub config_hash: ConfigHash,
    /// The config's declared `[run].seed`, root of every derived seed.
    pub root_seed: u64,
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
    pub oos_pooled: Summary,
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
            WalkForwardError::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WalkForwardError {}

/// Replays every combo in `grid` on `events` and cuts each result into
/// `plan`'s folds.
///
/// `fill_model` is cloned per combo, so a stateful fill model (M4's queue
/// simulator) starts each run from the same state rather than inheriting the
/// previous combo's book.
///
/// # Errors
/// [`WalkForwardError`] if the plan does not match the series or the grid, or
/// if the feed violates the engine's ordering contract.
pub fn run_grid<M: FillModel + Clone>(
    events: &[MarketEvent],
    grid: &Grid,
    plan: &FoldPlan,
    spec: &ContractSpec,
    params: &BacktestParams,
    identity: &RunIdentity,
    fill_model: &M,
) -> Result<WalkForwardReport, WalkForwardError> {
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

    let run = GridRun {
        events,
        grid,
        plan,
        spec,
        params,
        identity,
        fill_model,
    };
    let mut combos = Vec::with_capacity(grid.len());
    for index in 0..grid.len() {
        combos.push(run.one_combo(index)?);
    }
    Ok(WalkForwardReport { combos })
}

/// Everything one combo's replay needs, borrowed once instead of threaded
/// through an argument list nobody can read.
struct GridRun<'a, M> {
    events: &'a [MarketEvent],
    grid: &'a Grid,
    plan: &'a FoldPlan,
    spec: &'a ContractSpec,
    params: &'a BacktestParams,
    identity: &'a RunIdentity,
    fill_model: &'a M,
}

impl<M: FillModel + Clone> GridRun<'_, M> {
    fn one_combo(&self, index: usize) -> Result<ComboWalkForward, WalkForwardError> {
        let combo = self.grid.combo(index);
        let mut strategy = self.grid.aligned_strategy(index);
        let mut feed = SliceFeed {
            events: self.events,
            at: 0,
        };
        let mut fills = self.fill_model.clone();
        let result: BacktestResult =
            run(&mut feed, &mut strategy, &mut fills, self.spec, self.params)
                .map_err(WalkForwardError::Engine)?;

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
                seed: derive_seed(&identity.config_hash, identity.root_seed, index, fold.index),
                is: trace.window(fold.train.bars.clone(), cash, per_year),
                oos: trace.window(fold.test.bars.clone(), cash, per_year),
            })
            .collect();

        let test_windows: Vec<_> = plan.folds().iter().map(|f| f.test.bars.clone()).collect();
        let oos_pooled = trace.pooled(&test_windows, cash, per_year);
        // Training windows overlap between folds under both schemes, so they
        // are pooled as the union rather than concatenated — a bar that
        // appears in three folds' training samples is still one bar.
        let is_pooled = trace.pooled(
            &union_of(plan.folds().iter().map(|f| f.train.bars.clone())),
            cash,
            per_year,
        );

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
            oos_pooled,
            is_pooled,
            whole_run: result.summary.clone(),
            cancelled_at_eof: result.cancelled_at_eof,
        })
    }
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
