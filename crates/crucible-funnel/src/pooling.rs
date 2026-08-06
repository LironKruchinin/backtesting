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

/// Why a contract contributed nothing to a pooled evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// Its front window could not fit one whole fold at the declared geometry.
    NoCompleteFold {
        /// Trading sessions the contract's front window held.
        sessions: usize,
        /// Sessions one fold needs: `train_days + test_days`.
        needed: usize,
    },
    /// Its **own** curated history could not supply the grid's warmup, so the
    /// front window could only be evaluated by spending part of itself on
    /// warmup — which is the coupling D-0121 exists to remove (evaluable
    /// sessions would fall as the grid's warmup rose). Skipped rather than
    /// evaluated on a shorter window, and never warmed on the previous
    /// contract's bars, which would be stitching (D-0076).
    InsufficientWarmup {
        /// Bars this contract had before its front window opened.
        available: usize,
        /// Bars the grid's widest indicator needs (§2.6).
        needed: usize,
    },
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCompleteFold { sessions, needed } => write!(
                f,
                "{sessions} front-month session(s), but one fold needs {needed}"
            ),
            Self::InsufficientWarmup { available, needed } => write!(
                f,
                "{available} bar(s) of its own history before the front window, \
                 but the grid's warmup needs {needed}"
            ),
        }
    }
}

/// One contract's measured contribution to a pooled evaluation.
///
/// Every field is **measured from that contract's own replay**, never derived
/// from a per-contract average. Front windows are not equal — ES 2024 measured
/// 64, 70, 66 and 66 sessions over spans of 84 to 98 calendar days — so
/// `N x constant` is wrong by construction, and it is exactly the extrapolation
/// that produced the false claim "four contracts clear 250".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractEvaluation {
    /// Curated contract symbol.
    pub instrument: String,
    /// Trading-day keys of the contract's whole front window, ascending.
    pub front_window_days: Vec<i64>,
    /// Trading-day keys that fell in an out-of-sample window, ascending.
    pub oos_day_keys: Vec<i64>,
    /// Round-trips closed in those out-of-sample windows.
    pub oos_trades: usize,
    /// Warmup bars this contract's OWN curated history supplied before its
    /// front window opened, and what the grid needed — `Some` only when it
    /// fell short (D-0121). Judged before `folds`, because a contract that
    /// cannot warm up never got a fold plan to begin with.
    pub insufficient_warmup: Option<(usize, usize)>,
    /// Complete folds the front window fitted. Zero means the contract is
    /// skipped and contributes nothing.
    pub folds: usize,
    /// Sessions one fold needed, carried so a skip can say what it missed by.
    pub fold_needs_sessions: usize,
}

/// A stretch of the pool's span that no evaluated contract covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGap {
    /// First uncovered trading-day key.
    pub from_day: i64,
    /// Last uncovered trading-day key.
    pub to_day: i64,
    /// Contracts whose front windows bound the hole.
    pub between: (String, String),
}

impl SessionGap {
    /// Trading days the pool's span contains here and its contracts do not.
    #[must_use]
    pub fn days(&self) -> i64 {
        self.to_day - self.from_day + 1
    }
}

/// What a pooled run may present to the admission gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PooledEvaluation {
    /// **Out-of-sample sessions: the UNION.** What `min_oos_sessions` judges.
    pub oos_sessions: usize,
    /// **Out-of-sample round-trips: the SUM.** What `min_oos_trades` judges —
    /// and the floor is two-part, so a pool clearing the session half has
    /// cleared half a floor.
    ///
    /// Summed rather than unioned, and the asymmetry is real rather than an
    /// oversight: two contracts can trade the *same session*, so sessions
    /// double-count and must be unioned; a round-trip belongs to exactly one
    /// contract's replay, so trades cannot double-count and summing is exact.
    pub oos_trades: usize,
    /// Contracts that contributed at least one complete fold.
    pub contracts_evaluated: usize,
    /// Contracts that contributed nothing, and why. **Reported even when
    /// empty**, on D-0070's spread-filter pattern: a declared filter with a
    /// number beside it, never a silent absence. A pool quietly dropping two
    /// short contracts reports a smaller sample than it declared and nothing
    /// on the page would say so.
    pub skipped: Vec<(String, SkipReason)>,
    /// Holes between evaluated contracts' front windows.
    pub gaps: Vec<SessionGap>,
    /// Per-contract `(instrument, oos sessions, oos trades)`, measured.
    pub per_contract: Vec<(String, usize, usize)>,
}

