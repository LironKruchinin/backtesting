//! Pooling a grid's evidence across many contracts of one root.
//!
//! Block C of `docs/plans/m3-full.md`. Today a grade-A config replays one
//! contract's active life — roughly 60 sessions for ES — and no
//! sample-adequacy criterion worth registering is satisfiable at 60 sessions,
//! so every grade-A run is killed at admission, correctly, by the machine.
//! Pooling is how those floors get met.
//!
//! # The one arithmetic rule, and it is the whole module
//!
//! **Pooled sessions are the count of DISTINCT trading days. Never the sum.**
//!
//! Two contracts of the same root trade the same calendar days for as long as
//! both are listed — ESH2024 and ESM2024 overlap for months. Adding their
//! session counts claims a sample twice the size of the one that exists, and
//! every statistic downstream reads that number: the admission floor, the
//! deflated Sharpe's observation count, the fold plan's session budget. It is
//! the D-0062 argument against overlapping out-of-sample windows, applied
//! across contracts instead of within one: a session counted twice inflates
//! `n` and flatters everything computed from it.
//!
//! So [`PooledSessions`] reports the union, and reports the sum beside it with
//! the overlap named. Not because the sum is useful — it is not — but because
//! a reader who sees only "412 sessions" cannot tell whether the pooling was
//! honest, and one who sees "412 distinct, 631 summed, 219 overlapping" can.
//!
//! **Cross-instrument breadth is a different claim.** Pooling ES with NQ over
//! the same 250 sessions is not 500 sessions of evidence; it is one 250-session
//! sample and a statement that the effect appears in two instruments. That
//! second statement is the rhyme check and it is worth making — but it is not
//! extra `n`, and this module will not let it be counted as any.
//!
//! # What pooling does NOT do
//!
//! - **It does not lower a floor.** H-007 and H-008 both register 200 trades
//!   and 250 sessions, and both say the floors come down "only when registry
//!   pooling supplies the sessions honestly, never to make a short run pass".
//!   Pooling is the mechanism that meets them.
//! - **It does not enable continuous aliases for grids.** `combo` and
//!   `walk-forward` refuse `ES.v.0` because a grid expands rules nobody has
//!   read and a level comparison is unsafe on a back-adjusted series (D-0076).
//!   Pooling replays *real contracts* and pools their evidence; it is the
//!   sanctioned route to a long sample precisely so that stitching does not
//!   have to be. A future design that wants back-adjusted grids supersedes
//!   D-0076 explicitly, and that is not this module's to do.
//! - **It does not charge one trial.** Every contract pooled is a trial
//!   (§4's `(config_hash, account_id, combo_index)` is per-contract here
//!   because each contract is a separate run), so pooling N contracts charges
//!   N times the trials and the block-B deflated Sharpe falls accordingly.
//!   That is correct and is the price of the larger sample.

use std::collections::BTreeSet;

/// One contract's contribution to a pooled sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractDays {
    /// Curated contract symbol, four-digit year (D-0072).
    pub instrument: String,
    /// Distinct trading-day keys this contract contributes, ascending.
    ///
    /// Days rather than bars, and **keys** rather than dates: the caller
    /// computes `days_from_civil(Calendar::trading_day(avail_ts))` once and
    /// every consumer reads the same slice (D-0071). Two independent
    /// attributions of "which day" is how one breach lands on two dates in two
    /// reports.
    pub day_keys: Vec<i64>,
}

/// Why a pooled sample could not be formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoolingError {
    /// Nothing to pool.
    NoContracts,
    /// A contract contributed no evaluable trading day.
    EmptyContract {
        /// The offending symbol.
        instrument: String,
    },
    /// The same symbol appeared twice. Pooling a contract with itself is the
    /// double-count this module exists to prevent, in its most direct form.
    DuplicateContract {
        /// The repeated symbol.
        instrument: String,
    },
    /// A contract's day keys were not strictly ascending, so "distinct days"
    /// could not be read off them without a second opinion about ordering.
    UnorderedDays {
        /// The offending symbol.
        instrument: String,
        /// The key that did not advance.
        key: i64,
    },
}

