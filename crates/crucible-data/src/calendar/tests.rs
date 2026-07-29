//! Calendar tests.
//!
//! Almost everything here drives the engine from a **hand-written fixture
//! table** rather than from the bundled CME one. That is deliberate: the
//! bundled table records facts about a real exchange and will be corrected as
//! sources are checked, and a test suite that moves every time a holiday rule
//! is fixed tests nothing. The bundled table gets its own small set of
//! structural tests at the bottom.
//!
//! Expected instants are hand-derived from the definitions of CST (UTC−6) and
//! CDT (UTC−5), with the arithmetic written out in each comment.

use super::*;

/// A deliberately plain calendar: 23-hour session, no halts, no holidays.
/// Everything about `bars_per_year` is checkable by hand against it.
const PLAIN: &str = r#"
schema_version = 1

[[calendar]]
id = "test_plain"
description = "23-hour session, no halts, no holidays"
timezone = "America/Chicago"
roots = ["ZZ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00"
close_local = "16:00"
rth_open_local = "08:30"
rth_close_local = "15:15"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2024-01-08"
rationale = "one week, so the arithmetic fits in a comment"
"#;

/// The realistic shape: an afternoon halt, one closed holiday, one early
/// close, and one dated exception.
const SHAPED: &str = r#"
schema_version = 1

[[calendar]]
id = "test_shaped"
description = "halt, a closed holiday, an early close, and a one-off"
timezone = "America/Chicago"
roots = ["ES", "NQ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00"
close_local = "16:00"
halt_local = [["15:15", "15:30"]]
rth_open_local = "08:30"
rth_close_local = "15:15"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2025-01-01"
rationale = "one calendar year"

[[calendar.holiday]]
name = "Christmas Day"
rule = { kind = "fixed_date", month = 12, day = 25, observance = "nearest_weekday" }
effect = { kind = "closed" }
source = "hand-written test fixture"

[[calendar.holiday]]
name = "Independence Day"
rule = { kind = "fixed_date", month = 7, day = 4, observance = "nearest_weekday" }
effect = { kind = "early_close", close_local = "12:00" }
source = "hand-written test fixture"

[[calendar.holiday]]
name = "Juneteenth"
rule = { kind = "fixed_date", month = 6, day = 19, observance = "nearest_weekday" }
effect = { kind = "closed" }
first_year = 2022
source = "hand-written test fixture"

[[calendar.one_off]]
date = "2024-03-13"
name = "Test day of mourning"
effect = { kind = "closed" }
source = "hand-written test fixture"
"#;

fn plain() -> Calendar {
    let mut all = Calendar::parse_table("fixture", PLAIN).expect("fixture parses");
    all.pop().expect("fixture has one calendar")
}

fn shaped() -> Calendar {
    let mut all = Calendar::parse_table("fixture", SHAPED).expect("fixture parses");
    all.pop().expect("fixture has one calendar")
}

fn d(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
}

/// UTC instant from epoch seconds, for hand-derived expectations.
fn utc(seconds: i64) -> Ts {
    Ts(seconds * 1_000_000_000)
}

// 2024-01-01 is 19_723 days after the Unix epoch, i.e. 1_704_067_200 s.
// Trading day Tuesday 2024-01-02 therefore opens Monday 2024-01-01 at
// 17:00 CST = 23:00 UTC = 1_704_067_200 + 82_800 = 1_704_150_000, and closes
// Tuesday at 16:00 CST = 22:00 UTC = 1_704_067_200 + 86_400 + 79_200
// = 1_704_232_800. That is 82_800 s = 23 h.
#[test]
fn a_trading_day_runs_from_the_previous_evening_to_its_own_close() {
    let cal = plain();
    let intervals = cal.open_intervals(d(2024, 1, 2));
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].0, utc(1_704_150_000));
    assert_eq!(intervals[0].1, utc(1_704_232_800));
    assert_eq!(intervals[0].1.0 - intervals[0].0.0, 82_800 * 1_000_000_000);
}

