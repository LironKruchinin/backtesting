//! `crucible pull` — the only command that can spend money, and the wiring
//! that makes it hard to do so by accident.
//!
//! The command is a **dry run by default** (D-0024). It plans, prices, and
//! prints; spending needs `--execute`, and `--execute` without an explicit
//! `--max-cost-usd` is refused by the library rather than defaulted, because
//! a default spending cap is a number nobody chose.
//!
//! Everything here is wiring: parse, validate, resolve the environment, hand
//! resolved values to `crucible-data`, print what it returns, and turn its
//! errors into exit codes. All the judgement lives in the library (CLAUDE.md
//! §3) — including the whole execute state machine, which is why it can be
//! tested offline while this file cannot be.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | done — archived, or nothing left to buy |
//! | 2 | usage, configuration, or environment error |
//! | 3 | refused to spend (cost gate, or `--execute` with no cap) |
//! | 4 | provider, archive, or filesystem failure |
//! | 5 | jobs still in flight — bought, recorded, resumable by re-running |
//!
//! 5 is deliberately not 0: a cron that treats "still processing" as success
//! never comes back for the data it paid for.

use std::path::PathBuf;

use crucible_core::types::NanoUsd;
use crucible_data::catalog::{Catalog, TsRange};
use crucible_data::ingest::money::{format_usd, parse_usd_to_nano};
use crucible_data::ingest::window::parse_civil_date;
use crucible_data::ingest::{PullRequest, StypeIn, WindowSpan, range_from_dates};

/// Exit code for a usage or environment error.
pub const EXIT_USAGE: i32 = 2;
/// Exit code for a refusal to spend.
pub const EXIT_REFUSED: i32 = 3;
/// Exit code for a provider, archive, or filesystem failure.
pub const EXIT_FAILED: i32 = 4;
/// Exit code for work that is paid for, recorded, and still in flight.
#[cfg(feature = "databento")]
pub const EXIT_IN_FLIGHT: i32 = 5;

/// Input symbology, as a CLI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum StypeArg {
    /// Raw exchange contract symbols, e.g. `ESU6`.
    RawSymbol,
    /// Parent symbology, e.g. `ES.FUT`.
    Parent,
    /// Vendor continuous series, e.g. `ES.v.0`.
    Continuous,
}

impl From<StypeArg> for StypeIn {
    fn from(arg: StypeArg) -> StypeIn {
        match arg {
            StypeArg::RawSymbol => StypeIn::RawSymbol,
            StypeArg::Parent => StypeIn::Parent,
            StypeArg::Continuous => StypeIn::Continuous,
        }
    }
}

/// How wide each download window is, as a CLI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum WindowArg {
    /// One job per calendar month — the recurring archival job's shape.
    Month,
    /// One job per contiguous gap — the bootstrap backfill's shape.
    Whole,
}

impl From<WindowArg> for WindowSpan {
    fn from(arg: WindowArg) -> WindowSpan {
        match arg {
            WindowArg::Month => WindowSpan::Month,
            WindowArg::Whole => WindowSpan::Whole,
        }
    }
}

/// Arguments to `crucible pull`.
#[derive(Debug, clap::Args)]
pub struct PullArgs {
    /// Vendor dataset, e.g. GLBX.MDP3.
    #[arg(long)]
    pub dataset: String,
    /// Vendor schema, e.g. ohlcv-1m.
    #[arg(long)]
    pub schema: String,
    /// Symbol keys, comma-separated. For parent symbology these are the
    /// parents (ES.FUT), not the contracts they expand to.
    #[arg(long, value_delimiter = ',', required = true)]
    pub symbols: Vec<String>,
    /// How to read `--symbols`.
    #[arg(long, value_enum, default_value = "parent")]
    pub stype_in: StypeArg,
    /// Inclusive UTC start date, YYYY-MM-DD.
    #[arg(long)]
    pub start: String,
    /// Exclusive UTC end date, YYYY-MM-DD.
    #[arg(long)]
    pub end: String,
    /// Window granularity: one job per month, or one per contiguous gap.
    #[arg(long = "window", value_enum, default_value = "month")]
    pub window_span: WindowArg,
    /// Quote and exit without spending. This is already the default; the flag
    /// exists so a script can say so out loud.
    #[arg(long)]
    pub dry_run: bool,
    /// Actually buy the plan. Requires --max-cost-usd.
    #[arg(long)]
    pub execute: bool,
    /// Hard ceiling on the quoted total, in dollars (e.g. 1.00, or 0.00 to
    /// proceed only while an entitlement covers the data).
    #[arg(long)]
    pub max_cost_usd: Option<String>,
    /// Seconds between job-status polls.
    #[arg(long, default_value_t = 15)]
    pub poll_secs: u64,
    /// Give up waiting after this many minutes and exit 5.
    #[arg(long, default_value_t = 60)]
    pub timeout_mins: u64,
    /// Buy again any window whose job expired before it was collected.
    #[arg(long)]
    pub repurchase_expired: bool,
}

