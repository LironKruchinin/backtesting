//! Run summary statistics.
//!
//! Unit rule (CLAUDE.md §2.3): accounting stays integer nano-USD; this module
//! is the boundary where results convert to `f64` for *statistics* — ratios,
//! drawdowns, Sharpe. Statistics never flow back into accounting.
//!
//! The per-run Sharpe here is deliberately naive (per-bar returns, flat
//! annualization). Research-grade inference — deflated Sharpe, PBO,
//! permutation p-values — lives in `crucible-funnel::stats` and operates
//! across runs, not inside one.

use crucible_core::prelude::*;

use crate::portfolio::ClosedTrade;

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub initial_equity_nano_usd: NanoUsd,
    pub final_equity_nano_usd: NanoUsd,
    pub total_return_pct: f64,
    pub max_drawdown_pct: f64,
    pub round_trips: usize,
    /// Fraction of round-trips with positive net PnL; `None` if no trades.
    pub win_rate: Option<f64>,
    /// Naive annualized Sharpe of per-bar equity returns; `None` if the
    /// return series is empty or has zero variance.
    pub sharpe_naive: Option<f64>,
    /// Sample size and higher moments of the series `sharpe_naive` came from,
    /// carried so the ratio can be deflated without re-deriving them from a
    /// different series. See [`ReturnShape`].
    pub return_shape: ReturnShape,
    pub fees_nano_usd: NanoUsd,
}

impl Summary {
    /// `bars_per_year` sets Sharpe annualization (e.g. ≈347,760 for 1m bars
    /// on a 23h/252-day futures session).
    #[must_use]
    pub fn compute(
        equity: &[(Ts, NanoUsd)],
        closed_trades: &[ClosedTrade],
        fees_nano_usd: NanoUsd,
        bars_per_year: f64,
    ) -> Summary {
        let initial = equity.first().map_or(0, |&(_, e)| e);
        let final_ = equity.last().map_or(0, |&(_, e)| e);

        let total_return_pct = if initial != 0 {
            (nano_usd_to_f64(final_) / nano_usd_to_f64(initial) - 1.0) * 100.0
        } else {
            0.0
        };

        // Max drawdown over the equity curve.
        let mut peak = f64::MIN;
        let mut max_dd = 0.0_f64;
        for &(_, e) in equity {
            let e = nano_usd_to_f64(e);
            peak = peak.max(e);
            if peak > 0.0 {
                max_dd = max_dd.max((peak - e) / peak);
            }
        }

        let (sharpe_naive, return_shape) = sharpe_and_shape(equity, bars_per_year);

        let wins = closed_trades.iter().filter(|t| t.net_nano_usd > 0).count();
        let win_rate = if closed_trades.is_empty() {
            None
        } else {
            #[expect(clippy::cast_precision_loss, reason = "trade counts are small")]
            Some(wins as f64 / closed_trades.len() as f64)
        };

        Summary {
            initial_equity_nano_usd: initial,
            final_equity_nano_usd: final_,
            total_return_pct,
            max_drawdown_pct: max_dd * 100.0,
            round_trips: closed_trades.len(),
            win_rate,
            sharpe_naive,
            return_shape,
            fees_nano_usd,
        }
    }
}

/// The **path-independent** half of a [`Summary`]: the naive Sharpe and the
/// shape of the series it was computed on.
///
/// # Why this is a separate function
///
/// A [`Summary`] also carries `max_drawdown_pct`, and a drawdown is meaningful
/// only over a curve an account actually walked. A curve stitched across
/// CONTRACTS is not one: its seams join windows months apart, so a drawdown
/// measured over it describes a path nobody took (D-0119). Block C therefore
/// needs the Sharpe and the moments of a stitched curve **without** the
/// drawdown — and the safe way to provide that is a function that never
/// computes one, rather than a `Summary` whose fields mean different things
/// depending on which path produced it.
///
/// The precedent is `AdjustedPrice` (D-0042): a value meaningful in one space
/// and corrupting in another is separated by a type that cannot cross, not by
/// a convention readers must remember. A computed-then-discarded drawdown
/// would be one refactor away from being surfaced, and the person doing that
/// refactor would see a field sitting there looking available.
///
/// [`Summary::compute`] calls this, so there is exactly one implementation of
/// the arithmetic and the two paths cannot drift.
#[must_use]
pub fn sharpe_and_shape(
    equity: &[(Ts, NanoUsd)],
    bars_per_year: f64,
) -> (Option<f64>, ReturnShape) {
    // Per-bar simple returns for the naive Sharpe.
    let mut rets = Vec::with_capacity(equity.len().saturating_sub(1));
    for w in equity.windows(2) {
        let prev = nano_usd_to_f64(w[0].1);
        let curr = nano_usd_to_f64(w[1].1);
        if prev > 0.0 {
            rets.push(curr / prev - 1.0);
        }
    }
    let sharpe_naive = sharpe(&rets, bars_per_year);
    let return_shape = ReturnShape::of(&rets);
    (sharpe_naive, return_shape)
}

