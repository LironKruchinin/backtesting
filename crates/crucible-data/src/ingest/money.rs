//! Exact money conversion at the vendor boundary.
//!
//! Databento quotes costs as `f64` dollars. CLAUDE.md §2.3 permits `f64` in
//! indicator space and statistics space only — a dollar figure that decides
//! whether we spend money is neither, so it is converted to [`NanoUsd`] the
//! moment it arrives and every comparison downstream is integer.
//!
//! Two rules make that conversion trustworthy:
//!
//! - **Round up, never down.** A quote we understate is a charge we did not
//!   consent to. Sub-nanodollar remainders round to a whole nanodollar.
//! - **Never route a decimal *string* through `f64`.** `"0.20"` parsed as
//!   `f64` is not 0.2, so `--max-cost-usd 0.20` compared against a quote of
//!   exactly $0.20 could refuse — or, worse, accept $0.2000000001.
//!   [`parse_usd_to_nano`] walks the digits itself.
//!
//! The `f64 → NanoUsd` direction ([`usd_f64_to_nano_ceil`]) has one subtlety
//! worth stating, because the naive implementation is wrong in a way that
//! looks right: `(1.2597_f64 * 1e9).ceil()` is 1_259_700_001, not
//! 1_259_700_000, because the product carries representation noise in its
//! last bits. Formatting to a fixed decimal first collapses that noise at a
//! precision far below anything a quote can express, and the ceiling is then
//! applied to an exact integer.

use crucible_core::types::NanoUsd;

/// Nanodollars in one US dollar.
pub const NANO_PER_USD: i64 = 1_000_000_000;

/// Picodollars per nanodollar — the intermediate precision
/// [`usd_f64_to_nano_ceil`] rounds through to strip `f64` noise.
const PICO_PER_NANO: i64 = 1_000;

/// The most fractional digits a `--max-cost-usd` value may carry: one
/// nanodollar. More than that is a typo, not a price, and is rejected rather
/// than silently truncated.
const MAX_FRACTIONAL_DIGITS: usize = 9;

/// Why a money value could not be converted exactly.
#[derive(Debug, Clone, PartialEq)]
pub enum MoneyError {
    /// The vendor returned `NaN` or an infinity where a price was expected.
    NotFinite,
    /// The vendor returned a negative price. We do not model refunds.
    Negative {
        /// The offending value.
        value: f64,
    },
    /// The value does not fit in `i64` nanodollars (beyond ~$9.22e9).
    OutOfRange {
        /// The offending value, rendered for the error message.
        value: String,
    },
    /// The text is not a plain decimal amount.
    Malformed {
        /// The text as supplied.
        text: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The text carries more precision than a nanodollar can represent.
    TooPrecise {
        /// The text as supplied.
        text: String,
        /// How many fractional digits were given.
        digits: usize,
    },
}

impl core::fmt::Display for MoneyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MoneyError::NotFinite => write!(f, "price is not a finite number"),
            MoneyError::Negative { value } => write!(f, "price is negative: {value}"),
            MoneyError::OutOfRange { value } => {
                write!(f, "price {value} does not fit in i64 nanodollars")
            }
            MoneyError::Malformed { text, reason } => {
                write!(f, "malformed amount {text:?}: {reason}")
            }
            MoneyError::TooPrecise { text, digits } => write!(
                f,
                "amount {text:?} has {digits} fractional digits; at most \
                 {MAX_FRACTIONAL_DIGITS} (one nanodollar) are representable"
            ),
        }
    }
}

impl std::error::Error for MoneyError {}

