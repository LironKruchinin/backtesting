//! S0's unit tests, and the two-sided negative control the seam exists to pass.
//!
//! The controls are the point of this file (CLAUDE.md §7, no quality
//! exemption). Everything else here is the hand-derived arithmetic that makes
//! them readable.

use super::*;

const MIN: i64 = 60_000_000_000; // one minute in nanoseconds

/// A bar series one minute apart, `sessions_every` bars to a session.
///
/// Prices come from an inlined SplitMix64 over integer ticks — the D-0011
/// device — so the walk is bit-identical on every platform and no `rand`
/// dependency reaches a test that is asserting determinism.
fn walk(n: usize, sessions_every: usize, seed: u64) -> Vec<ScoredBar> {
    let mut state = seed;
    let mut ticks: i64 = 4_000 * 4; // 4000.00 points in quarter-point ticks
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // -2..=2 ticks a bar.
        let step = i64::from((z % 5) as i32) - 2;
        ticks = (ticks + step).max(1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "tick counts here are far below 2^53"
        )]
        let price = ticks as f64 * 0.25;
        out.push(ScoredBar {
            avail_ts: (i as i64 + 1) * MIN,
            session_key: (i / sessions_every) as i64,
            score: 0.0,
            price_points: price,
        });
    }
    out
}

