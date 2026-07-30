//! Coarser bars from finer ones, cut on the exchange's own sessions.
//!
//! We bought `ohlcv-1s` and `ohlcv-1m` and nothing else (`docs/DATA_PLAN.md` —
//! the hourly and daily aggregates are $190/GB), so every idea stated on
//! 5-minute, hourly or daily bars was unrunnable until this module existed.
//! [`TimeFrame::M5`] and [`TimeFrame::M15`] have always been *deliberately*
//! unmapped in [`timeframe_for_schema`](crate::transcode::timeframe_for_schema)
//! rather than silently aliased onto something the vendor never sent; this is
//! the one path that produces them, and it is a **read-time** path (D-0077).
//!
//! ## Why nothing is written to disk
//!
//! A curated partition is named after the raw window it came from and records
//! exactly one `source_file_blake3`, so one raw file fans out to one curated
//! file and nothing ever has to be merged (D-0036). Raw windows are monthly and
//! a month boundary lands *inside* a CME session — 2024-02-01T00:00Z is 18:00
//! CT on 31 January, and the trading day that opened at 17:00 CT is only an
//! hour old. A daily bar for that trading day therefore has constituents in two
//! different raw windows, so writing it would need exactly the read-modify-write
//! merge D-0036 exists to prevent. Resampling on read has no such seam:
//! [`ParquetBarFeed`] already concatenates every window file in order, and this
//! module aggregates the concatenation.
//!
//! It also costs almost nothing. One integer pass over sixteen years of ES
//! 1-minute bars is a few million comparisons; the alternative is a second copy
//! of the archive that can go stale against the first.
//!
//! ## The bucket rule
//!
//! **A bar belongs to bucket `k` of its trading day, where the day's clock
//! starts at the session open:**
//!
//! ```text
//! day      = calendar.trading_day(bar.avail_ts())      // D-0062 / D-0071
//! anchor   = calendar.session_open(day)
//! k        = (bar.ts_open - anchor) / target_tf
//! ts_open  = anchor + k * target_tf
//! ```
//!
//! Three consequences, each of which is the point:
//!
//! - **No resampled bar spans a session boundary.** Two bars in different
//!   trading days have different anchors and therefore different buckets, so
//!   the last minute before the maintenance break and the first minute after it
//!   can never land in the same bar. On a naive UTC grid they would: a UTC-day
//!   daily bar for 3 January 2024 holds that day's 00:00–22:00Z bars *and* the
//!   23:00Z bars that opened the fourth's session — two trading days in one
//!   "daily" bar, which is the exact shape of a signal that fires on the
//!   calendar rather than on the market.
//! - **Daily bars are trading-day bars.** With a 24-hour target the whole
//!   session is one bucket, opening when the exchange opened.
//! - **An early close simply produces a short last bucket.** Nothing special
//!   happens: the buckets after the close have no bars in them and are not
//!   emitted, and the one the close lands inside is built from the minutes that
//!   traded. A 12:15 CT close nineteen and a quarter hours after a 17:00 CT
//!   open makes bucket 19 of a one-hour resample fifteen minutes long.
//!
//! ## Why this is not lookahead
//!
//! `avail_ts` is `ts_open + tf` and is computed by
//! [`Bar::avail_ts`](crucible_core::events::Bar::avail_ts), never here (§2.1).
//! So the resampled bar's availability is `anchor + (k+1) * target_tf`, and the
//! invariant that has to hold is that no constituent becomes available *after*
//! the bar built from it. It holds by arithmetic, given one precondition:
//!
//! A constituent's `ts_open` lies in `[start, start + target)`. If the anchor is
//! a whole number of source intervals from the epoch and the target is a whole
//! multiple of the source, then every constituent `ts_open` is congruent to
//! `start` modulo the source interval, so the largest one is
//! `start + target - source` and its availability is exactly `start + target`.
//! Both conditions are checked: an indivisible pair is refused up front, and a
//! session open off the source grid is refused per trading day
//! ([`ResampleError::SessionOpenOffGrid`]). CME opens on the hour, so a 1-minute
//! source satisfies it everywhere; a hypothetical exchange opening at 09:30:30
//! does not, and is told so rather than quietly producing bars a strategy could
//! see a minute early.
//!
//! The **bucketing** key is `ts_open` while the **day** key is `avail_ts`, and
//! that is not an inconsistency. Which interval a bar's content describes is a
//! question about the bar's own interval; which window a bar is *ordered into*
//! is a question about when it could be known, and D-0062 settled that one for
//! fold boundaries. Bucketing on `avail_ts` would file a 10:04 bar's content
//! under the bucket beginning 10:05 — right answer to the ordering question,
//! wrong answer about what happened.
//!
//! ## What this module refuses
//!
//! Every uncertainty is a refusal, as everywhere else in `curated` — resampled
//! bars are derived from derived data, so a false refusal costs one re-run:
//!
//! - a target that is not a whole multiple of the source, or not coarser;
//! - a calendar that declares an intraday halt, because a halt is a session
//!   boundary and this module's grid is anchored per *day*. Neither bundled
//!   table declares one (D-0040 removed the 15:15–15:30 CT halt the CME
//!   contract-spec page claims, because our own archive falsifies it), so the
//!   refusal costs nothing today and stops the promise above from silently
//!   becoming false the day a table grows one;
//! - a session open that is off the source grid (above);
//! - a source bar whose interval starts before its own trading day's session
//!   open, which means the source bar itself straddles the boundary and cannot
//!   be assigned to one side of it.
//!
//! [`ParquetBarFeed`]: super::ParquetBarFeed

