//! Deflated Sharpe Ratio — Bailey & López de Prado (2014).
//!
//! > Bailey, D. H. and López de Prado, M. (2014). "The Deflated Sharpe Ratio:
//! > Correcting for Selection Bias, Backtest Overfitting and Non-Normality."
//! > *Journal of Portfolio Management*, 40(5), 94–107.
//!
//! ## The question it answers
//!
//! A grid search reports its best Sharpe. That number is the **maximum of N
//! draws**, and the maximum of N draws from a zero-edge distribution is
//! positive and grows with N. So "Sharpe 1.4" means something very different
//! after 1 trial than after 240, and reporting it without its trial count is
//! not a rounding error — it is the single most common way a backtest lies.
//!
//! The deflated Sharpe converts an observed Sharpe into a **probability**: given
//! that we ran `n_trials` searches, that returns have this skew and kurtosis,
//! and that the sample is this long, what is the chance the true Sharpe is
//! greater than zero? A DSR of 0.95 is a claim; a DSR of 0.4 is a coin flip
//! dressed as a discovery.
//!
//! ## Two corrections, and they are separate
//!
//! 1. **Selection**: the benchmark is not zero, it is
//!    [`expected_max_sharpe`] — where the maximum of `n_trials` zero-edge
//!    trials would be expected to land. This is the multiple-testing
//!    correction, and it is why the trial count comes from the registry
//!    (D-0083) rather than from anybody's memory.
//! 2. **Non-normality**: the standard error of a Sharpe estimate depends on the
//!    skew and kurtosis of the returns that produced it. Negatively skewed,
//!    fat-tailed returns — every option-selling and mean-reversion strategy
//!    ever built — make a Sharpe *less* certain than the normal formula says.
//!
//! Both push the same way for the strategies most likely to be self-deception,
//! which is the point.
//!
//! ## What is deliberately NOT done here
//!
//! No p-value is manufactured from a distribution nobody checked. The DSR is a
//! probability under an explicitly normal approximation to the Sharpe's
//! sampling distribution, and that assumption is stated on the scorecard beside
//! the number rather than buried here. The block-permutation null (D-0087) is
//! the empirical answer and remains the one that can actually kill a strategy.

use crate::registry::Registry;

/// Euler–Mascheroni constant, for the expected maximum of `n` normal draws.
///
/// Written out rather than derived: it appears in one formula and a reader
/// should be able to check it against the paper without running anything.
const EULER_MASCHERONI: f64 = 0.577_215_664_901_532_9;

/// Where the maximum of `n_trials` zero-edge trials is expected to land,
/// **as a z-score** — dimensionless, in units of the trial Sharpes' own
/// dispersion.
///
/// Bailey & López de Prado's equation (as in the paper's §3): for `N`
/// independent draws from a standard normal, the expected maximum is
/// approximately
///
/// ```text
/// (1 - γ)·Z⁻¹(1 - 1/N)  +  γ·Z⁻¹(1 - 1/(N·e))
/// ```
///
/// where γ is Euler–Mascheroni and `Z⁻¹` is the inverse standard normal CDF.
///
/// **It is a z-score and not a Sharpe**, and conflating the two is a real bug
/// that this module's converse control caught: multiplying by a dispersion is
/// what turns it into the benchmark [`expected_max_sharpe`] returns. Comparing
/// this number directly against an observed Sharpe compares a z-score to a
/// ratio, and condemns every strategy ever run.
///
/// Its growth with `N` is the whole of the multiple-testing correction: one
/// trial expects 0, and 240 trials expect roughly 2.9 dispersions *from noise
/// alone*.
///
/// # Panics
/// Never. `n_trials <= 1` returns 0.0 — no search, no selection bias — and the
/// quantile arguments are clamped away from the tails where
/// [`inverse_normal_cdf`] stops being accurate.
#[must_use]
pub fn expected_max_z(n_trials: usize) -> f64 {
    // Zero or one trial is no selection at all: the benchmark is zero, and the
    // deflated Sharpe reduces to an ordinary significance test. Returning
    // anything else here would deflate a strategy nobody searched for.
    if n_trials <= 1 {
        return 0.0;
    }
    let n = n_trials as f64;
    let first = inverse_normal_cdf(1.0 - 1.0 / n);
    let second = inverse_normal_cdf(1.0 - 1.0 / (n * std::f64::consts::E));
    (1.0 - EULER_MASCHERONI) * first + EULER_MASCHERONI * second
}

