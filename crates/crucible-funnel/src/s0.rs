//! S0 — signal triage: a score goes in, forward-return evidence comes out, and
//! **nothing trades**.
//!
//! This module is the S0 predictor seam (D-0081): the measurement (D-0082) and,
//! below, the stage that calls it (D-0085). It
//! answers the only question worth asking before an equity curve exists: *does
//! this score predict the return that follows it, at what horizon, and by how
//! much* — with no orders, no fills, no portfolio and no fill model, because
//! none of those can rescue a signal that predicts nothing.
//!
//! # The join, and why it is the whole design
//!
//! A score is known at a bar's [`avail_ts`](crucible_core::Bar::avail_ts) — the
//! same instant the engine would let a strategy see it (§2.1). A **forward
//! return** is, by construction, information from *after* that instant. It is
//! legal here and nowhere else: this module consumes it as a *measurement*, and
//! the one thing that must never happen is a forward return reaching signal
//! space, where it becomes the lookahead §2.1 exists to prevent.
//!
//! So the join runs one way only. For a score at `avail_ts = t`, the partner is
//! the **last bar whose `avail_ts` is at or before `t + horizon`**, and it must
//! be strictly later than the scored bar itself. Never the first bar at or
//! after: that would read a price the horizon had not reached yet, which is a
//! lookahead of exactly one bar wearing a measurement's clothes.
//!
//! Two consequences worth stating because they surprise people:
//!
//! - **Horizons are durations, not bar counts.** `ohlcv` data has no bar for an
//!   interval that did not trade, so "ten bars ahead" is ten minutes only on a
//!   grain with no gaps. H-008 registers a *ten-minute* horizon and gets one.
//! - **A score with no partner is dropped and counted**, never zero-filled. The
//!   tail of every series has `horizon`-worth of scores that no bar answers, and
//!   a zero there is a fabricated observation that drags every statistic toward
//!   the middle.
//! - **The window must be fully observed, or the score is dropped.** A score is
//!   answered only when the series extends to `t + horizon` or beyond. Inside
//!   the series, a missing bar means *nothing traded* and the last bar in the
//!   window is the best price the horizon offered; at the **end** of the series
//!   the same absence means *the data stopped*, and the two are
//!   indistinguishable from inside. Pairing anyway would measure a one-minute
//!   return and label it ten — the mislabelling is silent, survives every
//!   downstream statistic, and biases the tail of every sample toward whatever
//!   the last few bars did.
//!
//! # Why the quantile buckets are not §2.1 lookahead
//!
//! [`buckets`] cuts the sample at quantiles computed over **the whole measured
//! sample**, which looks exactly like the full-sample quantile §2.1 names as
//! lookahead — so the difference has to be said out loud rather than assumed.
//! §2.1 bans full-sample statistics *used inside a strategy or feature*, because
//! there they decide a trade with information the trade could not have had.
//! Nothing here decides anything: no position is taken, and the bucket edges are
//! descriptive statistics of a finished measurement, in the same family as
//! reporting a sample mean. The moment a bucket edge is used to *trade* — a rule
//! saying "enter when the score is in the top quintile" — it is lookahead again
//! and must be recomputed from a trailing window.
//!
//! # Determinism
//!
//! Every reduction here runs in index order, every sort breaks ties on the
//! original index, and the bootstrap draws from a seeded [`ChaCha8Rng`] whose
//! seed arrives from the caller's lineage (D-0064). No clock, no `HashMap`
//! iteration, no thread-order-dependent float reduction (§2.2).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

/// One market bar on S0's price timeline, with an optional score.
///
/// `price_points` is the **tradeable** close in points, not the signal-space
/// one. The two coincide on every outright contract (`signal_offset` is zero —
/// D-0076), and the forward return is compared against a spread measured in
/// ticks, which is a tradeable quantity: measuring the payoff in signal space
/// and the cost in tradeable space would be comparing two different numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoredBar {
    /// When the score could first have been known (§2.1). The join's only key.
    pub avail_ts: i64,
    /// The trading day this bar belongs to, supplied by the caller as an
    /// `&[i64]` key — the D-0071 device, so the funnel never derives a day.
    pub session_key: i64,
    /// The signal's continuous reading, absent while the scorer has no
    /// opinion. The bar remains in the price timeline either way.
    pub score: Option<f64>,
    /// The tradeable close, in points.
    pub price_points: f64,
}

/// One score paired with the return that followed it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    /// The score, known at `avail_ts`.
    pub score: f64,
    /// The realized return over the horizon, as a fraction.
    pub fwd_return: f64,
    /// Tradeable entry close, in points. Retained so ticks are computed for
    /// this observation rather than reconstructed from an aggregate return.
    pub entry_price_points: f64,
    /// Tradeable exit close, in points. See [`Self::entry_price_points`].
    pub exit_price_points: f64,
    /// The session the *score* belongs to — the bootstrap's block key.
    pub session_key: i64,
}

/// The outcome of one join: the pairs it produced and the scores it could not
/// answer.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinReport {
    /// Score/return pairs, in the order the scores appeared.
    pub pairs: Vec<Pair>,
    /// Scores whose horizon reached past the end of the series.
    pub dropped_no_partner: usize,
    /// Scores dropped because the entry price was not usable as a denominator.
    pub dropped_zero_price: usize,
}

