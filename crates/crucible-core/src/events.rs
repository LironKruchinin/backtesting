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

use crate::types::{InstrumentId, NanoUsd, Price, Qty, TimeFrame, Ts};

/// One OHLCV bar. `ts_open` marks the interval start (Databento convention).
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
}

impl Bar {
    /// The earliest instant this completed bar could be known:
    /// `ts_open + tf`. All ordering and decision logic uses this.
    #[must_use]
    pub const fn avail_ts(&self) -> Ts {
        self.ts_open.plus_ns(self.tf.duration_ns())
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

/// A strategy's request for an order, before the engine assigns identity.
/// `qty` is a positive magnitude; direction lives in `side`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderIntent {
    pub side: Side,
    pub qty: Qty,
    pub kind: OrderKind,
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
}
