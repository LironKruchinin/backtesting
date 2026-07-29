//! CME contract symbols, parsed far enough to be **ordered**.
//!
//! A roll table's whole job is to say which contract follows which, so the
//! only thing this module has to get right is the total order over
//! `(root, year, month)`. `ESH4 < ESM4 < ESU4 < ESZ4 < ESH5` — and the
//! sortedness of the *text* would get that wrong at every year boundary
//! (`ESZ4` sorts after `ESH5` alphabetically), which is why symbols are
//! parsed rather than compared as strings.
//!
//! ## The single-digit year problem
//!
//! CME writes the year as one digit (`ESH4`); Databento passes that spelling
//! through as `raw_symbol`. One digit cannot distinguish 2014 from 2024, and
//! this project's archive starts in 2010 — so the ambiguity is real, not
//! theoretical.
//!
//! The rule, pinned here and recorded in every roll table:
//!
//! - **Two digits are absolute**: `yy` means `2000 + yy`. Unambiguous, and
//!   correct for every contract in a 21st-century archive.
//! - **One digit resolves against a [`DecadeAnchor`]**: the year congruent to
//!   the digit modulo 10 that is *nearest* the anchor, ties broken toward the
//!   **earlier** year. [`DecadeAnchor::DEFAULT`] is a pinned constant
//!   ([`DEFAULT_ANCHOR_YEAR`]), never a clock — a symbol must parse to the
//!   same contract on every machine, forever (CLAUDE.md §2.2).
//!
//! A table built over contracts spanning a decade boundary must therefore say
//! which decade it means; the anchor it used is stored in the table so the
//! same bytes always reparse to the same contracts. [`ContractSymbol`]'s
//! `Display` renders the **two-digit** form, which is anchor-independent and
//! round-trips through [`ContractSymbol::parse`] regardless of anchor. The
//! archive's own spelling is not reconstructed from the symbol: it travels
//! beside it, in [`ContractSeries::instrument`](super::ContractSeries), because
//! that is what names a curated partition.

use core::cmp::Ordering;
use core::fmt;

use super::error::ContinuousError;

/// The year [`DecadeAnchor::DEFAULT`] resolves single digits against.
///
/// A constant, deliberately: reading a clock here would make `ESH4` mean a
/// different contract next decade, and every roll table already built would
/// quietly re-interpret itself.
pub const DEFAULT_ANCHOR_YEAR: i32 = 2025;

/// Reference year for resolving a one-digit contract year.
///
/// See the module docs for the rule. Carried in a [`RollTable`](super::RollTable)
/// so a stored table always reparses to the contracts it was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecadeAnchor(i32);

impl DecadeAnchor {
    /// The pinned default: [`DEFAULT_ANCHOR_YEAR`].
    pub const DEFAULT: DecadeAnchor = DecadeAnchor(DEFAULT_ANCHOR_YEAR);

    /// An anchor at `year`.
    #[must_use]
    pub const fn new(year: i32) -> DecadeAnchor {
        DecadeAnchor(year)
    }

    /// The reference year.
    #[must_use]
    pub const fn year(self) -> i32 {
        self.0
    }

    /// Resolves a one-digit year to the nearest year ending in that digit,
    /// ties to the earlier year.
    ///
    /// # Panics
    /// Debug-asserts `digit < 10`; callers parse the digit out of ASCII, so a
    /// larger value is a caller bug.
    #[must_use]
    pub fn resolve(self, digit: u32) -> i32 {
        debug_assert!(digit < 10, "INVARIANT: a year digit is one ASCII digit");
        let digit = i32::try_from(digit).unwrap_or(0);
        // The year in the anchor's own decade whose last digit matches, and
        // its neighbours one decade either side. Exactly one of the three is
        // nearest (or two tie, and the earlier wins).
        let in_decade = self.0 - self.0.rem_euclid(10) + digit;
        let mut best = in_decade;
        for candidate in [in_decade - 10, in_decade + 10] {
            let closer = (candidate - self.0).abs() < (best - self.0).abs();
            let ties_earlier =
                (candidate - self.0).abs() == (best - self.0).abs() && candidate < best;
            if closer || ties_earlier {
                best = candidate;
            }
        }
        best
    }
}

