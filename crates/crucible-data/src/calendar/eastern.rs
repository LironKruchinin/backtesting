//! US/Eastern wall-clock instants → UTC nanoseconds.
//!
//! ThetaData stamps every historical row with a **local Eastern wall clock and
//! no offset**: `2024-01-02T09:30:00.000`. That is not a quirk to paper over,
//! it is a lossy encoding, and the loss is exactly one hour twice a year. This
//! module is where the offset is put back, and it lives in `calendar` because
//! CLAUDE.md §4 puts every timezone in the workspace here and nowhere else.
//!
//! The vendor's convention was established by measurement, not by reading its
//! documentation, which does not state a timezone at all: QQQ 1-minute bars
//! begin at `09:30:00.000` on 2024-03-08 (EST, UTC−5) and again at
//! `09:30:00.000` on 2024-03-11 (EDT, UTC−4), the first session after the
//! spring-forward. A UTC encoding would have shifted by an hour across that
//! boundary; a wall-clock encoding does not. The same holds across the November
//! transition.
//!
//! ## The two pathological local times, and why both are refused
//!
//! A wall-clock encoding cannot name every instant, and names some twice:
//!
//! - **Nonexistent** (spring forward, 02:00–03:00 local on the second Sunday in
//!   March). No instant has this local time.
//! - **Ambiguous** (fall back, 01:00–02:00 local on the first Sunday in
//!   November). Two instants share the local time, an hour apart.
//!
//! Both are hard errors, because both windows fall at 01:00–03:00 on a
//! **Sunday**, and no US equity or index-options session prints then. A
//! timestamp landing in either one is corrupt vendor data, not a situation to
//! model — exactly the reasoning the table loader already applies to session
//! boundaries (see [`CalendarError::AmbiguousLocalTime`]).
//!
//! ### Why not silently pick one (D-0052)
//!
//! Picking the **earlier** instant was the original implementation and it was
//! wrong in the dangerous direction. Every event here becomes an `avail_ts` —
//! the instant its information could first be known (§2.1). Resolving an
//! ambiguous stamp to the earlier of the two candidates asserts the information
//! existed an hour before it may actually have, which makes it visible to a
//! strategy that could not have seen it. That is lookahead, produced silently,
//! in the one part of the pipeline whose whole job is to prevent it.
//!
//! Delay is the conservative direction: information withheld too long can only
//! cost measured performance, never fabricate knowledge. So if some future feed
//! genuinely trades through the ambiguous hour and a blind choice must be made,
//! the choice is the **later** instant. It is not made here, because for this
//! feed the case is vacuous and refusing surfaces the corruption instead of
//! absorbing it.
//!
//! Both arms are covered by tests, and the refusals are asserted rather than
//! assumed.
//!
//! [`CalendarError::AmbiguousLocalTime`]: super::CalendarError::AmbiguousLocalTime

use chrono::{LocalResult, NaiveDate, TimeZone};
use chrono_tz::America::New_York;
use crucible_core::types::Ts;

use crate::ingest::window::CivilDate;

/// Nanoseconds in one second.
const NANOS_PER_SECOND: i64 = 1_000_000_000;

/// Why an Eastern wall-clock stamp could not become a UTC instant.
///
/// Deliberately *not* a [`CalendarError`](super::CalendarError) variant: that
/// type documents itself as load-time-only so that answering a built
/// [`Calendar`](super::Calendar) can stay total. These failures are
/// vendor-data failures, discovered per row while decoding, and belong to a
/// different phase of the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EasternTimeError {
    /// The civil date or clock reading is not a real date/time at all.
    NotACivilInstant {
        /// The date as received.
        date: CivilDate,
        /// Hour as received.
        hour: u32,
        /// Minute as received.
        minute: u32,
        /// Second as received.
        second: u32,
        /// Nanosecond-of-second as received.
        nanos: u32,
    },
    /// The local time falls in the spring-forward gap and names no instant.
    NonexistentLocalTime {
        /// The date as received.
        date: CivilDate,
        /// Hour as received.
        hour: u32,
        /// Minute as received.
        minute: u32,
    },
    /// The local time falls in the fall-back hour and names two instants.
    ///
    /// Refused rather than resolved (D-0052): the window is 01:00–02:00 on a
    /// Sunday, when no US equity or options session prints, so a row landing
    /// here is corrupt. Guessing the earlier candidate would assert the
    /// information existed an hour before it may have — lookahead (§2.1),
    /// manufactured silently.
    AmbiguousLocalTime {
        /// The date as received.
        date: CivilDate,
        /// Hour as received.
        hour: u32,
        /// Minute as received.
        minute: u32,
        /// UTC nanoseconds of the earlier candidate.
        earlier: Ts,
        /// UTC nanoseconds of the later candidate.
        later: Ts,
    },
    /// The instant is outside the range representable in `i64` nanoseconds.
    OutOfRange {
        /// The date as received.
        date: CivilDate,
    },
}

