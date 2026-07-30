//! Trailing-window statistics, and the one thing they refuse to be.
//!
//! ## Point-in-time by construction, not by discipline (D-0080)
//!
//! A normalized feature is where lookahead usually enters a research pipeline,
//! and it enters by convenience: the whole series is in memory, `mean()` and
//! `std()` are one call each, and the resulting z-score is standardized against
//! a mean the market had not yet produced. §2.1 names it —
//! "full-sample z-scores, full-sample quantiles, or fit-on-everything-then-
//! backtest are lookahead" — and `controls::LeakyZScore` is a working example
//! of exactly that, checked into this repository on purpose.
//!
//! So the defence here is structural rather than procedural. Everything in this
//! module is a [`RollingWindow`] behind [`Indicator::update`], which takes **one
//! bar** and returns a reading for **that** bar:
//!
//! - there is no constructor taking a slice, an iterator, or a series;
//! - there is no `fit`, no two-pass anything, and no way to hand one of these a
//!   bar it has not already seen in order;
//! - the config grammar can name [`RollingZScore`] and [`RollingStdev`] and
//!   cannot name a full-sample statistic, because no `IndicatorKind` is one.
//!
//! The property that follows, and which the tests state directly, is
//! **truncation invariance**: a reading at bar *i* is a function of bars
//! *i−period+1 … i* alone, so appending bars after *i* cannot change it.
//! A full-sample statistic is not truncation-invariant, and the difference
//! between the two readings *is* the lookahead — measured, in the tests, rather
//! than asserted.
//!
//! ## Conventions, pinned
//!
//! - **Population standard deviation** (ddof = 0), matching [`Bollinger`]
//!   (D-0009). Changing it would silently shift every z-score.
//! - **The window includes the current bar**, again matching [`Bollinger`]: the
//!   reading at bar *i* standardizes bar *i* against the window ending at it.
//!   That is not lookahead — bar *i* is complete and available (§2.1) — and it
//!   is what makes `zscore < -2` mean "this bar is two sigmas below its own
//!   recent range".
//! - **A window with no variation has no z-score**, and the reading is `None`
//!   rather than a NaN: every value in it equals the mean, so the numerator and
//!   the denominator are both zero. `None` is what the rule grammar already
//!   means by "no opinion this bar", so a flat window makes a rule silent
//!   instead of taking a position on 0/0.
//! - **Rolling sums**, like [`Bollinger`]: drift over very long streams is known
//!   and accepted until the M2 Welford/rebase task, and recomputing the window
//!   per bar is O(period) and kills grid throughput (CLAUDE.md §9).
//!
//! [`Bollinger`]: super::Bollinger

use std::collections::VecDeque;

use crucible_core::events::Bar;
use crucible_core::traits::Indicator;

/// Which series a rolling statistic is computed over.
///
/// Named in the config and carried into the canonical form, because "the
/// z-score of what" is not a detail: a 20-bar z-score of price and a 20-bar
/// z-score of volume are different features that would otherwise hash to the
/// same identity (D-0012).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RollingSource {
    /// The bar's close, in **signal space** — so on a stitched series it is the
    /// back-adjusted level every other indicator reads (D-0076).
    ///
    /// A z-score of it is additionally **shift-invariant**: adding a constant to
    /// every value in the window cancels between the numerator and the mean, so
    /// unlike `close > 4500` this feature does not depend on where the roll
    /// table happens to have put the level.
    Close,
    /// The bar's traded size, in contracts (D-0079).
    ///
    /// The reason volume normalization belongs here rather than in the operand:
    /// "twice its recent average" is a trailing-window question, and an absolute
    /// `volume > 1000` means different things on every grain and in every year.
    Volume,
    /// The simple bar-over-bar return of the close, as a fraction.
    ///
    /// Costs **one extra warmup bar** — the first bar has no predecessor and so
    /// no return — and that bar is added to the indicator's declared warmup, so
    /// §2.6's grid-wide alignment counts it.
    Return,
}

impl RollingSource {
    /// The config spelling, which is also the canonical-form spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            RollingSource::Close => "close",
            RollingSource::Volume => "volume",
            RollingSource::Return => "return",
        }
    }

    /// Every source, in canonical order.
    #[must_use]
    pub const fn all() -> &'static [RollingSource] {
        &[
            RollingSource::Close,
            RollingSource::Volume,
            RollingSource::Return,
        ]
    }

    /// Parses the config spelling.
    #[must_use]
    pub fn parse(name: &str) -> Option<RollingSource> {
        RollingSource::all()
            .iter()
            .copied()
            .find(|s| s.name() == name)
    }

    /// Bars this source needs beyond the window length itself.
    ///
    /// One for [`RollingSource::Return`], because a return needs two closes;
    /// zero for the rest. Added to the declared warmup so that a grid mixing
    /// sources still aligns every combo on the same bar (§2.6).
    #[must_use]
    pub const fn extra_warmup_bars(self) -> usize {
        match self {
            RollingSource::Close | RollingSource::Volume => 0,
            RollingSource::Return => 1,
        }
    }

    /// This bar's value, given the previous close in signal space.
    ///
    /// `None` only for [`RollingSource::Return`] on the first bar, and only
    /// because there is nothing to difference against — never because a value
    /// was inconvenient.
    fn value(self, bar: &Bar, previous_close: Option<f64>) -> Option<f64> {
        let close = bar.signal_close().as_points_f64();
        match self {
            RollingSource::Close => Some(close),
            RollingSource::Volume => {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "indicator space (§2.3); exact below 2^53 contracts"
                )]
                let volume = bar.volume as f64;
                Some(volume)
            }
            RollingSource::Return => {
                let previous = previous_close?;
                (previous != 0.0).then(|| close / previous - 1.0)
            }
        }
    }
}

