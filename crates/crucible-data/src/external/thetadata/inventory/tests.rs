//! Inventory round-trips, and the two failure modes that make resume honest.

use super::*;
use crate::testutil::TempDir;

fn report() -> ValidationReport {
    ValidationReport {
        raw_rows: 5912,
        distinct_rows: 2956,
        n_builds_distribution: BTreeMap::from([(2, 2956)]),
        identical_pairs: 2956,
        conflicting_pairs: 0,
        sentinel_rows_dropped: 0,
    }
}

fn record(request: &str) -> InventoryRecord {
    InventoryRecord::new(
        "/option/history/eod",
        "SPY",
        "eod",
        "2014-07-02",
        "2014-07-02",
        request,
        "external/thetadata/options/SPY/eod/2014-07-02.parquet",
        "5b1e0f4c",
        123_456,
        &report(),
        Some(Reconciliation {
            eod_and_greeks: 0,
            eod_without_greeks: 2956,
            oi_in_eod: 2221,
            eod_without_oi: 735,
        }),
        1_404_259_200_000_000_000,
    )
}

#[test]
fn a_record_carries_the_era_fingerprint_the_files_cannot() {
    let r = record("/option/history/eod?symbol=SPY&start_date=20140702");
    // The measured SPY 2014-07-02 shape (§3.1): 5,912 rows, 2,956 distinct.
    assert_eq!(r.row_count, 5912);
    assert_eq!(r.distinct_contracts, 2956);
    assert!((r.dup_rate - 2.0).abs() < 1e-9);

    let line = r.to_json_line();
    assert!(line.contains("\"dup_rate\":2.000"), "{line}");
    assert!(
        line.contains("\"n_builds_distribution\":{\"2\":2956}"),
        "{line}"
    );
    assert!(line.contains("\"oi_in_eod\":2221"), "{line}");
    assert!(line.starts_with('{') && line.ends_with('}'));
    assert!(!line.contains('\n'), "one record is one line");
}

// Once a file is deduplicated on the way to disk, nothing on disk remembers it
// arrived duplicated. If this field were dropped, "was this era duplicated?"
// would stop being answerable from the archive — and D-0054 makes that
// question load-bearing for every aggregate built below the greeks floor.
#[test]
fn the_post_2022_era_is_distinguishable_from_the_earlier_one_in_the_line() {
    let clean = ValidationReport {
        raw_rows: 9528,
        distinct_rows: 9528,
        n_builds_distribution: BTreeMap::from([(1, 9528)]),
        ..ValidationReport::default()
    };
    let r = InventoryRecord::new(
        "/option/history/eod",
        "SPY",
        "eod",
        "2022-01-03",
        "2022-01-03",
        "/option/history/eod?symbol=SPY&start_date=20220103",
        "external/thetadata/options/SPY/eod/2022-01-03.parquet",
        "aa",
        1,
        &clean,
        None,
        0,
    );
    assert!(r.to_json_line().contains("\"dup_rate\":1.000"));
    assert!(r.to_json_line().contains("\"reconciliation\":null"));
}

#[test]
fn appending_then_reading_round_trips_the_request_keys() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    assert!(
        inventory
            .completed_requests()
            .expect("absent is empty")
            .is_empty()
    );

    for n in 0..3 {
        inventory
            .append(&record(&format!(
                "/option/history/eod?symbol=SPY&start_date=2014070{n}"
            )))
            .expect("append");
    }
    let done = inventory.completed_requests().expect("read");
    assert_eq!(done.len(), 3);
    assert_eq!(
        done[0],
        "/option/history/eod?symbol=SPY&start_date=20140700"
    );
}

// Resume is a diff over requests, and the diff is what makes a re-run cheap.
#[test]
fn outstanding_returns_only_what_the_inventory_lacks() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    let planned: Vec<String> = (0..4)
        .map(|n| format!("/option/history/eod?symbol=SPY&start_date=2014070{n}"))
        .collect();

    inventory.append(&record(&planned[0])).expect("append");
    inventory.append(&record(&planned[2])).expect("append");

    let outstanding = inventory.outstanding(&planned).expect("diff");
    assert_eq!(outstanding, vec![planned[1].clone(), planned[3].clone()]);
}

// THE failure this design exists for. A run dies mid-write, leaving a
// half-written final line and possibly a half-written data file. A
// directory-listing resume sees the file and skips it — a silent gap. A diff
// resume sees an incomplete line, ignores it, and re-fetches. Re-fetching costs
// one request; the alternative costs a hole nobody ever notices.
#[test]
fn a_truncated_final_line_is_skipped_so_the_request_is_redone() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    let good = "/option/history/eod?symbol=SPY&start_date=20140702";
    inventory.append(&record(good)).expect("append");

    // Simulate the crash: append a line that stops mid-object.
    let torn = "{\"schema_version\":1,\"endpoint\":\"/option/history/eod\",\"root\":\"QQ";
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(inventory.path())
        .expect("open");
    file.write_all(torn.as_bytes()).expect("write");
    drop(file);

    let done = inventory.completed_requests().expect("read past the tear");
    assert_eq!(done, vec![good.to_owned()], "only the complete line counts");

    let planned = vec![good.to_owned(), "/option/history/eod?symbol=QQQ".to_owned()];
    assert_eq!(
        inventory.outstanding(&planned).expect("diff"),
        vec!["/option/history/eod?symbol=QQQ".to_owned()],
        "the torn record's request is still outstanding"
    );
}

// The resume key is the request, never the file path. Two requests that would
// write to the same path must stay two units of work, and the same request must
// be recognised wherever its output landed.
#[test]
fn the_resume_key_is_the_request_and_not_the_path() {
    let r = record("/option/history/eod?symbol=SPY&start_date=20140702");
    assert_eq!(r.resume_key(), r.request);
    assert_ne!(r.resume_key(), r.file_path);
}

// A request carrying a quote or a backslash must not be able to end the JSON
// string early and turn one line into two.
#[test]
fn strings_are_escaped_so_a_record_cannot_forge_a_second_line() {
    let nasty = "/option/history/eod?symbol=\"SP\\Y\"\nnot-a-record";
    let line = record(nasty).to_json_line();
    assert!(!line.contains('\n'), "still one line: {line}");
    assert_eq!(
        extract_string_field(&line, "request").as_deref(),
        Some(nasty),
        "and it reads back exactly"
    );
}

// A non-finite ratio would render as bare `NaN`, which is not JSON. It cannot
// arise from the current arithmetic, and that is precisely why the guard is
// worth having: nobody will notice when it starts to.
#[test]
fn a_non_finite_ratio_writes_null_rather_than_invalid_json() {
    assert_eq!(format_ratio(f64::NAN), "null");
    assert_eq!(format_ratio(f64::INFINITY), "null");
    assert_eq!(format_ratio(2.0), "2.000");
}