/// Everything a deflated Sharpe needs, kept together so no caller can supply
/// half of it.
///
/// Grouped for the reason `Counts` is grouped in the telemetry module: an
/// observed Sharpe means nothing without the trial count it was selected from,
/// and a signature that let one arrive without the other would be an invitation
/// to the exact mistake this module exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeflationInputs {
    /// The observed Sharpe of the selected strategy, already annualized or not
    /// — the ratio is scale-free as long as `n_observations` matches it.
    pub observed_sharpe: f64,
    /// Skewness of the return series the Sharpe was computed on.
    pub skew: f64,
    /// **Non-excess** kurtosis: 3.0 for a normal distribution.
    ///
    /// Spelled out because the two conventions differ by exactly 3 and reading
    /// one as the other silently changes every standard error on the page.
    pub kurtosis: f64,
    /// How many return observations the Sharpe was computed from.
    pub n_observations: usize,
    /// How many trials were charged to the hypothesis family.
    ///
    /// From [`Registry::trials_for`], which excludes voided runs by
    /// construction (D-0083) — never a hand-entered number.
    pub n_trials: usize,
    /// Cross-trial dispersion of the Sharpe estimates, when it is known.
    ///
    /// The paper's benchmark is `√V · E[max z]`, where `V` is the **variance of
    /// the trial Sharpes** — the spread of the grid's own results. `None` falls
    /// back to the estimated standard error of this one Sharpe, which is the
    /// standard substitute when only the winner is in hand.
    ///
    /// The fallback is stated rather than silent because it is an assumption:
    /// it treats the grid's spread as if it were sampling noise around one
    /// strategy. Where the grid's combos really are near-duplicates — which is
    /// exactly the account dimension's case — the true dispersion is smaller,
    /// so the fallback deflates *harder* than the paper strictly requires. That
    /// is the accepted direction (the plan's "preferred error is against the
    /// strategy"), and it is recorded here so nobody discovers it as a surprise.
    pub trial_sharpe_dispersion: Option<f64>,
}

/// A deflated Sharpe and the pieces a reader needs to argue with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deflated {
    /// The observed Sharpe, carried through so the pair is never separated.
    pub observed_sharpe: f64,
    /// The selection benchmark: where noise alone would have put the maximum.
    pub expected_max_sharpe: f64,
    /// Estimated standard error of the observed Sharpe, skew- and
    /// kurtosis-adjusted.
    pub standard_error: f64,
    /// P(true Sharpe > 0), in `0..=1`.
    pub dsr: f64,
    /// The trial count used, echoed so a scorecard cannot print one without it.
    pub n_trials: usize,
}

impl Deflated {
    /// Whether the observed Sharpe clears the selection benchmark at all.
    ///
    /// A `false` here means the number would be unremarkable **from noise**
    /// given how many times it was searched for, whatever its absolute size.
    #[must_use]
    pub fn beats_selection_benchmark(&self) -> bool {
        self.observed_sharpe > self.expected_max_sharpe
    }
}