/// Converts a vendor-quoted dollar amount to nanodollars, rounding **up**.
///
/// Rounding up is deliberate: this figure is compared against a spending cap
/// the operator consented to, and an understated quote spends money that was
/// never authorised. A sub-nanodollar quote becomes one nanodollar rather
/// than zero, so "too small to represent" can never read as "free".
///
/// # Errors
/// [`MoneyError::NotFinite`], [`MoneyError::Negative`], or
/// [`MoneyError::OutOfRange`] if the vendor value cannot be represented.
pub fn usd_f64_to_nano_ceil(usd: f64) -> Result<NanoUsd, MoneyError> {
    if !usd.is_finite() {
        return Err(MoneyError::NotFinite);
    }
    if usd < 0.0 {
        return Err(MoneyError::Negative { value: usd });
    }
    // Render at picodollar precision first. This is three digits finer than
    // we keep, which is enough to make the decimal expansion of any quote
    // exact at nanodollar scale while discarding the `f64` noise that would
    // otherwise survive a direct multiply-and-ceil.
    let pico_text = format!("{usd:.12}");
    let pico = parse_fixed_point(&pico_text, 12).map_err(|_| MoneyError::OutOfRange {
        value: pico_text.clone(),
    })?;
    // Ceiling division: any picodollar remainder becomes a whole nanodollar.
    let nano = pico / PICO_PER_NANO;
    let remainder = pico % PICO_PER_NANO;
    if remainder > 0 {
        nano.checked_add(1).ok_or(MoneyError::OutOfRange {
            value: pico_text.clone(),
        })
    } else {
        Ok(nano)
    }
}

/// Parses a decimal dollar amount (`"0"`, `"0.20"`, `"1234.5678"`) into
/// nanodollars without ever constructing an `f64`.
///
/// Accepts an optional leading `$` and underscores as digit separators, both
/// of which people type. Rejects signs, exponents, and anything finer than a
/// nanodollar.
///
/// # Errors
/// [`MoneyError::Malformed`] for anything that is not a plain decimal amount,
/// [`MoneyError::TooPrecise`] beyond nine fractional digits, and
/// [`MoneyError::OutOfRange`] on overflow.
pub fn parse_usd_to_nano(text: &str) -> Result<NanoUsd, MoneyError> {
    let malformed = |reason: &'static str| MoneyError::Malformed {
        text: text.to_string(),
        reason,
    };

    let trimmed = text.trim();
    let body = trimmed.strip_prefix('$').unwrap_or(trimmed);
    let body: String = body.chars().filter(|c| *c != '_').collect();

    if body.is_empty() {
        return Err(malformed("empty"));
    }
    if body.starts_with('-') {
        return Err(malformed("negative amounts are not spending caps"));
    }
    if body.starts_with('+') {
        return Err(malformed("leading '+' is not accepted"));
    }
    if body.contains(['e', 'E']) {
        return Err(malformed("exponent notation is not accepted for money"));
    }

    let mut parts = body.split('.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(malformed("more than one decimal point"));
    }
    if int_part.is_empty() && frac_part.is_empty() {
        return Err(malformed("no digits"));
    }
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return Err(malformed("contains a non-digit"));
    }
    if frac_part.len() > MAX_FRACTIONAL_DIGITS {
        return Err(MoneyError::TooPrecise {
            text: text.to_string(),
            digits: frac_part.len(),
        });
    }

    let mut padded = String::with_capacity(int_part.len() + MAX_FRACTIONAL_DIGITS);
    padded.push_str(if int_part.is_empty() { "0" } else { int_part });
    padded.push_str(frac_part);
    for _ in frac_part.len()..MAX_FRACTIONAL_DIGITS {
        padded.push('0');
    }
    padded.parse::<i64>().map_err(|_| MoneyError::OutOfRange {
        value: text.to_string(),
    })
}

/// Parses a fixed-point decimal string with exactly `scale` fractional
/// digits into a scaled integer. Internal helper; the input always comes
/// from `format!("{:.scale}")`, so it is well-formed by construction.
fn parse_fixed_point(text: &str, scale: usize) -> Result<i64, ()> {
    let (int_part, frac_part) = text.split_once('.').ok_or(())?;
    if frac_part.len() != scale {
        return Err(());
    }
    let mut digits = String::with_capacity(int_part.len() + scale);
    digits.push_str(int_part);
    digits.push_str(frac_part);
    digits.parse::<i64>().map_err(|_| ())
}

