//! The on-disk shape of a calendar table, and the rules it can express.
//!
//! ## Why rules rather than a list of dates
//!
//! A dated list of holidays is correct until the year it runs out, and then it
//! is silently wrong: `is_open` starts answering "yes" for every future
//! Christmas, and nothing fails. Rules ("the third Monday of January", "two
//! days before Easter") cover every year the archive will ever hold, and the
//! handful of genuine one-offs — days of mourning, weather closures — are the
//! only thing that has to be enumerated by date.
//!
//! ## Why TOML compiled in rather than a file read at runtime
//!
//! The module spec calls for "static tables checked into the repo as data
//! files with sources cited". Data files they are — but they are pulled in with
//! [`include_str!`], not read from disk, because a calendar is consulted from
//! inside replay and annualization. A runtime file read there would mean the
//! same config could produce different numbers on two machines depending on
//! what was on disk, which CLAUDE.md §2.2 forbids. Compiled in, the table is
//! part of the build, and `cargo test` proves it parses.
//!
//! Every rule carries a `source` field. An uncited holiday is a rumour about
//! the market, and this project's whole thesis is that rumours do not get
//! stored (§2.5).

use serde::Deserialize;

use super::error::CalendarError;
use crate::ingest::window::{CivilDate, civil_from_days, days_from_civil};

/// Schema version this build reads.
pub(super) const TABLE_SCHEMA_VERSION: u32 = 1;

/// A whole calendar table file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CalendarTable {
    /// Version of this file's own schema. Bumping it is a hard refusal.
    pub schema_version: u32,
    /// One entry per exchange session template.
    pub calendar: Vec<CalendarSpec>,
}

/// One exchange's session template, holidays, and provenance.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CalendarSpec {
    /// Stable identifier, e.g. `cme_globex_equity_index`.
    pub id: String,
    /// One line on what this calendar governs.
    pub description: String,
    /// IANA timezone the exchange states its hours in.
    pub timezone: String,
    /// Root symbols governed by this calendar, e.g. `["ES", "NQ", "RTY"]`.
    pub roots: Vec<String>,
    /// First date this calendar describes the exchange at all, `YYYY-MM-DD`.
    ///
    /// With eras this is the start of the **oldest** one, and the loader
    /// checks that: a `valid_from` earlier than every era would leave dates the
    /// calendar claims to describe with no template to describe them with.
    /// Answering about an earlier date still returns something — the functions
    /// are total — but it is a later era's answer to an earlier era's question,
    /// and `crucible qa` and `crucible backtest` say so.
    pub valid_from: String,
    /// The **current** weekly session template — the newest era.
    ///
    /// Kept as its own field rather than as the last element of [`Self::era`]
    /// because every table that predates eras means exactly this, and because
    /// the common case (an exchange that has not restructured) should not have
    /// to say the word "era" at all.
    pub session: SessionSpec,
    /// Session templates for eras that have **ended**, each carrying the first
    /// trading day it describes.
    ///
    /// A single-template calendar is a claim that the exchange has always run
    /// these hours, and for CME equity index that claim was false by 45
    /// minutes a day for three of the sixteen years this archive holds
    /// (D-0077). Order does not matter; the loader sorts and checks.
    #[serde(default)]
    pub era: Vec<SessionSpec>,
    /// Span over which `bars_per_year` averages. Constant, so the number is
    /// the same on every machine forever.
    pub reference_span: ReferenceSpan,
    /// Recurring holiday and early-close rules.
    #[serde(default)]
    pub holiday: Vec<HolidaySpec>,
    /// Dated exceptions that no rule generates.
    #[serde(default)]
    pub one_off: Vec<OneOffSpec>,
    /// Roots whose **trading-day set** this calendar governs but whose
    /// intraday hours it does not describe.
    ///
    /// The distinction is the whole point. A US index-option root keeps the
    /// same holidays as cash equities and a different session (Cboe global
    /// hours, 16:15 ET closes), so its *dates* are answerable here and its
    /// *instants* are not. Listing it in `roots` would hand out a confident
    /// wrong `is_open`; leaving it out entirely would make a coverage check
    /// impossible. It goes here, and only [`TradingDayCalendar`] can reach it.
    ///
    /// [`TradingDayCalendar`]: super::TradingDayCalendar
    #[serde(default)]
    pub day_level_roots: Vec<String>,
    /// Where the day-level claim came from. Required when
    /// `day_level_roots` is non-empty — an unsourced claim about someone
    /// else's exchange is exactly what this table format exists to prevent.
    #[serde(default)]
    pub day_level_source: Option<String>,
    /// Where the facts in this entry came from.
    pub sources: Vec<String>,
}