use std::path::Path;

use crucible_core::events::{Bar, MarketEvent};
use crucible_core::traits::Feed;
use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};

use crate::calendar::{Calendar, CalendarError, CivilDate};
use crate::catalog::TsRange;
use crate::ingest::window::days_from_civil;

use super::read::{CuratedSource, ParquetBarFeed};
use super::{BarColumns, CuratedError};

/// Version of the aggregation *semantics*.
///
/// The sibling of [`TRANSCODER_VERSION`](super::TRANSCODER_VERSION), and it
/// exists for the same reason: bumped whenever the same curated bars would
/// resample to different ones, so a stored result can say which rule produced
/// the bars behind it. Nothing persists it yet, because nothing persists
/// resampled bars — it is printed with the run instead.
pub const RESAMPLER_VERSION: u32 = 1;

/// Why a series could not be resampled.
#[derive(Debug)]
pub enum ResampleError {
    /// The target interval is not longer than the source interval.
    NotCoarser {
        /// The grain held.
        source: TimeFrame,
        /// The grain asked for.
        target: TimeFrame,
    },
    /// The target interval is not a whole multiple of the source interval.
    Indivisible {
        /// The grain held.
        source: TimeFrame,
        /// The grain asked for.
        target: TimeFrame,
    },
    /// The calendar declares an intraday halt, which is a session boundary this
    /// module's per-day bucket grid would let a bar span.
    CalendarDeclaresHalts {
        /// Which calendar.
        calendar: String,
    },
    /// No bundled calendar governs the instrument, so its sessions are unknown.
    NoCalendar {
        /// The instrument that was asked for.
        instrument: String,
    },
    /// A trading day's session opens at an instant that is not a whole number
    /// of source intervals from the epoch, so bucket arithmetic could hand a
    /// resampled bar an availability earlier than one of its constituents.
    SessionOpenOffGrid {
        /// Which calendar said so.
        calendar: String,
        /// The trading day whose open is off the grid.
        trading_day: CivilDate,
        /// The offending instant.
        session_open: Ts,
        /// The grain held.
        source: TimeFrame,
    },
    /// A source bar's interval begins before its own trading day's session
    /// open, so the bar straddles the boundary and belongs to neither side.
    BarPrecedesSessionOpen {
        /// The offending bar's interval start.
        ts_open: Ts,
        /// The trading day its availability attributed it to.
        trading_day: CivilDate,
        /// When that day's session opened.
        session_open: Ts,
    },
    /// Timestamps did not strictly increase.
    OutOfOrder {
        /// Which series — `"source"` or `"resampled"`.
        what: &'static str,
        /// The `ts_open` that came first.
        prev: Ts,
        /// The `ts_open` that failed to exceed it.
        next: Ts,
    },
    /// Summed volume does not fit a `u64`, so the bar cannot be built.
    VolumeOverflow {
        /// Interval start of the bucket that overflowed.
        ts_open: Ts,
    },
    /// The underlying curated store could not be read.
    Curated(CuratedError),
    /// A bundled calendar table could not be parsed — a build bug.
    Calendar(CalendarError),
}

