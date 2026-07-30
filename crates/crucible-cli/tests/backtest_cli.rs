//! Process-level tests for `crucible backtest` and `crucible transcode`.
//!
//! These run the real binary because what they pin is the *contract with an
//! operator*: which mistakes are caught before any file is opened, what exit
//! code each produces, and whether the error says how to fix itself. None of
//! that is observable from inside the library, and all of it is what a cron
//! job or a runbook depends on.
//!
//! Every case runs in a fresh temp directory with `DATABENTO_API_KEY` and
//! `CRUCIBLE_DATA_DIR` cleared, so the repo's own `.env` cannot reach in and
//! quietly supply the archive the test is asserting is absent.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII temp dir, mirroring `crucible-data`'s `testutil::TempDir`: pid plus a
/// process-wide counter for uniqueness without randomness or clocks, and
/// `create_dir` (not `_all`) so a leftover directory is skipped rather than
/// silently reused. Std-only — `tempfile` is not in the blessed set (§6).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("crucible-cli-backtest-{}-{n}", std::process::id()));
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

/// Runs the binary with a cleared environment plus `overrides`.
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

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A minimal, valid backtest invocation over the archive at `dir`.
fn base_args() -> Vec<&'static str> {
    vec!["backtest", "--instrument", "ESH4"]
}

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

fn data_dir(dir: &TempDir) -> [(&str, String); 1] {
    [(
        "CRUCIBLE_DATA_DIR",
        dir.path().to_string_lossy().into_owned(),
    )]
}

fn refs<'a>(env: &'a [(&'a str, String); 1]) -> Vec<(&'a str, &'a str)> {
    env.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

// The archive root has no default anywhere in the CLI, and inventing one
// would put market data inside the repo.
#[test]
fn a_missing_data_dir_is_a_usage_error() {
    let dir = TempDir::new();
    let out = run(dir.path(), &base_args(), &[]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("CRUCIBLE_DATA_DIR"),
        "{}",
        stderr(&out)
    );
}

// Timeframe spellings are pinned (CLAUDE.md §4). A typo must not fall back to
// a default interval — that would silently backtest the wrong bars.
#[test]
fn an_unknown_timeframe_is_a_usage_error() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    let args = with(&["--timeframe", "2m"]);
    let out = run(dir.path(), &as_refs(&args), &refs(&env));
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("1s 1m 5m 15m 1h 1d"),
        "{}",
        stderr(&out)
    );
}

// Caught from the arguments alone, before any file is opened: a crossover
// where the fast leg is not faster has no meaning, and `SmaCross::new` would
// panic on it rather than explain.
#[test]
fn a_fast_period_that_is_not_faster_is_a_usage_error() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    for pair in [
        ["--fast", "50", "--slow", "20"],
        ["--fast", "50", "--slow", "50"],
    ] {
        let args = with(&pair);
        let out = run(dir.path(), &as_refs(&args), &refs(&env));
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        assert!(stderr(&out).contains("--fast"), "{}", stderr(&out));
    }
}

#[test]
fn one_date_without_the_other_is_a_usage_error() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    for lone in [["--start", "2024-01-01"], ["--end", "2024-02-01"]] {
        let args = with(&lone);
        let out = run(dir.path(), &as_refs(&args), &refs(&env));
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        assert!(stderr(&out).contains("go together"), "{}", stderr(&out));
    }
}

#[test]
fn impossible_dates_are_rejected_before_anything_else() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    for range in [
        ["--start", "2024-02-30", "--end", "2024-03-01"],
        ["--start", "2024-13-01", "--end", "2024-12-01"],
        ["--start", "2024-02-01", "--end", "2024-01-01"],
    ] {
        let args = with(&range);
        let out = run(dir.path(), &as_refs(&args), &refs(&env));
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    }
}

// A tick size is a price that decides fill prices, so it is parsed as an
// integer and a typo is a hard error rather than a silent NaN (§2.3).
#[test]
fn a_malformed_contract_spec_is_a_usage_error() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    for bad in [
        vec!["--tick-points", "quarter"],
        vec!["--tick-points", "0"],
        vec!["--tick-points", "2.5e-1"],
        vec!["--fee-per-contract-usd", "-1.25"],
        vec!["--initial-cash-usd", "one hundred k"],
        vec!["--qty", "0"],
    ] {
        let args = with(&bad);
        let out = run(dir.path(), &as_refs(&args), &refs(&env));
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {}", stderr(&out));
    }
}

// A bracket distance of zero puts the level on the entry price and a negative
// one puts it on the wrong side of the market. `Bracket::new` asserts on both,
// so catching them here is the difference between an explanation and a panic
// backtrace (CLAUDE.md §5.1).
#[test]
fn a_nonpositive_bracket_distance_is_a_usage_error() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    for bad in [
        vec!["--stop-ticks", "0"],
        vec!["--stop-ticks", "-8"],
        vec!["--target-ticks", "0"],
        vec!["--target-ticks", "-12"],
    ] {
        let args = with(&bad);
        let out = run(dir.path(), &as_refs(&args), &refs(&env));
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {}", stderr(&out));
        assert!(
            stderr(&out).contains("positive tick distance"),
            "{bad:?}: {}",
            stderr(&out)
        );
    }
}

// "not found" has to be actionable: curated data is built, not downloaded.
#[test]
fn missing_curated_data_names_the_command_that_builds_it() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    let out = run(dir.path(), &base_args(), &refs(&env));
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("crucible transcode"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn help_lists_the_new_commands() {
    let dir = TempDir::new();
    let out = run(dir.path(), &[], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    for command in ["transcode", "backtest", "pull", "verify", "demo"] {
        assert!(text.contains(command), "help should list {command}: {text}");
    }
    // `transcode` graduated out of the PLANNED list when it was implemented.
    assert!(
        !text.contains("transcode   M1"),
        "transcode should no longer be listed as planned: {text}"
    );
}

// A build without the vendor client cannot decode DBN at all, so it must say
// which feature is missing instead of failing at the first file (D-0031).
#[cfg(not(feature = "databento"))]
#[test]
fn transcode_without_the_feature_says_which_feature_is_missing() {
    let dir = TempDir::new();
    let env = data_dir(&dir);
    let out = run(dir.path(), &["transcode"], &refs(&env));
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("--features databento"),
        "{}",
        stderr(&out)
    );
}
