use super::*;

/// Every 2-subset of `0..4`, in lexicographic order, is what CSCV must
/// enumerate for a four-fold plan. If this drifts, every PBO below is measuring
/// a different set of splits than it claims to.
#[test]
fn the_split_enumeration_is_every_half_subset_in_lexicographic_order() {
    let mut selection = vec![0usize, 1];
    let mut seen = vec![selection.clone()];
    while next_combination(&mut selection, 4) {
        seen.push(selection.clone());
    }
    assert_eq!(
        seen,
        vec![
            vec![0, 1],
            vec![0, 2],
            vec![0, 3],
            vec![1, 2],
            vec![1, 3],
            vec![2, 3]
        ]
    );
    assert_eq!(binomial(4, 2), Some(6));
    assert_eq!(binomial(20, 10), Some(184_756));
}

// Hand-derived, two blocks and two combos, so both splits can be checked by
// arithmetic rather than by trusting the loop.
//
//   combo 0: [1.0, 0.0]      combo 1: [0.0, 1.0]
//
// Split IS={0}, OOS={1}: IS scores (1.0, 0.0) so combo 0 wins; OOS scores
// (0.0, 1.0) put it last, rank 1 of 2. 2*1 <= 2+1, so the split is an overfit.
// Split IS={1}, OOS={0}: the mirror image, combo 1 wins and is last. Both
// splits overfit, so PBO = 2/2 = 1.
//
// This is the planted overfit in its purest form: in-sample rank is a perfect
// inversion of out-of-sample rank, which is what "the winner was the luckiest
// one" looks like when there is no signal at all.
#[test]
fn a_planted_perfect_inversion_gives_pbo_one() {
    let perf = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
    let pbo = probability_of_backtest_overfitting(&perf).expect("computable");
    assert_eq!(pbo.value, 1.0);
    assert_eq!(pbo.splits, 2);
    assert_eq!(pbo.underperforming_splits, 2);
    assert_eq!(pbo.blocks, 2);
    assert_eq!(pbo.combos, 2);
    assert_eq!(pbo.resolution(), 0.5);
}

// The converse, written to prove the estimator can also say "not overfit" —
// without it, a function that returned 1.0 unconditionally would pass the test
// above. Combo 0 dominates in every block, so the in-sample winner is also the
// out-of-sample winner on both splits and PBO is 0.
#[test]
fn converse_a_genuinely_dominant_combo_gives_pbo_zero() {
    let perf = vec![vec![1.0, 1.0], vec![0.0, 0.0]];
    let pbo = probability_of_backtest_overfitting(&perf).expect("computable");
    assert_eq!(pbo.value, 0.0);
    assert_eq!(pbo.underperforming_splits, 0);
}

// The third case that turns the difference into a diagnosis (§7). The two tests
// above differ in both the data and the answer; this one holds the shape fixed
// — same combos, same blocks, same magnitudes — and moves only which combo is
// good in which block. PBO tracks that and nothing else, so the finding is the
// inversion rather than the scale of the numbers.
#[test]
fn pbo_follows_the_inversion_and_not_the_magnitudes() {
    let inverted = vec![vec![10.0, -10.0], vec![-10.0, 10.0]];
    let aligned = vec![vec![10.0, 10.0], vec![-10.0, -10.0]];
    assert_eq!(
        probability_of_backtest_overfitting(&inverted)
            .expect("computable")
            .value,
        1.0
    );
    assert_eq!(
        probability_of_backtest_overfitting(&aligned)
            .expect("computable")
            .value,
        0.0
    );
}

/// A four-block plan is the shape a short walk-forward config produces, and its
/// six splits are the resolution the null harness will be read at. Pinned so
/// that a change in the enumeration shows up as a changed split count rather
/// than as a slightly different PBO nobody notices.
#[test]
fn four_blocks_evaluate_all_six_splits_at_one_sixth_resolution() {
    let perf = vec![
        vec![1.0, 0.0, 1.0, 0.0],
        vec![0.0, 1.0, 0.0, 1.0],
        vec![0.5, 0.5, 0.5, 0.5],
    ];
    let pbo = probability_of_backtest_overfitting(&perf).expect("computable");
    assert_eq!(pbo.splits, 6);
    assert_eq!(pbo.blocks, 4);
    assert_eq!(pbo.combos, 3);
    assert!((pbo.resolution() - 1.0 / 6.0).abs() < 1e-12);
    assert!(
        (0.0..=1.0).contains(&pbo.value),
        "PBO is a probability: {}",
        pbo.value
    );
}

