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
}

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
}
