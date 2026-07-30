//! The permutation null's tests, and the two-sided control that makes its
//! verdict mean anything.
//!
//! Read the two controls first. `a_leak_that_refits_on_every_permutation_is_not_
//! extreme` is the detector firing; `a_real_edge_destroyed_by_shuffling_survives`
//! is the converse, and without it the first proves only that the harness kills
//! things (CLAUDE.md §7).

use super::*;

/// A deterministic walk in points — the D-0011 device, inlined, so the fixture
/// is bit-identical everywhere and needs no `rand`.
fn walk(n: usize, seed: u64) -> Vec<f64> {
    let mut state = seed;
    let mut ticks: i64 = 4_000 * 4;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        ticks = (ticks + i64::from((z % 5) as i32) - 2).max(1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "tick counts here are far below 2^53"
        )]
        let px = ticks as f64 * 0.25;
        out.push(px);
    }
    out
}

/// A series with **real long-range structure**: a persistent drift that
/// reverses every `regime` bars, accumulated so the path is continuous. A
/// trend-follower makes money on it; block-shuffling the returns scatters the
/// regimes and it does not.
///
/// Built as a **cumulative** sum rather than a closed-form level. The first
/// draft of this fixture wrote `4000 + dir * t * drift`, which is a sawtooth:
/// every regime flip jumps the level by twice the accumulated drift, and the
/// trend-follower lost money on it. The converse control caught that — which is
/// the argument for having a converse control, so it is recorded here rather
/// than quietly corrected.
fn trending(n: usize, regime: usize, seed: u64) -> Vec<f64> {
    let noise = walk(n + 1, seed);
    let mut out = Vec::with_capacity(n);
    let mut level = 4_000.0_f64;
    for i in 0..n {
        let dir = if (i / regime).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        // Drift per bar, plus a damped share of the walk's own step so the path
        // is not deterministic.
        let noise_step = (noise[i + 1] - noise[i]) * 0.30;
        level += dir * 0.05 + noise_step;
        out.push(level);
    }
    out
}

/// The metric both controls are scored on: a trend-follower's total point PnL.
///
/// Deliberately the simplest thing that reads long-range structure — position
/// is the sign of a fast-minus-slow moving-average difference, and PnL is the
/// sum of position times the next return. No engine, no fills: this file is
/// testing the null, not the backtester.
fn trend_follower_pnl(path: &[f64], fast: usize, slow: usize) -> Option<f64> {
    if path.len() < slow + 2 {
        return None;
    }
    let mut pnl = 0.0;
    let mut traded = false;
    for i in slow..path.len() - 1 {
        let f: f64 = path[i + 1 - fast..=i].iter().sum::<f64>() / fast as f64;
        let s: f64 = path[i + 1 - slow..=i].iter().sum::<f64>() / slow as f64;
        let pos = if f > s {
            1.0
        } else if f < s {
            -1.0
        } else {
            0.0
        };
        if pos != 0.0 {
            traded = true;
        }
        pnl += pos * (path[i + 1] - path[i]);
    }
    traded.then_some(pnl)
}

/// A **leak**: standardize against the whole path, then trade the deviation.
/// The same shape as `controls::LeakyZScore` — and, critically, it re-fits on
/// whatever series it is handed.
fn leaky_zscore_pnl(path: &[f64]) -> Option<f64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "series lengths here are far below 2^53"
    )]
    let n = path.len() as f64;
    let mean = path.iter().sum::<f64>() / n;
    let var = path.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / n;
    let sd = var.sqrt();
    if sd == 0.0 {
        return None;
    }
    let mut pnl = 0.0;
    for i in 0..path.len() - 1 {
        let z = (path[i] - mean) / sd;
        // Below the mean, go long; above it, go short. It knows where the
        // series will average, which is the whole defect.
        let pos = if z < -0.5 {
            1.0
        } else if z > 0.5 {
            -1.0
        } else {
            0.0
        };
        pnl += pos * (path[i + 1] - path[i]);
    }
    Some(pnl)
}

// ---------------------------------------------------------------------------
// Mechanics
// ---------------------------------------------------------------------------

#[test]
fn a_block_permutation_preserves_the_multiset_of_returns() {
    let closes = walk(200, 5);
    let returns: Vec<f64> = closes.windows(2).map(|w| w[1] - w[0]).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let permuted = permute_blocks(&returns, 10, &mut rng);

    assert_eq!(
        permuted.len(),
        returns.len(),
        "no return is lost or invented"
    );
    let mut a: Vec<i64> = returns.iter().map(|r| (r * 1e6).round() as i64).collect();
    let mut b: Vec<i64> = permuted.iter().map(|r| (r * 1e6).round() as i64).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(
        a, b,
        "the permutation moves returns, it does not change them"
    );
}

