//! `crucible` — CLI entry point.
//!
//! `demo` runs the reference strategy on a seeded random walk under two fill
//! models: it proves the vertical slice end-to-end, shows the cost-of-costs
//! lesson in one screen, and gives CI a determinism hash
//! (`demo --hash-only`). `env` reports what the process environment looks
//! like, so a misconfigured key is found before a download is paid for rather
//! than after.
//!
//! The archive path is `pull` (buy and verify raw DBN) → `verify` (re-hash it)
//! → `transcode` (raw into curated Parquet) → `backtest` (replay it). Only
//! `pull` can spend money, and only `pull` and `transcode` need the
//! `databento` feature.
//!
//! ## Environment (D-0022)
//!
//! This is a **bin target**, so it owns environment resolution: [`main`]
//! loads `.env` into the process environment before dispatching, and library
//! crates keep reading plain `std::env` (or, better, take resolved values as
//! arguments — `Catalog::open` takes a `PathBuf`, never a variable name).
//! Real environment variables always win over the file. The values that
//! matter are `DATABENTO_API_KEY` and `CRUCIBLE_DATA_DIR`; neither has a
//! default, and the key is never printed, logged, or passed as an argument.

mod backtest;
mod pull;
mod rolls;
mod transcode;

use std::path::{Path, PathBuf};

use clap::{CommandFactory, Parser, Subcommand};
use crucible_core::prelude::*;
use crucible_data::SyntheticFeed;
use crucible_engine::{BacktestParams, BacktestResult, FreeFills, SpreadCrossFills, run};
use crucible_strategies::SmaCross;

/// 1-minute bars per year on a ~23h/252-day futures session.
const BARS_PER_YEAR_1M: f64 = 347_760.0;
const DEMO_SEED: u64 = 42;
const DEMO_BARS: usize = 100_000;

/// Environment and planned-command notes, appended to `--help`. Kept as prose
/// because the two variables below have no defaults and no config file, and a
/// misconfigured key should be discovered here rather than mid-download.
const AFTER_HELP: &str = "\
ENVIRONMENT:\n\
\x20 DATABENTO_API_KEY    Databento API key (never passed as an argument)\n\
\x20 CRUCIBLE_DATA_DIR    archive root; must live OUTSIDE the repo\n\
\x20 Both may be set in a .env file at the repo root; real environment\n\
\x20 variables take precedence over it. .env is gitignored — never commit it.\n\
\n\
TYPICAL ORDER:\n\
\x20 pull -> verify -> transcode -> backtest\n\
\n\
NOTE: `pull` and `transcode` need a build with the Databento client:\n\
\x20 cargo run -p crucible-cli --features databento -- pull ...\n\
\n\
PLANNED (see docs/MILESTONES.md):\n\
\x20 screen      M3  stage 0-1 signal triage / coarse grid\n\
\x20 funnel      M3  full staged evaluation of a config\n\
\x20 report      M3  render verdict scorecards";

#[derive(Parser)]
#[command(
    name = "crucible",
    about = "a backtesting engine designed to reject strategies",
    after_help = AFTER_HELP,
    disable_version_flag = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the reference SMA-cross demo on synthetic data.
    Demo {
        /// Print only the determinism hash (the CI gate reads this).
        #[arg(long)]
        hash_only: bool,
    },
    /// Show which environment variables are configured.
    Env,
    /// Batch-download entitled Databento windows into the archive.
    Pull(pull::PullArgs),
    /// Re-hash the archive against the manifest.
    Verify,
    /// Build curated Parquet bars from the raw DBN archive.
    Transcode(transcode::TranscodeArgs),
    /// Replay curated bars through the reference strategy.
    Backtest(backtest::BacktestArgs),
    /// Build a continuous-contract roll table from curated bars.
    Rolls(rolls::RollsArgs),
}

fn main() {
    // First statement in the process: every command below (and every crate it
    // calls) must see the same environment, and `set_var` is only sound while
    // we are still single-threaded.
    let env_file = load_env_file();

    let cli = Cli::parse();
    let code = match cli.command {
        Some(Command::Demo { hash_only }) => {
            demo(hash_only);
            0
        }
        Some(Command::Env) => {
            env_report(env_file.as_deref());
            0
        }
        Some(Command::Pull(args)) => pull::run(&args),
        Some(Command::Verify) => pull::verify(),
        Some(Command::Transcode(args)) => transcode::run(&args),
        Some(Command::Backtest(args)) => backtest::run(&args),
        Some(Command::Rolls(args)) => rolls::run(&args),
        // Bare `crucible` stays a zero-exit help screen: it is what a new
        // reader types first, and it has not failed at anything.
        None => {
            let _ = Cli::command().print_help();
            println!();
            0
        }
    };
    if code != 0 {
        std::process::exit(code);
    }
}

/// Loads `.env` (searching the working directory and its parents) into the
/// process environment, returning the file that was used.
///
/// Precedence is deliberate: `dotenvy` never overwrites a variable that is
/// already set, so a real environment variable — a CI secret, a `$env:` you
/// exported for one command — always beats the file. A missing file is
/// normal (the environment alone is a complete configuration). A file that
/// exists but does not parse is **fatal**: `dotenvy` stops at the bad line
/// having already applied the good ones, and a half-loaded secrets file is
/// exactly the silent misconfiguration CLAUDE.md §5.5 refuses to tolerate in
/// configs.
fn load_env_file() -> Option<PathBuf> {
    match dotenvy::dotenv() {
        Ok(path) => Some(path),
        Err(err) if err.not_found() => None,
        Err(err) => {
            eprintln!("error: .env exists but could not be loaded: {err}");
            eprintln!(
                "       it is applied line by line, so a partial load would leave the process\n\
                 \x20      half-configured. Fix the file (KEY=value, one per line) and retry."
            );
            std::process::exit(2);
        }
    }
}

