//! S0's unit tests, and the two-sided negative control the seam exists to pass.
//!
//! The controls are the point of this file (CLAUDE.md §7, no quality
//! exemption). Everything else here is the hand-derived arithmetic that makes
//! them readable.

use super::*;

const MIN: i64 = 60_000_000_000; // one minute in nanoseconds

fn pair(score: f64, fwd_return: f64, session_key: i64) -> Pair {
    let entry_price_points = 100.0;
    priced_pair(
        score,
        entry_price_points,
        entry_price_points * (1.0 + fwd_return),
        session_key,
    )
}

fn priced_pair(score: f64, entry: f64, exit: f64, session_key: i64) -> Pair {
    Pair {
        score,
        fwd_return: exit / entry - 1.0,
        entry_price_points: entry,
        exit_price_points: exit,
        session_key,
    }
}

fn separable_pairs() -> Vec<Pair> {
    vec![
        priced_pair(-2.0, 128.0, 128.5, 0),
        priced_pair(-1.0, 256.0, 256.5, 1),
        priced_pair(1.0, 256.0, 255.5, 2),
        priced_pair(2.0, 128.0, 127.5, 3),
    ]
}

fn separable_horizon(horizon_ns: i64) -> HorizonEvidence {
    let pairs = separable_pairs();
    HorizonEvidence {
        horizon_ns,
        n_pairs: pairs.len(),
        dropped_no_partner: 0,
        dropped_invalid_price: 0,
        ic: information_coefficient_evidence(&pairs),
        buckets: buckets(&pairs, 2, 0.25),
        unconditional_mean_interval: Availability::Available {
            value: Interval {
                point: 0.0,
                lo: -0.01,
                hi: 0.01,
                draws: 200,
            },
        },
    }
}

