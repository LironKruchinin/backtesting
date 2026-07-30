//! Where a contract's expiry comes from.
//!
//! Expiries are an **input** to the roll logic, never something it computes,
//! for the reason CLAUDE.md §2.1 gives for every new data source: "as known
//! when?" has to be answerable. A vendor `definition` record answers it (the
//! exchange published the expiry when it listed the contract, long before
//! any roll decided from it); a formula does not.
//!
//! So there are two sources, and a roll table records which one it used:
//!
//! - [`expiries_from_definitions`] (behind the `databento` feature) reads the
//!   `definition` schema out of the archive. This is the real one.
//! - [`nominal_expiry`] computes the **third Friday of the contract month**,
//!   at UTC midnight. It is a fallback and is wrong in two known ways: it is
//!   a date, not the 09:30-Central settlement instant, and third-Friday is
//!   the equity-index convention (ES, NQ, RTY, YM) — CL, ZN, and 6E all
//!   expire on different rules. It exists so that
//!   [`RollRule::CalendarDaysBeforeExpiry`](super::RollRule::CalendarDaysBeforeExpiry)
//!   is testable without vendor data, and so a volume-crossover table can be
//!   built on a machine that has no `definition` files. A table built from it
//!   says `expiry_source = "nominal-third-friday"`, which is the whole point
//!   of recording the field.
//!
//! The core roll logic in [`roll`](super::roll) never mentions either: it
//! takes a `BTreeMap<ContractSymbol, Ts>` and does not care where the map
//! came from. That is what keeps it unit-testable with synthetic fixtures and
//! free of the `databento` feature.
//!
//! ## The second consumer: naming a curated partition (D-0072)
//!
//! Expiries answer a question beyond "when must I roll?". They answer **"which
//! contract is this?"** — and `transcode` needs that answer for every bar it
//! files, because a one-digit CME year code repeats every ten years and every
//! bar window in this archive is sixteen years long. `GCZ4` names the December
//! 2014 gold contract *and* the December 2024 one, and both traded inside one
//! raw file.
//!
//! [`ContractCycles`] is that answer, and it is deliberately the *same* device
//! [`expiries_from_definitions`] already uses to separate `ESM0` from `ESM0`:
//! resolve a one-digit year against the contract's own expiry rather than
//! against a constant. Two consumers, one rule.
//!
//! ### Why the two readers disagree about what a conflict is
//!
//! [`expiries_from_definitions`] refuses a contract defined twice with
//! *different* expiries (D-0046): every roll decided from it turns on which is
//! true. [`contract_cycles_from_definitions`] does not, because it is asking a
//! coarser question — the archive's real `GC.FUT` definition file gives `GCX1`
//! two expiries **one hour apart**, and `6EM23` two **seventy-two hours**
//! apart. An hour does not make `GCX1` a different contract; it would make a
//! calendar roll fire on a different session. So the cycle reader keeps the
//! observed *span* and refuses only [`ContinuousError::ExpiryYearConflict`] —
//! two expiries in different years, which is the case where the identity itself
//! becomes unknowable. Measured on the archive: zero contracts across all seven
//! roots have that, so nothing is being waved through.
//!
//! ### The window rule, and the evidence for it
//!
//! A contract owns the stretch of time ending at its expiry and beginning at
//! the expiry of the previous contract in the same *family* — same root, same
//! month code, same year digit — which is exactly ten years earlier. Where the
//! family has no earlier member, the window opens
//! [`CONTRACT_CYCLE_DAYS`] before the expiry.
//!
//! The windows therefore tile the timeline without gaps or overlaps, so the
//! answer is unique when it exists. It exists for **every outright bar in the
//! archive**: decoding all seven roots' `ohlcv-1m` windows against their
//! `definition` files leaves zero unresolved records, while showing the defect
//! in the open — 101 of GC's 120 raw symbols, 111 of CL's, and 28 of ZN's have
//! a first and a last bar belonging to *different* contracts.

use crucible_core::types::Ts;

use crate::ingest::window::{CivilDate, NANOS_PER_DAY, date_of, days_from_civil};

use super::error::ContinuousError;
use super::symbol::{ContractSymbol, MonthCode, parse_parts};

/// Name recorded in a [`RollTable`](super::RollTable) built with
/// [`nominal_expiry`].
pub const NOMINAL_EXPIRY_SOURCE: &str = "nominal-third-friday";

/// Name recorded when no expiries were needed at all — a volume-crossover
/// table never asks for one.
pub const NO_EXPIRY_SOURCE: &str = "none";

/// UTC midnight of the third Friday of a contract's delivery month.
///
/// A **fallback**, with the caveats in the module docs. Never a substitute
/// for the `definition` schema.
#[must_use]
pub fn nominal_expiry(symbol: &ContractSymbol) -> Ts {
    let first = CivilDate {
        year: i64::from(symbol.year()),
        month: symbol.month().month(),
        day: 1,
    };
    let first_day = days_from_civil(first);
    // 1970-01-01 is a Thursday, so `(epoch_day + 4) % 7` numbers the week
    // from Sunday = 0, making Friday 5.
    let weekday = (first_day + 4).rem_euclid(7);
    let to_first_friday = (5 - weekday).rem_euclid(7);
    Ts((first_day + to_first_friday + 14) * NANOS_PER_DAY)
}

