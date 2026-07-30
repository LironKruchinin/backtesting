//! `crucible symbol-supplement` — does the manifest list every symbol its
//! files contain, and if not, credit the rest (D-0066).
//!
//! `verify` asks whether the bytes are the bytes we recorded. This asks the
//! other completeness question: whether the *symbology* we recorded is the
//! symbology those bytes declare. It is a different question with a different
//! consequence — a symbol missing from a record makes `coverage` report data we
//! own as missing, and the next pull buys it again (D-0033).
//!
//! It reads each archived file's DBN header through the same decoder the ingest
//! path uses, compares it with what the manifest credits, and — with
//! `--execute` — appends one `SymbolSupplement` line per record that is short.
//! **Nothing existing is rewritten**: the manifest is append-only, so a
//! correction is a new line pointing at the old one's manifest id.
//!
//! A dry run by default, like `pull`: this one cannot spend money, but it does
//! write to the archive's most load-bearing file, and "look first" is the house
//! rule for anything that does.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | every record's symbology is fully credited |
//! | 2 | nothing could be checked — no `CRUCIBLE_DATA_DIR`, no archive, no manifest, or a build without the `databento` feature |
//! | 4 | symbols are missing (dry run), a file could not be decoded, some symbol is still unrecordable, or an append failed |
//!
//! 4 rather than 0 on a dry run that finds something, for the reason `qa` and
//! `verify` exit 4: a scheduled job that reads "20 % of symbols missing" as
//! success is worse than no job at all.

use clap::Args;

use crate::pull::{EXIT_USAGE, data_dir};

/// Arguments for `crucible symbol-supplement`.
#[derive(Args, Debug)]
pub struct SupplementArgs {
    /// Append the supplement records. Without this the command only reports.
    #[arg(long)]
    pub execute: bool,
    /// Text recorded in each supplement's `reason` field.
    #[arg(long, default_value = "D-0066 manifest symbol completeness repair")]
    pub reason: String,
}

/// Runs the command, returning the process exit code.
pub fn run(args: &SupplementArgs) -> i32 {
    let dir = match data_dir() {
        Ok(dir) => dir,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    inspect(&dir, args)
}

#[cfg(feature = "databento")]
fn inspect(dir: &std::path::Path, args: &SupplementArgs) -> i32 {
    use crate::pull::EXIT_FAILED;
    use crucible_data::Catalog;
    use crucible_data::ingest::clock::Clock;
    use crucible_data::ingest::delivery::DbnDelivery;
    use crucible_data::ingest::supplement;

    let mut catalog = match Catalog::open(dir) {
        Ok(catalog) => catalog,
        Err(e) => {
            eprintln!("error: archive catalog: {e}");
            return EXIT_USAGE;
        }
    };

    let inspector = DbnDelivery;
    let found = supplement::plan(&catalog, &inspector);
    print_plan(&found, args.execute);

    if found.is_complete() {
        println!("\nnothing to do: every record credits every symbol its file declares.");
        return if found.undecodable.is_empty() {
            0
        } else {
            EXIT_FAILED
        };
    }

    if !args.execute {
        println!(
            "\ndry run — nothing was appended. To record the correction:\n\
             \x20 cargo run -p crucible-cli --features databento -- \
             symbol-supplement --execute"
        );
        return EXIT_FAILED;
    }

    // The bin target owns the clock (D-0032): `recorded_ts` is stamped here so
    // no library code reads one.
    let recorded_ts = crate::pull::SystemClock.now_ts();
    match supplement::apply(&mut catalog, &found, recorded_ts, &args.reason) {
        Ok(written) => {
            let symbols: usize = written.iter().map(|s| s.added_symbols.len()).sum();
            println!(
                "\nappended {} supplement line(s) crediting {symbols} symbol(s).",
                written.len()
            );
            // Re-measure rather than assert: the completion proof is a second
            // pass over the archive, not the append's own opinion of itself.
            let after = supplement::plan(&catalog, &inspector);
            if after.is_complete() {
                println!("re-checked: every record now credits every symbol its file declares.");
                0
            } else {
                println!(
                    "re-checked: {} symbol(s) still missing, {} unrecordable, \
                     {} file(s) undecodable — the repair is INCOMPLETE.",
                    after.missing_total(),
                    after.unrecordable_total(),
                    after.undecodable.len()
                );
                EXIT_FAILED
            }
        }
        Err(e) => {
            eprintln!("\nerror: {e}");
            eprintln!(
                "       earlier supplements are already durable; re-running resumes \
                 where this stopped."
            );
            EXIT_FAILED
        }
    }
}

/// Prints the findings in manifest record order.
#[cfg(feature = "databento")]
fn print_plan(found: &crucible_data::ingest::supplement::SupplementPlan, execute: bool) {
    println!(
        "manifest symbol completeness — {} record(s) re-read from their DBN headers",
        found.records_read
    );
    println!("mode: {}", if execute { "execute" } else { "dry run" });
    println!();
    if found.gaps.is_empty() && found.undecodable.is_empty() {
        return;
    }
    println!(
        "  {:>9} {:>9} {:>9} {:>13}  file",
        "observed", "credited", "missing", "unrecordable"
    );
    for gap in &found.gaps {
        println!(
            "  {:>9} {:>9} {:>9} {:>13}  {}",
            gap.observed,
            gap.credited,
            gap.missing.len(),
            gap.unrecordable.len(),
            gap.file_path
        );
        if let Some(first) = gap.missing.first() {
            println!("{:>34}e.g. {first:?}", "");
        }
        for symbol in &gap.unrecordable {
            println!("{:>34}UNRECORDABLE {symbol:?}", "");
        }
    }
    for bad in &found.undecodable {
        println!("  !! could not decode {}: {}", bad.file_path, bad.detail);
    }
    println!();
    println!("missing total       {}", found.missing_total());
    println!("unrecordable total  {}", found.unrecordable_total());
    println!("undecodable files   {}", found.undecodable.len());
}

#[cfg(not(feature = "databento"))]
fn inspect(_dir: &std::path::Path, _args: &SupplementArgs) -> i32 {
    eprintln!(
        "error: this build cannot decode DBN. Rebuild with the feature:\n\
        \x20      cargo run -p crucible-cli --features databento -- symbol-supplement\n\
        \x20The symbols are recovered from each archived file's own DBN header, through\n\
        \x20the same decoder the ingest path uses — never a second parser (D-0031/D-0066)."
    );
    EXIT_USAGE
}
