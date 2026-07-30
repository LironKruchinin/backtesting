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
//! theoretical. It is not a hypothetical either: the 16-year `GC.FUT ohlcv-1m`
//! window contains bars for **both** `GCZ4`s, ten years apart (D-0072).
//!
//! The rule, pinned here and recorded in every roll table:
//!
//! - **Four digits are absolute and unambiguous**: `yyyy` means exactly that
//!   year. This is the spelling this project *writes* — see below.
//! - **Two digits are absolute**: `yy` means `2000 + yy`. Correct for every
//!   contract in a 21st-century archive, and the spelling the vendor itself
//!   falls back to for far-dated listings (`CLZ36`).
//! - **One digit resolves against a [`DecadeAnchor`]**: the year congruent to
//!   the digit modulo 10 that is *nearest* the anchor, ties broken toward the
//!   **earlier** year. [`DecadeAnchor::DEFAULT`] is a pinned constant
//!   ([`DEFAULT_ANCHOR_YEAR`]), never a clock — a symbol must parse to the
//!   same contract on every machine, forever (CLAUDE.md §2.2).
//!
//! **A one-digit year is never resolved by an anchor where a record can say
//! better.** The anchor is a constant, and a constant cannot separate two
//! contracts that a single file contains; only the record can
//! ([`expiry`](super::expiry), D-0046, D-0072). The anchor remains the
//! fallback for the one place with nothing to consult — a bare symbol with no
//! timestamp beside it.
//!
//! ## The canonical spelling this project writes
//!
//! [`ContractSymbol`]'s `Display` renders the **four-digit** form (`GCZ2014`),
//! and that is what names a curated partition (D-0072). Two reasons, and the
//! second is the load-bearing one:
//!
//! 1. It is absolute: no anchor, no decade, no era. It reparses to the same
//!    contract on any machine and in any century.
//! 2. It **cannot be confused with a vendor spelling**. A two-digit form would
//!    be unambiguous arithmetically and still ambiguous to a reader: `GCZ14` is
//!    one character from the vendor's `GCZ4`, and `CLZ36` is a real vendor
//!    spelling that means 2036. A directory listing mixing our keys with the
//!    archive's would depend on knowing which convention wrote each name. Four
//!    digits can never collide with a CME year code, which has at most two.
//!
//! [`ContractSymbol::parse`] accepts all three spellings, so the canonical form
//! round-trips and every archive spelling still parses.

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

/// The earliest year a four-digit spelling may name.
///
/// Not a style rule: it is what stops [`ContractSymbol::parse`] from reading
/// four trailing digits out of a string that is not a contract at all. `ZN1234`
/// would otherwise parse as root `Z`, month `N` (July), year 1234. Every
/// timestamp in this project is nanoseconds since the Unix epoch, so a contract
/// that expired before 1970 could not be represented anywhere downstream.
const EARLIEST_FOUR_DIGIT_YEAR: i32 = 1970;

/// An outright contract symbol split into its written parts, **before** the
/// year is resolved.
///
/// This is the one place the text `ROOT + month letter + year digits` is taken
/// apart, so every consumer that has to reason about the *spelling* — the year
/// resolver in [`expiry`](super::expiry), the curated-layout check, and
/// [`ContractSymbol::parse_with_anchor`] itself — agrees by construction rather
/// than by three regexes that happen to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolParts<'a> {
    /// Product root, e.g. `ES`.
    pub root: &'a str,
    /// Delivery month.
    pub month: MonthCode,
    /// How many digits the year was written with: 1, 2, or 4.
    pub year_digits: usize,
    /// The digits' numeric value, *unresolved*: `4`, `24`, or `2024`.
    pub year_value: u32,
}

impl SymbolParts<'_> {
    /// Whether the year was written with a single digit — the spelling that
    /// cannot say which decade it means.
    ///
    /// The predicate a curated partition key must answer `false` to (D-0072).
    #[must_use]
    pub const fn has_ambiguous_year(&self) -> bool {
        self.year_digits == 1
    }
}