// The CME business date rolls at the session open, so the Sunday-evening
// session belongs to Monday. 2024-01-07 is a Sunday.
#[test]
fn the_sunday_evening_session_is_mondays_trade_date() {
    let cal = plain();
    // Sunday 16:59 CST = 22:59 UTC — still Sunday's (nonexistent) trade date.
    let before = utc(1_704_067_200 + 6 * 86_400 + 22 * 3600 + 59 * 60);
    assert_eq!(cal.trading_day(before), d(2024, 1, 7));
    assert!(!cal.is_open(before));
    // Sunday 17:30 CST = 23:30 UTC — Monday's trade date, and open.
    let after = utc(1_704_067_200 + 6 * 86_400 + 23 * 3600 + 30 * 60);
    assert_eq!(cal.trading_day(after), d(2024, 1, 8));
    assert!(cal.is_open(after));
}

// 16:30 CST on a Tuesday is inside the daily maintenance break: the day's
// session closed at 16:00 and the next day's opens at 17:00.
#[test]
fn the_maintenance_break_is_closed() {
    let cal = plain();
    let ts = utc(1_704_067_200 + 86_400 + 22 * 3600 + 30 * 60);
    assert_eq!(cal.session_of(ts), SessionId::Closed);
}

// Saturday has no session at all: Friday's close is the end of the week.
#[test]
fn the_weekend_is_closed() {
    let cal = plain();
    assert!(cal.open_intervals(d(2024, 1, 6)).is_empty());
    assert!(cal.open_intervals(d(2024, 1, 7)).is_empty());
    assert!(!cal.is_trading_day(d(2024, 1, 6)));
    // Friday evening does not reopen, because Saturday is not a trading day.
    let friday_evening = utc(1_704_067_200 + 4 * 86_400 + 23 * 3600 + 30 * 60);
    assert_eq!(cal.session_of(friday_evening), SessionId::Closed);
}

// A 15-minute halt splits one 23 h session into 22 h 15 m + 30 m = 22 h 45 m.
#[test]
fn the_afternoon_halt_splits_the_session() {
    let cal = shaped();
    let intervals = cal.open_intervals(d(2024, 1, 2));
    assert_eq!(intervals.len(), 2);
    let first = intervals[0].1.0 - intervals[0].0.0;
    let second = intervals[1].1.0 - intervals[1].0.0;
    assert_eq!(first, 80_100 * 1_000_000_000); // 17:00 -> 15:15 = 22h15m
    assert_eq!(second, 1_800 * 1_000_000_000); // 15:30 -> 16:00 = 30m
    assert_eq!(first + second, 81_900 * 1_000_000_000); // 23h - 15m
    // The halt itself is closed.
    let halt = intervals[0].1.plus_ns(60 * 1_000_000_000);
    assert_eq!(cal.session_of(halt), SessionId::Closed);
}

// US daylight saving starts on the second Sunday of March (2024-03-10) at
// 02:00 local — inside the weekend gap, never inside a session. So the session
// moves an hour in UTC and keeps its length exactly.
#[test]
fn daylight_saving_moves_the_session_in_utc_but_not_its_length() {
    let cal = plain();
    // Monday 2024-03-04 opens Sunday 17:00 CST (UTC-6) = 23:00 UTC.
    let winter = cal.open_intervals(d(2024, 3, 4));
    // Monday 2024-03-11 opens Sunday 17:00 CDT (UTC-5) = 22:00 UTC.
    let summer = cal.open_intervals(d(2024, 3, 11));
    assert_eq!(winter.len(), 1);
    assert_eq!(summer.len(), 1);

    let winter_open_hour = (winter[0].0.0 / 1_000_000_000).rem_euclid(86_400) / 3600;
    let summer_open_hour = (summer[0].0.0 / 1_000_000_000).rem_euclid(86_400) / 3600;
    assert_eq!(winter_open_hour, 23, "17:00 CST is 23:00 UTC");
    assert_eq!(summer_open_hour, 22, "17:00 CDT is 22:00 UTC");

    // Length is identical, which is what lets bars_per_year count local
    // seconds (see the module docs).
    assert_eq!(winter[0].1.0 - winter[0].0.0, summer[0].1.0 - summer[0].0.0);
}

