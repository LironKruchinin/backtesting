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
        identical_repeats_collapsed: 0,
        sentinel_rows_dropped: 0,
        // The measured VIX-style shape: most of a chain does not trade on a
        // given day, so a high rate is ordinary and only a *change* is a
        // finding (D-0055).
        zero_ohlc_rows: 1800,
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

// The two halves of the retry, and they must both be asserted: a retry loop
// nobody has seen survive a real lock is decoration, and one nobody has seen
// give up is a hang waiting to happen.
//
// Windows-only because the failure is Windows-only: these tests open with
// `share_mode(0)`, and POSIX has no mandatory sharing to reproduce.
#[cfg(windows)]
mod deny_share {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    /// Opens `path` denying every other handle — what `Get-Content` and
    /// `System.IO.StreamReader` do by default, and what killed a live run.
    fn deny_share_handle(path: &std::path::Path) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .expect("the exclusive reader must open")
    }

    #[test]
    fn append_survives_a_transient_deny_share_reader() {
        let dir = TempDir::new();
        let inventory = Inventory::open(dir.path());
        // The file must exist for a reader to lock it at all.
        inventory.append(&record("seed")).expect("seed appends");

        let path = inventory.path().to_path_buf();
        let held = deny_share_handle(&path);
        let releaser = std::thread::spawn(move || {
            // Well inside the ~4.5 s window, well outside one attempt.
            std::thread::sleep(std::time::Duration::from_millis(1200));
            drop(held);
        });

        inventory
            .append(&record("survived"))
            .expect("a glance at progress must not kill the run");
        releaser.join().expect("releaser thread");

        let requests = inventory.completed_requests().expect("readable");
        assert_eq!(requests, vec!["seed".to_owned(), "survived".to_owned()]);
    }

    #[test]
    fn append_still_fails_on_a_persistent_deny_share_reader() {
        let dir = TempDir::new();
        let inventory = Inventory::open(dir.path());
        inventory.append(&record("seed")).expect("seed appends");

        // Held across the whole call: the attempts are spent and never succeed.
        let _held = deny_share_handle(inventory.path());
        let error = inventory
            .append(&record("doomed"))
            .expect_err("a permanent holder must end the run, not hang it");

        match error {
            ThetaError::Io { during, source, .. } => {
                assert!(
                    during.contains("deny-share"),
                    "the message must name the cause: {during}"
                );
                assert_eq!(source.raw_os_error(), Some(32), "sharing violation");
            }
            other => panic!("expected an Io error, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Absence records (D-0093): the third thing a line can say, and the arithmetic
// that closes the 794.
// ---------------------------------------------------------------------------

/// An absence record round-trips, and reads back with its cause intact.
#[test]
fn an_absence_record_round_trips_with_its_cause_and_detail() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    let record = InventoryRecord::absence(
        "/option/history/greeks/eod",
        "SPX",
        "daily",
        "2021-03-02",
        "2021-03-02",
        "/option/history/greeks/eod?symbol=SPX&expiration=*&start_date=20210302",
        AbsenceCause::SolverSentinel,
        "answered with 5946 rows that are entirely zero",
        1_700_000_000_000_000_000,
    );
    inventory.append(&record).expect("append");

    let back = inventory.read_all().expect("read");
    assert_eq!(back.len(), 1);
    assert!(back[0].is_absence());
    assert_eq!(back[0].absence, Some(AbsenceCause::SolverSentinel));
    assert!(back[0].absence_detail.contains("entirely zero"));
    // No file is claimed, which is what makes it an absence rather than a
    // record of bytes nobody can find.
    assert!(back[0].file_path.is_empty());
    assert_eq!(back[0].row_count, 0);
}

/// **THE 794 → 0 ARITHMETIC.** Absence records satisfy plan-minus-inventory,
/// so a resume over the same plan issues nothing.
///
/// This is the whole remedy stated as a test: before absence records existed a
/// refusal appended nothing, so every one of the 794 stayed outstanding and was
/// re-issued on every run forever. The count is the real one deliberately —
/// a fixture of 3 would pass while proving nothing about the case that matters.
#[test]
fn absence_records_satisfy_the_plan_and_drive_outstanding_to_zero() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());

    let plan: Vec<String> = (0..794)
        .map(|n| format!("/option/history/greeks/eod?symbol=SPX&start_date=2021{n:04}"))
        .collect();

    // Before: the whole plan is outstanding.
    assert_eq!(
        inventory.outstanding(&plan).expect("outstanding").len(),
        794,
        "nothing recorded yet"
    );

    for (n, request) in plan.iter().enumerate() {
        // Two causes, mixed, because the real set is mixed.
        let cause = if n % 3 == 0 {
            AbsenceCause::AmbiguousDuplicate
        } else {
            AbsenceCause::SolverSentinel
        };
        inventory
            .append(&InventoryRecord::absence(
                "/option/history/greeks/eod",
                "SPX",
                "daily",
                "2021-03-02",
                "2021-03-02",
                request,
                cause,
                "measured cause",
                1,
            ))
            .expect("append");
    }

    // After: nothing is outstanding, and NOTHING was fetched to achieve it.
    assert_eq!(
        inventory.outstanding(&plan).expect("outstanding").len(),
        0,
        "an absence record settles its request"
    );

    // And "0 outstanding" is never allowed to read as "everything acquired":
    // the causes are counted and stay countable.
    let by_cause = inventory.absences_by_cause().expect("absences");
    assert_eq!(by_cause[&AbsenceCause::AmbiguousDuplicate], 265);
    assert_eq!(by_cause[&AbsenceCause::SolverSentinel], 529);
    assert_eq!(by_cause.values().sum::<u64>(), 794);
}

/// A line written before absence records existed reads as data, not absence.
///
/// The reader-first obligation (CLAUDE.md §8) in one assertion: every one of
/// the 82,668 lines already in the archive predates the `absence` field, and
/// each must keep meaning exactly what it meant.
#[test]
fn a_v1_line_without_the_absence_field_reads_as_data() {
    let dir = TempDir::new();
    let inventory = Inventory::open(dir.path());
    let path = inventory.path();
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    // Hand-written in the pre-absence shape, field-for-field.
    std::fs::write(
        path,
        "{\"schema_version\":1,\"endpoint\":\"/option/history/eod\",\"root\":\"SPY\",\
         \"grain\":\"daily\",\"start_date\":\"2024-01-02\",\"end_date\":\"2024-01-02\",\
         \"request\":\"/option/history/eod?symbol=SPY\",\"file_path\":\"a.parquet\",\
         \"file_blake3\":\"ab\",\"size_bytes\":10,\"row_count\":2,\"distinct_contracts\":1,\
         \"dup_rate\":2.000,\"n_builds_distribution\":{\"2\":1},\"conflicting_pairs\":0,\
         \"sentinel_rows_dropped\":0,\"fetch_millis\":5,\"zero_ohlc_rate\":0.500,\
         \"reconciliation\":null,\"fetched_ts\":7}\n",
    )
    .expect("write");

    let back = inventory.read_all().expect("read");
    assert_eq!(back.len(), 1);
    assert!(
        !back[0].is_absence(),
        "a v1 line is data; reading it as absence would retire a request nobody settled"
    );
    assert_eq!(back[0].absence, None);
    assert!(back[0].absence_detail.is_empty());
    assert_eq!(back[0].file_path, "a.parquet");
    // It still satisfies resume, exactly as it always did.
    assert_eq!(
        inventory
            .outstanding(&["/option/history/eod?symbol=SPY".to_owned()])
            .expect("outstanding")
            .len(),
        0
    );
}
