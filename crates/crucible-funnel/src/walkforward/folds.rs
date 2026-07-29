//! Where the fold boundaries are, and why they are there.
//!
//! ## Folds are counted in trading days (D-0062)
//!
//! A fold is `train_days` sessions of in-sample followed by `test_days`
//! sessions of out-of-sample, advancing `step_days` sessions per fold. Not
//! months, not wall-clock days, not bars:
//!
//! - **Not months.** A "6-month" test window holds between 122 and 130 CME
//!   sessions depending on where it lands, because holidays cluster
//!   (Thanksgiving, Christmas, Good Friday). The per-fold Sharpe of a
//!   122-session window and a 130-session one are not the same estimator, and
//!   the OOS summary that pools them weights them by an accident of the
//!   exchange's holiday schedule. Counting sessions makes every fold's sample
//!   size a number the researcher chose.
//! - **Not wall-clock days.** Weekends and holidays would pad a window with
//!   zero observations and shrink its real sample without changing its label.
//! - **Not bars.** A window that ends mid-session splits a trading day across
//!   the train/test boundary, which is the one place a strategy carrying a
//!   position across the boundary looks most like leakage. Every window here
//!   is a whole number of whole sessions.
//!
//! ## What a trading day is, is the caller's business
//!
//! This module takes `day_keys: &[i64]` — one nondecreasing key per bar,
//! usually `days_from_civil(calendar.trading_day(bar.avail_ts()))`. It never
//! asks what a session is, which is what keeps `crucible-data` (and with it
//! `chrono`, and the compiled-in calendar tables) out of this crate's
//! dependency graph. The keys are keyed on **`avail_ts`, never `ts_open`**
//! (CLAUDE.md §2.1): a fold boundary is an ordering decision, and ordering
//! decisions use availability time.
//!
//! ## Two boundary rules worth stating out loud
//!
//! **The first fold starts at the first WHOLE trading day at or after the
//! grid's warmup.** The warmup boundary falls wherever it falls inside a
//! session; taking the rest of that session as fold 0's first day would give
//! fold 0 a partial day and make its "60 sessions" a lie. The dropped
//! remainder is counted and reported, not silently absorbed.
//!
//! **`step_days < test_days` is refused.** Overlapping test windows put the
//! same bar in two folds' out-of-sample samples, and the headline OOS number
//! pools those windows — the same observation would be counted twice, which
//! inflates the pooled sample size, deflates its standard error, and flatters
//! every downstream statistic that reads `n`. Gaps (`step_days > test_days`)
//! are merely wasteful and are allowed, with the unused sessions reported.

use std::fmt;
use std::ops::Range;

/// How the training window moves between folds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldScheme {
    /// The training window keeps its start and grows: fold *k* trains on
    /// everything before its test window. More data per fold, but the early
    /// history is weighted into every fold's estimate.
    Anchored,
    /// The training window keeps its length and slides: fold *k* trains on
    /// the `train_days` sessions immediately before its test window. Fewer
    /// observations, but every fold's estimate is equally recent.
    Rolling,
}

impl fmt::Display for FoldScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FoldScheme::Anchored => f.write_str("anchored"),
            FoldScheme::Rolling => f.write_str("rolling"),
        }
    }
}

impl std::str::FromStr for FoldScheme {
    type Err = FoldError;

    fn from_str(s: &str) -> Result<FoldScheme, FoldError> {
        match s {
            "anchored" => Ok(FoldScheme::Anchored),
            "rolling" => Ok(FoldScheme::Rolling),
            other => Err(FoldError::UnknownScheme {
                found: other.to_owned(),
            }),
        }
    }
}

/// The fold layout a config asks for, in trading days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldSpec {
    /// Whether the training window grows or slides.
    pub scheme: FoldScheme,
    /// In-sample sessions per fold.
    pub train_days: usize,
    /// Out-of-sample sessions per fold.
    pub test_days: usize,
    /// Sessions each fold advances. Equal to `test_days` tiles the sample
    /// exactly; larger leaves gaps; smaller is refused (see module docs).
    pub step_days: usize,
}

