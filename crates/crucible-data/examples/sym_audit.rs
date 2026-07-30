//! Archive forensics: how much of each delivery's symbology did the manifest
//! actually record?
//!
//! Reads every `manifest.jsonl` line, re-reads the DBN metadata of the file it
//! names, and classifies each observed raw symbol with the *same* predicate the
//! append used (`catalog::is_valid_symbol`). Calling the production predicate
//! rather than restating it is the point: a second copy of the rule would
//! drift, and an audit that disagrees with the code it audits proves nothing.
//!
//! Read-only. Touches no inventory, writes nothing.
//!
//! ```text
//! cargo run -p crucible-data --features databento --example sym_audit
//! ```

#[cfg(not(feature = "databento"))]
fn main() {
    eprintln!(
        "sym_audit reads DBN metadata, so it needs the vendor client:\n\
         \n    cargo run -p crucible-data --features databento --example sym_audit"
    );
}

#[cfg(feature = "databento")]
fn main() {
    audit::run();
}

#[cfg(feature = "databento")]
mod audit {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crucible_data::catalog::is_valid_symbol;
    use crucible_data::ingest::delivery::{DbnDelivery, DeliveryInspector};

    /// The subset of a manifest record this audit needs.
    #[derive(serde::Deserialize)]
    struct Line {
        schema: String,
        symbols: Vec<String>,
        file_path: String,
    }

    pub fn run() {
        let root = PathBuf::from(
            std::env::var("CRUCIBLE_DATA_DIR").expect("CRUCIBLE_DATA_DIR must be set"),
        );
        let manifest = std::fs::read_to_string(root.join("manifest.jsonl"))
            .expect("manifest.jsonl must be readable");
        let inspector = DbnDelivery;

        let mut total_observed = 0usize;
        let mut total_recordable = 0usize;
        let mut total_dropped = 0usize;
        // schema -> (dropped, observed)
        let mut by_schema: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut affected = 0usize;
        let mut lines = 0usize;

        println!(
            "{:<12} {:>9} {:>9} {:>8}  file",
            "schema", "observed", "recorded", "dropped"
        );

        for raw in manifest.lines() {
            if raw.trim().is_empty() {
                continue;
            }
            lines += 1;
            let line: Line = serde_json::from_str(raw).expect("manifest line must parse");
            let path = root.join(&line.file_path);

            let observed = match inspector.observed_symbols(&path) {
                Ok(v) => v,
                Err(e) => {
                    println!("!! could not read {}: {e}", line.file_path);
                    continue;
                }
            };

            let dropped: Vec<&String> = observed.iter().filter(|s| !is_valid_symbol(s)).collect();
            let recordable = observed.len() - dropped.len();

            total_observed += observed.len();
            total_recordable += recordable;
            total_dropped += dropped.len();
            let entry = by_schema.entry(line.schema.clone()).or_insert((0, 0));
            entry.0 += dropped.len();
            entry.1 += observed.len();
            if !dropped.is_empty() {
                affected += 1;
            }

            println!(
                "{:<12} {:>9} {:>9} {:>8}  {}",
                line.schema,
                observed.len(),
                line.symbols.len(),
                dropped.len(),
                line.file_path
            );
            if !dropped.is_empty() {
                let sample: Vec<&str> = dropped.iter().take(3).map(|s| s.as_str()).collect();
                println!("                                            e.g. {sample:?}");
            }
        }

        println!();
        println!("manifest lines            {lines}");
        println!("lines with dropped syms   {affected}");
        println!("observed symbols total    {total_observed}");
        println!("recordable (kept)         {total_recordable}");
        println!("dropped                   {total_dropped}");
        if total_observed > 0 {
            let pct = (total_dropped as f64 / total_observed as f64) * 100.0;
            println!("dropped share             {pct:.2} %");
        }
        println!();
        println!("by schema (dropped / observed):");
        for (schema, (dropped, observed)) in &by_schema {
            println!("  {schema:<12} {dropped:>8} / {observed:<8}");
        }
    }
}
