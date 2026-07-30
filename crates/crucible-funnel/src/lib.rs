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
//! [`walkforward`] cuts a grid's replay into train/test folds by trading day
//! and reports each statistic on the window it names (M2). It is the machinery
//! S2 runs on, not S2 itself: it produces evidence and stops there, and says so
//! in its own footer.
//!
//! [`funnel`] is what turns that evidence into a verdict — claiming every run
//! in the [`registry`] before it executes, replaying the grid across
//! [`scheduler`]'s pool, screening cost-free, sweeping the cost assumption,
//! running the two mandatory controls, and judging against [`stages`]'
//! pre-registered criteria. [`scorecard`] renders the result with the honesty
//! box §2.4 and §2.5 require.
//!
//! **What is still spec**: [`stats`] — deflated Sharpe, PBO/CSCV, the
//! permutation nulls and the truncation harness — and with it stage S3, which
//! [`stages`] therefore **refuses** rather than skipping. Because S3 is what
//! "survived the full battery" means, this build cannot award
//! [`Verdict::Graduate`]; its ceiling is [`Verdict::Iterate`], and every report
//! says so. `crucible-strategies::controls::LeakyZScore` is the planted defect
//! those detectors will be measured against, and
//! `tests/planted_leak.rs` records that today's gates do not catch it.

pub mod funnel;
pub mod grid;
pub mod registry;
pub mod scheduler;
pub mod scorecard;
pub mod stages;
pub mod stats;
pub mod walkforward;

pub use funnel::{
    ComboOutcome, Control, CostLevel, Costs, FunnelError, FunnelInputs, FunnelReport, run_funnel,
    survivors,
};
pub use registry::{Registry, RegistryError, RunKey, RunRow, RunStatus, VerdictRow};
pub use stages::{Assessment, Criteria, CriteriaError, CriteriaSource, Stage, Verdict, assess};
pub use walkforward::{FoldPlan, FoldScheme, FoldSpec, WalkForwardReport};
