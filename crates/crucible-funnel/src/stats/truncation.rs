//! Truncation invariance — decisions made on a prefix must equal the decisions
//! the full series made over that same prefix.
//!
//! The second half of block A (`docs/plans/m3-full.md`, D-0087). It is the
//! companion to [`crate::stats::permutation`] and it asks a **different kind of
//! question**, which is the reason both exist:
//!
//! | | permutation | truncation |
//! |---|---|---|
//! | asks | is the observed result *extreme* against a null? | are two decision streams *equal*? |
//! | answer | a p-value, with a threshold | yes or no, with the first disagreement |
//! | catches | an edge that is not a property of this data | information flowing backwards in time |
//!
//! Permutation is statistical and can be fooled by a strategy that reproduces
//! its edge on every draw. Truncation is not statistical at all: it is a
//! **determinism property**, and it either holds or names the bar where it
//! stopped holding.
//!
//! # Which end is truncated, and why that is the whole design
//!
//! The **end**. Run the strategy on `data[0..t]`, then on the whole series, and
//! require every decision at or before the cut to match. Lookahead flows from
//! the future into the past — a full-sample statistic, a peek at tomorrow's
//! close, an indicator fitted on everything — so deleting the future is exactly
//! the perturbation that exposes it. A strategy that used no future information
//! cannot notice the deletion.
//!
//! Truncating the **start** would be a different and lesser test: it measures
//! warmup sensitivity, which is a real property but not this one, and a
//! strategy failing it is usually just under-warmed rather than leaking.
//!
//! # Bit-identity, and why there is no tolerance knob
//!
//! Decisions are compared as **bytes**, through [`Decision::encode`]. Not
//! structural equality, and emphatically not approximate agreement: a strategy
//! whose orders differ by one tick after the future is deleted has read the
//! future, and a tolerance that forgives one tick forgives exactly the defect
//! this harness exists to find. Any tolerance is a §9 decision with an entry
//! behind it, never a default and never a parameter.
//!
//! Floats never reach the comparison. An `OrderIntent` is a side, an integer
//! quantity, a kind and an optional integer bracket — all exact — so byte
//! equality is well-defined and platform-stable (§2.3).

use crucible_core::prelude::*;

/// What a strategy did on one bar, and when it could have known to.
///
/// The `avail_ts` is the key the comparison joins on (§2.1), never `ts_open`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Availability time of the bar that produced these intents.
    pub avail_ts: i64,
    /// Canonical bytes of every intent emitted on that bar, in order.
    pub bytes: Vec<u8>,
}

