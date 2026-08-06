//! Block C's orchestration controls (C6b-iii).
//!
//! Two planted bugs, each with its converse written FIRST, because an
//! assertion about a planted defect proves nothing until the honest path is
//! shown to behave differently. Both defects are the ones the block's own
//! decisions name: summing sessions instead of unioning them (D-0114), and
//! charging one trial for an N-contract pool (D-0124).

#[cfg(test)]
mod pooled_orchestration_controls {
    use crucible_engine::ReturnStats;
    use crucible_funnel::funnel::{PoolingInputs, pool_contract_evidence};
    use crucible_funnel::pooling::{ContractDays, pool_sessions};
    use crucible_funnel::stats::deflated::{DeflationInputs, deflated_sharpe};

    /// Two contracts that OVERLAP: 40 sessions each, sharing 15, so the union
    /// is 65 and the naive sum is 80. The gap is what the control measures.
    fn overlapping() -> Vec<ContractDays> {
        vec![
            ContractDays {
                instrument: "ESH2024".to_owned(),
                day_keys: (0..40).collect(),
            },
            ContractDays {
                instrument: "ESM2024".to_owned(),
                day_keys: (25..65).collect(),
            },
        ]
    }

    /// **CONTROL (a), converse first.** The honest wiring must produce the
    /// UNION, and admission must judge on it.
    ///
    /// The converse comes first because every assertion about the planted bug
    /// below is worthless if the honest path does not already do the right
    /// thing: a `pool_sessions` that returned 0 for everything would make
    /// "the sum is refused" trivially true.
    #[test]
    fn a_the_honest_union_is_what_admission_judges() {
        let pooled = pool_sessions(&overlapping()).expect("two distinct contracts pool");
        assert_eq!(pooled.distinct_days, 65, "the union of 0..40 and 25..65");
        assert_eq!(pooled.summed_days, 80, "and the sum it must not be");
        assert_eq!(pooled.overlap_days, 15);
        assert!(
            pooled.has_overlap(),
            "the fixture must actually overlap, or union and sum coincide and \
             this control cannot see the difference"
        );
    }

    /// **CONTROL (a), the planted bug.** Wire the SUM where the union belongs
    /// and watch admission accept a config it must refuse.
    ///
    /// The floor is 70 sessions. The honest union is 65 and fails it; the
    /// planted sum is 80 and passes. That is the whole defect in one line, and
    /// it is a *verdict* difference rather than a cosmetic one — which is why
    /// the assertion is on the criterion's outcome and not on the number.
    #[test]
    fn a_planting_the_summed_sessions_makes_admission_accept_what_it_must_refuse() {
        let pooled = pool_sessions(&overlapping()).expect("pools");
        const FLOOR: usize = 70;

        let honest_passes = pooled.distinct_days >= FLOOR;
        let planted_passes = pooled.summed_days >= FLOOR;

        assert!(
            !honest_passes,
            "the honest union ({}) must FAIL a {FLOOR}-session floor, or the \
             planted bug below changes nothing and this control is decoration",
            pooled.distinct_days
        );
        assert!(
            planted_passes,
            "the planted sum ({}) must PASS the same floor -- that gap IS the \
             defect D-0114 exists to prevent",
            pooled.summed_days
        );
    }

    fn stats(n: usize, mean: f64, spread: f64) -> ReturnStats {
        // A sample with real higher moments, built so the deflation has
        // something to work with rather than degenerate to `None`.
        #[expect(clippy::cast_precision_loss, reason = "small test counts")]
        let nf = n as f64;
        ReturnStats {
            n,
            mean,
            m2: spread * nf,
            m3: spread * spread * nf * 0.4,
            m4: spread * spread * spread * nf * 3.6,
            net_delta_nano_usd: 1_500_000_000,
        }
    }

    fn deflate(n_trials: usize) -> Option<f64> {
        let s = stats(600, 0.0009, 0.000_02);
        let shape = s.shape();
        deflated_sharpe(DeflationInputs {
            observed_sharpe: s.sharpe(1.0)?,
            skew: shape.skew?,
            kurtosis: shape.kurtosis?,
            n_observations: shape.n_returns,
            n_trials,
            trial_sharpe_dispersion: Some(0.01),
        })
        .map(|d| d.dsr)
    }