#[test]
fn rebuilding_an_unpermuted_path_reproduces_the_original_exactly() {
    // The third case that makes the two controls comparable: if the rebuild
    // itself distorted the series, "the null differs from the observed" would
    // be true for a reason that has nothing to do with shuffling.
    let closes = walk(500, 11);
    let returns: Vec<f64> = closes.windows(2).map(|w| w[1] - w[0]).collect();
    let rebuilt = rebuild_path(closes[0], &returns);
    assert_eq!(rebuilt.len(), closes.len());
    for (a, b) in closes.iter().zip(rebuilt.iter()) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }
}

#[test]
fn a_block_length_of_one_is_an_iid_shuffle_and_the_whole_series_moves() {
    let closes = walk(300, 3);
    let returns: Vec<f64> = closes.windows(2).map(|w| w[1] - w[0]).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let permuted = permute_blocks(&returns, 1, &mut rng);
    let same = returns
        .iter()
        .zip(permuted.iter())
        .filter(|(a, b)| (**a - **b).abs() < 1e-12)
        .count();
    assert!(
        same < returns.len(),
        "an L=1 shuffle that returned the identity would make every null draw \
         equal the observation"
    );
}

#[test]
#[should_panic(expected = "block length is at least 1")]
fn a_zero_block_length_is_refused_rather_than_promoted_to_one() {
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let _ = permute_blocks(&[1.0, 2.0, 3.0], 0, &mut rng);
}

#[test]
fn the_null_is_reproducible_from_its_seed() {
    let closes = walk(400, 13);
    let spec = PermutationSpec {
        block_len: 20,
        draws: 40,
        seed: 29,
    };
    let a = run_permutation_null(&closes, 0.0, spec, |p| trend_follower_pnl(p, 5, 20));
    let b = run_permutation_null(&closes, 0.0, spec, |p| trend_follower_pnl(p, 5, 20));
    assert_eq!(a, b, "same seed, same series, same null");

    let other = PermutationSpec { seed: 30, ..spec };
    let c = run_permutation_null(&closes, 0.0, other, |p| trend_follower_pnl(p, 5, 20));
    assert_ne!(a.null, c.null, "a different seed must resample differently");
}

#[test]
fn an_unevaluable_draw_is_counted_rather_than_folded_in_as_zero() {
    let closes = walk(100, 2);
    let spec = PermutationSpec {
        block_len: 10,
        draws: 5,
        seed: 1,
    };
    // An evaluator that never has an opinion.
    let r = run_permutation_null(&closes, 1.0, spec, |_| None);
    assert_eq!(r.unevaluable, 5);
    assert!(r.null.is_empty());
    assert!(r.p_value().is_none(), "an absent null has no p-value");
    assert!(
        !r.survives(0.05),
        "an absent null FAILS its criterion rather than passing it (D-0075)"
    );
}

#[test]
fn the_p_value_carries_the_plus_one_correction() {
    // Hand-derived: 4 null draws, none at or above the observation.
    // p = (1 + 0) / (1 + 4) = 0.2 — never 0, because 4 resamples cannot
    // support a stronger claim than one in five.
    let r = PermutationResult {
        observed: 10.0,
        null: vec![1.0, 2.0, 3.0, 4.0],
        unevaluable: 0,
        block_len: 5,
    };
    let p = r.p_value().expect("has a null");
    assert!((p - 0.2).abs() < 1e-12, "p = {p}");

    // And with two of four at or above: (1 + 2) / (1 + 4) = 0.6.
    let r = PermutationResult {
        observed: 3.0,
        null: vec![1.0, 2.0, 3.0, 4.0],
        unevaluable: 0,
        block_len: 5,
    };
    let p = r.p_value().expect("has a null");
    assert!((p - 0.6).abs() < 1e-12, "p = {p}");
}

// ---------------------------------------------------------------------------
// THE TWO-SIDED CONTROL — the converse first, because without it the leak
// result proves only that the harness kills things
// ---------------------------------------------------------------------------

/// Pre-registered significance level for both controls below.
const ALPHA: f64 = 0.05;

