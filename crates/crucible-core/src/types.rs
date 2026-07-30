//! Fundamental value types. Units are part of the type or the name — a bare
//! `i64` with an ambiguous unit is a bug by convention (CLAUDE.md §5.3).

use std::fmt;
use std::str::FromStr;

/// Fixed-point price scale: 1 point = 1e9 units. This matches Databento's
/// native `1e-9` price encoding, so ingested prices are stored losslessly
/// with **no float conversion anywhere in the data path**.
pub const PRICE_SCALE: i64 = 1_000_000_000;

/// Money type: integer nanodollars (1 USD = 1e9). All PnL, fees, cash, and
/// equity accounting uses this. Convert to `f64` only for *statistics and
/// display*, never for accounting.
pub type NanoUsd = i64;

/// Convert nano-USD to `f64` dollars for display/statistics only.
#[must_use]
pub fn nano_usd_to_f64(n: NanoUsd) -> f64 {
    n as f64 / 1e9
}

/// A price in fixed-point nanopoints (1 point = [`PRICE_SCALE`] units).
///
/// Arithmetic is plain integer arithmetic; the workspace enables
/// `overflow-checks` in every profile, so overflow aborts instead of
/// corrupting results silently.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Price(i64);

impl Price {
    pub const ZERO: Price = Price(0);

    /// Construct from raw nanopoints (Databento-native encoding).
    #[must_use]
    pub const fn from_nanos(nanos: i64) -> Self {
        Price(nanos)
    }

    /// Raw nanopoints.
    #[must_use]
    pub const fn as_nanos(self) -> i64 {
        self.0
    }

    /// Construct from whole points, e.g. `Price::from_points(5000)`.
    #[must_use]
    pub const fn from_points(points: i64) -> Self {
        Price(points * PRICE_SCALE)
    }

    /// Lossy conversion for **indicators, statistics, and synthetic data
    /// only**. Never route accounting through this (CLAUDE.md §2.3).
    #[must_use]
    pub fn as_points_f64(self) -> f64 {
        self.0 as f64 / PRICE_SCALE as f64
    }

    /// Lossy construction for **tests and synthetic data only**.
    #[must_use]
    pub fn from_points_f64_lossy(points: f64) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "documented lossy constructor"
        )]
        Price((points * PRICE_SCALE as f64).round() as i64)
    }

    /// Snap to the nearest multiple of `tick`. Fill prices must always be
    /// tick-aligned.
    #[must_use]
    pub fn round_to_tick(self, tick: Price) -> Self {
        debug_assert!(tick.0 > 0, "tick must be positive");
        let half = tick.0 / 2;
        let offset = if self.0 >= 0 { half } else { -half };
        Price((self.0 + offset) / tick.0 * tick.0)
    }

    /// Parses a decimal price in points (`"0.25"`, `"5000"`, `"-1.5"`)
    /// **without ever constructing an `f64`**.
    ///
    /// The lossy constructor above is sanctioned for tests and synthetic
    /// data; a tick size or a contract price typed by an operator is neither.
    /// `"0.25"` happens to be exact in binary, but `"0.1"` is not, and a tick
    /// size one nanopoint off snaps every fill in a backtest to the wrong
    /// grid — quietly, and only on some instruments (§2.3, and the same
    /// argument D-0027 makes for money).
    ///
    /// Accepts an optional sign and `_` digit separators. Rejects exponents
    /// and anything finer than a nanopoint, because both are typos rather
    /// than prices.
    ///
    /// # Errors
    /// [`ParsePriceError`] describing what was wrong with the text.
    pub fn from_points_str(text: &str) -> Result<Price, ParsePriceError> {
        let bad = |reason: &str| ParsePriceError(format!("invalid price {text:?}: {reason}"));

        let trimmed = text.trim();
        let (negative, body) = match trimmed.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
        };
        let body: String = body.chars().filter(|c| *c != '_').collect();
        if body.is_empty() {
            return Err(bad("no digits"));
        }
        if body.contains(['e', 'E']) {
            return Err(bad("exponent notation is not accepted for a price"));
        }

        let mut parts = body.split('.');
        let int_part = parts.next().unwrap_or("");
        let frac_part = parts.next().unwrap_or("");
        if parts.next().is_some() {
            return Err(bad("more than one decimal point"));
        }
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(bad("no digits"));
        }
        if !int_part.chars().all(|c| c.is_ascii_digit())
            || !frac_part.chars().all(|c| c.is_ascii_digit())
        {
            return Err(bad("contains a non-digit"));
        }
        // PRICE_SCALE is 1e-9, so nine fractional digits is the finest a
        // price can express; more is a typo, not precision.
        if frac_part.len() > 9 {
            return Err(bad(
                "more than nine fractional digits; one nanopoint is the finest price",
            ));
        }

        let mut digits = String::with_capacity(int_part.len() + 9);
        digits.push_str(if int_part.is_empty() { "0" } else { int_part });
        digits.push_str(frac_part);
        for _ in frac_part.len()..9 {
            digits.push('0');
        }
        let magnitude: i64 = digits
            .parse()
            .map_err(|_| bad("does not fit in i64 nanopoints"))?;
        Ok(Price(if negative { -magnitude } else { magnitude }))
    }
}