// The autumn transition (first Sunday of November, 2024-11-03) is the
// ambiguous one — 01:00-01:59 local happens twice. It also sits in the
// weekend gap.
#[test]
fn the_autumn_transition_is_also_outside_every_session() {
    let cal = plain();
    let before = cal.open_intervals(d(2024, 11, 1)); // Friday, still CDT
    let after = cal.open_intervals(d(2024, 11, 4)); // Monday, now CST
    assert_eq!(before[0].1.0 - before[0].0.0, 82_800 * 1_000_000_000);
    assert_eq!(after[0].1.0 - after[0].0.0, 82_800 * 1_000_000_000);
}

// Christmas 2024 fell on a Wednesday: no session on the 25th, and the evening
// of the 24th does not open because it exists only to open the 25th.
#[test]
fn a_closed_holiday_removes_the_day_and_the_evening_before() {
    let cal = shaped();
    assert!(!cal.is_trading_day(d(2024, 12, 25)));
    assert!(cal.open_intervals(d(2024, 12, 25)).is_empty());
    assert_eq!(
        cal.day_effect(d(2024, 12, 25)),
        DayEffect::Closed {
            name: "Christmas Day".to_owned()
        }
    );

    // 2024 is a leap year, so Christmas is day 360 — 359 days after
    // 2024-01-01 (1_704_067_200 s), i.e. 1_735_084_800 s. Christmas Eve is a
    // day earlier, 1_734_998_400 s; its evening is 17:30 CST = 23:30 UTC.
    let christmas_eve_evening = utc(1_734_998_400 + 23 * 3600 + 30 * 60);
    assert_eq!(cal.trading_day(christmas_eve_evening), d(2024, 12, 25));
    assert_eq!(cal.session_of(christmas_eve_evening), SessionId::Closed);
}

// ... but the evening *of* the closed day opens the following session, which
// is why Globex reopens at 17:00 on Christmas Day itself.
#[test]
fn the_evening_of_a_closed_day_opens_the_next_session() {
    let cal = shaped();
    assert!(cal.is_trading_day(d(2024, 12, 26)));
    let intervals = cal.open_intervals(d(2024, 12, 26));
    // 2024-12-25 is 1_735_084_800 s; 17:00 CST = 23:00 UTC.
    assert_eq!(intervals[0].0, utc(1_735_084_800 + 23 * 3600));

    let christmas_evening = utc(1_735_084_800 + 23 * 3600 + 30 * 60);
    assert!(cal.is_open(christmas_evening));
    assert_eq!(cal.trading_day(christmas_evening), d(2024, 12, 26));
}

// Independence Day 2024 was a Thursday, with an early close at 12:00 local.
// The session still opens the previous evening: 17:00 -> 12:00 = 19 h.
#[test]
fn an_early_close_shortens_the_day_but_keeps_the_evening_before() {
    let cal = shaped();
    assert!(cal.is_trading_day(d(2024, 7, 4)));
    let intervals = cal.open_intervals(d(2024, 7, 4));
    assert_eq!(
        intervals.len(),
        1,
        "the 15:15 halt is after the early close"
    );
    assert_eq!(
        intervals[0].1.0 - intervals[0].0.0,
        19 * 3600 * 1_000_000_000
    );
    assert!(matches!(
        cal.day_effect(d(2024, 7, 4)),
        DayEffect::EarlyClose { .. }
    ));
}

// Juneteenth carries first_year = 2022, so it must not delete a session from
// 2021 — the single most damaging kind of calendar error over a 16-year
// archive, because it is invisible in the output.
#[test]
fn a_holiday_does_not_apply_before_the_year_it_was_adopted() {
    let cal = shaped();
    // 2021-06-19 was a Saturday, observed on Friday the 18th — but not yet.
    assert_eq!(cal.day_effect(d(2021, 6, 18)), DayEffect::Normal);
    assert!(cal.is_trading_day(d(2021, 6, 18)));
    // 2022-06-19 was a Sunday, observed on Monday the 20th.
    assert!(!cal.is_trading_day(d(2022, 6, 20)));
}

// A one-off is a dated exception no rule generates.
#[test]
fn a_one_off_closes_a_single_dated_session() {
    let cal = shaped();
    assert!(!cal.is_trading_day(d(2024, 3, 13)));
    assert!(cal.is_trading_day(d(2024, 3, 12)));
    assert!(cal.is_trading_day(d(2024, 3, 14)));
}