#[test]
fn a_real_edge_destroyed_by_shuffling_survives() {
    // CONVERSE CONTROL. A series with genuine long-range structure — regimes
    // that persist for 500 bars — and a trend-follower that reads it. Block
    // shuffling at L = 20 moves whole regimes apart, so the null loses the
    // structure and the observed result should be extreme.
    let closes = trending(4_000, 500, 17);
    let observed = trend_follower_pnl(&closes, 10, 50).expect("the real run trades");
    let spec = PermutationSpec {
        block_len: 20,
        draws: 200,
        seed: 4,
    };
    let r = run_permutation_null(&closes, observed, spec, |p| trend_follower_pnl(p, 10, 50));
    let p = r.p_value().expect("a null was built");

    eprintln!(
        "[converse] real edge: observed = {observed:.2} pt, null median = {:.2} pt, p = {p:.4}",
        r.null[r.null.len() / 2]
    );
    assert!(
        r.survives(ALPHA),
        "a real edge must SURVIVE the permutation null: p = {p}, alpha = {ALPHA}. \
         A detector that kills everything is not a detector."
    );
}

#[test]
fn a_leak_that_refits_on_every_permutation_is_not_extreme() {
    // THE DETECTOR. The leak standardizes against whatever series it is given,
    // so it re-establishes itself on every permutation: the observed result is
    // an ordinary draw from its own null.
    let closes = walk(4_000, 29);
    let observed = leaky_zscore_pnl(&closes).expect("the leak trades");
    let spec = PermutationSpec {
        block_len: 20,
        draws: 200,
        seed: 4,
    };
    let r = run_permutation_null(&closes, observed, spec, leaky_zscore_pnl);
    let p = r.p_value().expect("a null was built");

    eprintln!(
        "[detector] leak:      observed = {observed:.2} pt, null median = {:.2} pt, p = {p:.4}",
        r.null[r.null.len() / 2]
    );
    assert!(
        !r.survives(ALPHA),
        "the leak must NOT survive: p = {p}, alpha = {ALPHA}. Its edge reproduces \
         on every permutation, so nothing about the real run was extreme."
    );
}

#[test]
fn the_third_case_names_the_cause_as_the_long_range_ordering() {
    // The two controls disagree; §7 asks for the case that turns the difference
    // into a diagnosis. If the real edge's significance came from the harness
    // rather than from structure, it would survive on a structureless series
    // too. It does not: the same trend-follower on a plain random walk is an
    // ordinary draw from its own null.
    let closes = walk(4_000, 17);
    let observed = trend_follower_pnl(&closes, 10, 50).expect("trades");
    let spec = PermutationSpec {
        block_len: 20,
        draws: 200,
        seed: 4,
    };
    let r = run_permutation_null(&closes, observed, spec, |p| trend_follower_pnl(p, 10, 50));
    let p = r.p_value().expect("a null was built");
    eprintln!("[third]    same strategy, no structure: p = {p:.4}");
    assert!(
        !r.survives(ALPHA),
        "the same strategy on a structureless series must not survive: p = {p}. \
         Otherwise the converse control was measuring the harness, not the edge."
    );
}

/// The harness's own **pinned determinism hash** (D-0087).
///
/// Same seed, same series, same block length, same draws ⇒ the same null, on
/// any machine (§2.2). Hashed rather than compared element-wise so a drift
/// anywhere in the distribution fails, not just at the percentile a p-value
/// happens to read.
///
/// Inputs, so the number can be re-derived: `walk(4_000, 29)` — the inlined
/// SplitMix64 walk from 4000.00 points in quarter-point ticks — evaluated with
/// `leaky_zscore_pnl`, `block_len = 20`, `draws = 200`, `seed = 4`.
#[test]
fn the_permutation_null_is_pinned() {
    let closes = walk(4_000, 29);
    let observed = leaky_zscore_pnl(&closes).expect("trades");
    let spec = PermutationSpec {
        block_len: 20,
        draws: 200,
        seed: 4,
    };
    let r = run_permutation_null(&closes, observed, spec, leaky_zscore_pnl);

    let mut h = blake3::Hasher::new();
    h.update(&observed.to_le_bytes());
    h.update(&(r.block_len as u64).to_le_bytes());
    h.update(&(r.unevaluable as u64).to_le_bytes());
    for v in &r.null {
        h.update(&v.to_le_bytes());
    }
    let digest = h.finalize().to_hex()[..16].to_owned();

    eprintln!("[pinned] permutation null hash = {digest}");
    assert_eq!(
        digest, "9fe41f6f5b3653e7",
        "the permutation null moved. Same seed and same series must give the same \
         distribution; if this changed on purpose, re-derive it and say so in a decision entry."
    );
}
