//! Process-level tests for `crucible walk-forward`.
//!
//! Same shape as `combo_cli.rs`, and for the same reason: what these pin is
//! the contract with a *researcher*. Which fold layouts are refused before a
//! bar is replayed, whether the refusal says how to fix itself, whether the
//! report says what window each number came from, and whether two runs of the
//! same config produce the same numbers. None of that is observable from
//! inside the library.
//!
//! `CRUCIBLE_DATA_DIR` and `DATABENTO_API_KEY` are cleared in every case, so
//! the repo's own `.env` cannot reach in and quietly supply an archive.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII temp dir, mirroring `combo_cli.rs`: pid plus a process-wide counter
/// for uniqueness without randomness or clocks, and `create_dir` (not `_all`)
/// so a leftover directory is skipped rather than silently reused.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-cli-wf-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    fn config(&self, body: &str) -> PathBuf {
        let path = self.path.join("wf.toml");
        std::fs::write(&path, body).expect("write config");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crucible"))
        .args(args)
        .env_remove("DATABENTO_API_KEY")
        .env_remove("CRUCIBLE_DATA_DIR")
        .output()
        .expect("failed to run the crucible binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn shipped(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../configs")
        .join(name)
}

/// Six synthetic days of 1m bars, with the `[walk_forward]` block left to
/// substitute so each test changes exactly the thing it is about.
fn template(walk_forward: &str) -> String {
    format!(
        r#"
schema_version = 1

[meta]
name = "t"
hypothesis_family = "t-family"
economic_rationale = "test fixture"

[universe]
instruments = ["SYN:RW"]
timeframes = ["1m"]

[data]
source = "synthetic"
seed = 7
bars = 8640
start_price_points = "5000"
vol_ticks = 4

[contract]
tick_points = "0.25"
point_value_usd = 50

[indicators.fast]
kind = "sma"
period = 5

[indicators.slow]
kind = "sma"
period = [20, 30]

[rules]
enter_long = "fast crosses_above slow"
exit_long = "fast crosses_below slow"

[execution]
fill_model = "spread_cross"
half_spread_ticks = 1
fee_per_contract_usd = "1.25"

{walk_forward}

[run]
seed = 42
initial_cash_usd = "100000"
qty_contracts = 1
"#
    )
}

