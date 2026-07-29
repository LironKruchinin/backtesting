//! Gate arithmetic, against hand-built inventories.

use super::*;
use crate::calendar::Calendar;
use crate::external::thetadata::validate::ValidationReport;

fn calendar() -> TradingDayCalendar {
    Calendar::by_id("us_equity_options")
        .expect("bundled")
        .into_trading_days()
}

fn day(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
}

/// One inventory record, with the fields the gate reads.
fn record(
    root: &str,
    endpoint: &str,
    date: &str,
    rows: u64,
    distinct: u64,
    zero_rate: Option<f64>,
) -> InventoryRecord {
    let report = ValidationReport {
        raw_rows: rows,
        distinct_rows: distinct,
        zero_ohlc_rows: 0,
        ..ValidationReport::default()
    };
    let mut out = InventoryRecord::new(
        endpoint,
        root,
        "daily",
        date,
        date,
        &format!("{endpoint}?symbol={root}&start_date={date}"),
        &format!("external/thetadata/options/{root}/x/{date}.parquet"),
        "aa",
        1,
        &report,
        None,
        0,
    );
    out.zero_ohlc_rate = zero_rate;
    out
}

// -------------------------------------------------------------------------
// (b) Distinct-contract accounting — never raw rows.
// -------------------------------------------------------------------------

// The arithmetic that would silently double a decade of GEX-style aggregates.
// Both numbers are reported so the difference is visible rather than implied.
#[test]
fn distinct_contracts_and_raw_rows_are_reported_separately() {
    let records = vec![
        record(
            "SPY",
            "/option/history/eod",
            "2019-01-02",
            14_040,
            7_020,
            None,
        ),
        record(
            "SPY",
            "/option/history/eod",
            "2024-01-02",
            7_950,
            7_950,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 3));
    assert_eq!(out.raw_rows, 21_990, "the tempting number");
    assert_eq!(out.distinct_contracts, 14_970, "the true one");
    assert_eq!(out.files_with_data, 2);
}

// An empty (472) request is not a file and must not inflate the file count.
#[test]
fn empty_requests_are_counted_apart_from_files() {
    let mut empty = record(
        "NDX",
        "/option/history/greeks/eod",
        "2015-06-01",
        0,
        0,
        None,
    );
    empty.file_path = String::new();
    let records = vec![
        record("NDX", "/option/history/eod", "2015-06-01", 100, 100, None),
        empty,
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2015, 6, 2));
    assert_eq!(out.files_with_data, 1);
    assert_eq!(out.empty_requests, 1);
}

// -------------------------------------------------------------------------
// (c) Dup rates by era.
// -------------------------------------------------------------------------

// 2.000 before the boundary and 1.000 after — reported as 50 % and 0 % of rows
// being redundant, which is the same fact in the form an operator needs.
#[test]
fn the_two_eras_report_fifty_percent_and_zero_percent() {
    let records = vec![
        record(
            "SPY",
            "/option/history/eod",
            "2019-01-02",
            14_040,
            7_020,
            None,
        ),
        record(
            "SPY",
            "/option/history/eod",
            "2021-12-15",
            19_720,
            9_860,
            None,
        ),
        record(
            "SPY",
            "/option/history/eod",
            "2022-01-03",
            9_528,
            9_528,
            None,
        ),
        record(
            "SPY",
            "/option/history/eod",
            "2024-01-02",
            7_950,
            7_950,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 3));

    assert_eq!(out.pre_2022.files, 2);
    assert_eq!(out.pre_2022.conforming, 2);
    assert!(
        (out.pre_2022.mean_duplicate_share - 0.5).abs() < 1e-9,
        "50 % of pre-2022 rows are duplicates"
    );
    assert!(out.pre_2022.deviations.is_empty());

    assert_eq!(out.post_2022.files, 2);
    assert_eq!(out.post_2022.conforming, 2);
    assert!((out.post_2022.mean_duplicate_share - 0.0).abs() < 1e-9);
}