/// **Sufficient statistics** for a run's out-of-sample return series, so a
/// pooled statistic can be computed without retaining the series.
///
/// # Why this exists rather than a retained curve
///
/// Pooling across contracts needs the moments of the CONCATENATED series, and
/// the obvious way to get them is to keep each contract's curve and join them.
/// That is D-0071's refused case, not an analogy to it: a stitched curve is one
/// `(Ts, NanoUsd)` per bar, so ~40 out-of-sample sessions at 1m is ~880 KB per
/// combo per contract, and a 4,000-combo grid is ~3.5 GB for a single contract.
/// D-0071 refused a retained per-bar series at 4.98 GiB and pinned the O(1)
/// reducer's size by test; this is the same trade, and
/// `a_return_stats_is_forty_eight_bytes` is the same pin.
///
/// # Sums, not averages
///
/// `m2`, `m3` and `m4` are Σ(r−μ)^k, **undivided**. [`sharpe`] wants
/// `Σ(r−μ)²/(n−1)` and [`ReturnShape`] wants `Σ(r−μ)²/n`, so storing the sum
/// lets both derive their own convention from one field. Storing either
/// average would force the other consumer to multiply back, and a stored value
/// whose divisor a reader has to remember is how two call sites end up
/// disagreeing about what it meant.
///
/// # Bit-identity is by construction
///
/// [`ReturnStats::shape`] divides by `n`, takes the square root and forms the
/// powers at exactly the points [`ReturnShape::of`] does, so the two agree
/// bit-for-bit rather than to a tolerance. That is asserted, not hoped for:
/// there are now two moment implementations and
/// `return_stats_and_return_shape_agree_bit_for_bit` is the guard against them
/// drifting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReturnStats {
    /// Returns the statistics were computed from.
    pub n: usize,
    /// Arithmetic mean of those returns.
    pub mean: f64,
    /// Σ(r−μ)² — undivided.
    pub m2: f64,
    /// Σ(r−μ)³ — undivided.
    pub m3: f64,
    /// Σ(r−μ)⁴ — undivided.
    pub m4: f64,
    /// Final minus initial equity, in nanodollars. Integer, so a pooled total
    /// return carries no float error at all: the stitch is delta-based and
    /// every contract starts from the same capital, so the pooled final is
    /// `initial + Σ deltas`.
    pub net_delta_nano_usd: NanoUsd,
}

impl ReturnStats {
    /// Reduces an equity curve to its sufficient statistics in one pass over
    /// the returns, retaining nothing per-bar.
    ///
    /// The return series is built exactly as [`sharpe_and_shape`] builds it,
    /// including the `prev > 0.0` guard that drops steps out of an insolvent
    /// prefix (D-0076).
    #[must_use]
    pub fn of(equity: &[(Ts, NanoUsd)]) -> ReturnStats {
        let mut rets = Vec::with_capacity(equity.len().saturating_sub(1));
        for w in equity.windows(2) {
            let prev = nano_usd_to_f64(w[0].1);
            let curr = nano_usd_to_f64(w[1].1);
            if prev > 0.0 {
                rets.push(curr / prev - 1.0);
            }
        }
        let net_delta_nano_usd = match (equity.first(), equity.last()) {
            (Some(&(_, a)), Some(&(_, b))) => b - a,
            _ => 0,
        };
        let (n, mean, m2, m3, m4) = moments_of(&rets);
        ReturnStats {
            n,
            mean,
            m2,
            m3,
            m4,
            net_delta_nano_usd,
        }
    }

