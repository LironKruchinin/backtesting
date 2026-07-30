//! The event loop. Read this file before touching anything else in the
//! engine — the ordering rules here ARE the no-lookahead guarantee.
//!
//! Per-event processing order (never reorder these steps):
//! 1. **Fills.**
//!    1. Pending orders whose `placed_ts` is *strictly before* this event's
//!       `avail_ts` are offered to the fill model. An order can never fill
//!       against the event that triggered it.
//!    2. Then, if the position carries a live protective bracket, this bar is
//!       tested against its levels.
//! 2. **Mark.** The portfolio marks to this bar's close; one equity point is
//!    recorded per bar.
//! 3. **Decide.** The strategy sees the event and may emit intents, which
//!    become orders stamped `placed_ts = this event's avail_ts`.
//!
//! Step 1.1 before 1.2 is not arbitrary. A market order fills at the bar's
//! **opening** print, which is the first trade of the bar by definition, and a
//! resting level can only be touched at or after it. Testing the bracket first
//! would let a stop trigger on a bar whose open had already reversed the
//! position out from under it.
//!
//! A bracket installed by a fill on *this* bar is eligible on *this* bar: it
//! inherits its parent order's `placed_ts`, which is strictly before this
//! event, and the resting order genuinely existed from the moment the parent
//! filled. Deferring it to the next bar would leave every entry unprotected
//! for exactly one bar — and a stop that cannot protect the bar you entered on
//! is a stop that misses the move that matters. Which leg wins an ambiguous
//! bar is decided by the named convention in [`crate::bracket`], counted in
//! [`BacktestResult::path_sensitive_bars`], and printed.
//!
//! The loop also enforces the Feed contract (nondecreasing `avail_ts`) and
//! aborts the run on violation — a misordered feed corrupts every number
//! downstream, so it is an error, not a warning.

use std::collections::VecDeque;

use crucible_core::prelude::*;

use crate::bracket::{ActiveBracket, Outcome, StopFirstIntrabar};
use crate::metrics::Summary;
use crate::portfolio::{ClosedTrade, FeeEvent, Portfolio};
use crate::series::{AccountCapture, CaptureError};

#[derive(Debug, Clone)]
pub struct BacktestParams {
    pub initial_cash_nano_usd: NanoUsd,
    /// Sharpe annualization factor for the naive per-run Sharpe.
    pub bars_per_year: f64,
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    /// One `(avail_ts, equity)` point per bar, in nano-USD.
    pub equity: Vec<(Ts, NanoUsd)>,
    pub summary: Summary,
    pub n_fills: usize,
    /// Orders still pending when the feed ended (cancelled, never filled).
    pub cancelled_at_eof: usize,
    /// Every completed round-trip, stamped with the fill that flattened it.
    ///
    /// `summary` already aggregates these over the whole run; they are kept
    /// per-event so a caller slicing the run into windows — the walk-forward
    /// runner — can report the trades that happened inside each one instead
    /// of repeating the whole-run count under every fold.
    pub closed_trades: Vec<ClosedTrade>,
    /// Every nonzero commission, stamped with its fill. Same reason.
    pub fee_events: Vec<FeeEvent>,
    /// Fills produced by a protective bracket leg rather than by a strategy
    /// order. Included in `n_fills` as well — they are fills.
    pub n_protective_exits: usize,
    /// Bars on which a live bracket had **both** legs touched and the opening
    /// print settled neither, so the exit reported rests on
    /// `stop_first_intrabar` (see [`crate::bracket`]) rather than on the data.
    ///
    /// This is the path-sensitivity flag, and it is a count rather than a
    /// boolean because the question a reader has is "how much of this result
    /// turns on a convention?". Zero means every exit in the run was
    /// determined by the bars. A number approaching `n_protective_exits` means
    /// the run's PnL is largely an artifact of the worst-case rule, and a
    /// different-but-equally-defensible rule would have produced a different
    /// number — which is a reason to distrust the run, not to change the rule.
    pub path_sensitive_bars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The feed emitted events with decreasing availability time.
    OutOfOrderFeed { prev: Ts, next: Ts },
    /// The account-evaluation capture could not be fed. Only reachable from
    /// [`run_capturing`], and always a caller wiring bug: the trading-day keys
    /// did not describe the series being replayed.
    Capture(CaptureError),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::OutOfOrderFeed { prev, next } => write!(
                f,
                "feed violated ordering contract: {next} after {prev} (events must be nondecreasing in avail_ts)"
            ),
            EngineError::Capture(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EngineError::OutOfOrderFeed { .. } => None,
            EngineError::Capture(e) => Some(e),
        }
    }
}

