//! The deflated Sharpe's controls.
//!
//! The order below is the order they must be read in, and it is not cosmetic.
//! **The converse comes first**: until a planted REAL edge with a known effect
//! size survives deflation at its true trial count, a deflation result means
//! nothing — a statistic that condemns everything is not a detector, it is a
//! constant. Only then is the detector control worth anything: N no-edge
//! strategies, the best selected in-sample, and the DSR condemning the
//! selection.

use super::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

/// One standard-normal draw by Box–Muller from a seeded ChaCha stream.
///
/// Hand-rolled rather than pulled from `rand_distr`: §6 blesses `rand_chacha`
/// and nothing else, and two `u64` draws through a fixed transform is
/// bit-reproducible on any machine, which is what §2.2 actually needs.
fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1 = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let u2 = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let u1 = u1.max(1e-12);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// `n` returns with per-observation mean `edge` and unit volatility.
fn series(rng: &mut ChaCha8Rng, n: usize, edge: f64) -> Vec<f64> {
    (0..n).map(|_| edge + normal(rng)).collect()
}

// ---------------------------------------------------------------------------
// CONVERSE CONTROL — runs first, and everything below depends on it.
// ---------------------------------------------------------------------------

/// **A planted REAL edge survives deflation at its true trial count.**
///
/// The effect size is known by construction: 2,000 observations with a
/// per-observation mean of 0.12 and unit vol is a true Sharpe of 0.12, which
/// over 2,000 observations is roughly 5.4 standard errors from zero. A search
/// of 24 trials cannot manufacture that, so the DSR must be decisively high.
///
/// Without this test the detector control below is worthless: a `deflated_sharpe`
/// that returned 0.0 unconditionally would pass it perfectly.
#[test]
fn converse_a_planted_real_edge_survives_deflation_at_its_true_trial_count() {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let returns = series(&mut rng, 2_000, 0.12);
    let (sharpe, skew, kurtosis) = moments(&returns).expect("a real series has moments");

    let d = deflated_sharpe(DeflationInputs {
        observed_sharpe: sharpe,
        skew,
        kurtosis,
        n_observations: returns.len(),
        n_trials: 24,
        trial_sharpe_dispersion: None,
    })
    .expect("well-formed inputs");

    eprintln!(
        "[converse] observed {:.4}  benchmark {:.4}  se {:.5}  DSR {:.6}",
        d.observed_sharpe, d.expected_max_sharpe, d.standard_error, d.dsr
    );
    assert!(
        d.beats_selection_benchmark(),
        "a real edge must clear the selection benchmark: {d:?}"
    );
    assert!(
        d.dsr > 0.99,
        "a 5-sigma edge searched 24 times must survive; DSR was {:.6}",
        d.dsr
    );
}

/// The converse again, at the boundary that matters: the SAME edge must still
/// survive when the trial count is honest but large.
///
/// This is what stops the correction being so aggressive that no real strategy
/// could ever pass — an over-deflating statistic is as useless as an
/// under-deflating one, it just fails in the flattering-to-nobody direction.
#[test]
fn converse_a_real_edge_survives_even_a_large_honest_trial_count() {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let returns = series(&mut rng, 2_000, 0.12);
    let (sharpe, skew, kurtosis) = moments(&returns).expect("moments");

    let d = deflated_sharpe(DeflationInputs {
        observed_sharpe: sharpe,
        skew,
        kurtosis,
        n_observations: returns.len(),
        n_trials: 3_840, // 240 combos × 16 accounts, Block E's worst case.
        trial_sharpe_dispersion: None,
    })
    .expect("inputs");
    eprintln!(
        "[converse-large] benchmark {:.4}  DSR {:.6}",
        d.expected_max_sharpe, d.dsr
    );
    assert!(
        d.dsr > 0.95,
        "a real edge must survive an honest 3,840-trial correction; DSR {:.6}",
        d.dsr
    );
}

// ---------------------------------------------------------------------------
// THE DETECTOR CONTROL — the lucky best-of-N must die.
// ---------------------------------------------------------------------------

