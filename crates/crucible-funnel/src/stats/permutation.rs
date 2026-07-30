//! The permutation null — block-shuffle the series, re-run, and ask whether the
//! real result was ever extreme.
//!
//! This is block A of `docs/plans/m3-full.md` and the first half of what
//! `crucible-funnel::stats` owes. It answers a question no statistic computed
//! *on a run* can answer: **is this edge a property of this data, or would the
//! same strategy have produced it on any series with the same shape?**
//!
//! # What the null actually is, stated because it is a choice
//!
//! `H₀: the return series is exchangeable at block scale L.` Concretely — any
//! dependence structure *longer* than `L` bars is absent, while the return
//! distribution and dependence *shorter* than `L` are preserved exactly, because
//! blocks are moved intact.
//!
//! That sentence is the whole test. A p-value here means "the observed metric
//! was in the top `p` of what this strategy produces on series that keep the
//! marginal distribution and the short-range dependence but lose the long-range
//! ordering". It does **not** mean "the strategy is profitable", and it does not
//! mean "the edge is real" — only that the ordering mattered.
//!
//! # Why the leak dies here when it survives every gate before it
//!
//! `controls::LeakyZScore` standardizes against the whole series and then
//! trades. Permute the series and it simply **re-fits on the permutation** —
//! its edge reproduces on every draw, so the observed result sits in the middle
//! of its own null and `p ≈ 0.5`. Nothing about the leaked run is extreme; the
//! leak is not a property of the data, it is a property of being allowed to look.
//!
//! An honest edge behaves the opposite way: block-shuffling destroys the
//! long-range structure it trades on, the null collapses, and the observed
//! result lands in the tail. **Both directions are pinned by tests here** — a
//! detector that kills everything is not a detector, and the converse control
//! is what makes the leak result mean anything (CLAUDE.md §7).
//!
//! # The block length is a knob with two opposing jobs (see CLAUDE.md §9)
//!
//! `L` preserves dependence *and* destroys the signal, and those pull against
//! each other. Too long and the strategy's own horizon fits inside a block, so
//! the null keeps the edge and the test is conservative to the point of being
//! useless. Too short and the null loses real autocorrelation the market has,
//! so an ordinary strategy looks extreme against a straw man. There is no
//! correct `L` — there is only a *declared* `L` and a **sweep** showing how the
//! answer moves, which is exactly what `ACCOUNT_EVAL_SPEC.md` §4.3 already
//! requires of its own bootstrap. A single unswept `L` is a result with a
//! parameter hidden inside it.
//!
//! # Determinism
//!
//! Fisher–Yates over a seeded [`ChaCha8Rng`], reductions in index order, sorts
//! that break ties on index. Same seed and same series ⇒ same p-value on any
//! machine (§2.2).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

/// What the permutation null was asked to do, declared before the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermutationSpec {
    /// Block length in bars. Blocks move intact, so dependence shorter than
    /// this survives and dependence longer than it does not.
    pub block_len: usize,
    /// How many permuted series to evaluate.
    pub draws: usize,
    /// Seed, from the caller's lineage (D-0064).
    pub seed: u64,
}

/// The outcome: the observed metric, the null it was compared against, and the
/// one number that summarises them.
#[derive(Debug, Clone, PartialEq)]
pub struct PermutationResult {
    /// The metric on the real, unpermuted series.
    pub observed: f64,
    /// Null draws, ascending. Kept so a report can show the distribution rather
    /// than asserting it.
    pub null: Vec<f64>,
    /// Draws that could not be evaluated (the strategy took no position, the
    /// metric was undefined). Counted, never silently dropped from the
    /// denominator.
    pub unevaluable: usize,
    /// The block length used.
    pub block_len: usize,
}

