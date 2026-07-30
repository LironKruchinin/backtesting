//! Market events and order/fill records.
//!
//! ## The availability-time invariant (CLAUDE.md §2.1)
//!
//! Every event distinguishes two instants:
//! - **event time** — when the thing happened in the market (a bar's
//!   `ts_open` marks the *start* of its interval, matching Databento).
//! - **availability time** (`avail_ts`) — the earliest instant the completed
//!   information could have been known. For a bar this is
//!   `ts_open + timeframe`: a 1-minute bar opening at 09:30:00 is knowable at
//!   09:31:00, not before.
//!
//! Replay order, strategy visibility, and fill sequencing are **always**
//! governed by `avail_ts`. Keying anything on `ts_open` lets strategies see
//! one bar into the future — the exact bias this project exists to prevent.
//!
//! ## Two price views on one bar (CLAUDE.md §2.3, D-0042, D-0073)
//!
//! A [`Bar`]'s four `Price` fields are always the **tradeable** prices: what
//! the market actually quoted, what a fill model may charge against, and the
//! only thing `ContractSpec::pnl_nano_usd` will accept. Beside them sits
//! [`Bar::signal_offset`], an additive shift into **signal space**, and four
//! accessors that apply it and hand back an
//! [`AdjustedPrice`](crate::types::AdjustedPrice) — a type with no way back to
//! `Price`.
//!
//! The offset is zero for every outright contract and every synthetic series,
//! so the two views coincide numerically there and nothing changes. It is
//! nonzero only on a stitched continuous series, where the older segments are
//! shifted onto the newest so an indicator does not see a step at each roll.
//! Indicators and strategies read the signal view; fills, marks and PnL read
//! the tradeable one. Which one a caller gets is decided by which method it
//! names, and the type system refuses the confusion rather than trusting a
//! comment.
//!
//! ## Brackets and the one thing an OHLC bar refuses to say
//!
//! [`Bracket`] attaches protective levels to an order. A resting stop or
//! target is not a *decision*, so testing it against the high and low of the
//! bar it rests inside is not lookahead — but a bar does not record whether
//! its high or its low printed first, so a stop and a target inside one bar
//! are genuinely path-ambiguous. That ambiguity is resolved by a single named
//! convention in `crucible-engine::bracket` (`stop_first_intrabar`), never
//! here and never per fill model, and every run counts the bars where it
//! mattered.

use crate::types::{AdjustedPrice, InstrumentId, NanoUsd, Price, Qty, TimeFrame, Ts};

/// One OHLCV bar. `ts_open` marks the interval start (Databento convention).
///
/// `open`/`high`/`low`/`close` are **tradeable** prices. The signal view of
/// the same bar is `signal_*()`; see the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    pub instrument: InstrumentId,
    pub tf: TimeFrame,
    /// Interval start (event time). NOT the time this bar becomes knowable.
    pub ts_open: Ts,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: u64,
    /// Additive shift from tradeable space into **signal space**, nanopoints.
    ///
    /// [`Price::ZERO`] for every bar of a real contract and every synthetic
    /// one — a price nobody adjusted is its own signal. Nonzero only on a
    /// stitched continuous series, where it is the sum of the roll gaps that
    /// come *after* this bar's segment, so the whole history lines up on the
    /// newest contract's scale (`crucible-data::continuous::adjust`).
    ///
    /// Carried as the offset rather than as a second set of prices for the
    /// reason `RollTable` stores gaps rather than running offsets (D-0043):
    /// the tradeable prices stay the bytes the archive holds, the adjustment
    /// stays a load-time research choice, and there is exactly one way to
    /// combine them — [`AdjustedPrice::from_tradeable`], which is one-way.
    pub signal_offset: Price,
}

impl Bar {
    /// The earliest instant this completed bar could be known:
    /// `ts_open + tf`. All ordering and decision logic uses this.
    #[must_use]
    pub const fn avail_ts(&self) -> Ts {
        self.ts_open.plus_ns(self.tf.duration_ns())
    }

    /// The bar's open in signal space. Indicators only.
    #[must_use]
    pub const fn signal_open(&self) -> AdjustedPrice {
        AdjustedPrice::from_tradeable(self.open, self.signal_offset)
    }

    /// The bar's high in signal space. Indicators only.
    #[must_use]
    pub const fn signal_high(&self) -> AdjustedPrice {
        AdjustedPrice::from_tradeable(self.high, self.signal_offset)
    }

    /// The bar's low in signal space. Indicators only.
    #[must_use]
    pub const fn signal_low(&self) -> AdjustedPrice {
        AdjustedPrice::from_tradeable(self.low, self.signal_offset)
    }

