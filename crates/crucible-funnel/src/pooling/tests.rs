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

fn evaluation(
    sym: &str,
    front: (i64, i64),
    oos: &[i64],
    trades: usize,
    folds: usize,
) -> ContractEvaluation {
    ContractEvaluation {
        instrument: sym.to_owned(),
        front_window_days: (front.0..=front.1).collect(),
        oos_day_keys: oos.to_vec(),
        oos_trades: trades,
        insufficient_warmup: None,
        folds,
        fold_needs_sessions: 36,
    }
}

/// Sessions union, trades sum — and the asymmetry is the point.
///
/// Two contracts trading the same session double-count it, so sessions must be
/// unioned. A round-trip belongs to exactly one contract's replay, so trades
/// cannot double-count and summing them is exact. Here the two contracts share
/// OOS days 12 and 13: five distinct sessions, and 7 trades that are genuinely
/// 7 because no trade is in both replays.
#[test]
fn oos_sessions_union_while_oos_trades_sum() {
    let pooled = pool_evaluations(&[
        evaluation("ESH2024", (10, 13), &[10, 11, 12, 13], 3, 1),
        evaluation("ESM2024", (14, 17), &[12, 13, 14], 4, 1),
    ])
    .expect("poolable");
    assert_eq!(pooled.oos_sessions, 5, "the union of the two OOS day sets");
    assert_eq!(pooled.oos_trades, 7, "3 + 4; trades cannot double-count");
    assert_eq!(pooled.contracts_evaluated, 2);
}

/// The floor is two-part, and clearing one half is not clearing it.
///
/// Written because "four contracts clear 250" addressed sessions alone while
/// `min_oos_trades` is an equally real criterion. A pool can be session-rich
/// and trade-poor — a rule firing once a fortnight over 250 sessions produces
/// ~18 round-trips — and this asserts the two numbers are independent.
#[test]
fn a_session_rich_trade_poor_pool_clears_only_half_the_floor() {
    let pooled = pool_evaluations(&[evaluation(
        "ESH2024",
        (0, 299),
        &(0..300).collect::<Vec<_>>(),
        18,
        4,
    )])
    .expect("poolable");
    assert_eq!(pooled.oos_sessions, 300);
    assert_eq!(pooled.oos_trades, 18);
    assert!(pooled.oos_sessions >= 250, "the session half clears");
    assert!(pooled.oos_trades < 200, "the trade half does not");
}

/// A contract that fits no fold is SKIPPED WITH A COUNT, never dropped.
///
/// D-0070's spread-filter pattern: a declared filter with a number beside it.
/// A pool that quietly lost a short contract would report a smaller sample
/// than it declared, and nothing on the page would say so.
#[test]
fn a_contract_that_fits_no_fold_is_skipped_with_its_reason() {
    let pooled = pool_evaluations(&[
        evaluation("ESH2024", (0, 65), &(26..66).collect::<Vec<_>>(), 40, 4),
        evaluation("ESM2024", (66, 85), &[], 0, 0),
        evaluation("ESU2024", (86, 151), &(112..152).collect::<Vec<_>>(), 37, 4),
    ])
    .expect("poolable");
    assert_eq!(pooled.contracts_evaluated, 2);
    assert_eq!(pooled.skipped.len(), 1);
    let (symbol, reason) = &pooled.skipped[0];
    assert_eq!(symbol, "ESM2024");
    assert_eq!(
        *reason,
        SkipReason::NoCompleteFold {
            sessions: 20,
            needed: 36
        }
    );
    assert!(reason.to_string().contains("one fold needs 36"), "{reason}");
    // A skipped contract contributes NOTHING — not its sessions, not its trades.
    assert_eq!(pooled.oos_sessions, 80);
    assert_eq!(pooled.oos_trades, 77);
    // And the skip count renders even though it is the interesting case; the
    // zero case is asserted by `the_skip_count_renders_even_when_zero`.
    assert!(pooled.render().contains("1 skipped"), "{}", pooled.render());
}

/// The converse: the count is printed even when nothing was skipped.
///
/// Without this, a `render` that only mentioned skips when nonzero would pass
/// the test above while making "nothing was dropped" indistinguishable from
/// "the report forgot to say".
#[test]
fn the_skip_count_renders_even_when_zero() {
    let pooled =
        pool_evaluations(&[evaluation("ESH2024", (0, 65), &[10, 11], 4, 1)]).expect("poolable");
    assert!(pooled.skipped.is_empty());
    assert!(pooled.render().contains("0 skipped"), "{}", pooled.render());
    assert!(
        pooled.render().contains("contiguous"),
        "{}",
        pooled.render()
    );
}