/// The bin target's clock. The only implementation of
/// [`Clock`](crucible_data::ingest::clock::Clock) in the workspace that
/// touches the OS, kept here because library crates read no clock (D-0015,
/// CLAUDE.md §2.2).
///
/// Gated on either acquisition feature rather than on `databento` alone: it is
/// not a Databento detail, it is *the* clock, and `thetadata` needs the same
/// one to stamp `fetched_ts` on an inventory line. Two OS-clock implementations
/// would be exactly the duplication D-0032 exists to prevent.
#[cfg(any(feature = "databento", feature = "thetadata"))]
pub struct SystemClock;

#[cfg(any(feature = "databento", feature = "thetadata"))]
impl crucible_data::ingest::clock::Clock for SystemClock {
    #[expect(
        clippy::disallowed_methods,
        reason = "D-0032: THE one implementation in the workspace that reads the \
                  OS clock, in a bin target for exactly that reason. Everything \
                  downstream takes the instant as an argument (D-0015)."
    )]
    fn now_ts(&self) -> crucible_core::types::Ts {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        crucible_core::types::Ts(i64::try_from(nanos).unwrap_or(i64::MAX))
    }

    fn sleep_secs(&self, secs: u64) {
        std::thread::sleep(std::time::Duration::from_secs(secs));
    }
}

/// Resolves `$CRUCIBLE_DATA_DIR`, which has no default by design.
pub fn data_dir() -> Result<PathBuf, String> {
    match std::env::var("CRUCIBLE_DATA_DIR") {
        Ok(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => Err(
            "CRUCIBLE_DATA_DIR is not set. The archive root has no default and \
                  must live outside the repo — see `crucible env`."
                .to_string(),
        ),
    }
}

/// Parses `--start`/`--end` into a day-aligned, half-open range.
fn parse_range(start: &str, end: &str) -> Result<TsRange, String> {
    let s = parse_civil_date(start)
        .ok_or_else(|| format!("--start {start:?} is not a YYYY-MM-DD calendar date"))?;
    let e = parse_civil_date(end)
        .ok_or_else(|| format!("--end {end:?} is not a YYYY-MM-DD calendar date"))?;
    range_from_dates((s.year, s.month, s.day), (e.year, e.month, e.day))
        .map_err(|err| err.to_string())
}

/// Parses `--max-cost-usd` without ever building an `f64` (D-0027).
fn parse_cap(text: Option<&String>) -> Result<Option<NanoUsd>, String> {
    match text {
        None => Ok(None),
        Some(raw) => parse_usd_to_nano(raw)
            .map(Some)
            .map_err(|e| format!("--max-cost-usd {raw:?}: {e}")),
    }
}

/// Maps a library refusal or failure onto this command's exit codes.
#[cfg(feature = "databento")]
fn exit_code_for(err: &crucible_data::ingest::IngestError) -> i32 {
    use crucible_data::ingest::IngestError;
    match err {
        IngestError::CostGateRefused { .. } | IngestError::ExecuteWithoutCap => EXIT_REFUSED,
        IngestError::PollTimedOut { .. } => EXIT_IN_FLIGHT,
        _ => EXIT_FAILED,
    }
}

/// Runs the command, returning the process exit code.
pub fn run(args: &PullArgs) -> i32 {
    if args.dry_run && args.execute {
        eprintln!("error: --dry-run and --execute contradict each other");
        return EXIT_USAGE;
    }
    if !args.execute && args.max_cost_usd.is_some() {
        eprintln!(
            "error: --max-cost-usd only means something with --execute; a dry run \
             spends nothing and needs no cap"
        );
        return EXIT_USAGE;
    }
    // `gate` refuses a capless execution too, and it is the authority — but it
    // only runs after the provider has been built and the plan priced, which
    // means "you forgot the cap" would otherwise be reported *after* several
    // network round trips, and would be unreachable at all without a working
    // API key. The rule is a property of the arguments; check it here.
    if args.execute && args.max_cost_usd.is_none() {
        eprintln!(
            "error: --execute requires an explicit --max-cost-usd. A default \
             spending cap is a number nobody chose."
        );
        return EXIT_REFUSED;
    }

    let range = match parse_range(&args.start, &args.end) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    let cap = match parse_cap(args.max_cost_usd.as_ref()) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    let dir = match data_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };

    let request = PullRequest {
        dataset: args.dataset.clone(),
        schema: args.schema.clone(),
        symbols: args.symbols.clone(),
        stype_in: args.stype_in.into(),
        range,
        window_span: args.window_span.into(),
    };

    println!(
        "pull {} / {} / {} [{}]  {} .. {}  windows: {}",
        request.dataset,
        request.schema,
        request.symbols.join(","),
        request.stype_in,
        args.start,
        args.end,
        request.window_span,
    );
    if args.execute {
        println!(
            "mode: EXECUTE, cap {}",
            cap.map_or_else(|| "(none)".to_string(), format_usd)
        );
    } else {
        println!("mode: dry run — prices the plan and exits without spending");
    }
    println!();

    run_planned(&dir, &request, args, cap)
}

