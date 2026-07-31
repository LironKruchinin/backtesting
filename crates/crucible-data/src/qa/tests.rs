//! Data-QA tests.
//!
//! The checks are exercised directly on hand-built bar series rather than
//! through `run`, which would need a curated Parquet partition on disk for
//! every case; `curated`'s own tests already cover that path. The calendar is
//! a hand-written fixture for the reason given in `calendar::tests`: these
//! assertions must not move when a holiday rule is corrected.

use super::*;

/// 23-hour session, no halts, no holidays — so every expected-bar count in
/// this file can be derived in a comment.
const PLAIN_CALENDAR: &str = r#"
schema_version = 1

[[calendar]]
id = "test_plain"
description = "23-hour session, no halts, no holidays"
timezone = "America/Chicago"
roots = ["ZZ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00"
close_local = "16:00"
rth_open_local = "08:30"
rth_close_local = "15:15"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2024-02-01"
rationale = "one month"
"#;

/// Same session, split by a one-hour halt at 12:00–13:00 local. The bundled
/// CME table has no halt (D-0040), so without this fixture the two-interval
/// path through `check_against_calendar` — the one where the bar cursor has to
/// resume inside the same trading day — would never be exercised.
const HALTED_CALENDAR: &str = r#"
schema_version = 1

[[calendar]]
id = "test_halted"
description = "23-hour session split by a one-hour midday halt"
timezone = "America/Chicago"
roots = ["ZZ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00"
close_local = "16:00"
halt_local = [["12:00", "13:00"]]
rth_open_local = "08:30"
rth_close_local = "15:15"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2024-02-01"
rationale = "one month"
"#;

fn calendar() -> Calendar {
    Calendar::parse_table("fixture", PLAIN_CALENDAR)
        .expect("fixture parses")
        .pop()
        .expect("fixture has one calendar")
}

fn halted_calendar() -> Calendar {
    Calendar::parse_table("fixture", HALTED_CALENDAR)
        .expect("fixture parses")
        .pop()
        .expect("fixture has one calendar")
}

fn blank_report() -> QaReport {
    QaReport {
        instrument: "TEST".to_owned(),
        tf: TimeFrame::H1,
        calendar_id: None,
        bars: 0,
        first_ts_open: None,
        last_ts_open: None,
        expected_bars: None,
        gaps: Vec::new(),
        missing_bars: 0,
        unexpected_bars: Vec::new(),
        unexpected_bar_count: 0,
        zero_volume_runs: Vec::new(),
        zero_volume_bars: 0,
        spikes: Vec::new(),
        spike_count: 0,
        spike_sigmas: DEFAULT_SPIKE_SIGMAS,
        checks: Vec::new(),
        robust_sigma_points: None,
        ohlc_violations: Vec::new(),
        dst_boundaries: Vec::new(),
        conditions: Vec::new(),
        condition_sources: Vec::new(),
    }
}

fn report_for(bars: &[Bar]) -> QaReport {
    let mut report = blank_report();
    report.bars = bars.len();
    report.first_ts_open = bars.first().map(|b| b.ts_open);
    report.last_ts_open = bars.last().map(|b| b.ts_open);
    report
}

const HOUR_NS: i64 = 3_600_000_000_000;

/// Trading day 2024-01-02 opens 2024-01-01 17:00 CST = 23:00 UTC
/// (1_704_150_000 s) and closes 16:00 CST the next day (1_704_232_800 s).
/// That is 23 hours, so exactly 23 hourly bars.
const SESSION_OPEN_NS: i64 = 1_704_150_000 * 1_000_000_000;

fn hourly_session(count: usize) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            #[expect(clippy::cast_possible_wrap, reason = "test index, bounded by `count`")]
            let offset = i as i64;
            bar(SESSION_OPEN_NS + offset * HOUR_NS, TimeFrame::H1, 5000, 100)
        })
        .collect()
}

#[test]
fn a_complete_session_has_full_coverage_and_no_gaps() {
    let bars = hourly_session(23);
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &calendar(),
        &QaOptions::default(),
        &mut report,
    );
    assert_eq!(report.expected_bars, Some(23));
    assert_eq!(report.missing_bars, 0);
    assert!(report.gaps.is_empty());
    assert_eq!(report.unexpected_bar_count, 0);
    let coverage = report.coverage_pct().expect("a calendar was supplied");
    assert!((coverage - 100.0).abs() < 1e-9, "{coverage}");
}

