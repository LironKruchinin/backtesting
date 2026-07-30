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

/// One scored bar: what a signal said, and when that could have been known.
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
    /// The signal's continuous reading on this bar.
    pub score: f64,
    /// The tradeable close, in points.
    pub price_points: f64,
}

/// One score paired with the return that followed it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// The score, known at `avail_ts`.
    pub score: f64,
    /// The realized return over the horizon, as a fraction.
    pub fwd_return: f64,
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
/// `scored` must be in ascending `avail_ts` order — the order a [`Feed`] yields
/// and the order the engine replays. The partner of a score at `t` is the last
/// bar at or before `t + horizon_ns`, and must be strictly later than the scored
/// bar.
///
/// # Panics
/// Panics if `scored` is not sorted by `avail_ts`, because an unsorted series
/// silently produces a join that is neither forward nor backward, and a wrong
/// number here is indistinguishable from a right one.
///
/// [`Feed`]: crucible_core::Feed
#[must_use]
pub fn join(scored: &[ScoredBar], horizon_ns: i64) -> JoinReport {
    assert!(
        scored.windows(2).all(|w| w[0].avail_ts <= w[1].avail_ts),
        "INVARIANT: S0 joins an availability-ordered series; \
         an unsorted one produces a join that is neither forward nor backward"
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
            score: s.score,
            fwd_return: exit / s.price_points - 1.0,
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

/// One quantile bucket of the score, and the returns that followed it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bucket {
    /// Lowest score in the bucket.
    pub score_lo: f64,
    /// Highest score in the bucket.
    pub score_hi: f64,
    /// Observations in the bucket.
    pub n: usize,
    /// Mean forward return of the bucket, as a fraction.
    pub mean_return: f64,
}

/// Cuts `pairs` into `k` equal-count buckets by score and averages the forward
/// return in each.
///
/// Equal **count**, not equal width: a score whose distribution is skewed would
/// otherwise put nine tenths of the sample in one bucket and report the other
/// nine as noise. Buckets are returned lowest-score first, which is the order a
/// monotonicity check reads them in.
///
/// Returns an empty vector when `k` is zero or there are fewer pairs than
/// buckets — there is no honest way to cut 3 observations into 5 groups.
#[must_use]
pub fn buckets(pairs: &[Pair], k: usize) -> Vec<Bucket> {
    if k == 0 || pairs.len() < k {
        return Vec::new();
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
        let sum: f64 = slice.iter().map(|&p| pairs[p].fwd_return).sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "bucket sizes are far below 2^53"
        )]
        let count = slice.len() as f64;
        out.push(Bucket {
            score_lo: pairs[slice[0]].score,
            score_hi: pairs[slice[slice.len() - 1]].score,
            n: slice.len(),
            mean_return: sum / count,
        });
    }
    out
}

/// A bootstrap interval for a mean.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// Whether the interval excludes zero — the shape H-008's Gate 0 registers.
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

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The caller: S0 as a funnel stage (D-0085)
// ---------------------------------------------------------------------------

/// What S0 was asked to measure, declared in the config's `[s0]` block before
/// the run.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct HorizonEvidence {
    /// The horizon, in nanoseconds.
    pub horizon_ns: i64,
    /// Score/return pairs that survived the join.
    pub n_pairs: usize,
    /// Scores whose window ran off the end of the series, or had no partner.
    pub dropped: usize,
    /// Spearman rank correlation, or `None` when there was nothing to
    /// correlate.
    pub ic: Option<f64>,
    /// Equal-count buckets of the score, lowest first.
    pub buckets: Vec<Bucket>,
    /// Block-bootstrap interval for the mean forward return.
    pub interval: Option<Interval>,
}

/// One combo's S0 evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct S0ComboReport {
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
    /// The largest `|IC|` across horizons, reported whether or not it was
    /// significant.
    pub best_abs_ic: Option<f64>,
    /// The first horizon (in nanoseconds) that cleared BOTH halves of the
    /// criterion, if any. `None` is the null-harness answer.
    pub cleared_at_ns: Option<i64>,
    /// Whether the pre-registered relationship criterion was met.
    pub passed: bool,
}

/// The whole S0 stage over a grid.
#[derive(Debug, Clone, PartialEq)]
pub struct S0Report {
    /// One entry per combo, in grid-index order.
    pub combos: Vec<S0ComboReport>,
    /// The slot the score was read from.
    pub score_slot: String,
    /// Combos whose evidence cleared the criterion.
    pub survivors: usize,
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
            fill_model: "none (s0 takes no position)".to_owned(),
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
            if let Some(score) = scorer.update(bar) {
                scored.push(ScoredBar {
                    avail_ts: bar.avail_ts().0,
                    session_key: inputs.day_keys.get(i).copied().unwrap_or(0),
                    score,
                    price_points: bar.close.as_points_f64(),
                });
            }
        }

        let mut horizons = Vec::with_capacity(inputs.s0.horizons_ns.len());
        let mut best_abs_ic: Option<f64> = None;
        for (h, &horizon_ns) in inputs.s0.horizons_ns.iter().enumerate() {
            let joined = join(&scored, horizon_ns);
            let ic = information_coefficient(&joined.pairs);
            if let Some(v) = ic {
                best_abs_ic = Some(best_abs_ic.map_or(v.abs(), |b: f64| b.max(v.abs())));
            }
            // The bootstrap's seed is derived from the run's, per horizon, so
            // two horizons of one combo do not share a resample (D-0064).
            let draw_seed = seed ^ ((h as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            horizons.push(HorizonEvidence {
                horizon_ns,
                n_pairs: joined.pairs.len(),
                dropped: joined.dropped_no_partner + joined.dropped_zero_price,
                ic,
                buckets: buckets(&joined.pairs, inputs.s0.buckets),
                interval: block_bootstrap_mean(&joined.pairs, inputs.s0.bootstrap_draws, draw_seed),
            });
        }

        // Both halves, at the same horizon.
        let cleared_at_ns = horizons
            .iter()
            .find(|h| {
                h.ic.is_some_and(|ic| ic.abs() >= inputs.s0.min_abs_ic)
                    && h.interval.is_some_and(|iv| iv.excludes_zero())
            })
            .map(|h| h.horizon_ns);
        let passed = cleared_at_ns.is_some();
        registry.finish(&key, RunStatus::Done, None, inputs.now)?;

        combos.push(S0ComboReport {
            combo_index: combo.index,
            label: combo.label(),
            warmup_bars: scorer.warmup_bars(),
            scores: scored.len(),
            horizons,
            best_abs_ic,
            cleared_at_ns,
            passed,
        });
    }

    let survivors = combos.iter().filter(|c| c.passed).count();
    Ok(S0Report {
        combos,
        score_slot: inputs.s0.score_slot.clone(),
        survivors,
    })
}