impl core::fmt::Display for ResampleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ResampleError::NotCoarser { source, target } => write!(
                f,
                "cannot resample {source} bars to {target}: the target must be coarser than the \
                 source. Resampling only ever removes detail"
            ),
            ResampleError::Indivisible { source, target } => write!(
                f,
                "cannot resample {source} bars to {target}: {target} is not a whole number of \
                 {source} intervals, so a resampled bar would be built from a fraction of one \
                 and could become available before it"
            ),
            ResampleError::CalendarDeclaresHalts { calendar } => write!(
                f,
                "calendar {calendar} declares an intraday halt. A halt is a session boundary, and \
                 this build anchors its bucket grid once per trading day — so a bucket could span \
                 the break and the bar built from it would mix two sessions. Refusing rather than \
                 producing that bar (D-0077)"
            ),
            ResampleError::NoCalendar { instrument } => write!(
                f,
                "no bundled calendar governs {instrument}, so its session boundaries are unknown \
                 and every resampled bar could span one. Resampling is a statement about sessions \
                 (D-0077); without a calendar there is nothing to make it against"
            ),
            ResampleError::SessionOpenOffGrid {
                calendar,
                trading_day,
                session_open,
                source,
            } => write!(
                f,
                "calendar {calendar} opens trading day {trading_day} at {session_open}, which is \
                 not a whole number of {source} intervals from the epoch. Bucket boundaries would \
                 fall inside source bars, and a resampled bar could become available before one \
                 of the bars it was built from (§2.1)"
            ),
            ResampleError::BarPrecedesSessionOpen {
                ts_open,
                trading_day,
                session_open,
            } => write!(
                f,
                "a source bar opening at {ts_open} is attributed to trading day {trading_day}, \
                 whose session opens at {session_open} — so the bar's own interval straddles the \
                 session boundary. It belongs to neither side, and picking one would put content \
                 from before the open inside the session's first bar"
            ),
            ResampleError::OutOfOrder { what, prev, next } => write!(
                f,
                "the {what} series has ts_open {next} after {prev}. Bars must strictly increase, \
                 or replaying them would show the engine one interval twice"
            ),
            ResampleError::VolumeOverflow { ts_open } => write!(
                f,
                "summed volume for the bar opening at {ts_open} does not fit a u64; the source \
                 data is corrupt"
            ),
            ResampleError::Curated(e) => write!(f, "{e}"),
            ResampleError::Calendar(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ResampleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResampleError::Curated(e) => Some(e),
            ResampleError::Calendar(e) => Some(e),
            _ => None,
        }
    }
}

impl From<CuratedError> for ResampleError {
    fn from(e: CuratedError) -> ResampleError {
        ResampleError::Curated(e)
    }
}

impl From<CalendarError> for ResampleError {
    fn from(e: CalendarError) -> ResampleError {
        ResampleError::Calendar(e)
    }
}

/// What one resampling pass did, in the terms a report needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResampleReport {
    /// Which calendar's sessions cut the buckets. Load-bearing provenance: the
    /// same source bars under a different calendar are different bars (§2.5).
    pub calendar_id: String,
    /// The grain read from the archive.
    pub source_tf: TimeFrame,
    /// The grain produced.
    pub target_tf: TimeFrame,
    /// Bars consumed.
    pub source_bars: usize,
    /// Bars produced.
    pub bars: usize,
    /// Distinct trading days the source bars touched.
    pub trading_days: usize,
    /// The first resampled bar's bucket started before the first source bar's
    /// interval, so the requested window may have cut into it.
    ///
    /// "May", not "did": `ohlcv` has no bar for an interval that did not trade,
    /// so a thin first minute looks exactly like a truncated one. Both are
    /// worth knowing about the first bar of a sample and neither is worth
    /// hiding.
    pub first_bar_may_be_partial: bool,
    /// The last resampled bar's bucket extends past the last source bar's
    /// interval **and past nothing else** — the session was still open, so the
    /// requested window is what ended, not the exchange.
    pub last_bar_may_be_partial: bool,
}