/// Renders nanodollars as `$1234.5678`.
///
/// Four decimal places, always: quotes for a single day of bars run to
/// fractions of a cent, and a display that rounds them to `$0.00` would make
/// "cheap" and "free" indistinguishable at exactly the moment that matters.
/// The rendered value is for humans; the cost gate compares the underlying
/// integers.
#[must_use]
pub fn format_usd(nano: NanoUsd) -> String {
    let negative = nano < 0;
    let magnitude = nano.unsigned_abs();
    let whole = magnitude / (NANO_PER_USD as u64);
    // Truncate to four decimal places for display only.
    let frac = (magnitude % (NANO_PER_USD as u64)) / 100_000;
    let sign = if negative { "-" } else { "" };
    format!("{sign}${whole}.{frac:04}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-derived: 1.2597 USD = 1.2597 * 1e9 nano = 1_259_700_000 nano.
    // The naive `(1.2597_f64 * 1e9).ceil()` yields 1_259_700_001 because the
    // product is 1259700000.0000002...; this test is the regression guard.
    #[test]
    fn vendor_cost_converts_without_floating_point_noise() {
        assert_eq!(usd_f64_to_nano_ceil(1.2597), Ok(1_259_700_000));
        assert_eq!(usd_f64_to_nano_ceil(0.14), Ok(140_000_000));
        assert_eq!(usd_f64_to_nano_ceil(1377.17), Ok(1_377_170_000_000));
        assert_eq!(usd_f64_to_nano_ceil(0.0), Ok(0));
    }

    // A quote too small to express in nanodollars must not read as free.
    #[test]
    fn subnano_cost_rounds_up_to_one_nano() {
        assert_eq!(usd_f64_to_nano_ceil(1e-10), Ok(1));
        assert_eq!(usd_f64_to_nano_ceil(1e-12), Ok(1));
    }

    #[test]
    fn vendor_cost_rejects_unusable_values() {
        assert_eq!(usd_f64_to_nano_ceil(f64::NAN), Err(MoneyError::NotFinite));
        assert_eq!(
            usd_f64_to_nano_ceil(f64::INFINITY),
            Err(MoneyError::NotFinite)
        );
        assert!(matches!(
            usd_f64_to_nano_ceil(-0.01),
            Err(MoneyError::Negative { .. })
        ));
    }

    // Hand-derived: "0.20" is exactly 200_000_000 nano. Routing this through
    // f64 gives 0.200000000000000011..., which is the bug this guards.
    #[test]
    fn cap_text_parses_exactly() {
        assert_eq!(parse_usd_to_nano("0.20"), Ok(200_000_000));
        assert_eq!(parse_usd_to_nano("0"), Ok(0));
        assert_eq!(parse_usd_to_nano("0.00"), Ok(0));
        assert_eq!(parse_usd_to_nano("1234.5678"), Ok(1_234_567_800_000));
        assert_eq!(parse_usd_to_nano("199"), Ok(199_000_000_000));
        assert_eq!(parse_usd_to_nano(".5"), Ok(500_000_000));
        assert_eq!(parse_usd_to_nano("$0.20"), Ok(200_000_000));
        assert_eq!(parse_usd_to_nano("  0.20  "), Ok(200_000_000));
        assert_eq!(parse_usd_to_nano("1_000"), Ok(1_000_000_000_000));
        assert_eq!(parse_usd_to_nano("0.000000001"), Ok(1));
    }

    #[test]
    fn cap_text_rejects_junk() {
        for bad in ["", "abc", "1.2.3", "-1", "+1", "1e3", "$", "1.2a", " . "] {
            assert!(
                matches!(parse_usd_to_nano(bad), Err(MoneyError::Malformed { .. })),
                "expected {bad:?} to be rejected as malformed"
            );
        }
        assert!(matches!(
            parse_usd_to_nano("0.0000000001"),
            Err(MoneyError::TooPrecise { digits: 10, .. })
        ));
        assert!(matches!(
            parse_usd_to_nano("99999999999"),
            Err(MoneyError::OutOfRange { .. })
        ));
    }

    #[test]
    fn display_keeps_fractions_of_a_cent_visible() {
        assert_eq!(format_usd(140_000_000), "$0.1400");
        assert_eq!(format_usd(0), "$0.0000");
        assert_eq!(format_usd(1), "$0.0000"); // display truncates; the gate does not
        assert_eq!(format_usd(1_377_170_000_000), "$1377.1700");
        assert_eq!(format_usd(199_000_000_000), "$199.0000");
    }

    // The gate must be integer-exact even where the display collapses.
    #[test]
    fn one_nano_over_a_cap_is_not_equal_to_the_cap() {
        let cap = parse_usd_to_nano("0.20").expect("valid cap");
        assert_eq!(format_usd(cap), format_usd(cap + 1));
        assert!(cap + 1 > cap);
    }
}