/// Error returned when parsing a [`Price`] from decimal text fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsePriceError(pub String);

impl fmt::Display for ParsePriceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParsePriceError {}

impl std::ops::Add for Price {
    type Output = Price;
    fn add(self, rhs: Price) -> Price {
        Price(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Price {
    type Output = Price;
    fn sub(self, rhs: Price) -> Price {
        Price(self.0 - rhs.0)
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let scale = PRICE_SCALE.unsigned_abs();
        let whole = abs / scale;
        let frac = abs % scale;
        if frac == 0 {
            write!(f, "{sign}{whole}")
        } else {
            let frac_str = format!("{frac:09}");
            write!(f, "{sign}{whole}.{}", frac_str.trim_end_matches('0'))
        }
    }
}

/// A price level in **signal space**: a [`Price`] shifted by an additive
/// offset that no market ever quoted.
///
/// A back-adjusted continuous futures series is the reason this type exists
/// (D-0042). Stitching `ESH2024` onto `ESM2024` leaves a step where one
/// contract handed over to the next, and shifting the older segment by the
/// roll gap removes it — but nobody ever traded at the shifted number. PnL
/// computed against it is wrong by the cumulative roll gap, which for ES over
/// a decade is hundreds of points, and it *looks fine* because the equity
/// curve stays smooth. That is the silent corruption this type makes
/// unrepresentable.
///
/// So `AdjustedPrice` is deliberately a dead end:
///
/// - no `From<AdjustedPrice> for Price`, no `Into<Price>`, no
///   `Deref<Target = Price>`, no `as_price()`, and none may ever be added;
/// - the only exit is [`as_points_f64`](AdjustedPrice::as_points_f64), which
///   lands in indicator space where §2.3 already says `f64` belongs and where
///   money cannot be computed;
/// - [`ContractSpec::pnl_nano_usd`] — the one sanctioned price→money
///   conversion in the workspace — takes `Price`, so an adjusted level cannot
///   reach it.
///
/// A caller reaching for a conversion wants the tradeable price, and every
/// [`Bar`](crate::events::Bar) carries it in `open`/`high`/`low`/`close`, next
/// to the offset that produces this.
///
/// The rejection is a property of the type, not of a typo, and both halves are
/// proven. This does not compile:
///
/// ```compile_fail
/// # use crucible_core::types::{AdjustedPrice, ContractSpec, InstrumentId, Price};
/// let spec = ContractSpec {
///     instrument: InstrumentId::new("ES.v.0"),
///     tick: Price::from_nanos(250_000_000),
///     point_value_usd: 50,
/// };
/// let _ = spec.pnl_nano_usd(AdjustedPrice::from_nanos(1_000_000_000), 1);
/// ```
///
/// and the same call with a tradeable [`Price`] does:
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct AdjustedPrice(i64);

impl AdjustedPrice {
    /// Zero, for completeness with [`Price::ZERO`].
    pub const ZERO: AdjustedPrice = AdjustedPrice(0);

    /// Applies an additive offset to a tradeable price.
    ///
    /// The **only** constructor that takes a [`Price`], and the one-way door:
    /// what comes out cannot go back.
    #[must_use]
    pub const fn from_tradeable(price: Price, offset: Price) -> AdjustedPrice {
        AdjustedPrice(price.as_nanos() + offset.as_nanos())
    }

    /// Construct from raw nanopoints. For tests and for reading an
    /// already-adjusted level; not a conversion from a tradeable price.
    #[must_use]
    pub const fn from_nanos(nanos: i64) -> AdjustedPrice {
        AdjustedPrice(nanos)
    }

    /// Raw nanopoints. Deliberately an `i64` and not a [`Price`]: an integer
    /// carries no unit claim, so nothing downstream can mistake it for
    /// something tradeable.
    #[must_use]
    pub const fn as_nanos(self) -> i64 {
        self.0
    }

    /// Points as `f64` — the sanctioned exit into indicator space, matching
    /// [`Price::as_points_f64`] (§2.3: indicators consume points, never raw
    /// nanos).
    #[must_use]
    pub fn as_points_f64(self) -> f64 {
        Price::from_nanos(self.0).as_points_f64()
    }
}

impl std::ops::Add for AdjustedPrice {
    type Output = AdjustedPrice;
    fn add(self, rhs: AdjustedPrice) -> AdjustedPrice {
        AdjustedPrice(self.0 + rhs.0)
    }
}

impl std::ops::Sub for AdjustedPrice {
    type Output = AdjustedPrice;
    fn sub(self, rhs: AdjustedPrice) -> AdjustedPrice {
        AdjustedPrice(self.0 - rhs.0)
    }
}

impl fmt::Display for AdjustedPrice {
    /// Trailing `~` so an adjusted level is never mistaken for a quote in a
    /// log line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}~", Price::from_nanos(self.0))
    }
}

/// Position/order size in whole contracts. Sign convention: **orders carry a
/// positive `Qty` plus a [`crate::events::Side`]; positions are signed**
/// (long > 0, short < 0).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Qty(pub i32);

impl Qty {
    pub const ZERO: Qty = Qty(0);

    #[must_use]
    pub const fn abs(self) -> Qty {
        Qty(self.0.abs())
    }

    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0 as i64
    }
}

impl fmt::Display for Qty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A UTC timestamp in nanoseconds since the Unix epoch.
///
/// All timestamps inside Crucible are UTC nanoseconds. Timezone/session logic
/// lives at the edges (`crucible-data::calendar`), never in the engine.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Ts(pub i64);

impl Ts {
    #[must_use]
    pub const fn plus_ns(self, ns: i64) -> Ts {
        Ts(self.0 + ns)
    }
}

impl fmt::Display for Ts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ns", self.0)
    }
}