/// Joins scores to the returns that followed them, `horizon_ns` later.
///
/// `scored` must contain every bar in strictly ascending `avail_ts` order — the order a
/// [`Feed`] yields and the order the engine replays. A bar without a score is
/// not an entry observation, but remains eligible as a tradeable exit. The
/// partner of a score at `t` is the last bar at or before `t + horizon_ns`, and
/// must be strictly later than the score-bearing bar.
///
/// # Panics
/// Panics if `scored` is not strictly ordered by `avail_ts`. Duplicate
/// availability timestamps cannot establish which close is forward.
///
/// [`Feed`]: crucible_core::Feed
#[must_use]
pub fn join(scored: &[ScoredBar], horizon_ns: i64) -> JoinReport {
    assert!(
        scored.windows(2).all(|w| w[0].avail_ts < w[1].avail_ts),
        "INVARIANT: S0 joins a strictly availability-ordered series; \
         duplicate or decreasing timestamps cannot define a forward return"
    );
    assert!(
        horizon_ns > 0,
        "INVARIANT: a forward horizon is strictly positive; \
         zero would pair a score with its own bar"
    );

    let mut pairs = Vec::new();
    let mut dropped_no_partner = 0;
    let mut dropped_zero_price = 0;
    // The last instant the series can speak about. A score whose horizon
    // reaches past it is unanswerable, not answerable-with-a-shorter-window.
    let series_end = match scored.last() {
        Some(b) => b.avail_ts,
        None => {
            return JoinReport {
                pairs,
                dropped_no_partner,
                dropped_zero_price,
            };
        }
    };
    // The partner index is non-decreasing in `i`, so one forward scan answers
    // every score — O(n) rather than O(n log n) per horizon, and no binary
    // search whose tie-breaking could differ between horizons.
    let mut j = 0usize;

    for (i, s) in scored.iter().enumerate() {
        let Some(score) = s.score else {
            continue;
        };
        let target = s.avail_ts.saturating_add(horizon_ns);
        if target > series_end {
            // The window runs off the end of the data: we cannot tell "nothing
            // traded" from "the series stopped", so the score is unanswerable.
            dropped_no_partner += 1;
            continue;
        }
        if j < i {
            j = i;
        }
        // Advance to the LAST bar at or before the target instant.
        while j + 1 < scored.len() && scored[j + 1].avail_ts <= target {
            j += 1;
        }
        if j <= i {
            // Nothing landed inside the horizon: the series ended, or the gap
            // to the next bar is wider than the horizon itself.
            dropped_no_partner += 1;
            continue;
        }
        if s.price_points == 0.0 || !s.price_points.is_finite() {
            dropped_zero_price += 1;
            continue;
        }
        let exit = scored[j].price_points;
        if !exit.is_finite() {
            dropped_zero_price += 1;
            continue;
        }
        pairs.push(Pair {
            score,
            fwd_return: exit / s.price_points - 1.0,
            entry_price_points: s.price_points,
            exit_price_points: exit,
            session_key: s.session_key,
        });
    }

    JoinReport {
        pairs,
        dropped_no_partner,
        dropped_zero_price,
    }
}

/// Average ranks of `xs`, ties sharing the mean of the ranks they span.
///
/// Sorting breaks ties on the original index so the result does not depend on
/// the sort's stability (§2.2).
fn ranks(xs: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..xs.len()).collect();
    idx.sort_by(|&a, &b| {
        xs[a]
            .partial_cmp(&xs[b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut out = vec![0.0; xs.len()];
    let mut i = 0usize;
    while i < idx.len() {
        let mut k = i + 1;
        while k < idx.len() && xs[idx[k]] == xs[idx[i]] {
            k += 1;
        }
        // Ranks are 1-based; the tied group shares their average.
        let lo = i + 1;
        let hi = k;
        #[expect(
            clippy::cast_precision_loss,
            reason = "rank arithmetic on sample sizes far below 2^53"
        )]
        let avg = (lo + hi) as f64 / 2.0;
        for &p in &idx[i..k] {
            out[p] = avg;
        }
        i = k;
    }
    out
}

/// Pearson correlation, in index order so the reduction is deterministic.
fn pearson(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample sizes here are far below 2^53"
    )]
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    let mut syy = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mx;
        let dy = y - my;
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    let denom = (sxx * syy).sqrt();
    if denom == 0.0 || !denom.is_finite() {
        return None;
    }
    Some(sxy / denom)
}

/// Spearman rank correlation between score and forward return — the
/// information coefficient S0 reports.
///
/// Returns `None` when there is nothing to correlate: fewer than two pairs, or
/// a constant score or return (a flat series has no rank order, and reporting
/// zero would claim we measured no relationship rather than that we could not
/// measure one).
#[must_use]
pub fn information_coefficient(pairs: &[Pair]) -> Option<f64> {
    if pairs.len() < 2 {
        return None;
    }
    let scores: Vec<f64> = pairs.iter().map(|p| p.score).collect();
    let rets: Vec<f64> = pairs.iter().map(|p| p.fwd_return).collect();
    pearson(&ranks(&scores), &ranks(&rets))
}