impl From<CaptureError> for EngineError {
    fn from(e: CaptureError) -> EngineError {
        EngineError::Capture(e)
    }
}

/// Run one backtest to completion. Deterministic: identical inputs produce a
/// bit-identical `BacktestResult`.
///
/// # Errors
/// [`EngineError::OutOfOrderFeed`] if the feed violates its ordering contract.
pub fn run<F, S, M>(
    feed: &mut F,
    strategy: &mut S,
    fill_model: &mut M,
    spec: &ContractSpec,
    params: &BacktestParams,
) -> Result<BacktestResult, EngineError>
where
    F: Feed,
    S: Strategy,
    M: FillModel,
{
    run_inner(feed, strategy, fill_model, spec, params, None)
}

/// The same run, with the account-evaluation series captured as it happens
/// (`docs/ACCOUNT_EVAL_SPEC.md` §3, D-0071).
///
/// Capture is **additive observation**: it reads the marks the loop already
/// produces and changes no fill, no order and no equity point. A run through
/// this function and a run through [`run`] produce identical
/// [`BacktestResult`]s, which is asserted by
/// `tests/account_series.rs::capturing_changes_no_number_in_the_result`.
///
/// It is a separate entry point rather than a flag because the capture needs
/// something [`run`]'s arguments cannot supply: one trading-day key per bar,
/// which only a caller holding `crucible-data::calendar` can compute. See the
/// module docs of [`crate::series`] for why that is data rather than a
/// dependency.
///
/// # Errors
/// [`EngineError::OutOfOrderFeed`] if the feed violates its ordering contract,
/// or [`EngineError::Capture`] if the day keys do not describe the series the
/// feed emits.
pub fn run_capturing<F, S, M>(
    feed: &mut F,
    strategy: &mut S,
    fill_model: &mut M,
    spec: &ContractSpec,
    params: &BacktestParams,
    capture: &mut AccountCapture<'_>,
) -> Result<BacktestResult, EngineError>
where
    F: Feed,
    S: Strategy,
    M: FillModel,
{
    run_inner(feed, strategy, fill_model, spec, params, Some(capture))
}

