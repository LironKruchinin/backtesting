//! Splitting a requested range into archive-shaped download windows.
//!
//! The archive stores one file per `(dataset, schema, symbol key, window)`
//! at `raw/{dataset}/{schema}/{key}/{stem}.dbn.zst`, so planning has to cut a
//! requested range along the same boundaries the path template names.
//!
//! ## Why the file stem has two shapes
//!
//! A window covering exactly one calendar month is named `{yyyy-mm}`; any
//! other window is named `{yyyy-mm-dd}--{yyyy-mm-dd}` (start inclusive, end
//! exclusive). Both shapes are needed, and using only the first is a bug that
//! costs money:
//!
//! A cheap one-day validation pull of 2024-01-02 would land at
//! `.../2024-01.dbn.zst`. A later full-month pull of January 2024 would
//! compute the same path, and `Catalog::append` would reject it with
//! `DuplicateFilePath` (raw is immutable, D-0017) — *after* the bytes were
//! bought and downloaded. Partial windows are not an edge case either: they
//! are the norm at dataset-range boundaries and at the "up to yesterday" end
//! of the monthly archival job.
//!
//! Coverage is unaffected by the naming, because
//! [`Catalog::coverage`](crate::catalog::Catalog::coverage) subtracts
//! *ranges*, not paths — a partial January and the rest of January merge into
//! one owned interval regardless of what the files are called.
//!
//! ## Why not `chrono`
//!
//! §6 blesses `chrono` for `calendar` only, and rightly: this module does no
//! timezone work at all. Windows are cut on **UTC** civil-day boundaries, and
//! exchange sessions never enter the calculation. The civil-date conversion
//! below is Howard Hinnant's `days_from_civil`/`civil_from_days`, which is
//! exact integer arithmetic over the proleptic Gregorian calendar and carries
//! no dependency, no locale, and no clock.

use crucible_core::types::Ts;

/// Nanoseconds in one UTC day. Leap seconds do not exist in this
/// representation — Databento timestamps are UTC nanoseconds since the epoch.
pub const NANOS_PER_DAY: i64 = 86_400 * 1_000_000_000;

/// A calendar date in the proleptic Gregorian calendar, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CivilDate {
    /// Year.
    pub year: i64,
    /// Month, 1–12.
    pub month: u32,
    /// Day of month, 1–31.
    pub day: u32,
}

impl core::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Days in `month` of `year`, proleptic Gregorian.
#[must_use]
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap { 29 } else { 28 }
        }
        _ => 0,
    }
}

/// Parses `YYYY-MM-DD` into a real calendar date, or `None`.
///
/// The validation is the point. [`days_from_civil`] is Hinnant's arithmetic
/// and will happily convert month 13 or day 32 into *some* day number, so a
/// fat-fingered `--start 2024-13-01` would silently become January 2025 and
/// buy the wrong data. Rejecting it here is the last place that is free.
#[must_use]
pub fn parse_civil_date(text: &str) -> Option<CivilDate> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some(CivilDate { year, month, day })
}