/// **N no-edge strategies, best selected in-sample, condemned by the DSR.**
///
/// This is the failure mode the whole statistic exists for. Forty zero-edge
/// series are generated, the best Sharpe among them is selected exactly as a
/// grid search would, and that Sharpe is then scored against a trial count of
/// 40. The naive reading of the winner is a positive Sharpe with a
/// respectable-looking t-statistic; the deflated reading must be that it is
/// what forty coin flips produce.
///
/// Watched firing: the winner's naive Sharpe is printed beside its DSR, so the
/// gap between "looks like an edge" and "is an edge" is visible in the output
/// rather than asserted in the abstract.
#[test]
fn the_lucky_best_of_n_no_edge_strategies_is_condemned_by_deflation() {
    const N: usize = 40;
    const T: usize = 500;
    let mut rng = ChaCha8Rng::seed_from_u64(29);

    let mut best: Option<(f64, f64, f64)> = None;
    for _ in 0..N {
        let returns = series(&mut rng, T, 0.0); // NO edge, by construction.
        let m = moments(&returns).expect("moments");
        if best.is_none_or(|b| m.0 > b.0) {
            best = Some(m);
        }
    }
    let (sharpe, skew, kurtosis) = best.expect("N > 0");

    // The naive reading: selected without its trial count, this looks real.
    let naive_t = sharpe * ((T - 1) as f64).sqrt();
    let d = deflated_sharpe(DeflationInputs {
        observed_sharpe: sharpe,
        skew,
        kurtosis,
        n_observations: T,
        n_trials: N,
        trial_sharpe_dispersion: None,
    })
    .expect("inputs");

    eprintln!(
        "[detector] best-of-{N} naive sharpe {sharpe:.4} (t = {naive_t:.2})  \
         benchmark {:.4}  DSR {:.6}",
        d.expected_max_sharpe, d.dsr
    );
    assert!(
        naive_t > 1.96,
        "the fixture is only interesting if the winner LOOKS significant naively; t was {naive_t:.2}"
    );
    assert!(
        d.dsr < 0.95,
        "the best of {N} zero-edge searches must not survive deflation; DSR was {:.6}",
        d.dsr
    );

    // NOT asserted: that the winner falls below the benchmark. The expected
    // maximum is a MEAN, so the realised maximum of N zero-edge trials lands
    // above it roughly half the time by construction — here 0.1068 against
    // 0.0990. That is why the verdict is the DSR, which divides the gap by the
    // Sharpe's standard error, and not the point comparison. An earlier draft
    // asserted the point comparison and it failed on exactly this fixture; the
    // assertion was wrong, not the statistic.
    eprintln!(
        "[detector] clears the point benchmark: {} (immaterial — see comment)",
        d.beats_selection_benchmark()
    );
}

/// The selection benchmark must GROW with the trial count.
///
/// The single property the multiple-testing correction rests on. A benchmark
/// that ignored `n_trials` would leave the DSR a plain significance test with
/// extra arithmetic.
#[test]
fn the_selection_benchmark_grows_with_the_trial_count() {
    let one = expected_max_z(1);
    let ten = expected_max_z(10);
    let many = expected_max_z(240);
    let huge = expected_max_z(3_840);
    eprintln!("[benchmark] 1={one:.4} 10={ten:.4} 240={many:.4} 3840={huge:.4}");

    assert_eq!(one, 0.0, "one trial is no selection at all");
    assert!(ten > 0.0 && many > ten && huge > many, "must be increasing");
    // A sanity anchor against the paper's shape rather than a magic number:
    // the maximum of 240 standard normals lands near 2.9 standard errors.
    assert!(
        (2.5..3.3).contains(&many),
        "240 trials should expect ~2.9 se, got {many:.4}"
    );
}