// PLANTED: a pre-2022 file that is NOT duplicated. That is a finding about the
// vendor's boundary, not a number to round to the era's expectation.
#[test]
fn planted_off_era_dup_rate_is_recorded_as_a_deviation() {
    let records = vec![
        record(
            "SPY",
            "/option/history/eod",
            "2019-01-02",
            14_040,
            7_020,
            None,
        ),
        // Same era, ratio 1.000 — does not belong.
        record(
            "SPY",
            "/option/history/eod",
            "2019-01-03",
            7_020,
            7_020,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2019, 1, 4));
    assert_eq!(out.pre_2022.files, 2);
    assert_eq!(out.pre_2022.conforming, 1);
    assert_eq!(out.pre_2022.deviations.len(), 1);
    assert_eq!(out.pre_2022.deviations[0].1, "2019-01-03");
}

// The era expectation applies to `eod` only: OI and greeks were never
// duplicated in any era, so folding them in would drag the mean toward zero and
// make a genuinely duplicated era look half-clean.
#[test]
fn only_eod_carries_the_era_expectation() {
    let records = vec![
        record(
            "SPY",
            "/option/history/eod",
            "2019-01-02",
            14_040,
            7_020,
            None,
        ),
        record(
            "SPY",
            "/option/history/open_interest",
            "2019-01-02",
            5_000,
            5_000,
            None,
        ),
        record(
            "SPY",
            "/option/history/greeks/eod",
            "2019-01-02",
            7_020,
            7_020,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2019, 1, 3));
    assert_eq!(out.pre_2022.files, 1, "only the eod file");
    assert!((out.pre_2022.mean_duplicate_share - 0.5).abs() < 1e-9);
}

// -------------------------------------------------------------------------
// (d) The reconciliation edges.
// -------------------------------------------------------------------------

#[test]
fn exact_eod_greeks_parity_is_counted_and_oi_coverage_averaged() {
    let records = vec![
        record(
            "SPY",
            "/option/history/eod",
            "2024-01-02",
            7_950,
            7_950,
            None,
        ),
        record(
            "SPY",
            "/option/history/greeks/eod",
            "2024-01-02",
            7_950,
            7_950,
            None,
        ),
        record(
            "SPY",
            "/option/history/open_interest",
            "2024-01-02",
            3_975,
            3_975,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 3));
    assert_eq!(out.eod_greeks_exact, 1);
    assert_eq!(out.days_fully_triangulated, 1);
    assert_eq!(out.oi_coverage_mean, Some(0.5));
    assert!(out.edges.iter().all(|e| !e.refuses));
}

// PLANTED: greeks holding more contracts than eod. Impossible under the
// established mechanism, so it refuses the day and sorts to the front.
#[test]
fn planted_inverted_eod_greeks_edge_refuses_and_sorts_first() {
    let records = vec![
        record("SPY", "/option/history/eod", "2024-01-02", 100, 100, None),
        record(
            "SPY",
            "/option/history/greeks/eod",
            "2024-01-02",
            120,
            120,
            None,
        ),
        // An ordinary asymmetry on another day, to prove the ordering.
        record("SPY", "/option/history/eod", "2024-01-03", 100, 100, None),
        record(
            "SPY",
            "/option/history/greeks/eod",
            "2024-01-03",
            90,
            90,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 4));
    assert!(out.edges[0].refuses, "refusals first: {:?}", out.edges);
    assert_eq!(out.edges[0].edge, "greeks/eod ⊆ eod");
    assert!(
        out.edges.iter().any(|e| !e.refuses),
        "and the asymmetry too"
    );
}

// PLANTED: a contract with open interest and no eod row inverts `OI ⊆ eod`.
#[test]
fn planted_inverted_oi_edge_refuses_the_day() {
    let records = vec![
        record("SPY", "/option/history/eod", "2024-01-02", 100, 100, None),
        record(
            "SPY",
            "/option/history/open_interest",
            "2024-01-02",
            130,
            130,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 3));
    assert_eq!(out.edges.len(), 1);
    assert!(out.edges[0].refuses);
    assert_eq!(out.edges[0].edge, "open_interest ⊆ eod");
}