/// Days since the Unix epoch for a civil date (Hinnant).
#[must_use]
pub fn days_from_civil(date: CivilDate) -> i64 {
    let m = i64::from(date.month);
    let d = i64::from(date.day);
    let y = date.year - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date for a day count since the Unix epoch (Hinnant).
#[must_use]
pub fn civil_from_days(days: i64) -> CivilDate {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    CivilDate {
        year: y + i64::from(m <= 2),
        month: u32::try_from(m).unwrap_or(1),
        day: u32::try_from(d).unwrap_or(1),
    }
}

/// Floor division that rounds toward negative infinity, so pre-epoch
/// timestamps land on the correct day rather than toward zero.
fn div_floor(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// UTC civil date containing `ts`.
#[must_use]
pub fn date_of(ts: Ts) -> CivilDate {
    civil_from_days(div_floor(ts.0, NANOS_PER_DAY))
}

/// Instant of UTC midnight starting `date`.
#[must_use]
pub fn start_of(date: CivilDate) -> Ts {
    Ts(days_from_civil(date) * NANOS_PER_DAY)
}

/// True if `ts` is exactly UTC midnight.
#[must_use]
pub fn is_day_aligned(ts: Ts) -> bool {
    ts.0.rem_euclid(NANOS_PER_DAY) == 0
}

/// First day of the calendar month containing `date`.
#[must_use]
pub fn month_start(date: CivilDate) -> CivilDate {
    CivilDate { day: 1, ..date }
}

/// First day of the calendar month after the one containing `date`.
#[must_use]
pub fn next_month_start(date: CivilDate) -> CivilDate {
    if date.month == 12 {
        CivilDate {
            year: date.year + 1,
            month: 1,
            day: 1,
        }
    } else {
        CivilDate {
            year: date.year,
            month: date.month + 1,
            day: 1,
        }
    }
}

/// Cuts `[start_ts, end_ts)` at calendar-month boundaries.
///
/// Both bounds must be UTC-midnight aligned; planning enforces that before
/// calling, so an unaligned range here would be a caller bug. The returned
/// windows tile the input exactly: contiguous, non-overlapping, in order, and
/// summing to the original duration.
///
/// # Panics
/// Debug-asserts that the input is non-empty and day-aligned.
#[must_use]
pub fn split_into_month_windows(start_ts: Ts, end_ts: Ts) -> Vec<(Ts, Ts)> {
    debug_assert!(start_ts < end_ts, "INVARIANT: window must be non-empty");
    debug_assert!(
        is_day_aligned(start_ts) && is_day_aligned(end_ts),
        "INVARIANT: planning aligns windows to UTC midnight before splitting"
    );

    let mut out = Vec::new();
    let mut cursor = start_ts;
    while cursor < end_ts {
        let boundary = start_of(next_month_start(date_of(cursor)));
        let window_end = if boundary < end_ts { boundary } else { end_ts };
        out.push((cursor, window_end));
        cursor = window_end;
    }
    out
}

/// File stem for a window: `2024-01` when it is exactly one calendar month,
/// `2024-01-02--2024-01-03` otherwise.
///
/// See the module docs for why both shapes exist — collapsing them would let
/// a partial pull and a full-month pull of the same month collide on
/// `file_path` after the second one had already been paid for.
#[must_use]
pub fn window_file_stem(start_ts: Ts, end_ts: Ts) -> String {
    let start = date_of(start_ts);
    let end = date_of(end_ts);
    let covers_whole_month =
        start.day == 1 && end == next_month_start(start) && is_day_aligned(end_ts);
    if covers_whole_month {
        format!("{:04}-{:02}", start.year, start.month)
    } else {
        format!("{start}--{end}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-derived anchors. 1970-01-01 is day 0 by definition; 2010-06-06 is
    // the GLBX.MDP3 dataset start, cross-checked against the epoch-day value
    // 14766 (40 years and change: 1970..2010 spans 10 leap days in 1972..2008
    // giving 14610 days to 2010-01-01, plus 31+28+31+30+31+5 = 156).
    #[test]
    fn civil_date_round_trips_at_known_anchors() {
        let cases = [
            (0_i64, 1970, 1, 1),
            (14_766, 2010, 6, 6),
            (17_307, 2017, 5, 21), // MBO availability start
            (-1, 1969, 12, 31),
            (11_016, 2000, 2, 29), // leap day, century-leap year
        ];
        for (days, year, month, day) in cases {
            let date = CivilDate { year, month, day };
            assert_eq!(days_from_civil(date), days, "days_from_civil {date}");
            assert_eq!(civil_from_days(days), date, "civil_from_days {days}");
        }
    }

    // A date parser that accepts month 13 turns a typo into a purchase of
    // the wrong window, so every rejection here is a saved charge.
    #[test]
    fn date_parsing_accepts_real_dates_and_rejects_plausible_typos() {
        assert_eq!(
            parse_civil_date("2024-02-29"),
            Some(CivilDate {
                year: 2024,
                month: 2,
                day: 29
            }),
            "2024 is a leap year"
        );
        assert_eq!(
            parse_civil_date("2010-06-06"),
            Some(CivilDate {
                year: 2010,
                month: 6,
                day: 6
            })
        );
        for bad in [
            "2023-02-29", // not a leap year
            "2024-13-01", // month 13 -- Hinnant would map this to Jan 2025
            "2024-00-01",
            "2024-01-00",
            "2024-01-32",
            "2024-04-31", // April has 30 days
            "2024-1-1",   // unpadded
            "24-01-01",
            "2024/01/01",
            "2024-01-01T00:00:00Z",
            "",
            "notadate!!",
        ] {
            assert!(
                parse_civil_date(bad).is_none(),
                "{bad:?} must not parse as a date"
            );
        }
    }

    #[test]
    fn date_of_uses_floor_division_before_the_epoch() {
        // One nanosecond before the epoch is 1969-12-31, not 1970-01-01.
        assert_eq!(
            date_of(Ts(-1)),
            CivilDate {
                year: 1969,
                month: 12,
                day: 31
            }
        );
        assert!(!is_day_aligned(Ts(-1)));
        assert!(is_day_aligned(Ts(-NANOS_PER_DAY)));
        assert!(is_day_aligned(Ts(0)));
    }

    fn ts(year: i64, month: u32, day: u32) -> Ts {
        start_of(CivilDate { year, month, day })
    }

    #[test]
    fn month_split_tiles_the_range_exactly() {
        // 2023-11-15 .. 2024-02-03 cuts into four windows.
        let start = ts(2023, 11, 15);
        let end = ts(2024, 2, 3);
        let windows = split_into_month_windows(start, end);
        assert_eq!(
            windows,
            vec![
                (ts(2023, 11, 15), ts(2023, 12, 1)),
                (ts(2023, 12, 1), ts(2024, 1, 1)),
                (ts(2024, 1, 1), ts(2024, 2, 1)),
                (ts(2024, 2, 1), ts(2024, 2, 3)),
            ]
        );
        // Tiling invariants: contiguous, ordered, and duration-preserving.
        assert_eq!(windows[0].0, start);
        assert_eq!(windows[windows.len() - 1].1, end);
        for pair in windows.windows(2) {
            assert_eq!(pair[0].1, pair[1].0);
        }
        let total: i64 = windows.iter().map(|(s, e)| e.0 - s.0).sum();
        assert_eq!(total, end.0 - start.0);
    }

    #[test]
    fn month_split_leaves_a_single_whole_month_intact() {
        let windows = split_into_month_windows(ts(2024, 1, 1), ts(2024, 2, 1));
        assert_eq!(windows, vec![(ts(2024, 1, 1), ts(2024, 2, 1))]);
    }

    #[test]
    fn month_split_handles_a_year_boundary_and_a_leap_february() {
        let windows = split_into_month_windows(ts(2024, 2, 1), ts(2024, 3, 1));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].1.0 - windows[0].0.0, 29 * NANOS_PER_DAY);

        let windows = split_into_month_windows(ts(2023, 12, 31), ts(2024, 1, 2));
        assert_eq!(
            windows,
            vec![
                (ts(2023, 12, 31), ts(2024, 1, 1)),
                (ts(2024, 1, 1), ts(2024, 1, 2)),
            ]
        );
    }

    // The regression guard for the collision described in the module docs.
    #[test]
    fn whole_months_and_partial_windows_get_distinguishable_names() {
        assert_eq!(window_file_stem(ts(2024, 1, 1), ts(2024, 2, 1)), "2024-01");
        assert_eq!(
            window_file_stem(ts(2024, 1, 2), ts(2024, 1, 3)),
            "2024-01-02--2024-01-03"
        );
        // A validation slice and a full-month pull of the same month must not
        // produce the same path, or the second one fails after being paid for.
        assert_ne!(
            window_file_stem(ts(2024, 1, 2), ts(2024, 1, 3)),
            window_file_stem(ts(2024, 1, 1), ts(2024, 2, 1))
        );
        // A window that starts on the 1st but stops short is still partial.
        assert_eq!(
            window_file_stem(ts(2024, 1, 1), ts(2024, 1, 15)),
            "2024-01-01--2024-01-15"
        );
        // February in a leap year is a whole month at 29 days.
        assert_eq!(window_file_stem(ts(2024, 2, 1), ts(2024, 3, 1)), "2024-02");
    }
}