/// One contiguous stretch of the run, addressed both ways.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// Bar indices into the replayed series, `[start, end)`.
    pub bars: Range<usize>,
    /// Trading-day indices into [`FoldPlan::days`], `[start, end)`.
    pub days: Range<usize>,
}

impl Window {
    /// Bars in the window.
    #[must_use]
    pub const fn n_bars(&self) -> usize {
        self.bars.end - self.bars.start
    }

    /// Trading days in the window.
    #[must_use]
    pub const fn n_days(&self) -> usize {
        self.days.end - self.days.start
    }
}

/// One train/test split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fold {
    /// Position in the plan. Part of the run identity a derived seed is
    /// built from (CLAUDE.md §2.2), so it is stable for a given plan.
    pub index: usize,
    /// In-sample window.
    pub train: Window,
    /// Out-of-sample window. These never overlap between folds.
    pub test: Window,
}

/// Nothing here is recoverable: every variant is a config or data problem
/// that has to be fixed before any number is worth computing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FoldError {
    /// `scheme` was neither `rolling` nor `anchored`.
    UnknownScheme {
        /// What the config said.
        found: String,
    },
    /// A window length was zero.
    ZeroLength {
        /// Which field.
        field: &'static str,
    },
    /// The day keys go backwards, so the series is not availability-ordered.
    KeysOutOfOrder {
        /// Bar index where the key decreased.
        at: usize,
    },
    /// Test windows would overlap.
    OverlappingTestWindows {
        /// `step_days`.
        step_days: usize,
        /// `test_days`.
        test_days: usize,
    },
    /// The evaluable span is too short to hold even one fold.
    NotEnoughDays {
        /// Whole trading days available after warmup.
        available: usize,
        /// Days one fold needs.
        needed: usize,
    },
}

impl fmt::Display for FoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FoldError::UnknownScheme { found } => write!(
                f,
                "unknown walk-forward scheme {found:?}; this build implements `rolling` \
                 (fixed-length training window that slides) and `anchored` (training window \
                 that grows from a fixed start)"
            ),
            FoldError::ZeroLength { field } => {
                write!(f, "{field} must be at least 1 trading day")
            }
            FoldError::KeysOutOfOrder { at } => write!(
                f,
                "trading-day keys decrease at bar {at}; the series is not availability-ordered \
                 (CLAUDE.md §2.1) and any fold cut from it would be wrong"
            ),
            FoldError::OverlappingTestWindows {
                step_days,
                test_days,
            } => write!(
                f,
                "step_days = {step_days} is shorter than test_days = {test_days}, so \
                 consecutive out-of-sample windows would overlap. The headline number pools \
                 them, and pooling overlapping windows counts the same session twice: it \
                 inflates the sample size, shrinks the standard error, and flatters every \
                 statistic that reads n. Set step_days >= test_days (equal tiles the sample \
                 exactly)"
            ),
            FoldError::NotEnoughDays { available, needed } => write!(
                f,
                "{available} whole trading day(s) after the grid's warmup, but one fold needs \
                 {needed} (train + test). Widen the date range, shorten the windows, or \
                 shorten the grid's longest warmup"
            ),
        }
    }
}

impl std::error::Error for FoldError {}

/// A fold layout resolved against one concrete bar series.
///
/// Built once and shared by every combo in the grid — that is CLAUDE.md §2.6
/// applied to folds rather than to warmup. Two combos are compared on the
/// same sessions, cut at the same bars, or they are not compared at all.
#[derive(Clone, Debug)]
pub struct FoldPlan {
    spec: FoldSpec,
    /// The distinct trading-day key of each evaluable day, in order.
    days: Vec<i64>,
    /// Bar index each evaluable day starts at, plus a terminating bar index.
    /// Length is `days.len() + 1`.
    day_starts: Vec<usize>,
    warmup_bars: usize,
    /// Bars between the end of warmup and the first whole trading day.
    partial_day_bars: usize,
    folds: Vec<Fold>,
    /// Trading days after the last fold's test window.
    unused_tail_days: usize,
}