/// Whether a trading day's session starts the calendar day before it.
///
/// CME Globex opens the evening before, so the session that starts Sunday
/// evening is Monday's trade date. A US equity or equity-option session does
/// not: 09:30 to 16:00 on the trading day itself, with no overnight.
///
/// This was originally not a choice at all — the loader hard-refused any table
/// whose close was not before its open, which encoded "every market is CME"
/// into a module whose docs claimed to be about exchanges in general (D-0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SessionShape {
    /// Opens on `D - 1`, closes on `D`. The CME convention, and the default so
    /// that a table written before this existed keeps its exact meaning.
    #[default]
    Overnight,
    /// Opens and closes on `D`. US cash equities and their options.
    SameDay,
}

/// The repeating weekly shape of a trading day.
///
/// For an `overnight` session a trading day `D` runs from `open_local` on
/// `D - 1` to `close_local` on `D`, minus any halts: the session that opens on
/// Sunday evening is Monday's trade date, and the gap between `close_local` and
/// the next `open_local` is the daily maintenance break. For a `same_day`
/// session both ends fall on `D`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionSpec {
    /// First trading day this template describes, `YYYY-MM-DD`.
    ///
    /// Required on every `[[calendar.era]]` entry, and required on
    /// `[calendar.session]` too as soon as one era exists — an era list says
    /// where each *earlier* template started, and something still has to say
    /// where the current one did. Omitted on a single-template calendar, where
    /// it can only be `valid_from`.
    #[serde(default)]
    pub from: Option<String>,
    /// Whether the session starts the calendar day before the trading day.
    #[serde(default)]
    pub shape: SessionShape,
    /// Local time the session opens — on the day *before* the trading day for
    /// an `overnight` session, on the trading day itself for `same_day`.
    pub open_local: String,
    /// Local time the session closes, on the trading day itself.
    pub close_local: String,
    /// Intraday halts, as `[from, to]` local times on the trading day.
    #[serde(default)]
    pub halt_local: Vec<[String; 2]>,
    /// Start of the regular-trading-hours window, local, on the trading day.
    pub rth_open_local: String,
    /// End of the regular-trading-hours window, local, on the trading day.
    pub rth_close_local: String,
    /// Where these hours came from.
    pub source: String,
}

/// The fixed span `bars_per_year` averages over.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReferenceSpan {
    /// Inclusive first trading day considered, `YYYY-MM-DD`.
    pub start: String,
    /// Exclusive last day considered, `YYYY-MM-DD`.
    pub end: String,
    /// Why this span and not another.
    pub rationale: String,
}

/// One recurring holiday or early close.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HolidaySpec {
    /// Human name, for messages and for reading the table.
    pub name: String,
    /// How to find its date in a given year.
    pub rule: HolidayRule,
    /// What it does to the session.
    pub effect: Effect,
    /// First year the exchange observed it. Juneteenth is not retroactive.
    #[serde(default)]
    pub first_year: Option<i64>,
    /// Last year the exchange observed it, if it has been discontinued.
    #[serde(default)]
    pub last_year: Option<i64>,
    /// Where this rule came from.
    pub source: String,
}

/// A dated exception no rule generates.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OneOffSpec {
    /// The affected trading day, `YYYY-MM-DD`.
    pub date: String,
    /// Human name, for messages.
    pub name: String,
    /// What it did to the session.
    pub effect: Effect,
    /// Where this came from.
    pub source: String,
}