fn information_coefficient_evidence(pairs: &[Pair]) -> Availability<f64> {
    if pairs.len() < 2 {
        return Availability::Unavailable {
            reason: UnavailableReason::TooFewObservations {
                observed: pairs.len(),
                required: 2,
            },
        };
    }
    if pairs
        .iter()
        .any(|p| !p.score.is_finite() || !p.fwd_return.is_finite())
    {
        return Availability::Unavailable {
            reason: UnavailableReason::NonFiniteObservation,
        };
    }
    if pairs.iter().all(|p| p.score == pairs[0].score) {
        return Availability::Unavailable {
            reason: UnavailableReason::ConstantScore,
        };
    }
    if pairs.iter().all(|p| p.fwd_return == pairs[0].fwd_return) {
        return Availability::Unavailable {
            reason: UnavailableReason::ConstantForwardReturn,
        };
    }
    match information_coefficient(pairs) {
        Some(value) if value.is_finite() => Availability::Available { value },
        _ => Availability::Unavailable {
            reason: UnavailableReason::DegenerateCorrelation,
        },
    }
}

/// One quantile bucket of the score, and the returns that followed it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bucket {
    /// Lowest score in the bucket.
    pub score_lo: f64,
    /// Highest score in the bucket.
    pub score_hi: f64,
    /// Observations in the bucket.
    pub n: usize,
    /// Mean forward return of the bucket, as a fraction.
    pub mean_return: f64,
    /// Mean forward price move in contract ticks, computed observation by
    /// observation from each retained tradeable entry/exit pair.
    pub mean_move_ticks: f64,
}

/// A non-empty, lowest-score-first bucket sequence.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BucketSet(Vec<Bucket>);

fn validate_buckets(buckets: &[Bucket]) -> Result<(), &'static str> {
    if buckets.is_empty() {
        return Err("an available bucket set cannot be empty");
    }
    if buckets.iter().any(|bucket| {
        bucket.n == 0
            || !bucket.score_lo.is_finite()
            || !bucket.score_hi.is_finite()
            || !bucket.mean_return.is_finite()
            || !bucket.mean_move_ticks.is_finite()
            || bucket.score_lo > bucket.score_hi
    }) {
        return Err("every bucket must be finite, nonempty, and have ordered bounds");
    }
    if buckets
        .windows(2)
        .any(|pair| pair[0].score_hi > pair[1].score_lo)
    {
        return Err("buckets must be lowest-score first and non-overlapping");
    }
    Ok(())
}

impl BucketSet {
    fn new(buckets: Vec<Bucket>) -> Self {
        assert!(validate_buckets(&buckets).is_ok());
        Self(buckets)
    }

    /// Constructs a bucket set, refusing an empty success.
    #[must_use]
    pub fn from_nonempty(buckets: Vec<Bucket>) -> Option<Self> {
        validate_buckets(&buckets).ok().map(|()| Self(buckets))
    }

    /// Buckets in declared lowest-score-first order.
    #[must_use]
    pub fn as_slice(&self) -> &[Bucket] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BucketSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let buckets = Vec::<Bucket>::deserialize(deserializer)?;
        validate_buckets(&buckets).map_err(serde::de::Error::custom)?;
        Ok(Self(buckets))
    }
}

/// Why a declared S0 measurement could not be formed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UnavailableReason {
    /// The sample did not contain the minimum number of usable observations.
    TooFewObservations {
        /// Usable observations found.
        observed: usize,
        /// Minimum required by this measurement.
        required: usize,
    },
    /// All scores shared one value, so they had no rank order.
    ConstantScore,
    /// All forward returns shared one value, so they had no rank order.
    ConstantForwardReturn,
    /// The declaration requested no quantile buckets.
    NoBucketsDeclared,
    /// The declaration requested no bootstrap resamples.
    NoBootstrapDraws,
    /// A score or measured return was not finite.
    NonFiniteObservation,
    /// Rank covariance was degenerate despite non-constant inputs.
    DegenerateCorrelation,
    /// The unconditional bootstrap could not form a resample.
    DegenerateBootstrap,
}

impl std::fmt::Display for UnavailableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewObservations { observed, required } => {
                write!(
                    f,
                    "too few observations (observed: {observed}, required: {required})"
                )
            }
            Self::ConstantScore => write!(f, "all scores are equal"),
            Self::ConstantForwardReturn => write!(f, "all forward returns are equal"),
            Self::NoBucketsDeclared => write!(f, "zero buckets were declared"),
            Self::NoBootstrapDraws => write!(f, "zero bootstrap draws were declared"),
            Self::NonFiniteObservation => write!(f, "a score or return was non-finite"),
            Self::DegenerateCorrelation => write!(f, "rank correlation was degenerate"),
            Self::DegenerateBootstrap => write!(f, "bootstrap resampling was degenerate"),
        }
    }
}

/// A measured value or an explicit, persisted reason it was unavailable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Availability<T> {
    /// The measurement exists.
    Available {
        /// Its typed value.
        value: T,
    },
    /// The measurement does not exist and must not be read as zero.
    Unavailable {
        /// The exact reason measurement was impossible.
        reason: UnavailableReason,
    },
}

impl<T> Availability<T> {
    /// Borrows the available value, if one exists.
    #[must_use]
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Available { value } => Some(value),
            Self::Unavailable { .. } => None,
        }
    }

    /// Borrows the absence reason, if unavailable.
    #[must_use]
    pub fn reason(&self) -> Option<&UnavailableReason> {
        match self {
            Self::Available { .. } => None,
            Self::Unavailable { reason } => Some(reason),
        }
    }
}

