use super::*;

fn contract(symbol: &str, days: &[i64]) -> ContractDays {
    ContractDays {
        instrument: symbol.to_owned(),
        day_keys: days.to_vec(),
    }
}

/// **The double-count control.** Two contracts of one root that trade the same
/// days must pool to the UNION, never the sum.
///
/// Hand-derived. ESH2024 trades days 10,11,12,13; ESM2024 trades 12,13,14,15.
/// The union is {10,11,12,13,14,15} — six days. The sum is eight. Days 12 and
/// 13 are traded by both contracts and exist once in the calendar, so a pooled
/// run has six sessions of evidence and a naive sum claims eight.
///
/// Watched firing: planting `distinct_days: summed` fails here with left 8,
/// right 6. That mutation is the exact bug this control exists for — it is what
/// "pooling two contracts doubles the sample" looks like in code.
#[test]
fn overlapping_contracts_pool_to_the_union_and_never_the_sum() {
    let pooled = pool_sessions(&[
        contract("ESH2024", &[10, 11, 12, 13]),
        contract("ESM2024", &[12, 13, 14, 15]),
    ])
    .expect("poolable");

    assert_eq!(pooled.distinct_days, 6, "the union of the two day sets");
    assert_eq!(
        pooled.summed_days, 8,
        "what naive addition would have claimed"
    );
    assert_eq!(pooled.overlap_days, 2, "days 12 and 13, traded by both");
    assert!(pooled.has_overlap());
    assert_eq!(pooled.contracts(), 2);
    assert_eq!(
        pooled.per_contract,
        vec![("ESH2024".to_owned(), 4), ("ESM2024".to_owned(), 4)]
    );
}

/// The converse, and it is what makes the control above mean something.
///
/// Without it, a `pool_sessions` that returned a constant 6, or one that
/// always subtracted two, would pass the overlap test. Disjoint contracts
/// share no session, so here the union and the sum agree — and the code must
/// say so rather than subtracting an overlap it invented.
#[test]
fn converse_disjoint_contracts_pool_to_the_sum_because_there_is_no_overlap() {
    let pooled = pool_sessions(&[
        contract("ESH2024", &[10, 11, 12]),
        contract("ESU2024", &[40, 41, 42]),
    ])
    .expect("poolable");

    assert_eq!(pooled.distinct_days, 6);
    assert_eq!(pooled.summed_days, 6);
    assert_eq!(pooled.overlap_days, 0);
    assert!(!pooled.has_overlap());
}

/// The third case that turns the difference into a diagnosis (§7).
///
/// The two tests above differ in both their data and their answer, which
/// proves only that something changed. This one holds everything fixed —
/// same contracts, same day counts, same totals — and slides only the amount
/// of overlap. The distinct count tracks that and nothing else, so the finding
/// is the overlap rather than the sizes.
#[test]
fn the_distinct_count_tracks_overlap_and_nothing_else() {
    let expectations = [
        (0i64, 8usize, 0usize), // fully disjoint
        (2, 6, 2),              // two shared days
        (4, 4, 4),              // completely coincident
    ];
    for (shared, distinct, overlap) in expectations {
        let second_start = 14 - shared;
        let second: Vec<i64> = (second_start..second_start + 4).collect();
        let pooled = pool_sessions(&[
            contract("ESH2024", &[10, 11, 12, 13]),
            contract("ESM2024", &second),
        ])
        .expect("poolable");
        assert_eq!(
            (pooled.distinct_days, pooled.overlap_days),
            (distinct, overlap),
            "sharing {shared} day(s), second contract {second:?}"
        );
        assert_eq!(pooled.summed_days, 8, "the sum is fixed across all three");
    }
}

/// Cross-instrument breadth is not extra `n`.
///
/// Pooling ES with NQ over the same sessions is one sample and a claim that
/// the effect appears in two instruments — the rhyme check. The arithmetic is
/// the same union, and the test exists because this is the case where the
/// temptation to add is strongest: the contracts look independent.
#[test]
fn two_instruments_over_the_same_sessions_are_not_twice_the_sample() {
    let days: Vec<i64> = (0..250).collect();
    let pooled =
        pool_sessions(&[contract("ESH2024", &days), contract("NQH2024", &days)]).expect("poolable");
    assert_eq!(pooled.distinct_days, 250, "one 250-session sample, not 500");
    assert_eq!(pooled.summed_days, 500);
    assert_eq!(pooled.overlap_days, 250);
}

