use super::*;

/// The block-B gate: one hash over the whole battery, covering both statistics
/// and every input the plan names — config, trial count, block count, seed.
///
/// Hashed rather than compared field by field so drift anywhere fails, not only
/// where an assertion happens to look. The fixture is a deterministic
/// SplitMix64 walk rather than a hand-written matrix because the estimator's
/// interesting behaviour is over many combos and folds, and a matrix that small
/// enough to write by hand cannot exercise the rank logic.
///
/// Inputs, so the number can be re-derived: `matrix(seed = 7, combos = 8,
/// blocks = 6)`, deflated over `n_trials = 24` with the observed Sharpe, skew
/// and kurtosis of each combo's own row.
#[test]
fn the_block_b_battery_is_pinned() {
    const SEED: u64 = 7;
    const COMBOS: usize = 8;
    const BLOCKS: usize = 6;

    // Inlined SplitMix64: the same generator the permutation gate uses, and
    // inlined for the same reason — a pinned fixture must not move when a
    // dependency's internals do.
    let mut state = SEED;
    let mut next = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let bits = z ^ (z >> 31);
        #[expect(clippy::cast_precision_loss, reason = "a 53-bit mantissa is plenty")]
        let unit = (bits >> 11) as f64 / (1u64 << 53) as f64;
        unit * 2.0 - 1.0
    };
    let performance: Vec<Vec<f64>> = (0..COMBOS)
        .map(|_| (0..BLOCKS).map(|_| next()).collect())
        .collect();

    let pbo = probability_of_backtest_overfitting(&performance).expect("computable");

    let mut h = blake3::Hasher::new();
    h.update(b"crucible-block-b/1\0");
    h.update(&SEED.to_le_bytes());
    h.update(&(pbo.blocks as u64).to_le_bytes());
    h.update(&(pbo.combos as u64).to_le_bytes());
    h.update(&(pbo.splits as u64).to_le_bytes());
    h.update(&(pbo.underperforming_splits as u64).to_le_bytes());
    h.update(&pbo.value.to_le_bytes());

    // The deflated half, over the same fixture and a trial count of 24 — the
    // grid size the null harness charges, so the two halves of block B are
    // pinned against one another rather than in isolation.
    for row in &performance {
        let shape = crucible_engine::ReturnShape::of(row);
        let observed = row.iter().sum::<f64>() / row.len() as f64;
        let deflated =
            crate::stats::deflated::deflated_sharpe(crate::stats::deflated::DeflationInputs {
                observed_sharpe: observed,
                skew: shape.skew.expect("a random row has a skew"),
                kurtosis: shape.kurtosis.expect("a random row has a kurtosis"),
                n_observations: shape.n_returns,
                n_trials: 24,
                trial_sharpe_dispersion: None,
            })
            .expect("a finite row deflates");
        h.update(&deflated.observed_sharpe.to_le_bytes());
        h.update(&deflated.expected_max_sharpe.to_le_bytes());
        h.update(&deflated.standard_error.to_le_bytes());
        h.update(&deflated.dsr.to_le_bytes());
        h.update(&(deflated.n_trials as u64).to_le_bytes());
    }
    let digest = h.finalize().to_hex()[..16].to_owned();

    eprintln!("[pinned] block B battery hash = {digest}");
    assert_eq!(
        digest, "ef703dfd8d19fdd3",
        "the block-B battery moved. Same fixture, same trial count and same seed must give the \
         same PBO and the same deflated Sharpes; if this changed on purpose, re-derive it twice \
         in clean invocations and say so in a decision entry."
    );
}