/// Cuts `pairs` into `k` equal-count buckets by score and averages the forward
/// return in each.
///
/// Equal **count**, not equal width: a score whose distribution is skewed would
/// otherwise put nine tenths of the sample in one bucket and report the other
/// nine as noise. Buckets are returned lowest-score first, which is the order a
/// monotonicity check reads them in.
///
/// Returns a typed unavailable result when `k` is zero or there are fewer
/// pairs than buckets — there is no honest way to cut 3 observations into 5
/// groups, and an empty table would conceal that fact.
#[must_use]
pub fn buckets(pairs: &[Pair], k: usize, tick_size_points: f64) -> Availability<BucketSet> {
    assert!(tick_size_points.is_finite());
    assert!(tick_size_points > 0.0);
    if pairs.iter().any(|pair| {
        !pair.score.is_finite()
            || !pair.fwd_return.is_finite()
            || !pair.entry_price_points.is_finite()
            || !pair.exit_price_points.is_finite()
    }) {
        return Availability::Unavailable {
            reason: UnavailableReason::NonFiniteObservation,
        };
    }
    if k == 0 {
        return Availability::Unavailable {
            reason: UnavailableReason::NoBucketsDeclared,
        };
    }
    if pairs.len() < k {
        return Availability::Unavailable {
            reason: UnavailableReason::TooFewObservations {
                observed: pairs.len(),
                required: k,
            },
        };
    }
    let mut idx: Vec<usize> = (0..pairs.len()).collect();
    idx.sort_by(|&a, &b| {
        pairs[a]
            .score
            .partial_cmp(&pairs[b].score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let n = pairs.len();
    let mut out = Vec::with_capacity(k);
    for b in 0..k {
        // Boundaries by exact integer arithmetic, so the bucket sizes differ by
        // at most one and no observation is used twice or skipped.
        let lo = b * n / k;
        let hi = (b + 1) * n / k;
        let slice = &idx[lo..hi];
        if slice.is_empty() {
            continue;
        }
        let return_sum: f64 = slice.iter().map(|&p| pairs[p].fwd_return).sum();
        let tick_sum: f64 = slice
            .iter()
            .map(|&p| (pairs[p].exit_price_points - pairs[p].entry_price_points) / tick_size_points)
            .sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "bucket sizes are far below 2^53"
        )]
        let count = slice.len() as f64;
        out.push(Bucket {
            score_lo: pairs[slice[0]].score,
            score_hi: pairs[slice[slice.len() - 1]].score,
            n: slice.len(),
            mean_return: return_sum / count,
            mean_move_ticks: tick_sum / count,
        });
    }
    Availability::Available {
        value: BucketSet::new(out),
    }
}

/// A bootstrap interval for a mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interval {
    /// The point estimate on the observed sample.
    pub point: f64,
    /// Lower percentile of the resampled means.
    pub lo: f64,
    /// Upper percentile of the resampled means.
    pub hi: f64,
    /// Resamples drawn.
    pub draws: usize,
}

impl Interval {
    /// Whether this unconditional interval excludes zero.
    ///
    /// This is not H-008 Gate 0b, whose population is conditional on a close
    /// beyond its Bollinger band.
    #[must_use]
    pub fn excludes_zero(&self) -> bool {
        (self.lo > 0.0 && self.hi > 0.0) || (self.lo < 0.0 && self.hi < 0.0)
    }
}

/// Block bootstrap of the mean forward return, resampling **whole sessions**.
///
/// Resampling individual observations would treat a minute's return as
/// independent of the minute beside it, which is false at every horizon this
/// stage measures and produces an interval far too narrow. A session is the
/// natural block: it is the unit the archive is organized in, the unit a fold
/// boundary lands on (D-0062), and long enough to carry the autocorrelation a
/// short horizon has.
///
/// Sessions are drawn with replacement until the resample holds at least as
/// many observations as the original sample. `seed` comes from the caller's
/// lineage (D-0064); the same seed and the same pairs give the same interval on
/// any machine.
#[must_use]
pub fn block_bootstrap_mean(pairs: &[Pair], draws: usize, seed: u64) -> Option<Interval> {
    if pairs.is_empty() || draws == 0 {
        return None;
    }
    // Group by session, preserving first-seen order so the block list does not
    // depend on a hash (§2.2).
    let mut session_order: Vec<i64> = Vec::new();
    let mut blocks: Vec<Vec<f64>> = Vec::new();
    for p in pairs {
        match session_order.iter().position(|&s| s == p.session_key) {
            Some(i) => blocks[i].push(p.fwd_return),
            None => {
                session_order.push(p.session_key);
                blocks.push(vec![p.fwd_return]);
            }
        }
    }
    if blocks.is_empty() {
        return None;
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "sample sizes here are far below 2^53"
    )]
    let point = pairs.iter().map(|p| p.fwd_return).sum::<f64>() / pairs.len() as f64;

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut means = Vec::with_capacity(draws);
    let target = pairs.len();
    for _ in 0..draws {
        let mut sum = 0.0;
        let mut count = 0usize;
        while count < target {
            // `next_u64 % len` is a negligibly biased choice for the block
            // counts this sees, and it is the same arithmetic on every
            // platform — which is what §2.2 asks of it.
            let pick = (rng.next_u64() % blocks.len() as u64) as usize;
            for v in &blocks[pick] {
                sum += v;
                count += 1;
            }
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "sample sizes here are far below 2^53"
        )]
        let mean = sum / count as f64;
        means.push(mean);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // The 2.5 / 97.5 percentiles, by nearest-rank on the sorted draws.
    let lo_i = (draws as f64 * 0.025).floor() as usize;
    let hi_i = ((draws as f64 * 0.975).ceil() as usize).saturating_sub(1);
    Some(Interval {
        point,
        lo: means[lo_i.min(draws - 1)],
        hi: means[hi_i.min(draws - 1)],
        draws,
    })
}

