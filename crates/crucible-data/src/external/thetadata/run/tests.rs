//! Ordering, refusal semantics, and the golden-raw census.
//!
//! The acquisition loop itself needs a live Terminal, so what is tested here is
//! everything that decides *what the loop does* — the parts that would be wrong
//! silently. The end-to-end path is covered by `crucible theta-golden`, which
//! runs the same fetch → validate → transcode → read-back chain against real
//! bytes and compares every cell.

use super::*;
use crate::calendar::Calendar;
use crate::external::thetadata::plan::t0;

fn day(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
}

fn plan_over(start: CivilDate, end: CivilDate, roots: &[&str]) -> TranchePlan {
    let calendar = Calendar::by_id("us_equity_options")
        .expect("bundled")
        .into_trading_days();
    super::super::plan::TrancheSpec {
        roots: roots.iter().map(|r| (*r).to_owned()).collect(),
        ..t0(start, end)
    }
    .expand(&calendar)
}

// -------------------------------------------------------------------------
// Golden-raw sampling: one day per root x type x year (§6).
// -------------------------------------------------------------------------

#[test]
fn golden_sampling_takes_one_day_per_root_type_and_year() {
    // Two roots, three endpoints, spanning two calendar years.
    let plan = plan_over(day(2023, 12, 1), day(2024, 2, 1), &["SPY", "QQQ"]);
    let golden = golden_sample_set(&plan);

    // 2 roots x 3 types x 2 years = 12.
    assert_eq!(golden.len(), 12);

    // And every (root, type, year) really is represented exactly once.
    let mut seen: std::collections::BTreeMap<(String, &str, i64), u32> =
        std::collections::BTreeMap::new();
    for request in &plan.requests {
        if golden.contains(&request.request.render()) {
            *seen
                .entry((
                    request.root.clone(),
                    type_segment(request.endpoint),
                    request.date.year,
                ))
                .or_default() += 1;
        }
    }
    assert_eq!(seen.len(), 12);
    assert!(
        seen.values().all(|n| *n == 1),
        "one sample each, never two: {seen:?}"
    );
}

// Deterministic, like everything else that decides what gets written: the
// sample is the first session of each year the plan contains, so two runs
// choose the same days and a re-run does not scatter extra copies.
#[test]
fn golden_sampling_is_deterministic_and_picks_the_first_session_of_each_year() {
    let plan = plan_over(day(2024, 1, 1), day(2024, 3, 1), &["SPY"]);
    assert_eq!(golden_sample_set(&plan), golden_sample_set(&plan));

    // 2024-01-01 is New Year's Day, so the first session is the 2nd.
    let first = plan
        .requests
        .iter()
        .find(|r| r.endpoint == Endpoint::OptionEod)
        .expect("eod present");
    assert_eq!(first.date, day(2024, 1, 2));
    assert!(golden_sample_set(&plan).contains(&first.request.render()));
}

// A golden sample is kept per TYPE, not per root-day. `eod` and `greeks/eod`
// for the same day are different response shapes with different pins, and a
// fidelity reference that held only one of them could not catch a drift in the
// other.
#[test]
fn each_endpoint_gets_its_own_golden_sample() {
    let plan = plan_over(day(2024, 1, 1), day(2024, 1, 10), &["SPY"]);
    let golden = golden_sample_set(&plan);
    let types: BTreeSet<&str> = plan
        .requests
        .iter()
        .filter(|r| golden.contains(&r.request.render()))
        .map(|r| type_segment(r.endpoint))
        .collect();
    assert_eq!(types.len(), 3, "eod, greeks_eod, open_interest");
}

// -------------------------------------------------------------------------
// Paths.
// -------------------------------------------------------------------------

// Two endpoints for one root-day must not collide. A flat {root}/{date} layout
// would have `eod` and `greeks/eod` overwrite each other, and the survivor
// would be whichever ran last — which is exactly the kind of ordering-dependent
// archive §2.2 exists to prevent.
#[test]
fn two_endpoints_for_one_root_day_write_to_different_paths() {
    let plan = plan_over(day(2024, 1, 1), day(2024, 1, 4), &["SPY"]);
    let root = Path::new("/data");
    let paths: BTreeSet<PathBuf> = plan
        .requests
        .iter()
        .filter(|r| r.date == day(2024, 1, 2))
        .map(|r| output_path(root, r))
        .collect();
    assert_eq!(paths.len(), 3, "one path per endpoint: {paths:?}");
}