/// Bar interval. String forms (`"1m"`, `"1h"`, …) are the canonical config
/// spelling — parse with `FromStr`, render with `Display`, and never invent
/// alternative spellings.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TimeFrame {
    S1,
    M1,
    M5,
    M15,
    H1,
    D1,
}

impl TimeFrame {
    /// Length of one bar interval in nanoseconds.
    #[must_use]
    pub const fn duration_ns(self) -> i64 {
        match self {
            TimeFrame::S1 => 1_000_000_000,
            TimeFrame::M1 => 60 * 1_000_000_000,
            TimeFrame::M5 => 5 * 60 * 1_000_000_000,
            TimeFrame::M15 => 15 * 60 * 1_000_000_000,
            TimeFrame::H1 => 60 * 60 * 1_000_000_000,
            TimeFrame::D1 => 24 * 60 * 60 * 1_000_000_000,
        }
    }
}

impl fmt::Display for TimeFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TimeFrame::S1 => "1s",
            TimeFrame::M1 => "1m",
            TimeFrame::M5 => "5m",
            TimeFrame::M15 => "15m",
            TimeFrame::H1 => "1h",
            TimeFrame::D1 => "1d",
        };
        write!(f, "{s}")
    }
}

/// Error returned when parsing a [`TimeFrame`] from a string fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseTimeFrameError(pub String);

impl fmt::Display for ParseTimeFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown timeframe {:?} (expected one of: 1s 1m 5m 15m 1h 1d)",
            self.0
        )
    }
}

impl std::error::Error for ParseTimeFrameError {}

impl FromStr for TimeFrame {
    type Err = ParseTimeFrameError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1s" => Ok(TimeFrame::S1),
            "1m" => Ok(TimeFrame::M1),
            "5m" => Ok(TimeFrame::M5),
            "15m" => Ok(TimeFrame::M15),
            "1h" => Ok(TimeFrame::H1),
            "1d" => Ok(TimeFrame::D1),
            other => Err(ParseTimeFrameError(other.to_owned())),
        }
    }
}

/// Instrument identifier. Convention (CLAUDE.md §4): Databento raw symbols
/// (`"ESU6"`) or continuous aliases (`"ES.c.0"`); synthetic instruments use
/// the `"SYN:*"` prefix.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct InstrumentId(String);

