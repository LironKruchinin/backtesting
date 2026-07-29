//! # crucible-funnel
//!
//! The research harness — the part of Crucible that turns "an idea" into
//! "a verdict" with as little human self-deception as possible.
//!
//! A strategy idea flows through staged gates, cheap and optimistic first,
//! expensive and brutal last. Most ideas must die at stage 0–1 within
//! seconds; that early killing is where weeks-to-thesis is won:
//!
//! | Stage | Question | Cost model | Kills on |
//! |-------|----------|-----------|----------|
//! | S0 signal triage | does it predict *anything*? | none (no trading) | no quantile spread / IC ≈ 0 |
//! | S1 coarse grid | could it work in the best case? | `FreeFills` | not clearly positive even cost-free |
//! | S2 walk-forward | does it survive honest costs OOS? | `spread_cross` | OOS collapse; dead at 1 tick sensitivity |
//! | S3 battery | is it real or a mining artifact? | `spread_cross`+ | fails plateau/regime/DSR/PBO/permutation |
//!
//! Verdicts are computed against criteria written down *before* the run
//! (config `[funnel]` section), because criteria chosen after seeing results
//! are not criteria, they're rationalization.
//!
//! [`walkforward`] is implemented (M2): it cuts a grid's replay into
//! train/test folds by trading day and reports each statistic on the window
//! it names. It is the machinery S2 runs on, not S2 itself — it produces
//! evidence and stops there, because a verdict needs the cost sweep, the
//! trial count and the battery that only this crate's M3 half provides.
//!
//! Everything else here except [`stages::Verdict`] is an **M3 spec encoded in
//! module docs**. Implement in this order: `grid` → `registry` →
//! `scheduler` → `stages` → `stats` → `scorecard`.

pub mod grid;
pub mod registry;
pub mod scheduler;
pub mod scorecard;
pub mod stages;
pub mod stats;
pub mod walkforward;

pub use stages::Verdict;
pub use walkforward::{FoldPlan, FoldScheme, FoldSpec, WalkForwardReport};