#[test]
fn a_hole_in_the_middle_is_reported_as_one_gap() {
    let mut bars = hourly_session(23);
    // Drop the 6th, 7th and 8th bars of the session.
    bars.drain(5..8);
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &calendar(),
        &QaOptions::default(),
        &mut report,
    );
    // First and last bars are unchanged, so the expected count is unchanged.
    assert_eq!(report.expected_bars, Some(23));
    assert_eq!(report.missing_bars, 3);
    assert_eq!(report.gaps.len(), 1);
    assert_eq!(report.gaps[0].missing_bars, 3);
    assert_eq!(report.gaps[0].from_ts, Ts(SESSION_OPEN_NS + 5 * HOUR_NS));
    assert_eq!(report.gaps[0].to_ts, Ts(SESSION_OPEN_NS + 8 * HOUR_NS));
    // 20 of 23.
    let coverage = report.coverage_pct().expect("a calendar was supplied");
    assert!((coverage - 20.0 / 23.0 * 100.0).abs() < 1e-9, "{coverage}");
}

// The check that runs backwards. A bar stamped inside the daily maintenance
// break cannot be right unless the calendar is wrong — and the archive is the
// evidence, so this is how a bad calendar announces itself.
#[test]
fn bars_the_calendar_says_cannot_exist_are_reported() {
    let mut bars = hourly_session(23);
    // 16:30 CST on 2024-01-02 is inside the break: 1_704_232_800 + 1800 s.
    bars.push(bar(
        (1_704_232_800 + 1_800) * 1_000_000_000,
        TimeFrame::H1,
        5000,
        100,
    ));
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &calendar(),
        &QaOptions::default(),
        &mut report,
    );
    assert_eq!(report.unexpected_bar_count, 1);
    assert_eq!(report.unexpected_bars.len(), 1);
}

// 2024-03-10 is the spring-forward Sunday, so trading day 2024-03-11 opens at
// 22:00 UTC where the previous day opened at 23:00.
#[test]
fn the_daylight_saving_boundary_is_found() {
    let cal = calendar();
    // Session for Monday 2024-03-11 opens 2024-03-10 17:00 CDT = 22:00 UTC.
    let intervals = cal.open_intervals(CivilDate {
        year: 2024,
        month: 3,
        day: 11,
    });
    let start = intervals[0].0.0;
    let bars: Vec<Bar> = (0..40)
        .map(|i| bar(start - 20 * HOUR_NS + i * HOUR_NS, TimeFrame::H1, 5000, 10))
        .collect();
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &cal,
        &QaOptions::default(),
        &mut report,
    );
    let found = report
        .dst_boundaries
        .iter()
        .find(|b| b.date.year == 2024 && b.date.month == 3 && b.date.day == 11);
    let boundary = found.expect("the spring transition should be reported");
    assert_eq!(boundary.previous_open_hour_utc, 23);
    assert_eq!(boundary.open_hour_utc, 22);
}

#[test]
fn zero_volume_runs_are_grouped_and_ranked() {
    let mut bars = hourly_session(10);
    for i in [2, 3, 4, 7] {
        bars[i].volume = 0;
    }
    let mut report = report_for(&bars);
    check_zero_volume(&bars, &QaOptions::default(), &mut report);
    assert_eq!(report.zero_volume_bars, 4);
    assert_eq!(report.zero_volume_runs.len(), 2);
    assert_eq!(report.zero_volume_runs[0].bars, 3, "longest run first");
    assert_eq!(
        report.zero_volume_runs[0].from_ts,
        Ts(SESSION_OPEN_NS + 2 * HOUR_NS)
    );
    assert_eq!(report.zero_volume_runs[1].bars, 1);
}

#[test]
fn a_zero_volume_run_at_the_end_is_not_dropped() {
    let mut bars = hourly_session(5);
    bars[4].volume = 0;
    let mut report = report_for(&bars);
    check_zero_volume(&bars, &QaOptions::default(), &mut report);
    assert_eq!(report.zero_volume_runs.len(), 1);
    assert_eq!(report.zero_volume_bars, 1);
}