impl Default for DecadeAnchor {
    fn default() -> DecadeAnchor {
        DecadeAnchor::DEFAULT
    }
}

impl fmt::Display for DecadeAnchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A futures delivery-month code. Declaration order **is** calendar order, so
/// the derived `Ord` sorts January before December without a lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MonthCode {
    /// January.
    F,
    /// February.
    G,
    /// March.
    H,
    /// April.
    J,
    /// May.
    K,
    /// June.
    M,
    /// July.
    N,
    /// August.
    Q,
    /// September.
    U,
    /// October.
    V,
    /// November.
    X,
    /// December.
    Z,
}

impl MonthCode {
    /// Every code, in calendar order.
    pub const ALL: [MonthCode; 12] = [
        MonthCode::F,
        MonthCode::G,
        MonthCode::H,
        MonthCode::J,
        MonthCode::K,
        MonthCode::M,
        MonthCode::N,
        MonthCode::Q,
        MonthCode::U,
        MonthCode::V,
        MonthCode::X,
        MonthCode::Z,
    ];

    /// The letter the exchange writes.
    #[must_use]
    pub const fn letter(self) -> char {
        match self {
            MonthCode::F => 'F',
            MonthCode::G => 'G',
            MonthCode::H => 'H',
            MonthCode::J => 'J',
            MonthCode::K => 'K',
            MonthCode::M => 'M',
            MonthCode::N => 'N',
            MonthCode::Q => 'Q',
            MonthCode::U => 'U',
            MonthCode::V => 'V',
            MonthCode::X => 'X',
            MonthCode::Z => 'Z',
        }
    }

    /// Calendar month, 1–12.
    #[must_use]
    pub const fn month(self) -> u32 {
        match self {
            MonthCode::F => 1,
            MonthCode::G => 2,
            MonthCode::H => 3,
            MonthCode::J => 4,
            MonthCode::K => 5,
            MonthCode::M => 6,
            MonthCode::N => 7,
            MonthCode::Q => 8,
            MonthCode::U => 9,
            MonthCode::V => 10,
            MonthCode::X => 11,
            MonthCode::Z => 12,
        }
    }

    /// The code for a letter, or `None` for one that is not a month code.
    ///
    /// Deliberately case-sensitive: exchange symbology is uppercase, and
    /// accepting `esh4` would let two spellings of one contract into a
    /// [`BTreeMap`](std::collections::BTreeMap) keyed by symbol.
    #[must_use]
    pub const fn from_letter(letter: char) -> Option<MonthCode> {
        match letter {
            'F' => Some(MonthCode::F),
            'G' => Some(MonthCode::G),
            'H' => Some(MonthCode::H),
            'J' => Some(MonthCode::J),
            'K' => Some(MonthCode::K),
            'M' => Some(MonthCode::M),
            'N' => Some(MonthCode::N),
            'Q' => Some(MonthCode::Q),
            'U' => Some(MonthCode::U),
            'V' => Some(MonthCode::V),
            'X' => Some(MonthCode::X),
            'Z' => Some(MonthCode::Z),
            _ => None,
        }
    }
}

impl fmt::Display for MonthCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.letter())
    }
}

/// An outright futures contract, identified well enough to be ordered.
///
/// Equality and ordering are on `(root, year, month)` only — the two archive
/// spellings of one contract (`ESH4` and `ESH24`) are the same contract and
/// must collapse to one key in any map.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractSymbol {
    root: String,
    year: i32,
    month: MonthCode,
}