    /// The statistics of the two samples' return series **concatenated**,
    /// computed from the two summaries alone.
    ///
    /// # What is concatenated, and what is not
    ///
    /// The *return series*, never the equity curves. Joining two contracts'
    /// curves end to end would manufacture one extra return across the seam —
    /// the step from contract A's last equity to contract B's first — and that
    /// step is not a trade, not a mark, and not a number any strategy could
    /// have earned. It is the same seam `RunTrace::pooled_with_stats` already
    /// refuses to double-count between folds, one level up.
    ///
    /// `net_delta_nano_usd` is added, always, and in integer arithmetic — so a
    /// pooled *total return* carries no float error whatever this function does
    /// to the moments. It is summed even when a sample contributes no returns,
    /// because a curve can have a delta and no usable return step (D-0076's
    /// insolvent prefix).
    ///
    /// # The arithmetic, and its citation
    ///
    /// Pébay's pairwise update for arbitrary-order moments (Sandia report
    /// SAND2008-6212, 2008), whose `M2` term is Chan, Golub & LeVeque's (1979)
    /// parallel variance formula. Written out rather than expressed with
    /// `powi`, per D-0126.
    ///
    /// # Not bit-identical to a single pass, and not associative
    ///
    /// Combining two halves and reducing the whole in one pass are different
    /// sequences of floating-point operations, so they agree to a tolerance
    /// rather than to the bit — and `combine(combine(a, b), c)` may differ from
    /// `combine(a, combine(b, c))` in the last bits for the same reason. That
    /// does **not** weaken §2.2: determinism requires the same config and data
    /// to give the same answer, not that the answer equal some other route's.
    /// What it does require is that the fold order be **fixed and part of the
    /// definition** — a caller pooling N contracts must fold them in a declared
    /// order (contract order), never in completion order.
    #[must_use]
    pub fn combine(a: ReturnStats, b: ReturnStats) -> ReturnStats {
        let net_delta_nano_usd = a.net_delta_nano_usd + b.net_delta_nano_usd;
        // An empty sample contributes no moments, and returning the other side
        // unchanged is exact rather than merely close: the general formulas
        // below divide by `n`, which is zero when both are empty.
        if a.n == 0 {
            return ReturnStats {
                net_delta_nano_usd,
                ..b
            };
        }
        if b.n == 0 {
            return ReturnStats {
                net_delta_nano_usd,
                ..a
            };
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "bar counts fit f64 exactly at our scales"
        )]
        let (na, nb) = (a.n as f64, b.n as f64);
        let n = na + nb;
        let d = b.mean - a.mean;
        let mean = a.mean + d * nb / n;
        let m2 = a.m2 + b.m2 + d * d * na * nb / n;
        let m3 = a.m3
            + b.m3
            + d * d * d * na * nb * (na - nb) / (n * n)
            + 3.0 * d * (na * b.m2 - nb * a.m2) / n;
        let m4 = a.m4
            + b.m4
            + d * d * d * d * na * nb * (na * na - na * nb + nb * nb) / (n * n * n)
            + 6.0 * d * d * (na * na * b.m2 + nb * nb * a.m2) / (n * n)
            + 4.0 * d * (na * b.m3 - nb * a.m3) / n;
        ReturnStats {
            n: a.n + b.n,
            mean,
            m2,
            m3,
            m4,
            net_delta_nano_usd,
        }
    }

    /// The [`ReturnShape`] these statistics describe.
    ///
    /// Divides by `n`, takes the square root and forms the powers at exactly
    /// the points [`ReturnShape::of`] does, which is what makes the two
    /// bit-identical rather than merely close. The `< 2` and flat-window guards
    /// are reproduced for the same reason.
    #[must_use]
    pub fn shape(&self) -> ReturnShape {
        if self.n < 2 {
            return ReturnShape {
                n_returns: self.n,
                skew: None,
                kurtosis: None,
            };
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "bar counts fit f64 exactly at our scales"
        )]
        let n = self.n as f64;
        let m2 = self.m2 / n;
        if m2 <= 0.0 {
            return ReturnShape {
                n_returns: self.n,
                skew: None,
                kurtosis: None,
            };
        }
        let m3 = self.m3 / n;
        let m4 = self.m4 / n;
        let sd = m2.sqrt();
        ReturnShape {
            n_returns: self.n,
            skew: Some(m3 / (sd * sd * sd)),
            kurtosis: Some(m4 / (m2 * m2)),
        }
    }

    /// The naive annualized Sharpe these statistics describe.
    ///
    /// The sibling of [`Self::shape`], and bit-identical to the ratio
    /// [`sharpe_and_shape`] computes on the same series for the same reason:
    /// `mean` and `m2` were accumulated by the very expressions that function's
    /// `sharpe` uses, and the divide, the square root and the ratio are formed
    /// here at exactly the points it forms them. `Σ(r−μ)²` is divided by
    /// `n − 1` — the sample convention — while [`Self::shape`] divides the same
    /// field by `n`, which is precisely why the field is stored undivided.
    ///
    /// `None` on fewer than two returns, and `None` on a series with no
    /// deviation, where the ratio is undefined rather than infinite. Both
    /// guards are reproduced from `sharpe` rather than reinvented, and
    /// `return_stats_and_sharpe_and_shape_agree_bit_for_bit` is what stops the
    /// two from drifting.
    ///
    /// # Why this exists
    ///
    /// A series pooled across contracts by [`Self::combine`] has no equity
    /// curve left to hand to [`sharpe_and_shape`] — not retaining the curve is
    /// the whole reason this reducer exists. Without a Sharpe readable from the
    /// statistics, a caller pooling N contracts would be left averaging the
    /// contracts' *own* Sharpes, which is not the Sharpe of the pooled series
    /// and is not a statistic at all: it is the mean of N ratios with N
    /// different denominators, and it can sit above every one of them.
    /// `the_pooled_sharpe_is_not_the_mean_of_the_contracts_sharpes` measures
    /// that gap rather than asserting it.
    #[must_use]
    pub fn sharpe(&self, periods_per_year: f64) -> Option<f64> {
        if self.n < 2 {
            return None;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "bar counts fit f64 exactly at our scales"
        )]
        let n = self.n as f64;
        let var = self.m2 / (n - 1.0);
        let sd = var.sqrt();
        if sd == 0.0 {
            return None;
        }
        Some(self.mean / sd * periods_per_year.sqrt())
    }
}