// New Year's Day 2022 fell on a Saturday; nearest-weekday observance moves it
// to Friday 2021-12-31, which is a date in the *previous* year. The lookup has
// to consider neighbouring years or it silently misses this every few years.
#[test]
fn an_observance_that_lands_in_the_previous_year_is_still_found() {
    let table = SHAPED.replace(
        r#"rule = { kind = "fixed_date", month = 12, day = 25, observance = "nearest_weekday" }"#,
        r#"rule = { kind = "fixed_date", month = 1, day = 1, observance = "nearest_weekday" }"#,
    );
    let cal = Calendar::parse_table("fixture", &table)
        .expect("fixture parses")
        .pop()
        .expect("one calendar");
    assert!(!cal.is_trading_day(d(2021, 12, 31)));
}

#[test]
fn session_of_classifies_regular_and_electronic_hours() {
    let cal = shaped();
    // 2024-01-02 is 1_704_153_600 s. CST is UTC-6.
    let base = 1_704_153_600;
    let at = |h: i64, m: i64| utc(base + (h + 6) * 3600 + m * 60);
    assert_eq!(cal.session_of(at(3, 0)), SessionId::Overnight);
    assert_eq!(cal.session_of(at(8, 29)), SessionId::Overnight);
    assert_eq!(cal.session_of(at(8, 30)), SessionId::Regular);
    assert_eq!(cal.session_of(at(15, 14)), SessionId::Regular);
    assert_eq!(cal.session_of(at(15, 20)), SessionId::Closed); // in the halt
    assert_eq!(cal.session_of(at(15, 45)), SessionId::PostRegular);
    assert_eq!(cal.session_of(at(16, 30)), SessionId::Closed);
}

// Hand-derived. The PLAIN fixture spans 2024-01-01 (a Monday) to 2024-01-08,
// exclusive: 7 calendar days holding 5 trading days (Mon-Fri) of 23 h each.
//   open seconds   = 5 * 82_800                     = 414_000
//   years in span  = 7 / 365.2425                   = 0.0191655...
//   trading days/y = 5 * 365.2425 / 7               = 260.8875
//   seconds/y      = 414_000 * 365.2425 / 7         = 21_601_485
//   1m bars/y      = 21_601_485 / 60                = 360_024.75
//   1h bars/y      = 21_601_485 / 3600              = 6_000.412_5
#[test]
fn bars_per_year_matches_hand_arithmetic() {
    let cal = plain();
    assert!((cal.trading_days_per_year() - 260.8875).abs() < 1e-6);
    assert!((cal.open_seconds_per_year() - 21_601_485.0).abs() < 1e-3);
    assert!((cal.bars_per_year(TimeFrame::M1) - 360_024.75).abs() < 1e-3);
    assert!((cal.bars_per_year(TimeFrame::H1) - 6_000.412_5).abs() < 1e-4);
    assert!((cal.bars_per_year(TimeFrame::S1) - 21_601_485.0).abs() < 1e-3);
}

// A daily bar exists once per session however long the session is, so D1 must
// count trading days rather than dividing open seconds by 86_400 (which would
// give ~250 * 23/24 and understate the count by 4%).
#[test]
fn daily_bars_are_counted_as_trading_days() {
    let cal = plain();
    assert!((cal.bars_per_year(TimeFrame::D1) - cal.trading_days_per_year()).abs() < 1e-9);
    assert!(cal.bars_per_year(TimeFrame::D1) > 250.0);
}

// The shaped fixture loses 15 minutes a day to the halt and a handful of days
// to holidays, so it must sit below the plain one on both measures.
#[test]
fn halts_and_holidays_reduce_the_annualization_factor() {
    let plain_1m = plain().bars_per_year(TimeFrame::M1);
    let shaped_1m = shaped().bars_per_year(TimeFrame::M1);
    assert!(
        shaped_1m < plain_1m,
        "shaped {shaped_1m} should be below plain {plain_1m}"
    );
    // Roughly 252 trading days rather than ~261 weekdays.
    let days = shaped().trading_days_per_year();
    assert!((250.0..=262.0).contains(&days), "{days} trading days/year");
}

