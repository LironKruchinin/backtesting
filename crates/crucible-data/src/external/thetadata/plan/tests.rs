//! The four acceptance properties, each with the failure it exists to prevent.

use super::*;
use crate::calendar::Calendar;
use crate::testutil::TempDir;

fn day(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
}

fn calendar() -> TradingDayCalendar {
    Calendar::by_id("us_equity_options")
        .expect("bundled")
        .into_trading_days()
}

/// A small T0 over one week, so the assertions can be counted by hand.
fn one_week() -> TrancheSpec {
    TrancheSpec {
        roots: vec!["SPY".to_owned(), "QQQ".to_owned()],
        ..t0(day(2024, 1, 1), day(2024, 1, 8))
    }
}

// ---------------------------------------------------------------------------
// 1. Deterministic, idempotent expansion.
// ---------------------------------------------------------------------------

// The property that makes an inventory diffable at all. If two runs disagreed
// about the request list, a resumed run would re-fetch work the first had done
// and skip work it had not.
#[test]
fn the_same_spec_expands_to_an_identical_request_list_every_time() {
    let spec = one_week();
    let a = spec.expand(&calendar());
    let b = spec.expand(&calendar());
    assert_eq!(a.requests, b.requests);
    assert!(!a.requests.is_empty());

    // And the rendered strings — the actual resume keys — are identical.
    let render = |p: &TranchePlan| {
        p.requests
            .iter()
            .map(|r| r.request.render())
            .collect::<Vec<_>>()
    };
    assert_eq!(render(&a), render(&b));
}

// Hand-counted. 2024-01-01 is New Year's Day, so the week 1-7 Jan holds four
// sessions (Tue 2 - Fri 5); 6-7 Jan is a weekend. SPY is above its 2017-01-03
// greeks floor and QQQ is above its (it has none), so both get all three
// endpoints: 2 roots x 4 sessions x 3 endpoints = 24.
#[test]
fn expansion_covers_sessions_only_and_the_count_is_hand_checkable() {
    let plan = one_week().expand(&calendar());
    assert_eq!(plan.requests.len(), 24);
    assert!(
        plan.requests
            .iter()
            .all(|r| calendar().is_trading_day(r.date)),
        "a plan must never ask for a day the exchange was shut"
    );
    // New Year's Day and the weekend appear nowhere.
    for absent in [day(2024, 1, 1), day(2024, 1, 6), day(2024, 1, 7)] {
        assert!(plan.requests.iter().all(|r| r.date != absent), "{absent:?}");
    }
}

// Order is fixed and documented: root, then date ascending, then endpoint.
#[test]
fn expansion_order_is_root_then_date_then_endpoint() {
    let plan = one_week().expand(&calendar());
    assert_eq!(plan.requests[0].root, "SPY");
    assert_eq!(plan.requests[0].date, day(2024, 1, 2));
    assert_eq!(plan.requests[0].endpoint, Endpoint::OptionEod);
    assert_eq!(plan.requests[1].endpoint, Endpoint::OptionGreeksEod);
    assert_eq!(plan.requests[2].endpoint, Endpoint::OptionOpenInterest);
    assert_eq!(plan.requests[3].date, day(2024, 1, 3));
    // All of SPY precedes all of QQQ.
    let first_qqq = plan
        .requests
        .iter()
        .position(|r| r.root == "QQQ")
        .expect("QQQ present");
    assert!(plan.requests[..first_qqq].iter().all(|r| r.root == "SPY"));
}

// Below a root's greeks floor, `greeks/eod` is not requested — it would answer
// 472 every time. `eod` and `open_interest` still are: `eod` is the only source
// down there (D-0054's worst case) and OI is unaffected by the floor.
#[test]
fn greeks_are_not_requested_below_a_root_s_measured_floor() {
    let spec = TrancheSpec {
        roots: vec!["SPY".to_owned()],
        ..t0(day(2016, 6, 1), day(2016, 6, 8))
    };
    let plan = spec.expand(&calendar());
    assert!(
        plan.requests
            .iter()
            .all(|r| r.endpoint != Endpoint::OptionGreeksEod),
        "SPY has no greeks before 2017-01-03"
    );
    assert!(
        plan.requests
            .iter()
            .any(|r| r.endpoint == Endpoint::OptionEod)
    );
    assert!(
        plan.requests
            .iter()
            .any(|r| r.endpoint == Endpoint::OptionOpenInterest)
    );

    // QQQ has greeks at the subscription floor, so the same week does include
    // them — proving the gate is per root and not a date rule.
    let qqq = TrancheSpec {
        roots: vec!["QQQ".to_owned()],
        ..t0(day(2016, 6, 1), day(2016, 6, 8))
    }
    .expand(&calendar());
    assert!(
        qqq.requests
            .iter()
            .any(|r| r.endpoint == Endpoint::OptionGreeksEod)
    );
}