impl std::fmt::Display for PoolingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoContracts => write!(f, "a pooled run needs at least one contract"),
            Self::EmptyContract { instrument } => write!(
                f,
                "{instrument} contributes no evaluable trading day; pooling it would charge a \
                 trial for no evidence"
            ),
            Self::DuplicateContract { instrument } => write!(
                f,
                "{instrument} appears twice in the pool — pooling a contract with itself doubles \
                 its sessions and charges two trials for one run"
            ),
            Self::UnorderedDays { instrument, key } => write!(
                f,
                "{instrument} has non-ascending trading-day keys at {key}; distinct-day counting \
                 requires the caller's ordered key slice (D-0071)"
            ),
        }
    }
}

impl std::error::Error for PoolingError {}

/// The honest denominator for a pooled run, with the overlap visible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PooledSessions {
    /// **The number that counts.** Distinct trading days across every pooled
    /// contract — the union, never the sum.
    pub distinct_days: usize,
    /// What naive addition would have claimed. Reported *only* so the gap
    /// below is visible; nothing may consume it as a sample size.
    pub summed_days: usize,
    /// `summed_days - distinct_days`: sessions two or more contracts share.
    /// A large number here is normal for one root and is exactly the quantity
    /// a naive pooling would have invented.
    pub overlap_days: usize,
    /// Per-contract distinct-day counts, in declaration order.
    pub per_contract: Vec<(String, usize)>,
}

impl PooledSessions {
    /// Contracts pooled — and therefore trials charged, one per contract.
    ///
    /// The count is derived from the pool rather than passed in, so a caller
    /// cannot declare a trial count that disagrees with the evidence. The
    /// authoritative count still comes from `Registry::trials_for`; this is
    /// what that count must *equal* after the claims land, and
    /// `pooling_n_contracts_charges_n_trials` is that assertion.
    #[must_use]
    pub fn contracts(&self) -> usize {
        self.per_contract.len()
    }

    /// Whether pooling changed anything — false only for a single contract or
    /// for contracts that share no session at all.
    #[must_use]
    pub fn has_overlap(&self) -> bool {
        self.overlap_days > 0
    }
}

/// Pools contracts into one honest session count.
///
/// # Errors
/// [`PoolingError`] when the pool cannot be formed. Every variant is a
/// refusal: none returns a partial count, because a session total that quietly
/// dropped a contract is the same lie as one that double-counted it.
pub fn pool_sessions(contracts: &[ContractDays]) -> Result<PooledSessions, PoolingError> {
    if contracts.is_empty() {
        return Err(PoolingError::NoContracts);
    }
    let mut seen_symbols: BTreeSet<&str> = BTreeSet::new();
    let mut union: BTreeSet<i64> = BTreeSet::new();
    let mut summed = 0usize;
    let mut per_contract = Vec::with_capacity(contracts.len());

    for contract in contracts {
        if contract.day_keys.is_empty() {
            return Err(PoolingError::EmptyContract {
                instrument: contract.instrument.clone(),
            });
        }
        if !seen_symbols.insert(contract.instrument.as_str()) {
            return Err(PoolingError::DuplicateContract {
                instrument: contract.instrument.clone(),
            });
        }
        let mut previous: Option<i64> = None;
        for &key in &contract.day_keys {
            if let Some(previous) = previous
                && key <= previous
            {
                return Err(PoolingError::UnorderedDays {
                    instrument: contract.instrument.clone(),
                    key,
                });
            }
            previous = Some(key);
            union.insert(key);
        }
        summed += contract.day_keys.len();
        per_contract.push((contract.instrument.clone(), contract.day_keys.len()));
    }

    let distinct_days = union.len();
    Ok(PooledSessions {
        distinct_days,
        summed_days: summed,
        // Saturating only for defence: the union can never exceed the sum,
        // because every distinct day was contributed by at least one contract.
        overlap_days: summed.saturating_sub(distinct_days),
        per_contract,
    })
}

#[cfg(test)]
mod tests;