/// Aggregates `src` from `source_tf` to `target_tf` on `calendar`'s sessions.
///
/// Integer arithmetic throughout: open is the first constituent's open, close
/// the last one's close, high and low the extremes, volume the sum. No `f64`
/// appears anywhere on this path, so the result is bit-identical on every
/// machine (§2.2, §2.3).
///
/// # Errors
/// [`ResampleError`] for any of the refusals in this module's docs, and
/// [`ResampleError::OutOfOrder`] if `src` is not strictly increasing.
pub fn resample(
    src: &BarColumns,
    source_tf: TimeFrame,
    target_tf: TimeFrame,
    calendar: &Calendar,
) -> Result<(BarColumns, ResampleReport), ResampleError> {
    let source_ns = source_tf.duration_ns();
    let target_ns = target_tf.duration_ns();
    if target_ns <= source_ns {
        return Err(ResampleError::NotCoarser {
            source: source_tf,
            target: target_tf,
        });
    }
    if target_ns % source_ns != 0 {
        return Err(ResampleError::Indivisible {
            source: source_tf,
            target: target_tf,
        });
    }
    if calendar.declares_halts() {
        return Err(ResampleError::CalendarDeclaresHalts {
            calendar: calendar.id().to_owned(),
        });
    }

    let mut out = BarColumns::default();
    let mut open_bucket: Option<Bucket> = None;
    // The anchor is recomputed only when the trading day changes, which for a
    // 1-minute source is once every ~1,380 bars.
    let mut anchored: Option<(i64, i64)> = None;
    let mut trading_days = 0usize;
    let mut last_date: Option<CivilDate> = None;

    for i in 0..src.len() {
        let ts_open = src.ts_open[i];
        if i > 0 && ts_open <= src.ts_open[i - 1] {
            return Err(ResampleError::OutOfOrder {
                what: "source",
                prev: Ts(src.ts_open[i - 1]),
                next: Ts(ts_open),
            });
        }
        // Availability decides the trading day (D-0062: a bar is attributed to
        // the session it completed in), spelled exactly as the funnel and the
        // engine spell it so the two can never disagree about a date (D-0071).
        let date = calendar.trading_day(Ts(ts_open + source_ns));
        let day = days_from_civil(date);
        let anchor = match anchored {
            Some((known, anchor)) if known == day => anchor,
            _ => {
                let anchor = calendar.session_open(date).0;
                if anchor.rem_euclid(source_ns) != 0 {
                    return Err(ResampleError::SessionOpenOffGrid {
                        calendar: calendar.id().to_owned(),
                        trading_day: date,
                        session_open: Ts(anchor),
                        source: source_tf,
                    });
                }
                anchored = Some((day, anchor));
                trading_days += 1;
                anchor
            }
        };
        if ts_open < anchor {
            return Err(ResampleError::BarPrecedesSessionOpen {
                ts_open: Ts(ts_open),
                trading_day: date,
                session_open: Ts(anchor),
            });
        }
        last_date = Some(date);

        let start = anchor + (ts_open - anchor) / target_ns * target_ns;
        match &mut open_bucket {
            Some(bucket) if bucket.day == day && bucket.start == start => {
                bucket.absorb(src, i)?;
            }
            _ => {
                if let Some(finished) = open_bucket.take() {
                    finished.emit(&mut out)?;
                }
                open_bucket = Some(Bucket::start(start, day, src, i));
            }
        }
    }
    if let Some(finished) = open_bucket.take() {
        finished.emit(&mut out)?;
    }

    // The first bucket of a sample can be cut by the request; every later one
    // is cut only by the market. The last one is cut by the request only if the
    // session it sits in was still open when the source bars ran out.
    let first_bar_may_be_partial = match (src.ts_open.first(), out.ts_open.first()) {
        (Some(&first_src), Some(&first_out)) => first_src > first_out,
        _ => false,
    };
    let last_bar_may_be_partial = match (src.ts_open.last(), out.ts_open.last(), last_date) {
        (Some(&last_src), Some(&last_out), Some(date)) => {
            let bucket_end = last_out + target_ns;
            last_src + source_ns < bucket_end && bucket_end <= calendar.session_close(date).0
        }
        _ => false,
    };

    let report = ResampleReport {
        calendar_id: calendar.id().to_owned(),
        source_tf,
        target_tf,
        source_bars: src.len(),
        bars: out.len(),
        trading_days,
        first_bar_may_be_partial,
        last_bar_may_be_partial,
    };
    Ok((out, report))
}

/// One target-interval bucket, accumulating.
struct Bucket {
    start: i64,
    day: i64,
    open: i64,
    high: i64,
    low: i64,
    close: i64,
    volume: u64,
}

impl Bucket {
    fn start(start: i64, day: i64, src: &BarColumns, i: usize) -> Bucket {
        Bucket {
            start,
            day,
            open: src.open[i],
            high: src.high[i],
            low: src.low[i],
            close: src.close[i],
            volume: src.volume[i],
        }
    }

    fn absorb(&mut self, src: &BarColumns, i: usize) -> Result<(), ResampleError> {
        self.high = self.high.max(src.high[i]);
        self.low = self.low.min(src.low[i]);
        // Open stays the first constituent's; close follows the last one.
        self.close = src.close[i];
        self.volume =
            self.volume
                .checked_add(src.volume[i])
                .ok_or(ResampleError::VolumeOverflow {
                    ts_open: Ts(self.start),
                })?;
        Ok(())
    }