fn unconditional_mean_evidence(pairs: &[Pair], draws: usize, seed: u64) -> Availability<Interval> {
    if pairs.is_empty() {
        return Availability::Unavailable {
            reason: UnavailableReason::TooFewObservations {
                observed: 0,
                required: 1,
            },
        };
    }
    if draws == 0 {
        return Availability::Unavailable {
            reason: UnavailableReason::NoBootstrapDraws,
        };
    }
    match block_bootstrap_mean(pairs, draws, seed) {
        Some(value) => Availability::Available { value },
        None => Availability::Unavailable {
            reason: UnavailableReason::DegenerateBootstrap,
        },
    }
}

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The caller: S0 as a funnel stage (D-0085)
// ---------------------------------------------------------------------------

/// Registry fill-model marker for a predictor measurement that takes no position.
pub const S0_FILL_MODEL: &str = "none (s0 takes no position)";

/// What S0 was asked to measure, declared in the config's `[s0]` block before
/// the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S0Spec {
    /// The indicator slot the score is read from, e.g. `z` or `bb.upper`.
    pub score_slot: String,
    /// Forward horizons, in nanoseconds, in the order declared.
    pub horizons_ns: Vec<i64>,
    /// Quantile buckets of the score.
    pub buckets: usize,
    /// Bootstrap resamples.
    pub bootstrap_draws: usize,
    /// **Pre-registered**: the smallest `|IC|` at a declared horizon that
    /// counts as a relationship.
    ///
    /// A relationship needs **both** halves: `|IC| >= min_abs_ic` *and* a mean
    /// forward return whose bootstrap interval excludes zero, at the **same**
    /// horizon. Size without significance is what a large enough sample of
    /// noise produces for free — the null harness measures `|IC| = 0.0378` on
    /// 20,000 bars of seeded random walk, which would clear any bar low enough
    /// to be useful. Significance without size is an effect too small to trade.
    /// Requiring both at one horizon is what makes this a gate rather than a
    /// formality (D-0085).
    pub min_abs_ic: f64,
}

/// One horizon's evidence about one combo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HorizonEvidence {
    /// The horizon, in nanoseconds.
    pub horizon_ns: i64,
    /// Score/return pairs that survived the join.
    pub n_pairs: usize,
    /// Scores whose window ran off the end of the series, or had no partner.
    pub dropped_no_partner: usize,
    /// Scores dropped because an entry or exit price was invalid.
    pub dropped_invalid_price: usize,
    /// Spearman rank correlation, or a typed absence reason.
    pub ic: Availability<f64>,
    /// Equal-count buckets of the score, lowest first, or typed absence.
    pub buckets: Availability<BucketSet>,
    /// Unconditional block-bootstrap interval over every joined observation.
    pub unconditional_mean_interval: Availability<Interval>,
}

/// Outcome of D-0085's generic S0 criterion.
///
/// This is not H-008 Gate 0b: the latter is conditional on a close beyond its
/// Bollinger band, while these measurements use equal-count score buckets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum S0CriterionOutcome {
    /// Both declared halves cleared at the first such horizon.
    Passed {
        /// Horizon that cleared, in nanoseconds.
        horizon_ns: i64,
    },
    /// Every declared horizon had both measurements, but none cleared both.
    Failed,
    /// At least one required measurement was absent and no horizon passed.
    Unavailable {
        /// Representative typed reason; each horizon retains its own reason.
        reason: UnavailableReason,
    },
}

fn evaluate_criterion(horizons: &[HorizonEvidence], min_abs_ic: f64) -> S0CriterionOutcome {
    let mut first_absence = None;
    for horizon in horizons {
        match (&horizon.ic, &horizon.unconditional_mean_interval) {
            (Availability::Available { value: ic }, Availability::Available { value: mean }) => {
                if ic.abs() >= min_abs_ic && mean.excludes_zero() {
                    return S0CriterionOutcome::Passed {
                        horizon_ns: horizon.horizon_ns,
                    };
                }
            }
            (Availability::Unavailable { reason }, _) => {
                first_absence.get_or_insert_with(|| reason.clone());
            }
            (_, Availability::Unavailable { reason }) => {
                first_absence.get_or_insert_with(|| reason.clone());
            }
        }
    }
    if let Some(reason) = first_absence {
        S0CriterionOutcome::Unavailable { reason }
    } else if horizons.is_empty() {
        S0CriterionOutcome::Unavailable {
            reason: UnavailableReason::TooFewObservations {
                observed: 0,
                required: 1,
            },
        }
    } else {
        S0CriterionOutcome::Failed
    }
}

