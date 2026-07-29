//! Layout-check tests.
//!
//! CLAUDE.md §7: a detector nobody has seen fire is decoration. There is one
//! **negative control** per violation class below — a synthetic archive with
//! exactly that fault planted — and each asserts the specific finding, not
//! merely that something was reported. The happy-path archive is the same
//! shape as the real one, so a rule that would reject `G:/Crucible` fails here
//! first.

use super::*;

use crate::catalog::ManifestRecord;
use crate::testutil::TempDir;

/// Builds an archive tree from a list of relative paths. A path ending in `/`
/// is a directory; anything else is a file with one byte in it.
fn build(paths: &[&str]) -> TempDir {
    let dir = TempDir::new();
    for path in paths {
        let full = dir.path().join(path.trim_end_matches('/'));
        if path.ends_with('/') {
            std::fs::create_dir_all(&full).expect("mkdir");
        } else {
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&full, b"x").expect("write");
        }
    }
    dir
}

/// The shape a healthy archive has, mirroring the live one.
const HEALTHY: &[&str] = &[
    "manifest.jsonl",
    "jobs.jsonl",
    "pull.lock",
    "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst",
    "raw/GLBX.MDP3/definition/ES.FUT/2010-06-06--2026-07-28.dbn.zst",
    "curated/bars/ESH4/1m/2024-01.parquet",
    "curated/rolls/ES/1m/v-confirm1.json",
    "staging/",
    "delivery/GLBX-20260728-TQE7SY3VTC/condition.json",
    "external/cboe/VIX_History.csv",
];

fn record(file_path: &str) -> ManifestRecord {
    ManifestRecord {
        schema_version: 1,
        dataset: "GLBX.MDP3".to_owned(),
        schema: "ohlcv-1m".to_owned(),
        symbols: vec!["ES.FUT".to_owned()],
        start_ts: crucible_core::types::Ts(0),
        end_ts: crucible_core::types::Ts(1),
        acquired_ts: crucible_core::types::Ts(0),
        databento_job_id: "JOB".to_owned(),
        file_path: file_path.to_owned(),
        file_blake3: "0".repeat(64),
        size_bytes: 1,
    }
}

fn check_ok(dir: &TempDir, records: &[ManifestRecord]) -> LayoutReport {
    check(dir.path(), records).expect("the tree is walkable")
}

// --- happy path ---------------------------------------------------------

#[test]
fn a_healthy_archive_is_clean() {
    let dir = build(HEALTHY);
    let report = check_ok(
        &dir,
        &[
            record("raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"),
            record("raw/GLBX.MDP3/definition/ES.FUT/2010-06-06--2026-07-28.dbn.zst"),
        ],
    );
    assert!(report.is_clean(), "{report}");
    assert_eq!(report.raw_files, 2);
    assert_eq!(report.curated_files, 2);
    assert_eq!(report.records, 2);
    assert_eq!(
        report.schemas_seen,
        vec!["GLBX.MDP3/definition", "GLBX.MDP3/ohlcv-1m"]
    );
    assert_eq!(report.kinds_seen, vec!["bars", "rolls"]);
}

// `staging/`, `delivery/` and `external/` hold working files and foreign data
// whose shapes this project does not define, so they are walked past rather
// than judged. A rule invented purely to be enforced is worse than no rule.
#[test]
fn working_directories_and_external_data_are_not_judged() {
    let dir = build(&[
        "staging/abc123/anything/at/any/depth.tmp",
        "delivery/JOB/metadata.json",
        "external/thetadata/options/SPY/1m/2024.csv",
        "external/cboe/README.md",
    ]);
    let report = check_ok(&dir, &[]);
    assert!(report.is_clean(), "{report}");
}

#[test]
fn an_empty_archive_is_clean() {
    let dir = TempDir::new();
    let report = check_ok(&dir, &[]);
    assert!(report.is_clean(), "{report}");
    assert_eq!(report.raw_files, 0);
}

#[test]
fn a_missing_data_dir_is_an_error_not_a_clean_result() {
    let dir = TempDir::new();
    let gone = dir.path().join("not-there");
    assert!(matches!(
        check(&gone, &[]),
        Err(LayoutError::MissingDataDir { .. })
    ));
}

// --- negative controls, one per violation class -------------------------