    fn emit(self, out: &mut BarColumns) -> Result<(), ResampleError> {
        if let Some(&prev) = out.ts_open.last()
            && self.start <= prev
        {
            // Provably unreachable for a calendar whose session opens increase
            // with the date, which is every calendar that parses — but the
            // proof lives in this module's docs and a check costs one compare
            // per emitted bar, against a feed that has no error channel left.
            return Err(ResampleError::OutOfOrder {
                what: "resampled",
                prev: Ts(prev),
                next: Ts(self.start),
            });
        }
        out.ts_open.push(self.start);
        out.open.push(self.open);
        out.high.push(self.high);
        out.low.push(self.low);
        out.close.push(self.close);
        out.volume.push(self.volume);
        Ok(())
    }
}

/// A [`Feed`] over curated bars aggregated to a coarser grain.
///
/// Interchangeable with [`ParquetBarFeed`] from the engine's point of view, and
/// it does all its failing in [`ResampledBarFeed::open`] for the same reason:
/// [`Feed::next_event`] returns `Option`, not `Result`.
#[derive(Debug)]
pub struct ResampledBarFeed {
    instrument: InstrumentId,
    tf: TimeFrame,
    bars: BarColumns,
    cursor: usize,
    sources: Vec<CuratedSource>,
    report: ResampleReport,
}

impl ResampledBarFeed {
    /// Loads `source_tf` partitions for `instrument` and aggregates them to
    /// `target_tf` on the calendar that governs the instrument.
    ///
    /// `range` filters the **source** bars, half-open on their `ts_open`, so a
    /// request that starts mid-bucket produces a first bar built from part of
    /// its bucket — reported by
    /// [`ResampleReport::first_bar_may_be_partial`] rather than hidden.
    ///
    /// # Errors
    /// [`ResampleError::NoCalendar`] when nothing bundled governs the
    /// instrument, [`ResampleError::Curated`] for anything wrong with the
    /// underlying partitions, and the refusals in this module's docs.
    pub fn open(
        data_dir: &Path,
        instrument: &InstrumentId,
        source_tf: TimeFrame,
        target_tf: TimeFrame,
        range: Option<TsRange>,
    ) -> Result<ResampledBarFeed, ResampleError> {
        let calendar =
            Calendar::for_instrument(instrument)?.ok_or_else(|| ResampleError::NoCalendar {
                instrument: instrument.as_str().to_owned(),
            })?;
        let feed = ParquetBarFeed::open(data_dir, instrument, source_tf, range)?;
        let (bars, report) = resample(feed.columns(), source_tf, target_tf, &calendar)?;
        Ok(ResampledBarFeed {
            instrument: instrument.clone(),
            tf: target_tf,
            bars,
            cursor: 0,
            sources: feed.sources().to_vec(),
            report,
        })
    }

    /// The curated files the source bars came from, with the manifest id of the
    /// raw bytes behind each. Unchanged by resampling: a resampled bar's
    /// provenance is its constituents' provenance (D-0013).
    #[must_use]
    pub fn sources(&self) -> &[CuratedSource] {
        &self.sources
    }

    /// What the aggregation did.
    #[must_use]
    pub fn report(&self) -> &ResampleReport {
        &self.report
    }

    /// Total bars produced.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// Whether the feed holds no bars.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// `ts_open` of the first resampled bar.
    #[must_use]
    pub fn first_ts_open(&self) -> Option<Ts> {
        self.bars.ts_open.first().copied().map(Ts)
    }

    /// `ts_open` of the last resampled bar.
    #[must_use]
    pub fn last_ts_open(&self) -> Option<Ts> {
        self.bars.ts_open.last().copied().map(Ts)
    }

    /// The instrument every bar carries.
    #[must_use]
    pub fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }

    /// The bar interval produced.
    #[must_use]
    pub fn tf(&self) -> TimeFrame {
        self.tf
    }
}

impl Feed for ResampledBarFeed {
    fn next_event(&mut self) -> Option<MarketEvent> {
        let i = self.cursor;
        let &ts_open = self.bars.ts_open.get(i)?;
        self.cursor += 1;
        Some(MarketEvent::Bar(Bar {
            instrument: self.instrument.clone(),
            tf: self.tf,
            ts_open: Ts(ts_open),
            open: Price::from_nanos(self.bars.open[i]),
            high: Price::from_nanos(self.bars.high[i]),
            low: Price::from_nanos(self.bars.low[i]),
            close: Price::from_nanos(self.bars.close[i]),
            volume: self.bars.volume[i],
            // Curated partitions hold one contract's tradeable prices, so the
            // two price views coincide (D-0076). A stitched series is resampled
            // by nothing in this build: a bucket spanning a roll would mix two
            // offsets, and that needs its own decision.
            signal_offset: Price::ZERO,
        }))
    }
}

#[cfg(test)]
mod tests;
