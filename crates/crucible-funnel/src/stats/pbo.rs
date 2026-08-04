//! Probability of backtest overfitting, by combinatorially symmetric
//! cross-validation (Bailey, Borwein, López de Prado & Zhu 2015).
//!
//! # The question
//!
//! A grid search reports its best combo. CSCV asks: **when we pick the winner
//! in-sample, how often is it below median out-of-sample?** If the answer is
//! about half, the selection carried no information and the "best" combo was
//! the luckiest one. That is the number this module produces, and it is a
//! property of the *search*, not of any one combo — a grid of one has no
//! selection to be wrong about, which is why `n_combos < 2` is an absence
//! rather than a zero.
//!
//! # Why it is symmetric
//!
//! The sample is cut into `S` equal blocks and **every** way of assigning
//! `S/2` of them to in-sample is evaluated, each with its complement as
//! out-of-sample. Symmetric because each split's mirror image is also
//! evaluated, so no split direction is privileged and the result cannot depend
//! on which half of the data happened to come first. That is what separates it
//! from a single train/test cut, whose answer depends on where the cut landed.
//!
//! # The blocks are FOLDS, and nothing else may define them
//!
//! `FoldPlan` is the sole boundary authority in this codebase (D-0062,
//! D-0071): it decides what a trading day is, which days are in sample, and
//! which are out. This module therefore takes an already-blocked performance
//! matrix and **never cuts a sample itself**. A second splitter — bar counts,
//! calendar months, an even division of the row count — would be a second
//! opinion about which observations are out of sample, and two independent
//! attributions of that is exactly the class of bug D-0071 exists to refuse.
//! The caller passes one column per fold; the block structure *is* the fold
//! table.
//!
//! # What is deliberately refused
//!
//! [`PboUnavailable`] has **five** variants and they are two different kinds
//! of thing. Both render as ABSENT with their reason and both **fail** the
//! `max_pbo` criterion rather than passing quietly (D-0075) — the split below
//! is about who has to fix them, not about how they are reported.
//!
//! **Policy refusals — the inputs are well-formed and the answer is still
//! withheld.** These are the judgement calls, and each names its remedy:
//!
//! - **An odd block count** ([`PboUnavailable::UnsplittableBlocks`], which also
//!   covers fewer than two). CSCV splits `S` blocks into two halves of `S/2`;
//!   an odd `S` has no such split. The tempting repair is to drop a block, and
//!   it is refused: silently discarding a fold changes the sample the number
//!   describes while the number still claims to describe the run. The config
//!   changes its fold plan, or PBO is reported absent with this reason.
//! - **More splits than [`MAX_SPLITS`]** ([`PboUnavailable::TooManySplits`]).
//!   `C(S, S/2)` grows fast — twenty blocks is 184,756 splits. Above the cap
//!   this refuses instead of subsampling, because a subsampled CSCV is a
//!   different estimator wearing the same name, and §9's no-silent-caps rule
//!   applies: a bound that changes the answer must be visible in the answer.
//! - **Fewer than two combos** ([`PboUnavailable::TooFewCombos`]). A grid of
//!   one has no selection to be wrong about. This is an absence and not a
//!   zero, because a zero here would read as "the search was reliable".
//!
//! **Input-contract errors — the matrix is malformed and no answer exists.**
//! A caller that hits one of these has a bug, not a fold plan to change:
//!
//! - **A ragged matrix** ([`PboUnavailable::RaggedMatrix`]): combos disagreeing
//!   about how many folds they were measured over cannot be ranked against
//!   each other.
//! - **A non-finite performance cell**
//!   ([`PboUnavailable::NonFinitePerformance`]). A NaN would propagate into the
//!   argmax and silently decide which combo "won".
//!
//! # Resolution
//!
//! PBO can only take the values `k / C(S, S/2)`. With the four folds a short
//! config produces, that is sixths — so "≈ 0.5" means 3/6, and 2/6 or 4/6 are
//! one split away from it. The estimate is coarse at small `S` and the split
//! count is reported beside it so a reader can see how coarse. Raising `S`
//! until the number looks better is the fixture-fitting this project's
//! decision log forbids; raising it because the fold plan genuinely changed is
//! ordinary.

/// Largest `C(S, S/2)` this module will evaluate before refusing.
///
/// `C(20, 10) = 184,756`, which is the last block count that stays comfortably
/// inside a second of work for a realistic grid. The bound exists to make an
/// accidental fold explosion loud rather than slow.
pub const MAX_SPLITS: u64 = 200_000;