impl core::fmt::Display for EasternTimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EasternTimeError::NotACivilInstant {
                date,
                hour,
                minute,
                second,
                nanos,
            } => write!(
                f,
                "{date} {hour:02}:{minute:02}:{second:02}.{nanos:09} is not a real \
                 date and time"
            ),
            EasternTimeError::NonexistentLocalTime { date, hour, minute } => write!(
                f,
                "{date} {hour:02}:{minute:02} US/Eastern does not exist: the clock \
                 jumps from 02:00 to 03:00 on that date, so no instant has this local \
                 time. The source row is corrupt"
            ),
            EasternTimeError::AmbiguousLocalTime {
                date,
                hour,
                minute,
                earlier,
                later,
            } => write!(
                f,
                "{date} {hour:02}:{minute:02} US/Eastern happens twice (the clock falls \
                 back from 02:00 to 01:00 on that date): it names both {} and {} as UTC \
                 nanoseconds. No US session prints at 01:00 on a Sunday, so the source \
                 row is corrupt. It is deliberately not resolved — guessing the earlier \
                 candidate would date the information an hour before it existed, which \
                 is lookahead (D-0052)",
                earlier.0, later.0
            ),
            EasternTimeError::OutOfRange { date } => write!(
                f,
                "{date} US/Eastern is outside the range representable as i64 \
                 nanoseconds since the Unix epoch"
            ),
        }
    }
}

impl std::error::Error for EasternTimeError {}

/// Converts a US/Eastern wall-clock reading to UTC nanoseconds.
///
/// `nanos` is the nanosecond-of-second (ThetaData supplies milliseconds, so
/// callers pass `millis * 1_000_000`).
///
/// # Errors
/// [`EasternTimeError::NotACivilInstant`] if the components do not form a real
/// date and time, [`EasternTimeError::NonexistentLocalTime`] if the reading
/// falls in the spring-forward gap, [`EasternTimeError::AmbiguousLocalTime`] if
/// it falls in the fall-back hour, and [`EasternTimeError::OutOfRange`] if the
/// result does not fit an `i64`.
pub fn eastern_wall_clock_to_ts(
    date: CivilDate,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
) -> Result<Ts, EasternTimeError> {
    let not_civil = || EasternTimeError::NotACivilInstant {
        date,
        hour,
        minute,
        second,
        nanos,
    };

    let year = i32::try_from(date.year).map_err(|_| not_civil())?;
    let naive = NaiveDate::from_ymd_opt(year, date.month, date.day)
        .ok_or_else(not_civil)?
        .and_hms_nano_opt(hour, minute, second, nanos)
        .ok_or_else(not_civil)?;

    // Both pathological arms are refused (D-0052). Matching the mapping
    // explicitly, rather than collapsing it with `earliest()`/`single()`,
    // is what keeps the ambiguous case from being silently resolved.
    let resolved = match New_York.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt,
        LocalResult::None => {
            return Err(EasternTimeError::NonexistentLocalTime { date, hour, minute });
        }
        LocalResult::Ambiguous(earlier, later) => {
            return Err(EasternTimeError::AmbiguousLocalTime {
                date,
                hour,
                minute,
                earlier: to_ts(&earlier).ok_or(EasternTimeError::OutOfRange { date })?,
                later: to_ts(&later).ok_or(EasternTimeError::OutOfRange { date })?,
            });
        }
    };

    to_ts(&resolved).ok_or(EasternTimeError::OutOfRange { date })
}