/// How to locate a recurring holiday in an arbitrary year.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub(super) enum HolidayRule {
    /// A fixed month and day, e.g. Christmas.
    FixedDate {
        /// Month, 1–12.
        month: u32,
        /// Day of month, 1–31.
        day: u32,
        /// How a weekend landing is observed.
        observance: Observance,
    },
    /// The nth weekday of a month, e.g. the third Monday of January.
    NthWeekday {
        /// Month, 1–12.
        month: u32,
        /// Which weekday.
        weekday: Weekday,
        /// 1–5, or -1 for the last one in the month.
        nth: i8,
    },
    /// Two days before Easter Sunday, by the Gregorian computus.
    GoodFriday,
    /// The last weekday strictly before a fixed date — "the day before
    /// Independence Day", which is July 3 unless July 3 is a weekend.
    WeekdayBefore {
        /// Month of the anchor date, 1–12.
        month: u32,
        /// Day of the anchor date, 1–31.
        day: u32,
        /// How the anchor's weekend landing is observed before stepping back.
        observance: Observance,
        /// Weekdays the **unobserved** anchor may fall on for this rule to
        /// fire at all. Empty means every weekday, which is the old behaviour.
        ///
        /// This exists because the early close before Independence Day happens
        /// only when 4 July falls Tuesday–Friday, and a rule that could not say
        /// so produced a confident early close on six dates the exchange traded
        /// in full (D-0059 recorded the defect and could not fix it; D-0077
        /// does). The condition is on the anchor rather than on the result
        /// because that is how the exchange states it: it is about which
        /// weekday the holiday lands on, not which weekday precedes it.
        #[serde(default)]
        anchor_weekday: Vec<Weekday>,
    },
    /// The day after an nth-weekday holiday — "the Friday after Thanksgiving".
    DayAfterNthWeekday {
        /// Month, 1–12.
        month: u32,
        /// Which weekday the anchor holiday falls on.
        weekday: Weekday,
        /// 1–5, or -1 for the last one in the month.
        nth: i8,
    },
}

/// What a holiday does to the session.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub(super) enum Effect {
    /// No session at all on this trading day. The evening before does not
    /// open, because the evening before opens *for* this day's session.
    Closed,
    /// The session runs as usual but ends at `close_local` instead of the
    /// template's close.
    EarlyClose {
        /// Local time the shortened session ends.
        close_local: String,
    },
}

/// How a fixed-date holiday is observed when it lands on a weekend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Observance {
    /// The date itself, whatever weekday it is. Correct when the exchange does
    /// not shift the holiday at all.
    Actual,
    /// Saturday moves to the preceding Friday, Sunday to the following Monday.
    NearestWeekday,
    /// Sunday moves to the following Monday; Saturday is left alone.
    SundayToMonday,
}

/// Day of the week. Defined here rather than taken from `chrono` so that
/// weekday arithmetic stays integer arithmetic on [`CivilDate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Weekday {
    /// Sunday.
    Sunday,
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
}

impl Weekday {
    /// 0 = Sunday … 6 = Saturday.
    pub(super) const fn index(self) -> i64 {
        match self {
            Weekday::Sunday => 0,
            Weekday::Monday => 1,
            Weekday::Tuesday => 2,
            Weekday::Wednesday => 3,
            Weekday::Thursday => 4,
            Weekday::Friday => 5,
            Weekday::Saturday => 6,
        }
    }
}

/// Weekday of a civil date.
///
/// 1970-01-01 was a Thursday, so `days_from_civil` (which counts days from
/// that epoch) is 4 modulo 7 away from Sunday.
pub(super) fn weekday_of(date: CivilDate) -> Weekday {
    match (days_from_civil(date) + 4).rem_euclid(7) {
        0 => Weekday::Sunday,
        1 => Weekday::Monday,
        2 => Weekday::Tuesday,
        3 => Weekday::Wednesday,
        4 => Weekday::Thursday,
        5 => Weekday::Friday,
        _ => Weekday::Saturday,
    }
}

/// True when the date is Monday through Friday.
pub(super) fn is_weekday(date: CivilDate) -> bool {
    !matches!(weekday_of(date), Weekday::Saturday | Weekday::Sunday)
}

/// Shift a date by whole days.
pub(super) fn add_days(date: CivilDate, days: i64) -> CivilDate {
    civil_from_days(days_from_civil(date) + days)
}