#[test]
fn output_paths_follow_the_documented_layout() {
    let plan = plan_over(day(2024, 1, 1), day(2024, 1, 4), &["SPY"]);
    let eod = plan
        .requests
        .iter()
        .find(|r| r.endpoint == Endpoint::OptionEod && r.date == day(2024, 1, 2))
        .expect("present");
    let path = output_path(Path::new("/data"), eod);
    let rendered = path.to_string_lossy().replace('\\', "/");
    assert!(
        rendered.ends_with("external/thetadata/options/SPY/eod/daily/2024-01-02.parquet"),
        "{rendered}"
    );

    let golden = golden_path(Path::new("/data"), eod)
        .to_string_lossy()
        .replace('\\', "/");
    assert!(
        golden.ends_with("external/thetadata/golden_raw/SPY/eod/2024-01-02.csv"),
        "{golden}"
    );
}

// -------------------------------------------------------------------------
// Refusal semantics.
// -------------------------------------------------------------------------

// One bad vendor day must not kill a 60,000-request run, so a handful of
// refusals is survivable and must not trip anything.
#[test]
fn a_few_refusals_do_not_halt_a_long_run() {
    let mut report = RunReport {
        attempted: 10_000,
        ..RunReport::default()
    };
    for i in 0..20 {
        report.refusals.push(Refusal {
            request: format!("/x/{i}"),
            reason: "one bad day".to_owned(),
        });
    }
    assert!((report.refusal_rate() - 0.002).abs() < 1e-9);
    assert!(
        !report.refusal_rate_is_systemic(),
        "0.2% is a vendor hiccup, not a finding"
    );
}

// But a systematic misread must. Refusing one day in five is this build
// misunderstanding the feed, and continuing would produce a tranche whose gaps
// are our own bug wearing the vendor's clothes.
#[test]
fn a_systemic_refusal_rate_is_a_finding_and_halts() {
    let mut report = RunReport {
        attempted: 1_000,
        ..RunReport::default()
    };
    for i in 0..200 {
        report.refusals.push(Refusal {
            request: format!("/x/{i}"),
            reason: "header drift".to_owned(),
        });
    }
    assert!(report.refusal_rate_is_systemic());
}

// THE small-sample trap, and §0.4 in its arithmetic costume: three refusals in
// the first four requests is 75%, and tripping there would abort a healthy run
// over a coincidence. The floor is what stops it.
#[test]
fn planted_early_refusals_below_the_sample_floor_do_not_trip_the_breaker() {
    let mut report = RunReport {
        attempted: 4,
        ..RunReport::default()
    };
    for i in 0..3 {
        report.refusals.push(Refusal {
            request: format!("/x/{i}"),
            reason: "coincidence".to_owned(),
        });
    }
    assert!(report.refusal_rate() > REFUSAL_RATE_LIMIT, "75% > 2%");
    assert!(
        !report.refusal_rate_is_systemic(),
        "but four attempts prove nothing — the floor must hold"
    );

    // And at the floor it does trip, so the guard is not simply inert.
    let mut big = RunReport {
        attempted: REFUSAL_MIN_SAMPLE,
        ..RunReport::default()
    };
    for i in 0..REFUSAL_MIN_SAMPLE / 4 {
        big.refusals.push(Refusal {
            request: format!("/x/{i}"),
            reason: "systematic".to_owned(),
        });
    }
    assert!(big.refusal_rate_is_systemic());
}

#[test]
fn an_empty_run_has_no_refusal_rate_to_speak_of() {
    let report = RunReport::default();
    assert_eq!(report.refusal_rate(), 0.0);
    assert!(!report.refusal_rate_is_systemic());
}

// The refusal ledger is printed, because a refusal that is neither inventoried
// nor reported is simply lost — and the whole reason refusals are not
// inventoried is so a later run retries them.
#[test]
fn the_report_names_refusals_so_they_are_not_merely_dropped() {
    let mut report = RunReport {
        attempted: 3,
        written: 2,
        ..RunReport::default()
    };
    report.refusals.push(Refusal {
        request: "/option/history/eod?symbol=SPY&start_date=20240102".to_owned(),
        reason: "UnexpectedColumns".to_owned(),
    });
    let rendered = report.to_string();
    assert!(rendered.contains("resume retries these"), "{rendered}");
    assert!(rendered.contains("symbol=SPY"), "{rendered}");
    assert!(rendered.contains("UnexpectedColumns"), "{rendered}");
}
