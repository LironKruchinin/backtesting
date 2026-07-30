//! `crucible rolls` — build and print a continuous-contract roll table.
//!
//! Wiring only. Which contract follows which, when a roll takes effect, and
//! what the gap between two contracts is all live in
//! [`crucible_data::continuous`]; this file resolves the archive, turns flags
//! into a [`RollRule`], prints the table, and maps errors onto exit codes
//! (CLAUDE.md §3 — logic in the CLI is a smell).
//!
//! A run is **read-only by default**: it prints the table it computed and
//! writes nothing. `--write` puts it under
//! `curated/rolls/{root}/{tf}/{rule}.json`. That is the same shape `pull` uses
//! for a reason — the default action of a command should not change what is
//! on disk — though nothing here can spend money, so there is no cap to pass.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | a table was computed (and written, if asked) |
//! | 2 | usage error, or nothing curated to build from |
//! | 4 | curated data exists but a table could not be built from it |

use crucible_core::prelude::*;
use crucible_data::continuous::{
    ContinuousError, ContractSymbol, DecadeAnchor, NO_EXPIRY_SOURCE, NOMINAL_EXPIRY_SOURCE,
    RollRule, RollTable, RollTableInput, build_roll_table, gather_series, nominal_expiries,
    roll_table_path, write_roll_table,
};
use crucible_data::ingest::window::date_of;

use crate::pull::{EXIT_FAILED, EXIT_USAGE, data_dir};

/// Arguments to `crucible rolls`.
#[derive(Debug, clap::Args)]
pub struct RollsArgs {
    /// Product root to stitch, e.g. ES. One table, one root.
    #[arg(long)]
    pub root: String,
    /// Bar interval the volume comparison is measured at: 1s 1m 5m 15m 1h 1d.
    #[arg(long, default_value = "1m")]
    pub timeframe: String,
    /// Consecutive sessions the next contract must out-trade the front before
    /// the roll is taken (the `.v` rule).
    #[arg(long, default_value_t = 1)]
    pub confirm_days: u32,
    /// Use the `.c` rule instead: roll this many calendar days before the
    /// front contract's expiry.
    #[arg(long)]
    pub calendar_days: Option<u32>,
    /// Reference year for resolving one-digit contract years (`ESH4`). The
    /// default is pinned in `crucible_data::continuous::symbol`, never read
    /// from a clock; override it for an archive in another decade.
    #[arg(long)]
    pub decade_anchor: Option<i32>,
    /// Where expiries come from: `auto` prefers a `definition` file in the
    /// archive and falls back to the nominal third-Friday rule, `nominal`
    /// forces the rule, `none` supplies none at all.
    #[arg(long, default_value = "auto")]
    pub expiries: String,
    /// Write the table to curated/rolls/. Without this the command prints
    /// and changes nothing.
    #[arg(long)]
    pub write: bool,
}

/// Locates an archived `definition` file covering `root`.
///
/// The lookup itself lives in [`Catalog::definition_file`] — `transcode` needs
/// the same file to name its curated partitions (D-0072), and CLAUDE.md §3 puts
/// logic in the owning crate rather than in two CLI commands that agree by
/// accident.
///
/// [`Catalog::definition_file`]: crucible_data::Catalog::definition_file
#[cfg(feature = "databento")]
fn archived_definitions(dir: &std::path::Path, root: &str) -> Option<std::path::PathBuf> {
    crucible_data::Catalog::open(dir)
        .ok()?
        .definition_file(root)
}

/// Recorded in the table when expiries came from an archived `definition` file.
#[cfg(feature = "databento")]
const ARCHIVED_EXPIRY_SOURCE: &str = "databento-definition";