// Hand-derived. Closes step by 1 point per bar except one 40-point jump, so
// the absolute moves are twenty-odd 1s and a single 40. The median absolute
// move is therefore 1 point, robust sigma is 1 * 1.4826 = 1.4826, and the jump
// is 40 / 1.4826 = 26.98 sigma — far above the default 8.
#[test]
fn a_single_large_move_is_reported_as_a_spike() {
    let mut bars = hourly_session(23);
    for (i, b) in bars.iter_mut().enumerate() {
        #[expect(clippy::cast_possible_wrap, reason = "test index")]
        let close = 5000 + i as i64;
        b.close = Price::from_points(close);
    }
    // Push bar 12 forty points away, then continue from there.
    for b in bars.iter_mut().skip(12) {
        b.close = Price::from_nanos(b.close.as_nanos() + 40 * 1_000_000_000);
    }
    let mut report = report_for(&bars);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);

    let sigma = report.robust_sigma_points.expect("moves exist");
    assert!((sigma - 1.482_602).abs() < 1e-5, "sigma was {sigma}");
    assert_eq!(report.spikes.len(), 1);
    assert!((report.spikes[0].move_points - 41.0).abs() < 1e-9);
    assert_eq!(report.spikes[0].ts_open, Ts(SESSION_OPEN_NS + 12 * HOUR_NS));
}

// A jump across a gap is the overnight move, not a spike. Counting it as one
// would both raise a false alarm and inflate the scale estimate that finds the
// real ones.
#[test]
fn a_jump_across_a_gap_is_not_a_spike() {
    let mut bars = hourly_session(23);
    for (i, b) in bars.iter_mut().enumerate() {
        #[expect(clippy::cast_possible_wrap, reason = "test index")]
        let close = 5000 + i as i64;
        b.close = Price::from_points(close);
    }
    // Remove one bar to create a gap, and make the move across it enormous.
    bars.remove(12);
    for b in bars.iter_mut().skip(12) {
        b.close = Price::from_nanos(b.close.as_nanos() + 500 * 1_000_000_000);
    }
    let mut report = report_for(&bars);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);
    assert!(
        report.spikes.is_empty(),
        "the 500-point move spans a gap and must be ignored: {:?}",
        report.spikes
    );
}

#[test]
fn a_flat_series_makes_no_spike_claim() {
    let bars = hourly_session(23);
    let mut report = report_for(&bars);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);
    assert!(report.robust_sigma_points.is_none());
    assert!(report.spikes.is_empty());
}

#[test]
fn self_contradictory_bars_are_flagged() {
    let mut bars = hourly_session(3);
    bars[1].high = Price::from_points(4990);
    bars[1].low = Price::from_points(5010);
    let mut report = report_for(&bars);
    check_ohlc(&bars, &mut report);
    assert!(!report.ohlc_violations.is_empty());
    assert!(
        report
            .ohlc_violations
            .iter()
            .all(|v| v.ts_open == bars[1].ts_open)
    );
}

#[test]
fn a_consistent_bar_is_not_flagged() {
    let mut bars = hourly_session(1);
    bars[0].open = Price::from_points(5000);
    bars[0].high = Price::from_points(5010);
    bars[0].low = Price::from_points(4990);
    bars[0].close = Price::from_points(5005);
    let mut report = report_for(&bars);
    check_ohlc(&bars, &mut report);
    assert!(report.ohlc_violations.is_empty());
}

#[test]
fn vendor_conditions_are_read_and_only_the_bad_ones_reported() {
    let dir = crate::testutil::TempDir::new();
    let job = dir.path().join(DELIVERY_DIR).join("GLBX-20260101-ABCDEF");
    std::fs::create_dir_all(&job).expect("create delivery dir");
    std::fs::write(
        job.join("condition.json"),
        r#"[
          {"date":"2024-01-02","condition":"available","last_modified_date":"2024-01-03"},
          {"date":"2024-01-03","condition":"degraded","last_modified_date":"2024-01-04"},
          {"date":"2024-01-04","condition":"missing"}
        ]"#,
    )
    .expect("write condition.json");

    let mut report = blank_report();
    read_conditions(dir.path(), &mut report).expect("conditions read");
    assert_eq!(report.condition_sources, vec!["GLBX-20260101-ABCDEF"]);
    assert_eq!(report.conditions.len(), 2);
    assert_eq!(report.conditions[0].date, "2024-01-03");
    assert_eq!(report.conditions[0].condition, "degraded");
    assert_eq!(report.conditions[1].condition, "missing");
}

