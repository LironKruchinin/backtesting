//! Account-evaluation series capture — the engine half of
//! `docs/ACCOUNT_EVAL_SPEC.md` §3 (D-0067, D-0071).
//!
//! # Why capture rather than compute later
//!
//! An account rule is a **path functional**: "did cumulative PnL ever come
//! within $2,000 of its own running high-water, measured on unrealized equity,
//! before it first closed a day $3,000 up?" Total return, Sharpe and max
//! drawdown are functionals of the equity *curve*, which [`crate::Summary`]
//! already computes. A first-passage question is not, and no amount of
//! post-processing recovers it once the path resolution is gone.
//!
//! This module is **capture only**. The breach test, the block bootstrap,
//! P(pass) and payout cadence land with the funnel in M3 and are specified in
//! `docs/ACCOUNT_EVAL_SPEC.md` §4. Nothing here evaluates an account: there is
//! no account config in this crate and there must not be one.
//!
//! # The four series
//!
//! | # | what | type | sampled |
//! |---|---|---|---|
//! | 1 | per-trading-day PnL | [`DayRecord`] | `replay` step 2, after the mark |
//! | 2 | intraday unrealized-equity high-water | [`HighWaterState`] | `replay` step 2, after the mark |
//! | 3 | MAE / MFE per round-trip | [`crate::ClosedTrade`] | `Portfolio::mark`, guarded on an open position |
//! | 4 | worst-day distribution | [`WorstDayDistribution`] | **derived** from series 1 |
//!
//! # The reconstruction ban (spec §3.3, and it is the point of the module)
//!
//! Series 2 is **never rebuilt from OHLC after the run**. A reconstruction
//! re-derives the equity path from high and low prints, and the order of a
//! bar's high and low is not in the data. The engine already resolves that
//! ambiguity, once, in [`crate::bracket`]'s `stop_first_intrabar` convention
//! (D-0069); every fill, every position and therefore every mark downstream is
//! a consequence of that choice. A reconstruction re-opens the question with
//! its own convention, and the drawdown it computes is then the drawdown of a
//! path the account never took. Two paths that produce identical bars can
//! differ by more than the whole drawdown allowance. `tests/account_series.rs`
//! plants that divergence and measures it rather than asserting it.
//!
//! # How a trading day reaches this crate: it is handed over, not derived
//!
//! `crucible-engine` depends on `crucible-core` only. A trading day is an
//! *exchange* fact — CME Globex rolls the trade date at 17:00 CT, so a loss
//! taken at 18:30 CT Monday belongs to Tuesday — and the only module that
//! knows what a timezone is is `crucible-data::calendar`. So the day is taken
//! as **data**: one nondecreasing `i64` key per bar, exactly the
//! `days_from_civil(Calendar::trading_day(bar.avail_ts()))` slice that
//! `crucible-funnel::walkforward::folds` already takes. This is the fourth
//! application of the same device — caller-supplied `acquired_ts` (D-0015),
//! caller-supplied config hash (D-0060), caller-supplied fold boundaries
//! (D-0062) — and no dependency edge moves.
//!
//! **One producer, both consumers.** The keys are computed once, in
//! `crucible-cli`, and the *same slice* feeds fold attribution and day slicing.
//! Two independent attributions of "which day" is how a daily-loss-limit
//! breach lands on a different date in two reports; the reconciliation control
//! in `crucible-funnel::walkforward::tests` asserts the two agree to the
//! nanodollar and was watched failing when one consumer was handed wall-clock
//! keys.
//!
//! Keys are read on `avail_ts` because everything is (§2.1). A key that
//! decreases is refused, not smoothed: it means the series is not
//! availability-ordered and every window cut from it is wrong.
//!
//! # Memory: O(days) + O(round-trips), never O(bars)
//!
//! A 16-year 1-second replay is ~334 M bars. The per-bar equity vector
//! `replay` already builds is ~4.98 GiB at 16 bytes a bar; a second per-bar
//! series would double it. So series 2 is a **streaming reducer** — two
//! integer comparisons and 16 bytes of state, for any number of bars — and the
//! retained artifact is series 1, at [`size_of::<DayRecord>()`] = 56 bytes per
//! session. Sixteen years is 252 × 16 = 4,032 sessions ⇒ **226 KB**,
//! independent of the replay grain. Spec §3.3.1 proves the day summary is
//! exactly sufficient for the breach question, and
//! [`intraday_peak_crossing`] counts the one day per path where it is
//! conservative instead.
//!
//! What is deliberately *not* done here: the per-bar equity vector's opt-out
//! (spec §3.3 requirement 2). That is a change to an artifact the walk-forward
//! runner consumes, not a capture hook, and it belongs with the funnel work
//! that needs it.

use std::fmt;
use std::ops::Range;

use crucible_core::prelude::*;

