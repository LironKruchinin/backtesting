//! Recovering the symbols a manifest record should have listed (D-0066).
//!
//! `ManifestRecord.symbols` is the requested key plus every raw symbol the
//! delivery declared (D-0033), and symbols the catalog would not accept were
//! **dropped and reported** rather than refused — dropping only ever makes
//! coverage understate what we own, while refusing would strand a file already
//! paid for. That trade was right; the predicate behind it was not. It banned
//! whitespace for every symbol, and CME's exotic spread names contain spaces
//! (`CL:BF F0-G0-H0`, `UD:ZN: TL 0110987001`), so 21,736 of 108,696 observed
//! symbols — every one of them on CL.FUT or ZN.FUT — never reached the
//! manifest. `coverage` reads those as missing, and the next pull would buy them
//! again: the re-buy bug D-0033 exists to prevent.
//!
//! ## What this module does, and what it refuses to do
//!
//! The predicate is fixed in [`crate::catalog`], which stops the bleeding for
//! every future append. This module repairs the records already written, in the
//! only way an append-only manifest permits: it re-reads each archived file's
//! symbology and appends a [`SymbolSupplement`] naming the record's manifest id
//! and the symbols it lacks. **No existing line is read back, rewritten,
//! reordered, or deleted.**
//!
//! Three deliberate constraints:
//!
//! 1. **The archive is the source, not `delivery/`.** The vendor's support
//!    files carry no resolved symbology — `metadata.json` holds the request
//!    (`symbols:["CL.FUT"]`), `manifest.json` a file list. The resolved set
//!    lives in each `.dbn.zst`'s own DBN header: immutable, already checksummed
//!    by `verify`, and the exact bytes the original append read.
//! 2. **The same decoder the ingest path uses**, through the same
//!    [`DeliveryInspector`] seam — never a second, "forensic" parser. A parser
//!    that disagreed with the production one would write a manifest nobody can
//!    reproduce, which is the version-skew failure D-0031 pinned for the
//!    decoder/client pair.
//! 3. **A file that cannot be decoded is a finding, not a guess.** Its record
//!    keeps whatever it has and the report says so; the caller exits non-zero.
//!    A symbol that the (loosened) predicate still refuses is likewise reported
//!    rather than smuggled in — the count is what makes "0 dropped" a
//!    measurement instead of a claim.
//!
//! The seam is why this is testable in a default build: [`plan`] takes a
//! `&dyn DeliveryInspector`, so a fake can script a spaced symbol and the whole
//! recover→append→credit path runs offline in microseconds (D-0032).

use crate::catalog::{
    Catalog, CatalogError, SupplementRequest, SupplementSource, SymbolSupplement,
};
use crate::ingest::delivery::DeliveryInspector;
use crucible_core::types::Ts;

/// One record whose symbol list is shorter than its file's symbology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolGap {
    /// Relative archive path of the record's file.
    pub file_path: String,
    /// The record's manifest id — what a supplement points at (D-0014).
    pub file_blake3: String,
    /// How many distinct symbols the file's DBN metadata declares.
    pub observed: usize,
    /// How many the manifest already credits, supplements included.
    pub credited: usize,
    /// Observed, recordable, and not yet credited — sorted and deduplicated.
    /// This is exactly what a supplement would add.
    pub missing: Vec<String>,
    /// Observed but still refused by [`crate::catalog::is_valid_symbol`], and
    /// therefore *not* in `missing`. Non-empty means the repair is incomplete
    /// for this record and the reason is visible rather than absorbed.
    pub unrecordable: Vec<String>,
}

/// A record whose file could not be decoded, so its symbology is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndecodableRecord {
    /// Relative archive path of the record's file.
    pub file_path: String,
    /// What the decoder said.
    pub detail: String,
}

/// What a recovery pass found. Deterministic: everything is in manifest record
/// order, and every symbol list is sorted (§2.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplementPlan {
    /// How many acquisition records were examined.
    pub records_read: usize,
    /// Records missing at least one symbol, in manifest order.
    pub gaps: Vec<SymbolGap>,
    /// Records whose file could not be decoded, in manifest order.
    pub undecodable: Vec<UndecodableRecord>,
}

impl SupplementPlan {
    /// Total symbols that would be credited by appending this plan.
    #[must_use]
    pub fn missing_total(&self) -> usize {
        self.gaps.iter().map(|gap| gap.missing.len()).sum()
    }

    /// Total observed symbols the predicate still refuses. Non-zero means the
    /// manifest cannot be made complete by appending — report it, do not round
    /// it away.
    #[must_use]
    pub fn unrecordable_total(&self) -> usize {
        self.gaps.iter().map(|gap| gap.unrecordable.len()).sum()
    }

    /// True when every record's symbology is fully credited and every file was
    /// readable.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.gaps.is_empty() && self.undecodable.is_empty()
    }
}

