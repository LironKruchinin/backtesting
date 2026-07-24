//! Parallel run scheduling — **M3 spec, not yet implemented.**
//!
//! ## The model (docs/DECISIONS.md D-0005)
//! The unit of parallelism is the **run** (strategy × params × instrument ×
//! timeframe × fold), never threads inside a run. Runs are share-nothing
//! except `Arc`'d immutable market data; results merge deterministically at
//! the end (sort by run id — never rely on completion order).
//!
//! This is the ONLY crate that spawns threads (rayon). If you are about to
//! write `std::thread`/`tokio` anywhere else, stop and re-read CLAUDE.md §3.
//!
//! ## Scheduling shape
//! - Tasks are grouped by dataset key (instrument, timeframe, date-slice).
//!   Groups sharing a dataset run against a single `Arc` copy.
//! - A counting semaphore bounds *resident datasets*, not threads — that is
//!   the real memory control on a 32 GB box. Threads: rayon pool sized to
//!   physical cores by default; benchmark 8 vs 16 on the 5800X3D before
//!   changing the default (criterion evidence required, CLAUDE.md §7).
//! - **Multi-instance pass (the big win):** one replay pass over the data
//!   feeds K strategy instances (K grid combos) simultaneously, amortizing
//!   the data pass across the grid. Size K so combined instance state stays
//!   cache-resident (the 5800X3D's 96 MB L3 is the budget). Requires a
//!   `MultiStrategy` fan-out adapter in the engine (M3 engine work item).
//! - Determinism: seeds derive from (config_hash, combo_index, fold) — never
//!   from thread id, completion order, or time.

/// Placeholder for the M3 scheduler implementation.
#[derive(Debug, Clone, Copy)]
pub struct SchedulerPlan;