#[test]
fn governs_matches_contracts_but_not_neighbouring_roots() {
    let cal = shaped();
    for yes in ["ES", "ESH4", "ESH24", "ESZ9", "ES.FUT", "ES.v.0", "NQM5"] {
        assert!(cal.governs(yes), "{yes} should be governed");
    }
    for no in [
        "ESG", "MES", "ESH", "ESHH4", "CL", "CLM4", "SYN:RW", "", "E",
    ] {
        assert!(!cal.governs(no), "{no} should not be governed");
    }
}

#[test]
fn for_instrument_declines_what_no_table_claims() {
    let found = Calendar::for_instrument(&InstrumentId::new("SYN:RW")).expect("tables parse");
    assert!(found.is_none(), "synthetic instruments have no exchange");
}

#[test]
fn a_table_without_sources_is_refused() {
    let bad = PLAIN.replace(r#"sources = ["hand-written test fixture"]"#, "sources = []");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

#[test]
fn a_session_that_does_not_span_midnight_is_refused() {
    let bad = PLAIN.replace(r#"open_local = "17:00""#, r#"open_local = "09:00""#);
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

#[test]
fn an_unknown_field_is_a_hard_error() {
    let bad = format!("{PLAIN}\nunexpected_key = 3\n");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Unparseable { .. })
    ));
}

#[test]
fn an_unknown_timezone_is_refused() {
    let bad = PLAIN.replace(
        r#"timezone = "America/Chicago""#,
        r#"timezone = "Mars/Olympus""#,
    );
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::UnknownTimezone { .. })
    ));
}

// --- the bundled tables ------------------------------------------------
//
// Structural only. The *contents* are facts about a real exchange, checked
// against the cited sources rather than against this file.

#[test]
fn every_bundled_table_parses_and_cites_sources() {
    let all = Calendar::all().expect("bundled calendar tables must parse");
    assert!(!all.is_empty(), "this build carries no calendars at all");
    for cal in &all {
        assert!(!cal.sources().is_empty(), "{} cites nothing", cal.id());
        assert!(!cal.roots().is_empty(), "{} claims no roots", cal.id());
        assert!(
            cal.trading_days_per_year() > 200.0,
            "{} reports {} trading days a year, which cannot be right",
            cal.id(),
            cal.trading_days_per_year()
        );
    }
}

#[test]
fn bundled_ids_are_unique() {
    let ids = Calendar::bundled_ids().expect("bundled calendar tables must parse");
    let mut sorted = ids.clone();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "duplicate calendar id in a table");
}

#[test]
fn an_unknown_id_names_what_does_exist() {
    let err = Calendar::by_id("not_a_calendar").expect_err("no such calendar");
    let message = err.to_string();
    assert!(message.contains("not_a_calendar"));
    for id in Calendar::bundled_ids().expect("tables parse") {
        assert!(message.contains(&id), "the error should name {id}");
    }
}