impl Decision {
    /// Encodes intents into a canonical, exact byte form.
    ///
    /// Little-endian integers, one tag byte per variant, in emission order. No
    /// floats, so this is exact and identical on every platform.
    #[must_use]
    pub fn encode(intents: &[OrderIntent]) -> Vec<u8> {
        let mut out = Vec::with_capacity(intents.len() * 24);
        for i in intents {
            out.push(match i.side {
                Side::Buy => 0u8,
                Side::Sell => 1u8,
            });
            out.extend_from_slice(&i.qty.0.to_le_bytes());
            out.push(match i.kind {
                OrderKind::Market => 0u8,
            });
            match &i.bracket {
                None => out.push(0u8),
                Some(b) => {
                    out.push(1u8);
                    for leg in [b.stop_ticks(), b.target_ticks()] {
                        match leg {
                            None => out.push(0u8),
                            Some(t) => {
                                out.push(1u8);
                                out.extend_from_slice(&t.to_le_bytes());
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

/// A [`Strategy`] decorator that records what its inner strategy decided.
///
/// Wrapping rather than re-implementing is what makes the harness faithful: the
/// decisions recorded are the ones the real engine acted on, produced against
/// the real [`PortfolioView`], not a reconstruction. A reconstruction would be
/// measuring the reconstruction.
pub struct Recording<S> {
    inner: S,
    log: Vec<Decision>,
}

impl<S: Strategy> Recording<S> {
    /// Wraps a strategy.
    pub const fn new(inner: S) -> Recording<S> {
        Recording {
            inner,
            log: Vec::new(),
        }
    }

    /// The decisions recorded so far, in bar order.
    #[must_use]
    pub fn decisions(&self) -> &[Decision] {
        &self.log
    }

    /// Consumes the decorator and yields its log.
    #[must_use]
    pub fn into_decisions(self) -> Vec<Decision> {
        self.log
    }
}

impl<S: Strategy> Strategy for Recording<S> {
    fn warmup_bars(&self) -> usize {
        self.inner.warmup_bars()
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        self.inner.on_event(ev, view, actions);
        // Take, record, put back in the same order. `Actions` exposes no
        // non-consuming reader, and adding one for a test harness would be the
        // tail wagging the dog; the round-trip is exact because `push_intent`
        // appends and `take_intents` drains in order.
        let intents = actions.take_intents();
        if !intents.is_empty() {
            let MarketEvent::Bar(bar) = ev;
            self.log.push(Decision {
                avail_ts: bar.avail_ts().0,
                bytes: Decision::encode(&intents),
            });
        }
        for i in intents {
            actions.push_intent(i);
        }
    }
}

/// Where two decision streams first stopped agreeing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The cut point whose run disagreed, as an index into the event series.
    pub cut: usize,
    /// Availability time of the first differing decision, or of the decision
    /// one side made and the other did not.
    pub avail_ts: i64,
    /// What happened, in words a report can print.
    pub detail: String,
}

/// The outcome of a truncation sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationReport {
    /// Cut points tested, in the order tested.
    pub cuts: Vec<usize>,
    /// Every disagreement found. **Empty means invariant.**
    pub divergences: Vec<Divergence>,
    /// Decisions compared across all cuts — the denominator that says whether
    /// "no divergence" meant anything.
    pub compared: usize,
}

impl TruncationReport {
    /// Whether the property held.
    ///
    /// A sweep that compared **nothing** does not hold: it is an absent
    /// measurement, and an absent measurement is not a cleared bar (D-0075).
    #[must_use]
    pub fn holds(&self) -> bool {
        self.divergences.is_empty() && self.compared > 0
    }
}

/// Runs the truncation-invariance sweep.
///
/// `replay` is handed a prefix of the events and must return the decisions that
/// prefix produced. It is a closure so this module needs no opinion about how a
/// strategy is built — the caller owns that, and the caller is the only one who
/// can rebuild a strategy that must be re-fitted per series.
///
/// # Panics
/// Panics if any cut is zero or exceeds the series length: a cut outside the
/// data is a caller bug, and silently clamping it would compare a prefix
/// against itself and report success.
pub fn truncation_invariance<F>(
    events: &[MarketEvent],
    cuts: &[usize],
    mut replay: F,
) -> TruncationReport
where
    F: FnMut(&[MarketEvent]) -> Vec<Decision>,
{
    assert!(
        cuts.iter().all(|&c| c > 0 && c <= events.len()),
        "INVARIANT: every cut point lies inside the series; a clamped cut would \
         compare a prefix against itself and report success"
    );

    let full = replay(events);
    let mut divergences = Vec::new();
    let mut compared = 0usize;

    for &cut in cuts {
        let MarketEvent::Bar(last) = &events[cut - 1];
        let boundary = last.avail_ts().0;
        let truncated = replay(&events[..cut]);
        // The full run's decisions at or before the boundary — the ones the
        // truncated run had the same information for.
        let prefix: Vec<&Decision> = full.iter().filter(|d| d.avail_ts <= boundary).collect();

        for (i, t) in truncated.iter().enumerate() {
            match prefix.get(i) {
                None => {
                    divergences.push(Divergence {
                        cut,
                        avail_ts: t.avail_ts,
                        detail: format!(
                            "the truncated run decided at {} and the full run made no decision \
                             there — deleting the future ADDED a decision",
                            t.avail_ts
                        ),
                    });
                    break;
                }
                Some(f) => {
                    compared += 1;
                    if f.avail_ts != t.avail_ts || f.bytes != t.bytes {
                        divergences.push(Divergence {
                            cut,
                            avail_ts: t.avail_ts,
                            detail: format!(
                                "decision {i} differs: truncated at {} ({} byte(s)), full at {} \
                                 ({} byte(s)). Decisions are compared bit-for-bit; a difference \
                                 of one tick is still information that flowed backwards",
                                t.avail_ts,
                                t.bytes.len(),
                                f.avail_ts,
                                f.bytes.len()
                            ),
                        });
                        break;
                    }
                }
            }
        }
        if truncated.len() < prefix.len() {
            divergences.push(Divergence {
                cut,
                avail_ts: prefix[truncated.len()].avail_ts,
                detail: format!(
                    "the full run decided at {} and the truncated run did not — deleting the \
                     future REMOVED a decision it should not have seen the cause of",
                    prefix[truncated.len()].avail_ts
                ),
            });
        }
    }

    TruncationReport {
        cuts: cuts.to_vec(),
        divergences,
        compared,
    }
}

#[cfg(test)]
mod tests;
