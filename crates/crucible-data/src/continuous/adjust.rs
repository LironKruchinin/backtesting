//! Two price series that must never be confused, kept apart by the type
//! system.
//!
//! A back-adjusted continuous series is **not a price**. It is a signal
//! input: the level at which the front contract traded, shifted by the sum of
//! every future roll gap, so that a moving average does not see a jump where
//! one contract handed over to the next. Nobody ever traded at that number.
//! PnL computed against it is wrong by the cumulative roll gap — which for ES
//! over a decade is hundreds of points, and which *looks fine* because the
//! equity curve is still smooth. That is the classic silent-corruption bug
//! this module exists to make unrepresentable.
//!
//! So [`AdjustedPrice`] is a separate type from
//! [`Price`](crucible_core::types::Price), with no `From`, no `Into`, no
//! `Deref` and no `as_price()`, and its only exit is
//! [`as_points_f64`](AdjustedPrice::as_points_f64) — indicator space, where
//! §2.3 already says `f64` belongs and where money cannot be computed. The
//! type itself lives in [`crucible_core::types`] and is re-exported here.
//!
//! **Why it moved into the core (D-0076).** It was defined in this module
//! while nothing replayed a continuous series. Wiring [`ContinuousFeed`] into
//! `crucible backtest` gave it a consumer that cannot see this crate:
//! `crucible-strategies` depends on `crucible-core` only, and it holds the
//! indicators — the one place an adjusted level is *supposed* to arrive.
//! Leaving the type here would have meant either a dependency edge §3 forbids
//! or a second adjusted-price type in the core, and a second spelling of "this
//! number is not tradeable" is how the first one stops being enforced. The
//! barrier is unchanged and is still proven by a `compile_fail` doctest paired
//! with a compiling one, now on the definition itself.
//!
//! [`ContinuousFeed`]: super::ContinuousFeed
//!
//! ## The arithmetic
//!
//! Additive back-adjustment, in integer nanopoints, exactly.
//!
//! At roll *k* the table records `adjustment_k = close(to) − close(from)`,
//! both closes taken from the last bar of each contract available at
//! `roll_ts`. With *n* rolls there are *n + 1* segments, and the offset
//! applied to segment *j* is
//!
//! ```text
//! offset_j = Σ_{k = j}^{n − 1} adjustment_k
//! ```
//!
//! so the **latest** segment carries offset zero and is left exactly as it
//! traded; history is shifted onto it. That direction is deliberate: it is
//! the one that never changes the most recent prices, so adding next month's
//! roll does not rewrite yesterday's signal.
//!
//! Every term is an `i64` difference of two `i64`s, so the sum is exact and
//! `raw + offset` is exact. No `f64` appears anywhere on this path (§2.3).
//!
//! ## Why there is no `Ratio` in v1
//!
//! Ratio (proportional) adjustment multiplies each segment by a running
//! product of `close(to) / close(from)`. In integer nanopoints that product
//! has to be rounded, and the rounding compounds once per roll — so the
//! adjusted value of a bar from 2010 would depend on how many rolls happen
//! *after* it, and two backtests ending in different years would disagree
//! about the same historical bar. Deterministic schemes exist (fixed-point
//! rationals carried as (numerator, denominator) pairs, or rescaling from the
//! raw series on every load), but each is a real design with real tests, and
//! shipping a rounded-`f64` version in the meantime would put floats in the
//! price path. `Ratio` therefore does not exist as a variant: a name that
//! parses and then silently means something else is worse than a name that
//! does not parse. It arrives with its own decision-log entry.

use crucible_core::events::Bar;
use crucible_core::types::{InstrumentId, Price};

pub use crucible_core::types::AdjustedPrice;

/// How a continuous series is adjusted **at load**.
///
/// Never baked into storage: a [`RollTable`](super::RollTable) records the
/// per-roll gaps, and the loader decides what to do with them. Storing
/// adjusted bars would make the archive depend on a research choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdjustmentKind {
    /// Leave every segment at the price it traded. The adjusted series is
    /// numerically equal to the tradeable one — still a different type, so a
    /// signal written against it keeps working when the kind changes.
    None,
    /// Additive back-adjustment (the default). See the module docs.
    #[default]
    BackAdjust,
}

impl core::fmt::Display for AdjustmentKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            AdjustmentKind::None => "none",
            AdjustmentKind::BackAdjust => "back_adjust",
        };
        write!(f, "{s}")
    }
}

/// One bar's OHLC in adjusted space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdjustedOhlc {
    /// Adjusted open.
    pub open: AdjustedPrice,
    /// Adjusted high.
    pub high: AdjustedPrice,
    /// Adjusted low.
    pub low: AdjustedPrice,
    /// Adjusted close.
    pub close: AdjustedPrice,
}

impl AdjustedOhlc {
    /// Shifts a bar's four prices by the bar's **own**
    /// [`signal_offset`](Bar::signal_offset).
    ///
    /// Deliberately not "by an offset you pass in": the bar carries the offset
    /// the engine's indicators will apply, and taking a second one here would
    /// let `ContinuousBar::adjusted` and `bar.signal_close()` disagree about
    /// the same bar. One source, one answer.
    #[must_use]
    pub const fn of(bar: &Bar) -> AdjustedOhlc {
        AdjustedOhlc {
            open: bar.signal_open(),
            high: bar.signal_high(),
            low: bar.signal_low(),
            close: bar.signal_close(),
        }
    }
}