/// Splits an outright contract symbol into [`SymbolParts`].
///
/// # Errors
/// [`ContinuousError::UnparseableSymbol`] for anything that is not
/// `ROOT + month letter + 1, 2, or 4 year digits` — including calendar spreads
/// (`ESH4-ESM4`), which are real instruments but are never the front contract
/// of a continuous series.
pub fn parse_parts(text: &str) -> Result<SymbolParts<'_>, ContinuousError> {
    let bad = |reason: &'static str| ContinuousError::UnparseableSymbol {
        symbol: text.to_owned(),
        reason,
    };
    if !text.is_ascii() {
        return Err(bad("must be ASCII"));
    }
    let bytes = text.as_bytes();
    let year_digits = bytes
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if year_digits == 0 {
        return Err(bad("has no year digits"));
    }
    // 1 and 2 are CME's spellings; 4 is this project's canonical one. 3 is
    // nobody's, and more than 4 is a serial number, not a year.
    if !matches!(year_digits, 1 | 2 | 4) {
        return Err(bad(
            "has a trailing digit count that is not a year: CME writes 1 or 2, \
             this project writes 4",
        ));
    }
    // With the digits removed, the last character must be the month code and
    // everything before it the product root.
    let head = &text[..text.len() - year_digits];
    let letter = head.chars().next_back().ok_or_else(|| bad("has no root"))?;
    let month = MonthCode::from_letter(letter)
        .ok_or_else(|| bad("does not carry an uppercase month code (F G H J K M N Q U V X Z)"))?;
    let root = &head[..head.len() - letter.len_utf8()];
    validate_root(root).map_err(|_| {
        bad("has a product root that is not uppercase ASCII alphanumeric (a calendar spread is not an outright)")
    })?;
    let year_value: u32 = text[text.len() - year_digits..]
        .parse()
        .map_err(|_| bad("has an unparseable year"))?;
    if year_digits == 4 && i64::from(year_value) < i64::from(EARLIEST_FOUR_DIGIT_YEAR) {
        return Err(bad(
            "spells four year digits that are not a year a futures contract \
             could carry (before 1970, the epoch every timestamp here counts from)",
        ));
    }
    Ok(SymbolParts {
        root,
        month,
        year_digits,
        year_value,
    })
}

