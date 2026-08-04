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
    // The refusal survived D-0076, but its *reason* changed: `backtest` now
    // replays a stitched series, so "nothing can name which price series it
    // wants" is no longer true. What still holds is that a grid expands rules
    // it has not seen, and a rule comparing a price to an absolute constant
    // reads a level that back-adjustment took from the future. A test
    // asserting only "it refused" would have kept passing over a stale reason.
    assert!(text.contains("D-0076"), "{text}");
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

// ------------------------------------------------------ rolling normalizer

/// A rolling z-score is nameable from TOML, runs, and its **source** reaches
/// the config hash (D-0080).
///
/// The hash comparison is the load-bearing half: two configs identical except
/// for `source` are two different features, and a registry that gave them one
/// identity would pool their trials and report one idea where there were two.
#[test]
fn a_rolling_zscore_slot_runs_and_its_source_is_part_of_its_identity() {
    let dir = TempDir::new();
    let with = |source: &str| {
        TEMPLATE
            .replace(
                "[indicators.slow]\nkind = \"sma\"\nperiod = [20, 30]",
                &format!(
                    "[indicators.slow]\nkind = \"zscore\"\nperiod = 20\nsource = \"{source}\""
                ),
            )
            .replace(
                "enter_long = \"fast crosses_above slow\"",
                "enter_long = \"slow < -1.5\"",
            )
            .replace(
                "exit_long = \"fast crosses_below slow\"",
                "exit_long = \"slow > 0\"",
            )
    };

    let hash_of = |body: String, name: &str| {
        let path = dir.path.join(name);
        std::fs::write(&path, body).expect("write config");
        let out = run(&["combo", "--config", &path.to_string_lossy(), "--run"]);
        assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
        let text = stdout(&out);
        let line = text
            .lines()
            .find(|l| l.contains("config hash"))
            .expect("a config hash line")
            .to_owned();
        (line, text)
    };

    let (close_hash, close_text) = hash_of(with("close"), "close.toml");
    let (volume_hash, _) = hash_of(with("volume"), "volume.toml");
    let (return_hash, return_text) = hash_of(with("return"), "return.toml");

    assert_ne!(close_hash, volume_hash);
    assert_ne!(close_hash, return_hash);
    assert_ne!(volume_hash, return_hash);

    // The warmup a `return` source needs is declared, not absorbed: 20 bars of
    // window plus the one bar its first difference costs.
    assert!(close_text.contains("warmup 20 bars"), "{close_text}");
    assert!(return_text.contains("warmup 21 bars"), "{return_text}");
}

/// A misspelled source is a hard error naming the slot and listing the three
/// that exist, rather than a silent default to `close`.
#[test]
fn an_unknown_rolling_source_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        "[indicators.slow]\nkind = \"sma\"\nperiod = [20, 30]",
        "[indicators.slow]\nkind = \"zscore\"\nperiod = 20\nsource = \"closs\"",
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2, "stdout: {}", stdout(&out));
    let text = stderr(&out);
    assert!(text.contains("\"slow\""), "{text}");
    assert!(text.contains("close, volume, return"), "{text}");
}

/// A rolling slot with no `source` at all is refused by `deny_unknown_fields`'s
/// sibling — a missing required field — rather than defaulted.
#[test]
fn a_rolling_slot_without_a_source_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        "[indicators.slow]\nkind = \"sma\"\nperiod = [20, 30]",
        "[indicators.slow]\nkind = \"zscore\"\nperiod = 20",
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2, "stdout: {}", stdout(&out));
    assert!(stderr(&out).contains("source"), "{}", stderr(&out));
}

