//! Bars folded into per-day buckets, keyed by **availability**.
//!
//! A volume-crossover rule compares two contracts' volume "per session", and a
//! session is the calendar's business — so when a [`Calendar`] is supplied the
//! bucket is its trading day, and only otherwise the UTC civil day.
//!
//! The difference is not cosmetic. 00:00 UTC falls in the *middle* of a Globex
//! session, so a CME week that has five sessions produces **six** UTC-day
//! buckets: the Sunday-evening open gets one of its own, holding about an hour
//! of the thinnest trade of the week. A crossover rule reading those buckets
//! compares two contracts over that hour as though it were a session, and
//! `confirm_days` counts it as one of its consecutive sessions — so
//! `confirm_days = 2` spanning a weekend never means two sessions. With a
//! calendar the week is five buckets and it does.
//!
//! [`Calendar`]: crate::calendar::Calendar
//!
//! Bucketing on `avail_ts` rather than `ts_open` is not a detail. CLAUDE.md
//! §2.1 forbids ordering, joining, or triggering on event time, and a bucket
//! key is a join key: two contracts' volumes are compared *because they share
//! a bucket*. Keyed on availability, a bucket's own availability is trivially
//! the largest `avail_ts` it contains, and a roll decided from it can be
//! stamped with exactly that instant. Keyed on `ts_open` it would not be:
//! the bucket would contain a bar that was still in the future when the
//! bucket "closed".
//!
//! The cost is one bar of skew — a 1m bar opening at 23:59 becomes available
//! at 00:00 and lands in the next bucket. That shifts a volume total by one
//! bar out of ~1400, and it never shifts information earlier, which is the
//! only direction that matters.

use crucible_core::events::{Bar, MarketEvent};
use crucible_core::traits::Feed;
use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};

use crate::calendar::Calendar;

use crate::ingest::window::{date_of, days_from_civil};

use super::error::ContinuousError;

/// One day's trading in one contract, as it became knowable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionBar {
    /// Days since the Unix epoch of `avail_ts` — the bucket key.
    pub day: i64,
    /// Contracts traded in the bucket.
    pub volume: u64,
    /// `avail_ts` of the last bar in the bucket: the earliest instant this
    /// bucket's totals could be known, and therefore the earliest instant a
    /// roll decided from them could take effect.
    pub avail_ts: Ts,
    /// Close of that last bar — the tradeable price a roll decided here uses
    /// to measure the gap between two contracts.
    pub close: Price,
}

/// One contract's bars, reduced to what a roll rule needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSeries {
    /// The instrument as the **archive** spells it (`ESH4`). This is the key
    /// the curated store is partitioned by and the string a roll table
    /// records, so the feed can find the bars again.
    pub instrument: InstrumentId,
    /// Bar interval the series was measured at.
    pub tf: TimeFrame,
    /// `ts_open` of the first bar.
    pub first_ts_open: Ts,
    /// `ts_open` of the last bar.
    pub last_ts_open: Ts,
    /// Day buckets, ascending and unique in `day`.
    pub sessions: Vec<SessionBar>,
}

impl ContractSeries {
    /// The bucket for `day`, if the contract traded that day.
    #[must_use]
    pub fn session(&self, day: i64) -> Option<&SessionBar> {
        self.sessions
            .binary_search_by_key(&day, |s| s.day)
            .ok()
            .and_then(|i| self.sessions.get(i))
    }
}

/// Folds availability-ordered bars into [`SessionBar`]s.
#[derive(Debug)]
pub struct SessionAccumulator {
    instrument: InstrumentId,
    tf: TimeFrame,
    calendar: Option<Calendar>,
    first_ts_open: Option<Ts>,
    last_ts_open: Option<Ts>,
    current: Option<SessionBar>,
    sessions: Vec<SessionBar>,
}