/// Re-reads every record's DBN symbology and reports what the manifest lacks.
///
/// Read-only: touches no manifest, spends nothing, and decodes only each file's
/// metadata header. Safe to run at any time — it is also the measurement that
/// tells you whether a previous repair finished.
#[must_use]
pub fn plan(catalog: &Catalog, inspector: &dyn DeliveryInspector) -> SupplementPlan {
    let mut out = SupplementPlan::default();
    for record in catalog.records() {
        out.records_read += 1;
        let path = catalog.data_dir().join(&record.file_path);
        let observed = match inspector.observed_symbols(&path) {
            Ok(observed) => observed,
            Err(source) => {
                out.undecodable.push(UndecodableRecord {
                    file_path: record.file_path.clone(),
                    detail: source.to_string(),
                });
                continue;
            }
        };
        let credited = catalog.credited_symbols(record);

        let mut distinct: Vec<&String> = observed.iter().collect();
        distinct.sort_unstable();
        distinct.dedup();
        let mut missing = Vec::new();
        let mut unrecordable = Vec::new();
        for symbol in distinct.iter().filter(|s| !credited.contains(**s)) {
            if crate::catalog::is_valid_symbol(symbol) {
                missing.push((*symbol).clone());
            } else {
                unrecordable.push((*symbol).clone());
            }
        }
        if missing.is_empty() && unrecordable.is_empty() {
            continue;
        }
        out.gaps.push(SymbolGap {
            file_path: record.file_path.clone(),
            file_blake3: record.file_blake3.clone(),
            observed: distinct.len(),
            credited: credited.len(),
            missing,
            unrecordable,
        });
    }
    out
}

