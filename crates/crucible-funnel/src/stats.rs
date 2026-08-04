//! Overfitting-aware statistics — **partly implemented; the spec below marks
//! what is still owed.**
//!
//! Landed: [`permutation`] (D-0087), [`truncation`] (D-0088) and
//! [`deflated`] (D-0097). Still owed: **PBO/CSCV**, and the wiring that puts a
//! per-combo deflated Sharpe onto a scorecard — the estimator and its trial
//! count exist, the return series does not reach them yet, and the scorecard
//! says so in those words rather than printing a number it does not have.
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
//! - **PBO** — Bailey, Borwein, López de Prado & Zhu (2015), "The
//!   Probability of Backtest Overfitting" (CSCV). Partition the sample into
//!   S blocks, evaluate all IS/OOS recombinations, measure how often the IS
//!   winner underperforms OOS.
//! - **Permutation / null harness**: rerun the exact pipeline on (a) seeded
//!   random walks (`SyntheticFeed`) and (b) block-bootstrap shuffles of real
//!   returns (block length ≳ strategy horizon to preserve autocorrelation
//!   structure). Two readings: real-data edge should VANISH on nulls
//!   (otherwise suspect lookahead), and the null distribution of the metric
//!   gives an empirical p-value for the real result.
//! - **Truncation invariance** (engine correctness, run from CI): for
//!   sampled cut points t, decisions computed on data[0..t] must be
//!   bit-identical to decisions ≤ t computed on the full dataset. Catches
//!   lookahead that code review misses. Merge-blocking once implemented
//!   (CLAUDE.md §7).
//!
//! Implementation notes: statistics run on `f64` copies of results (§2.3
//! boundary); all resampling seeds derive from the run's seed lineage so
//! every p-value is reproducible bit-for-bit.

pub mod deflated;
pub mod pbo;
pub mod permutation;
pub mod truncation;

/// Placeholder for the M3 statistics implementation.
#[derive(Debug, Clone, Copy)]
pub struct StatsPlan;
