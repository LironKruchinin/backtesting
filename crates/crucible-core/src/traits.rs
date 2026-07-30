//! The trait seams. Everything pluggable in Crucible — data sources,
//! strategies, indicators, execution assumptions — plugs in here.

use crate::events::{
    Bar, Bracket, Fill, MarketEvent, Order, OrderIntent, OrderKind, ProtectiveExit, Side,
};
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
        self.order(Side::Buy, qty, None);
    }

    pub fn sell(&mut self, qty: Qty) {
        self.order(Side::Sell, qty, None);
    }

    /// Buy, and install a protective [`Bracket`] around whatever position the
    /// fill leaves open.
    pub fn buy_bracketed(&mut self, qty: Qty, bracket: Bracket) {
        self.order(Side::Buy, qty, Some(bracket));
    }

    /// Sell, and install a protective [`Bracket`] around whatever position the
    /// fill leaves open.
    pub fn sell_bracketed(&mut self, qty: Qty, bracket: Bracket) {
        self.order(Side::Sell, qty, Some(bracket));
    }

    /// Emit whatever single order moves `current` to `target`. The idiomatic
    /// way to express "be long 1 / be short 1 / be flat".
    pub fn target_position(&mut self, current: Qty, target: Qty) {
        self.move_position(current, target, None);
    }

    /// [`Actions::target_position`], with a protective bracket on the
    /// resulting position.
    ///
    /// The bracket rides along with the order and is dropped when the order
    /// would leave the position flat: a pure exit has nothing to protect, and
    /// attaching a bracket to it would install levels around a position that
    /// no longer exists.
    pub fn target_position_bracketed(&mut self, current: Qty, target: Qty, bracket: Bracket) {
        let bracket = (target.0 != 0).then_some(bracket);
        self.move_position(current, target, bracket);
    }

    /// Emit an intent verbatim.
    ///
    /// For decorators that rewrite what an inner strategy asked for — see
    /// `crucible-strategies::bracket` — rather than for strategies, which have
    /// the named helpers above.
    pub fn push_intent(&mut self, intent: OrderIntent) {
        self.intents.push(intent);
    }

    fn order(&mut self, side: Side, qty: Qty, bracket: Option<Bracket>) {
        self.intents.push(OrderIntent {
            side,
            qty: qty.abs(),
            kind: OrderKind::Market,
            bracket,
        });
    }

    fn move_position(&mut self, current: Qty, target: Qty, bracket: Option<Bracket>) {
        let delta = target.0 - current.0;
        match delta.cmp(&0) {
            std::cmp::Ordering::Greater => self.order(Side::Buy, Qty(delta), bracket),
            std::cmp::Ordering::Less => self.order(Side::Sell, Qty(-delta), bracket),
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

    /// Price a bracket leg that has already triggered.
    ///
    /// Whether it triggered, which leg won an ambiguous bar, and what price the
    /// level resolved to are decided *before* this call, once, by the engine's
    /// named intrabar convention — a fill model cannot move the path, only
    /// charge for it. It returns a `Fill` rather than an `Option<Fill>`
    /// because the trigger decision has been made: an execution assumption
    /// that could veto a touched stop would be modelling a market that lets
    /// you off.
    ///
    /// **Required, not defaulted.** A default implementation would price every
    /// bracketed strategy's exits under an assumption nobody named, which
    /// CLAUDE.md §2.4 forbids: the cost of a stop (a market order, crossing
    /// the spread) and of a target (a resting limit, not crossing it) is
    /// exactly the kind of optimism that has to be visible and greppable.
    fn fill_protective_exit(&mut self, exit: &ProtectiveExit, spec: &ContractSpec) -> Fill;
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

    #[test]
    fn a_plain_order_carries_no_bracket() {
        let mut a = Actions::new();
        a.buy(Qty(1));
        assert_eq!(a.take_intents()[0].bracket, None);
    }

    /// Going flat is a pure exit: there is no resulting position for a bracket
    /// to protect, so the bracket is dropped rather than installed around
    /// nothing.
    #[test]
    fn a_bracketed_exit_drops_the_bracket() {
        let bracket = Bracket::new(Some(8), Some(12));
        let mut a = Actions::new();
        a.target_position_bracketed(Qty(1), Qty(0), bracket);
        let intents = a.take_intents();
        assert_eq!(intents[0].side, Side::Sell);
        assert_eq!(intents[0].bracket, None);

        // A flip does leave a position, so it keeps its bracket.
        a.target_position_bracketed(Qty(1), Qty(-1), bracket);
        let intents = a.take_intents();
        assert_eq!(intents[0].qty, Qty(2));
        assert_eq!(intents[0].bracket, Some(bracket));
    }
}