impl PermutationResult {
    /// One-sided right-tail p-value with the **+1 correction**:
    /// `(1 + #{null ≥ observed}) / (1 + draws)`.
    ///
    /// The correction is not cosmetic. Without it a strategy that beat every
    /// draw reports `p = 0`, which claims more certainty than `draws`
    /// resamples can support; with it the floor is `1/(1+draws)`, which is
    /// exactly the resolution the experiment has. Returns `None` when no draw
    /// could be evaluated — an absent null is not a cleared bar (D-0075).
    #[must_use]
    pub fn p_value(&self) -> Option<f64> {
        if self.null.is_empty() {
            return None;
        }
        let at_least = self.null.iter().filter(|v| **v >= self.observed).count();
        #[expect(
            clippy::cast_precision_loss,
            reason = "draw counts here are far below 2^53"
        )]
        let p = (1 + at_least) as f64 / (1 + self.null.len()) as f64;
        Some(p)
    }

    /// Whether the observed result is extreme enough to have survived, at a
    /// **pre-registered** `alpha`.
    ///
    /// `None` — no evaluable null — is a **failure**, not a pass, for the same
    /// reason an absent control fails its criterion (D-0075).
    #[must_use]
    pub fn survives(&self, alpha: f64) -> bool {
        self.p_value().is_some_and(|p| p <= alpha)
    }
}

/// Splits `returns` into consecutive blocks of `block_len` and reorders the
/// blocks, keeping each block's contents intact and in order.
///
/// The final block is short when `len` is not a multiple of `block_len`, and it
/// is shuffled with the rest rather than pinned to the end — pinning it would
/// leave one stretch of the series never permuted, which is a small hole in
/// exactly the place a strategy's last trades live.
///
/// # Panics
/// Panics if `block_len` is zero: blocks of length zero would make the null an
/// infinite loop rather than an i.i.d. shuffle, and silently promoting it to 1
/// would answer a different question than the caller asked.
#[must_use]
pub fn permute_blocks(returns: &[f64], block_len: usize, rng: &mut ChaCha8Rng) -> Vec<f64> {
    assert!(
        block_len > 0,
        "INVARIANT: a permutation block length is at least 1; zero is not \
         'i.i.d.', it is undefined"
    );
    let blocks: Vec<&[f64]> = returns.chunks(block_len).collect();
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    // Fisher-Yates, descending, from a seeded stream.
    for i in (1..order.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    let mut out = Vec::with_capacity(returns.len());
    for &b in &order {
        out.extend_from_slice(blocks[b]);
    }
    out
}

/// Rebuilds a price path from a first level and a series of **additive**
/// returns, in points.
///
/// Additive rather than multiplicative on purpose: the synthetic feed clamps at
/// a price floor (D-0011), and a multiplicative rebuild of a clamped walk can
/// wander to levels the tick grid cannot express. Point differences reconstruct
/// the original series exactly when the permutation is the identity, which is
/// the property the round-trip test pins.
#[must_use]
pub fn rebuild_path(first: f64, returns: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(returns.len() + 1);
    let mut level = first;
    out.push(level);
    for r in returns {
        level += r;
        out.push(level);
    }
    out
}

/// Runs the permutation null: permute, rebuild, evaluate, collect.
///
/// `evaluate` receives a rebuilt price path and returns the metric, or `None`
/// when that draw produced nothing to measure (no trades, undefined ratio).
/// `None` draws are **counted** in `unevaluable` and excluded from the null
/// rather than folded in as zeros — a zero is a result, and inventing one moves
/// the p-value.
///
/// # Panics
/// Panics if `closes` has fewer than two points (no returns to permute) or if
/// `spec.block_len` is zero.
pub fn run_permutation_null<F>(
    closes: &[f64],
    observed: f64,
    spec: PermutationSpec,
    mut evaluate: F,
) -> PermutationResult
where
    F: FnMut(&[f64]) -> Option<f64>,
{
    assert!(
        closes.len() >= 2,
        "INVARIANT: a permutation null needs at least one return to permute"
    );
    let returns: Vec<f64> = closes.windows(2).map(|w| w[1] - w[0]).collect();
    let first = closes[0];

    let mut rng = ChaCha8Rng::seed_from_u64(spec.seed);
    let mut null = Vec::with_capacity(spec.draws);
    let mut unevaluable = 0usize;
    for _ in 0..spec.draws {
        let permuted = permute_blocks(&returns, spec.block_len, &mut rng);
        let path = rebuild_path(first, &permuted);
        match evaluate(&path) {
            Some(v) if v.is_finite() => null.push(v),
            _ => unevaluable += 1,
        }
    }
    null.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    PermutationResult {
        observed,
        null,
        unevaluable,
        block_len: spec.block_len,
    }
}

#[cfg(test)]
mod tests;