/// The trailing window every statistic in this module shares.
///
/// Private state with one entry point: values go in one at a time, in bar
/// order, and the mean and variance come out of the window that ended at the
/// most recent one. There is no method that takes more than one value, which is
/// the structural half of this module's promise.
#[derive(Debug, Clone)]
struct RollingWindow {
    period: usize,
    source: RollingSource,
    values: VecDeque<f64>,
    sum: f64,
    sumsq: f64,
    previous_close: Option<f64>,
}

impl RollingWindow {
    fn new(period: usize, source: RollingSource) -> RollingWindow {
        assert!(
            period >= 2,
            "a rolling window needs at least 2 bars: a one-bar window has zero \
             spread, so every reading taken over it is 0/0"
        );
        RollingWindow {
            period,
            source,
            values: VecDeque::with_capacity(period + 1),
            sum: 0.0,
            sumsq: 0.0,
            previous_close: None,
        }
    }

    fn warmup(&self) -> usize {
        self.period + self.source.extra_warmup_bars()
    }

    /// Absorbs one bar and returns `(mean, population sd, this bar's value)`
    /// once the window is full.
    fn update(&mut self, bar: &Bar) -> Option<(f64, f64, f64)> {
        let value = self.source.value(bar, self.previous_close);
        self.previous_close = Some(bar.signal_close().as_points_f64());
        let x = value?;
        if !x.is_finite() {
            return None;
        }

        self.values.push_back(x);
        self.sum += x;
        self.sumsq += x * x;
        if self.values.len() > self.period
            && let Some(old) = self.values.pop_front()
        {
            self.sum -= old;
            self.sumsq -= old * old;
        }
        if self.values.len() < self.period {
            return None;
        }

        #[expect(clippy::cast_precision_loss, reason = "periods are small")]
        let n = self.period as f64;
        let mean = self.sum / n;
        // Population variance (ddof = 0), matching Bollinger (D-0009); clamped
        // at zero because the rolling difference can go slightly negative on
        // float noise when every value in the window is equal.
        let variance = (self.sumsq / n - mean * mean).max(0.0);
        Some((mean, variance.sqrt(), x))
    }
}

/// The trailing-window z-score of a source: `(x − mean) / sd`, over the last
/// `period` bars including this one.
///
/// See the module docs for why there is no full-sample counterpart and cannot
/// be one.
#[derive(Debug, Clone)]
pub struct RollingZScore {
    window: RollingWindow,
}

impl RollingZScore {
    /// A z-score over `period` bars of `source`.
    ///
    /// # Panics
    /// Panics if `period < 2`. A one-bar window has zero spread, so every
    /// reading over it is 0/0 — a config bug, and one that should fail at
    /// build-the-strategy time rather than produce `None` forever (§5.1).
    #[must_use]
    pub fn new(period: usize, source: RollingSource) -> RollingZScore {
        RollingZScore {
            window: RollingWindow::new(period, source),
        }
    }
}

impl Indicator for RollingZScore {
    type Out = f64;

    fn warmup(&self) -> usize {
        self.window.warmup()
    }

    fn update(&mut self, bar: &Bar) -> Option<f64> {
        let (mean, sd, x) = self.window.update(bar)?;
        // A window with no variation has no z-score: numerator and denominator
        // are both zero. `None` is the grammar's "no opinion", which is the
        // honest answer and keeps a NaN out of a comparison.
        (sd > 0.0).then(|| (x - mean) / sd)
    }
}

/// The trailing-window population standard deviation of a source.
///
/// The denominator of [`RollingZScore`], exposed on its own because realized
/// volatility is a feature in its own right — a volatility-managed exposure
/// rule scales on it rather than on a normalized deviation.
#[derive(Debug, Clone)]
pub struct RollingStdev {
    window: RollingWindow,
}

impl RollingStdev {
    /// A standard deviation over `period` bars of `source`.
    ///
    /// # Panics
    /// Panics if `period < 2`, as [`RollingZScore::new`] does.
    #[must_use]
    pub fn new(period: usize, source: RollingSource) -> RollingStdev {
        RollingStdev {
            window: RollingWindow::new(period, source),
        }
    }
}

impl Indicator for RollingStdev {
    type Out = f64;

    fn warmup(&self) -> usize {
        self.window.warmup()
    }

    fn update(&mut self, bar: &Bar) -> Option<f64> {
        // Zero is a real answer here, unlike for the z-score: a window with no
        // variation has a standard deviation, and it is zero.
        self.window.update(bar).map(|(_, sd, _)| sd)
    }
}

#[cfg(test)]
mod tests;