/// The shipped runnable config walks forward, and the report says which
/// window every number came from.
#[test]
fn the_smoke_config_walks_forward() {
    let out = run(&[
        "walk-forward",
        "--config",
        shipped("combo-smoke.toml").to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("trading days (D-0062)"), "{text}");
    assert!(text.contains("OUT OF SAMPLE"), "{text}");
    assert!(text.contains("test window (OOS)"), "{text}");
    // The fold table names dates, not just bar counts: a window nobody can
    // locate on a calendar is not a window anyone can check.
    assert!(text.contains(".."), "{text}");
    // And the honesty box survives.
    assert!(text.contains("EVIDENCE, NOT A VERDICT"), "{text}");
    assert!(text.contains("fill model     spread_cross"), "{text}");
}

/// A random walk under costs must lose money out of sample. If this ever
/// prints a positive OOS return, the engine has a bug — there is no edge in
/// the data (D-0011).
#[test]
fn the_null_harness_finds_nothing_out_of_sample() {
    let out = run(&[
        "walk-forward",
        "--config",
        shipped("combo-smoke.toml").to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let text = stdout(&out);
    let table = text
        .split("OUT OF SAMPLE")
        .nth(1)
        .and_then(|rest| rest.split("`whole-run`").next())
        .expect("the report has an out-of-sample table");
    let rows = table
        .lines()
        .filter(|l| l.trim_start().starts_with(char::is_numeric));
    let mut n = 0usize;
    for line in rows {
        n += 1;
        assert!(
            line.contains('-'),
            "a combo made money out of sample on a random walk, under costs: {line}"
        );
    }
    assert_eq!(
        n, 6,
        "the smoke grid has six combos, and all six were checked"
    );
}

/// Two runs of the same config produce the same report, to the bit.
#[test]
fn a_walk_forward_is_bit_identical_across_runs() {
    let path = shipped("combo-smoke.toml");
    let arg = path.to_str().expect("utf-8 path");
    let a = run(&["walk-forward", "--config", arg, "--hash-only"]);
    let b = run(&["walk-forward", "--config", arg, "--hash-only"]);
    assert_eq!(code(&a), 0, "{}", stderr(&a));
    assert_eq!(stdout(&a), stdout(&b));
    assert_eq!(stdout(&a).trim().len(), 16, "a 64-bit hash in hex");
}

/// The hash covers the fold layout, not only the equity: moving a boundary
/// while leaving each window's arithmetic alone must not go unnoticed.
#[test]
fn changing_the_fold_layout_changes_the_hash() {
    let dir = TempDir::new();
    let hash = |wf: &str| {
        let path = dir.config(&template(wf));
        let out = run(&[
            "walk-forward",
            "--config",
            path.to_str().expect("utf-8 path"),
            "--hash-only",
        ]);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        stdout(&out)
    };
    let a = hash("[walk_forward]\nscheme = \"rolling\"\ntrain_days = 2\ntest_days = 1\n");
    let b = hash("[walk_forward]\nscheme = \"anchored\"\ntrain_days = 2\ntest_days = 1\n");
    assert_ne!(a, b, "anchored and rolling train on different windows");
}

/// Overlapping test windows are refused, and the message says why rather than
/// just that.
#[test]
fn overlapping_test_windows_are_refused_with_their_reason() {
    let dir = TempDir::new();
    let path = dir.config(&template(
        "[walk_forward]\nscheme = \"rolling\"\ntrain_days = 2\ntest_days = 2\nstep_days = 1\n",
    ));
    let out = run(&[
        "walk-forward",
        "--config",
        path.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2, "{}", stdout(&out));
    let err = stderr(&out);
    assert!(err.contains("overlap"), "{err}");
    assert!(err.contains("twice"), "{err}");
}

/// A span too short for even one fold says how many days it had and how many
/// it needed.
#[test]
fn too_short_a_span_says_what_it_needed() {
    let dir = TempDir::new();
    let path = dir.config(&template(
        "[walk_forward]\nscheme = \"rolling\"\ntrain_days = 40\ntest_days = 10\n",
    ));
    let out = run(&[
        "walk-forward",
        "--config",
        path.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2, "{}", stdout(&out));
    let err = stderr(&out);
    assert!(
        err.contains("50"),
        "it needed train + test = 50 days: {err}"
    );
    assert!(err.contains("Widen the date range"), "{err}");
}

/// A config with no `[walk_forward]` is a usage error that says what to add,
/// including the unit — the whole point of D-0062.
#[test]
fn a_config_without_folds_says_what_to_add() {
    let dir = TempDir::new();
    let path = dir.config(&template(""));
    let out = run(&[
        "walk-forward",
        "--config",
        path.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2, "{}", stdout(&out));
    let err = stderr(&out);
    assert!(err.contains("train_days"), "{err}");
    assert!(err.contains("TRADING"), "{err}");
}

/// A scheme this build does not implement is named back, with the ones it
/// does.
#[test]
fn an_unknown_scheme_names_the_ones_that_exist() {
    let dir = TempDir::new();
    let path = dir.config(&template(
        "[walk_forward]\nscheme = \"expanding\"\ntrain_days = 2\ntest_days = 1\n",
    ));
    let out = run(&[
        "walk-forward",
        "--config",
        path.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2, "{}", stdout(&out));
    let err = stderr(&out);
    assert!(err.contains("expanding"), "{err}");
    assert!(err.contains("rolling"), "{err}");
    assert!(err.contains("anchored"), "{err}");
}

/// The old month-denominated spelling is a hard error rather than a silently
/// ignored field (§5.5): a config carrying `train_months` was written against
/// a layout this build no longer implements, and running it under a default
/// would be worse than refusing (D-0062).
#[test]
fn the_month_denominated_spelling_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&template(
        "[walk_forward]\nscheme = \"rolling\"\ntrain_months = 24\ntest_months = 6\n",
    ));
    let out = run(&[
        "walk-forward",
        "--config",
        path.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2, "{}", stdout(&out));
    let err = stderr(&out);
    assert!(err.contains("unknown field"), "{err}");
    assert!(
        err.contains("train_days"),
        "the refusal must name the spelling that replaced it: {err}"
    );
}

/// `combo` still reports the whole series and still says so; the two commands
/// must not quietly print the same number under different headings.
#[test]
fn combo_and_walk_forward_disagree_and_both_say_why() {
    let path = shipped("combo-smoke.toml");
    let arg = path.to_str().expect("utf-8 path");

    let flat = run(&["combo", "--config", arg, "--run"]);
    assert_eq!(code(&flat), 0, "{}", stderr(&flat));
    let flat_text = stdout(&flat);
    assert!(flat_text.contains("D-0061"), "{flat_text}");
    assert!(
        flat_text.contains("crucible walk-forward"),
        "the flat runner should point at the one that slices: {flat_text}"
    );

    let wf = run(&["walk-forward", "--config", arg]);
    assert_eq!(code(&wf), 0, "{}", stderr(&wf));
    let wf_text = stdout(&wf);
    assert!(
        wf_text.contains("does not apply here"),
        "the sliced runner should retire the caveat: {wf_text}"
    );
    // Same combos, different numbers: the whole-run column is printed beside
    // the out-of-sample one precisely so the gap is visible.
    assert!(wf_text.contains("whole-run"), "{wf_text}");
}