/// The running high-water of cumulative PnL and the running decline from it —
/// series 2, as an O(1) reducer over the marks.
///
/// Both fields are monotone over the run, which is what makes the reducer
/// exact: a running maximum and a running maximum-drop need no history.
///
/// The zero value is the spec's starting state, and it is not merely
/// convenient: §2.2 defines `peak(t) = max(0, max_{s≤t} B(s))`, so the
/// threshold opens at `−D` and a day-one loss of `D` breaches. There is no
/// grace period, and starting the peak at the first observation instead of at
/// zero would invent one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HighWaterState {
    /// `max(0, max cum(s))` over every mark so far.
    pub peak_cum_nano_usd: NanoUsd,
    /// The largest `peak_cum − cum` seen at any mark so far. Non-negative.
    pub max_drop_from_peak_nano_usd: NanoUsd,
}

impl HighWaterState {
    /// Folds one mark in. Two integer comparisons; no allocation, ever.
    ///
    /// `cum_nano_usd` is equity at this mark minus equity at the anchor —
    /// cumulative-PnL space (spec §2.1), which is the space in which a Topstep
    /// Combine starting at $50,000 and a Topstep Express Funded Account
    /// starting at $0 are the same account.
    pub const fn observe(&mut self, cum_nano_usd: NanoUsd) {
        if cum_nano_usd > self.peak_cum_nano_usd {
            self.peak_cum_nano_usd = cum_nano_usd;
        }
        let drop = self.peak_cum_nano_usd - cum_nano_usd;
        if drop > self.max_drop_from_peak_nano_usd {
            self.max_drop_from_peak_nano_usd = drop;
        }
    }
}

/// One trading day of the run — series 1, and the retained artifact.
///
/// Every field is relative to the **day's open**, which is the cumulative-PnL
/// level carried *into* the day (the previous day's closing mark, or the
/// anchor for the first day). Not the day's first mark: a gap between one
/// day's close and the next day's first print is a real move of the account,
/// and measuring from the first mark would make it invisible in both
/// excursions and break the recursion in spec §3.3.1 that lets a day-block
/// bootstrap answer an intraday question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayRecord {
    /// The caller's trading-day key: `days_from_civil(Calendar::trading_day(
    /// avail_ts))`, i.e. days since 1970-01-01. Plain `i64` on purpose — it is
    /// the same vocabulary `crucible-funnel::walkforward::folds` takes, and
    /// the point of the seam is that both consumers read one slice.
    pub trading_day_key: i64,
    /// Bar indices of this day within the run's equity series, `[start, end)`.
    pub bars: Range<usize>,
    /// `cum(day close) − cum(previous day close)`. Fees included.
    pub close_pnl_nano_usd: NanoUsd,
    /// Maximum of `cum(t) − cum(day open)` over the day. Never negative: the
    /// day's open is itself in the range.
    pub peak_from_open_nano_usd: NanoUsd,
    /// Minimum of `cum(t) − cum(day open)` over the day. Never positive.
    pub trough_from_open_nano_usd: NanoUsd,
    /// The largest decline from the running high-water **within** the day,
    /// with the running high-water seeded at the day's open.
    ///
    /// Seeding at the open is the conservative reading and it invents nothing:
    /// the open *is* the previous day's closing level, an instant the account
    /// genuinely stood at. Spec §3.3.1's proof is unaffected either way,
    /// because the carried-in peak is never below the day's open.
    pub max_drop_from_running_peak_nano_usd: NanoUsd,
}

impl DayRecord {
    /// Bars in this day.
    #[must_use]
    pub const fn n_bars(&self) -> usize {
        self.bars.end - self.bars.start
    }
}

/// Everything one run captured for account evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSeries {
    /// Equity the run's cumulative-PnL space is measured from. Series 1 and 2
    /// are both differences against this, so it is recorded rather than
    /// assumed.
    pub anchor_equity_nano_usd: NanoUsd,
    /// Series 2, folded to its final state.
    pub high_water: HighWaterState,
    /// Series 1, in order. One entry per distinct trading-day key observed.
    pub days: Vec<DayRecord>,
    /// The grain the threshold was tested at, carried so a report cannot omit
    /// it. Spec §3.3.2 makes this mandatory beside any probability derived
    /// from these series: bar-close marks are a *lower bound* on the number of
    /// times a real account is tested, hence a lower bound on P(breach).
    pub mark_grain: TimeFrame,
    /// Marks folded in. Equal to the run's bar count.
    pub marks: usize,
}

/// Why a capture refused. Every variant is a wiring bug in the caller: the
/// keys did not describe the series that was replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// The run produced more bars than the caller supplied keys for.
    DayKeysExhausted {
        /// Bar index that had no key.
        bar_index: usize,
        /// Keys the caller supplied.
        day_keys: usize,
    },
    /// A key decreased, so the series is not availability-ordered.
    DayKeysOutOfOrder {
        /// Bar index where it decreased.
        bar_index: usize,
        /// Key of the day in progress.
        previous: i64,
        /// Key found at `bar_index`.
        found: i64,
    },
    /// Marks arrived out of order. The engine folds one mark per bar in bar
    /// order, so this can only come from a hand-driven capture.
    BarIndexOutOfOrder {
        /// The index the capture was expecting.
        expected: usize,
        /// The index it was handed.
        found: usize,
    },
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::DayKeysExhausted {
                bar_index,
                day_keys,
            } => write!(
                f,
                "account capture ran out of trading-day keys at bar {bar_index}: only {day_keys} \
                 were supplied. The keys must describe the same bar series the run replays, or \
                 every day boundary after this bar is in the wrong place"
            ),
            CaptureError::DayKeysOutOfOrder {
                bar_index,
                previous,
                found,
            } => write!(
                f,
                "trading-day key {found} at bar {bar_index} is before {previous}, the day already \
                 in progress; the series is not availability-ordered (CLAUDE.md §2.1) and every \
                 day sliced from it would be wrong"
            ),
            CaptureError::BarIndexOutOfOrder { expected, found } => write!(
                f,
                "account capture expected bar {expected} and was handed bar {found}; marks are \
                 folded in bar order"
            ),
        }
    }
}

