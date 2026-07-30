//! Archive forensics: does the manifest credit every symbol its files declare?
//!
//! Opens the archive with the production [`Catalog`], re-reads the DBN metadata
//! of every file it names, and reports four numbers per record: how many
//! symbols the file declares, how many the manifest credits (its own list plus
//! any D-0066 supplement), how many are missing, and how many the catalog's
//! symbol predicate still refuses. Everything comes from production code —
//! `Catalog::open`, `Catalog::credited_symbols`, `catalog::is_valid_symbol`,
//! `DbnDelivery` — because an audit that restates the rules it audits proves
//! nothing, and a second parser that disagreed with the ingest path would be a
//! finding of its own (D-0031).
//!
//! This is D-0066's **completion proof**: `dropped 0` and `missing 0`
//! archive-wide is what "the manifest tells the whole truth per contract" means.
//! `dropped 0` alone does not — a record can credit fewer symbols than its file
//! declares while the predicate refuses none of them, which is exactly the state
//! the repair had to fix.
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

    use crucible_data::Catalog;
    use crucible_data::catalog::is_valid_symbol;
    use crucible_data::ingest::delivery::{DbnDelivery, DeliveryInspector};

    pub fn run() {
        let root = PathBuf::from(
            std::env::var("CRUCIBLE_DATA_DIR").expect("CRUCIBLE_DATA_DIR must be set"),
        );
        let catalog = Catalog::open(&root).expect("the archive manifest must load");
        let inspector = DbnDelivery;

        let mut total_observed = 0usize;
        let mut total_credited = 0usize;
        let mut total_missing = 0usize;
        let mut total_dropped = 0usize;
        // schema -> (dropped, missing, observed)
        let mut by_schema: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
        let mut affected = 0usize;

        println!(
            "{:<12} {:>9} {:>9} {:>8} {:>8}  file",
            "schema", "observed", "credited", "missing", "dropped"
        );

        for record in catalog.records() {
            let path = root.join(&record.file_path);
            let observed = match inspector.observed_symbols(&path) {
                Ok(observed) => observed,
                Err(e) => {
                    println!("!! could not read {}: {e}", record.file_path);
                    continue;
                }
            };
            let credited = catalog.credited_symbols(record);

            // "dropped" is what the predicate refuses — the D-0066 mechanism.
            // "missing" is what the manifest does not credit for any reason —
            // the consequence, and the number `coverage` would re-buy.
            let dropped = observed.iter().filter(|s| !is_valid_symbol(s)).count();
            let missing = observed
                .iter()
                .filter(|s| !credited.contains(s.as_str()))
                .count();

            total_observed += observed.len();
            total_credited += credited.len();
            total_missing += missing;
            total_dropped += dropped;
            let entry = by_schema.entry(record.schema.clone()).or_insert((0, 0, 0));
            entry.0 += dropped;
            entry.1 += missing;
            entry.2 += observed.len();
            if missing > 0 || dropped > 0 {
                affected += 1;
            }

            println!(
                "{:<12} {:>9} {:>9} {:>8} {:>8}  {}",
                record.schema,
                observed.len(),
                credited.len(),
                missing,
                dropped,
                record.file_path
            );
            if missing > 0 {
                let sample: Vec<&str> = observed
                    .iter()
                    .filter(|s| !credited.contains(s.as_str()))
                    .take(3)
                    .map(String::as_str)
                    .collect();
                println!("                                                    e.g. {sample:?}");
            }
        }

        println!();
        println!("manifest records          {}", catalog.records().len());
        println!("symbol supplements        {}", catalog.supplements().len());
        println!("records with a shortfall  {affected}");
        println!("observed symbols total    {total_observed}");
        println!("credited total            {total_credited}");
        println!("missing                   {total_missing}");
        println!("dropped (predicate)       {total_dropped}");
        if total_observed > 0 {
            let pct = (total_missing as f64 / total_observed as f64) * 100.0;
            println!("missing share             {pct:.2} %");
        }
        println!();
        println!("by schema (dropped / missing / observed):");
        for (schema, (dropped, missing, observed)) in &by_schema {
            println!("  {schema:<12} {dropped:>8} / {missing:>8} / {observed:<8}");
        }
    }
}
