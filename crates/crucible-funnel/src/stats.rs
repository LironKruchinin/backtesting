//! Overfitting-aware statistics — **partly implemented; the spec below marks
//! what is still owed.**
//!
//! Landed: [`permutation`] (D-0087), [`truncation`] (D-0088), [`deflated`]
//! (D-0097) and [`pbo`] (D-0109) — the last of which also wired a per-combo
//! deflated Sharpe and a PBO onto the report and the scorecard, turning two
//! named holes into numbers and two S3 criteria into ones that are evaluated
//! rather than declared and ignored.
//!
//! **Still owed, and it is a wiring problem before it is a statistics one.**
//! [`permutation`] and [`truncation`] are built and hash-pinned and both caught
//! the planted leak in their own suites, but neither reaches a run:
//! `max_permutation_p` cannot be set from any config, so no scorecard has ever
//! carried a p-value, and the truncation verdict is a test-suite fact rather
//! than a reported one. Beyond those two, the plateau/perturbation test and the
//! cross-instrument rhyme check are not written. Those four are the whole of
//! what stands between this build and a declarable S3.
//!
//! This module is the quant-research heart of the project. Implement with
//! the papers open and cite them in doc comments:
//!
//! - **Deflated Sharpe Ratio** — **IMPLEMENTED** in [`deflated`] (D-0097).
//!   Bailey & López de Prado (2014), "The Deflated Sharpe Ratio". Corrects an
//!   observed Sharpe for the number of trials (from `registry`, per hypothesis
//!   family — never trust a hand-entered trial count), skewness, and kurtosis
//!   of returns. A headline Sharpe without its trial count is not reported
//!   anywhere. The trial count has exactly one door,
//!   [`deflated::trials_from_registry`], so that rule is enforced by there
//!   being no other way in.
//! - **PBO** — **IMPLEMENTED** in [`pbo`] (D-0109). Bailey, Borwein, López de
//!   Prado & Zhu (2015), "The Probability of Backtest Overfitting" (CSCV).
//!   Partition the sample into S blocks, evaluate all IS/OOS recombinations,
//!   measure how often the IS winner underperforms OOS. The blocks are the
//!   fold plan's folds, so [`crate::walkforward::FoldPlan`] stays the sole
//!   boundary authority.
//! - **Permutation / null harness** — **IMPLEMENTED** in [`permutation`]
//!   (D-0087), **and not reachable from a config**: rerun the exact pipeline on
//!   (a) seeded random walks (`SyntheticFeed`) and (b) block-bootstrap shuffles
//!   of real returns (block length ≳ strategy horizon to preserve
//!   autocorrelation structure). Two readings: real-data edge should VANISH on
//!   nulls (otherwise suspect lookahead), and the null distribution of the
//!   metric gives an empirical p-value for the real result. Wiring it to the
//!   run path owes a block-length sweep, a draw-count policy — a draw is a full
//!   grid replay — and a guard on a non-finite observed statistic, which today
//!   would score as maximally extreme and *pass*.
//! - **Truncation invariance** (engine correctness) — **IMPLEMENTED** in
//!   [`truncation`] (D-0088), green locally and **not yet gated in CI**: for
//!   sampled cut points t, decisions computed on data[0..t] must be
//!   bit-identical to decisions ≤ t computed on the full dataset. Catches
//!   lookahead that code review misses. Merge-blocking now that it exists
//!   (CLAUDE.md §7), which is a CI change still owed.
//!
//! Implementation notes: statistics run on `f64` copies of results (§2.3
//! boundary); all resampling seeds derive from the run's seed lineage so
//! every p-value is reproducible bit-for-bit.

pub mod deflated;
pub mod pbo;
pub mod permutation;
pub mod truncation;
