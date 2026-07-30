//! Process-level tests for `crucible combo`.
//!
//! These run the real binary because what they pin is the contract with a
//! *researcher*: which config mistakes are caught before a single bar is
//! replayed, whether the message says how to fix itself, and whether two runs
//! of the same config produce the same numbers. None of that is observable
//! from inside the library.
//!
//! `CRUCIBLE_DATA_DIR` and `DATABENTO_API_KEY` are cleared in every case, so
//! the repo's own `.env` cannot reach in and quietly supply an archive.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII temp dir, mirroring `backtest_cli.rs`: pid plus a process-wide
/// counter for uniqueness without randomness or clocks, and `create_dir` (not
/// `_all`) so a leftover directory is skipped rather than silently reused.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-cli-combo-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    /// Writes a config and returns its path.
    fn config(&self, body: &str) -> PathBuf {
        let path = self.path.join("combo.toml");
        std::fs::write(&path, body).expect("write config");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Runs the binary with a cleared environment.
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

/// A config in the repo, by name.
fn shipped(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../configs")
        .join(name)
}

/// A minimal valid config with one field left to substitute, so each test
/// changes exactly the thing it is about.
const TEMPLATE: &str = r#"
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
bars = 300
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

[run]
seed = 42
initial_cash_usd = "100000"
qty_contracts = 1
"#;

/// The shipped reference config is the schema's own documentation, so it had
/// better parse. 7 bb periods × 3 k × 2 trend periods = 42 combos.
#[test]
fn the_reference_config_expands() {
    let out = run(&[
        "combo",
        "--config",
        &shipped("example-combo.toml").to_string_lossy(),
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("42 combos"), "{text}");
    assert!(text.contains("warmup 200 bars"), "{text}");
    // Identity is printed in full, never truncated.
    assert!(
        text.lines().any(|l| l.contains("config hash")
            && l.split_whitespace().nth(2).is_some_and(|h| h.len() == 64)),
        "{text}"
    );
    // Declared-but-unconsumed sections are named, not skipped past.
    assert!(text.contains("[walk_forward]"), "{text}");
    assert!(text.contains("[funnel]"), "{text}");
}

/// The runnable one replays, and its every combo loses money under costs —
/// it is a random walk, so anything else would be an engine bug.
#[test]
fn the_smoke_config_replays_and_finds_nothing() {
    let out = run(&[
        "combo",
        "--config",
        &shipped("combo-smoke.toml").to_string_lossy(),
        "--run",
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("6 combos"), "{text}");
    assert!(text.contains("20000 bars"), "{text}");
    assert!(
        !text.contains("  n/a") || text.contains("Sharpe"),
        "a replay must produce a table: {text}"
    );
    // §2.4: the intrabar ordering assumption is stated even when nothing in
    // this grid can trigger it. "No number printed" and "no ambiguous bars"
    // must not look the same to a reader.
    assert!(
        text.contains("no combo declared a stop or target"),
        "{text}"
    );
    assert!(text.contains("stop_first_intrabar"), "{text}");
}

/// The determinism gate: same config, same data, bit-identical hash.
#[test]
fn a_grid_replays_bit_identically_across_runs() {
    let path = shipped("combo-smoke.toml");
    let first = run(&["combo", "--config", &path.to_string_lossy(), "--hash-only"]);
    let second = run(&["combo", "--config", &path.to_string_lossy(), "--hash-only"]);
    assert_eq!(code(&first), 0, "stderr: {}", stderr(&first));
    assert_eq!(stdout(&first), stdout(&second));
    assert_eq!(stdout(&first).trim().len(), 16, "a 64-bit hex hash");
}

/// A typo'd key is a hard error, not a silently-ignored line (§5.5). This is
/// the single most important property of the loader: `half_spread_tick` being
/// ignored would report a costed backtest as a costless one.
#[test]
fn an_unknown_field_is_a_hard_error() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace("half_spread_ticks = 1", "half_spread_tick = 1"));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("half_spread_tick"), "{text}");
    assert!(text.contains("silently ignored"), "{text}");
}

/// A version this build does not implement must say so, rather than failing
/// on the first field it does not recognize and sending the reader hunting
/// for a typo that is not there.
#[test]
fn an_unknown_schema_version_names_the_version() {
    let dir = TempDir::new();
    let path = dir.config(
        &TEMPLATE
            .replace("schema_version = 1", "schema_version = 2")
            .replace("[run]", "[what_v2_added]\nx = 1\n\n[run]"),
    );
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("schema_version = 2"), "{text}");
    assert!(
        !text.contains("what_v2_added"),
        "the version must be reported before the unknown field: {text}"
    );
}

