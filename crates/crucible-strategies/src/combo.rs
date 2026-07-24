//! Config-driven indicator combos — milestone M2 spec (not yet implemented).
//!
//! Goal: the strategies described in `configs/*.toml` are *assembled*, not
//! hand-coded. A combo config names indicators with parameter ranges plus
//! entry/exit rules; the funnel expands ranges into a parameter grid and
//! instantiates one `ComboStrategy` per grid point.
//!
//! Planned shape (keep this doc in sync with `configs/example-combo.toml`):
//!
//! - **Indicator nodes**: `{ id = "bb", kind = "bollinger", period = [10..40 step 5], k = [1.5, 2.0, 2.5] }`
//! - **Rules**: boolean expressions over node outputs and price, e.g.
//!   `enter_long = "close < bb.lower and ema_fast > ema_slow"`,
//!   evaluated on completed bars only.
//! - **Exits**: rule-based plus optional stop/target in ticks. Stops inside a
//!   bar are ambiguous (high-first vs low-first is unknowable from OHLC) —
//!   the engine must apply the **worst-case ordering** convention, and the
//!   scorecard must flag stop-dependent results as path-sensitive.
//!
//! Implementation notes for whoever builds this (likely with Claude Code):
//! - Parse rules into a tiny expression AST at config-load time; evaluating
//!   per bar must be allocation-free. No `eval`-style string parsing in the
//!   hot loop.
//! - A `ComboStrategy` instance must stay small (a few hundred bytes of
//!   state) — the funnel batches thousands of instances per data pass.
//! - Reject configs referencing unknown indicator ids or fields at load time
//!   (deny-unknown-fields policy, CLAUDE.md §5.5). A typo must be a hard
//!   error, never a silently-ignored rule.

/// Placeholder marker for the M2 combo implementation. Exists so the module
/// compiles and the plan above ships with the crate docs.
#[derive(Debug, Clone, Copy)]
pub struct ComboStrategyPlan;