/// The nth `weekday` of `month` in `year`; `nth = -1` means the last one.
///
/// Returns `None` when the month has no such occurrence (a fifth Monday in a
/// month with only four).
pub(super) fn nth_weekday(year: i64, month: u32, weekday: Weekday, nth: i8) -> Option<CivilDate> {
    if nth == -1 {
        // Walk back from the last day of the month.
        let next_month = if month == 12 {
            CivilDate {
                year: year + 1,
                month: 1,
                day: 1,
            }
        } else {
            CivilDate {
                year,
                month: month + 1,
                day: 1,
            }
        };
        let last = add_days(next_month, -1);
        let back = (weekday_of(last).index() - weekday.index()).rem_euclid(7);
        return Some(add_days(last, -back));
    }
    if nth < 1 {
        return None;
    }
    let first = CivilDate {
        year,
        month,
        day: 1,
    };
    let forward = (weekday.index() - weekday_of(first).index()).rem_euclid(7);
    let day_of_month = forward + 1 + 7 * (i64::from(nth) - 1);
    let candidate = add_days(first, day_of_month - 1);
    // `add_days` happily rolls into the next month; an nth that does not exist
    // must be `None`, not a date in the wrong month.
    (candidate.month == month).then_some(candidate)
}

/// Easter Sunday in the Gregorian calendar (anonymous Gregorian computus,
/// a.k.a. Meeus/Jones/Butcher). Pure integer arithmetic, valid for any
/// Gregorian year.
pub(super) fn easter_sunday(year: i64) -> CivilDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;
    CivilDate {
        year,
        month: u32::try_from(month).unwrap_or(4),
        day: u32::try_from(day).unwrap_or(1),
    }
}

/// Apply a weekend-observance rule to a date.
pub(super) fn observed(date: CivilDate, observance: Observance) -> CivilDate {
    match (observance, weekday_of(date)) {
        (Observance::Actual, _) => date,
        (Observance::NearestWeekday, Weekday::Saturday) => add_days(date, -1),
        (Observance::NearestWeekday | Observance::SundayToMonday, Weekday::Sunday) => {
            add_days(date, 1)
        }
        _ => date,
    }
}

impl HolidayRule {
    /// The date this rule picks out in `year`, or `None` if it picks none.
    pub(super) fn date_in_year(&self, year: i64) -> Option<CivilDate> {
        match self {
            HolidayRule::FixedDate {
                month,
                day,
                observance,
            } => Some(observed(
                CivilDate {
                    year,
                    month: *month,
                    day: *day,
                },
                *observance,
            )),
            HolidayRule::NthWeekday {
                month,
                weekday,
                nth,
            } => nth_weekday(year, *month, *weekday, *nth),
            HolidayRule::GoodFriday => Some(add_days(easter_sunday(year), -2)),
            HolidayRule::WeekdayBefore {
                month,
                day,
                observance,
                anchor_weekday,
            } => {
                let unobserved = CivilDate {
                    year,
                    month: *month,
                    day: *day,
                };
                if !anchor_weekday.is_empty() && !anchor_weekday.contains(&weekday_of(unobserved)) {
                    return None;
                }
                let anchor = observed(unobserved, *observance);
                let mut candidate = add_days(anchor, -1);
                // Step back to the last weekday. Bounded by construction: at
                // most two steps reach a Friday from any Sunday.
                while !is_weekday(candidate) {
                    candidate = add_days(candidate, -1);
                }
                Some(candidate)
            }
            HolidayRule::DayAfterNthWeekday {
                month,
                weekday,
                nth,
            } => nth_weekday(year, *month, *weekday, *nth).map(|d| add_days(d, 1)),
        }
    }
}

/// A local clock time, validated at load so conversion at query time cannot
/// fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LocalTime {
    /// Hour, 0–23.
    pub hour: u32,
    /// Minute, 0–59.
    pub minute: u32,
    /// Second, 0–59.
    pub second: u32,
}

impl LocalTime {
    /// Seconds since local midnight.
    pub(super) const fn seconds_of_day(self) -> i64 {
        (self.hour as i64) * 3600 + (self.minute as i64) * 60 + (self.second as i64)
    }