// A vendor file we cannot parse is worth mentioning, not worth failing over:
// the bars are the evidence, condition.json is the courtesy.
#[test]
fn an_unparseable_condition_file_is_noted_not_fatal() {
    let dir = crate::testutil::TempDir::new();
    let job = dir.path().join(DELIVERY_DIR).join("JOB");
    std::fs::create_dir_all(&job).expect("create delivery dir");
    std::fs::write(job.join("condition.json"), "{not json").expect("write");

    let mut report = blank_report();
    read_conditions(dir.path(), &mut report).expect("must not fail");
    assert_eq!(report.conditions.len(), 0);
    assert!(report.condition_sources[0].contains("unreadable"));
}

#[test]
fn a_missing_delivery_directory_is_not_an_error() {
    let dir = crate::testutil::TempDir::new();
    let mut report = blank_report();
    read_conditions(dir.path(), &mut report).expect("an archive may have no deliveries");
    assert!(report.condition_sources.is_empty());
}

#[test]
fn a_clean_report_says_so() {
    let bars = hourly_session(23);
    let mut report = report_for(&bars);
    check_ohlc(&bars, &mut report);
    check_zero_volume(&bars, &QaOptions::default(), &mut report);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &calendar(),
        &QaOptions::default(),
        &mut report,
    );
    assert!(report.is_clean(), "{report}");
    assert!(report.to_string().contains("coverage"));
}

// --- the two-interval session ------------------------------------------
//
// Hand-derived. With a 12:00–13:00 CT halt, trading day 2024-01-02 splits into
//   [17:00 CST Jan 1, 12:00 CST Jan 2) = 23:00 .. 18:00 UTC = 19 hourly bars
//   [13:00 CST Jan 2, 16:00 CST Jan 2) = 19:00 .. 22:00 UTC =  3 hourly bars
// = 22 expected bars, and one hour (18:00 UTC) that must not contain any.

/// The 22 bars a halted trading day 2024-01-02 should contain.
fn halted_session_bars() -> Vec<Bar> {
    let mut bars = Vec::new();
    for i in 0..19 {
        bars.push(bar(SESSION_OPEN_NS + i * HOUR_NS, TimeFrame::H1, 5000, 100));
    }
    // Skip 18:00 UTC (the halt), resume at 19:00 UTC.
    for i in 20..23 {
        bars.push(bar(SESSION_OPEN_NS + i * HOUR_NS, TimeFrame::H1, 5000, 100));
    }
    bars
}

#[test]
fn a_halt_splits_the_expected_bars_without_creating_a_gap() {
    let bars = halted_session_bars();
    assert_eq!(bars.len(), 22);
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &halted_calendar(),
        &QaOptions::default(),
        &mut report,
    );
    assert_eq!(report.expected_bars, Some(22));
    assert_eq!(
        report.missing_bars, 0,
        "the halt is not a gap: {:?}",
        report.gaps
    );
    assert_eq!(report.unexpected_bar_count, 0);
}

// A bar stamped inside the halt is out of session, and must not be counted as
// present by the interval walk either.
#[test]
fn a_bar_inside_the_halt_is_out_of_session() {
    let mut bars = halted_session_bars();
    bars.insert(
        19,
        bar(SESSION_OPEN_NS + 19 * HOUR_NS, TimeFrame::H1, 5000, 100),
    );
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &halted_calendar(),
        &QaOptions::default(),
        &mut report,
    );
    assert_eq!(report.expected_bars, Some(22));
    assert_eq!(report.missing_bars, 0);
    assert_eq!(report.unexpected_bar_count, 1);
}

// A hole in the SECOND interval is the case that catches a cursor which does
// not resume correctly after the halt.
#[test]
fn a_hole_after_the_halt_is_still_found() {
    let mut bars = halted_session_bars();
    // Drop 20:00 UTC, the middle bar of the post-halt interval.
    bars.retain(|b| b.ts_open.0 != SESSION_OPEN_NS + 21 * HOUR_NS);
    assert_eq!(bars.len(), 21);
    let mut report = report_for(&bars);
    check_against_calendar(
        &bars,
        TimeFrame::H1,
        &halted_calendar(),
        &QaOptions::default(),
        &mut report,
    );
    assert_eq!(report.expected_bars, Some(22));
    assert_eq!(report.missing_bars, 1);
    assert_eq!(report.gaps.len(), 1);
    assert_eq!(report.gaps[0].from_ts, Ts(SESSION_OPEN_NS + 21 * HOUR_NS));
    assert_eq!(report.unexpected_bar_count, 0);
}