impl SessionAccumulator {
    /// A fresh accumulator for one instrument and interval.
    #[must_use]
    pub fn new(instrument: InstrumentId, tf: TimeFrame) -> SessionAccumulator {
        SessionAccumulator {
            instrument,
            tf,
            calendar: None,
            first_ts_open: None,
            last_ts_open: None,
            current: None,
            sessions: Vec::new(),
        }
    }

    /// Folds in one bar.
    ///
    /// # Errors
    /// [`ContinuousError::OutOfOrderSegments`] if `ts_open` does not exceed
    /// the previous bar's. The rule needs bars in availability order to know
    /// which close is the last of a bucket, and a feed that breaks its own
    /// contract must not produce a plausible-looking roll table.
    /// Buckets by this exchange's trading day instead of the UTC civil day.
    ///
    /// Strongly preferred, and the reason is not cosmetic. A CME week has five
    /// sessions but **six** UTC civil days carrying bars: 00:00 UTC falls in
    /// the middle of a Globex session, so the Sunday-evening open lands in its
    /// own bucket holding roughly one hour of the thinnest trade of the week.
    /// A volume-crossover rule then compares two contracts over that hour as
    /// though it were a session, and `confirm_days` counts it as one of its
    /// consecutive sessions — so `confirm_days = 2` across a weekend never
    /// means what it says. Keying on [`Calendar::trading_day`] is still an
    /// availability-keyed join, so it costs nothing under §2.1 (D-0041).
    #[must_use]
    pub fn with_calendar(mut self, calendar: Calendar) -> SessionAccumulator {
        self.calendar = Some(calendar);
        self
    }

    /// Which bucket an instant belongs to.
    fn bucket_of(&self, avail_ts: Ts) -> i64 {
        match &self.calendar {
            Some(cal) => days_from_civil(cal.trading_day(avail_ts)),
            None => days_from_civil(date_of(avail_ts)),
        }
    }

    pub fn push(&mut self, bar: &Bar) -> Result<(), ContinuousError> {
        if let Some(prev) = self.last_ts_open
            && bar.ts_open <= prev
        {
            return Err(ContinuousError::OutOfOrderSegments {
                contract: self.instrument.as_str().to_owned(),
                prev,
                next: bar.ts_open,
            });
        }
        if self.first_ts_open.is_none() {
            self.first_ts_open = Some(bar.ts_open);
        }
        self.last_ts_open = Some(bar.ts_open);

        let avail_ts = bar.avail_ts();
        let day = self.bucket_of(avail_ts);
        match &mut self.current {
            Some(open) if open.day == day => {
                // Volumes at futures scale never approach 2^64; saturating
                // beats a panic on data that is corrupt anyway, and the
                // curated store already refuses a volume it cannot store.
                open.volume = open.volume.saturating_add(bar.volume);
                open.avail_ts = avail_ts;
                open.close = bar.close;
            }
            slot => {
                if let Some(done) = slot.take() {
                    self.sessions.push(done);
                }
                *slot = Some(SessionBar {
                    day,
                    volume: bar.volume,
                    avail_ts,
                    close: bar.close,
                });
            }
        }
        Ok(())
    }

    /// Closes the last bucket and yields the series, or `None` if no bar was
    /// ever pushed — an empty contract is not a contract with an empty series,
    /// it is a contract that never traded.
    #[must_use]
    pub fn finish(mut self) -> Option<ContractSeries> {
        if let Some(done) = self.current.take() {
            self.sessions.push(done);
        }
        Some(ContractSeries {
            instrument: self.instrument,
            tf: self.tf,
            first_ts_open: self.first_ts_open?,
            last_ts_open: self.last_ts_open?,
            sessions: self.sessions,
        })
    }
}