/// Why a PBO could not be computed.
///
/// Each variant is a refusal rather than a fallback, and each is reported to
/// the reader verbatim: an absent PBO must never be mistaken for a passing
/// one, which is the same rule the funnel's controls follow (D-0075).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PboUnavailable {
    /// Fewer than two combos: there is no selection to measure.
    TooFewCombos {
        /// Combos supplied.
        combos: usize,
    },
    /// Fewer than two blocks, or an odd number of them.
    UnsplittableBlocks {
        /// Blocks supplied.
        blocks: usize,
    },
    /// Combos disagreed about how many blocks they were measured over.
    RaggedMatrix {
        /// Blocks the first combo carried.
        expected: usize,
        /// Blocks the offending combo carried.
        found: usize,
        /// Index of the offending combo.
        combo: usize,
    },
    /// A performance cell was NaN or infinite.
    NonFinitePerformance {
        /// Index of the offending combo.
        combo: usize,
        /// Index of the offending block.
        block: usize,
    },
    /// `C(S, S/2)` exceeded [`MAX_SPLITS`].
    TooManySplits {
        /// Blocks supplied.
        blocks: usize,
        /// Splits those blocks imply.
        splits: u64,
    },
}

impl std::fmt::Display for PboUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewCombos { combos } => write!(
                f,
                "PBO needs at least two combos to have a selection to measure; {combos} supplied"
            ),
            Self::UnsplittableBlocks { blocks } => write!(
                f,
                "CSCV needs an even number of at least two blocks so S/2 can be held out; \
                 the fold plan supplied {blocks}. Dropping one to make it even is refused — \
                 change the fold plan instead"
            ),
            Self::RaggedMatrix {
                expected,
                found,
                combo,
            } => write!(
                f,
                "every combo must be measured over the same folds; combo {combo} carries {found} \
                 blocks against the first combo's {expected}"
            ),
            Self::NonFinitePerformance { combo, block } => write!(
                f,
                "combo {combo} has a non-finite performance in block {block}; a NaN would decide \
                 the in-sample argmax silently"
            ),
            Self::TooManySplits { blocks, splits } => write!(
                f,
                "{blocks} blocks imply {splits} CSCV splits, above the {MAX_SPLITS} cap; \
                 subsampling would be a different estimator under the same name"
            ),
        }
    }
}

impl std::error::Error for PboUnavailable {}

/// A computed probability of backtest overfitting.
#[derive(Clone, Debug, PartialEq)]
pub struct Pbo {
    /// Fraction of splits whose in-sample winner landed at or below the median
    /// out-of-sample. Near 0.5 means the selection carried no information.
    pub value: f64,
    /// Splits evaluated — every `C(S, S/2)`, never a sample of them.
    pub splits: usize,
    /// Splits whose in-sample winner underperformed out of sample.
    pub underperforming_splits: usize,
    /// Blocks the sample was cut into. One per fold.
    pub blocks: usize,
    /// Combos ranked against each other.
    pub combos: usize,
}

impl Pbo {
    /// The coarsest difference this estimate can express, `1 / splits`.
    ///
    /// Reported so "≈ 0.5" can be read against the grid it was measured on
    /// rather than trusted as a precise number.
    #[must_use]
    pub fn resolution(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "split counts are bounded by MAX_SPLITS"
        )]
        let splits = self.splits as f64;
        if splits > 0.0 { 1.0 / splits } else { f64::NAN }
    }
}