/// Nominal expiries for every symbol given.
#[must_use]
pub fn nominal_expiries<'a, I>(symbols: I) -> std::collections::BTreeMap<ContractSymbol, Ts>
where
    I: IntoIterator<Item = &'a ContractSymbol>,
{
    symbols
        .into_iter()
        .map(|s| (s.clone(), nominal_expiry(s)))
        .collect()
}

/// How far back the *first* contract of a family may reach.
///
/// A one-digit CME year code repeats every ten years, so two contracts sharing
/// a family are exactly ten calendar years apart — 3,652 or 3,653 days,
/// depending on how many leap days the decade holds. 3,653 is the larger.
///
/// This constant is only ever the lower bound of a family's **earliest**
/// contract: every later one opens at its predecessor's expiry instead, so the
/// windows tile exactly however far apart two real expiries turn out to be
/// (measured across this archive: 3,647 to 3,657 days, since the expiry *date*
/// drifts within its month). Where it does apply, precision is irrelevant — the
/// bars it is there to catch are a whole cycle out, not a week — and no listed
/// future reaches ten years past its own listing: the longest at CME is crude
/// oil at nine.
pub const CONTRACT_CYCLE_DAYS: i64 = 3_653;

/// One contract, and the stretch of time bars may carry its vendor spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cycle {
    /// The contract, with an absolute year.
    symbol: ContractSymbol,
    /// Earliest expiry the definition file recorded for it.
    expiry_earliest: Ts,
    /// Latest expiry the definition file recorded for it. Equal to
    /// `expiry_earliest` unless the vendor restated the contract with a
    /// slightly different settlement instant, which it does.
    expiry_latest: Ts,
}

/// Which contract a vendor symbol names at a given instant.
///
/// Built from a root's archived `definition` records. The module docs carry the
/// window rule and the evidence; the short version is that a one-digit year is
/// resolved against the contract's **own expiry**, never against a
/// [`DecadeAnchor`](super::DecadeAnchor) constant, because a constant cannot
/// separate two contracts that one file contains (D-0046, D-0072).
#[derive(Debug, Clone, Default)]
pub struct ContractCycles {
    /// Keyed by (month, year digit) — the family a one-digit spelling lands in.
    /// Values are sorted by expiry, which is what makes the windows tile.
    families: std::collections::BTreeMap<(MonthCode, u32), Vec<Cycle>>,
}

impl ContractCycles {
    /// Builds a resolver from expiry spans: `(contract, earliest, latest)`.
    ///
    /// # Errors
    /// [`ContinuousError::ExpiryYearConflict`] if any contract's two extreme
    /// expiries fall in different calendar years — the case where the year that
    /// resolves the code is itself unknown.
    pub fn from_spans<I>(spans: I) -> Result<ContractCycles, ContinuousError>
    where
        I: IntoIterator<Item = (ContractSymbol, Ts, Ts)>,
    {
        let mut families: std::collections::BTreeMap<(MonthCode, u32), Vec<Cycle>> =
            std::collections::BTreeMap::new();
        for (symbol, earliest, latest) in spans {
            let (lo, hi) = if earliest <= latest {
                (earliest, latest)
            } else {
                (latest, earliest)
            };
            if date_of_year(lo) != date_of_year(hi) {
                return Err(ContinuousError::ExpiryYearConflict {
                    contract: symbol.to_string(),
                    earliest: lo,
                    latest: hi,
                });
            }
            let digit = u32::try_from(symbol.year().rem_euclid(10)).unwrap_or(0);
            families
                .entry((symbol.month(), digit))
                .or_default()
                .push(Cycle {
                    symbol,
                    expiry_earliest: lo,
                    expiry_latest: hi,
                });
        }
        for cycles in families.values_mut() {
            cycles.sort_by(|a, b| {
                (a.expiry_earliest, &a.symbol).cmp(&(b.expiry_earliest, &b.symbol))
            });
            // Members of a family are, by the arithmetic of a one-digit year
            // code, about ten years apart. Two that are not mean the definition
            // file put at least one of them in the wrong decade — and the
            // windows below would then be narrow slivers rather than cycles,
            // so a bar would be filed by rounding. Half a cycle is the
            // threshold because nothing legitimate comes close to it: the
            // narrowest real gap this archive contains is 3,647 days.
            for pair in cycles.windows(2) {
                let apart = pair[1].expiry_earliest.0 - pair[0].expiry_latest.0;
                if apart < CONTRACT_CYCLE_DAYS / 2 * NANOS_PER_DAY {
                    return Err(ContinuousError::ContractCycleCollision {
                        first: pair[0].symbol.to_string(),
                        second: pair[1].symbol.to_string(),
                        apart_days: apart / NANOS_PER_DAY,
                    });
                }
            }
        }
        Ok(ContractCycles { families })
    }