/// Drains a feed into a [`ContractSeries`].
///
/// # Errors
/// [`ContinuousError::OutOfOrderSegments`] if the feed breaks its own
/// ordering contract.
pub fn series_of<F: Feed>(
    feed: &mut F,
    instrument: InstrumentId,
    tf: TimeFrame,
    calendar: Option<&Calendar>,
) -> Result<Option<ContractSeries>, ContinuousError> {
    let mut acc = SessionAccumulator::new(instrument, tf);
    if let Some(cal) = calendar {
        acc = acc.with_calendar(cal.clone());
    }
    while let Some(MarketEvent::Bar(bar)) = feed.next_event() {
        acc.push(&bar)?;
    }
    Ok(acc.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000_000_000;
    const DAY: i64 = 86_400 * 1_000_000_000;

    fn bar(ts_open: i64, close: i64, volume: u64) -> Bar {
        Bar {
            instrument: InstrumentId::new("ESH4"),
            tf: TimeFrame::M1,
            ts_open: Ts(ts_open),
            open: Price::from_nanos(close),
            high: Price::from_nanos(close),
            low: Price::from_nanos(close),
            close: Price::from_nanos(close),
            volume,
            signal_offset: Price::ZERO,
        }
    }

    fn accumulate(bars: &[Bar]) -> ContractSeries {
        let mut acc = SessionAccumulator::new(InstrumentId::new("ESH4"), TimeFrame::M1);
        for b in bars {
            acc.push(b).expect("ordered bars");
        }
        acc.finish().expect("at least one bar")
    }

    // Hand-derived. 1970-01-01 is day 0. A 1m bar opening at 00:00 on day 1
    // is available at 00:01 on day 1, so it buckets into day 1. Volumes
    // 10 + 20 + 30 = 60; the bucket's close is the LAST bar's close.
    #[test]
    fn bars_fold_into_one_bucket_per_utc_day() {
        let series = accumulate(&[
            bar(DAY, 100, 10),
            bar(DAY + MIN, 101, 20),
            bar(DAY + 2 * MIN, 102, 30),
        ]);
        assert_eq!(series.sessions.len(), 1);
        let s = series.sessions[0];
        assert_eq!(s.day, 1);
        assert_eq!(s.volume, 60);
        assert_eq!(s.close, Price::from_nanos(102));
        assert_eq!(s.avail_ts, Ts(DAY + 3 * MIN));
        assert_eq!(series.first_ts_open, Ts(DAY));
        assert_eq!(series.last_ts_open, Ts(DAY + 2 * MIN));
    }

    // The documented one-bar skew, pinned so it is a decision and not a
    // surprise: a 1m bar opening at 23:59 on day 1 becomes available at
    // 00:00 on day 2, and buckets with day 2.
    #[test]
    fn the_last_bar_of_a_utc_day_buckets_with_the_next_one() {
        let last_minute = 2 * DAY - MIN;
        let series = accumulate(&[bar(DAY, 100, 7), bar(last_minute, 200, 9)]);
        assert_eq!(series.sessions.len(), 2);
        assert_eq!(series.sessions[0].day, 1);
        assert_eq!(series.sessions[1].day, 2);
        assert_eq!(series.sessions[1].avail_ts, Ts(2 * DAY));
        assert_eq!(series.sessions[1].volume, 9);
    }

    #[test]
    fn a_bucket_can_be_looked_up_by_day() {
        let series = accumulate(&[bar(DAY, 100, 7), bar(3 * DAY, 200, 9)]);
        assert_eq!(series.session(1).map(|s| s.volume), Some(7));
        assert_eq!(series.session(3).map(|s| s.volume), Some(9));
        assert_eq!(series.session(2), None);
    }

    #[test]
    fn an_out_of_order_bar_is_refused_rather_than_bucketed() {
        let mut acc = SessionAccumulator::new(InstrumentId::new("ESH4"), TimeFrame::M1);
        acc.push(&bar(2 * MIN, 100, 1)).expect("first");
        let err = acc
            .push(&bar(MIN, 100, 1))
            .expect_err("must refuse a backward bar");
        assert!(
            matches!(err, ContinuousError::OutOfOrderSegments { .. }),
            "{err}"
        );
    }

    #[test]
    fn a_contract_that_never_traded_has_no_series() {
        let acc = SessionAccumulator::new(InstrumentId::new("ESH4"), TimeFrame::M1);
        assert!(acc.finish().is_none());
    }
}

#[cfg(test)]
mod calendar_bucket_tests {
    use super::*;
    use crate::calendar::CivilDate;

    const MIN: i64 = 60_000_000_000;

    /// A 23-hour ES-shaped session for CME trading day `date`, one bar a
    /// minute, taken straight from the calendar so the fixture cannot drift
    /// away from what the calendar believes.
    fn bar(ts_open: i64, close: i64, volume: u64) -> Bar {
        Bar {
            instrument: InstrumentId::new("ESH4"),
            tf: TimeFrame::M1,
            ts_open: Ts(ts_open),
            open: Price::from_points(close),
            high: Price::from_points(close),
            low: Price::from_points(close),
            close: Price::from_points(close),
            volume,
            signal_offset: Price::ZERO,
        }
    }

    fn session_bars(cal: &Calendar, date: CivilDate, volume: u64) -> Vec<Bar> {
        let mut out = Vec::new();
        for (start, end) in cal.open_intervals(date) {
            let mut ts = start.0;
            while ts < end.0 {
                out.push(bar(ts, 5000, volume));
                ts += MIN;
            }
        }
        out
    }

    /// Monday 2024-01-08 through Friday 2024-01-12: five CME trading days.
    fn one_week(cal: &Calendar) -> Vec<Bar> {
        let mut bars = Vec::new();
        for day in 8..=12 {
            bars.extend(session_bars(
                cal,
                CivilDate {
                    year: 2024,
                    month: 1,
                    day,
                },
                100,
            ));
        }
        bars.sort_by_key(|b| b.ts_open);
        bars
    }

    fn cme() -> Calendar {
        Calendar::by_id("cme_globex_equity_index").expect("the bundled table parses")
    }

    // The defect this fixes: 00:00 UTC falls inside a Globex session, so a
    // five-session week produces SIX UTC-day buckets. The extra one is the
    // Sunday-evening open — one hour in winter — and a crossover rule would
    // weigh it as a full session.
    #[test]
    fn a_five_session_week_is_six_utc_day_buckets_but_five_trading_days() {
        let cal = cme();
        let bars = one_week(&cal);

        let mut utc = SessionAccumulator::new(InstrumentId::new("ESH4"), TimeFrame::M1);
        for b in &bars {
            utc.push(b).expect("ordered");
        }
        let utc = utc.finish().expect("bars were pushed");

        let mut cme_days = SessionAccumulator::new(InstrumentId::new("ESH4"), TimeFrame::M1)
            .with_calendar(cal.clone());
        for b in &bars {
            cme_days.push(b).expect("ordered");
        }
        let cme_days = cme_days.finish().expect("bars were pushed");

        assert_eq!(utc.sessions.len(), 6, "the UTC week has a Sunday sliver");
        assert_eq!(cme_days.sessions.len(), 5, "the exchange week has five");

        // Every bucket keeps its bars — this is a re-partition, not a filter.
        let utc_volume: u64 = utc.sessions.iter().map(|s| s.volume).sum();
        let cme_volume: u64 = cme_days.sessions.iter().map(|s| s.volume).sum();
        assert_eq!(utc_volume, cme_volume);

        // The sliver is a small fraction of a session, which is exactly why
        // weighing it as one distorts a volume comparison.
        let smallest = utc
            .sessions
            .iter()
            .map(|s| s.volume)
            .min()
            .expect("six buckets");
        let typical = cme_volume / 5;
        assert!(
            smallest * 5 < typical,
            "sliver {smallest} should be a small fraction of a session {typical}"
        );

        // With the calendar, every bucket is a genuine trading day.
        for session in &cme_days.sessions {
            let date = crate::ingest::window::civil_from_days(session.day);
            assert!(
                cal.is_trading_day(date),
                "{date} is not a trading day but has a bucket"
            );
        }
    }
}
