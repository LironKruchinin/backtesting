//! Trial registry and results store — **M3 spec, not yet implemented.**
//!
//! Research memory. Two jobs: make every run resumable/dedupable, and make
//! the *trial count* — the denominator of research honesty — automatic.
//!
//! ## Store (DuckDB file per project, `results/crucible.duckdb`)
//!
//! ```sql
//! runs(run_id, config_hash, combo_index, fold, seed, git_sha,
//!      data_manifest_ids, strategy, params_json, fill_model,
//!      started_at, finished_at, status)
//! metrics(run_id, final_equity_nano, return_pct, max_dd_pct, sharpe_naive,
//!         round_trips, win_rate, fees_nano)
//! equity_curves(run_id, parquet_path)      -- curves live in Parquet files
//! hypotheses(family_id, description, created_at)
//! trials(family_id, run_id)                -- feeds DSR's trial count
//! verdicts(config_hash, stage, verdict, reasons_json, decided_at)
//! ```
//!
//! ## Rules
//! - INSERT the run row *before* executing (status `running`): crashes leave
//!   visible corpses, not silent gaps.
//! - Dedup on (config_hash, combo_index, fold, seed): finished work is never
//!   recomputed; grids resume for free.
//! - Every config declares a `hypothesis_family`; every run increments that
//!   family's trial count. DSR reads the count from here — never from a
//!   human's memory of "a few dozen tries".
//! - The strategy graveyard is a query, not a document: killed configs with
//!   stage, reasons, and dates. Month three of research must not re-litigate
//!   month one.

/// Placeholder for the M3 registry implementation.
#[derive(Debug, Clone, Copy)]
pub struct RegistryPlan;