// Class 1: a file at the wrong depth under raw/.
#[test]
fn detects_a_raw_file_at_the_wrong_depth() {
    // Missing the symbol level: dataset/schema/file instead of
    // dataset/schema/symbol/file.
    let dir = build(&["raw/GLBX.MDP3/ohlcv-1m/2024-01.dbn.zst"]);
    let report = check_ok(&dir, &[]);
    assert_eq!(
        report.findings,
        vec![Finding::RawWrongDepth {
            path: "raw/GLBX.MDP3/ohlcv-1m/2024-01.dbn.zst".to_owned(),
            depth: 3,
            expected: 4,
        }]
    );
    assert!(!report.is_clean());
}

#[test]
fn detects_a_raw_file_buried_too_deep() {
    let dir = build(&["raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024/01.dbn.zst"]);
    let report = check_ok(&dir, &[]);
    // The extra directory and the file inside it are both wrong, and both are
    // reported — an operator needs the extent, not the first instance.
    assert!(
        report.findings.iter().any(|f| matches!(
            f,
            Finding::RawWrongDepth { path, depth: 4, expected: 3 } if path == "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024/"
        )),
        "{report}"
    );
    assert!(
        report.findings.iter().any(|f| matches!(
            f,
            Finding::RawWrongDepth {
                depth: 5,
                expected: 4,
                ..
            }
        )),
        "{report}"
    );
}

// Class 2: a schema directory that is not a known vendor schema.
#[test]
fn detects_an_unknown_schema_directory() {
    // `ohclv-1m` — the transposition that looks right in a listing.
    let dir = build(&["raw/GLBX.MDP3/ohclv-1m/ES.FUT/2024-01.dbn.zst"]);
    let report = check_ok(&dir, &[]);
    assert!(
        report.findings.contains(&Finding::UnknownSchemaDir {
            dataset: "GLBX.MDP3".to_owned(),
            schema: "ohclv-1m".to_owned(),
        }),
        "{report}"
    );
}

// Class 3: a manifest file_path that does not parse.
#[test]
fn detects_an_unparseable_manifest_path() {
    let dir = build(HEALTHY);
    let report = check_ok(
        &dir,
        &[
            record("raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"),
            record("raw/GLBX.MDP3/ES.FUT/2024-01.dbn.zst"),
            record("curated/bars/ESH4/1m/2024-01.parquet"),
        ],
    );
    let paths: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| matches!(f, Finding::ManifestPathUnparseable { .. }))
        .collect();
    assert_eq!(paths.len(), 2, "{report}");
    // Reported in record order, 1-based, so the manifest line can be found.
    assert!(matches!(
        paths[0],
        Finding::ManifestPathUnparseable { record: 2, .. }
    ));
    assert!(matches!(
        paths[1],
        Finding::ManifestPathUnparseable { record: 3, .. }
    ));
}

// Class 4a: a Parquet file under raw/.
#[test]
fn detects_parquet_under_raw() {
    let dir = build(&["raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.parquet"]);
    let report = check_ok(&dir, &[]);
    assert!(
        report.findings.contains(&Finding::WrongTree {
            path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.parquet".to_owned(),
            found_under: "raw",
            belongs_under: "curated",
        }),
        "{report}"
    );
}

// Class 4b: a DBN file under curated/.
#[test]
fn detects_dbn_under_curated() {
    let dir = build(&["curated/bars/ESH4/1m/2024-01.dbn.zst"]);
    let report = check_ok(&dir, &[]);
    assert!(
        report.findings.contains(&Finding::WrongTree {
            path: "curated/bars/ESH4/1m/2024-01.dbn.zst".to_owned(),
            found_under: "curated",
            belongs_under: "raw",
        }),
        "{report}"
    );
}

// Class 5: a curated file outside kind/instrument/tf.
#[test]
fn detects_a_curated_file_at_the_wrong_depth() {
    let dir = build(&["curated/bars/ESH4/2024-01.parquet"]);
    let report = check_ok(&dir, &[]);
    assert!(
        report.findings.contains(&Finding::CuratedWrongDepth {
            path: "curated/bars/ESH4/2024-01.parquet".to_owned(),
            depth: 3,
            expected: 4,
        }),
        "{report}"
    );
}

#[test]
fn detects_an_unknown_curated_kind() {
    let dir = build(&["curated/quotes/ESH4/1m/2024-01.parquet"]);
    let report = check_ok(&dir, &[]);
    assert!(
        report.findings.contains(&Finding::UnknownCuratedKind {
            kind: "quotes".to_owned(),
        }),
        "{report}"
    );
}