/// **Trial-count sensitivity, watched firing** (plan Block B, control 1).
///
/// The same run scored at 1 trial and at 24 must give materially different
/// answers. A deflated Sharpe that ignores its denominator is decoration.
///
/// **The 1-vs-24 pair alone is not enough, and this was measured rather than
/// reasoned.** Block B planted an `expected_max_z` that ignored `n_trials`
/// entirely — hardcoding `n = 2` — and this control **survived it**, because
/// `n_trials <= 1` short-circuits to a zero benchmark before the mutated line
/// is reached. The pair therefore compared "no selection at all" against "some
/// selection" and would pass for any implementation that distinguished only
/// those two states. The 2-vs-24 pair below closes it: both operands are above
/// the short-circuit, so nothing but the trial count itself can separate them.
/// Four sibling controls did catch that mutation; this one is the reason the
/// gap is worth stating rather than quietly patching.
#[test]
fn the_same_run_scored_at_one_and_twenty_four_trials_differs_materially() {
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    let returns = series(&mut rng, 400, 0.05);
    let (sharpe, skew, kurtosis) = moments(&returns).expect("moments");
    let at = |n| {
        deflated_sharpe(DeflationInputs {
            observed_sharpe: sharpe,
            skew,
            kurtosis,
            n_observations: returns.len(),
            n_trials: n,
            trial_sharpe_dispersion: None,
        })
        .expect("inputs")
    };
    let one = at(1);
    let twenty_four = at(24);
    eprintln!(
        "[sensitivity] DSR@1 = {:.6}  DSR@24 = {:.6}",
        one.dsr, twenty_four.dsr
    );
    assert!(
        one.dsr > twenty_four.dsr,
        "more trials must deflate, not inflate"
    );
    assert!(
        one.dsr - twenty_four.dsr > 0.05,
        "the denominator must MOVE the number: {:.6} vs {:.6}",
        one.dsr,
        twenty_four.dsr
    );

    // Both operands above the `n_trials <= 1` short-circuit, so the only thing
    // that can separate them is the trial count arriving at the arithmetic.
    let two = at(2);
    eprintln!(
        "[sensitivity] DSR@2 = {:.6}  DSR@24 = {:.6}",
        two.dsr, twenty_four.dsr
    );
    assert!(
        two.dsr > twenty_four.dsr,
        "twelve times the search must deflate further: {:.6} vs {:.6}",
        two.dsr,
        twenty_four.dsr
    );
    assert!(
        two.expected_max_sharpe < twenty_four.expected_max_sharpe,
        "the selection benchmark must rise with the trial count: {:.6} vs {:.6}",
        two.expected_max_sharpe,
        twenty_four.expected_max_sharpe
    );
}

/// Skew and kurtosis must move the standard error in the correct directions.
///
/// Negative skew widens the error bar: a strategy that earns steadily and loses
/// in one day is less certain than its Sharpe implies. A sign error here would
/// flatter exactly the strategies most likely to be self-deception, and would
/// be invisible in every other test on this page.
#[test]
fn negative_skew_and_fat_tails_widen_the_error_bar() {
    let base = DeflationInputs {
        observed_sharpe: 0.10,
        skew: 0.0,
        kurtosis: 3.0,
        n_observations: 1_000,
        n_trials: 10,
        trial_sharpe_dispersion: None,
    };
    let normal = deflated_sharpe(base).expect("inputs");
    let skewed = deflated_sharpe(DeflationInputs { skew: -1.5, ..base }).expect("inputs");
    let fat = deflated_sharpe(DeflationInputs {
        kurtosis: 12.0,
        ..base
    })
    .expect("inputs");

    assert!(
        skewed.standard_error > normal.standard_error,
        "negative skew must widen: {:.6} vs {:.6}",
        skewed.standard_error,
        normal.standard_error
    );
    assert!(
        fat.standard_error > normal.standard_error,
        "fat tails must widen: {:.6} vs {:.6}",
        fat.standard_error,
        normal.standard_error
    );
    assert!(
        skewed.dsr < normal.dsr && fat.dsr < normal.dsr,
        "a wider error bar must lower confidence, never raise it"
    );
}