/// Pooling N contracts charges N trials, which is what makes the larger sample
/// cost something.
///
/// The count here is derived from the pool so a caller cannot declare one that
/// disagrees with the evidence; `Registry::trials_for` remains the
/// authoritative number, and this is what it must equal once the claims land.
/// Block B's deflated Sharpe divides by that count, so the selection benchmark
/// rises with the pool — pooling buys sessions and pays in trials.
#[test]
fn pooling_n_contracts_charges_n_trials() {
    for n in 1..=6usize {
        let contracts: Vec<ContractDays> = (0..n)
            .map(|i| {
                let start = i as i64 * 60;
                contract(
                    &format!("ES{i}2024"),
                    &(start..start + 60).collect::<Vec<_>>(),
                )
            })
            .collect();
        let pooled = pool_sessions(&contracts).expect("poolable");
        assert_eq!(pooled.contracts(), n);
        assert_eq!(pooled.distinct_days, n * 60, "disjoint by construction");
    }

    // And the block-B consequence, asserted rather than described: more trials
    // must raise the selection benchmark, so a pooled run's deflated Sharpe is
    // strictly harder to clear than a single contract's.
    let one = crate::stats::deflated::expected_max_z(1);
    let six = crate::stats::deflated::expected_max_z(6);
    assert!(
        six > one,
        "pooling six contracts must deflate harder than one: {six} vs {one}"
    );
}

/// The admission floor is *met* by pooling, never lowered by it.
///
/// H-007 and H-008 both register 250 sessions. One ES contract's active life is
/// roughly 60, so a single-contract run dies at admission — correctly. Five
/// contracts of disjoint life supply 300 distinct sessions and the same floor
/// now discriminates instead of always firing.
#[test]
fn the_two_hundred_and_fifty_session_floor_is_met_by_pooling_not_lowered() {
    const FLOOR: usize = 250;
    let single =
        pool_sessions(&[contract("ESH2024", &(0..60).collect::<Vec<_>>())]).expect("poolable");
    assert!(
        single.distinct_days < FLOOR,
        "one contract must still die at admission: {} sessions",
        single.distinct_days
    );

    let pooled = pool_sessions(
        &(0..5)
            .map(|i| {
                let start = i as i64 * 60;
                contract(
                    &format!("ES{i}2024"),
                    &(start..start + 60).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
    .expect("poolable");
    assert_eq!(pooled.distinct_days, 300);
    assert!(
        pooled.distinct_days >= FLOOR,
        "pooling must SUPPLY the sessions, not relax the floor"
    );

    // The dishonest route to the same pass, refused by arithmetic: five copies
    // of one contract's sessions sum to 300 and are worth 60.
    let overlapping = pool_sessions(
        &(0..5)
            .map(|i| contract(&format!("ES{i}2024"), &(0..60).collect::<Vec<_>>()))
            .collect::<Vec<_>>(),
    )
    .expect("poolable");
    assert_eq!(overlapping.summed_days, 300, "what a naive sum would claim");
    assert_eq!(
        overlapping.distinct_days, 60,
        "five contracts over one contract's sessions are still 60 sessions"
    );
    assert!(
        overlapping.distinct_days < FLOOR,
        "the naive sum would have passed a floor the evidence does not meet"
    );
}

#[test]
fn a_pool_that_cannot_be_formed_is_refused_rather_than_counted() {
    assert_eq!(pool_sessions(&[]), Err(PoolingError::NoContracts));

    assert_eq!(
        pool_sessions(&[contract("ESH2024", &[])]),
        Err(PoolingError::EmptyContract {
            instrument: "ESH2024".to_owned()
        })
    );

    // The double-count in its most direct form.
    let error = pool_sessions(&[contract("ESH2024", &[1, 2]), contract("ESH2024", &[1, 2])])
        .expect_err("a contract cannot be pooled with itself");
    assert_eq!(
        error,
        PoolingError::DuplicateContract {
            instrument: "ESH2024".to_owned()
        }
    );
    assert!(
        error.to_string().contains("doubles its sessions"),
        "{error}"
    );

    assert_eq!(
        pool_sessions(&[contract("ESH2024", &[5, 5])]),
        Err(PoolingError::UnorderedDays {
            instrument: "ESH2024".to_owned(),
            key: 5
        })
    );
    assert_eq!(
        pool_sessions(&[contract("ESH2024", &[5, 4])]),
        Err(PoolingError::UnorderedDays {
            instrument: "ESH2024".to_owned(),
            key: 4
        })
    );
}

/// Same inputs, same answer, every time, and independent of declaration order
/// for the count itself (§2.2). `per_contract` deliberately keeps declaration
/// order — it is a report, not a set.
#[test]
fn the_pooled_count_is_deterministic_and_order_independent() {
    let a = contract("ESH2024", &[10, 11, 12, 13]);
    let b = contract("ESM2024", &[12, 13, 14, 15]);
    let forward = pool_sessions(&[a.clone(), b.clone()]).expect("poolable");
    let reverse = pool_sessions(&[b, a]).expect("poolable");
    assert_eq!(forward.distinct_days, reverse.distinct_days);
    assert_eq!(forward.summed_days, reverse.summed_days);
    assert_eq!(forward.overlap_days, reverse.overlap_days);
    assert_ne!(
        forward.per_contract, reverse.per_contract,
        "the per-contract table follows declaration order and is a report"
    );
}