/// The half that needs a provider. Split out so the pure argument handling
/// above is identical whether or not this build can reach the network.
#[cfg(feature = "databento")]
fn run_planned(
    dir: &std::path::Path,
    request: &PullRequest,
    args: &PullArgs,
    cap: Option<NanoUsd>,
) -> i32 {
    use crucible_data::ingest::databento::DatabentoProvider;
    use crucible_data::ingest::delivery::DbnDelivery;
    use crucible_data::ingest::{Deps, ExecuteOptions, execute, gate, plan, quote};

    // Read as late as possible, hand it straight to the client, never store
    // it, never log it (D-0022 and the `ingest` module docs).
    let key = match std::env::var("DATABENTO_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!(
                "error: DATABENTO_API_KEY is not set. `pull` cannot authenticate \
                 without it — see `crucible env`."
            );
            return EXIT_USAGE;
        }
    };
    let mut provider = match DatabentoProvider::new(&key) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not reach the provider: {e}");
            return EXIT_FAILED;
        }
    };
    drop(key);

    let mut catalog = match Catalog::open(dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: archive catalog: {e}");
            return EXIT_USAGE;
        }
    };

    let planned = match plan(&catalog, &mut provider, request) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return exit_code_for(&e);
        }
    };
    let report = match quote(&mut provider, &planned) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return exit_code_for(&e);
        }
    };
    print!("{report}");

    if !args.execute {
        return 0;
    }
    if let Err(e) = gate(&report, cap) {
        eprintln!("\nerror: {e}");
        return exit_code_for(&e);
    }
    if report.is_empty() {
        println!("\nnothing to buy.");
        return 0;
    }

    let clock = SystemClock;
    let inspector = DbnDelivery;
    let opts = ExecuteOptions {
        poll_secs: args.poll_secs,
        timeout_secs: args.timeout_mins.saturating_mul(60),
        submit_spacing_secs: 3,
        repurchase_expired: args.repurchase_expired,
    };
    let mut deps = Deps {
        provider: &mut provider,
        inspector: &inspector,
        clock: &clock,
    };

    println!("\nexecuting…");
    match execute(&mut catalog, &mut deps, &planned, &report, &opts) {
        Ok(done) => {
            print!("{done}");
            0
        }
        Err(e) => {
            eprintln!("\nerror: {e}");
            exit_code_for(&e)
        }
    }
}

#[cfg(not(feature = "databento"))]
fn run_planned(
    _dir: &std::path::Path,
    _request: &PullRequest,
    _args: &PullArgs,
    _cap: Option<NanoUsd>,
) -> i32 {
    eprintln!(
        "error: this build cannot reach Databento. Rebuild with the feature:\n\
        \x20      cargo run -p crucible-cli --features databento -- pull ...\n\
        \x20The feature is off by default so ordinary builds and CI stay free of\n\
        \x20the async client's dependency graph (D-0025)."
    );
    EXIT_USAGE
}

/// Runs `crucible verify`: re-hashes the archive against the manifest.
pub fn verify() -> i32 {
    let dir = match data_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    let catalog = match Catalog::open(&dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: archive catalog: {e}");
            return EXIT_USAGE;
        }
    };
    let report = match catalog.verify() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_FAILED;
        }
    };
    let records = catalog.records().len();
    // Supplements are named but not re-hashed: they carry symbols, not bytes
    // (D-0066). Saying so keeps `verify`'s count honest about what it read.
    let supplements = catalog.supplements().len();
    if supplements > 0 {
        println!(
            "manifest also carries {supplements} symbol supplement(s), which name no \
             bytes to verify (D-0066)"
        );
    }
    if report.is_clean() {
        println!("archive clean: {records} record(s) re-hashed, no findings");
        return 0;
    }
    println!("archive has {} finding(s):", report.findings.len());
    for finding in &report.findings {
        println!("  {finding}");
    }
    println!(
        "\nRaw is append-only: corrections are new acquisitions at new paths, \
         never edits (D-0017)."
    );
    EXIT_FAILED
}