    /// **CONTROL (b), converse first.** N contracts charge N trials, and the
    /// deflated Sharpe falls when they do.
    ///
    /// **Both guards, and the order matters.** The DSR is a probability, so it
    /// saturates: at 1.0 or 0.0 "it did not fall" is trivially true and a
    /// correction that ignored its denominator entirely would pass a naive
    /// `assert!(a >= b)`. So the value is first asserted to sit STRICTLY inside
    /// (0, 1) — where it can still move in both directions — and only then
    /// asserted to fall STRICTLY.
    #[test]
    fn b_the_deflated_sharpe_falls_strictly_as_the_trial_count_grows() {
        let one = deflate(1).expect("the fixture deflates at one trial");
        let five = deflate(5).expect("the fixture deflates at five trials");

        for (label, v) in [("1 trial", one), ("5 trials", five)] {
            assert!(
                v > 0.0 && v < 1.0,
                "{label}: the DSR must sit strictly inside (0,1) or a saturated \
                 endpoint makes the comparison below vacuous -- got {v}"
            );
        }
        assert!(
            five < one,
            "five trials must deflate STRICTLY harder than one: {five} vs {one}"
        );
    }

    /// **CONTROL (b), the planted bug.** Charge a single trial for an
    /// N-contract pool and watch the deflated Sharpe fail to fall.
    ///
    /// This is the defect D-0124 names: N pooled contracts are N runs and
    /// therefore N trials, and a pooling step that charged one would report a
    /// correction that had not been applied. The assertion is that the planted
    /// value is EQUAL to the one-trial value — not merely "higher" — because
    /// equality is what "the denominator was ignored" actually looks like.
    #[test]
    fn b_planting_a_single_trial_for_a_pool_stops_the_deflation_falling() {
        let honest = deflate(5).expect("deflates");
        let planted = deflate(1).expect("deflates");

        assert!(
            (planted - deflate(1).expect("deflates")).abs() < f64::EPSILON,
            "the planted path is one-trial deflation by construction"
        );
        assert!(
            planted > honest,
            "charging one trial for a five-contract pool must leave the Sharpe \
             UNDEFLATED relative to the honest five: {planted} vs {honest}"
        );
        // And the gap must be big enough to be a finding rather than noise:
        // a control that fired on a 1e-15 difference would fire on rounding.
        assert!(
            planted - honest > 1e-6,
            "the gap must be real: {planted} vs {honest}"
        );
    }

    /// The pooled `Evidence` reads the union it was handed and never derives a
    /// session count from the contracts — the structural half of control (a),
    /// asserted at the seam admission actually reads.
    #[test]
    fn a_the_pooled_evidence_carries_the_union_it_was_given() {
        let ce = crucible_funnel::funnel::ContractEvidence {
            free_fill_oos: crucible_engine::Summary::compute(&[], &[], 0, 252.0),
            free_fill_oos_stats: stats(10, 0.001, 0.0001),
            costed_oos_stats: stats(10, 0.001, 0.0001),
            oos_trades: 9,
            sweep: Vec::new(),
            controls: [
                crucible_funnel::Control {
                    name: "matched random-entry",
                    oos_stitched: None,
                    absent_because: Some("fixture".to_owned()),
                    seed: None,
                    draws: 0,
                    draws_beaten: 0,
                },
                crucible_funnel::Control {
                    name: "buy-and-hold",
                    oos_stitched: None,
                    absent_because: Some("fixture".to_owned()),
                    seed: None,
                    draws: 0,
                    draws_beaten: 0,
                },
            ],
        };
        let d = |s: f64| s;
        let inputs = PoolingInputs {
            distinct_oos_sessions: 65,
            n_trials: 2,
            pbo: None,
            trial_sharpe_dispersion: None,
            deannualize: &d,
            kill_if_dead_half_ticks: 2,
            initial_cash_nano_usd: 100_000_000_000_000,
            bars_per_year: 252.0,
        };
        let pooled = pool_contract_evidence(&ce, std::slice::from_ref(&ce), &inputs);
        assert_eq!(pooled.oos_sessions, 65, "the union, as handed in");
        assert_eq!(pooled.oos_trades, 18, "trades SUM: 9 + 9");
        assert_ne!(
            pooled.oos_sessions, 130,
            "and are emphatically not summed the way trades are"
        );
    }
}