impl FoldPlan {
    /// Resolves `spec` against a series whose bars carry `day_keys`.
    ///
    /// `warmup_bars` is the **grid's** warmup (`Grid::max_warmup_bars`), not
    /// any one combo's: folds are laid out over the bars every combo is
    /// allowed to trade, so the plan is identical across the grid by
    /// construction (§2.6).
    ///
    /// # Errors
    /// [`FoldError`] naming what to change. None of them is recoverable.
    pub fn build(
        day_keys: &[i64],
        warmup_bars: usize,
        spec: FoldSpec,
    ) -> Result<FoldPlan, FoldError> {
        if spec.train_days == 0 {
            return Err(FoldError::ZeroLength {
                field: "train_days",
            });
        }
        if spec.test_days == 0 {
            return Err(FoldError::ZeroLength { field: "test_days" });
        }
        if spec.step_days == 0 {
            return Err(FoldError::ZeroLength { field: "step_days" });
        }
        if spec.step_days < spec.test_days {
            return Err(FoldError::OverlappingTestWindows {
                step_days: spec.step_days,
                test_days: spec.test_days,
            });
        }
        for (i, pair) in day_keys.windows(2).enumerate() {
            if pair[1] < pair[0] {
                return Err(FoldError::KeysOutOfOrder { at: i + 1 });
            }
        }

        // The first bar whose orders count, and then forward to the first bar
        // that OPENS a trading day: a fold that begins mid-session is not the
        // number of sessions it claims to be.
        let eval_start = warmup_bars.min(day_keys.len());
        let mut first = eval_start;
        while first > 0 && first < day_keys.len() && day_keys[first] == day_keys[first - 1] {
            first += 1;
        }
        let partial_day_bars = first - eval_start;

        let mut days: Vec<i64> = Vec::new();
        let mut day_starts: Vec<usize> = Vec::new();
        for (i, &key) in day_keys.iter().enumerate().skip(first) {
            if days.last() != Some(&key) {
                days.push(key);
                day_starts.push(i);
            }
        }
        day_starts.push(day_keys.len());

        let n_days = days.len();
        let needed = spec.train_days + spec.test_days;
        if n_days < needed {
            return Err(FoldError::NotEnoughDays {
                available: n_days,
                needed,
            });
        }

        // Both schemes place the SAME test windows; only the training window
        // differs. That is deliberate — two schemes scored on different
        // out-of-sample samples cannot be compared to each other.
        let mut folds = Vec::new();
        let mut k = 0usize;
        loop {
            let test_day_start = spec.train_days + k * spec.step_days;
            let test_day_end = test_day_start + spec.test_days;
            if test_day_end > n_days {
                break;
            }
            let train_day_start = match spec.scheme {
                FoldScheme::Anchored => 0,
                FoldScheme::Rolling => k * spec.step_days,
            };
            folds.push(Fold {
                index: k,
                train: Self::window(&day_starts, train_day_start..test_day_start),
                test: Self::window(&day_starts, test_day_start..test_day_end),
            });
            k += 1;
        }

        let unused_tail_days = folds.last().map_or(n_days, |f| n_days - f.test.days.end);

        Ok(FoldPlan {
            spec,
            days,
            day_starts,
            warmup_bars,
            partial_day_bars,
            folds,
            unused_tail_days,
        })
    }

    /// Bar range spanned by a range of day indices.
    fn window(day_starts: &[usize], days: Range<usize>) -> Window {
        Window {
            bars: day_starts[days.start]..day_starts[days.end],
            days,
        }
    }

    /// The layout this plan resolves.
    #[must_use]
    pub const fn spec(&self) -> &FoldSpec {
        &self.spec
    }

    /// The folds, in order.
    #[must_use]
    pub fn folds(&self) -> &[Fold] {
        &self.folds
    }

    /// The trading-day key of each evaluable day, in order. Index *i* is what
    /// [`Window::days`] refers to.
    #[must_use]
    pub fn days(&self) -> &[i64] {
        &self.days
    }

