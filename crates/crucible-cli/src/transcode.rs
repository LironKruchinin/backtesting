//! `crucible transcode` — raw DBN into curated Parquet.
//!
//! Wiring only: resolve the archive, hand the catalog to
//! [`crucible_data::transcode`], print what it returns, map its errors onto
//! exit codes. Every judgement — which schemas are bars, which records are
//! trustworthy, what a partition is named — lives in the library (CLAUDE.md
//! §3).
//!
//! It needs the `databento` feature for the same reason `pull` does: DBN
//! decoding is reached through the vendor client's own re-export so the
//! decoder can never drift from the client that wrote the file (D-0031).
//! That costs nothing operationally — the machine that transcodes is the
//! machine that pulled.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | done — curated data is up to date |
//! | 2 | usage, configuration, or environment error |
//! | 4 | a raw file could not be trusted, or a write failed |

use crate::pull::{EXIT_USAGE, data_dir};

/// Arguments to `crucible transcode`.
#[derive(Debug, clap::Args)]
pub struct TranscodeArgs {
    /// Only write partitions for these instruments, comma-separated
    /// (e.g. ESH4,ESM4). Default: every instrument each file resolves to.
    #[arg(long, value_delimiter = ',')]
    pub symbols: Vec<String>,
    /// Only transcode these manifest ids (`file_blake3`), comma-separated.
    #[arg(long = "manifest-id", value_delimiter = ',')]
    pub manifest_ids: Vec<String>,
    /// Rebuild partitions that already exist. Curated data is derived and
    /// disposable, so this is safe; without it, existing partitions are left
    /// alone and re-running is a cheap no-op.
    #[arg(long)]
    pub force: bool,
    /// Also write partitions for spread instruments (`ESH4-ESM4`,
    /// `CL:BF F0-G0-H0`). Off by default: a parent window resolves to far more
    /// spreads than outrights — `GC.FUT` to 12,661 of them — and nothing
    /// replays one yet. The records excluded are counted and printed, and
    /// `raw/` keeps every spread forever, so this is one rebuild away (D-0070).
    #[arg(long)]
    pub include_spreads: bool,
}

/// Runs the command, returning the process exit code.
pub fn run(args: &TranscodeArgs) -> i32 {
    let dir = match data_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    run_with_feature(&dir, args)
}

/// The half that needs a DBN decoder. Split out so argument handling above is
/// identical whether or not this build has one.
#[cfg(feature = "databento")]
fn run_with_feature(dir: &std::path::Path, args: &TranscodeArgs) -> i32 {
    use crucible_data::Catalog;
    use crucible_data::transcode::{TranscodeOptions, transcode};

    use crate::pull::EXIT_FAILED;

    let catalog = match Catalog::open(dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: archive catalog: {e}");
            return EXIT_USAGE;
        }
    };
    let opts = TranscodeOptions {
        symbols: (!args.symbols.is_empty()).then(|| args.symbols.clone()),
        manifest_ids: (!args.manifest_ids.is_empty()).then(|| args.manifest_ids.clone()),
        force: args.force,
        include_spreads: args.include_spreads,
    };

    println!(
        "transcode — {} manifest record(s){}{}",
        catalog.records().len(),
        if args.force { ", rebuilding" } else { "" },
        if args.include_spreads {
            ", spreads included"
        } else {
            ", spreads excluded (--include-spreads)"
        }
    );
    println!();

    match transcode(&catalog, &opts) {
        Ok(report) => {
            print!("{report}");
            0
        }
        // A contradiction between two flags is the operator's mistake, not the
        // archive's, so it exits like any other usage error.
        Err(e @ crucible_data::transcode::TranscodeError::ContradictorySpreadFilter { .. }) => {
            eprintln!("error: {e}");
            EXIT_USAGE
        }
        Err(e) => {
            eprintln!("error: {e}");
            EXIT_FAILED
        }
    }
}

#[cfg(not(feature = "databento"))]
fn run_with_feature(_dir: &std::path::Path, _args: &TranscodeArgs) -> i32 {
    eprintln!(
        "error: this build has no DBN decoder. Rebuild with the feature:\n\
        \x20      cargo run -p crucible-cli --features databento -- transcode\n\
        \x20The decoder comes from the Databento client's own re-export so it\n\
        \x20cannot drift from the client that wrote the file (D-0031), and that\n\
        \x20client is behind a non-default feature (D-0025)."
    );
    EXIT_USAGE
}