/// The second shipped reference config — the one that exercises every operand
/// the grammar has — expands.
///
/// Two `stretch` periods and nothing else varying, so 2 combos; warmup is the
/// max across the grid, which is `stretch`'s 40 and not `rv`'s 21 (§2.6).
#[test]
fn the_grammar_surface_config_expands() {
    let out = run(&[
        "combo",
        "--config",
        &shipped("example-session-volume.toml").to_string_lossy(),
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("2 combos"), "{text}");
    assert!(text.contains("warmup 40 bars"), "{text}");
    // Every unlock is visible in the echoed rules and slots.
    for token in [
        "minutes_since_rth_open",
        "minutes_to_close",
        "volume",
        "zscore",
        "stdev",
    ] {
        assert!(text.contains(token), "{token} missing from:\n{text}");
    }
}

// ---------------------------------------------------------------------------
// `[pooling]` — block C's declaration surface. Inert: these assert what a
// config may DECLARE, and nothing consumes the declaration yet (D-0114 lists
// the orchestration as owed). The parser and its refusals land first, which is
// the ordering §8 requires of any shape a later writer will depend on.
// ---------------------------------------------------------------------------

/// The converse, and it comes first: without it every refusal below could be
/// satisfied by a parser that rejected `[pooling]` outright for any reason.
///
/// A well-formed pool must reach the **orchestration** refusal — the blanket
/// "this build cannot run it" — rather than any of the shape refusals. That is
/// what proves the four shape rules below are diagnosing shape and not just
/// rejecting the block on sight.
#[test]
fn a_well_formed_pool_reaches_the_not_yet_orchestrated_refusal() {
    let dir = TempDir::new();
    let path = dir.config(
        &TEMPLATE
            .replace(
                r#"instruments = ["SYN:RW"]"#,
                r#"instruments = ["ESH2024", "ESM2024"]"#,
            )
            .replace("[run]", "[pooling]\nroot = \"ES\"\n\n[run]"),
    );
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        !stderr(&out).contains("unknown field"),
        "`[pooling]` must parse: {}",
        stderr(&out)
    );
    assert!(
        !stderr(&out).contains("partial answer as a whole one"),
        "a declared pool is exactly what makes >1 instrument legal: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("is not implemented"),
        "a well-formed pool must reach the orchestration refusal: {}",
        stderr(&out)
    );
}

/// Pooling one contract is not pooling. It would read the same sessions and
/// charge the same trial while printing a pooled-run header over a
/// single-contract result.
#[test]
fn pooling_fewer_than_two_contracts_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace("[run]", "[pooling]\nroot = \"SYN\"\n\n[run]"));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("Pooling needs at least two contracts"),
        "{}",
        stderr(&out)
    );
}

/// The double-count in its most direct form, refused before any replay.
/// `crucible-funnel::pooling` refuses it again on the day keys (D-0114); this
/// is the earlier and cheaper of the two.
#[test]
fn the_same_contract_twice_in_a_pool_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(
        &TEMPLATE
            .replace(
                r#"instruments = ["SYN:RW"]"#,
                r#"instruments = ["ESH2024", "ESH2024"]"#,
            )
            .replace("[run]", "[pooling]\nroot = \"ES\"\n\n[run]"),
    );
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("charges two trials for one run"),
        "{}",
        stderr(&out)
    );
}

/// D-0076 stands. Pooling exists so a long sample does not require stitching;
/// letting a continuous alias into a pool would enable back-adjusted grids by
/// the back door, and that is a supersession rather than a declaration.
#[test]
fn a_continuous_alias_inside_a_pool_is_refused_naming_d0076() {
    for alias in ["ES.v.0", "ES.c.0"] {
        let dir = TempDir::new();
        let path = dir.config(
            &TEMPLATE
                .replace(
                    r#"instruments = ["SYN:RW"]"#,
                    &format!(r#"instruments = ["ESH2024", "{alias}"]"#),
                )
                .replace("[run]", "[pooling]\nroot = \"ES\"\n\n[run]"),
        );
        let out = run(&["combo", "--config", &path.to_string_lossy()]);
        assert_eq!(code(&out), 2, "{alias}");
        assert!(
            stderr(&out).contains("D-0076"),
            "the refusal must name the decision it upholds: {}",
            stderr(&out)
        );
    }
}

/// A pool is a claim that these contracts are ONE instrument across time.
/// Two roots is a cross-instrument claim — breadth, not sample size (D-0114).
#[test]
fn a_contract_outside_the_declared_root_is_refused() {
    let dir = TempDir::new();
    let path = dir.config(
        &TEMPLATE
            .replace(
                r#"instruments = ["SYN:RW"]"#,
                r#"instruments = ["ESH2024", "NQH2024"]"#,
            )
            .replace("[run]", "[pooling]\nroot = \"ES\"\n\n[run]"),
    );
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("breadth rather than sample size"),
        "{}",
        stderr(&out)
    );
}