/// Computes the deflated Sharpe.
///
/// The estimator, following the paper:
///
/// ```text
///                ( SR_obs − SR_expected_max ) · √(T − 1)
/// DSR = Z( ────────────────────────────────────────────────── )
///            √( 1 − γ₃·SR_obs + (γ₄−1)/4 · SR_obs² )
/// ```
///
/// where `γ₃` is skew, `γ₄` is non-excess kurtosis and `T` is the observation
/// count. The denominator is the non-normality correction: negative skew
/// (`γ₃ < 0`) *increases* it, widening the error bar, which is the correct
/// direction — a strategy that makes money steadily and loses it in one day is
/// less certain than its Sharpe suggests.
///
/// Returns `None` when the inputs cannot support the statistic: fewer than two
/// observations, or a denominator that is not finite and positive. `None` is
/// "no opinion", never a zero that a scorecard would render as a verdict — the
/// same rule `zscore` follows on a flat window (D-0080).
#[must_use]
pub fn deflated_sharpe(inputs: DeflationInputs) -> Option<Deflated> {
    let DeflationInputs {
        observed_sharpe,
        skew,
        kurtosis,
        n_observations,
        n_trials,
        trial_sharpe_dispersion,
    } = inputs;
    if n_observations < 2 || !observed_sharpe.is_finite() {
        return None;
    }

    // The non-normality correction on the Sharpe's variance.
    // `observed_sharpe * observed_sharpe`, not `powi(2)` (D-0126). This site is
    // inside the estimator D-0122 was written to repair, and D-0122 left it.
    let variance =
        1.0 - skew * observed_sharpe + (kurtosis - 1.0) / 4.0 * (observed_sharpe * observed_sharpe);
    if !variance.is_finite() || variance <= 0.0 {
        return None;
    }
    let t = n_observations as f64;
    let standard_error = (variance / (t - 1.0)).sqrt();
    if !standard_error.is_finite() || standard_error <= 0.0 {
        return None;
    }

    // The benchmark is in SHARPE units: a dimensionless expected-maximum
    // z-score scaled by the dispersion of the trial Sharpes. Skipping the scale
    // compares a z-score against a ratio and condemns everything.
    let dispersion = trial_sharpe_dispersion
        .filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(standard_error);
    let benchmark = dispersion * expected_max_z(n_trials);

    let z = (observed_sharpe - benchmark) / standard_error;
    Some(Deflated {
        observed_sharpe,
        expected_max_sharpe: benchmark,
        standard_error,
        dsr: normal_cdf(z),
        n_trials,
    })
}

/// Reads the trial count for a family straight from the registry.
///
/// A one-line wrapper that exists to be the *only* way this module obtains a
/// trial count, so that "never trust a hand-entered trial count" is enforced by
/// there being no other door. [`Registry::trials_for`] excludes voided runs by
/// construction (D-0083); voiding a trial therefore *raises* the deflated
/// Sharpe, and `voiding_a_trial_raises_the_deflated_sharpe` is that assertion.
#[must_use]
pub fn trials_from_registry(registry: &Registry, family: &str) -> usize {
    registry.trials_for(family)
}

/// How a trial count is composed, so a scorecard never prints a bare integer.
///
/// §4 pins a trial as `(config_hash, account_id, combo_index)`, so **every
/// account is a trial** (D-0067). With Block E's 16 accounts a strategy's count
/// multiplies by up to sixteen and every deflated Sharpe drops accordingly.
/// That is correct and it must be a *stated* consequence rather than a surprise
/// — hence composition beside the total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialComposition {
    /// Points in the expanded parameter grid.
    pub combos: usize,
    /// Account configurations evaluated per combo.
    pub accounts: usize,
    /// Walk-forward folds per (combo, account).
    pub folds: usize,
    /// Seeds per (combo, account, fold).
    pub seeds: usize,
    /// Runs withdrawn by an appended `void` record, netted out of the product.
    pub voided: usize,
}

impl TrialComposition {
    /// The trial count this composition implies.
    ///
    /// Saturating rather than wrapping: a composition large enough to overflow
    /// is a config bug, and a wrapped trial count would deflate *nothing*,
    /// which is the failure direction this whole module is against.
    #[must_use]
    pub fn total(&self) -> usize {
        self.combos
            .saturating_mul(self.accounts)
            .saturating_mul(self.folds)
            .saturating_mul(self.seeds)
            .saturating_sub(self.voided)
    }