impl PooledEvaluation {
    /// Whether the evaluated contracts form one unbroken run of front months.
    #[must_use]
    pub fn is_contiguous(&self) -> bool {
        self.gaps.is_empty()
    }

    /// The honesty line a pooled report prints.
    #[must_use]
    pub fn render(&self) -> String {
        let mut line = format!(
            "{} OOS session(s), {} OOS round-trip(s), {} contract(s) evaluated, {} skipped",
            self.oos_sessions,
            self.oos_trades,
            self.contracts_evaluated,
            self.skipped.len()
        );
        if self.gaps.is_empty() {
            line.push_str("; contiguous");
        } else {
            let days: i64 = self.gaps.iter().map(SessionGap::days).sum();
            line.push_str(&format!(
                "; {} gap(s) totalling {days} trading day(s)",
                self.gaps.len()
            ));
        }
        line
    }
}

/// Pools measured per-contract evaluations into the numbers admission judges.
///
/// # Path-dependent statistics must NOT be computed across this output
///
/// A pooled sample is many short, non-contiguous out-of-sample windows — not
/// one long track record. Max drawdown, longest losing streak and time under
/// water computed across the concatenation describe a path **no account ever
/// walked**: the seams join windows that are months apart, so a drawdown
/// measured across one is an artifact of the concatenation order. Each
/// contract's replay starts and ends flat, so no spurious *return* appears at
/// a seam — the trap is only in reading concatenated equity as one path.
/// Path-independent aggregates — returns, trade counts, win rate, per-window
/// Sharpe — pool correctly, and are what this returns.
///
/// # Errors
/// [`PoolingError`] on [`pool_sessions`]'s terms: nonempty, distinct
/// contracts, each with strictly ascending day keys.
pub fn pool_evaluations(
    contracts: &[ContractEvaluation],
) -> Result<PooledEvaluation, PoolingError> {
    if contracts.is_empty() {
        return Err(PoolingError::NoContracts);
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for contract in contracts {
        if contract.front_window_days.is_empty() {
            return Err(PoolingError::EmptyContract {
                instrument: contract.instrument.clone(),
            });
        }
        if !seen.insert(contract.instrument.as_str()) {
            return Err(PoolingError::DuplicateContract {
                instrument: contract.instrument.clone(),
            });
        }
        for keys in [&contract.front_window_days, &contract.oos_day_keys] {
            let mut previous: Option<i64> = None;
            for &key in keys {
                if let Some(previous) = previous
                    && key <= previous
                {
                    return Err(PoolingError::UnorderedDays {
                        instrument: contract.instrument.clone(),
                        key,
                    });
                }
                previous = Some(key);
            }
        }
    }

    let mut oos_union: BTreeSet<i64> = BTreeSet::new();
    let mut oos_trades = 0usize;
    let mut skipped = Vec::new();
    let mut per_contract = Vec::new();
    let mut evaluated: Vec<&ContractEvaluation> = Vec::new();

    for contract in contracts {
        // Judged before the fold check: a contract whose own history cannot
        // supply the grid's warmup never got a fold plan, so `folds == 0`
        // would report the wrong reason for the right skip (D-0121).
        if let Some((available, needed)) = contract.insufficient_warmup {
            skipped.push((
                contract.instrument.clone(),
                SkipReason::InsufficientWarmup { available, needed },
            ));
            continue;
        }
        if contract.folds == 0 {
            skipped.push((
                contract.instrument.clone(),
                SkipReason::NoCompleteFold {
                    sessions: contract.front_window_days.len(),
                    needed: contract.fold_needs_sessions,
                },
            ));
            continue;
        }
        // Measured per contract and summed. Never N x a per-contract average:
        // front windows are not equal, and that extrapolation is what produced
        // the false claim "four contracts clear 250".
        oos_union.extend(contract.oos_day_keys.iter().copied());
        oos_trades = oos_trades.saturating_add(contract.oos_trades);
        per_contract.push((
            contract.instrument.clone(),
            contract.oos_day_keys.len(),
            contract.oos_trades,
        ));
        evaluated.push(contract);
    }

    // Holes are measured between consecutive evaluated contracts' FRONT
    // windows, not between out-of-sample days: within one contract the
    // out-of-sample days are already broken up by its training windows, and
    // reporting those would bury the hole that matters — a contract missing
    // from the middle of the pool.
    //
    // Scanned in TIME order, which is not the order the contracts arrived in.
    // A pool is a *set*: the sequence they were typed into the config is not a
    // fact about the market, and until this sort existed the scan inherited it.
    // Declared newest-first, `first > last + 1` never held and a genuine hole
    // rendered as "contiguous" — the honesty device reporting the opposite of
    // the truth. Declared out of sequence, it invented a hole that another
    // contract covered.
    //
    // The key is a total order — (first day, last day, symbol) — rather than
    // the first day alone, so the answer cannot depend on the input order even
    // when two contracts share a first day. Ties broken by a stable sort on
    // declaration order would still be deterministic, but only because the
    // input was; §2.2 wants the result to be independent of it.
    //
    // `per_contract` deliberately keeps declaration order: it is a report of
    // what was asked for, not a set, exactly as `pool_sessions` treats its own.
    let mut chronological = evaluated.clone();
    chronological.sort_by(|a, b| {
        let key = |c: &&ContractEvaluation| {
            (
                c.front_window_days[0],
                *c.front_window_days.last().expect("nonempty checked"),
                c.instrument.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    let mut gaps = Vec::new();
    for pair in chronological.windows(2) {
        let (before, after) = (pair[0], pair[1]);
        let last = *before.front_window_days.last().expect("nonempty checked");
        let first = *after.front_window_days.first().expect("nonempty checked");
        if first > last + 1 {
            gaps.push(SessionGap {
                from_day: last + 1,
                to_day: first - 1,
                between: (before.instrument.clone(), after.instrument.clone()),
            });
        }
    }

    Ok(PooledEvaluation {
        oos_sessions: oos_union.len(),
        oos_trades,
        contracts_evaluated: evaluated.len(),
        skipped,
        gaps,
        per_contract,
    })
}

/// The **effective registration hash** for one pooled contract's run (D-0124).
///
/// # Why a composite rather than a wider key
///
/// A pooled run replays the same strategy over several contracts, and each
/// contract is its own run and its own **trial** — D-0114 requires N contracts
/// to charge N trials, because each is a separate opportunity for the grid to
/// have got lucky. The identity therefore has to distinguish them.
///
/// It does so by *deriving a different `config_hash`*, exactly as D-0106
/// derives one when `[s0]` is declared, rather than by adding a contract field
/// to [`RunKey`](crate::registry::RunKey). That choice is not stylistic:
/// `RunKey` is **persisted** and `TrialKey` is **derived at read time**, so a
/// new key field is a persisted-shape change and drags in §8's reader-first
/// sequencing under a contract D-0083 and D-0105 both constrain. A composite
/// changes no stored shape, and every existing reader keeps working.
///
/// The same contract appearing twice hashes the same and therefore collides
/// into **one** trial, which is correct — it is one run of one contract, and
/// D-0115 refuses the duplicate declaration that would produce it anyway.
///
/// # What is bound, and why the window is in it
///
/// Strategy, contract, and **front window**. The window belongs in the
/// identity because it is what the run was evaluated on: rebuild the `.v` roll
/// table against a longer archive and a contract's front window can move
/// (D-0045 already refuses a replay outside the table's build span), which
/// makes it a different run of the same strategy on the same symbol. Two runs
/// that are not comparable must not share a trial.
///
/// Boundaries are the window's own nanosecond instants, never civil dates —
/// C4b-i exists because a date round-trip moved them to UTC midnight, and an
/// identity built from the rounded form would call two different windows the
/// same run.
#[must_use]
pub fn pooled_registration_hash(
    strategy_hash: &str,
    instrument: &str,
    front_window_start_ns: i64,
    front_window_end_ns: i64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    // Domain separation, and versioned: a change to what is bound below is a
    // change to run identity, and it must not silently alias the old scheme.
    hasher.update(b"crucible-pooled-registration/1\0");
    hash_text(&mut hasher, strategy_hash);
    hash_text(&mut hasher, instrument);
    hasher.update(&front_window_start_ns.to_le_bytes());
    hasher.update(&front_window_end_ns.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Length-prefixed so that concatenation is unambiguous: without it,
/// `("ab", "c")` and `("a", "bc")` would hash alike and two different runs
/// could share a trial.
fn hash_text(hasher: &mut blake3::Hasher, text: &str) {
    let bytes = text.as_bytes();
    let len = u64::try_from(bytes.len()).expect("text length fits the 64-bit identity encoding");
    hasher.update(&len.to_le_bytes());
    hasher.update(bytes);
}

/// Everything a pooled run must say about its own sample, in one block.
///
/// # Path-dependent statistics are absent BY CONSTRUCTION, not by convention
///
/// Max drawdown, longest losing streak and time-under-water across the pooled
/// concatenation describe a path no account ever walked — the seams join
/// windows months apart (D-0119). This report cannot compute them, and not
/// because it declines to: [`ContractEvaluation`] and [`PooledEvaluation`]
/// carry no equity curve, no [`Summary`](crucible_engine::Summary) and no
/// per-bar series, so there is nothing to concatenate. The inputs are day
/// keys and counts, all path-independent.
///
/// `the_pooled_types_carry_nothing_path_dependent` is the tripwire: it fails
/// if a field whose name suggests a path is ever added, so the guarantee stops
/// being a property someone has to re-check by reading.
pub struct PooledReport<'a> {
    /// Declared root, e.g. `ES`.
    pub root: &'a str,
    /// Union / sum / overlap (D-0114).
    pub sessions: &'a PooledSessions,
    /// Admission's numbers, skips and gaps.
    pub evaluation: &'a PooledEvaluation,
    /// Combos in the grid — the other factor of the trial count.
    pub combos: usize,
    /// Which roll rule produced the front windows. Named because every report
    /// in this codebase says which of two things produced its numbers.
    pub roll_rule: &'a str,
}

impl PooledReport<'_> {
    /// The block a pooled run prints, and the same text the scorecard embeds.
    #[must_use]
    pub fn render(&self) -> String {
        let e = self.evaluation;
        let s = self.sessions;
        let mut out = String::new();

        out.push_str(&format!(
            "  pooling       root {}, {} contract(s) declared, {} evaluated, {} skipped\n",
            self.root,
            s.contracts(),
            e.contracts_evaluated,
            e.skipped.len()
        ));
        out.push_str(&format!(
            "  attribution   front-month windows from the {} roll table\n",
            self.roll_rule
        ));

        // The union IS the sample; the sum is printed beside it so a reader can
        // tell whether the pooling was honest, and can never be consumed as a
        // sample size (D-0114).
        out.push_str(&format!(
            "  sessions      {} distinct out-of-sample — {} summed, {} overlapping\n",
            e.oos_sessions, s.summed_days, s.overlap_days
        ));
        if s.has_overlap() {
            out.push_str(
                "                the SUM is not a sample size: contracts of one root trade\n\
                 \x20               the same days, and adding them claims a sample twice the\n\
                 \x20               size of the one that exists (D-0114)\n",
            );
        }

        // The other half of the two-part floor. A pool clearing the session
        // half has cleared half a floor (D-0119).
        out.push_str(&format!(
            "  trades        {} out-of-sample round-trip(s), summed across contracts\n\
             \x20               (a round-trip belongs to exactly one contract, so summing is\n\
             \x20               exact — unlike sessions, which must be unioned)\n",
            e.oos_trades
        ));

        // Printed EVEN AT ZERO (D-0070): "nothing was dropped" and "the report
        // forgot to say" must not look the same.
        if e.skipped.is_empty() {
            out.push_str("  skipped       none — every declared contract contributed\n");
        } else {
            out.push_str(&format!(
                "  skipped       {} contract(s):\n",
                e.skipped.len()
            ));
            for (instrument, reason) in &e.skipped {
                out.push_str(&format!("                  {instrument} — {reason}\n"));
            }
        }

        // Gaps are reported, never refused: a deliberately non-consecutive pool
        // is a legitimate experiment and an invisible hole is not (D-0119).
        if e.gaps.is_empty() {
            out.push_str("  gaps          none — the evaluated contracts are contiguous\n");
        } else {
            let total: i64 = e.gaps.iter().map(SessionGap::days).sum();
            out.push_str(&format!(
                "  gaps          {} hole(s), {total} trading day(s) total:\n",
                e.gaps.len()
            ));
            for gap in &e.gaps {
                out.push_str(&format!(
                    "                  day {}..{} ({}) between {} and {}\n",
                    gap.from_day,
                    gap.to_day,
                    gap.days(),
                    gap.between.0,
                    gap.between.1
                ));
            }
        }

        // Composition rather than a bare integer, with the direction of the
        // error stated (D-0124, D-0125).
        out.push_str(&format!(
            "  trials        {} = {} contract(s) x {} combo(s)\n\
             \x20               pooled contracts are a LARGER SAMPLE, not independent\n\
             \x20               searches, so this count OVER-deflates the Sharpe. It is used\n\
             \x20               anyway: the preferred error is against the strategy. Read the\n\
             \x20               correction as conservative, not precise (D-0124)\n",
            e.contracts_evaluated * self.combos,
            e.contracts_evaluated,
            self.combos
        ));

        out.push_str(
            "  NOT COMPUTED  max drawdown, longest losing streak and time under water are\n\
             \x20               not reported across this pool and cannot be: it is many short\n\
             \x20               non-contiguous windows, not one track record, and such a\n\
             \x20               number would describe a path no account ever walked (D-0119)\n",
        );
        out
    }
}