/// Inputs that cannot support the statistic return `None`, never a zero.
///
/// A zero DSR renders on a scorecard as "certainly worthless", which is a
/// verdict. "We could not compute this" is not — the D-0080 rule.
#[test]
fn unsupportable_inputs_are_no_opinion_rather_than_a_zero() {
    let ok = DeflationInputs {
        observed_sharpe: 0.1,
        skew: 0.0,
        kurtosis: 3.0,
        n_observations: 100,
        n_trials: 4,
        trial_sharpe_dispersion: None,
    };
    assert!(deflated_sharpe(ok).is_some());
    assert!(
        deflated_sharpe(DeflationInputs {
            n_observations: 1,
            ..ok
        })
        .is_none(),
        "one observation has no sampling distribution"
    );
    assert!(
        deflated_sharpe(DeflationInputs {
            observed_sharpe: f64::NAN,
            ..ok
        })
        .is_none()
    );
    // A variance correction driven negative by extreme skew: no opinion.
    assert!(
        deflated_sharpe(DeflationInputs {
            observed_sharpe: 2.0,
            skew: 10.0,
            ..ok
        })
        .is_none()
    );
    assert!(moments(&[1.0]).is_none(), "one point has no moments");
    assert!(
        moments(&[2.0, 2.0, 2.0]).is_none(),
        "a flat series has no Sharpe, and 0.0 would be a different claim"
    );
}

// ---------------------------------------------------------------------------
// The registry is the ONLY source of a trial count (D-0083).
// ---------------------------------------------------------------------------

/// **The void must move it** (plan Block B, control 2).
///
/// Voiding a trial lowers the denominator, so the deflated Sharpe must RISE.
/// This is what proves D-0083's exclusion is real rather than cosmetic, and it
/// lives here rather than in the registry because here is where the number is
/// consumed.
#[test]
fn voiding_a_trial_raises_the_deflated_sharpe() {
    use crate::registry::Registry;
    let dir = std::env::temp_dir().join("crucible-deflated-void");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    // `Registry::open` takes the JSONL file, not its directory.
    let mut registry = Registry::open(&dir.join("registry.jsonl")).expect("registry");

    let family = "deflation-void-control";
    let ids = seed_family(&mut registry, family, 8);
    let before = trials_from_registry(&registry, family);
    assert_eq!(before, 8, "eight standing trials");

    let score = |n| {
        deflated_sharpe(DeflationInputs {
            observed_sharpe: 0.09,
            skew: 0.0,
            kurtosis: 3.0,
            n_observations: 1_000,
            n_trials: n,
            trial_sharpe_dispersion: None,
        })
        .expect("inputs")
        .dsr
    };
    let dsr_before = score(before);

    registry
        .void(
            &ids[0],
            "withdrawn by the void control",
            "2026-07-31T00:00:00Z",
        )
        .expect("void");
    let after = trials_from_registry(&registry, family);
    let dsr_after = score(after);

    eprintln!("[void] trials {before} -> {after}   DSR {dsr_before:.6} -> {dsr_after:.6}");
    assert_eq!(after, before - 1, "the void must reach the trial count");
    assert!(
        dsr_after > dsr_before,
        "a smaller denominator must raise the deflated Sharpe: {dsr_before:.6} -> {dsr_after:.6}"
    );
}

/// A trial count's composition renders as arithmetic, not a bare integer.
///
/// §4 makes every account a trial (D-0067), so `3840` alone cannot tell a
/// reader whether the account dimension is in there. The plan's amendment
/// requires the composition on the scorecard for exactly that reason.
#[test]
fn a_trial_count_renders_its_composition_including_the_account_dimension() {
    let c = TrialComposition {
        combos: 240,
        accounts: 16,
        folds: 1,
        seeds: 1,
        voided: 0,
    };
    assert_eq!(c.total(), 3_840);
    assert_eq!(
        c.render(),
        "240 combos × 16 accounts × 1 folds × 1 seeds = 3840"
    );

    let voided = TrialComposition { voided: 40, ..c };
    assert_eq!(voided.total(), 3_800);
    assert!(
        voided.render().contains("− 40 voided"),
        "voids are netted out visibly: {}",
        voided.render()
    );

    // The account dimension multiplies. Stated as a test because the plan flags
    // it as a contradiction to be surfaced rather than discovered.
    let single = TrialComposition { accounts: 1, ..c };
    assert_eq!(single.total() * 16, c.total());
}