/// `deny_unknown_fields` on the new struct, like every other config struct
/// (§5.5). A typo'd pooling key must be a hard error, not a silent no-op.
#[test]
fn an_unknown_pooling_key_is_a_hard_error() {
    let dir = TempDir::new();
    let path = dir.config(
        &TEMPLATE
            .replace(
                r#"instruments = ["SYN:RW"]"#,
                r#"instruments = ["ESH2024", "ESM2024"]"#,
            )
            .replace(
                "[run]",
                "[pooling]\nroot = \"ES\"\ncontarcts = [\"ESH2024\"]\n\n[run]",
            ),
    );
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("contarcts") || stderr(&out).contains("unknown field"),
        "{}",
        stderr(&out)
    );
}

/// The original rule is unchanged for configs without `[pooling]`, and the
/// message now names the remedy rather than only the refusal.
#[test]
fn many_instruments_without_a_pooling_block_still_refuse_and_name_the_remedy() {
    let dir = TempDir::new();
    let path = dir.config(&TEMPLATE.replace(
        r#"instruments = ["SYN:RW"]"#,
        r#"instruments = ["ESH2024", "ESM2024"]"#,
    ));
    let out = run(&["combo", "--config", &path.to_string_lossy()]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("declare `[pooling].root`"),
        "{}",
        stderr(&out)
    );
}

// ---------------------------------------------------------------------------
// C2 — the per-contract evaluation window. Inert like C0/C1: nothing supplies
// a window yet (C3 does, from the `.v` roll table), and D-0117 still refuses
// every pooled config at validation time.
// ---------------------------------------------------------------------------

/// The curated single-contract path still works through C1's delegation and
/// C2's window seam.
///
/// A **regression** control, not a control on the window: no CLI surface
/// supplies a window until C3, so nothing here can exercise one. What it does
/// prove is that `collect_events` → `collect_events_for` →
/// `collect_events_in_window(None)` still reads real curated bars — the chain
/// C1 and C2 inserted between the caller and the archive.
///
/// C2's windowing behaviour is deliberately left without a behavioural control
/// until C3 wires it, and C3 owes one. Asserting on the source text instead
/// would be decoration (§7), so this file does not.
#[test]
fn the_curated_path_survives_the_window_seam() {
    const SYNTHETIC: &str = "source = \"synthetic\"\nseed = 7\nbars = 300\nstart_price_points = \"5000\"\nvol_ticks = 4";
    const CURATED: &str = "source = \"curated\"\nstart = \"2024-01-01\"\nend = \"2024-02-01\"";
    assert!(
        TEMPLATE.contains(SYNTHETIC),
        "positive control (D-0118): the pattern must match before its absence means anything"
    );
    let dir = TempDir::new();
    let path = dir.config(
        &TEMPLATE
            .replace(
                r#"instruments = ["SYN:RW"]"#,
                r#"instruments = ["ESH2024"]"#,
            )
            .replace(SYNTHETIC, CURATED),
    );
    let out = run(&["combo", "--config", &path.to_string_lossy(), "--run"]);
    assert_eq!(
        code(&out),
        0,
        "the curated single-contract path must still run: {}",
        stderr(&out)
    );
}