// ---------------------------------------------------------------------------
// 2. Resume is strictly an inventory diff.
// ---------------------------------------------------------------------------

#[test]
fn resume_subtracts_exactly_what_the_inventory_records() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    let plan = one_week().expand(&calendar());
    assert_eq!(
        plan.outstanding(&inventory).expect("diff").len(),
        24,
        "an empty inventory holds nothing back"
    );

    let done = &plan.requests[0];
    inventory
        .append(&super::super::InventoryRecord::new(
            done.endpoint.path(),
            &done.root,
            "eod",
            "2024-01-02",
            "2024-01-02",
            &done.request.render(),
            "external/thetadata/x.parquet",
            "aa",
            1,
            &super::super::validate::ValidationReport::default(),
            None,
            0,
        ))
        .expect("append");

    let outstanding = plan.outstanding(&inventory).expect("diff");
    assert_eq!(outstanding.len(), 23);
    assert!(
        outstanding
            .iter()
            .all(|r| r.request.render() != done.request.render())
    );
}

// THE failure this design exists for, restated at the plan level. A file that
// exists on disk but has no inventory line is a half-written file; resume must
// re-fetch it. Nothing in this module looks at the filesystem to decide.
#[test]
fn a_file_on_disk_with_no_inventory_line_is_still_outstanding() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    let plan = one_week().expand(&calendar());

    // Plant exactly the trap: the output file exists, the inventory does not
    // know about it.
    let planted = dir
        .path()
        .join("external")
        .join("thetadata")
        .join("options");
    std::fs::create_dir_all(&planted).expect("mkdir");
    std::fs::write(planted.join("SPY-2024-01-02.parquet"), b"half a file").expect("write");

    assert_eq!(
        plan.outstanding(&inventory).expect("diff").len(),
        24,
        "resume must not be fooled by a file it never recorded"
    );
}

// ---------------------------------------------------------------------------
// 3. The disk guard.
// ---------------------------------------------------------------------------

#[test]
fn a_healthy_volume_lets_the_run_proceed() {
    let dir = TempDir::new();
    let plan = one_week().expand(&calendar());
    let report = plan
        .dry_run_report(
            &Inventory::open(dir.path()),
            0,
            Some(900 * 1024 * 1024 * 1024),
        )
        .expect("report");
    assert!(report.may_execute(), "{}", report);
    assert_eq!(report.refusal, None);
}

#[test]
fn planted_low_free_space_refuses_before_anything_is_fetched() {
    let dir = TempDir::new();
    let plan = one_week().expand(&calendar());
    let report = plan
        .dry_run_report(
            &Inventory::open(dir.path()),
            0,
            Some(100 * 1024 * 1024 * 1024),
        )
        .expect("report");
    assert!(!report.may_execute());
    assert!(
        report
            .refusal
            .as_deref()
            .unwrap_or_default()
            .contains("400"),
        "{:?}",
        report.refusal
    );
}

// The guard checks the projection, not just today's free space. A run that
// clears the floor on its first file and crosses it on its last has not passed
// — and discovering that mid-tranche means a half-acquired tranche AND a full
// volume.
#[test]
fn planted_projection_that_would_cross_the_floor_refuses_up_front() {
    let dir = TempDir::new();
    let plan = t0(day(2012, 6, 1), day(2026, 7, 1)).expand(&calendar());
    let projected: u64 = plan.requests.iter().map(|r| r.projected_bytes).sum();
    // Free space that clears the floor now but not after the tranche lands.
    let free = MIN_FREE_BYTES + projected / 2;
    let report = plan
        .dry_run_report(&Inventory::open(dir.path()), 0, Some(free))
        .expect("report");
    assert!(!report.may_execute());
    assert!(
        report
            .refusal
            .as_deref()
            .unwrap_or_default()
            .contains("would leave less"),
        "{:?}",
        report.refusal
    );
}