// An "early" close later than the session's own close would make the holiday
// session longer than a normal day — a typo that would be invisible in every
// number the calendar produces.
#[test]
fn an_early_close_later_than_the_normal_close_is_refused() {
    let bad = SHAPED.replace(
        r#"effect = { kind = "early_close", close_local = "12:00" }"#,
        r#"effect = { kind = "early_close", close_local = "16:30" }"#,
    );
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

#[test]
fn a_one_off_early_close_later_than_the_normal_close_is_refused() {
    let bad = SHAPED.replace(
        r#"name = "Test day of mourning"
effect = { kind = "closed" }"#,
        r#"name = "Test day of mourning"
effect = { kind = "early_close", close_local = "23:00" }"#,
    );
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

// ---------------------------------------------------------------------------
// The same-day session shape, and the bundled US equity/options table (D-0058).
// ---------------------------------------------------------------------------

/// Loads the bundled US equity calendar, or explains why it could not.
fn us() -> Calendar {
    Calendar::by_id("us_equity_options").expect("the bundled table must parse and be bundled")
}

// A cash-equity session opens and closes on the same date. Before this existed
// the loader hard-refused such a table — "a trading day must open on the
// previous calendar day" — which encoded "every market is CME" into a module
// whose docs claim to describe exchanges generally.
#[test]
fn a_same_day_calendar_has_no_overnight_leg() {
    let cal = us();
    // 2024-01-02 is EST (UTC-5). Unix days to 2024-01-02 = 19_724, so midnight
    // UTC is 19_724 * 86_400 = 1_704_153_600. 09:30 ET = 14:30 UTC = +52_200s;
    // 16:00 ET = 21:00 UTC = +75_600s.
    let midnight_utc = 1_704_153_600i64;
    let open = Ts((midnight_utc + 52_200) * 1_000_000_000);
    let before_open = Ts((midnight_utc + 52_100) * 1_000_000_000);
    let close = Ts((midnight_utc + 75_600) * 1_000_000_000);
    let midday = Ts((midnight_utc + 60_000) * 1_000_000_000);

    assert!(cal.is_open(open), "09:30 ET is open");
    assert!(cal.is_open(midday));
    assert!(!cal.is_open(before_open), "09:28 ET is not open");
    assert!(!cal.is_open(close), "16:00 ET is the close, exclusive");

    // The decisive one: an overnight calendar would call 03:00 ET open,
    // because the session that started the previous evening is still running.
    let overnight = Ts((midnight_utc + 3 * 3600) * 1_000_000_000);
    assert!(
        !cal.is_open(overnight),
        "cash equities do not trade at 03:00"
    );

    assert_eq!(cal.session_of(midday), SessionId::Regular);
    assert_eq!(cal.session_of(overnight), SessionId::Closed);
}

// An overnight calendar attributes the evening to tomorrow's trade date. A
// same-day one must not: rolling forward at 09:30 would label a whole session
// with the next day's date.
#[test]
fn a_same_day_calendar_attributes_the_session_to_its_own_date() {
    let cal = us();
    let midnight_utc = 1_704_153_600i64;
    let midday = Ts((midnight_utc + 60_000) * 1_000_000_000);
    assert_eq!(
        cal.trading_day(midday),
        CivilDate {
            year: 2024,
            month: 1,
            day: 2
        }
    );
}

// Hand-derived, and the single best check that the holiday set is right.
// 2024 is a leap year beginning on a Monday, so it holds 366 days = 52 weeks
// + Monday + Tuesday, giving 52*5 + 2 = 262 weekdays. Ten NYSE holidays fall
// on weekdays that year: New Year (Mon 1 Jan), MLK (15 Jan), Washington's
// Birthday (19 Feb), Good Friday (29 Mar), Memorial (27 May), Juneteenth
// (Wed 19 Jun), Independence (Thu 4 Jul), Labor (2 Sep), Thanksgiving
// (28 Nov) and Christmas (Wed 25 Dec). 262 - 10 = 252, the figure the
// exchange publishes.
#[test]
fn twenty_twenty_four_has_two_hundred_and_fifty_two_sessions() {
    let cal = us();
    let mut day = CivilDate {
        year: 2024,
        month: 1,
        day: 1,
    };
    let end = CivilDate {
        year: 2025,
        month: 1,
        day: 1,
    };
    let mut sessions = 0;
    while days_from_civil(day) < days_from_civil(end) {
        if cal.is_trading_day(day) {
            sessions += 1;
        }
        day = add_days(day, 1);
    }
    assert_eq!(sessions, 252);
}

// THE divergence that justifies this file existing rather than pointing
// ThetaData at the CME table. The NYSE closed for two consecutive days; CME
// Globex kept trading electronically and only cancelled its floor session, so
// `cme_globex.toml` records these dates as early closes. Reusing CME's
// calendar would have reported two real closures as missing data.
#[test]
fn hurricane_sandy_closed_the_equity_market_but_not_globex() {
    let equities = us();
    let futures = Calendar::by_id("cme_globex_equity_index").expect("parses and is bundled");

    for day in [29, 30] {
        let date = CivilDate {
            year: 2012,
            month: 10,
            day,
        };
        assert!(
            !equities.is_trading_day(date),
            "2012-10-{day}: the NYSE was closed"
        );
        assert!(
            futures.is_trading_day(date),
            "2012-10-{day}: Globex traded electronically — this is the disagreement"
        );
    }
}

// Days of national mourning are the same story: a full equity closure against
// an abbreviated CME session.
#[test]
fn days_of_national_mourning_close_the_equity_market_outright() {
    let cal = us();
    for (year, month, day) in [(2018, 12, 5), (2025, 1, 9)] {
        assert!(
            !cal.is_trading_day(CivilDate { year, month, day }),
            "{year}-{month:02}-{day:02} was a full closure"
        );
    }
}

// Juneteenth is rule-limited rather than back-dated. Applying it to 2012-2021
// would delete ten sessions that genuinely traded, which is the error that
// hides — a missing session looks like a vendor gap, not like a calendar bug.
#[test]
fn juneteenth_closes_only_from_2022() {
    let cal = us();
    // 2021-06-18 was a Friday and an ordinary session; the holiday did not
    // exist yet. (19 June 2021 was a Saturday.)
    assert!(cal.is_trading_day(CivilDate {
        year: 2021,
        month: 6,
        day: 18
    }));
    // First observance: Monday 20 June 2022, since 19 June fell on a Sunday.
    assert!(!cal.is_trading_day(CivilDate {
        year: 2022,
        month: 6,
        day: 20
    }));
    assert!(!cal.is_trading_day(CivilDate {
        year: 2024,
        month: 6,
        day: 19
    }));
}

// The NYSE does NOT close on the Friday before a Saturday New Year's Day. The
// generic federal nearest-weekday rule would close 2021-12-31, deleting a real
// session — one of the few places US equity observance genuinely differs.
#[test]
fn a_saturday_new_year_does_not_close_the_preceding_friday() {
    let cal = us();
    assert!(
        cal.is_trading_day(CivilDate {
            year: 2021,
            month: 12,
            day: 31
        }),
        "31 Dec 2021 traded a full session"
    );
    // Good Friday, by contrast, closes every year in this window — unlike CME,
    // which runs an abbreviated session when payrolls land on it.
    assert!(!cal.is_trading_day(CivilDate {
        year: 2024,
        month: 3,
        day: 29
    }));
}

#[test]
fn the_day_after_thanksgiving_is_an_early_close_not_a_closure() {
    let cal = us();
    let date = CivilDate {
        year: 2024,
        month: 11,
        day: 29,
    };
    assert!(cal.is_trading_day(date), "it trades");
    assert_eq!(
        cal.day_effect(date),
        DayEffect::EarlyClose {
            name: "Day after Thanksgiving".to_owned(),
            close_local: "13:00".to_owned()
        }
    );
}

// The table claims only the roots whose hours it actually describes. Index
// options keep the same holidays but different hours, so claiming them would
// give `is_open` a confident wrong answer for exactly the roots this project
// cares most about.
#[test]
fn the_us_table_claims_etf_roots_and_declines_index_roots() {
    for root in ["SPY", "QQQ", "IWM", "DIA"] {
        let found = Calendar::for_instrument(&InstrumentId::new(root))
            .expect("parses")
            .expect("claimed");
        assert_eq!(found.id(), "us_equity_options", "{root}");
    }
    for root in ["SPX", "SPXW", "VIX", "NDX", "RUT"] {
        assert!(
            Calendar::for_instrument(&InstrumentId::new(root))
                .expect("parses")
                .is_none(),
            "{root} must not be claimed — its hours are not this template's"
        );
    }
    // And the CME table still owns its own roots.
    let es = Calendar::for_instrument(&InstrumentId::new("ESH4"))
        .expect("parses")
        .expect("claimed");
    assert_eq!(es.id(), "cme_globex_equity_index");
}

// A same-day table whose close precedes its open is as wrong as an overnight
// table whose does not, and must be refused rather than silently producing a
// negative-length session.
#[test]
fn a_same_day_table_with_an_inverted_session_is_refused() {
    let bad = r#"
schema_version = 1

[[calendar]]
id = "backwards"
description = "closes before it opens"
timezone = "America/New_York"
roots = ["ZZZ"]
valid_from = "2020-01-01"
sources = ["fixture"]

[calendar.session]
shape = "same_day"
open_local = "16:00"
close_local = "09:30"
rth_open_local = "09:30"
rth_close_local = "16:00"
source = "fixture"

[calendar.reference_span]
start = "2020-01-01"
end = "2021-01-01"
rationale = "fixture"
"#;
    assert!(matches!(
        Calendar::parse_table("fixture", bad),
        Err(CalendarError::Invalid { .. })
    ));
}