/// Reports what the environment looks like *after* `.env` is applied, so a
/// missing or mangled key is found here rather than halfway through a paid
/// download.
///
/// `CRUCIBLE_DATA_DIR` is a path, not a secret, and is printed in full.
/// `DATABENTO_API_KEY` is never printed — only its presence and length,
/// which is enough to catch the classic "I pasted the surrounding quotes"
/// error without putting key material in a terminal scrollback.
fn env_report(env_file: Option<&Path>) {
    match env_file {
        Some(path) => println!("env file: {}", path.display()),
        None => println!("env file: none found (using the process environment only)"),
    }
    println!();

    match std::env::var("DATABENTO_API_KEY") {
        Ok(key) if !key.is_empty() => {
            println!(
                "  DATABENTO_API_KEY  set, {} chars (value never printed)",
                key.chars().count()
            );
        }
        _ => println!("  DATABENTO_API_KEY  NOT SET — `pull` cannot authenticate without it"),
    }

    match std::env::var("CRUCIBLE_DATA_DIR") {
        Ok(dir) if !dir.is_empty() => {
            println!("  CRUCIBLE_DATA_DIR  {dir}");
            if !Path::new(&dir).is_dir() {
                println!(
                    "                     ^ not a directory yet — the catalog refuses a missing\n\
                     \x20                      data dir rather than treating it as an empty archive"
                );
            }
        }
        _ => println!(
            "  CRUCIBLE_DATA_DIR  NOT SET — the archive root has no default, and must live\n\
             \x20                    outside the repo"
        ),
    }
}

fn es_like_spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("SYN:RW"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    }
}

fn one_run<M: FillModel>(fill_model: &mut M) -> BacktestResult {
    let spec = es_like_spec();
    let mut feed = SyntheticFeed::random_walk(
        DEMO_SEED,
        DEMO_BARS,
        TimeFrame::M1,
        Price::from_points(5000),
        Price::from_points_f64_lossy(0.25),
        4,
    );
    let mut strategy = SmaCross::new(20, 50, Qty(1));
    let params = BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000, // $100k
        bars_per_year: BARS_PER_YEAR_1M,
    };
    run(&mut feed, &mut strategy, fill_model, &spec, &params)
        .expect("INVARIANT: SyntheticFeed yields ordered events")
}

fn demo(hash_only: bool) {
    let free = one_run(&mut FreeFills);
    let costed = one_run(&mut SpreadCrossFills {
        half_spread_ticks: 1,
        fee_per_contract_nano_usd: 1_250_000_000, // $1.25/contract/side
    });

    if hash_only {
        let mut h = Fnv64::new();
        for r in [&free, &costed] {
            for &(ts, eq) in &r.equity {
                h.write_i64(ts.0);
                h.write_i64(eq);
            }
        }
        println!("{:016x}", h.finish());
        return;
    }

    println!("Crucible demo — SMA(20/50) cross, 1 contract");
    println!(
        "{DEMO_BARS} synthetic 1m bars (seeded random walk, seed {DEMO_SEED}); \
         ES-like spec: 0.25 tick, $50/pt\n"
    );
    println!(
        "  {:<22} {:>14} {:>9} {:>8} {:>7} {:>7} {:>8}",
        "fill model", "final equity", "return", "max DD", "trades", "win%", "Sharpe"
    );
    print_row("free_fills (S0-S1)", &free);
    print_row("spread_cross 1t+fees", &costed);

    let gap = free.summary.final_equity_nano_usd - costed.summary.final_equity_nano_usd;
    println!("\n  Cost of pretending execution is free: {}", usd(gap));
    println!(
        "\n  Reading: the data is a random walk — there is NO edge to find. Whatever\n\
         \x20 free_fills shows is luck; spread_cross shows the same trades after paying\n\
         \x20 the half-spread and fees. Any engine change that makes this demo look\n\
         \x20 profitable under costs has introduced a bug, not an edge."
    );
}

fn print_row(name: &str, r: &BacktestResult) {
    let s = &r.summary;
    let win = s
        .win_rate
        .map_or_else(|| "  n/a".to_owned(), |w| format!("{:5.1}", w * 100.0));
    let sharpe = s
        .sharpe_naive
        .map_or_else(|| "   n/a".to_owned(), |x| format!("{x:6.2}"));
    println!(
        "  {:<22} {:>14} {:>8.2}% {:>7.2}% {:>7} {:>7} {:>8}",
        name,
        usd(s.final_equity_nano_usd),
        s.total_return_pct,
        s.max_drawdown_pct,
        s.round_trips,
        win,
        sharpe
    );
}

fn usd(n: NanoUsd) -> String {
    format!("${:.2}", nano_usd_to_f64(n))
}

/// FNV-1a 64-bit, hand-rolled: stable across Rust versions (unlike
/// `DefaultHasher`), which is exactly what a CI determinism hash needs.
struct Fnv64 {
    state: u64,
}

impl Fnv64 {
    const fn new() -> Self {
        Fnv64 {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }
    fn write_i64(&mut self, v: i64) {
        for b in v.to_le_bytes() {
            self.state ^= u64::from(b);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    const fn finish(&self) -> u64 {
        self.state
    }
}