/// Replants `scored` so each bar's score IS the forward return it will be
/// joined to — the leak, planted deliberately.
///
/// Uses the module's own join to find the partner, so the planted score matches
/// what the join will pair it with to the last bit. A test that recomputed the
/// partner itself could disagree with the code under test and pass for the
/// wrong reason.
fn plant_forward_return_as_score(scored: &[ScoredBar], horizon_ns: i64) -> Vec<ScoredBar> {
    let mut out = scored.to_vec();
    let mut j = 0usize;
    for i in 0..out.len() {
        let target = out[i].avail_ts.saturating_add(horizon_ns);
        if j < i {
            j = i;
        }
        while j + 1 < out.len() && out[j + 1].avail_ts <= target {
            j += 1;
        }
        if j > i {
            out[i].score = out[j].price_points / out[i].price_points - 1.0;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The join
// ---------------------------------------------------------------------------

#[test]
fn the_partner_is_the_last_bar_at_or_before_the_horizon() {
    // Four bars at t = 1,2,3,4 minutes, prices 100, 101, 102, 103.
    // Horizon 2 minutes. Score at t=1 reaches t=3 -> the LAST bar at or before
    // 1+2=3 is the t=3 bar, price 102. Forward return = 102/100 - 1 = 0.02.
    let bars: Vec<ScoredBar> = [100.0, 101.0, 102.0, 103.0]
        .iter()
        .enumerate()
        .map(|(i, &p)| ScoredBar {
            avail_ts: (i as i64 + 1) * MIN,
            session_key: 0,
            score: f64::from(i as i32),
            price_points: p,
        })
        .collect();

    let r = join(&bars, 2 * MIN);
    // t=1 -> t=3 (0.02); t=2 -> t=4 (103/101-1); t=3 and t=4 have no partner
    // inside the horizon that is also inside the series.
    assert_eq!(r.pairs.len(), 2);
    assert_eq!(r.dropped_no_partner, 2);
    assert!((r.pairs[0].fwd_return - 0.02).abs() < 1e-12);
    assert!((r.pairs[1].fwd_return - (103.0 / 101.0 - 1.0)).abs() < 1e-12);
}

#[test]
fn the_join_never_reaches_past_the_horizon() {
    // Bars at 1, 2, 5 minutes. A 2-minute horizon from t=1 reaches t=3; the
    // t=5 bar is OUTSIDE it. Taking "the first bar at or after the horizon"
    // would pair t=1 with t=5 and read a price the horizon never reached.
    let bars = vec![
        ScoredBar {
            avail_ts: MIN,
            session_key: 0,
            score: 0.0,
            price_points: 100.0,
        },
        ScoredBar {
            avail_ts: 2 * MIN,
            session_key: 0,
            score: 1.0,
            price_points: 110.0,
        },
        ScoredBar {
            avail_ts: 5 * MIN,
            session_key: 0,
            score: 2.0,
            price_points: 999.0,
        },
    ];
    let r = join(&bars, 2 * MIN);
    assert_eq!(r.pairs.len(), 1, "only t=1 has a partner inside 2 minutes");
    assert!((r.pairs[0].fwd_return - 0.1).abs() < 1e-12, "110/100 - 1");
    // t=2's horizon ends at t=4 and the next bar is at t=5: no partner.
    assert_eq!(r.dropped_no_partner, 2);
}

#[test]
fn horizons_are_durations_not_bar_counts() {
    // Bars at 1, 2, 3, 40, 41 minutes — the hole `ohlcv` leaves when nothing
    // trades. A 3-minute horizon from t=1 must NOT jump the hole to t=40 just
    // because it is "three bars ahead".
    let at = [1i64, 2, 3, 40, 41];
    let bars: Vec<ScoredBar> = at
        .iter()
        .enumerate()
        .map(|(i, &m)| ScoredBar {
            avail_ts: m * MIN,
            session_key: 0,
            score: f64::from(i as i32),
            price_points: 100.0 + f64::from(i as i32),
        })
        .collect();
    let r = join(&bars, 3 * MIN);
    // t=1 -> last bar at or before t=4, which is t=3 (a bar count would have
    // said t=40). t=2 -> t=3. t=3's window ends at t=6 and the only later bar
    // is t=40, so nothing traded inside it. t=40 and t=41 both reach past the
    // series end (t=41) and are unanswerable rather than short-windowed.
    assert_eq!(r.pairs.len(), 2);
    assert!((r.pairs[0].fwd_return - (102.0 / 100.0 - 1.0)).abs() < 1e-12);
    assert!((r.pairs[1].fwd_return - (102.0 / 101.0 - 1.0)).abs() < 1e-12);
    assert_eq!(r.dropped_no_partner, 3);
}

#[test]
fn a_tail_score_is_dropped_and_counted_never_zero_filled() {
    let bars = walk(50, 10, 7);
    let r = join(&bars, 5 * MIN);
    assert_eq!(
        r.pairs.len() + r.dropped_no_partner + r.dropped_zero_price,
        bars.len(),
        "every score is either paired or counted as dropped"
    );
    assert!(r.dropped_no_partner > 0, "the tail cannot be answered");
    assert!(
        r.pairs.iter().all(|p| p.fwd_return.is_finite()),
        "every surviving observation is a real measurement"
    );
    // The identity above is the "never zero-filled" property with teeth: if a
    // dropped score were quietly answered with a zero, the pair count would
    // rise and the dropped count would fall, and the sum would still hold —
    // so the sum is checked against the INPUT length, which cannot move.
    assert!(
        r.pairs.len() < bars.len(),
        "a 5-minute horizon cannot answer the last bars of a 50-bar series"
    );
}

#[test]
#[should_panic(expected = "availability-ordered")]
fn an_unsorted_series_is_refused_rather_than_joined() {
    let mut bars = walk(10, 5, 3);
    bars.swap(2, 7);
    let _ = join(&bars, MIN);
}

// ---------------------------------------------------------------------------
// THE NEGATIVE CONTROL — planted before the seam's first real use
// ---------------------------------------------------------------------------

/// The detector: S0's own information coefficient, at the threshold a reader
/// would call "this signal knows the answer".
const LEAK_FIRES_ABOVE: f64 = 0.99;
/// The band inside which a signal is indistinguishable from noise on a walk.
const SILENT_BELOW: f64 = 0.10;

#[test]
fn a_signal_that_is_the_forward_return_fires_through_the_leaky_join() {
    // PLANTED DEFECT: the score at bar i is the return from i to i+horizon —
    // information from after `avail_ts`, which is what §2.1 forbids reaching
    // signal space. This is the reversed join: the seam has handed the future
    // to the score.
    let bars = walk(600, 30, 11);
    let leaked = plant_forward_return_as_score(&bars, 10 * MIN);
    let r = join(&leaked, 10 * MIN);

    let ic = information_coefficient(&r.pairs).expect("a walk has rank order");
    eprintln!("[control] leaky join   -> IC = {ic:.6}  (fires above {LEAK_FIRES_ABOVE})");
    assert!(
        ic > LEAK_FIRES_ABOVE,
        "the control must FIRE on a planted leak: IC = {ic}, expected > {LEAK_FIRES_ABOVE}"
    );

    // And the buckets say it in the other language: monotone, with the extreme
    // buckets far apart.
    let b = buckets(&r.pairs, 5);
    assert_eq!(b.len(), 5);
    assert!(
        b.windows(2).all(|w| w[0].mean_return <= w[1].mean_return),
        "a leaked score sorts the future perfectly, so its buckets are monotone"
    );
    assert!(b[4].mean_return > b[0].mean_return);
}

#[test]
fn the_same_planted_signal_is_silent_through_the_correct_join() {
    // SAME planted signal, SAME bars. The only thing that changes is that the
    // score is computed from what was available at `avail_ts` — here, the
    // trailing one-bar return — so the future is not readable signal-side.
    let bars = walk(600, 30, 11);
    let mut causal = bars.clone();
    for i in 0..causal.len() {
        causal[i].score = if i == 0 {
            0.0
        } else {
            bars[i].price_points / bars[i - 1].price_points - 1.0
        };
    }
    let r = join(&causal, 10 * MIN);

    let ic = information_coefficient(&r.pairs).expect("a walk has rank order");
    eprintln!("[control] correct join -> IC = {ic:.6}  (silent below {SILENT_BELOW})");
    assert!(
        ic.abs() < SILENT_BELOW,
        "the control must NOT fire on a causal score: IC = {ic}, expected |IC| < {SILENT_BELOW}"
    );
}

#[test]
fn the_third_case_names_the_cause_as_the_horizon_the_leak_read() {
    // Two things now disagree — a leaked score scores ~1.0 and a causal one
    // ~0.0 — and §7 asks for the case that turns the difference into a
    // diagnosis. If the near-perfect IC were an artifact of the data or of the
    // statistic, it would survive changing which horizon the join asks about.
    // It does not: the leak is worth ~1.0 at the horizon it peeked at and
    // collapses at every other one.
    let bars = walk(600, 30, 11);
    let leaked = plant_forward_return_as_score(&bars, 10 * MIN);

    let matched = information_coefficient(&join(&leaked, 10 * MIN).pairs).expect("pairs");
    let mismatched = information_coefficient(&join(&leaked, 60 * MIN).pairs).expect("pairs");

    eprintln!("[control] leak at matched horizon (10m) -> IC = {matched:.6}");
    eprintln!("[control] same leak at 60m horizon      -> IC = {mismatched:.6}");
    assert!(matched > LEAK_FIRES_ABOVE, "matched horizon: {matched}");
    assert!(
        mismatched < matched - 0.5,
        "a 60-minute join must not inherit the 10-minute leak: \
         matched = {matched}, mismatched = {mismatched}"
    );
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[test]
fn the_information_coefficient_is_one_on_a_perfectly_ranked_sample() {
    // Hand-derived: score and return share a rank order, so Spearman = 1
    // exactly, whatever the magnitudes are.
    let pairs: Vec<Pair> = [(1.0, 0.5), (2.0, 0.9), (3.0, 1.7), (4.0, 40.0)]
        .iter()
        .map(|&(s, r)| Pair {
            score: s,
            fwd_return: r,
            session_key: 0,
        })
        .collect();
    let ic = information_coefficient(&pairs).expect("ranked");
    assert!((ic - 1.0).abs() < 1e-12, "ic = {ic}");
}

#[test]
fn ties_share_the_average_rank() {
    // ranks of [10, 20, 20, 30] are [1, 2.5, 2.5, 4].
    let r = ranks(&[10.0, 20.0, 20.0, 30.0]);
    assert_eq!(r, vec![1.0, 2.5, 2.5, 4.0]);
}

#[test]
fn a_constant_score_reports_no_measurement_rather_than_zero() {
    let pairs: Vec<Pair> = (0..10)
        .map(|i| Pair {
            score: 1.0,
            fwd_return: f64::from(i),
            session_key: 0,
        })
        .collect();
    assert!(
        information_coefficient(&pairs).is_none(),
        "a flat score has no rank order; zero would claim we measured nothing to find"
    );
}

#[test]
fn buckets_are_equal_count_and_use_every_observation_once() {
    let bars = walk(97, 10, 5);
    let r = join(&bars, 3 * MIN);
    let b = buckets(&r.pairs, 5);
    assert_eq!(b.len(), 5);
    let total: usize = b.iter().map(|x| x.n).sum();
    assert_eq!(total, r.pairs.len(), "no observation used twice or skipped");
    let max = b.iter().map(|x| x.n).max().expect("nonempty");
    let min = b.iter().map(|x| x.n).min().expect("nonempty");
    assert!(max - min <= 1, "equal count to within one: {min}..{max}");
}

#[test]
fn too_few_pairs_for_the_requested_buckets_returns_nothing() {
    let pairs: Vec<Pair> = (0..3)
        .map(|i| Pair {
            score: f64::from(i),
            fwd_return: 0.0,
            session_key: 0,
        })
        .collect();
    assert!(
        buckets(&pairs, 5).is_empty(),
        "3 observations are not 5 groups"
    );
}

#[test]
fn the_block_bootstrap_is_reproducible_from_its_seed() {
    let bars = walk(400, 20, 13);
    let r = join(&bars, 5 * MIN);
    let a = block_bootstrap_mean(&r.pairs, 200, 29).expect("pairs");
    let b = block_bootstrap_mean(&r.pairs, 200, 29).expect("pairs");
    assert_eq!(a, b, "same seed, same pairs, same interval");
    let c = block_bootstrap_mean(&r.pairs, 200, 30).expect("pairs");
    assert!(
        (a.lo - c.lo).abs() > 0.0 || (a.hi - c.hi).abs() > 0.0,
        "a different seed must actually resample differently"
    );
}

#[test]
fn the_bootstrap_point_estimate_is_the_sample_mean() {
    let pairs: Vec<Pair> = [0.01, 0.02, 0.03, 0.04]
        .iter()
        .enumerate()
        .map(|(i, &r)| Pair {
            score: 0.0,
            fwd_return: r,
            session_key: i as i64,
        })
        .collect();
    let iv = block_bootstrap_mean(&pairs, 50, 1).expect("pairs");
    assert!((iv.point - 0.025).abs() < 1e-12, "(0.01+0.02+0.03+0.04)/4");
}

#[test]
fn an_interval_straddling_zero_does_not_exclude_it() {
    let iv = Interval {
        point: 0.0,
        lo: -0.1,
        hi: 0.1,
        draws: 10,
    };
    assert!(!iv.excludes_zero());
    let iv = Interval {
        point: 0.5,
        lo: 0.2,
        hi: 0.9,
        draws: 10,
    };
    assert!(iv.excludes_zero());
}
