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

use crucible_core::types::Ts;

use crate::ingest::window::{CivilDate, NANOS_PER_DAY, days_from_civil};

use super::symbol::ContractSymbol;

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

#[cfg(feature = "databento")]
pub use imp::expiries_from_definitions;

#[cfg(feature = "databento")]
mod imp {
    use std::collections::BTreeMap;
    use std::path::Path;

    use crucible_core::types::Ts;

    use crate::ingest::window::date_of;
    use databento::dbn::decode::{DecodeRecord, dbn::Decoder};
    use databento::dbn::{InstrumentClass, UNDEF_TIMESTAMP, record::InstrumentDefMsg};

    use crate::continuous::error::ContinuousError;
    use crate::continuous::symbol::{ContractSymbol, DecadeAnchor};

    /// Reads outright futures expiries out of a `definition` DBN file.
    ///
    /// Only records whose `instrument_class` is exactly
    /// [`InstrumentClass::Future`] are kept. `InstrumentClass::is_future()` is
    /// **not** used: it returns true for `FutureSpread` too, and a calendar
    /// spread carries an expiry that would then compete with an outright's
    /// under the same parsed symbol.
    ///
    /// Records for other roots, records whose symbol is not an outright
    /// contract, and records with a null `expiration` are skipped silently —
    /// a `definition` file for a parent key legitimately contains thousands
    /// of each. A contract defined twice with *different* expiries is a
    /// refusal: every roll decided from it depends on which is true.
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
        let undecodable = |detail: String| ContinuousError::Undecodable {
            path: path.to_path_buf(),
            detail,
        };
        let mut decoder = Decoder::from_zstd_file(path).map_err(|e| undecodable(e.to_string()))?;
        let mut out: BTreeMap<ContractSymbol, Ts> = BTreeMap::new();

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
            // and two-digit years are absolute either way.
            let record_anchor =
                i32::try_from(date_of(expiry).year).map_or(anchor, DecadeAnchor::new);
            let Ok(symbol) = ContractSymbol::parse_with_anchor(&raw_symbol, record_anchor) else {
                continue;
            };
            if symbol.root() != root {
                continue;
            }
            match out.get(&symbol) {
                Some(&first) if first != expiry => {
                    return Err(ContinuousError::ExpiryConflict {
                        contract: raw_symbol,
                        first,
                        second: expiry,
                    });
                }
                Some(_) => {}
                None => {
                    out.insert(symbol, expiry);
                }
            }
        }
        Ok(out)
    }

    #[cfg(test)]
    pub(super) mod tests {
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

        /// `(raw_symbol, instrument_class, expiration)`.
        pub(in crate::continuous::expiry) fn write_definitions(
            path: &Path,
            rows: &[(&str, InstrumentClass, u64)],
        ) {
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
            assert_eq!(names, vec!["ESH24", "ESU24"]);
            assert_eq!(
                found.get(&ContractSymbol::parse("ESH4").expect("valid")),
                Some(&Ts(JAN1))
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
