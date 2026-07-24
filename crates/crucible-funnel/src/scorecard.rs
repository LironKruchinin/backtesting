//! Verdict scorecards — **M3 spec, not yet implemented.**
//!
//! The scorecard is Crucible's user-facing output: one self-contained HTML
//! file per evaluated idea answering "is this worth pursuing?" with evidence
//! visible. It is also the portfolio artifact a reviewer clicks — quality
//! bar is accordingly high.
//!
//! ## Required sections (order fixed)
//! 1. **Verdict banner** — Kill / Iterate / Graduate, the stage that decided
//!    it, and the pre-registered criteria it was judged against.
//! 2. **Honesty box** — fill model name + parameters, total trial count for
//!    the hypothesis family (from the registry), deflated Sharpe next to
//!    naive Sharpe, config hash, git sha, data manifest ids. Nothing in this
//!    box is optional; a scorecard without its honesty box does not render.
//! 3. Equity curves — IS/OOS walk-forward panels, per-fold.
//! 4. **Cost sensitivity** — PnL at 0/0.5/1/2 ticks; the single most
//!    decision-relevant chart in the file.
//! 5. Parameter plateau heatmap (metric over the grid; survivors should sit
//!    on hills, not spikes).
//! 6. Regime table — per-year / per-vol-regime stats.
//! 7. Null comparison — real metric vs permutation distribution, empirical
//!    p-value.
//! 8. Trade stats — counts, win rate, holding times, fees paid.
//!
//! Rendering: self-contained static HTML (inline CSS/JS/data), no server, no
//! external CDN fetches; files must open from disk years later.

/// Placeholder for the M3 scorecard implementation.
#[derive(Debug, Clone, Copy)]
pub struct ScorecardPlan;