/// `(n, mean, Σ(r−μ)², Σ(r−μ)³, Σ(r−μ)⁴)` in one pass over `rets`.
///
/// Lifted out of [`ReturnStats::of`] verbatim so that the two-route control on
/// [`ReturnStats::combine`] can compare the combine against **this** pass
/// rather than against a second implementation written inside the test. A test
/// that reimplements the thing it checks is checking the reimplementation.
///
/// Same order as [`ReturnShape::of`]: mean first, then each power summed over
/// the same iteration, written out per D-0122/D-0126.
fn moments_of(rets: &[f64]) -> (usize, f64, f64, f64, f64) {
    if rets.is_empty() {
        return (0, 0.0, 0.0, 0.0, 0.0);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "bar counts fit f64 exactly at our scales"
    )]
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    let m2 = rets
        .iter()
        .map(|r| {
            let d = r - mean;
            d * d
        })
        .sum::<f64>();
    let m3 = rets
        .iter()
        .map(|r| {
            let d = r - mean;
            d * d * d
        })
        .sum::<f64>();
    let m4 = rets
        .iter()
        .map(|r| {
            let d = r - mean;
            let d2 = d * d;
            d2 * d2
        })
        .sum::<f64>();
    (rets.len(), mean, m2, m3, m4)
}

/// The shape of the return series a naive Sharpe was computed on.
///
/// Retained because a Sharpe alone cannot be deflated: Bailey & López de
/// Prado's correction needs the sample size and the third and fourth moments
/// of the *same* series the ratio came from. Recomputing them later from a
/// different series — daily marks instead of per-bar, say — would deflate one
/// number using another number's shape, which is the kind of quiet mismatch
/// this project treats as a wrong result rather than an approximation.
///
/// Computed in the pass that already materializes the returns, so it costs a
/// second traversal of a vector that exists either way and no extra memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReturnShape {
    /// Returns the ratio was computed from: `equity.len() - 1` less any step
    /// whose starting equity was not positive (D-0076's insolvent prefix).
    pub n_returns: usize,
    /// Fisher skewness. `None` when fewer than two returns, or when the
    /// series has zero variance and the moment is undefined rather than zero.
    pub skew: Option<f64>,
    /// **Non-excess** kurtosis: 3.0 for a normal distribution, matching
    /// `crucible-funnel::stats::deflated`'s convention. `None` on the same
    /// terms as `skew`.
    pub kurtosis: Option<f64>,
}

impl ReturnShape {
    /// Sample size and standardized third and fourth moments of `rets`.
    ///
    /// Population moments (divide by `n`), not sample moments (divide by
    /// `n - 1`), matching the Bailey & López de Prado formulation the deflated
    /// Sharpe uses. At the sample sizes a backtest produces the difference is
    /// far below the estimator's own error, but the convention is stated
    /// because a reader comparing against a textbook needs to know which one.
    #[must_use]
    pub fn of(rets: &[f64]) -> ReturnShape {
        if rets.len() < 2 {
            return ReturnShape {
                n_returns: rets.len(),
                skew: None,
                kurtosis: None,
            };
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "bar counts fit f64 exactly at our scales"
        )]
        let n = rets.len() as f64;
        let mean = rets.iter().sum::<f64>() / n;
        // `x*x`, not `powi(2)` (D-0126). Squaring has one association, so there is
        // nothing to choose here — but `powi(2)` still diverges by one ULP on 725 of
        // 1,512,658 dev samples, because LLVM's -O0 expansion is not a multiply.
        let m2 = rets
            .iter()
            .map(|r| {
                let d = r - mean;
                d * d
            })
            .sum::<f64>()
            / n;
        // A flat window has no third or fourth standardized moment: the
        // denominator is zero and every candidate answer is a fabrication.
        // `None` is what this codebase already means by "no opinion" (D-0080),
        // and a zero here would read as "symmetric, normal-tailed".
        if m2 <= 0.0 {
            return ReturnShape {
                n_returns: rets.len(),
                skew: None,
                kurtosis: None,
            };
        }
        // Powers written out, not `powi` (D-0122). `f64::powi` lowers to
        // `llvm.powi`, whose expansion is chosen per optimisation level, and
        // float multiplication is not associative — so `powi(3)` and `powi(4)`
        // gave different answers in `dev` and `release`. Measured over
        // 1,512,658 sampled inputs: `powi(3)` differs from `d*d*d` on 390,842
        // of them (25.8 %) in dev and on none in release.
        //
        // The FOURTH power is BALANCED, `(d*d)*(d*d)`, not `((d*d)*d)*d`, and
        // that is measured rather than chosen for symmetry: release's
        // expansion equals the balanced form on all 1,512,658 samples and the
        // linear form on only 989,165. Writing what release already computes
        // is what keeps every pinned value where it is.
        let m3 = rets
            .iter()
            .map(|r| {
                let d = r - mean;
                d * d * d
            })
            .sum::<f64>()
            / n;
        let m4 = rets
            .iter()
            .map(|r| {
                let d = r - mean;
                let d2 = d * d;
                d2 * d2
            })
            .sum::<f64>()
            / n;
        let sd = m2.sqrt();
        ReturnShape {
            n_returns: rets.len(),
            // Written out for D-0122's reason, exactly as above: this is the
            // third power in the *derivation* rather than the accumulation,
            // and it is no safer for being one line instead of a fold.
            skew: Some(m3 / (sd * sd * sd)),
            // `m2 * m2`, not `powi(2)` (D-0126).
            kurtosis: Some(m4 / (m2 * m2)),
        }
    }
}