/// Population this evidence product actually measures.
///
/// This is a typed capability boundary, not an H-008 experiment result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "population", rename_all = "snake_case", deny_unknown_fields)]
pub enum S0EvidenceScope {
    /// Equal-count buckets over every available score. This does not evaluate
    /// a gate conditional on closes beyond a Bollinger band.
    EqualCountScoreBuckets,
}

/// One combo's S0 evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct S0ComboReport {
    /// Fully resolved score identity: spec, slot expression, and combo label.
    pub score_identity: String,
    /// Declared contract tick in fixed-point nanopoints.
    pub tick_size_nanopoints: i64,
    /// The complete pre-run S0 declaration.
    pub spec: S0Spec,
    /// Index into the expanded grid.
    pub combo_index: usize,
    /// Human-readable parameters.
    pub label: String,
    /// Bars consumed before the score had an opinion.
    pub warmup_bars: usize,
    /// Scores emitted (post-warmup bars).
    pub scores: usize,
    /// One entry per declared horizon, in declaration order.
    pub horizons: Vec<HorizonEvidence>,
}

#[derive(Serialize)]
struct S0ComboWireRef<'a> {
    score_identity: &'a str,
    tick_size_nanopoints: i64,
    spec: &'a S0Spec,
    combo_index: usize,
    label: &'a str,
    warmup_bars: usize,
    scores: usize,
    horizons: &'a [HorizonEvidence],
    criterion: S0CriterionOutcome,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct S0ComboWire {
    score_identity: String,
    tick_size_nanopoints: i64,
    spec: S0Spec,
    combo_index: usize,
    label: String,
    warmup_bars: usize,
    scores: usize,
    horizons: Vec<HorizonEvidence>,
    criterion: S0CriterionOutcome,
}

impl Serialize for S0ComboReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        S0ComboWireRef {
            score_identity: &self.score_identity,
            tick_size_nanopoints: self.tick_size_nanopoints,
            spec: &self.spec,
            combo_index: self.combo_index,
            label: &self.label,
            warmup_bars: self.warmup_bars,
            scores: self.scores,
            horizons: &self.horizons,
            criterion: self.criterion(),
        }
        .serialize(serializer)
    }
}

impl S0ComboReport {
    /// Validates the cross-field invariants every consumer relies on.
    pub fn validate(&self) -> Result<(), String> {
        if self.score_identity.trim().is_empty() || self.label.trim().is_empty() {
            return Err("score identity and combo label must be nonempty".to_owned());
        }
        if self.tick_size_nanopoints <= 0 {
            return Err("contract tick must be positive".to_owned());
        }
        if self.spec.score_slot.trim().is_empty()
            || self.spec.horizons_ns.is_empty()
            || !self.spec.min_abs_ic.is_finite()
            || self.spec.min_abs_ic < 0.0
        {
            return Err("S0 declaration is incomplete or non-finite".to_owned());
        }
        if self.horizons.len() != self.spec.horizons_ns.len() {
            return Err("evidence must contain every declared horizon exactly once".to_owned());
        }
        for (declared, horizon) in self.spec.horizons_ns.iter().zip(&self.horizons) {
            if *declared <= 0 || *declared != horizon.horizon_ns {
                return Err("evidence horizons must retain positive declaration order".to_owned());
            }
            let classified = horizon
                .n_pairs
                .checked_add(horizon.dropped_no_partner)
                .and_then(|n| n.checked_add(horizon.dropped_invalid_price))
                .ok_or_else(|| "S0 observation counts overflowed".to_owned())?;
            if classified != self.scores {
                return Err("pair and drop counts must classify every emitted score".to_owned());
            }
            if let Availability::Available { value } = &horizon.ic
                && (!value.is_finite() || value.abs() > 1.0 + 16.0 * f64::EPSILON)
            {
                return Err("available IC must be finite and within [-1, 1]".to_owned());
            }
            if let Availability::Available { value } = &horizon.buckets {
                if value.as_slice().len() != self.spec.buckets {
                    return Err("available bucket count must equal the declaration".to_owned());
                }
                let observations = value
                    .as_slice()
                    .iter()
                    .try_fold(0usize, |sum, bucket| sum.checked_add(bucket.n))
                    .ok_or_else(|| "bucket observation counts overflowed".to_owned())?;
                if observations != horizon.n_pairs {
                    return Err("bucket observations must equal the joined pair count".to_owned());
                }
            }
            if let Availability::Available { value } = &horizon.unconditional_mean_interval
                && (!value.point.is_finite()
                    || !value.lo.is_finite()
                    || !value.hi.is_finite()
                    || value.lo > value.hi
                    || value.draws != self.spec.bootstrap_draws)
            {
                return Err("available unconditional interval is inconsistent".to_owned());
            }
        }
        Ok(())
    }

    /// Largest measurable absolute IC across declared horizons.
    #[must_use]
    pub fn best_abs_ic(&self) -> Option<f64> {
        self.horizons
            .iter()
            .filter_map(|h| h.ic.value().copied())
            .map(f64::abs)
            .reduce(f64::max)
    }

    /// Whether the generic S0 criterion cleared.
    #[must_use]
    pub fn passed(&self) -> bool {
        matches!(self.criterion(), S0CriterionOutcome::Passed { .. })
    }