/// One bar of a continuous series, in both spaces at once.
///
/// The two are carried side by side rather than as one convertible value
/// precisely so a consumer has to *name* which it wants. `tradeable` is what
/// filled; `adjusted` is what a signal should see.
///
/// ```compile_fail
/// # use crucible_core::types::{ContractSpec, InstrumentId, Price};
/// # use crucible_data::continuous::AdjustedPrice;
/// let spec = ContractSpec {
///     instrument: InstrumentId::new("ES.v.0"),
///     tick: Price::from_nanos(250_000_000),
///     point_value_usd: 50,
/// };
/// // The classic silent-corruption bug: PnL against a price nobody traded.
/// // It does not compile, which is the point.
/// let _ = spec.pnl_nano_usd(AdjustedPrice::from_nanos(1_000_000_000), 1);
/// ```
///
/// The same call with a tradeable [`Price`] compiles, which is what proves
/// the rejection above is about the type and not about a typo:
///
/// ```
/// # use crucible_core::types::{ContractSpec, InstrumentId, Price};
/// let spec = ContractSpec {
///     instrument: InstrumentId::new("ES.v.0"),
///     tick: Price::from_nanos(250_000_000),
///     point_value_usd: 50,
/// };
/// assert_eq!(spec.pnl_nano_usd(Price::from_nanos(1_000_000_000), 1), 50_000_000_000);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuousBar {
    /// The bar as it traded, labelled with the **continuous alias**
    /// (`ES.v.0`) so a single-instrument backtest sees one instrument across
    /// rolls. Its prices are the then-front contract's, unadjusted: this is
    /// what fills and what PnL must use.
    pub tradeable: Bar,
    /// The same bar shifted into adjusted space. Signals only.
    pub adjusted: AdjustedOhlc,
    /// The contract those tradeable prices actually belong to (`ESH2024`).
    pub contract: InstrumentId,
    /// The additive offset applied to reach `adjusted`, in nanopoints. Zero
    /// on the last segment by construction, and always equal to
    /// `tradeable.signal_offset` — the engine and this struct read the same
    /// number.
    pub offset: Price,
}

/// Per-segment additive offsets for a chain of roll gaps.
///
/// `gaps[k]` is roll *k*'s `close(to) − close(from)`; the result has
/// `gaps.len() + 1` entries, the last of which is always zero.
#[must_use]
pub fn back_adjust_offsets(gaps: &[Price]) -> Vec<Price> {
    let mut offsets = vec![Price::ZERO; gaps.len() + 1];
    // Accumulate from the back: the newest segment is untouched, and every
    // older one absorbs every gap that comes after it.
    for k in (0..gaps.len()).rev() {
        offsets[k] = offsets[k + 1] + gaps[k];
    }
    offsets
}

/// All-zero offsets, one per segment — [`AdjustmentKind::None`].
#[must_use]
pub fn unadjusted_offsets(segments: usize) -> Vec<Price> {
    vec![Price::ZERO; segments]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(points: i64) -> Price {
        Price::from_points(points)
    }

    // Hand-derived, the worked example from the module docs.
    //
    // Three contracts A -> B -> C with gaps g1 = close(B) - close(A) = +5 and
    // g2 = close(C) - close(B) = -3 (both in points).
    //   offset(C) = 0
    //   offset(B) = 0 + g2      = -3
    //   offset(A) = offset(B) + g1 = -3 + 5 = +2
    #[test]
    fn back_adjustment_accumulates_from_the_newest_segment_backwards() {
        let offsets = back_adjust_offsets(&[px(5), px(-3)]);
        assert_eq!(offsets, vec![px(2), px(-3), px(0)]);
    }

    // The property the arithmetic exists for: the adjusted series is
    // continuous across a roll. At the roll, A closed at 100 and B closed at
    // 105 (so g1 = +5). Adjusted, both are 102.
    #[test]
    fn adjusted_prices_meet_exactly_at_a_roll() {
        let offsets = back_adjust_offsets(&[px(5), px(-3)]);
        let a_at_roll = AdjustedPrice::from_tradeable(px(100), offsets[0]);
        let b_at_roll = AdjustedPrice::from_tradeable(px(105), offsets[1]);
        assert_eq!(a_at_roll, b_at_roll);
        assert_eq!(a_at_roll, AdjustedPrice::from_nanos(px(102).as_nanos()));

        // And at the second roll: B closed at 200, C at 197 (g2 = -3).
        let b_at_roll2 = AdjustedPrice::from_tradeable(px(200), offsets[1]);
        let c_at_roll2 = AdjustedPrice::from_tradeable(px(197), offsets[2]);
        assert_eq!(b_at_roll2, c_at_roll2);
        assert_eq!(c_at_roll2, AdjustedPrice::from_nanos(px(197).as_nanos()));
    }

    #[test]
    fn a_single_segment_needs_no_offset() {
        assert_eq!(back_adjust_offsets(&[]), vec![Price::ZERO]);
        assert_eq!(unadjusted_offsets(3), vec![Price::ZERO; 3]);
    }

    // Exactness is the reason this is integer arithmetic: one nanopoint in,
    // one nanopoint out, at real ES magnitudes where f64 has already run out
    // of mantissa.
    #[test]
    fn adjustment_is_exact_to_the_nanopoint() {
        let offsets = back_adjust_offsets(&[Price::from_nanos(1), Price::from_nanos(-2)]);
        assert_eq!(offsets[0], Price::from_nanos(-1));
        let raw = Price::from_nanos(5_000_000_000_001);
        assert_eq!(
            AdjustedPrice::from_tradeable(raw, offsets[0]).as_nanos(),
            5_000_000_000_000
        );
    }

    #[test]
    fn indicator_space_is_the_only_exit() {
        let a = AdjustedPrice::from_nanos(4_512_250_000_000);
        assert!((a.as_points_f64() - 4512.25).abs() < 1e-9);
        assert_eq!(a.to_string(), "4512.25~");
    }
}