// ---------------------------------------------------------------------------
// Every check emits an outcome. Never silence. (D-0099)
//
// The defect these controls exist for: a check whose precondition failed used
// to `return` with nothing printed, so an automated read scored the missing
// field as zero and a human read it as "nothing to report". A contract that was
// never examined looked exactly like a clean one — 44 of 68 ZN contracts and
// 5.7 % of the archive's bars were reported clean without being checked.
// ---------------------------------------------------------------------------

/// PLANTED CONTROL: a fixture that trips the zero-scale precondition must
/// carry a skip marker naming the reason, and must NOT look clean.
#[test]
fn planted_a_zero_scale_contract_reports_a_skip_and_never_reads_as_clean() {
    // Every bar closes at the same price, so every move is exactly zero and
    // the median absolute move is zero — the ZN case, in miniature.
    let bars = hourly_session(23);
    let mut report = report_for(&bars);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);

    // No claim is made, as before.
    assert!(report.robust_sigma_points.is_none());
    assert!(report.spikes.is_empty());

    // But the absence is now RECORDED, which is the whole point.
    let skipped = report.skipped_checks();
    assert_eq!(skipped.len(), 1, "exactly one check was skipped");
    assert_eq!(skipped[0].check, "spikes");
    assert!(
        matches!(
            skipped[0].skipped,
            Some(SkipReason::ZeroScale { moves: 22 })
        ),
        "expected ZeroScale over 22 moves, got {:?}",
        skipped[0].skipped
    );

    // And it is VISIBLE: the rendered report must say so in words a reader
    // cannot mistake for a pass.
    let rendered = report.to_string();
    assert!(rendered.contains("SKIPPED"), "{rendered}");
    assert!(
        rendered.contains("NOT a clean result"),
        "a skipped check must not read as a passing one: {rendered}"
    );
}

/// The converse: a contract that CAN be measured reports no skip at all.
///
/// Without this, a build that marked every check skipped would pass the control
/// above perfectly.
#[test]
fn converse_a_measurable_contract_records_no_skip() {
    let mut bars = hourly_session(23);
    // Give it a real scale: alternate the closes so the median move is nonzero.
    for (i, b) in bars.iter_mut().enumerate() {
        let bump = if i % 2 == 0 { 0 } else { 1 };
        b.close = Price::from_points(5000 + bump);
    }
    let mut report = report_for(&bars);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);

    assert!(
        report.robust_sigma_points.is_some(),
        "the fixture must be measurable or the control proves nothing"
    );
    assert!(
        report.skipped_checks().is_empty(),
        "a check that ran must not report a skip: {:?}",
        report.checks
    );
    assert!(!report.to_string().contains("SKIPPED"));
}

/// The other precondition — too few usable moves — is recorded too.
///
/// `moves.len() < 3` will keep skipping contracts however the estimator
/// changes, so it needs its own marker rather than being folded into the
/// zero-scale one.
#[test]
fn planted_a_too_thin_contract_reports_its_own_distinct_skip_reason() {
    let bars = hourly_session(2); // one move, needs three
    let mut report = report_for(&bars);
    check_spikes(&bars, TimeFrame::H1, &QaOptions::default(), &mut report);

    let skipped = report.skipped_checks();
    assert_eq!(skipped.len(), 1);
    assert!(
        matches!(
            skipped[0].skipped,
            Some(SkipReason::TooFewMoves { have: 1, need: 3 })
        ),
        "got {:?}",
        skipped[0].skipped
    );
    // The two reasons must stay distinguishable: they want different responses.
    assert_ne!(
        format!("{}", skipped[0].skipped.clone().expect("skipped")),
        format!("{}", SkipReason::ZeroScale { moves: 1 }),
    );
}

/// A contract with no calendar reports coverage as skipped, not as clean.
///
/// This is the shape that hid D-0072: gold had no bundled calendar, `qa`
/// printed `calendar none`, and a partition holding two decades concatenated
/// passed every visible check.
#[test]
fn planted_a_contract_with_no_calendar_reports_coverage_as_skipped() {
    let bars = hourly_session(23);
    let mut report = report_for(&bars);
    report.skip("coverage", SkipReason::NoCalendar);

    let rendered = report.to_string();
    assert!(rendered.contains("coverage"), "{rendered}");
    assert!(rendered.contains("SKIPPED"), "{rendered}");
    assert!(
        rendered.contains("no bundled calendar"),
        "the reason must name itself: {rendered}"
    );
}