/// UTC nanoseconds of a resolved instant, or `None` on `i64` overflow.
fn to_ts(dt: &chrono::DateTime<chrono_tz::Tz>) -> Option<Ts> {
    let seconds = dt.timestamp();
    let subsec = i64::from(dt.timestamp_subsec_nanos());
    seconds
        .checked_mul(NANOS_PER_SECOND)
        .and_then(|n| n.checked_add(subsec))
        .map(Ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i64, month: u32, day: u32) -> CivilDate {
        CivilDate { year, month, day }
    }

    // Hand-derived epoch arithmetic; the derivation is the test.
    //
    // Days from 1970-01-01 to 2024-01-01: 2020-01-01 is day 18262, then
    // +366 (2020 is a leap year) = 18628 for 2021-01-01, +365 = 18993 for
    // 2022-01-01, +365 = 19358 for 2023-01-01, +365 = 19723 for 2024-01-01.
    // 2024-01-02 is therefore day 19724 → 19724 * 86400 = 1_704_153_600 s at
    // UTC midnight. January is EST (UTC−5), so 09:30 Eastern is 14:30 UTC:
    // +52_200 s = 1_704_205_800 s.
    #[test]
    fn winter_open_is_utc_minus_five() {
        let ts = eastern_wall_clock_to_ts(date(2024, 1, 2), 9, 30, 0, 0).expect("real instant");
        assert_eq!(ts, Ts(1_704_205_800 * NANOS_PER_SECOND));
    }

    // The DST-transition test CLAUDE.md's session rules require, and the reason
    // the vendor's encoding had to be measured rather than assumed.
    //
    // 2024-03-08 is day 19723 + 31 + 29 + 7 = 19790 → 1_709_856_000 s at UTC
    // midnight. Still EST, so 09:30 local = 14:30 UTC = +52_200 s.
    //
    // 2024-03-11 is day 19723 + 31 + 29 + 10 = 19793 → 1_710_115_200 s at UTC
    // midnight. Now EDT, so the *same* local 09:30 is 13:30 UTC = +48_600 s.
    //
    // Both sessions are stamped `09:30:00.000` by the vendor. If this crate
    // ever treats those strings as UTC, this test fails by exactly one hour on
    // one of the two dates — which is the entire point of it.
    #[test]
    fn the_same_local_open_maps_to_different_utc_across_spring_forward() {
        let friday_est =
            eastern_wall_clock_to_ts(date(2024, 3, 8), 9, 30, 0, 0).expect("real instant");
        let monday_edt =
            eastern_wall_clock_to_ts(date(2024, 3, 11), 9, 30, 0, 0).expect("real instant");

        assert_eq!(friday_est, Ts(1_709_908_200 * NANOS_PER_SECOND));
        assert_eq!(monday_edt, Ts(1_710_163_800 * NANOS_PER_SECOND));

        // Three calendar days apart, but only 2 days 23 hours of real elapsed
        // time between the two opens, because an hour went missing on Sunday.
        let elapsed_hours = (monday_edt.0 - friday_est.0) / (3_600 * NANOS_PER_SECOND);
        assert_eq!(elapsed_hours, 71, "3 days minus the skipped hour");
    }

    // The autumn transition, where the vendor's encoding is genuinely
    // two-to-one — and is refused rather than resolved (D-0052).
    //
    // Resolving to the earlier candidate would date the information an hour
    // before it may have existed, and every timestamp here becomes an
    // `avail_ts`. Dating information early is precisely lookahead (§2.1);
    // dating it late merely costs measured performance. Since the window is
    // 01:00–02:00 on a Sunday and no US session prints then, a row landing
    // here is corrupt and the correct response is to say so.
    #[test]
    fn the_ambiguous_hour_is_refused_rather_than_guessed() {
        // 2024-11-03 01:30 Eastern happens twice: at 05:30 UTC (still EDT,
        // UTC−4) and again at 06:30 UTC (EST, UTC−5).
        //
        // Jan 31 + Feb 29 + Mar 31 + Apr 30 + May 31 + Jun 30 + Jul 31 +
        // Aug 31 + Sep 30 + Oct 31 = 305 days to 2024-11-01, so 2024-11-03 is
        // day 19723 + 305 + 2 = 20030 → 20030 * 86400 = 1_730_592_000 s at UTC
        // midnight. 05:30 = +19_800 s → 1_730_611_800 s; 06:30 = +23_400 s →
        // 1_730_615_400 s.
        let err = eastern_wall_clock_to_ts(date(2024, 11, 3), 1, 30, 0, 0)
            .expect_err("01:30 names two instants on the fall-back date");
        match err {
            EasternTimeError::AmbiguousLocalTime { earlier, later, .. } => {
                assert_eq!(earlier, Ts(1_730_611_800 * NANOS_PER_SECOND));
                assert_eq!(later, Ts(1_730_615_400 * NANOS_PER_SECOND));
                assert_eq!(
                    later.0 - earlier.0,
                    3_600 * NANOS_PER_SECOND,
                    "the two candidates are exactly an hour apart"
                );
            }
            other => panic!("expected AmbiguousLocalTime, got {other}"),
        }
    }

    // 01:30 on the day *before* the transition is unambiguous, so the refusal
    // must be about the transition and not about the hour. A guard that fires
    // on every early-morning row would quietly reject real open-interest data.
    #[test]
    fn the_same_clock_time_off_the_transition_converts_normally() {
        // 2024-11-02 is day 20029 → 1_730_505_600 s at UTC midnight. Still EDT
        // (UTC−4), so 01:30 local = 05:30 UTC = +19_800 s.
        let ts = eastern_wall_clock_to_ts(date(2024, 11, 2), 1, 30, 0, 0).expect("real instant");
        assert_eq!(ts, Ts(1_730_525_400 * NANOS_PER_SECOND));
    }

    // The gap hour is corrupt data, not something to snap into range: inventing
    // a timestamp is how a lookahead bug gets built.
    #[test]
    fn the_nonexistent_hour_is_refused() {
        let err = eastern_wall_clock_to_ts(date(2024, 3, 10), 2, 30, 0, 0)
            .expect_err("02:30 does not exist on the spring-forward date");
        assert!(
            matches!(err, EasternTimeError::NonexistentLocalTime { .. }),
            "{err}"
        );
    }

    // Milliseconds survive the round trip; ThetaData's stamps carry three
    // decimal places and dropping them would silently coarsen every event.
    #[test]
    fn subsecond_precision_is_preserved() {
        let ts = eastern_wall_clock_to_ts(date(2024, 1, 2), 9, 30, 0, 123_000_000)
            .expect("real instant");
        assert_eq!(ts, Ts(1_704_205_800 * NANOS_PER_SECOND + 123_000_000));
    }

    // The archive's oldest options data is June 2012, which is EDT.
    #[test]
    fn the_2012_history_floor_converts() {
        // 2012-06-01: 2012-01-01 is day 15340; +31+29+31+30+31 = 152 days to
        // 2012-06-01 → day 15492 → 1_338_508_800 s at UTC midnight. EDT, so
        // 09:30 local = 13:30 UTC = +48_600 s.
        let ts = eastern_wall_clock_to_ts(date(2012, 6, 1), 9, 30, 0, 0).expect("real instant");
        assert_eq!(ts, Ts(1_338_557_400 * NANOS_PER_SECOND));
    }

    // A malformed row must not become some other valid instant.
    #[test]
    fn impossible_components_are_refused() {
        for (d, h, m) in [
            (date(2024, 2, 30), 9, 30),
            (date(2024, 1, 2), 24, 0),
            (date(2024, 1, 2), 9, 60),
        ] {
            let err = eastern_wall_clock_to_ts(d, h, m, 0, 0).expect_err("not a real instant");
            assert!(
                matches!(err, EasternTimeError::NotACivilInstant { .. }),
                "{err}"
            );
        }
    }
}
