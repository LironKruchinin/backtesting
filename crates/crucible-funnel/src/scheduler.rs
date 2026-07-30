//! Parallel run scheduling.
//!
//! ## The model (docs/DECISIONS.md D-0005)
//!
//! The unit of parallelism is the **run** (strategy × params × instrument ×
//! timeframe × fold), never threads inside a run. Runs are share-nothing
//! except immutable market data; results merge deterministically — sorted by
//! run identity, never by completion order.
//!
//! This is the ONLY crate that spawns threads (rayon). If you are about to
//! write `std::thread`/`tokio` anywhere else, stop and re-read CLAUDE.md §3.
//!
//! ## What determinism costs here, and why it costs nothing
//!
//! [`run_grid_parallel`] is `into_par_iter().map(..).collect::<Vec<_>>()` over
//! the grid's **indices**. That is not a convenience: rayon's indexed
//! `collect` writes each result to its own slot, so the output vector is in
//! grid-index order whatever order the work finished in, and §2.2's
//! merge-by-identity rule is satisfied by construction rather than by a sort
//! afterwards.
//!
//! Two smaller things follow from the same rule and are easy to get wrong:
//!
//! - Errors are collected **per index** and the *lowest-indexed* one is
//!   returned. `collect::<Result<Vec<_>, _>>()` would return whichever error
//!   rayon noticed first, so the same broken grid would report a different
//!   failure on a different machine.
//! - Nothing is reduced across combos. Every float in a
//!   [`ComboWalkForward`][crate::walkforward::ComboWalkForward] is computed
//!   inside one combo's own [`Summary`][crucible_engine::Summary], so no
//!   floating-point sum's order depends on the thread count — the specific
//!   nondeterminism §2.2 names.
//!
//! The proof is `the_parallel_scheduler_agrees_with_the_serial_one` in
//! `crate::walkforward::tests`, which replays a grid both ways and asserts the
//! reports are equal. A parallel path that agrees with a serial one on one
//! machine is evidence; a parallel path with no serial twin is a claim.
//!
//! ## What is deliberately not here yet
//!
//! - **A dataset semaphore.** The real memory control on a 32 GB box is the
//!   number of *resident datasets*, not the number of threads. One config runs
//!   one instrument on one shared `Arc`-free slice today (`combo` refuses two
//!   instruments), so there is exactly one dataset resident and nothing to
//!   bound. The semaphore lands with the cross-product over a universe.
//! - **The multi-instance pass** — one replay pass feeding K strategy
//!   instances, amortizing the data pass across the grid. It needs a
//!   `MultiStrategy` fan-out adapter in the engine, and it is a throughput
//!   change, so CLAUDE.md §7 wants criterion evidence with it rather than an
//!   assertion that it is faster.
//! - **Pool sizing.** rayon's default is one worker per logical core. Changing
//!   it is a performance claim and needs the same evidence.
//!
//! Seeds come from `(config_hash, root_seed, account, combo_index, fold)` —
//! never from a thread id, completion order, or time. That derivation is
//! [`crate::walkforward::derive_run_seed`]; use it, and do not write a second
//! one here.

use crucible_core::prelude::*;
use crucible_engine::BacktestParams;
use crucible_strategies::combo::Grid;
use rayon::prelude::*;

use crate::walkforward::{
    ComboWalkForward, FoldPlan, GridRun, RunIdentity, WalkForwardError, WalkForwardReport,
};

/// Replays every combo in `grid` across rayon's pool and returns the report in
/// **grid-index order**.
///
/// Identical in every number to
/// [`crate::walkforward::run_grid`] — see the module docs for why that is a
/// property of the shape rather than a hope, and for the test that checks it.
///
/// # Errors
/// [`WalkForwardError`] if the plan, the day keys or the grid do not describe
/// the series, or if any combo's replay fails. When several combos fail, the
/// error reported is the **lowest-indexed** one, so the same broken grid
/// reports the same failure everywhere.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors `walkforward::run_grid` exactly; a different argument list on the parallel \
              path is how the two stop being the same run"
)]
pub fn run_grid_parallel<M: FillModel + Clone + Sync>(
    events: &[MarketEvent],
    day_keys: &[i64],
    grid: &Grid,
    plan: &FoldPlan,
    spec: &ContractSpec,
    params: &BacktestParams,
    identity: &RunIdentity,
    fill_model: &M,
) -> Result<WalkForwardReport, WalkForwardError> {
    let run = GridRun::new(
        events, day_keys, grid, plan, spec, params, identity, fill_model,
    )?;

    let results: Vec<Result<ComboWalkForward, WalkForwardError>> = (0..run.len())
        .into_par_iter()
        .map(|i| run.one_combo(i))
        .collect();

    let mut combos = Vec::with_capacity(results.len());
    for result in results {
        combos.push(result?);
    }
    Ok(WalkForwardReport { combos })
}