impl std::error::Error for CaptureError {}

/// The day in progress. Never escapes; it becomes a [`DayRecord`] when the key
/// changes or the run ends.
#[derive(Clone, Copy, Debug)]
struct OpenDay {
    key: i64,
    first_bar: usize,
    last_bar: usize,
    /// Cumulative PnL at the level carried into the day.
    cum_at_open_nano_usd: NanoUsd,
    /// Cumulative PnL at the most recent mark.
    last_cum_nano_usd: NanoUsd,
    peak_from_open_nano_usd: NanoUsd,
    trough_from_open_nano_usd: NanoUsd,
    /// Running high-water within the day, relative to the day's open.
    running_peak_nano_usd: NanoUsd,
    max_drop_from_running_peak_nano_usd: NanoUsd,
}

/// The streaming capture. Fold one mark per bar; keep O(1) state per day.
///
/// Borrowed rather than owned day keys, because the same slice is the one
/// `crucible-funnel::walkforward::folds::FoldPlan::build` was given. Sharing
/// the object is what makes "both consumers agree about which day" a property
/// of the wiring rather than of a convention two places remember to follow.
#[derive(Debug)]
pub struct AccountCapture<'k> {
    day_keys: &'k [i64],
    anchor_equity_nano_usd: NanoUsd,
    mark_grain: TimeFrame,
    high_water: HighWaterState,
    days: Vec<DayRecord>,
    open: Option<OpenDay>,
    next_bar: usize,
}

impl<'k> AccountCapture<'k> {
    /// A capture over a run whose bars carry `day_keys`, measured from
    /// `anchor_equity_nano_usd`.
    ///
    /// The anchor is the equity level cumulative PnL is zero at. For a whole
    /// run that is the declared starting cash — the move *into* the first bar
    /// is the run's first return, the same anchor convention D-0063 uses for a
    /// fold window, and for the same reason.
    #[must_use]
    pub const fn new(
        day_keys: &'k [i64],
        anchor_equity_nano_usd: NanoUsd,
        mark_grain: TimeFrame,
    ) -> AccountCapture<'k> {
        AccountCapture {
            day_keys,
            anchor_equity_nano_usd,
            mark_grain,
            high_water: HighWaterState {
                peak_cum_nano_usd: 0,
                max_drop_from_peak_nano_usd: 0,
            },
            days: Vec::new(),
            open: None,
            next_bar: 0,
        }
    }

    /// Folds the mark at `bar_index` in.
    ///
    /// Called from `replay` step 2, immediately after `portfolio.mark(...)`
    /// and the equity push, so `equity_nano_usd` is the same number that
    /// entered the equity curve — including unrealized PnL, which is what
    /// every account in `configs/accounts/` tests against its threshold,
    /// including the ones whose ratchet only advances at a daily close
    /// (spec §7.0).
    ///
    /// # Errors
    /// [`CaptureError`] if the keys do not describe the series being replayed.
    pub fn observe(
        &mut self,
        bar_index: usize,
        equity_nano_usd: NanoUsd,
    ) -> Result<(), CaptureError> {
        if bar_index != self.next_bar {
            return Err(CaptureError::BarIndexOutOfOrder {
                expected: self.next_bar,
                found: bar_index,
            });
        }
        let key = *self.day_keys.get(bar_index).ok_or({
            CaptureError::DayKeysExhausted {
                bar_index,
                day_keys: self.day_keys.len(),
            }
        })?;
        self.next_bar += 1;

        let cum = equity_nano_usd - self.anchor_equity_nano_usd;
        self.high_water.observe(cum);

        match self.open {
            Some(day) if day.key == key => {}
            Some(day) if key < day.key => {
                return Err(CaptureError::DayKeysOutOfOrder {
                    bar_index,
                    previous: day.key,
                    found: key,
                });
            }
            Some(day) => {
                let carried = day.last_cum_nano_usd;
                self.close_day(day);
                self.open = Some(Self::open_day(key, bar_index, carried));
            }
            None => {
                self.open = Some(Self::open_day(key, bar_index, 0));
            }
        }

        let day = self
            .open
            .as_mut()
            .expect("INVARIANT: the match above leaves a day open on every arm");
        let x = cum - day.cum_at_open_nano_usd;
        if x > day.peak_from_open_nano_usd {
            day.peak_from_open_nano_usd = x;
        }
        if x < day.trough_from_open_nano_usd {
            day.trough_from_open_nano_usd = x;
        }
        if x > day.running_peak_nano_usd {
            day.running_peak_nano_usd = x;
        }
        let drop = day.running_peak_nano_usd - x;
        if drop > day.max_drop_from_running_peak_nano_usd {
            day.max_drop_from_running_peak_nano_usd = drop;
        }
        day.last_cum_nano_usd = cum;
        day.last_bar = bar_index;
        Ok(())
    }

    /// The high-water state so far. Defined at every mark boundary.
    #[must_use]
    pub const fn high_water(&self) -> HighWaterState {
        self.high_water
    }

    /// Closes the day in progress and yields the captured series.
    #[must_use]
    pub fn finish(mut self) -> AccountSeries {
        if let Some(day) = self.open.take() {
            self.close_day(day);
        }
        AccountSeries {
            anchor_equity_nano_usd: self.anchor_equity_nano_usd,
            high_water: self.high_water,
            days: self.days,
            mark_grain: self.mark_grain,
            marks: self.next_bar,
        }
    }

    /// A fresh day carrying `cum_at_open` in. Every excursion is seeded at the
    /// open, so a day with a single mark reports that mark's move and no more.
    const fn open_day(key: i64, first_bar: usize, cum_at_open_nano_usd: NanoUsd) -> OpenDay {
        OpenDay {
            key,
            first_bar,
            last_bar: first_bar,
            cum_at_open_nano_usd,
            last_cum_nano_usd: cum_at_open_nano_usd,
            peak_from_open_nano_usd: 0,
            trough_from_open_nano_usd: 0,
            running_peak_nano_usd: 0,
            max_drop_from_running_peak_nano_usd: 0,
        }
    }

    fn close_day(&mut self, day: OpenDay) {
        self.days.push(DayRecord {
            trading_day_key: day.key,
            bars: day.first_bar..day.last_bar + 1,
            close_pnl_nano_usd: day.last_cum_nano_usd - day.cum_at_open_nano_usd,
            peak_from_open_nano_usd: day.peak_from_open_nano_usd,
            trough_from_open_nano_usd: day.trough_from_open_nano_usd,
            max_drop_from_running_peak_nano_usd: day.max_drop_from_running_peak_nano_usd,
        });
    }
}