/// Helper: charge `n` trials to a family and return their run ids.
///
/// One combo index per trial, because a trial is
/// `(config_hash, account_id, combo_index)` — folds and seeds deliberately do
/// not multiply it (registry rule 3).
fn seed_family(registry: &mut Registry, family: &str, n: usize) -> Vec<String> {
    use crate::registry::{RunKey, RunRow};
    use crate::stages::Criteria;
    let mut ids = Vec::new();
    for combo_index in 0..n {
        let key = RunKey {
            config_hash: "deadbeefdeadbeef".to_owned(),
            account_id: Some("default".to_owned()),
            combo_index,
            fold: None,
            seed: 42,
        };
        let id = key.run_id();
        let row = RunRow {
            key,
            hypothesis_family: family.to_owned(),
            params: format!("combo({combo_index})"),
            fill_model: "spread_cross".to_owned(),
            git_sha: "0000000".to_owned(),
            data_manifest_ids: Vec::new(),
            started_at: "2026-07-31T00:00:00Z".to_owned(),
            // The criteria are irrelevant to a trial COUNT and are filled in
            // minimally: this control is about the denominator, not the gates.
            criteria: Criteria {
                stages: Vec::new(),
                cost_sweep_half_ticks: vec![0, 1, 2, 4],
                min_oos_trades: 0,
                min_oos_sessions: 0,
                min_oos_return_pct_free_fills: 0.0,
                min_oos_sharpe_after_costs: 0.0,
                kill_if_dead_half_ticks: 2,
                require_controls_beaten: false,
                s0_min_abs_ic: None,
                max_permutation_p: None,
                max_pbo: 0.5,
                require_plateau: false,
            },
        };
        registry.insert_running(&row).expect("insert");
        ids.push(id);
    }
    ids
}

// ---------------------------------------------------------------------------
// The pin.
// ---------------------------------------------------------------------------

/// Pinned determinism hash over the deflation arithmetic.
///
/// Inputs: the seeded series above, its moments, and the DSR at four trial
/// counts. Anything that changes the estimator, the normal CDF, the probit or
/// the moment definitions moves this — which is the point, because every one of
/// those changes every deflated Sharpe on every scorecard.
#[test]
fn the_deflated_sharpe_determinism_hash_is_pinned() {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let returns = series(&mut rng, 2_000, 0.12);
    let (sharpe, skew, kurtosis) = moments(&returns).expect("moments");

    let mut h = blake3::Hasher::new();
    h.update(&sharpe.to_le_bytes());
    h.update(&skew.to_le_bytes());
    h.update(&kurtosis.to_le_bytes());
    for n in [1usize, 24, 240, 3_840] {
        let d = deflated_sharpe(DeflationInputs {
            observed_sharpe: sharpe,
            skew,
            kurtosis,
            n_observations: returns.len(),
            n_trials: n,
            trial_sharpe_dispersion: None,
        })
        .expect("inputs");
        h.update(&d.expected_max_sharpe.to_le_bytes());
        h.update(&d.standard_error.to_le_bytes());
        h.update(&d.dsr.to_le_bytes());
    }
    let digest = h.finalize().to_hex()[..16].to_owned();

    eprintln!("[pinned] deflated sharpe hash = {digest}");
    assert_eq!(
        digest, "fec6ffe24b0447a8",
        "the deflation arithmetic moved. Same seed and same series must give the same \
         numbers; if this changed on purpose, re-derive it and say so in a decision entry."
    );
}

// ---------------------------------------------------------------------------
// D-0125 — deflation is unit-consistent, and the invariance proves it.
// ---------------------------------------------------------------------------