fn sharpe(rets: &[f64], periods_per_year: f64) -> Option<f64> {
    if rets.len() < 2 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "bar counts fit f64 exactly at our scales"
    )]
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    // `d * d`, not `powi(2)` (D-0126).
    let var = rets
        .iter()
        .map(|r| {
            let d = r - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1.0);
    let sd = var.sqrt();
    if sd == 0.0 {
        return None;
    }
    Some(mean / sd * periods_per_year.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A curve whose per-bar returns vary, so the third and fourth moments
    /// exist. Deliberately not a straight line: on constant returns both
    /// implementations return `None` and any agreement test passes vacuously.
    fn skewed_curve() -> Vec<(Ts, NanoUsd)> {
        let steps: [i64; 12] = [
            1_000_000, -300_000, 250_000, -120_000, 900_000, -50_000, 70_000, -640_000, 30_000,
            410_000, -220_000, 1_800_000,
        ];
        let mut level: NanoUsd = 1_000_000_000_000;
        let mut out = vec![(Ts(0), level)];
        for (i, s) in steps.iter().enumerate() {
            level += s;
            out.push((Ts(i as i64 + 1), level));
        }
        out
    }

    /// **The drift guard.** Two moment implementations now exist —
    /// `ReturnShape::of` and `ReturnStats::shape` — and this asserts they agree
    /// BIT-FOR-BIT, not to a tolerance, because both perform the same
    /// operations in the same order (D-0126 having made both sides written-out
    /// forms; before that this could only have been a one-ULP tolerance that
    /// passed in release and failed in dev).
    ///
    /// What would this do if the thing it looks for were absent? On a constant
    /// return series both sides yield `None` and every comparison below would
    /// hold while proving nothing — so the fixture is asserted to produce
    /// `Some` first. That assertion is the control on the control.
    #[test]
    fn return_stats_and_return_shape_agree_bit_for_bit() {
        let eq = skewed_curve();
        let (_, direct) = sharpe_and_shape(&eq, 252.0);
        let derived = ReturnStats::of(&eq).shape();

        assert!(
            direct.skew.is_some() && direct.kurtosis.is_some(),
            "the fixture must produce real moments, or this test is vacuous"
        );
        assert_eq!(derived.n_returns, direct.n_returns);
        assert_eq!(
            derived.skew.map(f64::to_bits),
            direct.skew.map(f64::to_bits),
            "skew must agree bit-for-bit: {:?} vs {:?}",
            derived.skew,
            direct.skew
        );
        assert_eq!(
            derived.kurtosis.map(f64::to_bits),
            direct.kurtosis.map(f64::to_bits),
            "kurtosis must agree bit-for-bit: {:?} vs {:?}",
            derived.kurtosis,
            direct.kurtosis
        );
    }

    /// The flat window still agrees, and is a SEPARATE test rather than folded
    /// into the one above — there it would be the degenerate case that hides a
    /// real disagreement, here it is the behaviour under test.
    #[test]
    fn both_moment_paths_report_no_opinion_on_a_flat_window() {
        let eq: Vec<(Ts, NanoUsd)> = (0..8).map(|i| (Ts(i), 1_000_000_000_000)).collect();
        let (_, direct) = sharpe_and_shape(&eq, 252.0);
        let derived = ReturnStats::of(&eq).shape();
        assert!(direct.skew.is_none() && direct.kurtosis.is_none());
        assert_eq!(derived.skew, direct.skew);
        assert_eq!(derived.kurtosis, direct.kurtosis);
    }

    /// The net delta is exact integer arithmetic, so a pooled total return
    /// carries no float error.
    #[test]
    fn the_net_delta_is_the_exact_integer_difference() {
        let eq = skewed_curve();
        let stats = ReturnStats::of(&eq);
        assert_eq!(
            stats.net_delta_nano_usd,
            eq.last().expect("nonempty").1 - eq.first().expect("nonempty").1
        );
        assert_ne!(
            stats.net_delta_nano_usd, 0,
            "the fixture must actually move"
        );
    }

    /// Moments from a raw return series, for the combine controls below.
    fn from_returns(rets: &[f64]) -> ReturnStats {
        let (n, mean, m2, m3, m4) = super::moments_of(rets);
        ReturnStats {
            n,
            mean,
            m2,
            m3,
            m4,
            net_delta_nano_usd: 0,
        }
    }

    /// Two samples that genuinely differ, so a combine that ignored one of them
    /// could not pass. Different lengths, different means, different spreads.
    fn halves() -> (Vec<f64>, Vec<f64>) {
        let a = vec![0.004, -0.001, 0.0025, -0.0035, 0.006, 0.0005, -0.002];
        let b = vec![-0.011, 0.017, -0.003, 0.0085, -0.0125, 0.021];
        (a, b)
    }

    /// **The two-route agreement control.** Combining two halves must reproduce
    /// one pass over the whole — to a tolerance, never to the bit, because the
    /// two are different sequences of floating-point operations.
    ///
    /// What would this do if the thing it looks for were absent? If the halves
    /// had equal means the `delta` correction terms would all vanish and a
    /// combine that was nothing but `a.m2 + b.m2` would pass — so the fixture's
    /// means are asserted to differ, and to differ by more than the tolerance
    /// being applied. That assertion is the control on the control.
    #[test]
    fn combining_two_halves_reproduces_one_pass_over_the_whole() {
        let (a, b) = halves();
        let whole: Vec<f64> = a.iter().chain(b.iter()).copied().collect();

        let sa = from_returns(&a);
        let sb = from_returns(&b);
        assert!(
            (sa.mean - sb.mean).abs() > 1e-4,
            "the halves must have genuinely different means, or the correction \
             terms are all zero and this test cannot see them"
        );

        let combined = ReturnStats::combine(sa, sb);
        let direct = from_returns(&whole);

        assert_eq!(combined.n, direct.n);
        let close = |got: f64, want: f64, tol: f64, what: &str| {
            let rel = (got - want).abs() / want.abs();
            assert!(rel < tol, "{what}: {got} vs {want}, relative {rel:e}");
        };
        close(combined.mean, direct.mean, 1e-12, "mean");
        close(combined.m2, direct.m2, 1e-12, "m2");
        close(combined.m3, direct.m3, 1e-10, "m3");
        close(combined.m4, direct.m4, 1e-10, "m4");
    }

    /// **The converse.** The tolerances above are only evidence if a *wrong*
    /// combine would exceed them. Naively adding the two halves' moments —
    /// dropping every mean-difference correction, which is the mistake the
    /// formulas exist to prevent — must be caught by the same comparison.
    #[test]
    fn dropping_the_mean_correction_is_caught_by_those_tolerances() {
        let (a, b) = halves();
        let whole: Vec<f64> = a.iter().chain(b.iter()).copied().collect();
        let (sa, sb, direct) = (from_returns(&a), from_returns(&b), from_returns(&whole));

        let naive_m2 = sa.m2 + sb.m2;
        let rel = (naive_m2 - direct.m2).abs() / direct.m2.abs();
        assert!(
            rel > 1e-12,
            "a combine that ignored the mean difference would have to be caught \
             by the m2 tolerance, and here it is off by only {rel:e}"
        );
    }

    /// An empty operand contributes no moments and is EXACT, not merely close —
    /// the general formulas divide by `n`. The delta is still summed, because a
    /// curve can carry one and yield no usable return step (D-0076).
    #[test]
    fn an_empty_sample_leaves_the_moments_alone_but_still_adds_its_delta() {
        let (a, _) = halves();
        let full = ReturnStats {
            net_delta_nano_usd: 7_000_000_000,
            ..from_returns(&a)
        };
        let empty = ReturnStats {
            n: 0,
            mean: 0.0,
            m2: 0.0,
            m3: 0.0,
            m4: 0.0,
            net_delta_nano_usd: -2_000_000_000,
        };

        for combined in [
            ReturnStats::combine(full, empty),
            ReturnStats::combine(empty, full),
        ] {
            assert_eq!(combined.n, full.n);
            assert_eq!(combined.mean.to_bits(), full.mean.to_bits());
            assert_eq!(combined.m2.to_bits(), full.m2.to_bits());
            assert_eq!(combined.m3.to_bits(), full.m3.to_bits());
            assert_eq!(combined.m4.to_bits(), full.m4.to_bits());
            assert_eq!(combined.net_delta_nano_usd, 5_000_000_000);
        }

        // Two empties are still a legal question, and must not divide by zero.
        let both = ReturnStats::combine(empty, empty);
        assert_eq!(both.n, 0);
        assert!(both.mean.is_finite() && both.m2.is_finite());
        assert_eq!(both.net_delta_nano_usd, -4_000_000_000);
    }

    /// The pooled TOTAL RETURN is exact however the moments round, because the
    /// delta is integer — which is the whole reason the field is an `i64` and
    /// not an `f64` (§2.3).
    #[test]
    fn the_combined_delta_is_exact_integer_arithmetic() {
        let (a, b) = halves();
        let sa = ReturnStats {
            net_delta_nano_usd: 123_456_789_012,
            ..from_returns(&a)
        };
        let sb = ReturnStats {
            net_delta_nano_usd: -23_456_789_013,
            ..from_returns(&b)
        };
        assert_eq!(
            ReturnStats::combine(sa, sb).net_delta_nano_usd,
            99_999_999_999
        );
    }

    /// **The drift guard for the Sharpe**, the sibling of
    /// `return_stats_and_return_shape_agree_bit_for_bit` and asserted on the
    /// same terms: BIT-for-bit, because both sides accumulate `mean` and
    /// `Σ(r−μ)²` with the same written-out expressions and then divide, root
    /// and ratio at the same points. A tolerance here would let a genuinely
    /// different formula through.
    ///
    /// What would this do if the thing it looks for were absent? On a series
    /// with no deviation both sides return `None` and every comparison would
    /// hold vacuously — so the fixture is asserted to produce `Some` first, and
    /// to produce a ratio that is not zero. That assertion is the control on
    /// the control.
    #[test]
    fn return_stats_and_sharpe_and_shape_agree_bit_for_bit() {
        let curve = skewed_curve();
        let (direct, _) = sharpe_and_shape(&curve, 252.0);
        let derived = ReturnStats::of(&curve).sharpe(252.0);

        let value = direct.expect("the fixture must produce a real ratio, or this test is vacuous");
        assert!(
            value != 0.0 && value.is_finite(),
            "a zero or non-finite fixture ratio would agree with almost any \
             implementation: {value}"
        );
        assert_eq!(
            derived.map(f64::to_bits),
            direct.map(f64::to_bits),
            "the Sharpe read off the statistics must equal the one computed from \
             the series: {derived:?} vs {direct:?}"
        );
    }

    /// Both guards, and they are a SEPARATE test rather than folded into the
    /// one above, where a degenerate series would hide a real disagreement
    /// instead of being the behaviour under test.
    #[test]
    fn a_flat_or_single_step_series_has_no_sharpe_on_either_path() {
        let flat: Vec<(Ts, NanoUsd)> = (0..8).map(|i| (Ts(i), 1_000_000_000_000)).collect();
        let (direct, _) = sharpe_and_shape(&flat, 252.0);
        assert!(direct.is_none(), "a flat series has no ratio");
        assert!(ReturnStats::of(&flat).sharpe(252.0).is_none());

        // One return is not a sample: `n < 2` short-circuits before the
        // divide by `n - 1`, which would otherwise be a divide by zero.
        let one_step = vec![(Ts(0), 1_000_000_000_000), (Ts(1), 1_000_000_100_000)];
        let (direct, _) = sharpe_and_shape(&one_step, 252.0);
        assert!(direct.is_none());
        assert!(ReturnStats::of(&one_step).sharpe(252.0).is_none());
    }

    /// **What the pooled reader is FOR**, measured rather than asserted.
    ///
    /// The tempting shortcut for a caller pooling N contracts is to average the
    /// contracts' own Sharpes. That is the mean of N ratios with N different
    /// denominators, and it is not the Sharpe of the pooled series. This
    /// measures the gap on two samples that differ in length, mean and spread
    /// — 0.212 pooled against 0.253 averaged, a relative 16 % — so a future
    /// implementation that quietly averaged could not pass.
    ///
    /// Annualization is 1.0 so the numbers below are per-observation ratios and
    /// the comparison is not scaled by a factor common to both sides.
    #[test]
    fn the_pooled_sharpe_is_not_the_mean_of_the_contracts_sharpes() {
        let (a, b) = halves();
        let (sa, sb) = (from_returns(&a), from_returns(&b));

        let ra = sa.sharpe(1.0).expect("half a has a ratio");
        let rb = sb.sharpe(1.0).expect("half b has a ratio");
        let pooled = ReturnStats::combine(sa, sb)
            .sharpe(1.0)
            .expect("the pool has a ratio");
        let averaged = (ra + rb) / 2.0;

        // The halves must genuinely disagree, or averaging and pooling would
        // coincide and this test would prove nothing.
        assert!(
            (ra - rb).abs() > 0.01,
            "the halves' own ratios must differ: {ra} vs {rb}"
        );
        let gap = (pooled - averaged).abs() / averaged.abs();
        assert!(
            gap > 0.1,
            "pooling and averaging must be distinguishable here: pooled {pooled}, \
             averaged {averaged}, relative gap {gap:e}"
        );
    }

    /// **The size pin**, in `a_day_record_is_fifty_six_bytes`' shape (D-0071).
    ///
    /// The whole reason this type exists is that retaining the curve costs
    /// ~880 KB per combo per contract. Growing it back should be a failing test
    /// rather than a memory regression nobody measures.
    #[test]
    fn a_return_stats_is_forty_eight_bytes() {
        assert_eq!(
            std::mem::size_of::<ReturnStats>(),
            48,
            "ReturnStats is the O(1) reducer that replaced a retained per-bar              curve (D-0071's trade). If this grew, say why in a decision entry."
        );
    }

    fn eq(points: &[i64]) -> Vec<(Ts, NanoUsd)> {
        points
            .iter()
            .enumerate()
            .map(|(i, &e)| (Ts(i as i64), e * 1_000_000_000))
            .collect()
    }

    fn trades(nets: &[NanoUsd]) -> Vec<ClosedTrade> {
        nets.iter()
            .enumerate()
            .map(|(i, &net)| ClosedTrade {
                closed_ts: Ts(i as i64),
                net_nano_usd: net,
                direction: Side::Buy,
                opened_ts: Ts(i as i64),
                mae_nano_usd: net.min(0),
                mfe_nano_usd: net.max(0),
            })
            .collect()
    }

    #[test]
    fn drawdown_known_series() {
        // 100 -> 120 -> 90 -> 130: max dd = (120-90)/120 = 25%
        let s = Summary::compute(&eq(&[100, 120, 90, 130]), &[], 0, 1.0);
        assert!((s.max_drawdown_pct - 25.0).abs() < 1e-9);
        assert!((s.total_return_pct - 30.0).abs() < 1e-9);
    }

    #[test]
    fn win_rate_counts_positive_net_trades() {
        let s = Summary::compute(&eq(&[100, 104]), &trades(&[5, -3, 2, 0]), 0, 1.0);
        assert_eq!(s.round_trips, 4);
        let wr = s.win_rate.expect("has trades");
        assert!((wr - 0.5).abs() < 1e-9); // 2 of 4 (zero is not a win)
    }

    #[test]
    fn sharpe_none_on_flat_curve() {
        let s = Summary::compute(&eq(&[100, 100, 100]), &[], 0, 252.0);
        assert!(s.sharpe_naive.is_none());
    }

    // Hand-derived. rets = [1.0, 2.0, 3.0, 4.0]; mean = 2.5; deviations
    // -1.5, -0.5, 0.5, 1.5.
    //   m2 = (2.25 + 0.25 + 0.25 + 2.25) / 4 = 1.25
    //   m3 = (-3.375 - 0.125 + 0.125 + 3.375) / 4 = 0      (symmetric)
    //   m4 = (5.0625 + 0.0625 + 0.0625 + 5.0625) / 4 = 2.5625
    //   skew     = m3 / m2^1.5 = 0
    //   kurtosis = m4 / m2^2   = 2.5625 / 1.5625 = 1.64
    // 1.64 is well below 3.0, which is right: four evenly spaced points are
    // flatter-tailed than a normal, and a platykurtic reading here is the
    // check that the convention really is non-excess.
    #[test]
    fn return_shape_moments_match_hand_arithmetic_on_a_symmetric_series() {
        let shape = ReturnShape::of(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(shape.n_returns, 4);
        assert_eq!(shape.skew, Some(0.0));
        assert_eq!(shape.kurtosis, Some(1.64));
    }

    // Hand-derived. rets = [0.0, 0.0, 0.0, 4.0]; mean = 1.0; deviations
    // -1, -1, -1, 3.
    //   m2 = (1 + 1 + 1 + 9) / 4 = 3
    //   m3 = (-1 - 1 - 1 + 27) / 4 = 6
    //   m4 = (1 + 1 + 1 + 81) / 4 = 21
    //   skew     = 6 / 3^1.5 = 2 / sqrt(3) = 1.1547005383792515
    //   kurtosis = 21 / 9    = 2.3333333333333335
    // The companion to the case above: one right tail must produce a positive
    // skew, so a sign error cannot hide behind a symmetric fixture.
    #[test]
    fn return_shape_reports_a_positive_skew_for_a_right_tail() {
        let shape = ReturnShape::of(&[0.0, 0.0, 0.0, 4.0]);
        assert_eq!(shape.n_returns, 4);
        let skew = shape.skew.expect("a series with variance has a skew");
        assert!(
            (skew - 2.0 / 3.0_f64.sqrt()).abs() < 1e-12,
            "skew was {skew}"
        );
        let kurtosis = shape.kurtosis.expect("a series with variance has one");
        assert!(
            (kurtosis - 21.0 / 9.0).abs() < 1e-12,
            "kurtosis was {kurtosis}"
        );
    }

    // A flat window has no standardized third or fourth moment: the
    // denominator is zero. `None` is "no opinion" (D-0080); a zero would read
    // as "symmetric and normal-tailed", which is a claim the data cannot make.
    #[test]
    fn a_flat_or_too_short_series_has_no_higher_moments() {
        let flat = ReturnShape::of(&[0.25, 0.25, 0.25]);
        assert_eq!(flat.n_returns, 3);
        assert_eq!(flat.skew, None);
        assert_eq!(flat.kurtosis, None);

        for short in [vec![], vec![0.5]] {
            let shape = ReturnShape::of(&short);
            assert_eq!(shape.n_returns, short.len());
            assert_eq!(shape.skew, None);
            assert_eq!(shape.kurtosis, None);
        }
    }

    // The shape must describe the SAME series the ratio came from, which is
    // what makes it safe to deflate with. `compute` drops a step whose
    // starting equity was not positive, so the count follows the ratio's
    // sample rather than the equity length.
    #[test]
    fn the_shape_counts_the_same_returns_the_naive_sharpe_used() {
        let s = Summary::compute(&eq(&[100, 110, 120, 130]), &[], 0, 252.0);
        assert_eq!(s.return_shape.n_returns, 3);
        assert!(s.sharpe_naive.is_some());

        let flat = Summary::compute(&eq(&[100, 100, 100]), &[], 0, 252.0);
        assert!(flat.sharpe_naive.is_none());
        assert_eq!(flat.return_shape.n_returns, 2);
        assert_eq!(flat.return_shape.skew, None);
    }
}