    /// Outcome derived from this report's declaration and evidence.
    #[must_use]
    pub fn criterion(&self) -> S0CriterionOutcome {
        evaluate_criterion(&self.horizons, self.spec.min_abs_ic)
    }
}

impl<'de> Deserialize<'de> for S0ComboReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = S0ComboWire::deserialize(deserializer)?;
        let stored_criterion = wire.criterion;
        let report = Self {
            score_identity: wire.score_identity,
            tick_size_nanopoints: wire.tick_size_nanopoints,
            spec: wire.spec,
            combo_index: wire.combo_index,
            label: wire.label,
            warmup_bars: wire.warmup_bars,
            scores: wire.scores,
            horizons: wire.horizons,
        };
        report.validate().map_err(serde::de::Error::custom)?;
        if report.criterion() != stored_criterion {
            return Err(serde::de::Error::custom(
                "stored S0 criterion contradicts its declaration or evidence",
            ));
        }
        Ok(report)
    }
}

/// The complete typed result persisted for one measured S0 run.
///
/// The scope travels with the combo evidence so a registry reader never has
/// to infer which population the buckets describe. In particular,
/// [`S0EvidenceScope::EqualCountScoreBuckets`] is not the close-beyond-band
/// population registered by the original H-008 Gate 0b.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S0RunMetrics {
    /// Exact population measured by the evidence below.
    pub evidence_scope: S0EvidenceScope,
    /// Identity, declaration, observations, measurements, and derived outcome.
    pub combo: S0ComboReport,
}

impl S0RunMetrics {
    /// Validates the decision-bearing evidence after it has been read.
    pub fn validate(&self) -> Result<(), String> {
        self.combo.validate()
    }
}

/// The whole S0 stage over a grid.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct S0Report {
    /// Exact population capability shared by every combo in this report.
    pub evidence_scope: S0EvidenceScope,
    /// One entry per combo, in grid-index order.
    pub combos: Vec<S0ComboReport>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct S0ReportWire {
    evidence_scope: S0EvidenceScope,
    combos: Vec<S0ComboReport>,
}

impl<'de> Deserialize<'de> for S0Report {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = S0ReportWire::deserialize(deserializer)?;
        let report = Self {
            evidence_scope: wire.evidence_scope,
            combos: wire.combos,
        };
        report.validate().map_err(serde::de::Error::custom)?;
        Ok(report)
    }
}

impl S0Report {
    /// Validates invariants shared by assessment, persistence, rendering, and hashing.
    pub fn validate(&self) -> Result<(), String> {
        for (position, combo) in self.combos.iter().enumerate() {
            combo.validate()?;
            if combo.combo_index != position {
                return Err("S0 combos must appear once in contiguous grid-index order".to_owned());
            }
            if let Some(first) = self.combos.first()
                && combo.spec != first.spec
            {
                return Err("every combo must share one S0 declaration".to_owned());
            }
        }
        Ok(())
    }

    /// Combos whose generic S0 criterion cleared.
    #[must_use]
    pub fn survivors(&self) -> usize {
        self.combos.iter().filter(|combo| combo.passed()).count()
    }

    /// Canonical bytes consumed by the determinism hash.
    #[must_use]
    pub fn determinism_bytes(&self) -> Vec<u8> {
        self.validate().expect("invalid S0 report cannot be hashed");
        serde_json::to_vec(self).expect("S0 measurements must be finite")
    }
}

/// Anything that stops S0 from running.
#[derive(Debug)]
pub enum S0Error {
    /// The score slot could not be resolved against the config's indicators.
    Scorer(crucible_strategies::combo::ScorerError),
    /// The registry could not be claimed or written.
    Registry(crate::registry::RegistryError),
}

impl std::fmt::Display for S0Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            S0Error::Scorer(e) => write!(f, "s0: {e}"),
            S0Error::Registry(e) => write!(f, "s0: {e}"),
        }
    }
}

impl std::error::Error for S0Error {}

impl From<crate::registry::RegistryError> for S0Error {
    fn from(e: crate::registry::RegistryError) -> S0Error {
        S0Error::Registry(e)
    }
}

/// Everything S0 needs, supplied by the caller. No clock, no I/O.
pub struct S0Inputs<'a> {
    /// The shared bar series, in availability order.
    pub events: &'a [crucible_core::prelude::MarketEvent],
    /// One trading-day key per bar, the caller-computed slice (D-0071).
    pub day_keys: &'a [i64],
    /// The expanded grid.
    pub grid: &'a crucible_strategies::combo::Grid,
    /// The spec the grid came from.
    pub spec: &'a crucible_strategies::combo::ComboSpec,
    /// What to measure, declared before the run.
    pub s0: &'a S0Spec,
    /// Declared contract tick used for per-observation move conversion.
    pub tick_size: crucible_core::prelude::Price,
    /// Config hash, root seed, account.
    pub identity: &'a crate::walkforward::RunIdentity,
    /// The pre-registered criteria, stored verbatim on the registry row.
    pub criteria: &'a crate::stages::Criteria,
    /// `meta.hypothesis_family`. Mandatory: an S0 report that charged no family
    /// is a measurement nobody counted.
    pub hypothesis_family: &'a str,
    /// Repository revision (§2.5).
    pub git_sha: &'a str,
    /// blake3 of every archived file the series was read from (§2.5).
    pub data_manifest_ids: &'a [String],
    /// Wall clock, supplied by the caller.
    pub now: &'a str,
}