/// A non-consecutive pool is REPORTED, not refused.
///
/// Pooling ESH and ESU while skipping ESM leaves the OOS days disjoint and the
/// session count unchanged — every arithmetic check still passes — while the
/// sample carries a three-month hole. Deliberately non-consecutive pools are
/// legitimate experiments (every March contract across years, for
/// seasonality); a hole nobody can see is not.
#[test]
fn a_hole_in_the_middle_of_a_pool_is_reported_with_its_span_and_size() {
    let pooled = pool_evaluations(&[
        evaluation("ESH2024", (0, 65), &(26..66).collect::<Vec<_>>(), 40, 4),
        evaluation(
            "ESU2024",
            (132, 197),
            &(158..198).collect::<Vec<_>>(),
            38,
            4,
        ),
    ])
    .expect("poolable");
    assert!(!pooled.is_contiguous());
    assert_eq!(pooled.gaps.len(), 1);
    let gap = &pooled.gaps[0];
    assert_eq!((gap.from_day, gap.to_day), (66, 131));
    assert_eq!(gap.days(), 66, "one whole missing contract's front window");
    assert_eq!(gap.between, ("ESH2024".to_owned(), "ESU2024".to_owned()));
    assert!(
        pooled.render().contains("66 trading day(s)"),
        "{}",
        pooled.render()
    );
    // The point: every count is still correct. Only the gap report reveals it.
    assert_eq!(pooled.oos_sessions, 80);
}

/// Consecutive contracts report no gap — the converse that makes the hole
/// control mean something. Front windows tile (C3a), so an adjacent pool is
/// contiguous by construction.
#[test]
fn converse_consecutive_contracts_report_no_gap() {
    let pooled = pool_evaluations(&[
        evaluation("ESH2024", (0, 65), &(26..66).collect::<Vec<_>>(), 40, 4),
        evaluation("ESM2024", (66, 131), &(92..132).collect::<Vec<_>>(), 41, 4),
    ])
    .expect("poolable");
    assert!(pooled.is_contiguous(), "{:?}", pooled.gaps);
    assert_eq!(pooled.oos_sessions, 80);
    assert_eq!(pooled.oos_trades, 81);
}

/// Per-contract numbers are measured and summed, never extrapolated.
///
/// The contracts here deliberately differ — 64/70/66 sessions, as ES 2024
/// actually measured — so any `N x constant` shortcut gives a different
/// answer than the truth. This is the control on the specific error that
/// produced "four contracts clear 250".
#[test]
fn unequal_contracts_are_summed_from_measurement_not_extrapolated() {
    let pooled = pool_evaluations(&[
        evaluation("ESH2024", (0, 63), &(0..24).collect::<Vec<_>>(), 20, 2),
        evaluation("ESM2024", (64, 133), &(64..94).collect::<Vec<_>>(), 31, 3),
        evaluation(
            "ESU2024",
            (134, 199),
            &(134..160).collect::<Vec<_>>(),
            26,
            2,
        ),
    ])
    .expect("poolable");
    assert_eq!(pooled.oos_sessions, 24 + 30 + 26);
    assert_eq!(pooled.oos_trades, 77);
    assert_eq!(
        pooled.per_contract,
        vec![
            ("ESH2024".to_owned(), 24, 20),
            ("ESM2024".to_owned(), 30, 31),
            ("ESU2024".to_owned(), 26, 26),
        ]
    );
    let extrapolated = 3 * 24;
    assert_ne!(
        pooled.oos_sessions, extrapolated,
        "N x the first contract's count is not the pooled count"
    );
}