    /// Parses `HH:MM` or `HH:MM:SS`, rejecting anything inside the
    /// daylight-saving transition window.
    ///
    /// The window rejection is what makes the runtime local-to-UTC conversion
    /// total: a local time between 01:00 and 03:00 can name two instants (in
    /// autumn) or none (in spring), and no exchange schedules a session
    /// boundary there. See [`CalendarError::AmbiguousLocalTime`].
    pub(super) fn parse(
        text: &str,
        calendar: &str,
        field: &'static str,
    ) -> Result<LocalTime, CalendarError> {
        let malformed = || CalendarError::MalformedLocalTime {
            calendar: calendar.to_owned(),
            field,
            value: text.to_owned(),
        };
        let mut parts = text.split(':');
        let hour: u32 = parts
            .next()
            .ok_or_else(malformed)?
            .parse()
            .map_err(|_| malformed())?;
        let minute: u32 = parts
            .next()
            .ok_or_else(malformed)?
            .parse()
            .map_err(|_| malformed())?;
        let second: u32 = match parts.next() {
            Some(s) => s.parse().map_err(|_| malformed())?,
            None => 0,
        };
        if parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
            return Err(malformed());
        }
        let time = LocalTime {
            hour,
            minute,
            second,
        };
        if (3600..10_800).contains(&time.seconds_of_day()) {
            return Err(CalendarError::AmbiguousLocalTime {
                calendar: calendar.to_owned(),
                field,
                value: text.to_owned(),
            });
        }
        Ok(time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(year: i64, month: u32, day: u32) -> CivilDate {
        CivilDate { year, month, day }
    }

    // Hand-checked against a wall calendar: 2026-07-28 is a Tuesday, the Unix
    // epoch 1970-01-01 is a Thursday, and 2000-01-01 is a Saturday.
    #[test]
    fn weekdays_match_the_calendar() {
        assert_eq!(weekday_of(d(2026, 7, 28)), Weekday::Tuesday);
        assert_eq!(weekday_of(d(1970, 1, 1)), Weekday::Thursday);
        assert_eq!(weekday_of(d(2000, 1, 1)), Weekday::Saturday);
        assert_eq!(weekday_of(d(2024, 2, 29)), Weekday::Thursday);
    }

    // MLK Day is the third Monday of January: 2024-01-15, 2025-01-20.
    // Memorial Day is the last Monday of May: 2024-05-27, 2025-05-26.
    #[test]
    fn nth_weekday_finds_the_usual_suspects() {
        assert_eq!(
            nth_weekday(2024, 1, Weekday::Monday, 3),
            Some(d(2024, 1, 15))
        );
        assert_eq!(
            nth_weekday(2025, 1, Weekday::Monday, 3),
            Some(d(2025, 1, 20))
        );
        assert_eq!(
            nth_weekday(2024, 5, Weekday::Monday, -1),
            Some(d(2024, 5, 27))
        );
        assert_eq!(
            nth_weekday(2025, 5, Weekday::Monday, -1),
            Some(d(2025, 5, 26))
        );
        // Thanksgiving: fourth Thursday of November.
        assert_eq!(
            nth_weekday(2024, 11, Weekday::Thursday, 4),
            Some(d(2024, 11, 28))
        );
        assert_eq!(
            nth_weekday(2025, 11, Weekday::Thursday, 4),
            Some(d(2025, 11, 27))
        );
    }

    // February 2026 starts on a Sunday, so it has exactly four Mondays
    // (2, 9, 16, 23) and no fifth.
    #[test]
    fn a_missing_nth_weekday_is_none_not_the_next_month() {
        assert_eq!(
            nth_weekday(2026, 2, Weekday::Monday, 4),
            Some(d(2026, 2, 23))
        );
        assert_eq!(nth_weekday(2026, 2, Weekday::Monday, 5), None);
    }

    // Published Easter dates: 2024-03-31, 2025-04-20, 2026-04-05, 2010-04-04.
    // Good Friday is two days earlier.
    #[test]
    fn easter_matches_published_dates() {
        assert_eq!(easter_sunday(2024), d(2024, 3, 31));
        assert_eq!(easter_sunday(2025), d(2025, 4, 20));
        assert_eq!(easter_sunday(2026), d(2026, 4, 5));
        assert_eq!(easter_sunday(2010), d(2010, 4, 4));
        assert_eq!(
            HolidayRule::GoodFriday.date_in_year(2024),
            Some(d(2024, 3, 29))
        );
        assert_eq!(
            HolidayRule::GoodFriday.date_in_year(2025),
            Some(d(2025, 4, 18))
        );
    }

    // 2022-01-01 was a Saturday, so nearest-weekday observance is Friday
    // 2021-12-31. 2023-01-01 was a Sunday, observed Monday 2023-01-02.
    #[test]
    fn weekend_observance_moves_the_right_way() {
        assert_eq!(
            observed(d(2022, 1, 1), Observance::NearestWeekday),
            d(2021, 12, 31)
        );
        assert_eq!(
            observed(d(2023, 1, 1), Observance::NearestWeekday),
            d(2023, 1, 2)
        );
        assert_eq!(observed(d(2022, 1, 1), Observance::Actual), d(2022, 1, 1));
        assert_eq!(
            observed(d(2022, 1, 1), Observance::SundayToMonday),
            d(2022, 1, 1)
        );
    }

    // Independence Day 2026 falls on a Saturday. The weekday before it is
    // Friday 2026-07-03. In 2027 it is a Sunday, so the weekday before the
    // *observed* Monday holiday is Friday 2027-07-02.
    #[test]
    fn weekday_before_steps_over_the_weekend() {
        assert_eq!(
            HolidayRule::WeekdayBefore {
                month: 7,
                day: 4,
                observance: Observance::Actual,
                anchor_weekday: Vec::new(),
            }
            .date_in_year(2026),
            Some(d(2026, 7, 3))
        );
        assert_eq!(
            HolidayRule::WeekdayBefore {
                month: 7,
                day: 4,
                observance: Observance::NearestWeekday,
                anchor_weekday: Vec::new(),
            }
            .date_in_year(2027),
            Some(d(2027, 7, 2))
        );
    }

    // The condition CME actually states: the early close before Independence
    // Day happens only when 4 July falls Tuesday-Friday. 2024-07-04 is a
    // Thursday, so 2024-07-03 qualifies; 2026-07-04 is a Saturday and
    // 2021-07-04 a Sunday, so neither produces a date at all — where the
    // unconditional rule produced 2026-07-03 and 2021-07-02, both of which the
    // archive shows trading in full (D-0059's recorded defect, D-0077's fix).
    #[test]
    fn an_anchor_weekday_condition_suppresses_the_rule_entirely() {
        let rule = HolidayRule::WeekdayBefore {
            month: 7,
            day: 4,
            observance: Observance::Actual,
            anchor_weekday: vec![
                Weekday::Tuesday,
                Weekday::Wednesday,
                Weekday::Thursday,
                Weekday::Friday,
            ],
        };
        assert_eq!(rule.date_in_year(2024), Some(d(2024, 7, 3)));
        assert_eq!(rule.date_in_year(2019), Some(d(2019, 7, 3)));
        // 2026-07-04 Saturday, 2021-07-04 Sunday, 2022-07-04 Monday.
        assert_eq!(rule.date_in_year(2026), None);
        assert_eq!(rule.date_in_year(2021), None);
        assert_eq!(rule.date_in_year(2022), None);
    }

    // The Friday after Thanksgiving 2024 is 2024-11-29.
    #[test]
    fn day_after_nth_weekday_lands_on_the_friday() {
        assert_eq!(
            HolidayRule::DayAfterNthWeekday {
                month: 11,
                weekday: Weekday::Thursday,
                nth: 4,
            }
            .date_in_year(2024),
            Some(d(2024, 11, 29))
        );
    }

    #[test]
    fn local_times_parse_and_reject() {
        assert_eq!(
            LocalTime::parse("17:00", "cal", "open_local").expect("17:00 is a local time"),
            LocalTime {
                hour: 17,
                minute: 0,
                second: 0
            }
        );
        assert_eq!(
            LocalTime::parse("15:15:30", "cal", "open_local").expect("15:15:30 is a local time"),
            LocalTime {
                hour: 15,
                minute: 15,
                second: 30
            }
        );
        for bad in ["", "17", "17:60", "24:00", "17:00:00:00", "abc", "-1:00"] {
            assert!(
                LocalTime::parse(bad, "cal", "open_local").is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    // 01:00 through 02:59 local is the daylight-saving transition window: in
    // spring it names no instant, in autumn it names two.
    #[test]
    fn local_times_inside_the_dst_window_are_refused() {
        for bad in ["01:00", "02:00", "02:59:59"] {
            assert!(matches!(
                LocalTime::parse(bad, "cal", "open_local"),
                Err(CalendarError::AmbiguousLocalTime { .. })
            ));
        }
        // The edges are fine: 00:59:59 and 03:00 are unambiguous.
        assert!(LocalTime::parse("00:59:59", "cal", "open_local").is_ok());
        assert!(LocalTime::parse("03:00", "cal", "open_local").is_ok());
    }
}