/// Series 4 — the worst-day distribution, **derived** from [`DayRecord`]s
/// rather than captured.
///
/// Two distributions, not one, and the pair is the point: the gap between the
/// worst *close* and the worst *trough* is exactly the amount of a bad day
/// that a daily-close model never sees. When they diverge, the endpoint
/// fallacy is quantified instead of argued about.
///
/// Reported as the full empirical distribution — **never as a mean**. A mean
/// worst day is a number describing no day.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WorstDayDistribution {
    closes_nano_usd: Vec<NanoUsd>,
    troughs_nano_usd: Vec<NanoUsd>,
}

impl WorstDayDistribution {
    /// Order statistics of `days`, ascending.
    ///
    /// `days` is normally the pooled *out-of-sample* days; that pooling is the
    /// caller's business, because which days are out of sample is a fold
    /// question, not an engine one.
    #[must_use]
    pub fn from_days(days: &[DayRecord]) -> WorstDayDistribution {
        let mut closes: Vec<NanoUsd> = days.iter().map(|d| d.close_pnl_nano_usd).collect();
        let mut troughs: Vec<NanoUsd> = days.iter().map(|d| d.trough_from_open_nano_usd).collect();
        closes.sort_unstable();
        troughs.sort_unstable();
        WorstDayDistribution {
            closes_nano_usd: closes,
            troughs_nano_usd: troughs,
        }
    }

    /// Days in the distribution.
    #[must_use]
    pub fn n_days(&self) -> usize {
        self.closes_nano_usd.len()
    }

    /// Every day's close, ascending. The full distribution is the artifact;
    /// the percentile helpers below are conveniences over it.
    #[must_use]
    pub fn closes_nano_usd(&self) -> &[NanoUsd] {
        &self.closes_nano_usd
    }

    /// Every day's trough from its open, ascending.
    #[must_use]
    pub fn troughs_nano_usd(&self) -> &[NanoUsd] {
        &self.troughs_nano_usd
    }

    /// The single worst closing day, or `None` if there are no days.
    #[must_use]
    pub fn worst_close_nano_usd(&self) -> Option<NanoUsd> {
        self.closes_nano_usd.first().copied()
    }

    /// The single worst intraday trough, or `None` if there are no days.
    #[must_use]
    pub fn worst_trough_nano_usd(&self) -> Option<NanoUsd> {
        self.troughs_nano_usd.first().copied()
    }

    /// The `percentile`-th closing day by the **nearest-rank** definition.
    ///
    /// Nearest rank, not interpolation, and not because it is simpler:
    /// interpolating between two days puts an `f64` where a dollar amount
    /// belongs (§2.3), and the answer is meant to name a day that happened.
    /// `percentile` is 1–100; anything else, or an empty distribution, is
    /// `None`.
    #[must_use]
    pub fn close_percentile_nano_usd(&self, percentile: u32) -> Option<NanoUsd> {
        nearest_rank(&self.closes_nano_usd, percentile)
    }