    /// One line, for the scorecard.
    ///
    /// Renders the arithmetic rather than its result, because the whole reason
    /// this type exists is that `3840` tells a reader nothing about whether the
    /// account dimension is in there.
    #[must_use]
    pub fn render(&self) -> String {
        let base = format!(
            "{} combos × {} accounts × {} folds × {} seeds",
            self.combos, self.accounts, self.folds, self.seeds
        );
        if self.voided == 0 {
            format!("{base} = {}", self.total())
        } else {
            format!("{base} − {} voided = {}", self.voided, self.total())
        }
    }
}

/// Standard normal CDF via the error function.
///
/// `erf` is not in `std`, so it is Abramowitz & Stegun 7.1.26 — max absolute
/// error 1.5e-7, which is far below the precision anything downstream claims.
/// Deterministic and branch-stable, which §2.2 requires of anything that can
/// reach a result.
#[must_use]
pub fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

/// Abramowitz & Stegun 7.1.26.
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y
}

/// Inverse standard normal CDF (the probit), Acklam's rational approximation.
///
/// Relative error below 1.15e-9 over the open interval, which is well past what
/// [`expected_max_sharpe`] needs. Inputs at or outside `0..=1` are clamped into
/// a representable interior rather than returning an infinity that would
/// propagate into a benchmark and silently deflate everything to zero.
#[must_use]
pub fn inverse_normal_cdf(p: f64) -> f64 {
    // Clamped rather than asserted: the callers here derive `p` from a trial
    // count, so a value at the boundary means "an enormous grid", not a bug.
    let p = p.clamp(1e-12, 1.0 - 1e-12);

    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const P_LOW: f64 = 0.024_25;
    const P_HIGH: f64 = 1.0 - P_LOW;

    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// Sharpe, skew and non-excess kurtosis of a return series.
///
/// One pass per moment, in a fixed order, so the result does not depend on
/// summation order across threads (§2.2). Returns `None` for fewer than two
/// observations or a zero standard deviation — a flat series has no Sharpe, and
/// zero is a different claim from "no opinion" (D-0080).
#[must_use]
pub fn moments(returns: &[f64]) -> Option<(f64, f64, f64)> {
    if returns.len() < 2 {
        return None;
    }
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    // `d * d`, not `powi(2)` (D-0126). Also inside D-0122's estimator.
    let variance = returns
        .iter()
        .map(|r| {
            let d = r - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    if variance <= 0.0 || !variance.is_finite() {
        return None;
    }
    let sd = variance.sqrt();
    // The powers are written out, and this is a §2.2 fix rather than a style
    // choice — `powi` made these two moments depend on the OPTIMISATION LEVEL.
    //
    // `f64::powi` lowers to `llvm.powi`, which the backend expands into
    // multiplications, and it is free to associate them differently at
    // different optimisation levels. Floating-point multiplication is not
    // associative, so `(z*z)*z` and `z*(z*z)` are different values, as are
    // `(z*z)*(z*z)` and `((z*z)*z)*z`. Rust itself never reassociates float
    // arithmetic — there is no fast-math — so writing the association into the
    // source removes the freedom the backend was exercising, and the result
    // becomes a property of this function rather than of how it was compiled.
    //
    // Measured on 2,000 seeded returns, dev against release: `skew` read
    // -8.79018770777955371631e-2 and -8.79018770777955094076e-2, `kurtosis`
    // 2.88711809985583700566e0 and 2.88711809985583744975e0. The input series
    // hashed identically in both, which is what makes the divergence
    // attributable to this arithmetic and not to the data.
    //
    // `variance` above keeps `powi(2)`: `z*z` has only one association, so it
    // has no freedom to exercise — and that is not an assumption, it is the
    // third case in the same measurement. `sharpe`, which is built from the
    // variance, was byte-identical across both profiles while these two were
    // not.
    let skew = returns
        .iter()
        .map(|r| {
            let z = (r - mean) / sd;
            z * z * z
        })
        .sum::<f64>()
        / n;
    let kurtosis = returns
        .iter()
        .map(|r| {
            let z = (r - mean) / sd;
            let z2 = z * z;
            z2 * z2
        })
        .sum::<f64>()
        / n;
    Some((mean / sd, skew, kurtosis))
}

#[cfg(test)]
#[path = "deflated/tests.rs"]
mod tests;