    /// Bar index at which evaluable day `i` starts.
    ///
    /// # Panics
    /// Panics if `i > days().len()`; index `days().len()` is the terminator.
    #[must_use]
    pub fn day_start_bar(&self, i: usize) -> usize {
        self.day_starts[i]
    }

    /// The grid warmup this plan was laid out behind.
    #[must_use]
    pub const fn warmup_bars(&self) -> usize {
        self.warmup_bars
    }

    /// Bars dropped between the end of warmup and the first whole trading
    /// day. Reported rather than absorbed: a reader comparing bar counts
    /// should be able to account for every bar in the series.
    #[must_use]
    pub const fn partial_day_bars(&self) -> usize {
        self.partial_day_bars
    }

    /// Trading days after the last fold's test window — evaluable sessions no
    /// fold reached, because `step_days` did not divide the remainder. Zero
    /// when the layout tiles the sample exactly.
    #[must_use]
    pub const fn unused_tail_days(&self) -> usize {
        self.unused_tail_days
    }

    /// Bars in the series this plan was built against.
    ///
    /// The runner checks it against the series it is about to replay: a plan
    /// laid out over a different series would cut folds at the wrong bars,
    /// silently.
    #[must_use]
    pub fn n_bars(&self) -> usize {
        self.day_starts.last().copied().unwrap_or(0)
    }