#[test]
fn planted_stop_and_report_threshold_halts_further_acquisition() {
    let dir = TempDir::new();
    let plan = one_week().expand(&calendar());
    let report = plan
        .dry_run_report(
            &Inventory::open(dir.path()),
            STOP_AND_REPORT_BYTES,
            Some(900 * 1024 * 1024 * 1024),
        )
        .expect("report");
    assert!(!report.may_execute());
    assert!(
        report
            .refusal
            .as_deref()
            .unwrap_or_default()
            .contains("stop-and-report"),
        "{:?}",
        report.refusal
    );
}

// Unmeasurable free space is not "fine". The guard exists for the run nobody is
// watching, and a run that cannot check must not assume it would have passed —
// the §0.4 trap in its storage costume.
#[test]
fn unmeasurable_free_space_refuses_rather_than_assuming_the_best() {
    let dir = TempDir::new();
    let plan = one_week().expand(&calendar());
    let report = plan
        .dry_run_report(&Inventory::open(dir.path()), 0, None)
        .expect("report");
    assert!(!report.may_execute());
    assert!(
        report
            .refusal
            .as_deref()
            .unwrap_or_default()
            .contains("could not be measured"),
        "{:?}",
        report.refusal
    );
}

// ---------------------------------------------------------------------------
// 4. The dry run says everything, and costs nothing.
// ---------------------------------------------------------------------------

#[test]
fn the_dry_run_reports_counts_bytes_and_the_resume_split() {
    let dir = TempDir::new();
    let plan = one_week().expand(&calendar());
    let report = plan
        .dry_run_report(
            &Inventory::open(dir.path()),
            0,
            Some(900 * 1024 * 1024 * 1024),
        )
        .expect("report");

    assert_eq!(report.total_requests, 24);
    assert_eq!(report.already_held, 0);
    assert_eq!(report.outstanding, 24);
    assert_eq!(report.by_endpoint.len(), 3, "one line per endpoint");
    assert!(report.projected_bytes > 0);

    let rendered = report.to_string();
    assert!(rendered.contains("DRY RUN, nothing fetched"), "{rendered}");
    // A projection must never read as a measurement.
    assert!(
        rendered.contains("Projections, not measurements"),
        "{rendered}"
    );
    assert!(rendered.contains("guards clear"), "{rendered}");
}

// The whole-span T0, so the numbers the operator will actually see are
// exercised. This is the plan-then-execute artefact: if it is wrong, it is
// wrong here, for free, rather than four hours into an acquisition.
#[test]
fn the_full_t0_expansion_is_the_size_the_plan_doc_projects() {
    let plan = t0(day(2012, 6, 1), day(2026, 7, 1)).expand(&calendar());
    // Nine roots over 3,539 sessions. Every root gets eod + OI throughout, and
    // greeks only above its own floor, so the total sits between 2x and 3x
    // roots*sessions.
    let sessions = 3_539u64;
    let roots = 9u64;
    let lower = 2 * roots * sessions;
    let upper = 3 * roots * sessions;
    let actual = plan.requests.len() as u64;
    assert!(
        actual > lower && actual < upper,
        "{actual} requests should sit strictly between {lower} and {upper}"
    );

    let projected: u64 = plan.requests.iter().map(|r| r.projected_bytes).sum();
    // §7.3 puts T0 at 10-30 GB. This is a projection of a projection, so the
    // assertion is deliberately loose — it is here to catch an order-of-
    // magnitude error, not to pin a number nobody measured.
    let gib = 1024 * 1024 * 1024;
    assert!(
        projected > 5 * gib && projected < 60 * gib,
        "projected {} is outside the plausible band for T0",
        human_bytes(projected)
    );
}

#[test]
fn human_bytes_reads_the_way_an_operator_expects() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(2048), "2.0 kiB");
    assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
}