    /// The `percentile`-th trough, same definition.
    #[must_use]
    pub fn trough_percentile_nano_usd(&self, percentile: u32) -> Option<NanoUsd> {
        nearest_rank(&self.troughs_nano_usd, percentile)
    }
}

/// Nearest-rank order statistic of an ascending slice, in integers only.
fn nearest_rank(sorted: &[NanoUsd], percentile: u32) -> Option<NanoUsd> {
    if sorted.is_empty() || percentile == 0 || percentile > 100 {
        return None;
    }
    let n = u64::try_from(sorted.len()).ok()?;
    let rank = (u64::from(percentile) * n).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).ok()?;
    sorted.get(index).copied()
}

/// Where the run's running intraday peak first reached a level.
///
/// The one number M3 needs to know whether a day summary is exact or
/// conservative — see [`intraday_peak_crossing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeakCrossing {
    /// Index into the `days` slice.
    pub day_index: usize,
    /// That day's key.
    pub trading_day_key: i64,
    /// Running peak carried into the day, in cumulative-PnL space. Strictly
    /// below the level.
    pub peak_before_nano_usd: NanoUsd,
    /// Running peak at the end of the day. At or above the level.
    pub peak_after_nano_usd: NanoUsd,
}

/// The day on which the run's running **intraday** peak first reached
/// `level_nano_usd`, in cumulative-PnL space.
///
/// # What this is for: the one approximate case, counted rather than hidden
///
/// Spec §3.3.1 proves that four numbers per day decide the breach question
/// exactly — with one exception. When an account's ratchet **lock** engages
/// *inside* a day, the threshold stops rising mid-day, and testing the day's
/// whole-day trough against the locked level can declare a breach that
/// happened before the lock, at a level that was legal at the time. The bias
/// is conservative (it over-declares breaches, so P(pass) comes out too low),
/// and it affects **at most one day per path**, because the running peak is
/// non-decreasing and therefore crosses any level exactly once.
///
/// M3 passes `L + D` — the cumulative-PnL peak at which
/// `peak − drawdown_amount` first reaches the lock — and gets back the day
/// whose intraday path it must retain, or `None` if the lock was never
/// reachable. The count of approximate days is
/// [`approximate_day_count`].
///
/// # Why the crossing day is always flagged
///
/// A day's `peak_from_open` is by definition at least its `close_pnl`, so a
/// summary can never certify that the peak first reached a level *at* the
/// day's final mark rather than earlier inside it. Claiming otherwise would be
/// resolving an ambiguity in the flattering direction, which is the one thing
/// this whole document is against.
///
/// # Which ratchet this answers for
///
/// The **intraday** ratchet (`peak_equity_including_unrealized` — Apex, TPT
/// PRO). An account whose ratchet is `highest_daily_closing_equity` advances
/// its peak only at a close, so its lock engages at a close and its day
/// summary is exact; those accounts have no approximate day at all.
#[must_use]
pub fn intraday_peak_crossing(days: &[DayRecord], level_nano_usd: NanoUsd) -> Option<PeakCrossing> {
    let mut cum_at_open: NanoUsd = 0;
    let mut peak: NanoUsd = 0;
    for (day_index, day) in days.iter().enumerate() {
        let before = peak;
        let day_peak = cum_at_open + day.peak_from_open_nano_usd;
        if day_peak > peak {
            peak = day_peak;
        }
        if before < level_nano_usd && peak >= level_nano_usd {
            return Some(PeakCrossing {
                day_index,
                trading_day_key: day.trading_day_key,
                peak_before_nano_usd: before,
                peak_after_nano_usd: peak,
            });
        }
        cum_at_open += day.close_pnl_nano_usd;
    }
    None
}