/// Money never parses through `f64` (D-0027), so a bare TOML float is not a
/// money value.
#[test]
fn money_written_as_a_toml_float_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        r#"fee_per_contract_usd = "1.25""#,
        "fee_per_contract_usd = 1.25",
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("fee_per_contract_usd"),
        "{}",
        stderr(&out)
    );
}

/// A stepped float axis explains why it cannot exist, rather than reporting
/// an unknown field.
#[test]
fn a_stepped_float_axis_explains_itself() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        "[indicators.slow]\nkind = \"sma\"\nperiod = [20, 30]",
        "[indicators.slow]\nkind = \"bollinger\"\nperiod = 20\nk = { start = 1.5, end = 2.5, step = 0.5 }",
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("floating-point-dependent length"), "{text}");
    assert!(text.contains("Write the values out"), "{text}");
}

/// A rule naming a slot that does not exist fails at load, with the position
/// in the rule and the list of slots that do exist.
#[test]
fn a_rule_naming_an_absent_slot_fails_before_any_bar() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace("fast crosses_above slow", "fast crosses_above trend"));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("unknown indicator slot"), "{text}");
    assert!(text.contains("fast, slow"), "{text}");
}

/// Two identical combos would be two trials charged for one search.
#[test]
fn a_duplicated_axis_value_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace("period = [20, 30]", "period = [20, 30, 20]"));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("deflated Sharpe"), "{}", stderr(&out));
}

/// Running only the first entry of a universe would report a partial answer
/// as a whole one, so it refuses instead.
#[test]
fn a_multi_instrument_universe_is_refused_rather_than_truncated() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        r#"instruments = ["SYN:RW"]"#,
        r#"instruments = ["SYN:RW", "ESH4"]"#,
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("partial answer as a whole one"),
        "{}",
        stderr(&out)
    );
}

/// A back-adjusted series needs a consumer that names which of the two price
/// series it wants (D-0042); `combo` has nowhere to put that choice.
#[test]
fn a_continuous_alias_is_refused_with_its_reason() {
    let out = run(&[
        "combo",
        "--config",
        &shipped("example-combo.toml").to_string_lossy(),
        "--run",
    ]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("continuous alias"), "{text}");
    // The refusal survived D-0073, but its *reason* changed: `backtest` now
    // replays a stitched series, so "nothing can name which price series it
    // wants" is no longer true. What still holds is that a grid expands rules
    // it has not seen, and a rule comparing a price to an absolute constant
    // reads a level that back-adjustment took from the future. A test
    // asserting only "it refused" would have kept passing over a stale reason.
    assert!(text.contains("D-0073"), "{text}");
    assert!(text.contains("absolute constant"), "{text}");
    assert!(text.contains("backtest"), "{text}");
}

/// The contract spec must be labelled with the instrument the feed actually
/// emits, or the printed header is a lie.
#[test]
fn a_synthetic_run_must_name_the_instrument_the_feed_emits() {
    let dir = TempDir::new();
    let path =
        dir.config(&TEMPLATE.replace(r#"instruments = ["SYN:RW"]"#, r#"instruments = ["ESH4"]"#));
    let out = run(&["combo", "--config", &path.to_string_lossy(), "--run"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("SYN:RW"), "{}", stderr(&out));
}

/// An execution assumption is never implied (§2.4): an unrecognized fill
/// model is refused rather than defaulted.
#[test]
fn an_unknown_fill_model_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        r#"fill_model = "spread_cross""#,
        r#"fill_model = "optimistic""#,
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("never implied"), "{}", stderr(&out));
}

/// `free_fills` outside a screening stage has to announce itself (D-0006).
#[test]
fn free_fills_announces_that_it_is_screening_only() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        r#"fill_model = "spread_cross""#,
        r#"fill_model = "free_fills""#,
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy(), "--run"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("SCREENING ONLY"), "{}", stdout(&out));
}

/// A missing file is a usage error naming the path, not a panic.
#[test]
fn a_missing_config_is_a_usage_error() {
    let out = run(&["combo", "--config", "no/such/config.toml"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("cannot read"), "{}", stderr(&out));
}