/// **The same returns deflate to the same DSR whatever frequency the Sharpe
/// was annualized at**, once the observation and the denominator are put in
/// matching units.
///
/// This is an INVARIANCE, and an invariance has the mirror image of the
/// strict-inequality trap: it passes vacuously when the transformation changed
/// nothing. So the annualized Sharpes are asserted to DIFFER first — a
/// positive control on the fixture itself (D-0118 applied to an equality
/// rather than to a search). Without it, "the DSR is unchanged at 1m and 1h"
/// is trivially true of a test where 1m and 1h produced the same number.
#[test]
fn the_deflated_sharpe_does_not_depend_on_the_annualization_frequency() {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
    let returns = series(&mut rng, 2_000, 0.12);
    let (per_observation, skew, kurtosis) = moments(&returns).expect("moments");

    // Bars per year at two grains: 1-minute and 1-hour on a 24/5-ish calendar.
    let minute_bpy = 525_949.0_f64;
    let hour_bpy = 8_765.8_f64;

    // What `crucible-engine::metrics` would have reported at each grain.
    let annualized_at = |bpy: f64| per_observation * bpy.sqrt();
    let m = annualized_at(minute_bpy);
    let h = annualized_at(hour_bpy);

    // THE POSITIVE CONTROL ON THE FIXTURE. If these were equal the invariance
    // below would hold for a reason that has nothing to do with the fix.
    assert!(
        (m - h).abs() > 1.0,
        "the fixture must actually exercise two annualizations: 1m gave {m}, 1h gave {h}"
    );

    // The de-annualization the run path now performs before deflating.
    let dsr_at = |annualized: f64, bpy: f64| {
        deflated_sharpe(DeflationInputs {
            observed_sharpe: annualized / bpy.sqrt(),
            skew,
            kurtosis,
            n_observations: returns.len(),
            n_trials: 24,
            trial_sharpe_dispersion: None,
        })
        .expect("valid inputs")
        .dsr
    };
    let from_minutes = dsr_at(m, minute_bpy);
    let from_hours = dsr_at(h, hour_bpy);
    assert_eq!(
        from_minutes.to_bits(),
        from_hours.to_bits(),
        "same returns, same trials, two annualizations: {from_minutes} vs {from_hours}"
    );

    // And the converse that gives the invariance teeth: feeding the ANNUALIZED
    // ratio against a per-bar `n_observations` — the defect this replaced —
    // does NOT produce the same answer, and saturates. A test that only
    // asserted the equality above would pass on the broken build too, because
    // the broken build is also self-consistent at a single frequency.
    let broken = |annualized: f64| {
        deflated_sharpe(DeflationInputs {
            observed_sharpe: annualized,
            skew,
            kurtosis,
            n_observations: returns.len(),
            n_trials: 24,
            trial_sharpe_dispersion: None,
        })
        .expect("valid inputs")
        .dsr
    };
    // THE DISCRIMINATOR, and it is not the equality above.
    //
    // Measured while writing this: the mismatched-units form saturates to
    // EXACTLY 1.0 at BOTH frequencies — at 1h the annualized ratio is still
    // ~94x the per-observation one, far past what the standard error can
    // absorb. So the broken build is frequency-invariant too, and the
    // invariance assertion alone PASSES on it. Saturation is invariant.
    //
    // What separates fixed from broken is therefore not that the answer is
    // stable but that it is INFORMATIVE: a DSR strictly inside (0, 1) rather
    // than pinned to an endpoint. That is the assertion which fails on the
    // defect, and it is why the prescribed 1m-vs-1h invariance needed a
    // companion rather than standing alone.
    assert_eq!(broken(m), 1.0, "the defect saturates at 1m");
    assert_eq!(
        broken(h),
        1.0,
        "and at 1h — so equality alone cannot detect it"
    );
    assert!(
        from_minutes > 0.0 && from_minutes < 1.0,
        "a deflated Sharpe that carries information is strictly inside (0, 1);          {from_minutes} is an endpoint, which is what the mismatch produced"
    );
}