fn run_inner<F, S, M>(
    feed: &mut F,
    strategy: &mut S,
    fill_model: &mut M,
    spec: &ContractSpec,
    params: &BacktestParams,
    mut capture: Option<&mut AccountCapture<'_>>,
) -> Result<BacktestResult, EngineError>
where
    F: Feed,
    S: Strategy,
    M: FillModel,
{
    let mut portfolio = Portfolio::new(spec.clone(), params.initial_cash_nano_usd);
    let mut pending: VecDeque<Order> = VecDeque::new();
    let mut actions = Actions::new();
    let mut equity: Vec<(Ts, NanoUsd)> = Vec::new();
    let mut last_avail: Option<Ts> = None;
    let mut next_order_id: u64 = 1;
    let mut n_fills = 0usize;
    let mut n_protective_exits = 0usize;
    let mut path_sensitive_bars = 0usize;
    // At most one, because the portfolio holds one position.
    let mut live_bracket: Option<ActiveBracket> = None;

    while let Some(ev) = feed.next_event() {
        let ts = ev.avail_ts();
        if let Some(prev) = last_avail
            && ts < prev
        {
            return Err(EngineError::OutOfOrderFeed { prev, next: ts });
        }
        last_avail = Some(ts);

        // 1.1. Fills — only orders placed strictly before this event.
        let mut still_pending = VecDeque::with_capacity(pending.len());
        while let Some(order) = pending.pop_front() {
            if order.placed_ts < ts {
                match fill_model.fill(&order, &ev, spec) {
                    Some(fill) => {
                        debug_assert_eq!(fill.order_id, order.id);
                        portfolio.apply_fill(&fill);
                        n_fills += 1;
                        live_bracket =
                            rebracket(live_bracket, &order, &fill, portfolio.position(), spec);
                    }
                    None => still_pending.push_back(order),
                }
            } else {
                still_pending.push_back(order);
            }
        }
        pending = still_pending;

        let MarketEvent::Bar(bar) = &ev;

        // 1.2. Protective exits — the resting half of a bracketed order. The
        //      `placed_ts` guard is the same §2.1 rule the loop above applies,
        //      restated for the leg the strategy never re-placed.
        if let Some(active) = live_bracket
            && active.placed_ts < ts
        {
            let resolution = StopFirstIntrabar::resolve(&active, bar);
            if resolution.path_sensitive {
                path_sensitive_bars += 1;
            }
            if let Outcome::Touched { leg, at, gapped } = resolution.outcome {
                let position = portfolio.position();
                debug_assert!(
                    position != Qty::ZERO,
                    "INVARIANT: a live bracket has a position to protect"
                );
                let exit = ProtectiveExit {
                    order_id: active.order_id,
                    ts,
                    side: active.exit_side,
                    // A bracket exits the whole position. A partial protective
                    // exit would leave the remainder naked, with no resting
                    // order left to say so and nothing in the report to notice.
                    qty: position.abs(),
                    leg,
                    touch_price: at,
                    gapped,
                };
                let fill = fill_model.fill_protective_exit(&exit, spec);
                debug_assert_eq!(fill.order_id, active.order_id);
                portfolio.apply_fill(&fill);
                n_fills += 1;
                n_protective_exits += 1;
                // One-cancels-the-other: the surviving leg is gone with it.
                live_bracket = None;
            }
        }

        // 2. Mark to this bar's close and record equity. The account-eval
        //    capture folds the SAME number the equity curve just took, at the
        //    same instant — spec §3.2/§3.3 name this line as the sampling
        //    point, and a second pass over the bars afterwards would be the
        //    reconstruction the spec forbids.
        portfolio.mark(bar.close);
        let equity_nano_usd = portfolio.equity_nano_usd();
        if let Some(capture) = capture.as_deref_mut() {
            capture.observe(equity.len(), equity_nano_usd)?;
        }
        equity.push((ts, equity_nano_usd));

        // 3. Strategy decides.
        let view = portfolio.view();
        strategy.on_event(&ev, &view, &mut actions);
        for intent in actions.take_intents() {
            pending.push_back(Order {
                id: next_order_id,
                instrument: ev.instrument().clone(),
                side: intent.side,
                qty: intent.qty.abs(),
                kind: intent.kind,
                placed_ts: ts,
                bracket: intent.bracket,
            });
            next_order_id += 1;
        }
    }

    let summary = Summary::compute(
        &equity,
        portfolio.closed_trades(),
        portfolio.fees_nano_usd(),
        params.bars_per_year,
    );
    let cancelled_at_eof = pending.len();
    let (closed_trades, fee_events) = portfolio.into_records();

    Ok(BacktestResult {
        equity,
        summary,
        n_fills,
        cancelled_at_eof,
        closed_trades,
        fee_events,
        n_protective_exits,
        path_sensitive_bars,
    })
}

/// The bracket bookkeeping that follows one strategy-order fill.
///
/// Three rules, in this order:
/// 1. A fill that leaves the position **flat** cancels the bracket. There is
///    nothing left to protect, and a resting stop under no position is how a
///    backtest opens a trade nobody asked for.
/// 2. A **bracketed** order that leaves a position installs levels measured
///    from *its own* fill price, replacing anything already there. That is what
///    makes the levels the ones the position actually got rather than the ones
///    its strategy hoped for.
/// 3. Otherwise the existing bracket survives — but only if it still protects
///    the position's side. A fill that **flips** long to short leaves the old
///    levels on the wrong side of the market, where they would exit the new
///    position at once and for the wrong reason.
///
/// Note what rule 3 deliberately does *not* do: adding to a bracketed position
/// with an unbracketed order keeps the original levels where they are, rather
/// than re-centering them on the new average entry. A resting order does not
/// move because you bought more.
fn rebracket(
    current: Option<ActiveBracket>,
    order: &Order,
    fill: &Fill,
    position: Qty,
    spec: &ContractSpec,
) -> Option<ActiveBracket> {
    let sign = position.as_i64();
    if sign == 0 {
        return None;
    }
    if let Some(bracket) = order.bracket {
        return Some(ActiveBracket::from_fill(
            order.id,
            order.placed_ts,
            bracket,
            fill.price,
            sign,
            spec.tick,
        ));
    }
    current.filter(|active| active.protects(sign))
}
