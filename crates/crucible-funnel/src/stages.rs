//! Funnel stages and verdicts — **M3 spec except [`Verdict`].**
//!
//! ## Stage contracts
//!
//! **S0 — signal triage** (seconds; no trading, no fills). Compute the
//! signal on bars, bucket forward returns by signal quantile, check for a
//! monotonic relationship and a nonzero information coefficient. No
//! relationship ⇒ `Kill` — nothing downstream can rescue a signal that
//! predicts nothing.
//!
//! **S1 — coarse grid, free fills** (seconds–minutes). Wide-step grid under
//! `FreeFills`. This is the ONLY sanctioned use of `FreeFills` (CLAUDE.md
//! §2.4): a strategy that can't make money with free fills is dead, cheaply.
//! Keep surviving *regions* (plateaus), not single best points.
//!
//! **S2 — walk-forward with honest costs** (minutes). Survivor regions rerun
//! under `spread_cross` with rolling train/test folds. Also runs the
//! **cost-sensitivity sweep** (0 / 0.5 / 1 / 2 ticks): an edge that dies at
//! 1 tick is not an edge — `Kill`, and say so in the scorecard.
//!
//! **S3 — the battery** (the expensive one; survivors are rare by now):
//! - parameter perturbation: ±1 grid step neighbors must not collapse
//!   (plateau test)
//! - regime slices: per-year and per-vol-regime breakdown; flag
//!   single-regime PnL concentration
//! - deflated Sharpe (needs trial count from the registry) and PBO/CSCV
//! - permutation tests: the strategy must NOT retain performance on
//!   block-shuffled real data or seeded random walks (if it does, suspect a
//!   lookahead bug before celebrating)
//! - cross-instrument rhyme check (ES edge should at least rhyme on NQ/RTY)
//!
//! ## Rule zero
//! Kill criteria and thresholds come from the config, written before the
//! run. Post-hoc threshold adjustment is rationalization and does not ship.

/// The outcome of a funnel evaluation. `Kill` is the common, healthy case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Falsified at some stage; record the stage and reason in the scorecard.
    Kill,
    /// Survived with caveats worth another, sharper hypothesis iteration.
    Iterate,
    /// Survived the full battery; eligible for paper trading (M4).
    Graduate,
}

/// Placeholder for the M3 stage-runner implementation.
#[derive(Debug, Clone, Copy)]
pub struct StagesPlan;