/// How many retained days are **conservative rather than exact** for an
/// account whose ratchet locks once the running peak reaches
/// `level_nano_usd`. Zero or one, always — see [`intraday_peak_crossing`].
///
/// This is the counter spec §3.3.1 requires beside any number derived from the
/// day summaries: a number that is wrong in the safe direction and says so is
/// acceptable; one that is wrong in the flattering direction is not.
#[must_use]
pub fn approximate_day_count(days: &[DayRecord], level_nano_usd: NanoUsd) -> usize {
    usize::from(intraday_peak_crossing(days, level_nano_usd).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// $1 in nano-USD, the unit every boundary test below moves by.
    const DOLLAR: NanoUsd = 1_000_000_000;
    const CASH: NanoUsd = 100_000 * DOLLAR;

    /// A capture over `keys`, anchored at $100k.
    fn capture(keys: &[i64]) -> AccountCapture<'_> {
        AccountCapture::new(keys, CASH, TimeFrame::M1)
    }

    /// Feeds equity levels (in whole dollars, absolute) one per bar.
    fn feed(keys: &[i64], dollars: &[i64]) -> AccountSeries {
        let mut c = capture(keys);
        for (i, &d) in dollars.iter().enumerate() {
            c.observe(i, d * DOLLAR).expect("well-formed fixture");
        }
        c.finish()
    }

    /// The retained artifact's size is a load-bearing number: it is what makes
    /// 16 years of sessions 226 KB instead of 4.98 GiB. Pinned so a field
    /// added without thought shows up as a failing test rather than as a
    /// memory regression nobody measures.
    #[test]
    fn a_day_record_is_fifty_six_bytes() {
        // i64 key + Range<usize> (2 × 8) + four i64 = 8 + 16 + 32 = 56.
        assert_eq!(size_of::<DayRecord>(), 56);
        // 252 sessions/year × 16 years × 56 B = 225,792 B.
        assert_eq!(252 * 16 * size_of::<DayRecord>(), 225_792);
    }

    /// Series 2 is O(1) in the bar count, and this is the whole claim.
    #[test]
    fn the_high_water_reducer_is_sixteen_bytes() {
        assert_eq!(size_of::<HighWaterState>(), 16);
    }

    /// Hand-derived. Anchor $100,000; one trading day, five marks:
    /// 100,000 → 100,300 → 99,800 → 100,900 → 100,400.
    /// cum:          0        300      −200       900       400
    /// running peak: 0        300       300       900       900
    /// drop:         0          0       500         0       500
    ///
    /// So peak_cum = 900, max_drop = 500; and within the day, measured from
    /// the day's open (cum 0): peak_from_open = 900, trough = −200,
    /// max_drop_from_running_peak = 500, close_pnl = 400.
    #[test]
    fn one_day_of_marks_reduces_to_the_four_numbers() {
        let keys = [7i64; 5];
        let s = feed(&keys, &[100_000, 100_300, 99_800, 100_900, 100_400]);
        assert_eq!(s.high_water.peak_cum_nano_usd, 900 * DOLLAR);
        assert_eq!(s.high_water.max_drop_from_peak_nano_usd, 500 * DOLLAR);
        assert_eq!(s.days.len(), 1);
        let d = &s.days[0];
        assert_eq!(d.trading_day_key, 7);
        assert_eq!(d.bars, 0..5);
        assert_eq!(d.close_pnl_nano_usd, 400 * DOLLAR);
        assert_eq!(d.peak_from_open_nano_usd, 900 * DOLLAR);
        assert_eq!(d.trough_from_open_nano_usd, -200 * DOLLAR);
        assert_eq!(d.max_drop_from_running_peak_nano_usd, 500 * DOLLAR);
        assert_eq!(s.marks, 5);
    }

    /// Day two opens at day one's close, not at its own first mark, and not at
    /// the anchor. Anchor $100,000.
    ///
    /// day 0 (bars 0–1): 100,000 → 100,500. close_pnl = +500.
    /// day 1 (bars 2–3): 100,200 → 100,700. Its open is cum 500, so its first
    ///   mark is −300 from the open — the overnight gap, which a day slicer
    ///   that measured from the first mark of the day would report as zero.
    ///   peak_from_open = +200, trough = −300, close_pnl = +200.
    #[test]
    fn a_day_opens_at_the_previous_days_close_so_the_overnight_gap_is_visible() {
        let keys = [0i64, 0, 1, 1];
        let s = feed(&keys, &[100_000, 100_500, 100_200, 100_700]);
        assert_eq!(s.days.len(), 2);
        assert_eq!(s.days[0].close_pnl_nano_usd, 500 * DOLLAR);
        assert_eq!(s.days[0].bars, 0..2);

        let d1 = &s.days[1];
        assert_eq!(d1.bars, 2..4);
        assert_eq!(d1.close_pnl_nano_usd, 200 * DOLLAR);
        assert_eq!(d1.trough_from_open_nano_usd, -300 * DOLLAR);
        assert_eq!(d1.peak_from_open_nano_usd, 200 * DOLLAR);
        // The within-day decline is measured from a running peak seeded at the
        // day's open, so the overnight gap down of 300 counts as a decline —
        // the open is a level the account genuinely stood at the night before.
        // Seeding at the day's FIRST MARK instead would report 0 here.
        assert_eq!(d1.max_drop_from_running_peak_nano_usd, 300 * DOLLAR);
        // Day PnL sums to the run's total: 500 + 200 = 700.
        let total: NanoUsd = s.days.iter().map(|d| d.close_pnl_nano_usd).sum();
        assert_eq!(total, 700 * DOLLAR);
    }

    /// §2.2: `peak(t) = max(0, …)`, so the threshold opens at −D and a
    /// day-one loss of D breaches. A run that only ever loses must report a
    /// peak of zero, not of its own first mark.
    #[test]
    fn the_peak_starts_at_zero_so_a_losing_run_gets_no_grace() {
        let s = feed(&[0, 0, 0], &[99_400, 98_900, 98_100]);
        assert_eq!(s.high_water.peak_cum_nano_usd, 0);
        assert_eq!(s.high_water.max_drop_from_peak_nano_usd, 1_900 * DOLLAR);
    }

    /// The ±1 nanodollar boundary of the captured decline, which is the number
    /// a breach test compares against a drawdown allowance `D`.
    ///
    /// Peak is +$2,000 (cum 2_000e9). A mark at cum 0 is a decline of exactly
    /// 2_000e9; one nanodollar higher is 1_999_999_999_999 and one lower is
    /// 2_000_000_000_001. With D = $2,000, `max_drop >= D` must be true at the
    /// exact touch and false one nanodollar short of it — "touches or falls
    /// below" is a breach for Apex, Topstep and TPT alike (spec §2.2).
    #[test]
    fn the_captured_decline_is_exact_to_the_nanodollar() {
        let d: NanoUsd = 2_000 * DOLLAR;
        let keys = [0i64; 2];

        let mut exact = capture(&keys);
        exact.observe(0, CASH + d).expect("fixture");
        exact.observe(1, CASH).expect("fixture");
        assert_eq!(exact.finish().high_water.max_drop_from_peak_nano_usd, d);

        let mut short = capture(&keys);
        short.observe(0, CASH + d).expect("fixture");
        short.observe(1, CASH + 1).expect("fixture");
        assert_eq!(
            short.finish().high_water.max_drop_from_peak_nano_usd,
            d - 1,
            "one nanodollar above the level is not a touch"
        );

        let mut past = capture(&keys);
        past.observe(0, CASH + d).expect("fixture");
        past.observe(1, CASH - 1).expect("fixture");
        assert_eq!(past.finish().high_water.max_drop_from_peak_nano_usd, d + 1);
    }

    /// The keys have to describe the series that is replayed, or every
    /// boundary after the mismatch is in the wrong place. Refused, not padded.
    #[test]
    fn a_short_key_slice_is_refused() {
        let keys = [0i64, 0];
        let mut c = capture(&keys);
        c.observe(0, CASH).expect("fixture");
        c.observe(1, CASH).expect("fixture");
        assert_eq!(
            c.observe(2, CASH),
            Err(CaptureError::DayKeysExhausted {
                bar_index: 2,
                day_keys: 2
            })
        );
    }

    /// A key going backwards means the feed was not availability-ordered.
    #[test]
    fn keys_going_backwards_are_refused() {
        let keys = [5i64, 4];
        let mut c = capture(&keys);
        c.observe(0, CASH).expect("fixture");
        assert_eq!(
            c.observe(1, CASH),
            Err(CaptureError::DayKeysOutOfOrder {
                bar_index: 1,
                previous: 5,
                found: 4
            })
        );
    }

    #[test]
    fn marks_must_arrive_in_bar_order() {
        let keys = [0i64, 0, 0];
        let mut c = capture(&keys);
        c.observe(0, CASH).expect("fixture");
        assert_eq!(
            c.observe(2, CASH),
            Err(CaptureError::BarIndexOutOfOrder {
                expected: 1,
                found: 2
            })
        );
    }

    /// Trading days need not be contiguous keys — a weekend is a gap, and the
    /// records must not invent the missing days.
    #[test]
    fn a_weekend_gap_produces_two_days_not_four() {
        let s = feed(&[10, 10, 13, 13], &[100_000, 100_100, 100_050, 100_200]);
        assert_eq!(
            s.days.iter().map(|d| d.trading_day_key).collect::<Vec<_>>(),
            vec![10, 13]
        );
    }

    /// Hand-derived worst-day distribution. Five days closing
    /// −900, +400, −250, +100, −50 (dollars) with troughs
    /// −1,200, −100, −600, 0, −75.
    ///
    /// Ascending closes: −900, −250, −50, +100, +400.
    /// Ascending troughs: −1,200, −600, −100, −75, 0.
    /// Nearest rank at p on n = 5: rank = ceil(p·5/100).
    ///   p = 1 → rank 1 → the worst. p = 50 → ceil(2.5) = 3 → the median.
    /// The pair is the finding: the worst close is −900 but the worst trough
    /// is −1,200, so a daily-close model never sees $300 of that day.
    #[test]
    fn the_worst_day_distribution_is_two_distributions() {
        let days = [
            day(0, -900, -1_200),
            day(1, 400, -100),
            day(2, -250, -600),
            day(3, 100, 0),
            day(4, -50, -75),
        ];
        let w = WorstDayDistribution::from_days(&days);
        assert_eq!(w.n_days(), 5);
        assert_eq!(w.worst_close_nano_usd(), Some(-900 * DOLLAR));
        assert_eq!(w.worst_trough_nano_usd(), Some(-1_200 * DOLLAR));
        assert_eq!(w.close_percentile_nano_usd(1), Some(-900 * DOLLAR));
        assert_eq!(w.close_percentile_nano_usd(50), Some(-50 * DOLLAR));
        assert_eq!(w.close_percentile_nano_usd(100), Some(400 * DOLLAR));
        assert_eq!(w.trough_percentile_nano_usd(50), Some(-100 * DOLLAR));
        // A percentile outside 1–100, or no days at all, has no answer.
        assert_eq!(w.close_percentile_nano_usd(0), None);
        assert_eq!(w.close_percentile_nano_usd(101), None);
        assert_eq!(
            WorstDayDistribution::from_days(&[]).worst_close_nano_usd(),
            None
        );
    }

    /// A day whose close and trough are `close` and `trough` whole dollars.
    fn day(key: i64, close: i64, trough: i64) -> DayRecord {
        DayRecord {
            trading_day_key: key,
            bars: 0..1,
            close_pnl_nano_usd: close * DOLLAR,
            peak_from_open_nano_usd: close.max(0) * DOLLAR,
            trough_from_open_nano_usd: trough * DOLLAR,
            max_drop_from_running_peak_nano_usd: (close.max(0) - trough) * DOLLAR,
        }
    }

    /// The approximate-case counter. Days closing +1,000 / +1,000 / +1,000
    /// with intraday peaks of +1,500 / +1,200 / +1,000 relative to each open.
    ///
    /// cum at each open: 0, 1,000, 2,000.
    /// day peaks in cum space: 1,500 / 2,200 / 3,000.
    /// running peak after each day: 1,500 / 2,200 / 3,000.
    ///
    /// A lock reachable at a peak of 2,200 crosses on day 1; at 2,201 it
    /// crosses on day 2; at 3,001 it is never reached and there is no
    /// approximate day at all.
    #[test]
    fn the_lock_crossing_day_is_found_and_counted_once() {
        let days = [
            peaked_day(0, 1_000, 1_500),
            peaked_day(1, 1_000, 1_200),
            peaked_day(2, 1_000, 1_000),
        ];
        let at = |level: i64| intraday_peak_crossing(&days, level * DOLLAR);

        assert_eq!(at(2_200).map(|c| c.day_index), Some(1));
        // One nanodollar past day 1's peak pushes the crossing to day 2.
        assert_eq!(
            intraday_peak_crossing(&days, 2_200 * DOLLAR + 1).map(|c| c.day_index),
            Some(2)
        );
        assert_eq!(at(3_000).map(|c| c.day_index), Some(2));
        assert_eq!(intraday_peak_crossing(&days, 3_000 * DOLLAR + 1), None);

        // Never more than one approximate day, whatever the level.
        for level in [-1_000, 0, 1, 1_500, 2_200, 3_000, 9_999] {
            assert!(approximate_day_count(&days, level * DOLLAR) <= 1);
        }
        assert_eq!(approximate_day_count(&days, 2_200 * DOLLAR), 1);
        assert_eq!(approximate_day_count(&days, 3_001 * DOLLAR), 0);
    }

    /// A day closing `close` with an intraday peak of `peak` above its open.
    fn peaked_day(key: i64, close: i64, peak: i64) -> DayRecord {
        DayRecord {
            trading_day_key: key,
            bars: 0..1,
            close_pnl_nano_usd: close * DOLLAR,
            peak_from_open_nano_usd: peak * DOLLAR,
            trough_from_open_nano_usd: 0,
            max_drop_from_running_peak_nano_usd: (peak - close) * DOLLAR,
        }
    }

    /// Spec §3.3.1's sufficiency test, exercised on captured numbers: a day
    /// carrying in a peak `h` above its open breaches iff its trough reaches
    /// `h − D` **or** its within-day decline reaches `D`.
    ///
    /// The day, with h = +$500 carried in and D = $2,000:
    /// open 0 → +500 → −1,500 → close −1,000.
    ///   trough_from_open = −1,500; h − D = 500 − 2,000 = −1,500.
    ///     −1,500 ≤ −1,500 ⇒ breach by the first clause. `≤`, not `<`:
    ///     touching the threshold is a breach (spec §2.2, all three firms).
    ///   max_drop_from_running_peak = 500 − (−1,500) = 2,000 ≥ 2,000 ⇒ breach
    ///     by the second clause as well.
    ///
    /// Now move the trough up by **one nanodollar**, to −1,500 + 1e−9:
    ///   trough = −1,499.999999999 > −1,500 ⇒ first clause silent.
    ///   max_drop = 500 − (−1,500 + 1e−9) = 2,000 − 1e−9 < 2,000 ⇒ second
    ///     clause silent.
    /// One nanodollar is the difference between a breached account and a live
    /// one, and the capture has to be exact at that resolution or the whole
    /// first-passage estimate is noise.
    #[test]
    fn the_day_summary_decides_a_breach_at_the_nanodollar() {
        let d: NanoUsd = 2_000 * DOLLAR;
        let h: NanoUsd = 500 * DOLLAR;
        let breaches = |day: &DayRecord| {
            day.trough_from_open_nano_usd <= h - d || day.max_drop_from_running_peak_nano_usd >= d
        };

        let s = feed(&[0i64; 4], &[100_000, 100_500, 98_500, 99_000]);
        let day = &s.days[0];
        assert_eq!(day.trough_from_open_nano_usd, -1_500 * DOLLAR);
        assert_eq!(day.max_drop_from_running_peak_nano_usd, 2_000 * DOLLAR);
        assert!(breaches(day));

        // The identical path, one nanodollar shallower at the trough. Both
        // clauses go silent together, because both read the same number.
        let mut safe = capture(&[0i64; 4]);
        safe.observe(0, CASH).expect("fixture");
        safe.observe(1, CASH + 500 * DOLLAR).expect("fixture");
        safe.observe(2, CASH - 1_500 * DOLLAR + 1).expect("fixture");
        safe.observe(3, CASH - 1_000 * DOLLAR).expect("fixture");
        let safe = safe.finish();
        let day = &safe.days[0];
        assert_eq!(day.trough_from_open_nano_usd, -1_500 * DOLLAR + 1);
        assert_eq!(day.max_drop_from_running_peak_nano_usd, 2_000 * DOLLAR - 1);
        assert!(!breaches(day));
    }
}