    /// The bar's close in signal space. Indicators only.
    ///
    /// This is what an indicator consumes, and it is deliberately *not* what
    /// `Portfolio::mark` consumes: marking to an adjusted close would price a
    /// position at a level nobody traded.
    #[must_use]
    pub const fn signal_close(&self) -> AdjustedPrice {
        AdjustedPrice::from_tradeable(self.close, self.signal_offset)
    }
}

/// Any event a [`crate::traits::Feed`] can emit.
///
/// Adding a variant (Trade, Quote, MacroRelease, …) is a deliberate breaking
/// change: every `match` in the engine must be revisited so the new event's
/// availability and fill semantics are handled explicitly, not silently
/// ignored. Record the addition in `docs/DECISIONS.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketEvent {
    Bar(Bar),
}

impl MarketEvent {
    /// Availability time of the event — the global replay ordering key.
    #[must_use]
    pub const fn avail_ts(&self) -> Ts {
        match self {
            MarketEvent::Bar(b) => b.avail_ts(),
        }
    }

    #[must_use]
    pub fn instrument(&self) -> &InstrumentId {
        match self {
            MarketEvent::Bar(b) => &b.instrument,
        }
    }
}

/// Order direction. `sign()` gives +1 for Buy, −1 for Sell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    #[must_use]
    pub const fn sign(self) -> i64 {
        match self {
            Side::Buy => 1,
            Side::Sell => -1,
        }
    }
}

/// Order type. v0 supports market orders only; `Limit` arrives with the
/// queue-position fill model (milestone M4) so that limit fills are never
/// modeled optimistically by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderKind {
    Market,
}

/// Protective exit levels attached to an entry order, expressed as **tick
/// distances from the price that order actually fills at**.
///
/// Offsets rather than absolute prices, and that is not a convenience. A
/// strategy places its entry before knowing what the entry costs: under any
/// honest fill model the price is not knowable until the next event arrives
/// (§2.1). A bracket pinned to absolute levels would therefore have to be
/// either computed from a price the strategy *hoped* for, or placed one bar
/// later — a bar in which the position is unprotected, which is the one thing
/// a stop exists to prevent. Riding along with the parent order and resolving
/// against its fill price makes the position protected from the instant it
/// exists.
///
/// A bracket is **one-cancels-the-other**: whichever leg triggers first
/// flattens the position and the other is gone. Two independent resting
/// orders could both fill inside one bar and double the position in the wrong
/// direction — a bug that looks like a strategy discovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bracket {
    stop_ticks: Option<i64>,
    target_ticks: Option<i64>,
}

impl Bracket {
    /// A bracket `stop_ticks` against and `target_ticks` in favour of the
    /// position, measured from the parent order's fill price. At least one leg
    /// must be present.
    ///
    /// # Panics
    /// Panics unless every supplied leg is a positive tick count and at least
    /// one leg is supplied. Both are config bugs, not runtime conditions: a
    /// zero-tick stop sits exactly on the entry and a negative one sits on the
    /// wrong side of it, and a bracket with neither leg is a protective order
    /// that protects nothing (CLAUDE.md §5.1 — fail at build-the-strategy
    /// time, never silently mid-run).
    #[must_use]
    pub fn new(stop_ticks: Option<i64>, target_ticks: Option<i64>) -> Bracket {
        assert!(
            stop_ticks.is_some() || target_ticks.is_some(),
            "a bracket needs at least one of stop_ticks / target_ticks"
        );
        assert!(
            stop_ticks.is_none_or(|t| t > 0),
            "stop_ticks must be a positive distance from the fill price"
        );
        assert!(
            target_ticks.is_none_or(|t| t > 0),
            "target_ticks must be a positive distance from the fill price"
        );
        Bracket {
            stop_ticks,
            target_ticks,
        }
    }

    /// Ticks between the fill price and the stop — *below* it for a long
    /// position, *above* it for a short.
    #[must_use]
    pub const fn stop_ticks(self) -> Option<i64> {
        self.stop_ticks
    }

    /// Ticks between the fill price and the profit target — *above* it for a
    /// long position, *below* it for a short.
    #[must_use]
    pub const fn target_ticks(self) -> Option<i64> {
        self.target_ticks
    }
}

/// Which half of a [`Bracket`] produced an exit.
///
/// The distinction is a cost distinction, not a bookkeeping one: a stop is a
/// market order once touched and pays the spread, while a target is a resting
/// limit that does not. A fill model that priced both the same way would hide
/// half of a bracketed strategy's execution cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BracketLeg {
    /// The loss-limiting leg.
    Stop,
    /// The profit-taking leg.
    Target,
}

impl BracketLeg {
    /// Report spelling: `"stop"` / `"target"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            BracketLeg::Stop => "stop",
            BracketLeg::Target => "target",
        }
    }
}

