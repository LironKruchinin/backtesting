//! Telemetry tests, including the one that matters operationally: the
//! heartbeat must be readable **while a writer holds it**, because the whole
//! point of the file is to spare anyone the temptation to open the journal.

use super::*;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("crucible-telemetry-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn the_heartbeat_is_overwritten_not_appended() {
    let d = tmp("heartbeat-overwrite");
    write_heartbeat(
        &d,
        1_000,
        Counts {
            attempted: 25,
            written: 20,
            empty: 3,
            refused: 2,
        },
    );
    let first = std::fs::metadata(heartbeat_path(&d))
        .expect("written")
        .len();
    write_heartbeat(
        &d,
        2_000,
        Counts {
            attempted: 50,
            written: 44,
            empty: 4,
            refused: 2,
        },
    );
    let second = std::fs::metadata(heartbeat_path(&d))
        .expect("written")
        .len();

    let body = std::fs::read_to_string(heartbeat_path(&d)).expect("read");
    assert!(body.contains("ts=2000"), "{body}");
    assert!(
        !body.contains("ts=1000"),
        "the first beat must be gone: {body}"
    );
    assert!(
        second < first * 3,
        "the file must not grow without bound: {first} then {second}"
    );
}

#[test]
fn the_heartbeat_carries_every_count_the_inventory_cannot() {
    let d = tmp("heartbeat-counts");
    write_heartbeat(
        &d,
        7,
        Counts {
            attempted: 100,
            written: 90,
            empty: 6,
            refused: 4,
        },
    );
    let body = std::fs::read_to_string(heartbeat_path(&d)).expect("read");
    for expected in [
        "ts=7",
        "attempted=100",
        "written=90",
        "empty=6",
        "refused=4",
    ] {
        assert!(body.contains(expected), "missing {expected} in {body}");
    }
    // `refused` is the one the inventory deliberately does not record (D-0092).
    assert!(body.contains("refused=4"));
}

/// **The operational control.** A default Windows reader opened on a file a
/// writer holds can deny the writer and end it — that is how a live pull was
/// nearly lost once, and it is why the heartbeat exists at all. So the
/// heartbeat must survive being read while open for writing.
#[test]
fn the_heartbeat_is_readable_while_a_writer_holds_it() {
    let d = tmp("heartbeat-shared");
    let path = heartbeat_path(&d);
    write_heartbeat(
        &d,
        1,
        Counts {
            attempted: 1,
            written: 1,
            empty: 0,
            refused: 0,
        },
    );

    // Hold the file open for writing, exactly as a running pull would between
    // beats, then read it the way a monitor would.
    let mut held = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("a writer can hold it");

    let meta = std::fs::metadata(&path).expect("stat must work while held");
    assert!(
        meta.len() > 0,
        "stat is the cheap path and must always work"
    );
    let body = std::fs::read_to_string(&path).expect("read must work while a writer holds it");
    assert!(body.contains("ts=1"), "{body}");

    // And the writer is still usable afterwards — the reader did not evict it.
    held.write_all(b"").expect("the writer survived the read");
}

#[test]
fn every_exit_kind_has_a_distinct_spelling() {
    let kinds = [
        ExitKind::Completed,
        ExitKind::Halted,
        ExitKind::Failed,
        ExitKind::Panicked,
    ];
    let mut seen: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), before, "two exit kinds share a spelling");
}

#[test]
fn the_exit_record_is_one_json_object_and_replaces_the_previous_one() {
    let d = tmp("exit-replace");
    assert!(write_last_exit(
        &d,
        10,
        ExitKind::Failed,
        "disk full",
        Counts {
            attempted: 5,
            written: 1,
            empty: 0,
            refused: 4,
        },
    ));
    assert!(write_last_exit(
        &d,
        20,
        ExitKind::Completed,
        "",
        Counts {
            attempted: 83_489,
            written: 63_702,
            empty: 18_993,
            refused: 794,
        },
    ));

    let body = std::fs::read_to_string(last_exit_path(&d)).expect("read");
    assert_eq!(body.lines().count(), 1, "a snapshot, not a log: {body}");
    assert!(!body.contains("disk full"), "the earlier exit must be gone");
    assert!(body.contains("\"kind\":\"completed\""), "{body}");
    assert!(body.contains("\"refused\":794"), "{body}");
    assert!(body.contains("\"schema_version\":1"), "{body}");
}

#[test]
fn a_reason_containing_quotes_or_newlines_stays_one_valid_line() {
    // Refusal reasons are vendor strings and validator messages; both contain
    // quotes in practice, and a newline would turn a snapshot into two records.
    let d = tmp("exit-escape");
    assert!(write_last_exit(
        &d,
        1,
        ExitKind::Halted,
        "refusal rate 2.1% — validator said \"header drift\"\nsecond line",
        Counts {
            attempted: 200,
            written: 0,
            empty: 0,
            refused: 5,
        },
    ));
    let body = std::fs::read_to_string(last_exit_path(&d)).expect("read");
    assert_eq!(body.lines().count(), 1, "newline must not split it: {body}");
    assert!(
        body.contains("\\\"header drift\\\""),
        "quotes escaped: {body}"
    );
    assert!(!body.contains("\n\"reason\""), "{body}");
}

/// Both files live under `{data_dir}/telemetry/`, which `layout-check` knows
/// as a root entry (D-0098).
///
/// The previous shape put them bare at the archive root, where `layout-check`
/// correctly refused them and exited 4 on every run — training a reader to
/// ignore a red check, which is worse than the thing the check was watching.
#[test]
fn the_files_live_under_the_telemetry_directory_and_are_named_predictably() {
    let d = tmp("paths");
    assert_eq!(
        heartbeat_path(&d).file_name().expect("name"),
        "heartbeat.txt"
    );
    assert_eq!(
        last_exit_path(&d).file_name().expect("name"),
        "last_exit.json"
    );
    // One directory, archive-wide, and NOT the data dir root.
    let dir = d.join(TELEMETRY_DIR);
    assert_eq!(heartbeat_path(&d).parent(), Some(dir.as_path()));
    assert_eq!(last_exit_path(&d).parent(), Some(dir.as_path()));
    assert_ne!(
        heartbeat_path(&d).parent(),
        Some(d.as_path()),
        "a bare file at the archive root is what layout-check refuses"
    );
}

/// The writers create `telemetry/` themselves.
///
/// Both are best-effort and swallow their errors, so a missing parent would
/// mean writing nothing at all — silently, which is the exact failure mode this
/// module exists to end.
#[test]
fn the_writers_create_the_telemetry_directory_when_it_is_absent() {
    let d = tmp("creates-dir");
    assert!(
        !d.join(TELEMETRY_DIR).exists(),
        "the fixture must start without it"
    );
    write_heartbeat(&d, 1, Counts::default());
    assert!(heartbeat_path(&d).exists(), "heartbeat must have landed");

    let e = tmp("creates-dir-exit");
    assert!(write_last_exit(
        &e,
        1,
        ExitKind::Completed,
        "",
        Counts::default()
    ));
    assert!(last_exit_path(&e).exists());
}