/// Same inputs, same answer, every time — and in particular the tie-breaking
/// must not depend on anything but the combo index (§2.2).
#[test]
fn the_estimate_is_deterministic_including_every_tie() {
    let all_tied = vec![vec![1.0, 1.0, 1.0, 1.0]; 4];
    let first = probability_of_backtest_overfitting(&all_tied).expect("computable");
    for _ in 0..8 {
        assert_eq!(
            probability_of_backtest_overfitting(&all_tied).expect("computable"),
            first
        );
    }
    // Every cell equal: the argmax takes combo 0, and the ascending rank puts
    // combo 0 last among equals because ties resolve by index — rank 1 of 4.
    // 2*1 <= 4+1, so every split counts as an overfit, which is the honest
    // reading of "the search could not tell these apart".
    assert_eq!(first.value, 1.0);
}

#[test]
fn a_grid_of_one_has_no_selection_to_measure() {
    let one = vec![vec![1.0, 2.0]];
    assert_eq!(
        probability_of_backtest_overfitting(&one),
        Err(PboUnavailable::TooFewCombos { combos: 1 })
    );
    assert_eq!(
        probability_of_backtest_overfitting(&[]),
        Err(PboUnavailable::TooFewCombos { combos: 0 })
    );
}

/// An odd block count is refused rather than repaired. Dropping a fold to make
/// `S` even would change the sample the number describes while the number kept
/// claiming to describe the run — and the dropped fold would be invisible.
#[test]
fn an_odd_block_count_is_refused_rather_than_silently_trimmed() {
    let perf = vec![vec![1.0, 2.0, 3.0], vec![3.0, 2.0, 1.0]];
    let error = probability_of_backtest_overfitting(&perf).expect_err("odd S has no half split");
    assert_eq!(error, PboUnavailable::UnsplittableBlocks { blocks: 3 });
    assert!(
        error.to_string().contains("change the fold plan"),
        "the refusal must name the remedy: {error}"
    );

    let single = vec![vec![1.0], vec![2.0]];
    assert_eq!(
        probability_of_backtest_overfitting(&single),
        Err(PboUnavailable::UnsplittableBlocks { blocks: 1 })
    );
}

#[test]
fn a_ragged_or_non_finite_matrix_is_refused_before_any_argmax() {
    let ragged = vec![vec![1.0, 2.0], vec![3.0]];
    assert_eq!(
        probability_of_backtest_overfitting(&ragged),
        Err(PboUnavailable::RaggedMatrix {
            expected: 2,
            found: 1,
            combo: 1
        })
    );

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let perf = vec![vec![1.0, 2.0], vec![3.0, bad]];
        assert_eq!(
            probability_of_backtest_overfitting(&perf),
            Err(PboUnavailable::NonFinitePerformance { combo: 1, block: 1 }),
            "a {bad} cell would decide the argmax silently"
        );
    }
}

/// Above the cap this refuses instead of subsampling: a subsampled CSCV is a
/// different estimator under the same name (§9's no-silent-caps rule).
#[test]
fn a_fold_explosion_refuses_rather_than_subsampling() {
    // C(22, 11) = 705,432, above MAX_SPLITS.
    let perf = vec![vec![0.0; 22], vec![1.0; 22]];
    let error = probability_of_backtest_overfitting(&perf).expect_err("above the cap");
    let PboUnavailable::TooManySplits { blocks, splits } = error else {
        panic!("expected a split-count refusal, got {error:?}");
    };
    assert_eq!(blocks, 22);
    assert_eq!(splits, 705_432);
    assert!(splits > MAX_SPLITS);

    // C(20, 10) = 184,756 is the largest plan that still runs, so the cap is a
    // real boundary rather than a number nothing approaches.
    let ok = vec![vec![0.0; 20], vec![1.0; 20]];
    assert_eq!(
        probability_of_backtest_overfitting(&ok)
            .expect("at the cap")
            .splits,
        184_756
    );
}