    /// Bars inside some fold's out-of-sample window — the sample every
    /// headline number here is computed on.
    #[must_use]
    pub fn oos_bars(&self) -> usize {
        self.folds.iter().map(|f| f.test.n_bars()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six bars per trading day, `n` days, starting at day key 100.
    fn keys(n_days: usize, bars_per_day: usize) -> Vec<i64> {
        (0..n_days)
            .flat_map(|d| {
                std::iter::repeat_n(
                    100 + i64::try_from(d).expect("small test index"),
                    bars_per_day,
                )
            })
            .collect()
    }

    fn spec(scheme: FoldScheme, train: usize, test: usize, step: usize) -> FoldSpec {
        FoldSpec {
            scheme,
            train_days: train,
            test_days: test,
            step_days: step,
        }
    }

    /// Hand-laid-out. 10 trading days of 6 bars each = 60 bars, no warmup.
    /// train 4 / test 2 / step 2:
    ///
    /// | fold | train days | test days | test bars |
    /// |---|---|---|---|
    /// | 0 | 0..4 | 4..6 | 24..36 |
    /// | 1 | 2..6 | 6..8 | 36..48 |
    /// | 2 | 4..8 | 8..10 | 48..60 |
    ///
    /// A fourth fold would need days 10..12, which do not exist.
    #[test]
    fn rolling_folds_are_laid_out_by_hand() {
        let plan = FoldPlan::build(&keys(10, 6), 0, spec(FoldScheme::Rolling, 4, 2, 2))
            .expect("ten days hold three folds");
        assert_eq!(plan.folds().len(), 3);
        assert_eq!(plan.folds()[0].train.days, 0..4);
        assert_eq!(plan.folds()[0].test.days, 4..6);
        assert_eq!(plan.folds()[0].test.bars, 24..36);
        assert_eq!(plan.folds()[1].train.days, 2..6);
        assert_eq!(plan.folds()[1].test.bars, 36..48);
        assert_eq!(plan.folds()[2].train.days, 4..8);
        assert_eq!(plan.folds()[2].test.bars, 48..60);
        assert_eq!(plan.unused_tail_days(), 0);
    }

    /// Same series, anchored: the test windows are IDENTICAL and only the
    /// training window differs. Two schemes scored on different out-of-sample
    /// samples would not be comparable to each other.
    #[test]
    fn anchored_keeps_the_same_test_windows() {
        let rolling =
            FoldPlan::build(&keys(10, 6), 0, spec(FoldScheme::Rolling, 4, 2, 2)).expect("builds");
        let anchored =
            FoldPlan::build(&keys(10, 6), 0, spec(FoldScheme::Anchored, 4, 2, 2)).expect("builds");
        let tests =
            |p: &FoldPlan| -> Vec<Window> { p.folds().iter().map(|f| f.test.clone()).collect() };
        assert_eq!(tests(&rolling), tests(&anchored));
        assert_eq!(anchored.folds()[0].train.days, 0..4);
        assert_eq!(anchored.folds()[1].train.days, 0..6);
        assert_eq!(anchored.folds()[2].train.days, 0..8);
    }

    /// Warmup lands 2 bars into day 1 (bar 8 of 6-bar days), so day 1 is a
    /// partial session and fold 0 starts at day 2, bar 12. Taking the rest of
    /// day 1 would give fold 0 four bars of one session and call it a day.
    #[test]
    fn the_first_fold_starts_at_a_whole_trading_day() {
        let plan = FoldPlan::build(&keys(10, 6), 8, spec(FoldScheme::Rolling, 4, 2, 2))
            .expect("eight days remain, enough for one fold");
        assert_eq!(plan.partial_day_bars(), 4); // bars 8..12
        assert_eq!(plan.days().len(), 8); // days 2..10
        assert_eq!(plan.folds()[0].train.bars.start, 12);
        assert_eq!(plan.days()[0], 102);
    }

    /// Warmup ending exactly on a day boundary drops nothing.
    #[test]
    fn a_warmup_ending_on_a_boundary_wastes_no_bars() {
        let plan =
            FoldPlan::build(&keys(10, 6), 12, spec(FoldScheme::Rolling, 4, 2, 2)).expect("builds");
        assert_eq!(plan.partial_day_bars(), 0);
        assert_eq!(plan.folds()[0].train.bars.start, 12);
    }

    /// The refusal that keeps the pooled OOS sample honest.
    #[test]
    fn overlapping_test_windows_are_refused() {
        let err = FoldPlan::build(&keys(10, 6), 0, spec(FoldScheme::Rolling, 4, 3, 2))
            .expect_err("overlap must refuse");
        assert_eq!(
            err,
            FoldError::OverlappingTestWindows {
                step_days: 2,
                test_days: 3
            }
        );
        assert!(err.to_string().contains("twice"));
    }

    /// A step wider than the test window leaves sessions no fold reaches.
    /// Allowed — merely wasteful — but counted, because a reader comparing
    /// "1,000 bars replayed" against the fold table should find the gap.
    #[test]
    fn a_wide_step_leaves_a_counted_tail() {
        // train 2 / test 2 / step 4 over 10 days: folds test 2..4 and 6..8.
        let plan =
            FoldPlan::build(&keys(10, 6), 0, spec(FoldScheme::Rolling, 2, 2, 4)).expect("builds");
        assert_eq!(plan.folds().len(), 2);
        assert_eq!(plan.folds()[1].test.days, 6..8);
        assert_eq!(plan.unused_tail_days(), 2);
    }

    #[test]
    fn too_short_a_span_says_what_it_needed() {
        let err = FoldPlan::build(&keys(5, 6), 0, spec(FoldScheme::Rolling, 4, 2, 2))
            .expect_err("five days cannot hold a six-day fold");
        assert_eq!(
            err,
            FoldError::NotEnoughDays {
                available: 5,
                needed: 6
            }
        );
    }

    #[test]
    fn decreasing_day_keys_are_fatal() {
        let mut k = keys(10, 6);
        k[30] = 1;
        let err = FoldPlan::build(&k, 0, spec(FoldScheme::Rolling, 4, 2, 2))
            .expect_err("a series that goes backwards is not a series");
        assert_eq!(err, FoldError::KeysOutOfOrder { at: 30 });
    }

    #[test]
    fn schemes_round_trip_their_names() {
        assert_eq!("rolling".parse(), Ok(FoldScheme::Rolling));
        assert_eq!("anchored".parse(), Ok(FoldScheme::Anchored));
        assert_eq!(FoldScheme::Rolling.to_string(), "rolling");
        assert!("expanding".parse::<FoldScheme>().is_err());
    }
}
