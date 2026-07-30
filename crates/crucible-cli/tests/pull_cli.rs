//! Argument handling and refusals for `crucible pull`.
//!
//! These run the real binary, because what is being tested is *process*
//! behaviour: which exit code a refusal produces, and that a refusal happens
//! before anything reaches the network. The execute state machine itself is
//! tested offline in `crucible-data` against a faked provider; what cannot be
//! tested there is the wiring, and the wiring is where a mistyped flag turns
//! into a purchase.
//!
//! Every case clears `DATABENTO_API_KEY` and `CRUCIBLE_DATA_DIR` from the
//! inherited environment first (mirroring `env_file.rs`), so a developer's
//! real key can neither make this suite pass nor make it spend anything.
//!
//! Exit codes under test: 2 usage/config, 3 refused to spend. Codes 4
//! (provider failure) and 5 (in flight) need a provider, so they are covered
//! by the library suite instead.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII temp dir. Std-only; `tempfile` is not in the blessed set (§6).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-cli-pull-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The arguments of a minimal, valid one-month ES pull.
fn base_args() -> Vec<&'static str> {
    vec![
        "pull",
        "--dataset",
        "GLBX.MDP3",
        "--schema",
        "ohlcv-1m",
        "--symbols",
        "ES.FUT",
        "--start",
        "2024-01-01",
        "--end",
        "2024-02-01",
    ]
}

