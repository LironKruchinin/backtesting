//! The trait seams. Everything pluggable in Crucible — data sources,
//! strategies, indicators, execution assumptions — plugs in here.

use crate::events::{Bar, Fill, MarketEvent, Order, OrderIntent, OrderKind, Side};
use crate::types::{ContractSpec, NanoUsd, Price, Qty};

/// A time-ordered source of market events.
///
/// ## Contract
/// Events MUST be yielded in nondecreasing `avail_ts` order. The engine
/// verifies this at runtime and aborts the run on violation — a misordered
/// feed is a correctness bug, not a recoverable condition.
///
/// Implementations: historical replay (Parquet/DBN), synthetic generators,
/// and (milestone M4+) the live paper-trading feed. The engine cannot tell
/// them apart, which is what makes the backtest→paper path free.
pub trait Feed {
    fn next_event(&mut self) -> Option<MarketEvent>;
}

/// A streaming indicator: O(1) state update per bar, no history replays.
///
/// ## Contract
/// - Returns `None` until it has consumed `warmup()` bars, then `Some` on
///   every subsequent bar.
/// - `update` must be called with bars in availability order; indicators do
///   not defend against out-of-order input (the engine guarantees it).
/// - Implementations may use `f64` internally (indicator space), but must
///   consume prices via `Price::as_points_f64` — never raw nanos, which
///   destroy `f64` precision at real price magnitudes.
pub trait Indicator {
    type Out;
    /// Number of bars consumed before output begins.
    fn warmup(&self) -> usize;
    fn update(&mut self, bar: &Bar) -> Option<Self::Out>;
}

/// Read-only snapshot of portfolio state, as of the moment the current event
/// became available. Single-instrument in v0 (documented limitation; the
/// cross-sectional portfolio arrives with SSRN-style strategies, post-M4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortfolioView {
    /// Signed position: long > 0, short < 0.
    pub position: Qty,
    /// Volume-weighted average entry price of the open position, if any.
    pub avg_entry: Option<Price>,
    pub cash_nano_usd: NanoUsd,
    /// Cash + unrealized PnL, marked at the most recent close.
    pub equity_nano_usd: NanoUsd,
}

/// Order intents a strategy emits while handling one event. The engine
/// converts intents to [`Order`]s stamped with the current event's
/// `avail_ts`; they become fillable strictly afterwards.
#[derive(Debug, Default)]
pub struct Actions {
    intents: Vec<OrderIntent>,
}

impl Actions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn buy(&mut self, qty: Qty) {
        self.intents.push(OrderIntent {
            side: Side::Buy,
            qty: qty.abs(),
            kind: OrderKind::Market,
        });
    }

    pub fn sell(&mut self, qty: Qty) {
        self.intents.push(OrderIntent {
            side: Side::Sell,
            qty: qty.abs(),
            kind: OrderKind::Market,
        });
    }

    /// Emit whatever single order moves `current` to `target`. The idiomatic
    /// way to express "be long 1 / be short 1 / be flat".
    pub fn target_position(&mut self, current: Qty, target: Qty) {
        let delta = target.0 - current.0;
        match delta.cmp(&0) {
            std::cmp::Ordering::Greater => self.buy(Qty(delta)),
            std::cmp::Ordering::Less => self.sell(Qty(-delta)),
            std::cmp::Ordering::Equal => {}
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }

    /// Consume the accumulated intents (engine-side use).
    pub fn take_intents(&mut self) -> Vec<OrderIntent> {
        std::mem::take(&mut self.intents)
    }
}

/// A trading strategy driven by market events.
///
/// ## Contract
/// - `on_event` is invoked once per event, in availability order. The
///   information visible at that call is exactly: the event itself, all
///   prior events, and `view`. Nothing else exists yet.
/// - Strategies must be deterministic: no clocks, no OS randomness, no
///   iteration over unordered maps. Randomized strategies take a seed at
///   construction (CLAUDE.md §2.2).
/// - Emit orders via `actions`; never assume they fill (check `view` on
///   subsequent events).
pub trait Strategy {
    /// Bars to consume before the evaluation window opens. The funnel uses
    /// the max warmup across a grid so every combo is scored on an identical
    /// window (CLAUDE.md §2.6).
    fn warmup_bars(&self) -> usize;

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions);
}

/// Execution assumption: decides whether/how an order fills against a market
/// event.
///
/// ## Contract
/// - The engine calls `fill` with events **strictly after** `order.placed_ts`
///   — a fill model never sees the event that triggered the order, so
///   same-bar fantasy fills are impossible by construction.
/// - Returned fill prices must be tick-aligned via `Price::round_to_tick`.
/// - Fill models are named, versioned assumptions (see `crucible-engine`):
///   scorecards always state which fill model produced a number.
pub trait FillModel {
    fn fill(
        &mut self,
        order: &Order,
        next_event: &MarketEvent,
        spec: &ContractSpec,
    ) -> Option<Fill>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Qty;

    #[test]
    fn target_position_emits_minimal_delta() {
        let mut a = Actions::new();
        a.target_position(Qty(-1), Qty(1)); // short 1 -> long 1: buy 2
        let intents = a.take_intents();
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].side, Side::Buy);
        assert_eq!(intents[0].qty, Qty(2));
    }

    #[test]
    fn target_position_noop_when_already_there() {
        let mut a = Actions::new();
        a.target_position(Qty(3), Qty(3));
        assert!(a.is_empty());
    }
}