/// Appends one [`SymbolSupplement`] per gap that has recordable symbols.
///
/// `recorded_ts` is caller-supplied because library code never reads a clock
/// (D-0015, §2.2), and `reason` should name the decision that ordered the
/// correction. Gaps whose `missing` list is empty (everything observed is
/// unrecordable) are skipped: there is nothing to append and a line that
/// changes nothing is noise.
///
/// Appends are independent and durable one at a time, so a failure part-way
/// leaves the earlier supplements on disk and re-running picks up the rest —
/// the append-only log resuming exactly as `pull` does.
///
/// # Errors
/// Whatever [`Catalog::append_supplement`] refuses: an unknown target, an
/// unusable symbol or reason, or an I/O failure.
pub fn apply(
    catalog: &mut Catalog,
    plan: &SupplementPlan,
    recorded_ts: Ts,
    reason: &str,
) -> Result<Vec<SymbolSupplement>, CatalogError> {
    let mut written = Vec::new();
    for gap in &plan.gaps {
        if gap.missing.is_empty() {
            continue;
        }
        written.push(catalog.append_supplement(SupplementRequest {
            supplements_blake3: gap.file_blake3.clone(),
            added_symbols: gap.missing.clone(),
            source: SupplementSource::DbnMetadata,
            recorded_ts,
            reason: reason.to_owned(),
        })?);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Acquisition, CoverageRequest, TsRange};
    use crate::ingest::delivery::fake::FakeInspector;
    use crate::testutil::TempDir;
    use std::path::Path;

    const REL: &str = "raw/GLBX.MDP3/ohlcv-1m/ZN.FUT/2024-01.dbn.zst";
    /// A real dropped symbol from the archive: two colons and two spaces.
    const SPREAD: &str = "UD:ZN: TL 0110987001";

    fn plant(dir: &Path, rel: &str) {
        let abs = dir.join(rel);
        std::fs::create_dir_all(abs.parent().expect("rel path has a parent")).expect("mkdir");
        std::fs::write(&abs, b"zn").expect("write fixture");
    }

    fn ts_range(start: i64, end: i64) -> TsRange {
        TsRange::new(Ts(start), Ts(end)).expect("valid range")
    }

    /// A catalog holding one ZN.FUT record whose `symbols` lists only the key
    /// and one outright — i.e. the archive as the old predicate left it.
    fn catalog_with_short_record(dir: &TempDir) -> Catalog {
        plant(dir.path(), REL);
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog
            .append(Acquisition {
                dataset: "GLBX.MDP3".to_owned(),
                schema: "ohlcv-1m".to_owned(),
                symbols: vec!["ZN.FUT".to_owned(), "ZNH4".to_owned()],
                range: ts_range(100, 200),
                acquired_ts: Ts(150),
                databento_job_id: "job-1".to_owned(),
                file_path: REL.to_owned(),
            })
            .expect("append");
        catalog
    }

    /// The inspector reports what the file really contains: the key, the
    /// outright, and the spread the old rule dropped.
    fn inspector() -> FakeInspector {
        FakeInspector::new().with_symbols("2024-01.dbn.zst", &["ZN.FUT", "ZNH4", SPREAD])
    }

    #[test]
    fn plan_finds_exactly_the_dropped_symbol() {
        let dir = TempDir::new();
        let catalog = catalog_with_short_record(&dir);
        let found = plan(&catalog, &inspector());
        assert_eq!(found.records_read, 1);
        assert_eq!(found.gaps.len(), 1);
        assert_eq!(found.gaps[0].observed, 3);
        assert_eq!(found.gaps[0].credited, 2);
        assert_eq!(found.gaps[0].missing, vec![SPREAD.to_owned()]);
        assert!(found.gaps[0].unrecordable.is_empty());
        assert_eq!(found.missing_total(), 1);
        assert!(!found.is_complete());
    }

    #[test]
    fn apply_then_replan_is_complete_and_idempotent() {
        let dir = TempDir::new();
        let mut catalog = catalog_with_short_record(&dir);
        let first = plan(&catalog, &inspector());
        let written = apply(&mut catalog, &first, Ts(999), "D-0066 test").expect("apply");
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].added_symbols, vec![SPREAD.to_owned()]);

        // The measurement that proves the repair landed: a second pass over the
        // same archive finds nothing, so re-running appends nothing.
        let second = plan(&catalog, &inspector());
        assert!(second.is_complete(), "second pass: {second:?}");
        let again = apply(&mut catalog, &second, Ts(1000), "D-0066 test").expect("apply again");
        assert!(again.is_empty());

        // And it survives a reload — the credit is on disk, not in memory.
        let reloaded = Catalog::open(dir.path()).expect("reopen");
        assert_eq!(reloaded.supplements().len(), 1);
        assert!(plan(&reloaded, &inspector()).is_complete());
    }

    #[test]
    fn supplemented_symbol_round_trips_through_coverage() {
        // The half that was actually broken (D-0066 specific 3): coverage
        // credit, not merely a line in a file.
        let dir = TempDir::new();
        let mut catalog = catalog_with_short_record(&dir);

        let req = CoverageRequest {
            dataset: "GLBX.MDP3".to_owned(),
            schema: "ohlcv-1m".to_owned(),
            symbols: vec![SPREAD.to_owned()],
            range: ts_range(100, 200),
        };
        // Before: the whole range reads as missing, which is what would fund a
        // second purchase of bytes we already hold.
        assert_eq!(
            catalog.coverage(&req).expect("valid request")[SPREAD],
            vec![ts_range(100, 200)]
        );

        let found = plan(&catalog, &inspector());
        apply(&mut catalog, &found, Ts(999), "D-0066 test").expect("apply");

        // After: fully covered, and still fully covered on a fresh handle.
        assert_eq!(
            catalog.coverage(&req).expect("valid request")[SPREAD],
            Vec::<TsRange>::new()
        );
        let reloaded = Catalog::open(dir.path()).expect("reopen");
        assert_eq!(
            reloaded.coverage(&req).expect("valid request")[SPREAD],
            Vec::<TsRange>::new()
        );
    }

    #[test]
    fn undecodable_file_is_a_finding_not_a_silent_skip() {
        // Negative control: the one case where the recovery cannot know the
        // answer must be reported, never absorbed into "nothing missing".
        let dir = TempDir::new();
        let catalog = catalog_with_short_record(&dir);
        let mut broken = inspector();
        broken.undecodable.push("2024-01.dbn.zst".to_owned());

        let found = plan(&catalog, &broken);
        assert_eq!(found.records_read, 1);
        assert!(found.gaps.is_empty());
        assert_eq!(found.undecodable.len(), 1);
        assert_eq!(found.undecodable[0].file_path, REL);
        assert!(
            !found.is_complete(),
            "an unreadable file must not read as complete"
        );
    }

    #[test]
    fn still_unrecordable_symbols_are_reported_and_not_appended() {
        // A control character is still refused (the loosening was spaces and
        // colons, not everything), and the refusal must be visible in the plan
        // rather than swallowed — otherwise "0 dropped" would be a claim.
        let dir = TempDir::new();
        let mut catalog = catalog_with_short_record(&dir);
        let inspector =
            FakeInspector::new().with_symbols("2024-01.dbn.zst", &["ZN.FUT", "ZNH4", "ZN\tH4"]);

        let found = plan(&catalog, &inspector);
        assert_eq!(found.gaps.len(), 1);
        assert!(found.gaps[0].missing.is_empty());
        assert_eq!(found.gaps[0].unrecordable, vec!["ZN\tH4".to_owned()]);
        assert_eq!(found.unrecordable_total(), 1);

        // Nothing to append, and nothing appended.
        assert!(
            apply(&mut catalog, &found, Ts(999), "D-0066 test")
                .expect("apply")
                .is_empty()
        );
        assert!(catalog.supplements().is_empty());
    }
}