/// Runs the binary with a cleared environment plus `overrides`.
///
/// The working directory is a fresh temp dir so the repo's own `.env` cannot
/// reach in and supply a key or a data dir.
fn run(dir: &Path, args: &[&str], overrides: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_crucible"));
    cmd.args(args)
        .current_dir(dir)
        .env_remove("DATABENTO_API_KEY")
        .env_remove("CRUCIBLE_DATA_DIR");
    for (key, value) in overrides {
        cmd.env(key, value);
    }
    cmd.output().expect("run the crucible binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Adds flags to the base pull invocation.
fn with(extra: &[&str]) -> Vec<String> {
    base_args()
        .into_iter()
        .map(ToOwned::to_owned)
        .chain(extra.iter().map(|s| (*s).to_owned()))
        .collect()
}

fn as_refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

// D-0024: spending needs an explicit cap, and the absence of one is a
// refusal rather than a default. `gate` owns the rule; this asserts the CLI
// surfaces it as its own exit code rather than swallowing it.
#[test]
fn execute_without_a_cap_is_refused_with_the_spending_exit_code() {
    let dir = TempDir::new();
    let args = with(&["--execute"]);
    let out = run(
        dir.path(),
        &as_refs(&args),
        &[
            ("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy()),
            ("DATABENTO_API_KEY", "db-0000000000000000000000000000000"),
        ],
    );
    // Refused from the arguments alone, so the answer is the same in every
    // build and does not depend on the key being usable: the fake key above
    // would fail authentication, and this must be decided before that
    // matters.
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert!(stderr(&out).contains("--max-cost-usd"), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("401") && !stderr(&out).contains("credentials"),
        "the cap check must not require a working API key: {}",
        stderr(&out)
    );
}

// A cap on a dry run is a sign the operator thinks they are buying. Better a
// loud refusal than a silent no-op that reads like a purchase.
#[test]
fn a_cap_without_execute_is_a_usage_error() {
    let dir = TempDir::new();
    let args = with(&["--max-cost-usd", "1.00"]);
    let out = run(
        dir.path(),
        &as_refs(&args),
        &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("--max-cost-usd"), "{}", stderr(&out));
}

#[test]
fn dry_run_and_execute_together_are_a_usage_error() {
    let dir = TempDir::new();
    let args = with(&["--dry-run", "--execute", "--max-cost-usd", "1.00"]);
    let out = run(
        dir.path(),
        &as_refs(&args),
        &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("contradict"), "{}", stderr(&out));
}

// D-0027: the cap is parsed as decimal text, never through an `f64`. A
// malformed cap must stop the command, not round to something.
#[test]
fn a_malformed_cap_stops_the_command() {
    let dir = TempDir::new();
    // `=`-joined so clap hands every value to our parser rather than reading
    // a leading `-` as the start of another flag.
    for bad in [
        "--max-cost-usd=abc",
        "--max-cost-usd=-1",
        "--max-cost-usd=1e3",
        "--max-cost-usd=0.0000000001",
    ] {
        let args = with(&["--execute", bad]);
        let out = run(
            dir.path(),
            &as_refs(&args),
            &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
        );
        assert_eq!(
            out.status.code(),
            Some(2),
            "cap {bad:?} should be a usage error: {}",
            stderr(&out)
        );
        assert!(
            stderr(&out).contains("--max-cost-usd"),
            "cap {bad:?}: {}",
            stderr(&out)
        );
    }
}

// Hinnant's date arithmetic happily turns month 13 into January of the next
// year. A pull that silently shifted its window would buy the wrong data, so
// the parser rejects rather than normalises.
#[test]
fn impossible_dates_are_rejected_before_anything_else() {
    let dir = TempDir::new();
    for (start, end) in [
        ("2024-13-01", "2024-02-01"),
        ("2023-02-29", "2023-03-01"),
        ("2024-01-01", "2024-04-31"),
        ("2024-1-1", "2024-02-01"),
        ("not-a-date", "2024-02-01"),
    ] {
        let args = vec![
            "pull",
            "--dataset",
            "GLBX.MDP3",
            "--schema",
            "ohlcv-1m",
            "--symbols",
            "ES.FUT",
            "--start",
            start,
            "--end",
            end,
        ];
        let out = run(
            dir.path(),
            &args,
            &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
        );
        assert_eq!(
            out.status.code(),
            Some(2),
            "{start}..{end} must be refused: {}",
            stderr(&out)
        );
    }
}

// The archive root has no default: a missing variable must not quietly
// become the working directory, which would scatter bought data anywhere the
// command happened to run.
#[test]
fn a_missing_data_dir_is_a_usage_error_not_a_default() {
    let dir = TempDir::new();
    let args = base_args();
    let out = run(dir.path(), &args, &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("CRUCIBLE_DATA_DIR"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn verify_reports_a_clean_empty_archive() {
    let dir = TempDir::new();
    let out = run(
        dir.path(),
        &["verify"],
        &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("archive clean"), "{}", stdout(&out));
}

#[test]
fn verify_without_a_data_dir_is_a_usage_error() {
    let dir = TempDir::new();
    let out = run(dir.path(), &["verify"], &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

#[test]
fn symbol_supplement_without_a_data_dir_is_a_usage_error() {
    let dir = TempDir::new();
    let out = run(dir.path(), &["symbol-supplement"], &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

// An empty archive has no records, so nothing can be short: exit 0, and — in a
// default build — the refusal to guess without a DBN decoder, also 2. Which of
// the two applies depends on the feature, and both are usage-level answers, so
// the assertion is on the pair rather than on one code.
#[test]
fn symbol_supplement_on_an_empty_archive_does_not_fail() {
    let dir = TempDir::new();
    let out = run(
        dir.path(),
        &["symbol-supplement"],
        &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
    );
    let code = out.status.code();
    assert!(
        code == Some(0) || code == Some(2),
        "expected 0 (nothing to do) or 2 (build without the databento feature), \
         got {code:?}: {}",
        stderr(&out)
    );
}

// A dry run must never write. The manifest is the archive's most load-bearing
// file, so "look first" has to be the default rather than a flag.
#[test]
fn symbol_supplement_dry_run_creates_no_manifest() {
    let dir = TempDir::new();
    let out = run(
        dir.path(),
        &["symbol-supplement"],
        &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
    );
    assert!(out.status.code().is_some(), "the process ran");
    assert!(
        !dir.path().join("manifest.jsonl").exists(),
        "a dry run must not create a manifest"
    );
}

// The determinism gate reads this: one line, one hash, nothing else. Adding
// `clap` restructured argument parsing, so this pins the output shape the CI
// double-run compares.
#[test]
fn demo_hash_only_still_prints_exactly_one_bare_hash_line() {
    let dir = TempDir::new();
    let out = run(dir.path(), &["demo", "--hash-only"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "expected one line, got {text:?}");
    assert_eq!(lines[0].len(), 16, "expected 16 hex chars: {:?}", lines[0]);
    assert!(
        lines[0].bytes().all(|b| b.is_ascii_hexdigit()),
        "not hex: {:?}",
        lines[0]
    );
}

// Bare `crucible` is what a new reader types first; it has not failed at
// anything, so it exits 0.
#[test]
fn no_arguments_prints_help_and_succeeds() {
    let dir = TempDir::new();
    let out = run(dir.path(), &[], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("pull"), "{text}");
    assert!(text.contains("CRUCIBLE_DATA_DIR"), "{text}");
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let dir = TempDir::new();
    let out = run(dir.path(), &["nonesuch"], &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

// A build without the feature must say so plainly rather than failing
// somewhere further in with a confusing message.
#[cfg(not(feature = "databento"))]
#[test]
fn a_build_without_the_feature_says_which_feature_is_missing() {
    let dir = TempDir::new();
    let args = base_args();
    let out = run(
        dir.path(),
        &args,
        &[("CRUCIBLE_DATA_DIR", &dir.path().to_string_lossy())],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("--features databento"),
        "{}",
        stderr(&out)
    );
}