impl ContractSymbol {
    /// Builds a symbol from its parts.
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`] if `root` is empty or is not
    /// uppercase ASCII alphanumeric — the character set every CME product code
    /// in this archive uses, and the one that survives a path component
    /// unescaped.
    pub fn new(root: &str, year: i32, month: MonthCode) -> Result<ContractSymbol, ContinuousError> {
        validate_root(root)?;
        Ok(ContractSymbol {
            root: root.to_owned(),
            year,
            month,
        })
    }

    /// Parses an exchange symbol using [`DecadeAnchor::DEFAULT`].
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`] for anything that is not
    /// `ROOT + month letter + one or two year digits` — including calendar
    /// spreads (`ESH4-ESM4`), which are real instruments but are never the
    /// front contract of a continuous series.
    pub fn parse(text: &str) -> Result<ContractSymbol, ContinuousError> {
        ContractSymbol::parse_with_anchor(text, DecadeAnchor::DEFAULT)
    }

    /// Parses an exchange symbol, resolving a one-digit year against `anchor`.
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`]; see [`ContractSymbol::parse`].
    pub fn parse_with_anchor(
        text: &str,
        anchor: DecadeAnchor,
    ) -> Result<ContractSymbol, ContinuousError> {
        let bad = |reason: &'static str| ContinuousError::UnparseableSymbol {
            symbol: text.to_owned(),
            reason,
        };
        if !text.is_ascii() {
            return Err(bad("must be ASCII"));
        }
        let bytes = text.as_bytes();
        let digits = bytes
            .iter()
            .rev()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 {
            return Err(bad("has no year digits"));
        }
        if digits > 2 {
            return Err(bad(
                "has more than two trailing digits, so its year is not a CME year code",
            ));
        }
        // With the digits removed, the last character must be the month code
        // and everything before it the product root.
        let head = &text[..text.len() - digits];
        let letter = head.chars().next_back().ok_or_else(|| bad("has no root"))?;
        let month = MonthCode::from_letter(letter).ok_or_else(|| {
            bad("does not carry an uppercase month code (F G H J K M N Q U V X Z)")
        })?;
        let root = &head[..head.len() - letter.len_utf8()];
        validate_root(root).map_err(|_| {
            bad("has a product root that is not uppercase ASCII alphanumeric (a calendar spread is not an outright)")
        })?;

        let value: u32 = text[text.len() - digits..]
            .parse()
            .map_err(|_| bad("has an unparseable year"))?;
        let year = if digits == 2 {
            // Two digits are absolute in a 21st-century archive; see the
            // module docs.
            2000 + i32::try_from(value).unwrap_or(0)
        } else {
            anchor.resolve(value)
        };
        Ok(ContractSymbol {
            root: root.to_owned(),
            year,
            month,
        })
    }

    /// Product root, e.g. `ES`.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Delivery year, absolute.
    #[must_use]
    pub const fn year(&self) -> i32 {
        self.year
    }

    /// Delivery month code.
    #[must_use]
    pub const fn month(&self) -> MonthCode {
        self.month
    }

    /// Orders two contracts of the same root by delivery, or `None` when the
    /// roots differ (unrelated markets have no successor relationship).
    #[must_use]
    pub fn delivery_cmp(&self, other: &ContractSymbol) -> Option<Ordering> {
        if self.root != other.root {
            return None;
        }
        Some((self.year, self.month).cmp(&(other.year, other.month)))
    }
}

impl fmt::Display for ContractSymbol {
    /// The **two-digit** spelling (`ESH24`), which is anchor-independent and
    /// reparses to the same contract with any anchor. The archive's own
    /// spelling is not reconstructed here — see the module docs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{:02}",
            self.root,
            self.month.letter(),
            self.year.rem_euclid(100)
        )
    }
}

/// Roots are uppercase ASCII alphanumeric (`ES`, `NQ`, `6E`, `ZN`).
fn validate_root(root: &str) -> Result<(), ContinuousError> {
    let bad = |reason: &'static str| ContinuousError::UnparseableSymbol {
        symbol: root.to_owned(),
        reason,
    };
    if root.is_empty() {
        return Err(bad("has an empty product root"));
    }
    if !root
        .bytes()
        .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
    {
        return Err(bad(
            "has a product root that is not uppercase ASCII alphanumeric",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(text: &str) -> ContractSymbol {
        ContractSymbol::parse(text).expect("a valid contract symbol")
    }

    // Hand-derived: ES + March (H) + 2024. The default anchor is 2025, and
    // the year congruent to 4 nearest 2025 is 2024 (distance 1) against 2014
    // (11) and 2034 (9).
    #[test]
    fn one_digit_years_resolve_against_the_default_anchor() {
        let s = sym("ESH4");
        assert_eq!(s.root(), "ES");
        assert_eq!(s.month(), MonthCode::H);
        assert_eq!(s.year(), 2024);
    }

    // Two digits are absolute: 24 is 2024 whatever the anchor says.
    #[test]
    fn two_digit_years_ignore_the_anchor() {
        for anchor in [1999, 2015, 2025, 2099] {
            let s = ContractSymbol::parse_with_anchor("ESH24", DecadeAnchor::new(anchor))
                .expect("valid");
            assert_eq!(s.year(), 2024, "anchor {anchor}");
        }
        assert_eq!(sym("ESH4"), sym("ESH24"), "one contract, two spellings");
    }

    // Nearest-with-ties-to-earlier, worked by hand for anchor 2025:
    //   digit 0 -> 2020 (5) vs 2030 (5) -> tie -> earlier -> 2020
    //   digit 9 -> 2029 (4) vs 2019 (6) -> 2029
    //   digit 5 -> 2025 (0)
    // and for anchor 2016:
    //   digit 1 -> 2011 (5) vs 2021 (5) -> tie -> earlier -> 2011
    //   digit 0 -> 2020 (4) vs 2010 (6) -> 2020
    #[test]
    fn the_decade_anchor_rule_is_nearest_ties_earlier() {
        let d = DecadeAnchor::new(2025);
        assert_eq!(d.resolve(0), 2020);
        assert_eq!(d.resolve(9), 2029);
        assert_eq!(d.resolve(5), 2025);
        assert_eq!(d.resolve(4), 2024);
        let e = DecadeAnchor::new(2016);
        assert_eq!(e.resolve(1), 2011);
        assert_eq!(e.resolve(0), 2020);
        assert_eq!(e.resolve(6), 2016);
    }

    // Roots that are not two letters exist and must parse: 6E is the euro FX
    // future, ZN the 10-year note.
    #[test]
    fn roots_with_digits_and_other_lengths_parse() {
        assert_eq!(sym("6EU6").root(), "6E");
        assert_eq!(sym("6EU6").month(), MonthCode::U);
        assert_eq!(sym("ZNZ25").root(), "ZN");
        assert_eq!(sym("ZNZ25").year(), 2025);
        assert_eq!(sym("CLF6").root(), "CL");
    }

    // The whole reason this module exists: text order is wrong at every year
    // boundary. ESZ4 sorts after ESH5 alphabetically and before it in time.
    #[test]
    fn contracts_order_by_delivery_not_by_text() {
        let mut symbols = [sym("ESH5"), sym("ESZ4"), sym("ESM4"), sym("ESH4")];
        symbols.sort();
        let rendered: Vec<String> = symbols.iter().map(ToString::to_string).collect();
        assert_eq!(rendered, vec!["ESH24", "ESM24", "ESZ24", "ESH25"]);
        assert!("ESZ4" > "ESH5", "text order really is the wrong order");
    }

    #[test]
    fn roots_group_before_deliveries_so_a_map_partitions_by_product() {
        let mut symbols = [sym("NQH4"), sym("ESZ4"), sym("ESH4")];
        symbols.sort();
        let rendered: Vec<String> = symbols.iter().map(ToString::to_string).collect();
        assert_eq!(rendered, vec!["ESH24", "ESZ24", "NQH24"]);
        assert_eq!(sym("ESH4").delivery_cmp(&sym("NQH4")), None);
        assert_eq!(sym("ESH4").delivery_cmp(&sym("ESM4")), Some(Ordering::Less));
    }

    // A calendar spread is a real instrument with real curated bars, and it is
    // never a front contract. Refusing it here is what keeps it out of a roll
    // chain (D-0033 keeps it in the archive).
    #[test]
    fn spreads_and_malformed_symbols_are_refused() {
        for bad in [
            "ESH4-ESM4",
            "ES.FUT",
            "ES",
            "ESH",
            "H4",
            "ESh4",
            "ESI4",   // I is not a month code
            "ESH123", // three year digits
            "ES:H4",
            "",
            "ESH\u{00fc}4",
        ] {
            assert!(
                ContractSymbol::parse(bad).is_err(),
                "{bad:?} must not parse as an outright contract"
            );
        }
    }

    #[test]
    fn display_round_trips_through_parse() {
        for text in ["ESH4", "ESH24", "6EU6", "ZNZ25", "CLF6"] {
            let parsed = sym(text);
            assert_eq!(sym(&parsed.to_string()), parsed, "{text}");
        }
    }
}