    /// Builds a resolver from a plain expiry map, as
    /// [`nominal_expiries`] produces.
    ///
    /// # Errors
    /// [`ContinuousError::ExpiryYearConflict`], which a single-valued map
    /// cannot trigger — the signature matches [`ContractCycles::from_spans`] so
    /// callers do not branch.
    pub fn from_expiries(
        expiries: &std::collections::BTreeMap<ContractSymbol, Ts>,
    ) -> Result<ContractCycles, ContinuousError> {
        ContractCycles::from_spans(expiries.iter().map(|(s, ts)| (s.clone(), *ts, *ts)))
    }

    /// Whether any contract is known at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    /// How many contracts the resolver knows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.families.values().map(Vec::len).sum()
    }

    /// Resolves a vendor symbol observed at `ts` into an unambiguous contract.
    ///
    /// A two- or four-digit year is absolute, so it is returned without
    /// consulting anything: no expiry is needed, and demanding one would refuse
    /// far-dated listings the vendor itself spells `CLZ36` precisely to avoid
    /// the ambiguity. Only a one-digit year is resolved against the archive.
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`] if `symbol` is not an outright
    /// contract, and [`ContinuousError::UnresolvedContract`] if a one-digit
    /// year has no candidate at all, or none whose cycle contains `ts`.
    pub fn resolve(&self, symbol: &str, ts: Ts) -> Result<ContractSymbol, ContinuousError> {
        self.resolve_window(symbol, ts).map(|(symbol, _, _)| symbol)
    }

    /// [`ContractCycles::resolve`], plus the window over which the answer holds:
    /// `(opens, closes]`, exclusive on the left and inclusive on the right.
    ///
    /// A caller resolving millions of bar records uses the window to know when
    /// it may reuse the previous answer — and, more usefully, the window is the
    /// statement being made: *this* spelling means *this* contract only over
    /// *this* stretch. An unambiguous spelling holds for all time and says so.
    ///
    /// # Errors
    /// As [`ContractCycles::resolve`].
    pub fn resolve_window(
        &self,
        symbol: &str,
        ts: Ts,
    ) -> Result<(ContractSymbol, Ts, Ts), ContinuousError> {
        let parts = parse_parts(symbol)?;
        if !parts.has_ambiguous_year() {
            let resolved = ContractSymbol::new(parts.root, absolute_year(&parts), parts.month)?;
            return Ok((resolved, Ts(i64::MIN), Ts(i64::MAX)));
        }
        let empty: Vec<Cycle> = Vec::new();
        let family = self
            .families
            .get(&(parts.month, parts.year_value))
            .unwrap_or(&empty);
        let mine: Vec<&Cycle> = family
            .iter()
            .filter(|c| c.symbol.root() == parts.root)
            .collect();
        for (i, cycle) in mine.iter().enumerate() {
            // The window opens where the previous contract of this family
            // closed, so the windows tile with no gap and no overlap. The
            // family's earliest member has no predecessor, so it opens one
            // cycle before its own expiry.
            let opens = if i == 0 {
                Ts(cycle.expiry_earliest.0 - CONTRACT_CYCLE_DAYS * NANOS_PER_DAY)
            } else {
                mine[i - 1].expiry_latest
            };
            if ts > opens && ts <= cycle.expiry_latest {
                return Ok((cycle.symbol.clone(), opens, cycle.expiry_latest));
            }
        }
        Err(ContinuousError::UnresolvedContract {
            symbol: symbol.to_owned(),
            ts,
            candidates: mine
                .iter()
                .map(|c| (c.symbol.to_string(), c.expiry_latest))
                .collect(),
        })
    }
}

/// The absolute year an already-unambiguous spelling names.
fn absolute_year(parts: &super::symbol::SymbolParts<'_>) -> i32 {
    let value = i32::try_from(parts.year_value).unwrap_or(0);
    if parts.year_digits == 4 {
        value
    } else {
        2000 + value
    }
}

/// Calendar year of a UTC timestamp.
fn date_of_year(ts: Ts) -> i64 {
    date_of(ts).year
}

#[cfg(feature = "databento")]
pub use imp::{contract_cycles_from_definitions, expiries_from_definitions};

#[cfg(feature = "databento")]
pub(crate) mod imp {
    use std::collections::BTreeMap;
    use std::path::Path;

    use crucible_core::types::Ts;

    use crate::ingest::window::date_of;
    use databento::dbn::decode::{DecodeRecord, dbn::Decoder};
    use databento::dbn::{InstrumentClass, UNDEF_TIMESTAMP, record::InstrumentDefMsg};

    use crate::continuous::error::ContinuousError;
    use crate::continuous::symbol::{ContractSymbol, DecadeAnchor};

    use super::ContractCycles;

    /// Every expiry a `definition` file records for each outright of `root`,
    /// as `(earliest, latest)` — the one decode pass both public readers share.
    ///
    /// Only records whose `instrument_class` is exactly
    /// [`InstrumentClass::Future`] are kept. `InstrumentClass::is_future()` is
    /// **not** used: it returns true for `FutureSpread` too, and a calendar
    /// spread carries an expiry that would then compete with an outright's
    /// under the same parsed symbol.
    ///
    /// Records for other roots, records whose symbol is not an outright
    /// contract, and records with a null `expiration` are skipped silently — a
    /// `definition` file for a parent key legitimately contains thousands of
    /// each.
    ///
    /// What to *do* about a contract with two different expiries is left to the
    /// caller, because the two callers need different answers: see the module
    /// docs.
    fn expiry_spans(
        path: &Path,
        root: &str,
        anchor: DecadeAnchor,
    ) -> Result<BTreeMap<ContractSymbol, (Ts, Ts)>, ContinuousError> {
        let undecodable = |detail: String| ContinuousError::Undecodable {
            path: path.to_path_buf(),
            detail,
        };
        let mut decoder = Decoder::from_zstd_file(path).map_err(|e| undecodable(e.to_string()))?;
        let mut out: BTreeMap<ContractSymbol, (Ts, Ts)> = BTreeMap::new();

        while let Some(msg) = decoder
            .decode_record::<InstrumentDefMsg>()
            .map_err(|e| undecodable(e.to_string()))?
        {
            if msg.instrument_class().ok() != Some(InstrumentClass::Future) {
                continue;
            }
            if msg.expiration == UNDEF_TIMESTAMP {
                continue;
            }
            let Ok(raw_symbol) = msg.raw_symbol() else {
                continue;
            };
            // `raw_symbol` borrows the decoder's buffer; own it before the
            // next `decode_record` invalidates it.
            let raw_symbol = raw_symbol.to_owned();
            let expiration = i64::try_from(msg.expiration)
                .map_err(|_| undecodable(format!("expiration {} exceeds i64", msg.expiration)))?;
            let expiry = Ts(expiration);

            // A 16-year `definition` file contains `ESM0` twice — June 2010
            // and June 2020 — so no single [`DecadeAnchor`] can separate them,
            // and D-0046's constant makes the two collide into an
            // `ExpiryConflict` that refuses the whole file. The record can
            // separate them: resolve the one-digit year against the contract's
            // *own* expiry instead of a constant. Anchoring on the expiry year
            // rather than reading the year off it directly is deliberate —
            // some products expire in the month before the contract month, so
            // `CLF5` expiring in December 2024 must still resolve to 2025.
            // `anchor` remains the fallback for a timestamp we cannot place,
            // and two- and four-digit years are absolute either way.
            //
            // This is the device D-0072 reuses to name curated partitions:
            // same rule, second consumer.
            let record_anchor =
                i32::try_from(date_of(expiry).year).map_or(anchor, DecadeAnchor::new);
            let Ok(symbol) = ContractSymbol::parse_with_anchor(&raw_symbol, record_anchor) else {
                continue;
            };
            if symbol.root() != root {
                continue;
            }
            out.entry(symbol)
                .and_modify(|(lo, hi)| {
                    *lo = (*lo).min(expiry);
                    *hi = (*hi).max(expiry);
                })
                .or_insert((expiry, expiry));
        }
        Ok(out)
    }

    /// Reads outright futures expiries out of a `definition` DBN file.
    ///
    /// A contract defined twice with *different* expiries is a refusal: every
    /// roll decided from it depends on which is true (D-0046). See
    /// [`contract_cycles_from_definitions`] for the reader that asks a coarser
    /// question and so tolerates more.
    ///
    /// # Errors
    /// [`ContinuousError::Undecodable`] if the file cannot be opened or
    /// decoded, and [`ContinuousError::ExpiryConflict`] if one contract is
    /// given two expiries.
    pub fn expiries_from_definitions(
        path: &Path,
        root: &str,
        anchor: DecadeAnchor,
    ) -> Result<BTreeMap<ContractSymbol, Ts>, ContinuousError> {
        let spans = expiry_spans(path, root, anchor)?;
        let mut out = BTreeMap::new();
        for (symbol, (first, second)) in spans {
            if first != second {
                return Err(ContinuousError::ExpiryConflict {
                    contract: symbol.to_string(),
                    first,
                    second,
                });
            }
            out.insert(symbol, first);
        }
        Ok(out)
    }

    /// Reads the contract cycles a root's `definition` file describes.
    ///
    /// The input to [`ContractCycles::resolve`], and therefore to every curated
    /// partition key `transcode` writes (D-0072).
    ///
    /// # Errors
    /// [`ContinuousError::Undecodable`] if the file cannot be opened or
    /// decoded, and [`ContinuousError::ExpiryYearConflict`] if one contract is
    /// given expiries in two different years — the disagreement that makes the
    /// contract's identity, not merely its roll date, unknowable.
    pub fn contract_cycles_from_definitions(
        path: &Path,
        root: &str,
        anchor: DecadeAnchor,
    ) -> Result<ContractCycles, ContinuousError> {
        let spans = expiry_spans(path, root, anchor)?;
        ContractCycles::from_spans(spans.into_iter().map(|(s, (lo, hi))| (s, lo, hi)))
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use super::*;

        use std::fs::File;

        use crate::testutil::TempDir;
        use databento::dbn::encode::EncodeRecord;
        use databento::dbn::encode::dbn::Encoder;
        use databento::dbn::record::str_to_c_chars;
        use databento::dbn::{Metadata, RecordHeader, SType, Schema, rtype};

        const JAN1: i64 = 1_704_067_200_000_000_000;
        /// The same instant in the vendor's unsigned nanoseconds.
        const JAN1_NS: u64 = 1_704_067_200_000_000_000;

        /// Writes a `definition` DBN file: `(raw_symbol, class, expiration)`.
        ///
        /// `pub(crate)` because `transcode`'s fixtures need one too — a bar
        /// window cannot be transcoded without the expiries that name its
        /// contracts (D-0072), so every transcode fixture now plants the
        /// definition file a real archive would have.
        pub(crate) fn write_definitions(path: &Path, rows: &[(&str, InstrumentClass, u64)]) {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            let metadata = Metadata::builder()
                .dataset("GLBX.MDP3".to_owned())
                .schema(Some(Schema::Definition))
                .start(1_704_067_200_000_000_000)
                .stype_in(Some(SType::Parent))
                .stype_out(SType::InstrumentId)
                .symbols(vec!["ES.FUT".to_owned()])
                .build();
            let file = File::create(path).expect("create");
            let mut encoder = Encoder::with_zstd(file, &metadata).expect("encoder");
            for (index, (symbol, class, expiration)) in rows.iter().enumerate() {
                let mut msg = InstrumentDefMsg {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "fixture instrument ids are tiny"
                    )]
                    hd: RecordHeader::new::<InstrumentDefMsg>(
                        rtype::INSTRUMENT_DEF,
                        1,
                        index as u32 + 1,
                        1_704_067_200_000_000_000,
                    ),
                    ..InstrumentDefMsg::default()
                };
                msg.raw_symbol = str_to_c_chars(symbol).expect("short enough");
                msg.instrument_class = *class as i8;
                msg.expiration = *expiration;
                msg.leg_count = if matches!(class, InstrumentClass::FutureSpread) {
                    2
                } else {
                    0
                };
                encoder.encode_record(&msg).expect("encode");
            }
            drop(encoder);
        }

        #[test]
        fn outright_futures_are_kept_and_everything_else_is_skipped() {
            let dir = TempDir::new();
            let path = dir
                .path()
                .join("raw/GLBX.MDP3/definition/ES.FUT/2024-01.dbn.zst");
            let jan1 = JAN1_NS;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, jan1),
                    // A calendar spread. `is_future()` would let this in, and
                    // it parses as no outright anyway — belt and braces.
                    ("ESH4-ESM4", InstrumentClass::FutureSpread, jan1 + 1),
                    // Another root.
                    ("NQH4", InstrumentClass::Future, jan1 + 2),
                    // A null expiration.
                    ("ESM4", InstrumentClass::Future, UNDEF_TIMESTAMP),
                    ("ESU4", InstrumentClass::Future, jan1 + 3),
                ],
            );
            let found =
                expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT).expect("import");
            let names: Vec<String> = found.keys().map(ToString::to_string).collect();
            assert_eq!(names, vec!["ESH2024", "ESU2024"]);
            assert_eq!(
                found.get(&ContractSymbol::parse("ESH4").expect("valid")),
                Some(&Ts(JAN1))
            );
        }

        // The archive's real `GC.FUT` definition file gives `GCX1` two expiries
        // one hour apart, and `6EM23` two seventy-two hours apart. The roll
        // reader must refuse that (a calendar roll would fire on a different
        // session); the cycle reader must not, because an hour cannot make a
        // contract a different contract — and refusing would make every GC bar
        // in the archive untranscodable (D-0072).
        #[test]
        fn an_hours_disagreement_refuses_the_roll_reader_and_not_the_cycle_reader() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            let hour: u64 = 3_600_000_000_000;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, JAN1_NS),
                    ("ESH4", InstrumentClass::Future, JAN1_NS + hour),
                ],
            );

            let err = expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect_err("the roll reader still refuses");
            assert!(
                matches!(err, ContinuousError::ExpiryConflict { .. }),
                "{err}"
            );

            let cycles = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect("the cycle reader tolerates an hour");
            assert_eq!(cycles.len(), 1);
            // And the span it kept is the *later* end, so a bar printed in the
            // last hour still resolves.
            assert_eq!(
                cycles
                    .resolve("ESH4", Ts(JAN1 + i64::try_from(hour).expect("fits")))
                    .expect("resolves")
                    .to_string(),
                "ESH2024"
            );
        }

        // Two expiries in different YEARS is where the identity itself becomes
        // unknowable, and that is the one the cycle reader refuses. The control
        // for the tolerance above not being a blanket one.
        #[test]
        fn expiries_a_year_apart_refuse_the_cycle_reader_too() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            // JAN1_NS is 2024-01-01 and 2024 is a leap year, so 365 days would
            // land on 2024-12-31 and still be the same year. 400 does not.
            let year = 400 * 24 * 3_600_000_000_000_u64;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, JAN1_NS),
                    ("ESH4", InstrumentClass::Future, JAN1_NS + year),
                ],
            );
            let err = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect_err("a year is not an hour");
            assert!(
                matches!(err, ContinuousError::ExpiryYearConflict { .. }),
                "{err}"
            );
        }

        // Repeated definitions are the norm — the schema restates every
        // instrument daily — so agreeing repeats must be fine.
        #[test]
        fn a_repeated_definition_that_agrees_is_fine() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            let jan1 = JAN1_NS;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, jan1),
                    ("ESH4", InstrumentClass::Future, jan1),
                ],
            );
            let found =
                expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT).expect("import");
            assert_eq!(found.len(), 1);
        }

        #[test]
        fn a_contract_with_two_expiries_refuses() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            let jan1 = JAN1_NS;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, jan1),
                    ("ESH4", InstrumentClass::Future, jan1 + 1),
                ],
            );
            let err = expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect_err("must refuse two answers");
            assert!(
                matches!(err, ContinuousError::ExpiryConflict { .. }),
                "{err}"
            );
        }

        // The whole point, end to end through a real DBN encode: the two
        // `ESM0`s of a sixteen-year window separate into two contracts, and a
        // bar carrying that spelling resolves by *when it printed*.
        #[test]
        fn one_vendor_spelling_resolves_to_two_contracts_by_timestamp() {
            const JUN_2010: u64 = 1_276_867_800_000_000_000;
            const JUN_2020: u64 = 1_592_573_400_000_000_000;
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            write_definitions(
                &path,
                &[
                    ("ESM0", InstrumentClass::Future, JUN_2010),
                    ("ESM0", InstrumentClass::Future, JUN_2020),
                ],
            );
            let cycles = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect("two contracts, not a collision");
            assert_eq!(cycles.len(), 2);

            // A bar from March 2010 and one from March 2020 carry the same
            // four characters and are ten years and one contract apart.
            let mar_2010 = Ts(1_267_401_600_000_000_000);
            let mar_2020 = Ts(1_583_020_800_000_000_000);
            assert_eq!(
                cycles
                    .resolve("ESM0", mar_2010)
                    .expect("resolves")
                    .to_string(),
                "ESM2010"
            );
            assert_eq!(
                cycles
                    .resolve("ESM0", mar_2020)
                    .expect("resolves")
                    .to_string(),
                "ESM2020"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::window::date_of;

    fn expiry_date(symbol: &str) -> String {
        let s = ContractSymbol::parse(symbol).expect("valid symbol");
        date_of(nominal_expiry(&s)).to_string()
    }

    // Hand-derived against a wall calendar, and cross-checked against the
    // real CME equity-index expiries for 2024:
    //   ESH4 -> 2024-03-15 (2024-03-01 was itself a Friday)
    //   ESM4 -> 2024-06-21 (2024-06-01 was a Saturday; first Friday the 7th)
    //   ESU4 -> 2024-09-20 (2024-09-01 was a Sunday; first Friday the 6th)
    //   ESZ4 -> 2024-12-20 (2024-12-01 was a Sunday; first Friday the 6th)
    #[test]
    fn nominal_expiry_is_the_third_friday_of_the_delivery_month() {
        assert_eq!(expiry_date("ESH4"), "2024-03-15");
        assert_eq!(expiry_date("ESM4"), "2024-06-21");
        assert_eq!(expiry_date("ESU4"), "2024-09-20");
        assert_eq!(expiry_date("ESZ4"), "2024-12-20");
    }

    // A month starting on a Friday and one starting on a Thursday bracket the
    // wrap-around in the weekday arithmetic.
    #[test]
    fn the_weekday_arithmetic_wraps_correctly() {
        // 2025-08-01 was a Friday -> third Friday is the 15th.
        assert_eq!(expiry_date("ESQ25"), "2025-08-15");
        // 2026-01-01 is a Thursday -> first Friday the 2nd, third the 16th.
        assert_eq!(expiry_date("ESF26"), "2026-01-16");
    }

    #[test]
    fn a_map_can_be_built_for_a_whole_chain() {
        let symbols: Vec<ContractSymbol> = ["ESH4", "ESM4", "ESU4"]
            .iter()
            .map(|s| ContractSymbol::parse(s).expect("valid"))
            .collect();
        let map = nominal_expiries(&symbols);
        assert_eq!(map.len(), 3);
        assert!(map.keys().eq(symbols.iter()), "keys are in delivery order");
    }

    // ------------------------------------------- the cycle resolver (D-0072)

    use super::{CONTRACT_CYCLE_DAYS, ContractCycles};
    use crate::continuous::symbol::MonthCode;
    use crate::ingest::window::NANOS_PER_DAY;

    /// The two December gold contracts a sixteen-year window contains, with
    /// their real expiry days as epoch days (from the archive's own
    /// `definition` file): 2014-12-29 is day 16,433 and 2024-12-27 is 20,084.
    fn gold_z_cycles() -> ContractCycles {
        ContractCycles::from_spans([
            (
                ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid"),
                Ts(16_433 * NANOS_PER_DAY),
                Ts(16_433 * NANOS_PER_DAY),
            ),
            (
                ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid"),
                Ts(20_084 * NANOS_PER_DAY),
                Ts(20_084 * NANOS_PER_DAY),
            ),
        ])
        .expect("no year conflict")
    }

    /// A day, as this archive's timestamps count them.
    fn day(epoch_day: i64) -> Ts {
        Ts(epoch_day * NANOS_PER_DAY)
    }

    // THE fix, in one test. `GCZ4` is two contracts; which one a bar belongs to
    // is decided by when the bar printed, against the contract's own expiry.
    // 2014-01-02 is epoch day 16,072; 2024-01-02 is 19,724.
    #[test]
    fn one_spelling_two_contracts_resolved_by_the_bars_own_timestamp() {
        let cycles = gold_z_cycles();
        assert_eq!(
            cycles.resolve("GCZ4", day(16_072)).expect("resolves"),
            ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid")
        );
        assert_eq!(
            cycles.resolve("GCZ4", day(19_724)).expect("resolves"),
            ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid")
        );
    }

    // The boundary is the earlier contract's expiry, exclusive on the left and
    // inclusive on the right — the same half-open convention the roll instant
    // uses (D-0041), so the codebase has one rule and not two.
    #[test]
    fn the_cycle_boundary_is_the_previous_expiry_exclusive() {
        let cycles = gold_z_cycles();
        let expiry_2014 = 16_433;
        assert_eq!(
            cycles
                .resolve("GCZ4", day(expiry_2014))
                .expect("resolves")
                .year(),
            2014,
            "a bar AT the 2014 expiry is still the 2014 contract"
        );
        assert_eq!(
            cycles
                .resolve("GCZ4", day(expiry_2014 + 1))
                .expect("resolves")
                .year(),
            2024,
            "one day later it is the 2024 contract"
        );
    }

    // The refusal (2) of D-0072. The 2014 contract is the family's earliest, so
    // its window opens one cycle before its own expiry; a bar before that
    // belongs to a contract the definition file does not name, and filing it
    // anywhere would be a guess.
    #[test]
    fn a_bar_outside_every_cycle_is_refused_rather_than_filed() {
        let cycles = gold_z_cycles();
        let opens = 16_433 - CONTRACT_CYCLE_DAYS;
        assert!(cycles.resolve("GCZ4", day(opens)).is_err());
        assert!(cycles.resolve("GCZ4", day(opens + 1)).is_ok());
        // And past the last known expiry, which is the 2034 contract's turf.
        let err = cycles
            .resolve("GCZ4", day(20_085))
            .expect_err("no cycle owns this");
        assert!(
            matches!(err, ContinuousError::UnresolvedContract { .. }),
            "{err}"
        );
        // The message names the candidates rather than merely saying "no".
        let text = err.to_string();
        assert!(
            text.contains("GCZ2014") && text.contains("GCZ2024"),
            "{text}"
        );
    }

    // The `GCZ4`-shaped case with NO expiry available: the refusal must fire,
    // and it must not fall back to a `DecadeAnchor` constant. Under the
    // constant this would have quietly answered `GCZ2024` for every bar,
    // including the 2014 ones — which is exactly the bug (D-0072).
    #[test]
    fn a_one_digit_year_with_no_definition_at_all_is_refused() {
        let none = ContractCycles::default();
        assert!(none.is_empty());
        for epoch_day in [16_072, 19_724] {
            let err = none
                .resolve("GCZ4", day(epoch_day))
                .expect_err("nothing may resolve without an expiry");
            match &err {
                ContinuousError::UnresolvedContract { candidates, .. } => {
                    assert!(candidates.is_empty(), "{err}");
                }
                other => panic!("wrong refusal: {other}"),
            }
            assert!(
                err.to_string().contains("crucible pull"),
                "the refusal must say how to fix it: {err}"
            );
        }
        // The anchor would have answered — that is what makes it the wrong
        // device here, and the assertion that says so.
        assert_eq!(
            ContractSymbol::parse("GCZ4").expect("valid").year(),
            2024,
            "the constant always has an answer, and it is right half the time"
        );
    }

    // THE THIRD SIDE (CLAUDE.md §7). The two sides above show an aliased key
    // and a resolved key differ. This shows they AGREE the moment the resolver
    // is handed the expiry that separates them — so the difference is the year
    // resolution and nothing else: same root, same month, same everything but
    // the digit that could not say which decade.
    #[test]
    fn the_resolved_and_anchored_keys_agree_once_the_expiry_says_which_decade() {
        let cycles = gold_z_cycles();
        let anchored = ContractSymbol::parse("GCZ4").expect("valid");
        let resolved_2024 = cycles.resolve("GCZ4", day(19_724)).expect("resolves");
        let resolved_2014 = cycles.resolve("GCZ4", day(16_072)).expect("resolves");

        // Side one: on the 2014 bar they disagree, which is the defect.
        assert_ne!(anchored, resolved_2014);
        // Side two: on the 2024 bar they agree, so the anchor is not simply
        // broken — it is right for one of the two cycles.
        assert_eq!(anchored, resolved_2024);
        // The third side: the disagreement is *only* the year. Everything the
        // resolver did not have to guess is identical.
        assert_eq!(anchored.root(), resolved_2014.root());
        assert_eq!(anchored.month(), resolved_2014.month());
        assert_eq!(
            anchored.year() - resolved_2014.year(),
            10,
            "one decade — the period of a one-digit year code"
        );
        assert_eq!(
            ContractSymbol::parse_with_anchor("GCZ4", crate::continuous::DecadeAnchor::new(2015))
                .expect("valid"),
            resolved_2014,
            "give the anchor the decade the expiry knows and the two coincide"
        );
    }

    // Two- and four-digit years are absolute, so they resolve with no archive
    // at all. Demanding an expiry for them would refuse the far-dated listings
    // the vendor spells `CLZ36` precisely to avoid the ambiguity — 16 of them
    // trade in the archive's CL window.
    #[test]
    fn an_unambiguous_spelling_needs_no_expiry() {
        let none = ContractCycles::default();
        assert_eq!(
            none.resolve("CLZ36", day(19_724)).expect("absolute").year(),
            2036
        );
        assert_eq!(
            none.resolve("GCZ2014", day(0)).expect("absolute").year(),
            2014
        );
    }

    // A resolver for one root must not answer for another, even when the month
    // and digit line up. `NQZ4` is not a gold contract.
    #[test]
    fn a_resolver_does_not_answer_for_a_foreign_root() {
        let cycles = gold_z_cycles();
        let err = cycles
            .resolve("NQZ4", day(19_724))
            .expect_err("wrong root has no candidates here");
        match err {
            ContinuousError::UnresolvedContract { candidates, .. } => {
                assert!(candidates.is_empty());
            }
            other => panic!("wrong refusal: {other}"),
        }
    }

    // A family's members are one decade apart because that is the period of the
    // code that cannot distinguish them. Two that are not mean the definition
    // file dated one of them into the wrong decade, and the windows would then
    // be slivers a bar lands in by rounding.
    #[test]
    fn two_cycles_less_than_a_decade_apart_refuse_construction() {
        let err = ContractCycles::from_spans([
            (
                ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid"),
                day(16_433),
                day(16_433),
            ),
            (
                ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid"),
                day(16_433 + 400),
                day(16_433 + 400),
            ),
        ])
        .expect_err("400 days is not a decade");
        assert!(
            matches!(err, ContinuousError::ContractCycleCollision { .. }),
            "{err}"
        );
        // And the real spacing is accepted, so the check is not simply always on.
        assert!(gold_z_cycles().len() == 2);
    }

    #[test]
    fn a_spread_is_not_a_contract_the_resolver_will_name() {
        let cycles = gold_z_cycles();
        for bad in ["GCZ4-GCZ5", "CL:BF F0-G0-H0", "SYN:RW", "ES.v.0"] {
            assert!(
                matches!(
                    cycles.resolve(bad, day(19_724)),
                    Err(ContinuousError::UnparseableSymbol { .. })
                ),
                "{bad}"
            );
        }
    }
}

#[cfg(all(test, feature = "databento"))]
mod decade_tests {
    use super::ContractSymbol;
    use super::imp::expiries_from_definitions;

    use crate::continuous::symbol::{DecadeAnchor, MonthCode};
    use crate::testutil::TempDir;
    use databento::dbn::InstrumentClass;

    // The exact values the real 16-year ES definition archive produced, which
    // is how this defect was found: the June 2010 and June 2020 contracts are
    // BOTH called `ESM0`. 1_276_867_800 s is 2010-06-18T13:30:00Z and
    // 1_592_573_400 s is 2020-06-19T13:30:00Z — both 08:30 Central.
    const JUN_2010: u64 = 1_276_867_800_000_000_000;
    const JUN_2020: u64 = 1_592_573_400_000_000_000;

    // A single `DecadeAnchor` cannot separate them, so before this each file
    // spanning both decades refused with `ExpiryConflict` and every archived
    // expiry was unusable. Each record's own expiry can separate them.
    #[test]
    fn a_one_digit_year_resolves_against_each_records_own_expiry() {
        let dir = TempDir::new();
        let path = dir.path().join("defs.dbn.zst");
        super::imp::tests::write_definitions(
            &path,
            &[
                ("ESM0", InstrumentClass::Future, JUN_2010),
                ("ESM0", InstrumentClass::Future, JUN_2020),
            ],
        );

        let map = expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
            .expect("each record dates itself, so there is no conflict");
        assert_eq!(map.len(), 2, "two contracts, not one collision");

        let years: Vec<i32> = map.keys().map(ContractSymbol::year).collect();
        assert_eq!(years, vec![2010, 2020]);
        for symbol in map.keys() {
            assert_eq!(symbol.month(), MonthCode::M);
            assert_eq!(symbol.root(), "ES");
        }
        assert_eq!(
            map.values().map(|t| t.0).collect::<Vec<_>>(),
            vec![JUN_2010 as i64, JUN_2020 as i64]
        );
    }
}