/// A triggered bracket leg, handed to a fill model to be priced.
///
/// The *ordering* decision — which leg triggered, and what price its level
/// resolved to — is made once, by the engine's named intrabar convention, so
/// no two fill models can disagree about the path a bar took. What a fill
/// model decides is the **cost** of the exit, which differs per leg and per
/// assumption. That is why [`crate::traits::FillModel`] carries a second
/// *required* method rather than a defaulted one: a defaulted one would be an
/// anonymous execution assumption, which CLAUDE.md §2.4 does not allow to
/// exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectiveExit {
    /// Id of the order whose bracket this is — a protective exit is that
    /// order's other half, not an order of its own.
    pub order_id: u64,
    /// `avail_ts` of the bar the level was touched on.
    pub ts: Ts,
    /// The side that flattens the position.
    pub side: Side,
    /// Positive magnitude: the whole open position.
    pub qty: Qty,
    pub leg: BracketLeg,
    /// The price the level resolved to *before* the fill model's own costs:
    /// the level itself, or the bar's opening print when the bar opened
    /// already beyond it.
    pub touch_price: Price,
    /// True when the bar opened at or beyond the level, so `touch_price` is
    /// the opening print rather than the level. A gapped stop is worse than
    /// its level; a gapped target is better than its level. Both are what the
    /// market offered.
    pub gapped: bool,
}

/// A strategy's request for an order, before the engine assigns identity.
/// `qty` is a positive magnitude; direction lives in `side`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderIntent {
    pub side: Side,
    pub qty: Qty,
    pub kind: OrderKind,
    /// Protective levels to install when this order fills, if any. `None` is
    /// a naked position — the historical default, and still the default.
    pub bracket: Option<Bracket>,
}

/// An order accepted by the engine, awaiting a fill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub id: u64,
    pub instrument: InstrumentId,
    pub side: Side,
    /// Positive magnitude; see [`OrderIntent`].
    pub qty: Qty,
    pub kind: OrderKind,
    /// `avail_ts` of the event during which the order was placed. Fill models
    /// only ever see events strictly after this instant.
    pub placed_ts: Ts,
    /// Protective levels to install when this order fills; see [`Bracket`].
    /// The bracket inherits this order's `placed_ts`, which is what keeps it
    /// inside §2.1: it can only ever be tested against events strictly after
    /// the event that asked for it.
    pub bracket: Option<Bracket>,
}

/// An execution. Produced exclusively by fill models.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fill {
    pub order_id: u64,
    pub ts: Ts,
    pub side: Side,
    /// Positive magnitude.
    pub qty: Qty,
    /// Tick-aligned execution price, all cost adjustments included.
    pub price: Price,
    /// Commission + exchange fees for this fill (not spread — spread is
    /// expressed in `price`).
    pub fee_nano_usd: NanoUsd,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PRICE_SCALE;

    fn bar_at(ts_open_ns: i64, tf: TimeFrame) -> Bar {
        Bar {
            instrument: InstrumentId::new("SYN:TEST"),
            tf,
            ts_open: Ts(ts_open_ns),
            open: Price::from_nanos(PRICE_SCALE),
            high: Price::from_nanos(PRICE_SCALE),
            low: Price::from_nanos(PRICE_SCALE),
            close: Price::from_nanos(PRICE_SCALE),
            volume: 1,
            signal_offset: Price::ZERO,
        }
    }

    /// THE invariant: a 1m bar opening at T is only knowable at T + 60s.
    #[test]
    fn bar_availability_is_open_plus_interval() {
        let b = bar_at(1_000_000_000_000, TimeFrame::M1);
        assert_eq!(b.avail_ts(), Ts(1_000_000_000_000 + 60_000_000_000));
        let d = bar_at(0, TimeFrame::D1);
        assert_eq!(d.avail_ts(), Ts(86_400_000_000_000));
    }

    #[test]
    fn side_signs() {
        assert_eq!(Side::Buy.sign(), 1);
        assert_eq!(Side::Sell.sign(), -1);
    }

    #[test]
    fn a_bracket_keeps_the_legs_it_was_given() {
        let b = Bracket::new(Some(8), Some(12));
        assert_eq!(b.stop_ticks(), Some(8));
        assert_eq!(b.target_ticks(), Some(12));
        let stop_only = Bracket::new(Some(4), None);
        assert_eq!(stop_only.target_ticks(), None);
    }

    #[test]
    #[should_panic(expected = "at least one")]
    fn a_bracket_with_neither_leg_is_a_config_bug() {
        let _ = Bracket::new(None, None);
    }

    /// A zero-tick stop sits exactly on the entry price, which is not a stop.
    #[test]
    #[should_panic(expected = "stop_ticks must be a positive distance")]
    fn a_zero_tick_stop_is_a_config_bug() {
        let _ = Bracket::new(Some(0), Some(4));
    }

    #[test]
    #[should_panic(expected = "target_ticks must be a positive distance")]
    fn a_negative_target_is_a_config_bug() {
        let _ = Bracket::new(Some(4), Some(-4));
    }
}