/// Computes PBO by CSCV over a `combos × blocks` performance matrix.
///
/// `performance[c][b]` is combo `c`'s out-of-sample performance in fold `b` —
/// any metric where larger is better, on one scale for every combo. Both sides
/// of a split aggregate by the **mean over their blocks**, which is the
/// aggregation this function commits to; a caller wanting another one is
/// choosing a different statistic and should say so where the choice is
/// visible.
///
/// Deterministic in every particular (§2.2): the in-sample argmax breaks ties
/// toward the lowest combo index, and the out-of-sample rank breaks them the
/// same way, so every combo receives a distinct rank and no ordering depends
/// on iteration or completion order.
///
/// # Errors
/// [`PboUnavailable`] when the matrix cannot support the estimate. Every
/// variant is a refusal; none returns a substitute number.
pub fn probability_of_backtest_overfitting(
    performance: &[Vec<f64>],
) -> Result<Pbo, PboUnavailable> {
    let combos = performance.len();
    if combos < 2 {
        return Err(PboUnavailable::TooFewCombos { combos });
    }
    let blocks = performance[0].len();
    if blocks < 2 || !blocks.is_multiple_of(2) {
        return Err(PboUnavailable::UnsplittableBlocks { blocks });
    }
    for (combo, row) in performance.iter().enumerate() {
        if row.len() != blocks {
            return Err(PboUnavailable::RaggedMatrix {
                expected: blocks,
                found: row.len(),
                combo,
            });
        }
        for (block, value) in row.iter().enumerate() {
            if !value.is_finite() {
                return Err(PboUnavailable::NonFinitePerformance { combo, block });
            }
        }
    }

    let half = blocks / 2;
    let splits = binomial(blocks, half).ok_or(PboUnavailable::TooManySplits {
        blocks,
        splits: u64::MAX,
    })?;
    if splits > MAX_SPLITS {
        return Err(PboUnavailable::TooManySplits { blocks, splits });
    }

    let mut underperforming = 0usize;
    let mut evaluated = 0usize;
    // Every S/2-subset of the blocks, generated in lexicographic order so the
    // enumeration is a property of S alone and never of a container's layout.
    let mut selection: Vec<usize> = (0..half).collect();
    loop {
        evaluated += 1;
        let in_sample = &selection;
        let out_of_sample: Vec<usize> = (0..blocks).filter(|b| !in_sample.contains(b)).collect();

        let is_scores = aggregate(performance, in_sample);
        let oos_scores = aggregate(performance, &out_of_sample);

        // In-sample winner, ties to the lowest index.
        let mut winner = 0usize;
        for combo in 1..combos {
            if is_scores[combo] > is_scores[winner] {
                winner = combo;
            }
        }

        // Ascending rank of the winner out of sample: 1 is the worst combo,
        // `combos` the best. Ties resolve by index, so ranks are a permutation
        // of 1..=combos and the median test below is exact.
        let mut rank = 1usize;
        for combo in 0..combos {
            if combo == winner {
                continue;
            }
            let better = oos_scores[combo] < oos_scores[winner]
                || (oos_scores[combo] == oos_scores[winner] && combo < winner);
            if better {
                rank += 1;
            }
        }

        // ω = rank / (combos + 1); the split is an overfit when ω <= 0.5,
        // equivalently when the logit ln(ω / (1 - ω)) is not positive. Compared
        // as integers so no split's classification can turn on a rounding step.
        if 2 * rank <= combos + 1 {
            underperforming += 1;
        }

        if !next_combination(&mut selection, blocks) {
            break;
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "split counts are bounded by MAX_SPLITS"
    )]
    let value = underperforming as f64 / evaluated as f64;
    Ok(Pbo {
        value,
        splits: evaluated,
        underperforming_splits: underperforming,
        blocks,
        combos,
    })
}

/// Mean performance of every combo over the chosen blocks.
fn aggregate(performance: &[Vec<f64>], blocks: &[usize]) -> Vec<f64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "block counts are bounded by MAX_SPLITS' preconditions"
    )]
    let n = blocks.len() as f64;
    performance
        .iter()
        .map(|row| blocks.iter().map(|&b| row[b]).sum::<f64>() / n)
        .collect()
}

/// Advances `selection` to the next lexicographic `k`-subset of `0..n`.
///
/// Returns `false` when the last subset has been visited.
fn next_combination(selection: &mut [usize], n: usize) -> bool {
    let k = selection.len();
    if k == 0 {
        return false;
    }
    let mut i = k;
    while i > 0 {
        i -= 1;
        if selection[i] != i + n - k {
            selection[i] += 1;
            for j in i + 1..k {
                selection[j] = selection[j - 1] + 1;
            }
            return true;
        }
    }
    false
}

/// `C(n, k)`, or `None` on overflow.
fn binomial(n: usize, k: usize) -> Option<u64> {
    let k = k.min(n - k);
    let mut result: u64 = 1;
    for i in 0..k {
        result = result.checked_mul(u64::try_from(n - i).ok()?)?;
        result /= u64::try_from(i + 1).ok()?;
    }
    Some(result)
}

#[cfg(test)]
mod pinned;
#[cfg(test)]
mod tests;
