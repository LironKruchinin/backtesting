//! The T0 validation gate: seven questions, answered in numbers.
//!
//! Everything here is computed from `inventory.jsonl` — the per-file record
//! written at fetch time — rather than from the Parquet, because the era
//! fingerprints it reports (dup rate, zero-OHLC rate, sentinel drops) **cannot
//! be recovered from the Parquet at all**. A deduplicated file does not
//! remember arriving duplicated. That is the entire reason those fields are
//! written down (D-0054, D-0055).
//!
//! ## What "per (root, day)" means here, precisely
//!
//! Two different strengths of check, and conflating them would overstate what
//! was verified:
//!
//! - **Count-level, every day.** `distinct_contracts` for `eod`, `greeks/eod`
//!   and `open_interest` on the same (root, date) are compared directly. This
//!   covers 100 % of acquired days and catches every magnitude error.
//! - **Key-set level, not yet computed here.** Whether `keys(OI) ⊆ keys(eod)`
//!   is a statement about *identities*, not counts, and answering it means
//!   reading both Parquet files back. `validate::reconcile` does exactly that
//!   and is tested against planted inversions; wiring it over sampled days is
//!   the parity spot-check, and until it runs this report must not be read as
//!   having made that claim.
//!
//! A count-level equality is weaker than a key-set equality and the report
//! labels it as such. Reporting the weaker check under the stronger name is
//! precisely the "larger convenient number" §0.1 warns about.
//!
//! ## The dup-rate era boundary
//!
//! `dup_rate` is rows ÷ distinct: **2.000 before 2022-01-03 and 1.000 after**
//! (D-0054). Expressed as a share of redundant rows that is **50 % and 0 %**,
//! which is the same fact and the form the era check reports, because "50 % of
//! rows were duplicates" is what an operator needs to hear. A day off its era's
//! expectation is a **finding**, never a correction: the archive is evidence.

use std::collections::{BTreeMap, BTreeSet};

use super::inventory::InventoryRecord;
use crate::calendar::{CivilDate, TradingDayCalendar};
use crate::ingest::window::{civil_from_days, days_from_civil};

/// The date the vendor's duplication stopped (D-0054).
pub const DEDUP_ERA_BOUNDARY: CivilDate = CivilDate {
    year: 2022,
    month: 1,
    day: 3,
};

/// Parses `YYYY-MM-DD`.
#[must_use]
pub fn parse_date(text: &str) -> Option<CivilDate> {
    let mut parts = text.split('-');
    Some(CivilDate {
        year: parts.next()?.parse().ok()?,
        month: parts.next()?.parse().ok()?,
        day: parts.next()?.parse().ok()?,
    })
}

/// (a) Coverage of one root against the session calendar.
#[derive(Debug, Clone, Default)]
pub struct RootCoverage {
    /// Root symbol.
    pub root: String,
    /// First date any endpoint saw for this root.
    pub first_seen: Option<CivilDate>,
    /// Last date any endpoint saw.
    pub last_seen: Option<CivilDate>,
    /// Sessions the calendar expects across the evaluated span.
    pub expected: u64,
    /// Sessions with at least one inventory line.
    pub present: u64,
    /// Expected sessions with nothing at all.
    pub missing: Vec<CivilDate>,
    /// Dates held that the calendar says were not sessions. **A finding about
    /// the calendar, not a licence to delete data.**
    pub unexpected: Vec<CivilDate>,
}

impl RootCoverage {
    /// Present ÷ expected, or `None` over an empty span.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        (self.expected > 0).then(|| self.present as f64 / self.expected as f64)
    }
}

/// (c) Duplication behaviour of one era.
#[derive(Debug, Clone, Default)]
pub struct EraDup {
    /// Files measured.
    pub files: u64,
    /// Files whose dup rate matched the era's expectation.
    pub conforming: u64,
    /// Mean share of rows that were duplicates, 0.0–1.0.
    pub mean_duplicate_share: f64,
    /// Files that deviated, with what they showed. Findings.
    pub deviations: Vec<(String, String, f64)>,
}

/// (d) Count-level reconciliation across endpoints for one (root, day).
#[derive(Debug, Clone)]
pub struct EdgeFinding {
    /// Root.
    pub root: String,
    /// Date.
    pub date: String,
    /// Which edge.
    pub edge: &'static str,
    /// What was seen.
    pub detail: String,
    /// True when this inverts the vendor mechanism and refuses the day.
    pub refuses: bool,
}

/// (g) One golden-raw census cell.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GoldenCell {
    /// Root.
    pub root: String,
    /// Data type segment.
    pub data_type: String,
    /// Calendar year.
    pub year: i64,
}