/// An outright futures contract, identified well enough to be ordered.
///
/// Equality and ordering are on `(root, year, month)` only — the three
/// spellings of one contract (`ESH4`, `ESH24`, `ESH2024`) are the same contract
/// and must collapse to one key in any map.
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

    /// Parses a contract symbol using [`DecadeAnchor::DEFAULT`].
    ///
    /// Accepts all three spellings (`ESH4`, `ESH24`, `ESH2024`); only the
    /// one-digit form consults the anchor.
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`] for anything that is not
    /// `ROOT + month letter + 1, 2, or 4 year digits` — including calendar
    /// spreads (`ESH4-ESM4`), which are real instruments but are never the
    /// front contract of a continuous series.
    pub fn parse(text: &str) -> Result<ContractSymbol, ContinuousError> {
        ContractSymbol::parse_with_anchor(text, DecadeAnchor::DEFAULT)
    }

    /// Parses a contract symbol, resolving a one-digit year against `anchor`.
    ///
    /// Two- and four-digit years are absolute and ignore `anchor` entirely.
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`]; see [`ContractSymbol::parse`].
    pub fn parse_with_anchor(
        text: &str,
        anchor: DecadeAnchor,
    ) -> Result<ContractSymbol, ContinuousError> {
        let parts = parse_parts(text)?;
        Ok(ContractSymbol {
            root: parts.root.to_owned(),
            year: resolve_year(&parts, anchor),
            month: parts.month,
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

/// Turns written year digits into an absolute year.
///
/// Four and two digits are absolute; only one consults the anchor.
fn resolve_year(parts: &SymbolParts<'_>, anchor: DecadeAnchor) -> i32 {
    match parts.year_digits {
        // Absolute, and validated by `parse_parts` to be a year at all.
        4 => i32::try_from(parts.year_value).unwrap_or(EARLIEST_FOUR_DIGIT_YEAR),
        // Two digits are absolute in a 21st-century archive; see the module docs.
        2 => 2000 + i32::try_from(parts.year_value).unwrap_or(0),
        _ => anchor.resolve(parts.year_value),
    }
}

impl fmt::Display for ContractSymbol {
    /// The **four-digit** canonical spelling (`GCZ2014`) — absolute, anchor-free,
    /// and impossible to confuse with a CME year code, which has at most two
    /// digits. This is what names a curated partition (D-0072); the module docs
    /// argue why. It round-trips through [`ContractSymbol::parse`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{:04}", self.root, self.month.letter(), self.year)
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

    // Two and four digits are absolute: both mean 2024 whatever the anchor
    // says, and all three spellings are the same contract.
    #[test]
    fn two_and_four_digit_years_ignore_the_anchor() {
        for anchor in [1999, 2015, 2025, 2099] {
            for text in ["ESH24", "ESH2024"] {
                let s = ContractSymbol::parse_with_anchor(text, DecadeAnchor::new(anchor))
                    .expect("valid");
                assert_eq!(s.year(), 2024, "{text} at anchor {anchor}");
            }
        }
        assert_eq!(sym("ESH4"), sym("ESH24"), "one contract, two spellings");
        assert_eq!(sym("ESH24"), sym("ESH2024"), "and a third");
    }

    // The canonical spelling is four digits, and it is what `Display` writes —
    // so it is what names a curated partition (D-0072). A two-digit key would
    // be arithmetically unambiguous and still one character from the vendor's
    // `GCZ4`; four digits cannot collide with a CME year code at all.
    #[test]
    fn display_writes_the_four_digit_canonical_form() {
        assert_eq!(sym("GCZ4").to_string(), "GCZ2024");
        assert_eq!(
            ContractSymbol::parse_with_anchor("GCZ4", DecadeAnchor::new(2015))
                .expect("valid")
                .to_string(),
            "GCZ2014",
            "the two GCZ4s of a 16-year window must render differently"
        );
        assert_eq!(sym("CLZ36").to_string(), "CLZ2036");
        assert_eq!(sym("6EU6").to_string(), "6EU2026");
    }

    // The regression control for D-0072, at the level of the key itself: the
    // partition key of a 2014 contract and of a 2024 contract must not be the
    // same string. Under the spelling this replaces, both were `GCZ4`.
    #[test]
    fn two_contracts_ten_years_apart_never_share_a_partition_key() {
        let old = ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid");
        let new = ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid");
        assert_ne!(old, new);
        assert_ne!(
            old.to_string(),
            new.to_string(),
            "one key for two contracts is the bug D-0072 fixed"
        );
        assert_eq!(old.to_string(), "GCZ2014");
        assert_eq!(new.to_string(), "GCZ2024");
    }

    // The spelling predicate the curated-layout check reads. A key that spells
    // its year with one digit is a key that cannot say which decade it means.
    #[test]
    fn a_one_digit_year_is_the_only_ambiguous_spelling() {
        assert!(parse_parts("GCZ4").expect("valid").has_ambiguous_year());
        assert!(!parse_parts("GCZ14").expect("valid").has_ambiguous_year());
        assert!(!parse_parts("GCZ2014").expect("valid").has_ambiguous_year());
        // Not contracts at all, so not a curated-key question.
        for other in ["SYN:RW", "ES.v.0", "ESH4-ESM4", "ZN", ""] {
            assert!(parse_parts(other).is_err(), "{other}");
        }
    }

    // Widening `parse` to four digits must not let it read a year out of a
    // string that is not a contract: `ZN1234` would otherwise be root Z,
    // month N, year 1234.
    #[test]
    fn four_trailing_digits_must_still_be_a_plausible_year() {
        assert!(ContractSymbol::parse("ZN1234").is_err());
        assert!(ContractSymbol::parse("ESH1969").is_err());
        assert_eq!(sym("ESH1970").year(), 1970);
        // Three is nobody's spelling, and five is a serial number.
        assert!(ContractSymbol::parse("ESH123").is_err());
        assert!(ContractSymbol::parse("ESH20244").is_err());
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
        assert_eq!(rendered, vec!["ESH2024", "ESM2024", "ESZ2024", "ESH2025"]);
        assert!("ESZ4" > "ESH5", "text order really is the wrong order");
    }

    #[test]
    fn roots_group_before_deliveries_so_a_map_partitions_by_product() {
        let mut symbols = [sym("NQH4"), sym("ESZ4"), sym("ESH4")];
        symbols.sort();
        let rendered: Vec<String> = symbols.iter().map(ToString::to_string).collect();
        assert_eq!(rendered, vec!["ESH2024", "ESZ2024", "NQH2024"]);
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
            "ESH123", // three year digits: nobody's spelling
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
        for text in ["ESH4", "ESH24", "6EU6", "ZNZ25", "CLF6", "GCZ2014"] {
            let parsed = sym(text);
            assert_eq!(sym(&parsed.to_string()), parsed, "{text}");
            // And round-trips under *any* anchor, which is the point of the
            // canonical form: it carries no decade question to answer.
            for anchor in [1999, 2015, 2025, 2099] {
                assert_eq!(
                    ContractSymbol::parse_with_anchor(
                        &parsed.to_string(),
                        DecadeAnchor::new(anchor)
                    )
                    .expect("canonical parses"),
                    parsed,
                    "{text} at anchor {anchor}"
                );
            }
        }
    }
}