// A positive delta is coverage asymmetry, logged and not refused: below a
// root's greeks floor this is the expected shape, and the computed surface
// covers those contracts (D-0053).
#[test]
fn contracts_without_greeks_are_logged_not_refused() {
    let records = vec![
        record("SPY", "/option/history/eod", "2024-01-02", 100, 100, None),
        record(
            "SPY",
            "/option/history/greeks/eod",
            "2024-01-02",
            80,
            80,
            None,
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 3));
    assert_eq!(out.edges.len(), 1);
    assert!(!out.edges[0].refuses);
    assert!(out.edges[0].detail.contains("20 contracts"));
}

// -------------------------------------------------------------------------
// (a) Coverage, and (e) the zero-OHLC fingerprint.
// -------------------------------------------------------------------------

// The span starts at first-seen, not at the subscription floor: a root we never
// asked for before 2017 must not be reported as missing five years.
#[test]
fn coverage_spans_from_first_seen_rather_than_the_subscription_floor() {
    let records = vec![
        record("SPY", "/option/history/eod", "2024-01-02", 10, 10, None),
        record("SPY", "/option/history/eod", "2024-01-03", 10, 10, None),
        record("SPY", "/option/history/eod", "2024-01-04", 10, 10, None),
        record("SPY", "/option/history/eod", "2024-01-05", 10, 10, None),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 5));
    let spy = &out.coverage[0];
    assert_eq!(spy.first_seen, Some(day(2024, 1, 2)));
    assert_eq!(spy.expected, 4, "Tue-Fri only");
    assert_eq!(spy.present, 4);
    assert_eq!(spy.fraction(), Some(1.0));
    assert!(spy.missing.is_empty());
}

// PLANTED: a session with nothing acquired. This is the check `verify` and
// `layout-check` structurally cannot make.
#[test]
fn planted_missing_session_shows_up_in_coverage() {
    let records = vec![
        record("SPY", "/option/history/eod", "2024-01-02", 10, 10, None),
        // 2024-01-03 absent.
        record("SPY", "/option/history/eod", "2024-01-04", 10, 10, None),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 4));
    let spy = &out.coverage[0];
    assert_eq!(spy.missing, vec![day(2024, 1, 3)]);
    assert_eq!(spy.fraction(), Some(2.0 / 3.0));
}

// A date the calendar calls closed is a finding ABOUT THE CALENDAR. Real data
// is evidence; a calendar is a claim (D-0040). Never auto-corrected.
#[test]
fn data_on_a_non_session_is_surfaced_as_a_calendar_finding() {
    let records = vec![
        record("SPY", "/option/history/eod", "2024-01-02", 10, 10, None),
        // New Year's Day.
        record("SPY", "/option/history/eod", "2024-01-01", 10, 10, None),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 2));
    assert_eq!(out.coverage[0].unexpected, vec![day(2024, 1, 1)]);
}

#[test]
fn the_zero_ohlc_fingerprint_is_averaged_per_root() {
    let records = vec![
        record(
            "VIX",
            "/option/history/eod",
            "2024-01-02",
            10,
            10,
            Some(0.64),
        ),
        record(
            "VIX",
            "/option/history/eod",
            "2024-01-03",
            10,
            10,
            Some(0.60),
        ),
        record(
            "SPY",
            "/option/history/eod",
            "2024-01-02",
            10,
            10,
            Some(0.20),
        ),
    ];
    let out = build(&records, &calendar(), BTreeSet::new(), day(2024, 1, 3));
    let vix = out
        .zero_ohlc_by_root
        .iter()
        .find(|(r, _, _)| r == "VIX")
        .expect("VIX present");
    assert_eq!(vix.1, 2);
    assert!((vix.2 - 0.62).abs() < 1e-9);
}

// -------------------------------------------------------------------------
// (g) Golden census.
// -------------------------------------------------------------------------

#[test]
fn the_golden_census_reports_what_is_present() {
    let cell = GoldenCell {
        root: "SPY".to_owned(),
        data_type: "eod".to_owned(),
        year: 2024,
    };
    let out = build(
        &[record(
            "SPY",
            "/option/history/eod",
            "2024-01-02",
            10,
            10,
            None,
        )],
        &calendar(),
        BTreeSet::from([cell.clone()]),
        day(2024, 1, 3),
    );
    assert!(out.golden_present.contains(&cell));
}