impl InstrumentId {
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        InstrumentId(s.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Static facts about a contract, sourced from the Databento `definition`
/// schema in real use (milestone M1). PnL for a 1-point favorable move on one
/// contract is `point_value_usd` dollars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractSpec {
    pub instrument: InstrumentId,
    /// Minimum price increment, e.g. 0.25 points for ES.
    pub tick: Price,
    /// Dollars per point per contract, e.g. 50 for ES.
    pub point_value_usd: i64,
}

impl ContractSpec {
    /// PnL in nano-USD for a signed price move on a signed position.
    ///
    /// `delta` is (current − reference) price; `signed_qty` is the position
    /// (long > 0). This is the **only** place price-to-money conversion is
    /// allowed to happen.
    #[must_use]
    pub fn pnl_nano_usd(&self, delta: Price, signed_qty: i64) -> NanoUsd {
        delta.as_nanos() * self.point_value_usd * signed_qty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_display_trims_zeros() {
        assert_eq!(Price::from_points_f64_lossy(4512.25).to_string(), "4512.25");
        assert_eq!(Price::from_points(5000).to_string(), "5000");
        assert_eq!(Price::from_points_f64_lossy(-0.5).to_string(), "-0.5");
    }

    #[test]
    fn price_roundtrip_nanos() {
        let p = Price::from_nanos(4_512_250_000_000);
        assert_eq!(p.as_nanos(), 4_512_250_000_000);
        assert_eq!(p.to_string(), "4512.25");
    }

    #[test]
    fn price_tick_rounding() {
        let tick = Price::from_points_f64_lossy(0.25);
        let p = Price::from_points_f64_lossy(4512.30);
        assert_eq!(p.round_to_tick(tick).to_string(), "4512.25");
        let q = Price::from_points_f64_lossy(4512.40);
        assert_eq!(q.round_to_tick(tick).to_string(), "4512.5");
    }

    #[test]
    fn timeframe_parse_and_display_roundtrip() {
        for s in ["1s", "1m", "5m", "15m", "1h", "1d"] {
            let tf: TimeFrame = s.parse().expect("valid timeframe");
            assert_eq!(tf.to_string(), s);
        }
        assert!("2m".parse::<TimeFrame>().is_err());
    }

    #[test]
    fn es_pnl_example() {
        // ES: $50/point. Long 2 contracts, +1.25 points => $125 = 125e9 nano.
        let spec = ContractSpec {
            instrument: InstrumentId::new("SYN:ES"),
            tick: Price::from_points_f64_lossy(0.25),
            point_value_usd: 50,
        };
        let delta = Price::from_points_f64_lossy(1.25);
        assert_eq!(spec.pnl_nano_usd(delta, 2), 125_000_000_000);
    }

    // Hand-derived: PRICE_SCALE is 1e9, so 0.25 points is 250_000_000
    // nanopoints and 5000 points is 5_000_000_000_000.
    #[test]
    fn prices_parse_from_text_exactly() {
        assert_eq!(
            Price::from_points_str("0.25"),
            Ok(Price::from_nanos(250_000_000))
        );
        assert_eq!(
            Price::from_points_str("5000"),
            Ok(Price::from_nanos(5_000_000_000_000))
        );
        assert_eq!(Price::from_points_str("0"), Ok(Price::ZERO));
        assert_eq!(
            Price::from_points_str(".5"),
            Ok(Price::from_nanos(500_000_000))
        );
        assert_eq!(
            Price::from_points_str("-1.5"),
            Ok(Price::from_nanos(-1_500_000_000))
        );
        assert_eq!(
            Price::from_points_str("  0.25  "),
            Ok(Price::from_nanos(250_000_000))
        );
        assert_eq!(
            Price::from_points_str("1_000.5"),
            Ok(Price::from_nanos(1_000_500_000_000))
        );
        assert_eq!(
            Price::from_points_str("0.000000001"),
            Ok(Price::from_nanos(1))
        );
    }

    // The reason this exists rather than routing through the lossy
    // constructor: 0.1 is not representable in binary, and a tick size one
    // nanopoint off snaps every fill in a backtest onto the wrong grid.
    #[test]
    fn text_parsing_beats_the_lossy_constructor_where_binary_cannot_help() {
        assert_eq!(
            Price::from_points_str("0.1"),
            Ok(Price::from_nanos(100_000_000))
        );
        assert_eq!(
            Price::from_points_str("6.05"),
            Ok(Price::from_nanos(6_050_000_000))
        );
    }

    #[test]
    fn malformed_prices_are_rejected() {
        for bad in [
            "",
            "abc",
            "1.2.3",
            "1e3",
            "1.2a",
            " . ",
            "--1",
            "0.0000000001",
        ] {
            assert!(
                Price::from_points_str(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn parsed_prices_round_trip_through_display() {
        for text in ["0.25", "5000", "1234.5", "0.000000001"] {
            let price = Price::from_points_str(text).expect("valid price");
            assert_eq!(
                Price::from_points_str(&price.to_string()),
                Ok(price),
                "{text} did not survive Display"
            );
        }
    }
}