/// The whole gate report.
#[derive(Debug, Clone, Default)]
pub struct GateReport {
    /// (a) Per-root coverage against the calendar.
    pub coverage: Vec<RootCoverage>,
    /// (b) Distinct contracts, and the raw-row figure it must never be.
    pub distinct_contracts: u64,
    /// (b) Raw rows the vendor sent — the number that would double-count.
    pub raw_rows: u64,
    /// (b) Files that produced data.
    pub files_with_data: u64,
    /// (b) Requests recorded as empty (vendor 472).
    pub empty_requests: u64,
    /// (c) Dup behaviour before the boundary.
    pub pre_2022: EraDup,
    /// (c) Dup behaviour from the boundary onward.
    pub post_2022: EraDup,
    /// (d) Edge findings, worst first.
    pub edges: Vec<EdgeFinding>,
    /// (d) (root, day) pairs where all three endpoints were present.
    pub days_fully_triangulated: u64,
    /// (d) Days where eod and greeks agreed exactly on contract count.
    pub eod_greeks_exact: u64,
    /// (d) Mean OI ⊆ eod coverage fraction.
    pub oi_coverage_mean: Option<f64>,
    /// (e) Zero-OHLC rate per root: (root, files, mean rate).
    pub zero_ohlc_by_root: Vec<(String, u64, f64)>,
    /// (f) Conflicting build pairs seen — QA signal, not failure.
    pub conflicting_pairs: u64,
    /// (f) Sentinel rows dropped.
    pub sentinel_rows_dropped: u64,
    /// (g) Golden cells present.
    pub golden_present: BTreeSet<GoldenCell>,
    /// (g) Golden cells the plan implies but that are absent.
    pub golden_missing: Vec<GoldenCell>,
}

