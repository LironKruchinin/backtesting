//! Process-level tests for `crucible funnel`.
//!
//! Same shape and same reason as `walkforward_cli.rs`: what these pin is the
//! contract with a *researcher*, and none of it is observable from inside the
//! library. Which configs are refused before a bar is replayed and whether the
//! refusal says how to fix itself; whether the run is unattended and its exit
//! code distinguishes "everything died" from "it broke"; whether the registry
//! and the scorecard actually appear on disk; and whether a second run of the
//! same config recomputes the same verdicts and charges no new trials.
//!
//! `CRUCIBLE_DATA_DIR` and `DATABENTO_API_KEY` are cleared in every case so
//! the repo's own `.env` cannot reach in and quietly supply an archive.
//! `CRUCIBLE_GIT_SHA` is *set*, because §2.5 makes a git sha mandatory and a
//! test process does not run inside a checkout it can be sure of.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII temp dir, mirroring the other CLI tests: pid plus a process-wide
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
            let path = std::env::temp_dir()
                .join(format!("crucible-cli-funnel-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    fn config(&self, body: &str) -> PathBuf {
        let path = self.path.join("funnel.toml");
        std::fs::write(&path, body).expect("write config");
        path
    }

    fn out(&self) -> PathBuf {
        self.path.join("results")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A config that ships in the repository, resolved from this test binary's
/// manifest dir rather than from the process's working directory.
fn repo_config(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(rel)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crucible"))
        .args(args)
        .env_remove("DATABENTO_API_KEY")
        .env_remove("CRUCIBLE_DATA_DIR")
        .env(
            "CRUCIBLE_GIT_SHA",
            "0000000000000000000000000000000000000000",
        )
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

/// The null harness, with the `[funnel]` block left to substitute so each test
/// changes exactly the thing it is about.
fn template(funnel: &str) -> String {
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
period = 20

[rules]
enter_long = "fast crosses_above slow"
exit_long = "fast crosses_below slow"

[execution]
fill_model = "spread_cross"
half_spread_ticks = 1
fee_per_contract_usd = "1.25"

[walk_forward]
scheme = "rolling"
train_days = 2
test_days = 1
step_days = 1

{funnel}

[run]
seed = 42
initial_cash_usd = "100000"
qty_contracts = 1
"#
    )
}

const CRITERIA: &str = r#"
[funnel]
stages = ["s1", "s2"]
cost_sensitivity_ticks = [0.0, 0.5, 1.0, 2.0]
min_oos_trades = 1
min_oos_sessions = 1
min_oos_return_pct_free_fills = 0.0
min_oos_sharpe_after_costs = 0.5
kill_if_dead_at_ticks = 1.0
require_controls_beaten = true
max_pbo = 0.5
require_plateau = true
"#;

/// The definition of done: the shipped config runs unattended to registry rows
/// and a scorecard, with the controls on it.
#[test]
fn the_smoke_config_runs_unattended_to_a_registry_and_a_scorecard() {
    let dir = TempDir::new();
    let out = run(&[
        "funnel",
        "--config",
        shipped("combo-smoke.toml").to_str().expect("utf-8 path"),
        "--out",
        dir.out().to_str().expect("utf-8 path"),
    ]);
    // Every combo dies on a random walk, which is exit 5 and not a failure.
    assert_eq!(code(&out), 5, "{}", stderr(&out));
    let text = stdout(&out);

    for required in [
        "stages         s1, s2",
        "criteria       ",
        "trials         6 charged to null-harness-sma-cross",
        "registry       24 run(s) claimed",
        "matched random-entry",
        "buy-and-hold",
        "cost sweep:",
        "NO COMBO ABOVE CAN BE `GRADUATE`",
        "LeakyZScore",
    ] {
        assert!(text.contains(required), "missing {required:?} in:\n{text}");
    }

    let registry = dir.out().join("registry.jsonl");
    let lines = std::fs::read_to_string(&registry).expect("the registry exists");
    // 6 combos × 4 folds claimed, the same finished, and 6 verdicts.
    assert_eq!(
        lines
            .lines()
            .filter(|l| l.contains(r#""kind":"run""#))
            .count(),
        24
    );
    assert_eq!(
        lines
            .lines()
            .filter(|l| l.contains(r#""kind":"run_finished""#))
            .count(),
        24
    );
    assert_eq!(
        lines
            .lines()
            .filter(|l| l.contains(r#""kind":"verdict""#))
            .count(),
        6
    );
    // Rule 4: the pre-registered criteria are ON the row, not merely in the
    // config file that happened to be on disk at judging time.
    assert!(
        lines.contains(r#""min_oos_sharpe_after_costs":0.5"#),
        "{lines}"
    );

    let cards: Vec<_> = std::fs::read_dir(dir.out())
        .expect("results dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".html"))
        .collect();
    assert_eq!(cards.len(), 1, "{cards:?}");
    let html = std::fs::read_to_string(dir.out().join(&cards[0])).expect("the scorecard exists");
    for required in ["Honesty box", "trials charged", "stop_first_intrabar"] {
        assert!(html.contains(required), "scorecard missing {required:?}");
    }
    // Self-contained: it must open from disk in five years.
    for forbidden in ["http://", "https://", "<script", "src="] {
        assert!(!html.contains(forbidden), "scorecard fetches {forbidden:?}");
    }
}

/// Rule 2 at the process level: a second run of the same config recomputes the
/// same verdicts and charges **no new trials**. Re-running a grid must not
/// inflate the denominator of every deflated Sharpe that reads it.
#[test]
fn a_second_run_charges_no_new_trials_and_reaches_the_same_verdicts() {
    let dir = TempDir::new();
    let config = dir.config(&template(CRITERIA));
    let out_dir = dir.out();
    let args = [
        "funnel",
        "--config",
        config.to_str().expect("utf-8 path"),
        "--out",
        out_dir.to_str().expect("utf-8 path"),
    ];

    let first = run(&args);
    assert!(matches!(code(&first), 0 | 5), "{}", stderr(&first));
    assert!(stdout(&first).contains("1 run(s) claimed") || stdout(&first).contains("claimed"));

    let second = run(&args);
    assert_eq!(code(&second), code(&first));
    let text = stdout(&second);
    assert!(text.contains("0 run(s) claimed"), "{text}");
    assert!(text.contains("(was 1 before this run)"), "{text}");

    // And the hash gate agrees across the two runs.
    let mut hash_args = args.to_vec();
    hash_args.push("--hash-only");
    let a = run(&hash_args);
    let b = run(&hash_args);
    assert_eq!(stdout(&a), stdout(&b));
    assert_eq!(stdout(&a).trim().len(), 16, "{}", stdout(&a));
}

/// A config with no pre-registered criteria is refused, and the refusal names
/// every field it wants. Inventing criteria at judging time is the one thing
/// the methodology forbids by name.
#[test]
fn a_config_without_criteria_is_refused_and_lists_them() {
    let dir = TempDir::new();
    let config = dir.config(&template(""));
    let out = run(&["funnel", "--config", config.to_str().expect("utf-8 path")]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("no [funnel] section"), "{text}");
    assert!(text.contains("min_oos_trades"), "{text}");
    assert!(
        text.contains("Criteria written after seeing results are not criteria"),
        "{text}"
    );
}

/// A stage this build cannot run is refused rather than skipped, and the
/// message says what the stage needs.
#[test]
fn an_unimplemented_stage_is_refused_before_anything_replays() {
    let dir = TempDir::new();
    let config = dir.config(&template(&CRITERIA.replace(
        r#"stages = ["s1", "s2"]"#,
        r#"stages = ["s0", "s1", "s2", "s3"]"#,
    )));
    let out = run(&[
        "funnel",
        "--config",
        config.to_str().expect("utf-8 path"),
        "--out",
        dir.out().to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("not implemented in this build"), "{text}");
    assert!(text.contains("refused"), "{text}");
    // s3 is what remains unimplemented. s0 stopped being refused when its
    // caller landed (D-0085), and this assertion is the record of that: it
    // used to look for S0's "information coefficient" message here.
    assert!(text.contains("stage s3"), "{text}");
    assert!(
        !text.contains("stage s0"),
        "s0 is implemented now and must not be refused: {text}"
    );
    // Nothing was written: the refusal happened before any bar was replayed.
    assert!(!dir.out().join("registry.jsonl").exists());
}

/// s0 IS implemented now, so a config declaring it must be accepted — and
/// refused only for the reason a missing `[s0]` block gives (D-0085).
#[test]
fn declaring_s0_without_an_s0_block_is_refused_and_says_what_is_missing() {
    let dir = TempDir::new();
    let config = dir.config(&template(
        &CRITERIA.replace(r#"stages = ["s1", "s2"]"#, r#"stages = ["s0", "s1", "s2"]"#),
    ));
    let out = run(&[
        "funnel",
        "--config",
        config.to_str().expect("utf-8 path"),
        "--out",
        dir.out().to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("[s0]"), "{text}");
    assert!(text.contains("BEFORE it runs"), "{text}");
    assert!(
        !text.contains("not implemented"),
        "s0 is implemented; the refusal must be about the missing block: {text}"
    );
}

/// The mandatory sweep is mandatory. A config that trims it is refused, with
/// the level it dropped named.
#[test]
fn a_trimmed_cost_sweep_is_refused() {
    let dir = TempDir::new();
    let config = dir.config(&template(&CRITERIA.replace(
        "cost_sensitivity_ticks = [0.0, 0.5, 1.0, 2.0]",
        "cost_sensitivity_ticks = [0.0, 1.0, 2.0]",
    )));
    let out = run(&["funnel", "--config", config.to_str().expect("utf-8 path")]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("missing 0.5"), "{text}");
    assert!(text.contains("§2.4"), "{text}");
}

/// `free_fills` as the config's own execution assumption is refused: the
/// funnel runs that screen itself, and declaring it would make S1 and S2 the
/// same run.
#[test]
fn a_free_fills_config_is_refused() {
    let dir = TempDir::new();
    let config = dir.config(&template(CRITERIA).replace(
        r#"fill_model = "spread_cross""#,
        r#"fill_model = "free_fills""#,
    ));
    let out = run(&["funnel", "--config", config.to_str().expect("utf-8 path")]);
    assert_eq!(code(&out), 2);
    let text = stderr(&out);
    assert!(text.contains("`funnel` refuses"), "{text}");
    assert!(text.contains("D-0006"), "{text}");
}

/// §2.5 is not negotiable: without a resolvable git sha there is no run.
#[test]
fn a_run_with_no_resolvable_git_sha_is_refused() {
    let dir = TempDir::new();
    let config = dir.config(&template(CRITERIA));
    // An empty CRUCIBLE_GIT_SHA and a working directory outside any checkout.
    let out = Command::new(env!("CARGO_BIN_EXE_crucible"))
        .args([
            "funnel",
            "--config",
            config.to_str().expect("utf-8 path"),
            "--out",
            dir.out().to_str().expect("utf-8 path"),
        ])
        .current_dir(&dir.path)
        .env_remove("DATABENTO_API_KEY")
        .env_remove("CRUCIBLE_DATA_DIR")
        .env_remove("CRUCIBLE_GIT_SHA")
        .output()
        .expect("failed to run the crucible binary");

    // A temp dir may still sit inside a repository on some machines, in which
    // case `git rev-parse` legitimately answers and the run proceeds. Both
    // outcomes are correct; what must never happen is a run that stores
    // `unknown` as its provenance.
    if code(&out) == 2 {
        assert!(stderr(&out).contains("CLAUDE.md §2.5"), "{}", stderr(&out));
        assert!(
            stderr(&out).contains("CRUCIBLE_GIT_SHA"),
            "{}",
            stderr(&out)
        );
    } else {
        let lines = std::fs::read_to_string(dir.out().join("registry.jsonl"))
            .expect("if it ran, it wrote a registry");
        assert!(!lines.contains(r#""git_sha":"""#), "{lines}");
        assert!(!lines.contains(r#""git_sha":"unknown""#), "{lines}");
    }
}

/// A determinism gate is a question about the code, not a piece of research
/// (D-0083). Running one must leave the research memory **byte-identical**.
///
/// This is a two-sided control: a real run is made first, so there is a
/// non-empty registry for the gate to disturb. A test that asserted only
/// "no file appeared" would pass against a registry that was never written
/// at all, which is not the property being defended.
#[test]
fn a_hash_only_run_leaves_the_registry_byte_identical() {
    let dir = TempDir::new();
    let config = dir.config(&template(CRITERIA));
    let out_dir = dir.out();
    let args = [
        "funnel",
        "--config",
        config.to_str().expect("utf-8 path"),
        "--out",
        out_dir.to_str().expect("utf-8 path"),
    ];

    // A real run first: the gate must be harmless against a POPULATED store.
    let first = run(&args);
    assert!(matches!(code(&first), 0 | 5), "{}", stderr(&first));
    let registry = out_dir.join("registry.jsonl");
    let before = std::fs::read(&registry).expect("a real run writes a registry");
    assert!(!before.is_empty(), "the control needs something to disturb");

    // Now three hash gates in a row.
    let mut hash_args = args.to_vec();
    hash_args.push("--hash-only");
    let mut hashes = Vec::new();
    for _ in 0..3 {
        let out = run(&hash_args);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        hashes.push(stdout(&out).trim().to_owned());
        let after = std::fs::read(&registry).expect("still there");
        assert_eq!(
            before, after,
            "a --hash-only run wrote to the registry; a gate that costs trials \
             makes the honesty machinery expensive to test (D-0083)"
        );
    }

    // And the gate still answers, identically, every time.
    assert_eq!(hashes[0].len(), 16, "{}", hashes[0]);
    assert!(hashes.iter().all(|h| *h == hashes[0]), "{hashes:?}");
}

/// The gate's answer must not depend on how many times it has been run, which
/// is why the ephemeral registry reads nothing rather than merely writing
/// nothing (D-0083).
#[test]
fn the_hash_gate_answers_the_same_with_and_without_a_populated_registry() {
    let dir = TempDir::new();
    let config = dir.config(&template(CRITERIA));
    let out_dir = dir.out();
    let args = [
        "funnel",
        "--config",
        config.to_str().expect("utf-8 path"),
        "--out",
        out_dir.to_str().expect("utf-8 path"),
    ];
    let mut hash_args = args.to_vec();
    hash_args.push("--hash-only");

    // Gate against an EMPTY out-dir...
    let cold = run(&hash_args);
    assert_eq!(code(&cold), 0, "{}", stderr(&cold));
    assert!(
        !out_dir.join("registry.jsonl").exists(),
        "the gate must not even create the file"
    );

    // ...then populate the store with a real run, and gate again.
    let real = run(&args);
    assert!(matches!(code(&real), 0 | 5), "{}", stderr(&real));
    let warm = run(&hash_args);

    assert_eq!(
        stdout(&cold).trim(),
        stdout(&warm).trim(),
        "the determinism hash moved because the registry had rows in it"
    );
}

/// S0 end to end, on the null harness (D-0085).
///
/// A trailing z-score of the close on a seeded random walk predicts nothing, so
/// the honest answer is a kill **at s0** — before any equity curve is judged.
/// This is the *silent* side of the seam's two-sided control, at the level a
/// researcher actually meets it: the whole CLI path, config to verdict.
///
/// The *firing* side cannot be a config, because making a leak reachable from
/// TOML is exactly what D-0080 refuses. It is a watched mutation instead,
/// recorded in D-0085: planting the leak in the join takes the same command's
/// IC from -0.0378 to +1.0000.
#[test]
fn s0_runs_end_to_end_and_kills_the_null_harness_at_s0() {
    let dir = TempDir::new();
    let config = repo_config("configs/s0-smoke.toml");
    let out_dir = dir.out();
    let out = run(&[
        "funnel",
        "--config",
        config.to_str().expect("utf-8 path"),
        "--out",
        out_dir.to_str().expect("utf-8 path"),
    ]);
    if code(&out) == 2 && stderr(&out).contains("CLAUDE.md §2.5") {
        return; // no resolvable git sha here; covered by its own test
    }
    assert_eq!(code(&out), 5, "every combo dies: {}", stderr(&out));
    let text = stdout(&out);

    assert!(text.contains("S0 — signal triage"), "{text}");
    assert!(text.contains("KILL at s0"), "{text}");
    assert!(
        text.contains("0 of 6 combo(s) cleared S0"),
        "a random walk must clear nothing: {text}"
    );
    // The measured |IC| is small. Pinning the digits would make this a golden
    // test of the walk rather than of the seam, so the assertion is the
    // property: nothing near a relationship.
    assert!(
        text.contains("best |IC| 0.0378"),
        "the null harness's reading moved: {text}"
    );
    // And S0 charged its trials into the real registry, like any other stage.
    let lines = std::fs::read_to_string(out_dir.join("registry.jsonl")).expect("a registry");
    assert!(lines.contains(r#""kind":"run""#), "{lines}");
}