/// A pool is a **set** of contracts, and the order they were typed into a
/// config is not a fact about the market. The gap scan walks consecutive front
/// windows, so it has to walk them in *time* order — and until C4b-i it walked
/// them in declaration order.
///
/// This is the control `pool_evaluations` did not have while
/// `the_pooled_count_is_deterministic_and_order_independent` covered
/// `pool_sessions`. The asymmetry is how the defect survived: the arithmetic
/// was order-independent and the reporting built on it was not.
#[test]
fn a_hole_is_found_whichever_order_the_pool_is_declared_in() {
    // ESH 10..=13, ESM 14..=17, ESU 30..=33 — one hole, days 18..=29.
    let h = evaluation("ESH2024", (10, 13), &[12, 13], 4, 1);
    let m = evaluation("ESM2024", (14, 17), &[16, 17], 4, 1);
    let u = evaluation("ESU2024", (30, 33), &[32, 33], 4, 1);

    let forward = pool_evaluations(&[h.clone(), m.clone(), u.clone()]).expect("poolable");
    let reversed = pool_evaluations(&[u.clone(), m.clone(), h.clone()]).expect("poolable");
    let shuffled = pool_evaluations(&[m, u, h]).expect("poolable");

    assert_eq!(
        forward.gaps, reversed.gaps,
        "reversing must not hide a hole"
    );
    assert_eq!(forward.gaps, shuffled.gaps, "nor must shuffling");
    assert_eq!(forward.gaps.len(), 1);
    assert_eq!(forward.gaps[0].from_day, 18);
    assert_eq!(forward.gaps[0].to_day, 29);
    assert_eq!(forward.gaps[0].days(), 12);
    // The bound contracts are named in TIME order too. A gap reported as
    // "between ESU2024 and ESM2024" would be a hole running backwards.
    assert_eq!(
        forward.gaps[0].between,
        ("ESM2024".to_owned(), "ESU2024".to_owned())
    );
    assert_eq!(reversed.gaps[0].between, forward.gaps[0].between);
    assert!(!reversed.is_contiguous());
    assert!(
        !reversed.render().contains("contiguous"),
        "the render is the honesty device; declaring backwards must not silence it"
    );
}

/// The converse, and it is the one that makes the test above mean something:
/// a scan that reported a hole for every out-of-order pool would pass that
/// test while being useless. A genuinely contiguous pool declared backwards
/// has no hole and says so.
#[test]
fn a_contiguous_pool_declared_out_of_order_invents_no_hole() {
    let h = evaluation("ESH2024", (10, 13), &[12, 13], 4, 1);
    let m = evaluation("ESM2024", (14, 17), &[16, 17], 4, 1);

    let forward = pool_evaluations(&[h.clone(), m.clone()]).expect("poolable");
    let reversed = pool_evaluations(&[m, h]).expect("poolable");

    assert!(
        forward.gaps.is_empty(),
        "14 follows 13 with nothing missing"
    );
    assert!(
        reversed.gaps.is_empty(),
        "and reversing does not create one"
    );
    assert!(reversed.is_contiguous());
    assert!(reversed.render().contains("contiguous"));
}

/// Sorting the gap scan must not reorder the report. `per_contract` follows
/// declaration order because it echoes what the config asked for — the same
/// split `the_pooled_count_is_deterministic_and_order_independent` asserts for
/// `pool_sessions`.
#[test]
fn sorting_the_gap_scan_leaves_the_per_contract_report_in_declaration_order() {
    let h = evaluation("ESH2024", (10, 13), &[12, 13], 4, 1);
    let m = evaluation("ESM2024", (14, 17), &[16, 17], 4, 1);

    let forward = pool_evaluations(&[h.clone(), m.clone()]).expect("poolable");
    let reversed = pool_evaluations(&[m, h]).expect("poolable");

    let names = |p: &PooledEvaluation| -> Vec<String> {
        p.per_contract.iter().map(|(s, _, _)| s.clone()).collect()
    };
    assert_eq!(names(&forward), ["ESH2024", "ESM2024"]);
    assert_eq!(names(&reversed), ["ESM2024", "ESH2024"]);
    // The pooled numbers themselves are order-independent, as they must be.
    assert_eq!(forward.oos_sessions, reversed.oos_sessions);
    assert_eq!(forward.oos_trades, reversed.oos_trades);
}

// ---------------------------------------------------------------------------
// Pooled trial identity (D-0124). Design B: a third composite in the effective
// `config_hash`, never a wider `RunKey`.
// ---------------------------------------------------------------------------

/// **The converse, and it comes first.** The same contract over the same
/// window must hash the SAME — otherwise "N contracts charge N trials" would
/// be satisfied by a hash that simply varies, including one that varies per
/// call, and the trial count would be noise rather than a count.
///
/// It is also the correctness claim in its own right: a duplicate contract
/// collides into one trial, because it is one run of one contract.
#[test]
fn the_same_contract_and_window_hash_the_same() {
    let a = pooled_registration_hash(
        "strat",
        "ESH2024",
        1_702_332_000_000_000_001,
        1_710_190_800_000_000_001,
    );
    let b = pooled_registration_hash(
        "strat",
        "ESH2024",
        1_702_332_000_000_000_001,
        1_710_190_800_000_000_001,
    );
    assert_eq!(a, b, "identity must be a function of its inputs");
    assert_eq!(a.len(), 64, "blake3 lowercase hex");
}