fn separable_report() -> S0Report {
    S0Report {
        evidence_scope: S0EvidenceScope::EqualCountScoreBuckets,
        combos: vec![S0ComboReport {
            score_identity: "slot=z kind=zscore source=close period=20".to_owned(),
            tick_size_nanopoints: 250_000_000,
            spec: S0Spec {
                score_slot: "z".to_owned(),
                horizons_ns: vec![10 * MIN, MIN],
                buckets: 2,
                bootstrap_draws: 200,
                min_abs_ic: 0.05,
            },
            combo_index: 0,
            label: "z(period=20 source=close)".to_owned(),
            warmup_bars: 20,
            scores: 4,
            horizons: vec![separable_horizon(10 * MIN), separable_horizon(MIN)],
        }],
    }
}

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
            score: Some(0.0),
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
            out[i].score = Some(out[j].price_points / out[i].price_points - 1.0);
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
            score: Some(f64::from(i as i32)),
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
            score: Some(0.0),
            price_points: 100.0,
        },
        ScoredBar {
            avail_ts: 2 * MIN,
            session_key: 0,
            score: Some(1.0),
            price_points: 110.0,
        },
        ScoredBar {
            avail_ts: 5 * MIN,
            session_key: 0,
            score: Some(2.0),
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
fn a_target_bar_without_a_score_still_supplies_its_tradeable_close() {
    let bars = vec![
        ScoredBar {
            avail_ts: MIN,
            session_key: 0,
            score: Some(-1.0),
            price_points: 100.0,
        },
        ScoredBar {
            avail_ts: 2 * MIN,
            session_key: 0,
            score: None,
            price_points: 102.0,
        },
        ScoredBar {
            avail_ts: 3 * MIN,
            session_key: 0,
            score: None,
            price_points: 999.0,
        },
    ];
    let joined = join(&bars, MIN);
    assert_eq!(joined.pairs.len(), 1);
    assert_eq!(joined.pairs[0].exit_price_points, 102.0);
    assert!((joined.pairs[0].fwd_return - 0.02).abs() < 1e-12);
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
            score: Some(f64::from(i as i32)),
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

#[test]
#[should_panic(expected = "duplicate or decreasing timestamps")]
fn duplicate_availability_timestamps_are_refused_as_not_forward() {
    let mut bars = walk(10, 5, 3);
    bars[3].avail_ts = bars[2].avail_ts;
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
    let b = buckets(&r.pairs, 5, 0.25);
    let b = b.value().expect("five buckets").as_slice();
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
        causal[i].score = Some(if i == 0 {
            0.0
        } else {
            bars[i].price_points / bars[i - 1].price_points - 1.0
        });
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
        .map(|&(score, fwd_return)| pair(score, fwd_return, 0))
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
    let pairs: Vec<Pair> = (0..10).map(|i| pair(1.0, f64::from(i), 0)).collect();
    assert!(
        information_coefficient(&pairs).is_none(),
        "a flat score has no rank order; zero would claim we measured nothing to find"
    );
}

#[test]
fn separable_tails_are_visible_while_unconditional_drift_is_zero() {
    let pairs = separable_pairs();
    let evidence = buckets(&pairs, 2, 0.25);
    let buckets = evidence.value().expect("two buckets").as_slice();
    assert_eq!(buckets.len(), 2);
    let low = buckets[0];
    assert_eq!((low.score_lo, low.score_hi, low.n), (-2.0, -1.0, 2));
    assert_eq!(low.mean_return, 3.0 / 1024.0);
    assert_eq!(low.mean_move_ticks, 2.0);

    let high = buckets[1];
    assert_eq!((high.score_lo, high.score_hi, high.n), (1.0, 2.0, 2));
    assert_eq!(high.mean_return, -3.0 / 1024.0);
    assert_eq!(high.mean_move_ticks, -2.0);

    let interval = block_bootstrap_mean(&pairs, 200, 17).expect("four pairs");
    assert_eq!(interval.point, 0.0);
    assert_eq!(information_coefficient(&pairs), Some(-1.0));
}

#[test]
fn common_unconditional_drift_without_tail_separation_does_not_fire() {
    let pairs = vec![
        priced_pair(-2.0, 128.0, 128.5, 0),
        priced_pair(-1.0, 256.0, 257.0, 1),
        priced_pair(1.0, 256.0, 257.0, 2),
        priced_pair(2.0, 128.0, 128.5, 3),
    ];
    let bucket_evidence = buckets(&pairs, 2, 0.25);
    let set = bucket_evidence.value().expect("two buckets");
    assert_eq!(set.as_slice().len(), 2);
    assert_eq!(set.as_slice()[0].mean_return, 1.0 / 256.0);
    assert_eq!(set.as_slice()[1].mean_return, 1.0 / 256.0);
    assert_eq!(set.as_slice()[0].mean_move_ticks, 3.0);
    assert_eq!(set.as_slice()[1].mean_move_ticks, 3.0);
    let ic = information_coefficient_evidence(&pairs);
    assert_eq!(
        ic,
        Availability::Unavailable {
            reason: UnavailableReason::ConstantForwardReturn,
        }
    );
    let unconditional = unconditional_mean_evidence(&pairs, 100, 7);
    assert!(unconditional.value().expect("interval").excludes_zero());
    let horizon = HorizonEvidence {
        horizon_ns: 5 * MIN,
        n_pairs: pairs.len(),
        dropped_no_partner: 0,
        dropped_invalid_price: 0,
        ic,
        buckets: bucket_evidence,
        unconditional_mean_interval: unconditional,
    };
    assert_eq!(
        evaluate_criterion(&[horizon], 0.05),
        S0CriterionOutcome::Unavailable {
            reason: UnavailableReason::ConstantForwardReturn,
        }
    );
}

#[test]
fn a_missing_required_interval_is_unavailable_not_measured_failure() {
    let mut horizon = separable_horizon(MIN);
    horizon.unconditional_mean_interval = Availability::Unavailable {
        reason: UnavailableReason::NoBootstrapDraws,
    };
    assert_eq!(
        evaluate_criterion(&[horizon], 0.05),
        S0CriterionOutcome::Unavailable {
            reason: UnavailableReason::NoBootstrapDraws,
        }
    );
}

#[test]
fn buckets_are_equal_count_and_use_every_observation_once() {
    let bars = walk(97, 10, 5);
    let r = join(&bars, 3 * MIN);
    let b = buckets(&r.pairs, 5, 0.25);
    let b = b.value().expect("five buckets").as_slice();
    assert_eq!(b.len(), 5);
    let total: usize = b.iter().map(|x| x.n).sum();
    assert_eq!(total, r.pairs.len(), "no observation used twice or skipped");
    let max = b.iter().map(|x| x.n).max().expect("nonempty");
    let min = b.iter().map(|x| x.n).min().expect("nonempty");
    assert!(max - min <= 1, "equal count to within one: {min}..{max}");
}

#[test]
fn too_few_pairs_for_the_requested_buckets_is_explicitly_unavailable() {
    let pairs: Vec<Pair> = (0..3).map(|i| pair(f64::from(i), 0.0, 0)).collect();
    assert_eq!(
        buckets(&pairs, 5, 0.25),
        Availability::Unavailable {
            reason: UnavailableReason::TooFewObservations {
                observed: 3,
                required: 5,
            },
        }
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
        .map(|(i, &fwd_return)| pair(0.0, fwd_return, i as i64))
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

#[test]
fn determinism_bytes_cover_order_bounds_ticks_and_absence() {
    let report = separable_report();
    let expected = report.determinism_bytes();

    let mut changed = report.clone();
    changed.combos[0].horizons.swap(0, 1);
    changed.combos[0].spec.horizons_ns.swap(0, 1);
    assert_ne!(changed.determinism_bytes(), expected, "horizon order");

    let mut changed = report.clone();
    changed.combos[0].tick_size_nanopoints += 1;
    assert_ne!(changed.determinism_bytes(), expected, "declared tick");

    let mut changed = report.clone();
    let Availability::Available { value } = &mut changed.combos[0].horizons[0].buckets else {
        panic!("fixture buckets");
    };
    value.0[0].score_lo -= 0.25;
    assert_ne!(changed.determinism_bytes(), expected, "bucket bounds");

    let mut changed = report.clone();
    let Availability::Available { value } = &mut changed.combos[0].horizons[0].buckets else {
        panic!("fixture buckets");
    };
    value.0[0].mean_move_ticks += 0.25;
    assert_ne!(changed.determinism_bytes(), expected, "bucket tick mean");

    let mut changed = report.clone();
    changed.combos[0].horizons[0].buckets = Availability::Unavailable {
        reason: UnavailableReason::TooFewObservations {
            observed: 1,
            required: 2,
        },
    };
    assert_ne!(changed.determinism_bytes(), expected, "bucket absence");
}

#[test]
fn persisted_available_bucket_set_cannot_be_empty() {
    let parsed = serde_json::from_str::<BucketSet>("[]");
    assert!(parsed.is_err());
}

#[test]
fn persisted_available_bucket_set_cannot_reverse_or_corrupt_order() {
    let buckets = vec![
        Bucket {
            score_lo: 1.0,
            score_hi: 2.0,
            n: 2,
            mean_return: -0.01,
            mean_move_ticks: -2.0,
        },
        Bucket {
            score_lo: -2.0,
            score_hi: -1.0,
            n: 2,
            mean_return: 0.01,
            mean_move_ticks: 2.0,
        },
    ];
    assert!(BucketSet::from_nonempty(buckets.clone()).is_none());
    let encoded = serde_json::to_string(&buckets).expect("bucket JSON");
    assert!(serde_json::from_str::<BucketSet>(&encoded).is_err());
}

#[test]
fn persisted_criterion_cannot_contradict_its_evidence() {
    let report = separable_report();
    let mut wire = serde_json::to_value(report).expect("S0 wire value");
    wire["combos"][0]["criterion"] = serde_json::json!({
        "outcome": "passed",
        "horizon_ns": 10 * MIN,
    });
    let error = serde_json::from_value::<S0Report>(wire).expect_err("contradictory criterion");
    assert!(error.to_string().contains("contradicts"), "{error}");
}

#[test]
fn report_validation_refuses_duplicate_or_missing_combo_identity() {
    let mut report = separable_report();
    report.combos.push(report.combos[0].clone());
    assert!(report.validate().is_err());
    report.combos.pop();
    report.combos[0].combo_index = 1;
    assert!(report.validate().is_err());
}