// Class 6: anything unrecognized at a tree root.
#[test]
fn detects_unknown_entries_at_the_archive_root() {
    let dir = build(&["scratch/notes.txt", "TODO.md"]);
    let report = check_ok(&dir, &[]);
    assert!(
        report.findings.contains(&Finding::UnknownRootEntry {
            name: "scratch".to_owned(),
            is_dir: true,
        }),
        "{report}"
    );
    assert!(
        report.findings.contains(&Finding::UnknownRootEntry {
            name: "TODO.md".to_owned(),
            is_dir: false,
        }),
        "{report}"
    );
    // An unknown root is not descended into: naming it once is the finding,
    // and its contents are not ours to have an opinion about.
    assert_eq!(report.findings.len(), 2, "{report}");
}

// --- path parsing -------------------------------------------------------

#[test]
fn raw_paths_parse_into_their_pieces() {
    let parsed = parse_raw_path("raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst")
        .expect("a well-formed raw path");
    assert_eq!(parsed.dataset, "GLBX.MDP3");
    assert_eq!(parsed.schema, "ohlcv-1m");
    assert_eq!(parsed.symbol, "ES.FUT");
    assert_eq!(parsed.window, "2024-01.dbn.zst");
}

#[test]
fn malformed_raw_paths_are_rejected_with_a_reason() {
    for (path, needle) in [
        ("curated/bars/ESH4/1m/x.parquet", "must start with raw/"),
        (
            "raw/GLBX.MDP3/ohlcv-1m/x.dbn.zst",
            "component(s); expected 5",
        ),
        (
            "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/extra/x.dbn.zst",
            "component(s); expected 5",
        ),
        ("raw/GLBX.MDP3/ohclv-1m/ES.FUT/x.dbn.zst", "not a known"),
        ("raw/GLBX.MDP3/ohlcv-1m/ES.FUT/x.parquet", "not DBN"),
        ("raw//ohlcv-1m/ES.FUT/x.dbn.zst", "empty path component"),
    ] {
        let err = parse_raw_path(path).expect_err(path);
        assert!(err.contains(needle), "{path}: {err}");
    }
}

// The layout check must never be the thing that stops a real archive working,
// so the shape the code actually writes has to pass it. These mirror
// `curated::path::partition_path` and `continuous::roll::roll_table_path`.
#[test]
fn the_shapes_this_codebase_writes_are_accepted() {
    let dir = build(&[
        "curated/bars/ESH4/1m/2024-01.parquet",
        "curated/bars/SYN%3ARW/1s/x.parquet",
        "curated/bars/ESH4-ESM4/1m/2024-01.parquet",
        "curated/rolls/ES/1m/v-confirm1.json",
        "curated/rolls/ES/1m/c-minus5d.json",
        "raw/GLBX.MDP3/mbo/ES.FUT/2026-06-28--2026-07-28.dbn.zst",
    ]);
    let report = check_ok(&dir, &[]);
    assert!(report.is_clean(), "{report}");
}

// The review that found this bug found it in the *rendered* message, not the
// enum — the existing negative controls asserted only the variant. A file one
// level too shallow used to print "sits 3 level(s) below raw/, not 3", which is
// nonsense, and `layout-check` deliberately has no --fix (D-0049), so the
// printed finding IS the whole remedy handed to a human.
#[test]
fn a_wrong_depth_message_names_the_depth_the_entry_should_have_had() {
    let raw_file = Finding::RawWrongDepth {
        path: "raw/GLBX.MDP3/ohlcv-1m/2024-01.dbn.zst".to_owned(),
        depth: 3,
        expected: 4,
    };
    let text = raw_file.to_string();
    assert!(
        text.contains("sits 3 component(s) below raw/, not 4"),
        "{text}"
    );

    let raw_dir = Finding::RawWrongDepth {
        path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024/".to_owned(),
        depth: 4,
        expected: 3,
    };
    assert!(
        raw_dir
            .to_string()
            .contains("sits 4 component(s) below raw/, not 3"),
        "{raw_dir}"
    );

    let curated = Finding::CuratedWrongDepth {
        path: "curated/bars/ESH4/2024-01.parquet".to_owned(),
        depth: 3,
        expected: 4,
    };
    assert!(
        curated
            .to_string()
            .contains("sits 3 component(s) below curated/, not 4"),
        "{curated}"
    );

    // Whatever else it says, it must never assert that a number is not itself.
    for finding in [raw_file, raw_dir, curated] {
        let t = finding.to_string();
        for n in 1..8 {
            assert!(
                !t.contains(&format!("{n} component(s) below raw/, not {n}"))
                    && !t.contains(&format!("{n} component(s) below curated/, not {n}")),
                "self-contradictory message: {t}"
            );
        }
    }
}