/// Different contracts of one root are different runs and different trials.
#[test]
fn different_contracts_hash_differently() {
    let h = pooled_registration_hash("strat", "ESH2024", 10, 20);
    let m = pooled_registration_hash("strat", "ESM2024", 10, 20);
    assert_ne!(h, m, "two contracts are two runs and two trials (D-0114)");
}

/// The window is bound, so a rebuilt roll table that moves a contract's front
/// window makes a different run — two results that are not comparable must not
/// share a trial.
///
/// The instants are one NANOSECOND apart, which is the resolution C4b-i exists
/// to protect: an identity built from civil dates would call these the same
/// run, because both round to the same day.
#[test]
fn a_front_window_one_nanosecond_different_is_a_different_run() {
    let a = pooled_registration_hash(
        "strat",
        "ESH2024",
        1_702_332_000_000_000_001,
        1_710_190_800_000_000_001,
    );
    let b = pooled_registration_hash(
        "strat",
        "ESH2024",
        1_702_332_000_000_000_001,
        1_710_190_800_000_000_002,
    );
    assert_ne!(
        a, b,
        "the window is part of the identity, at its own resolution"
    );
    let c = pooled_registration_hash(
        "strat",
        "ESH2024",
        1_702_332_000_000_000_002,
        1_710_190_800_000_000_001,
    );
    assert_ne!(a, c);
}

/// A different strategy is a different run even on the same contract and
/// window — the composite extends the D-0012 hash rather than replacing it.
#[test]
fn the_strategy_hash_still_separates_runs() {
    let a = pooled_registration_hash("strat-a", "ESH2024", 10, 20);
    let b = pooled_registration_hash("strat-b", "ESH2024", 10, 20);
    assert_ne!(a, b);
}

/// Fields are length-prefixed, so concatenation cannot alias. Without it
/// `("ab","c")` and `("a","bc")` would hash alike and two different runs would
/// share a trial — the exact failure the trial count exists to prevent.
#[test]
fn adjacent_fields_cannot_alias_by_concatenation() {
    let a = pooled_registration_hash("ab", "c", 1, 2);
    let b = pooled_registration_hash("a", "bc", 1, 2);
    assert_ne!(a, b, "length prefixes must make the boundary unambiguous");
}

/// **N contracts yield N DISTINCT identities**, which is what makes
/// `Registry::trials_for` return N rather than one.
///
/// The count is derived from the pool rather than declared, so a caller cannot
/// claim a trial count that disagrees with the evidence (D-0114).
#[test]
fn n_contracts_yield_n_distinct_identities() {
    let contracts = ["ESH2024", "ESM2024", "ESU2024", "ESZ2024"];
    let hashes: std::collections::BTreeSet<String> = contracts
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let start = 1_000 + (i as i64) * 100;
            pooled_registration_hash("strat", c, start, start + 90)
        })
        .collect();
    assert_eq!(hashes.len(), contracts.len(), "four contracts, four trials");

    // And the duplicate collides rather than inflating the count.
    let mut with_dup = hashes.clone();
    with_dup.insert(pooled_registration_hash("strat", "ESH2024", 1_000, 1_090));
    assert_eq!(
        with_dup.len(),
        contracts.len(),
        "the same contract twice is one trial, not two"
    );
}

/// **The trial count is CONSUMED, not merely recorded.** This is the control
/// that crosses from the registry into block B's statistics, and it is the one
/// that would still fail if pooling charged N trials into a page that deflated
/// by one.
///
/// Pooling N contracts charges N trials (D-0114), and a deflated Sharpe
/// corrected for N trials must be strictly lower than the same observation
/// corrected for one. Strictly: a correction that ignored its denominator
/// would return the same number and `<=` would not notice.
#[test]
fn the_deflated_sharpe_falls_as_the_pool_grows() {
    use crate::stats::deflated::{DeflationInputs, deflated_sharpe};
    let at = |n_trials: usize| {
        deflated_sharpe(DeflationInputs {
            observed_sharpe: 0.12,
            skew: -0.09,
            kurtosis: 2.9,
            n_observations: 2_000,
            n_trials,
            trial_sharpe_dispersion: None,
        })
        .expect("valid inputs")
        .dsr
    };
    let one = at(1);
    let four = at(4);
    let sixteen = at(16);
    assert!(
        four < one,
        "four pooled contracts are four trials and must deflate harder than one: \
         {four} vs {one}"
    );
    assert!(
        sixteen < four,
        "and sixteen harder than four: {sixteen} vs {four}"
    );
    // The converse: the observation itself is unchanged, so what moved is the
    // correction rather than the measurement.
    assert!(
        (0.0..=1.0).contains(&one) && (0.0..=1.0).contains(&sixteen),
        "a DSR is a probability"
    );
}
