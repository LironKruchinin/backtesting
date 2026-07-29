//! The walk-forward runner: every statistic computed on the window it claims.
//!
//! This is the component D-0061 deferred slicing to. `crucible combo` reports
//! one number per combo for a whole replay, so every equity curve it prints
//! carries a flat prefix as long as the grid's warmup and every naive Sharpe
//! is scaled by the same `sqrt(n_eval / n_total)`. That caveat retires here,
//! and only here: the flat runner still carries it, because it still reports
//! one window and that window still contains the warmup.
//!
//! ## What this is not
//!
//! Not a funnel stage. There are no verdicts, no kill criteria, no trial
//! registry and no cost-sensitivity sweep — those are M3, they live in
//! [`crate::stages`] / [`crate::registry`], and they consume what this
//! produces. A fold table is evidence, not a conclusion, and every report
//! this feeds says so in its footer.
//!
//! Not a parameter optimizer either. Nothing here re-fits anything on the
//! training window; the in-sample numbers exist so that a *later* selection
//! step has an honest sample to select on, and so a reader can see the gap
//! between the two. Selecting the best in-sample combo and quoting its
//! out-of-sample number is a research act with a name — it is what PBO
//! measures — and it belongs where the trial count is kept.
//!
//! ## The three modules
//!
//! - [`folds`] — where the boundaries are. Trading days, not months
//!   (D-0062); caller-supplied day keys, so this crate needs no calendar.
//! - [`window`] — how a statistic is cut out of one replay. The anchor bar,
//!   rebasing to declared capital, and delta-stitched pooling (D-0063).
//! - [`runner`] — one replay per combo, sliced into every fold.
//! - [`seed`] — `(config_hash, root_seed, combo_index, fold) -> u64`
//!   (CLAUDE.md §2.2), built before its first consumer on purpose.
//!
//! ## Fairness, and where it comes from
//!
//! §2.6 wants combos scored on identical evaluation windows. Two devices
//! deliver it and neither is in this crate's control flow: the bar series is
//! collected once and shared (so every combo replays literally the same
//! objects), and [`crucible_strategies::Aligned`] suppresses orders until the
//! grid's max warmup (so no combo trades bars its neighbours could not). What
//! this crate adds is the third: one [`FoldPlan`], built once from the grid's
//! warmup and shared by every combo, so two combos are cut at the same bars
//! or not compared at all. [`runner::run_grid`] refuses a plan whose warmup
//! disagrees with the grid's rather than trusting the caller to have passed
//! the right one.

pub mod folds;
pub mod runner;
pub mod seed;
pub mod window;

pub use folds::{Fold, FoldError, FoldPlan, FoldScheme, FoldSpec, Window};
pub use runner::{
    ComboWalkForward, FoldResult, RunIdentity, WalkForwardError, WalkForwardReport, run_grid,
};
pub use seed::derive_seed;
pub use window::RunTrace;

#[cfg(test)]
mod tests;