/// Runs S0 over a grid: score, join, measure, and **record**.
///
/// Every combo gets a registry row claimed *before* it is measured and a trial
/// charged to `hypothesis_family`, exactly like S1 and S2 — an S0 report that
/// existed outside the registry would be a number nobody counted, which is the
/// whole reason the predictor workbench arrives as a funnel stage rather than
/// beside one (D-0081).
///
/// Single-threaded on purpose: S0 is one streaming pass per combo with no
/// replay behind it, so there is nothing here for rayon to win back.
///
/// # Errors
/// [`S0Error::Scorer`] if the declared score slot does not resolve;
/// [`S0Error::Registry`] if a row cannot be claimed or written.
pub fn run_s0(
    inputs: &S0Inputs<'_>,
    registry: &mut crate::registry::Registry,
) -> Result<S0Report, S0Error> {
    use crate::registry::{RunKey, RunRow, RunStatus};
    use crucible_core::prelude::MarketEvent;

    assert_eq!(inputs.events.len(), inputs.day_keys.len());
    assert!(inputs.tick_size.as_nanos() > 0);
    let mut combos = Vec::with_capacity(inputs.grid.len());

    for combo in inputs.grid.iter() {
        let mut scorer = crucible_strategies::combo::ComboScorer::build(
            inputs.spec,
            &combo,
            &inputs.s0.score_slot,
        )
        .map_err(S0Error::Scorer)?;

        // Claim before measuring (registry rule 1). `fold: None` — S0 is one
        // pass over the whole series, not a fold table; the seed is the
        // combo's, derived from the same lineage every other run uses (D-0064).
        // Fold index 0: S0 is one pass over the whole series, so it has the
        // shape of a single fold rather than a table of them.
        let seed = crate::walkforward::derive_run_seed(
            &inputs.identity.config_hash,
            inputs.identity.root_seed,
            inputs.identity.account_id.as_deref(),
            combo.index,
            0,
        );
        let key = RunKey {
            config_hash: inputs.identity.config_hash.to_string(),
            account_id: inputs.identity.account_id.clone(),
            combo_index: combo.index,
            fold: None,
            seed,
        };
        let row = RunRow {
            key: key.clone(),
            hypothesis_family: inputs.hypothesis_family.to_owned(),
            params: combo.label(),
            // S0 takes no position, so it has no execution assumption to name.
            // Saying so beats leaving the field to imply one (§2.4).
            fill_model: S0_FILL_MODEL.to_owned(),
            git_sha: inputs.git_sha.to_owned(),
            data_manifest_ids: inputs.data_manifest_ids.to_vec(),
            started_at: inputs.now.to_owned(),
            criteria: inputs.criteria.clone(),
        };
        registry.insert_running(&row)?;

        // Score every bar, in availability order.
        let mut scored: Vec<ScoredBar> = Vec::with_capacity(inputs.events.len());
        for (i, ev) in inputs.events.iter().enumerate() {
            let MarketEvent::Bar(bar) = ev;
            let score = scorer.update(bar);
            scored.push(ScoredBar {
                avail_ts: bar.avail_ts().0,
                session_key: inputs.day_keys[i],
                score,
                price_points: bar.close.as_points_f64(),
            });
        }

        let mut horizons = Vec::with_capacity(inputs.s0.horizons_ns.len());
        for (h, &horizon_ns) in inputs.s0.horizons_ns.iter().enumerate() {
            let joined = join(&scored, horizon_ns);
            let ic = information_coefficient_evidence(&joined.pairs);
            // The bootstrap's seed is derived from the run's, per horizon, so
            // two horizons of one combo do not share a resample (D-0064).
            let draw_seed = seed ^ ((h as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            horizons.push(HorizonEvidence {
                horizon_ns,
                n_pairs: joined.pairs.len(),
                dropped_no_partner: joined.dropped_no_partner,
                dropped_invalid_price: joined.dropped_zero_price,
                ic,
                buckets: buckets(
                    &joined.pairs,
                    inputs.s0.buckets,
                    inputs.tick_size.as_points_f64(),
                ),
                unconditional_mean_interval: unconditional_mean_evidence(
                    &joined.pairs,
                    inputs.s0.bootstrap_draws,
                    draw_seed,
                ),
            });
        }

        registry.finish(&key, RunStatus::Done, None, inputs.now)?;

        let score_identity = format!(
            "{} score={} combo={}",
            inputs.spec.canonical_form(),
            scorer.slot_name(),
            combo.label()
        );
        combos.push(S0ComboReport {
            score_identity,
            tick_size_nanopoints: inputs.tick_size.as_nanos(),
            spec: inputs.s0.clone(),
            combo_index: combo.index,
            label: combo.label(),
            warmup_bars: scorer.warmup_bars(),
            scores: scored.iter().filter(|bar| bar.score.is_some()).count(),
            horizons,
        });
    }

    let report = S0Report {
        evidence_scope: S0EvidenceScope::EqualCountScoreBuckets,
        combos,
    };
    report
        .validate()
        .expect("run_s0 must construct a valid owned report");
    Ok(report)
}