/// Builds the report from inventory records plus the golden-raw census.
///
/// `today` bounds the expected span; it is passed in rather than read, because
/// this crate reads no clock (§2.2).
#[must_use]
pub fn build(
    records: &[InventoryRecord],
    calendar: &TradingDayCalendar,
    golden_present: BTreeSet<GoldenCell>,
    today: CivilDate,
) -> GateReport {
    let mut report = GateReport {
        golden_present,
        ..GateReport::default()
    };

    // Index by root, then by (date, endpoint).
    let mut by_root: BTreeMap<String, BTreeMap<(String, String), &InventoryRecord>> =
        BTreeMap::new();
    for record in records {
        report.raw_rows += record.row_count;
        report.distinct_contracts += record.distinct_contracts;
        report.conflicting_pairs += record.conflicting_pairs;
        report.sentinel_rows_dropped += record.sentinel_rows_dropped;
        if record.file_path.is_empty() {
            report.empty_requests += 1;
        } else {
            report.files_with_data += 1;
        }
        by_root
            .entry(record.root.clone())
            .or_default()
            .insert((record.start_date.clone(), record.endpoint.clone()), record);
    }

    let mut oi_fractions: Vec<f64> = Vec::new();

    for (root, cells) in &by_root {
        // -------- (a) coverage --------
        let dates: BTreeSet<CivilDate> = cells
            .keys()
            .filter_map(|(date, _)| parse_date(date))
            .collect();
        let mut coverage = RootCoverage {
            root: root.clone(),
            first_seen: dates.iter().next().copied(),
            last_seen: dates.iter().next_back().copied(),
            ..RootCoverage::default()
        };
        if let Some(first) = coverage.first_seen {
            // Expected span is [max(subscription floor, first-seen), today]:
            // holding a root back to its own first sighting rather than to the
            // vendor's floor avoids reporting a root we never asked for as
            // missing a decade.
            let mut day = first;
            while days_from_civil(day) <= days_from_civil(today) {
                if calendar.is_trading_day(day) {
                    coverage.expected += 1;
                    if dates.contains(&day) {
                        coverage.present += 1;
                    } else {
                        coverage.missing.push(day);
                    }
                }
                day = civil_from_days(days_from_civil(day) + 1);
            }
            for date in &dates {
                if !calendar.is_trading_day(*date) {
                    coverage.unexpected.push(*date);
                }
            }
        }
        report.coverage.push(coverage);

        // -------- (c), (d), (e) --------
        let mut zero_rates: Vec<f64> = Vec::new();
        let mut zero_files = 0u64;
        for date in &dates {
            let key = |endpoint: &str| {
                cells.get(&(
                    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day),
                    endpoint.to_owned(),
                ))
            };
            let eod = key("/option/history/eod");
            let greeks = key("/option/history/greeks/eod");
            let oi = key("/option/history/open_interest");

            for record in [eod, greeks, oi].into_iter().flatten() {
                if record.file_path.is_empty() {
                    continue;
                }
                era_of(*date, record, &mut report);
                if let Some(rate) = record.zero_ohlc_rate {
                    zero_rates.push(rate);
                    zero_files += 1;
                }
            }

            let Some(eod) = eod.filter(|r| !r.file_path.is_empty()) else {
                continue;
            };
            if greeks.is_some_and(|g| !g.file_path.is_empty())
                && oi.is_some_and(|o| !o.file_path.is_empty())
            {
                report.days_fully_triangulated += 1;
            }

            // Edge 1: distinct(eod) vs distinct(greeks). Expected 0.
            if let Some(greeks) = greeks.filter(|r| !r.file_path.is_empty()) {
                let delta =
                    i128::from(eod.distinct_contracts) - i128::from(greeks.distinct_contracts);
                if delta == 0 {
                    report.eod_greeks_exact += 1;
                } else if delta < 0 {
                    // greeks holding more contracts than eod inverts the
                    // mechanism: greeks is derived from the same chain.
                    report.edges.push(EdgeFinding {
                        root: root.clone(),
                        date: eod.start_date.clone(),
                        edge: "greeks/eod ⊆ eod",
                        detail: format!(
                            "greeks {} > eod {} — inverted",
                            greeks.distinct_contracts, eod.distinct_contracts
                        ),
                        refuses: true,
                    });
                } else {
                    report.edges.push(EdgeFinding {
                        root: root.clone(),
                        date: eod.start_date.clone(),
                        edge: "eod without greeks",
                        detail: format!(
                            "{delta} contracts in eod lack greeks — coverage asymmetry"
                        ),
                        refuses: false,
                    });
                }
            }

            // Edge 2: OI ⊆ eod, count-level, with the coverage fraction.
            if let Some(oi) = oi.filter(|r| !r.file_path.is_empty()) {
                if oi.distinct_contracts > eod.distinct_contracts {
                    report.edges.push(EdgeFinding {
                        root: root.clone(),
                        date: eod.start_date.clone(),
                        edge: "open_interest ⊆ eod",
                        detail: format!(
                            "OI {} > eod {} — inverted",
                            oi.distinct_contracts, eod.distinct_contracts
                        ),
                        refuses: true,
                    });
                } else if eod.distinct_contracts > 0 {
                    oi_fractions.push(oi.distinct_contracts as f64 / eod.distinct_contracts as f64);
                }
            }
        }
        if zero_files > 0 {
            let mean = zero_rates.iter().sum::<f64>() / zero_rates.len() as f64;
            report
                .zero_ohlc_by_root
                .push((root.clone(), zero_files, mean));
        }
    }

    if !oi_fractions.is_empty() {
        report.oi_coverage_mean =
            Some(oi_fractions.iter().sum::<f64>() / oi_fractions.len() as f64);
    }
    // Refusals first, then asymmetries: the reader must meet the day-refusing
    // findings before the ordinary ones.
    report.edges.sort_by_key(|e| !e.refuses);
    report
}

/// Folds one file into its era's duplication statistics.
fn era_of(date: CivilDate, record: &InventoryRecord, report: &mut GateReport) {
    let pre = days_from_civil(date) < days_from_civil(DEDUP_ERA_BOUNDARY);
    // dup_rate is rows/distinct; the share of rows that were redundant is
    // 1 - 1/rate. 2.000 -> 50 %, 1.000 -> 0 %.
    let share = if record.dup_rate > 0.0 {
        1.0 - (1.0 / record.dup_rate)
    } else {
        0.0
    };
    // `open_interest` and `greeks/eod` were never duplicated in any era
    // (D-0054), so only `eod` carries the era expectation.
    if record.endpoint != "/option/history/eod" {
        return;
    }
    let era = if pre {
        &mut report.pre_2022
    } else {
        &mut report.post_2022
    };
    era.files += 1;
    era.mean_duplicate_share =
        (era.mean_duplicate_share * (era.files - 1) as f64 + share) / era.files as f64;
    let conforms = if pre {
        (record.dup_rate - 2.0).abs() < 0.01
    } else {
        (record.dup_rate - 1.0).abs() < 0.01
    };
    if conforms {
        era.conforming += 1;
    } else {
        era.deviations.push((
            record.root.clone(),
            record.start_date.clone(),
            record.dup_rate,
        ));
    }
}

#[cfg(test)]
#[path = "gate/tests.rs"]
mod tests;