/// A set of expiries and the name recorded beside them in the roll table.
type Expiries = (std::collections::BTreeMap<ContractSymbol, Ts>, &'static str);

/// What `--expiries` resolved to.
enum ExpiryChoice {
    /// Use exactly this map.
    Resolved(Expiries),
    /// Fall back to the nominal third-Friday rule.
    Nominal,
    /// Refuse, with this exit code.
    Refused(i32),
}

/// Resolves `--expiries`.
///
/// A default build has no DBN decoder, so `auto` quietly takes the nominal
/// route and the table records that it did — expiries are a backstop for the
/// volume rule, not its input, so failing the command would be wrong.
fn resolve_expiries(
    choice: &str,
    dir: &std::path::Path,
    root: &str,
    anchor: DecadeAnchor,
) -> ExpiryChoice {
    match choice {
        "none" => ExpiryChoice::Resolved((std::collections::BTreeMap::new(), NO_EXPIRY_SOURCE)),
        "nominal" => ExpiryChoice::Nominal,
        "auto" => resolve_auto(dir, root, anchor),
        other => {
            eprintln!("error: --expiries {other:?} is not one of: auto, nominal, none");
            ExpiryChoice::Refused(EXIT_USAGE)
        }
    }
}

#[cfg(feature = "databento")]
fn resolve_auto(dir: &std::path::Path, root: &str, anchor: DecadeAnchor) -> ExpiryChoice {
    let Some(path) = archived_definitions(dir, root) else {
        return ExpiryChoice::Nominal;
    };
    match crucible_data::continuous::expiries_from_definitions(&path, root, anchor) {
        Ok(map) if !map.is_empty() => {
            println!("  expiries read from {}", path.display());
            ExpiryChoice::Resolved((map, ARCHIVED_EXPIRY_SOURCE))
        }
        // A definition file with nothing for this root is not an error — it
        // may simply predate the contract — but silently substituting nominal
        // dates would hide it.
        Ok(_) => {
            eprintln!(
                "warning: {} carries no outright {root} expiries; using the \
                 nominal third-Friday rule",
                path.display()
            );
            ExpiryChoice::Nominal
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExpiryChoice::Refused(EXIT_FAILED)
        }
    }
}

#[cfg(not(feature = "databento"))]
fn resolve_auto(_dir: &std::path::Path, _root: &str, _anchor: DecadeAnchor) -> ExpiryChoice {
    ExpiryChoice::Nominal
}

/// Runs the command, returning the process exit code.
pub fn run(args: &RollsArgs) -> i32 {
    let tf: TimeFrame = match args.timeframe.parse() {
        Ok(tf) => tf,
        Err(e) => {
            eprintln!("error: --timeframe: {e}");
            return EXIT_USAGE;
        }
    };
    let rule = match args.calendar_days {
        Some(days) => RollRule::CalendarDaysBeforeExpiry { days },
        None => RollRule::VolumeCrossover {
            confirm_days: args.confirm_days,
        },
    };
    if let Err(e) = rule.validate() {
        eprintln!("error: {e}");
        return EXIT_USAGE;
    }
    let anchor = args
        .decade_anchor
        .map_or(DecadeAnchor::DEFAULT, DecadeAnchor::new);
    let dir = match data_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };

    let gathered = match gather_series(&dir, &args.root, tf, anchor) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_FAILED;
        }
    };
    // Named, not silent: a mistyped root shows up here as "everything was
    // skipped" rather than as an empty table. One line, because a real
    // archive skips every calendar spread it holds (D-0033 keeps them).
    if !gathered.skipped.is_empty() {
        let names: Vec<&str> = gathered
            .skipped
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        println!(
            "skipped {} curated instrument(s) that are not outright {} contracts:\n  {}",
            names.len(),
            args.root,
            names.join(", ")
        );
    }

    // Where expiries came from is recorded in the table whichever route they
    // took — a number nobody can trace is a rumour (§2.5). The volume rule
    // does not consult them to decide *when* to roll, but the expiry backstop
    // uses them to decide *that* one must (D-0046), so real ones beat the
    // nominal third-Friday rule for both rules.
    let (expiries, expiry_source) = match resolve_expiries(&args.expiries, &dir, &args.root, anchor)
    {
        ExpiryChoice::Resolved(pair) => pair,
        ExpiryChoice::Nominal => (
            nominal_expiries(gathered.series.keys()),
            NOMINAL_EXPIRY_SOURCE,
        ),
        ExpiryChoice::Refused(code) => return code,
    };

    let table = match build_roll_table(&RollTableInput {
        root: &args.root,
        tf,
        rule,
        decade_anchor: anchor,
        expiries: &expiries,
        expiry_source,
        series: &gathered.series,
        sources: &gathered.sources,
    }) {
        Ok(table) => table,
        Err(e) => {
            eprintln!("error: {e}");
            return match e {
                ContinuousError::NoSeries { .. } | ContinuousError::InvalidRule { .. } => {
                    EXIT_USAGE
                }
                _ => EXIT_FAILED,
            };
        }
    };

    print_table(&table);

    if args.write {
        match write_roll_table(&dir, &table) {
            Ok(path) => println!("\nwritten to {}", path.display()),
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_FAILED;
            }
        }
    } else {
        match roll_table_path(&dir, &table.root, table.tf, &table.rule) {
            Ok(path) => println!(
                "\nnothing written (dry run). --write would place this at\n  {}",
                path.display()
            ),
            Err(e) => eprintln!("warning: {e}"),
        }
    }
    0
}

/// Prints the table and, with it, every assumption it rests on.
fn print_table(table: &RollTable) {
    println!(
        "\nCrucible rolls — {} {}, alias {}",
        table.root,
        table.tf,
        table.alias()
    );
    println!("  rule           {}", table.rule);
    println!("  adjustment     stored as per-roll gaps; applied at load");
    println!("  decade anchor  {}", table.decade_anchor);
    println!("  expiries       {}", table.expiry_source);
    println!(
        "  span           {} .. {} (ts_open)",
        date_of(table.first_ts_open),
        date_of(table.last_ts_open)
    );
    println!(
        "  contracts      {} ({} roll(s))",
        table.contracts.join(" -> "),
        table.rows.len()
    );
    println!("  sources        {} curated file(s)", table.sources.len());
    if table.rows.is_empty() {
        println!("\n  no rolls: a single contract covers the whole span.");
        return;
    }
    println!(
        "\n  {:<10} {:<10} {:<12} {:>14}",
        "from", "to", "roll date", "gap (points)"
    );
    for row in &table.rows {
        println!(
            "  {:<10} {:<10} {:<12} {:>14}",
            row.from_contract,
            row.to_contract,
            date_of(row.roll_ts).to_string(),
            row.adjustment.to_string()
        );
    }
    println!(
        "\n  The roll instant is the availability of the deciding session, and the new\n\
         \x20 contract is front only for bars available STRICTLY after it — the same\n\
         \x20 comparison the engine makes when it fills an order. Back-adjusted prices\n\
         \x20 are for signals; PnL uses the tradeable prices of the then-front contract."
    );
}
