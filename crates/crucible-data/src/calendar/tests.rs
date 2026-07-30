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
    for yes in [
        "ES", "ESH4", "ESH24", "ESZ9", "ES.FUT", "ES.v.0", "NQM5", "ESH2024", "NQZ2010",
    ] {
        assert!(cal.governs(yes), "{yes} should be governed");
    }
    for no in [
        "ESG", "MES", "ESH", "ESHH4", "CL", "CLM4", "SYN:RW", "", "E", "CLZ2036",
    ] {
        assert!(!cal.governs(no), "{no} should not be governed");
    }
}

// The control for the regression D-0072 nearly shipped. Curated contracts are
// now keyed `ESH2024`, and this module used to recognise a contract by its own
// rule — "a month code and one or two year digits" — so a four-digit key stopped
// being an ES contract, no calendar claimed it, and `backtest` silently fell
// back to measuring `bars_per_year` from the sample. That changes the
// annualization factor, which changes every Sharpe (D-0039), and nothing would
// have failed. All three spellings must name the same contract to the calendar,
// because they name the same contract to everything else.
#[test]
fn every_spelling_of_one_contract_is_governed_by_the_same_calendar() {
    let mut ids = Vec::new();
    for spelling in ["ESH4", "ESH24", "ESH2024"] {
        let calendar = Calendar::for_instrument(&InstrumentId::new(spelling))
            .expect("tables parse")
            .unwrap_or_else(|| panic!("{spelling} must be claimed by the CME equity-index table"));
        ids.push(calendar.id().to_owned());
    }
    assert_eq!(ids[0], ids[1]);
    assert_eq!(ids[1], ids[2]);
    assert_eq!(ids[0], "cme_globex_equity_index");
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

// A day-level claim is a claim about an exchange this table does not otherwise
// describe. Unsourced, it is exactly the kind of plausible assertion the whole
// table format exists to prevent, so it refuses at load (D-0059).
#[test]
fn planted_day_level_roots_without_a_source_is_refused() {
    let unsourced = r#"
schema_version = 1

[[calendar]]
id = "unsourced"
description = "claims another exchange with no citation"
timezone = "America/New_York"
roots = ["AAA"]
day_level_roots = ["BBB"]
valid_from = "2020-01-01"
sources = ["fixture"]

[calendar.session]
shape = "same_day"
open_local = "09:30"
close_local = "16:00"
rth_open_local = "09:30"
rth_close_local = "16:00"
source = "fixture"

[calendar.reference_span]
start = "2020-01-01"
end = "2021-01-01"
rationale = "fixture"
"#;
    assert!(matches!(
        Calendar::parse_table("fixture", unsourced),
        Err(CalendarError::Invalid { .. })
    ));

    // The same table WITH a citation loads, so the refusal is about the
    // missing source and not about the feature.
    let sourced = unsourced.replace(
        r#"day_level_roots = ["BBB"]"#,
        "day_level_roots = [\"BBB\"]\nday_level_source = \"https://example.invalid/hours\"",
    );
    Calendar::parse_table("fixture", &sourced).expect("a sourced claim is fine");
}

// Listing a root in both would make `governs` and `governs_days` disagree about
// which answer is authoritative for it.
#[test]
fn planted_root_in_both_lists_is_refused() {
    let both = r#"
schema_version = 1

[[calendar]]
id = "double"
description = "same root twice"
timezone = "America/New_York"
roots = ["AAA"]
day_level_roots = ["AAA"]
day_level_source = "https://example.invalid/hours"
valid_from = "2020-01-01"
sources = ["fixture"]

[calendar.session]
shape = "same_day"
open_local = "09:30"
close_local = "16:00"
rth_open_local = "09:30"
rth_close_local = "16:00"
source = "fixture"

[calendar.reference_span]
start = "2020-01-01"
end = "2021-01-01"
rationale = "fixture"
"#;
    assert!(matches!(
        Calendar::parse_table("fixture", both),
        Err(CalendarError::Invalid { .. })
    ));
}

// The defect D-0059 recorded and deferred, now closed (D-0086) — same six
// dates, opposite assertion.
//
// These six dates are NOT early closes at the real exchange. The NYSE (and
// Cboe, whose 2026 schedule confirms it) closes early on 3 July only when
// 4 July falls Tuesday-Friday; when it lands on a weekend or a Monday there is
// no early close at all. `HolidayRule::WeekdayBefore` had no way to express
// that condition, so the table fired anyway, and this test asserted the wrong
// behaviour on purpose so that whoever fixed it would have to flip it
// deliberately. `anchor_weekday` is that fix, and this is that flip.
//
// The condition is checked against the **unobserved** anchor, which is why a
// Saturday 4 July (2015, 2020, 2026) suppresses the rule even though the
// observed holiday lands on Friday 3 July: the question is which weekday the
// holiday falls on, not which weekday precedes it.
#[test]
fn the_six_july_early_closes_that_were_phantoms_are_ordinary_sessions() {
    let cal = us();
    let phantoms = [
        (2015, 7, 2),
        (2016, 7, 1),
        (2020, 7, 2),
        (2021, 7, 2),
        (2022, 7, 1),
        (2026, 7, 2),
    ];
    for (year, month, day) in phantoms {
        let date = CivilDate { year, month, day };
        assert_eq!(
            cal.day_effect(date),
            DayEffect::Normal,
            "{year}-{month:02}-{day:02} is a full session at the real exchange"
        );
        assert!(cal.is_trading_day(date));
    }
    // And the genuine case still fires: 4 July 2024 was a Thursday, so
    // Wednesday 3 July really was a 13:00 close. Two-sided, because a rule
    // that suppressed everything would also pass the loop above.
    assert!(matches!(
        cal.day_effect(CivilDate {
            year: 2024,
            month: 7,
            day: 3
        }),
        DayEffect::EarlyClose { .. }
    ));
    // 2019-07-04 was a Thursday too, and 2013-07-04 a Thursday: both keep it.
    assert!(matches!(
        cal.day_effect(CivilDate {
            year: 2019,
            month: 7,
            day: 3
        }),
        DayEffect::EarlyClose { .. }
    ));
}

// ---------------------------------------------------------------------------
// Session eras (D-0086)
// ---------------------------------------------------------------------------

/// Two eras with different closes and a halt that exists in only one of them —
/// the exact shape CME equity index had, reduced to arithmetic that fits in a
/// comment.
const ERAS: &str = r#"
schema_version = 1

[[calendar]]
id = "test_eras"
description = "two session eras, one with a halt"
timezone = "America/Chicago"
roots = ["YY"]
valid_from = "2020-01-01"
sources = ["hand-written test fixture"]

[[calendar.era]]
from = "2020-01-01"
open_local = "17:00"
close_local = "16:15"
halt_local = [["15:15", "15:30"]]
rth_open_local = "08:30"
rth_close_local = "15:00"
source = "hand-written test fixture"

[calendar.session]
from = "2024-06-03"
open_local = "17:00"
close_local = "16:00"
rth_open_local = "08:30"
rth_close_local = "15:00"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2025-01-01"
end = "2026-01-01"
rationale = "one calendar year, entirely inside the current era"
"#;

fn eras() -> Calendar {
    let mut all = Calendar::parse_table("fixture", ERAS).expect("fixture parses");
    all.pop().expect("fixture has one calendar")
}

// Tuesday 2024-05-28 is in era 1: 17:00 -> 15:15 (22h15m = 80,100 s) plus
// 15:30 -> 16:15 (45m = 2,700 s) = 82,800 s = 23 h. Tuesday 2024-06-04 is in
// era 2: 17:00 -> 16:00 = 82,800 s in one interval. Same total length,
// different shape — which is exactly why a single-template calendar looks right
// until you ask where the bars are.
#[test]
fn each_era_answers_with_its_own_session_template() {
    let cal = eras();

    let before = cal.open_intervals(d(2024, 5, 28));
    assert_eq!(before.len(), 2, "era 1 has a halt");
    assert_eq!(before[0].1.0 - before[0].0.0, 80_100 * 1_000_000_000);
    assert_eq!(before[1].1.0 - before[1].0.0, 2_700 * 1_000_000_000);

    let after = cal.open_intervals(d(2024, 6, 4));
    assert_eq!(after.len(), 1, "era 2 has none");
    assert_eq!(after[0].1.0 - after[0].0.0, 82_800 * 1_000_000_000);

    assert_eq!(cal.era_starts(), vec![d(2020, 1, 1), d(2024, 6, 3)]);
}

// The boundary is a trading day, not a moment inside one: 2024-06-03 is the
// first day answered by the new template and 2024-05-31 the last answered by
// the old one.
#[test]
fn the_era_boundary_is_exact_to_the_trading_day() {
    let cal = eras();
    assert_eq!(cal.open_intervals(d(2024, 5, 31)).len(), 2);
    assert_eq!(cal.open_intervals(d(2024, 6, 3)).len(), 1);
}

// A date before every era still gets an answer, because `open_intervals` is
// total and there is nowhere to put a `Result` inside replay. It gets the
// OLDEST era's answer — a later era's hours would be a bigger lie about an
// earlier exchange.
#[test]
fn a_date_before_the_oldest_era_gets_the_oldest_eras_answer() {
    let cal = eras();
    assert_eq!(cal.open_intervals(d(2015, 6, 3)).len(), 2);
}

// The `\n` anchor matters: without it the pattern also matches inside
// `valid_from = "2020-01-01"` and the mutation becomes a TOML syntax error,
// which the test would happily accept for the wrong reason.
#[test]
fn an_era_without_a_from_is_refused() {
    let bad = ERAS.replace("\nfrom = \"2020-01-01\"\n", "\n");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

#[test]
fn a_session_without_a_from_beside_eras_is_refused() {
    let bad = ERAS.replace("from = \"2024-06-03\"\n", "");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

// `session` is the current era by definition. A table whose `era` entry starts
// later would silently make `session` describe the middle of history.
#[test]
fn an_era_newer_than_the_session_is_refused() {
    let bad = ERAS.replace("\nfrom = \"2020-01-01\"\n", "\nfrom = \"2025-06-03\"\n");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

// A `valid_from` earlier than every template is a claim with nothing behind it.
#[test]
fn a_valid_from_before_the_oldest_era_is_refused() {
    let bad = ERAS.replace("valid_from = \"2020-01-01\"", "valid_from = \"2019-01-01\"");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

#[test]
fn two_templates_starting_on_the_same_day_are_refused() {
    let bad = ERAS.replace("from = \"2024-06-03\"", "from = \"2020-01-01\"");
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

// The check D-0039 stated as prose and D-0086 made binding: a reference span
// that crosses an era boundary averages two exchanges and describes neither.
#[test]
fn a_reference_span_crossing_an_era_boundary_is_refused() {
    let bad = ERAS.replace("start = \"2025-01-01\"", "start = \"2023-01-01\"");
    let err = Calendar::parse_table("fixture", &bad).expect_err("must refuse");
    match err {
        CalendarError::Invalid { reason, .. } => {
            assert!(
                reason.contains("era"),
                "the message must name the cause: {reason}"
            );
        }
        other => panic!("wrong error: {other}"),
    }
}

// An early close must be early in EVERY era, so it is checked against the
// earliest normal close. 16:05 is before era 1's 16:15 and after era 2's 16:00.
#[test]
fn an_early_close_valid_in_one_era_only_is_refused() {
    let bad = ERAS.replace(
        "[calendar.reference_span]",
        "[[calendar.holiday]]\n\
         name = \"Plausible typo\"\n\
         rule = { kind = \"fixed_date\", month = 3, day = 3, observance = \"actual\" }\n\
         effect = { kind = \"early_close\", close_local = \"16:05\" }\n\
         source = \"hand-written test fixture\"\n\n\
         [calendar.reference_span]",
    );
    assert!(matches!(
        Calendar::parse_table("fixture", &bad),
        Err(CalendarError::Invalid { .. })
    ));
}

// ---------------------------------------------------------------------------
// The bundled CME equity-index table, against the archive it was measured from
// ---------------------------------------------------------------------------

fn cme() -> Calendar {
    Calendar::by_id("cme_globex_equity_index").expect("the bundled table must parse")
}

/// The instant of a local Chicago wall-clock time, derived by hand from the
/// UTC offset the caller states. CST is UTC−6, CDT is UTC−5.
fn chicago(date: CivilDate, hour: i64, minute: i64, utc_offset_hours: i64) -> Ts {
    Ts((crate::ingest::window::days_from_civil(date) * 86_400
        + (hour + utc_offset_hours) * 3600
        + minute * 60)
        * 1_000_000_000)
}

// THE D-0040 CORRECTION, CORRECTED (D-0086).
//
// The 15:15-15:30 CT halt was real until 2021-06-25 and gone from 2021-06-28.
// The archive says so with 2,018 silent trading dates before and 1,344 fully
// traded ones after; CME's SER-8788R says so too. A table with one session
// template cannot hold both answers, and the one it held was right for January
// 2024 and wrong for the five and a half years before it.
#[test]
fn the_afternoon_halt_exists_in_era_3a_and_not_in_era_3b() {
    let cal = cme();
    // Tuesday 2018-05-08, 15:20 CDT (UTC-5) = 20:20 UTC.
    assert!(
        !cal.is_open(chicago(d(2018, 5, 8), 15, 20, 5)),
        "the halt was real in 2018"
    );
    // Wednesday 2024-05-08, same wall clock.
    assert!(
        cal.is_open(chicago(d(2024, 5, 8), 15, 20, 5)),
        "and gone in 2024"
    );
    // The last era-3a session and the first era-3b one, to the day.
    assert!(!cal.is_open(chicago(d(2021, 6, 25), 15, 20, 5)));
    assert!(cal.is_open(chicago(d(2021, 6, 28), 15, 20, 5)));
}

// Era 2 closed at 16:15 CT and era 3 at 16:00. Friday 2014-05-16 at 16:05 CDT
// was trading; Friday 2016-05-20 at the same wall clock was not.
#[test]
fn the_close_moved_from_sixteen_fifteen_to_sixteen_hundred_in_2015() {
    let cal = cme();
    assert!(cal.is_open(chicago(d(2014, 5, 16), 16, 5, 5)));
    assert!(!cal.is_open(chicago(d(2016, 5, 20), 16, 5, 5)));
    assert_eq!(
        cal.era_starts(),
        vec![d(2012, 11, 19), d(2015, 9, 21), d(2021, 6, 28)]
    );
    assert_eq!(cal.valid_from(), d(2012, 11, 19));
}

// Christmas landing on a Saturday closes the Friday before; New Year's Day
// landing on a Saturday does not. Both are measured: 2021-12-24 has no session
// at all in the archive and 2021-12-31 is a full one.
#[test]
fn a_saturday_christmas_closes_the_friday_and_a_saturday_new_year_does_not() {
    let cal = cme();
    assert!(!cal.is_trading_day(d(2021, 12, 24)));
    assert_eq!(cal.open_intervals(d(2021, 12, 24)), Vec::new());
    assert!(cal.is_trading_day(d(2021, 12, 31)));
    assert_eq!(cal.day_effect(d(2021, 12, 31)), DayEffect::Normal);
    // Christmas Eve on an ordinary weekday is still a 12:15 early close.
    assert!(matches!(
        cal.day_effect(d(2024, 12, 24)),
        DayEffect::EarlyClose { .. }
    ));
}

// The Monday holidays were full closures in 2013-2014 and 12:00 CT halts from
// 2014/2015. Measured from the last traded minute before 17:00 CT, and from
// the Sunday evenings that did not open.
#[test]
fn the_monday_holidays_were_closures_before_they_were_halts() {
    let cal = cme();
    assert!(matches!(
        cal.day_effect(d(2013, 1, 21)),
        DayEffect::Closed { .. }
    ));
    assert!(matches!(
        cal.day_effect(d(2014, 1, 20)),
        DayEffect::Closed { .. }
    ));
    assert!(matches!(
        cal.day_effect(d(2015, 1, 19)),
        DayEffect::EarlyClose { .. }
    ));
    // Memorial Day switched a year earlier than MLK did.
    assert!(matches!(
        cal.day_effect(d(2013, 5, 27)),
        DayEffect::Closed { .. }
    ));
    assert!(matches!(
        cal.day_effect(d(2014, 5, 26)),
        DayEffect::EarlyClose { .. }
    ));
}

// The July-3 early close, and the condition that stops it. 4 July 2024 was a
// Thursday (so 3 July closed early); 4 July 2022 was a Monday and 2026 a
// Saturday (so neither 1 July 2022 nor 2 July 2026 did).
#[test]
fn the_day_before_independence_day_closes_early_only_when_the_fourth_is_midweek() {
    let cal = cme();
    assert!(matches!(
        cal.day_effect(d(2024, 7, 3)),
        DayEffect::EarlyClose { .. }
    ));
    assert!(matches!(
        cal.day_effect(d(2013, 7, 3)),
        DayEffect::EarlyClose { .. }
    ));
    assert_eq!(cal.day_effect(d(2022, 7, 1)), DayEffect::Normal);
    assert_eq!(cal.day_effect(d(2026, 7, 2)), DayEffect::Normal);
    // 2012-07-03 traded in full: the rule starts in 2013.
    assert_eq!(cal.day_effect(d(2012, 7, 3)), DayEffect::Normal);
}

// ---------------------------------------------------------------------------
// The four new tables (D-0086), against the archive they were measured from
// ---------------------------------------------------------------------------

// Every root the acquisition basket holds now resolves to a calendar, and to
// the right one. Before D-0086 four of the seven resolved to nothing and
// `bars_per_year` fell back to measuring the sample.
#[test]
fn every_archived_root_resolves_to_its_own_calendar() {
    for (contract, expected) in [
        ("ESH2024", "cme_globex_equity_index"),
        ("NQH2024", "cme_globex_equity_index"),
        ("RTYH2024", "cme_globex_equity_index"),
        ("CLM2024", "cme_globex_energy"),
        ("GCZ2014", "cme_globex_metals"),
        ("6EH2024", "cme_globex_fx"),
        ("ZNH2024", "cme_globex_rates"),
    ] {
        let cal = Calendar::for_instrument(&InstrumentId::new(contract))
            .expect("tables parse")
            .unwrap_or_else(|| panic!("{contract} must be claimed"));
        assert_eq!(cal.id(), expected, "for {contract}");
    }
}

// One date, four answers — the reason these are four tables and not one.
// MLK Day 2022-01-17, from the last traded minute before 17:00 CT in the
// archive: ES 12:00, ZN 12:00, CL and GC 13:30, 6E a full session to 15:58.
#[test]
fn one_holiday_has_four_different_answers_across_the_four_calendars() {
    let mlk = d(2022, 1, 17);
    let close_of = |id: &str| match Calendar::by_id(id).expect("bundled").day_effect(mlk) {
        DayEffect::EarlyClose { close_local, .. } => close_local,
        DayEffect::Normal => "normal".to_owned(),
        DayEffect::Closed { .. } => "closed".to_owned(),
    };
    assert_eq!(close_of("cme_globex_equity_index"), "12:00");
    assert_eq!(close_of("cme_globex_rates"), "12:00");
    assert_eq!(close_of("cme_globex_energy"), "13:30");
    assert_eq!(close_of("cme_globex_metals"), "13:30");
    assert_eq!(close_of("cme_globex_fx"), "normal");
    // And before 2022 energy closed at noon like everything else.
    assert!(matches!(
        Calendar::by_id("cme_globex_energy")
            .expect("bundled")
            .day_effect(d(2019, 1, 21)),
        DayEffect::EarlyClose { close_local, .. } if close_local == "12:00"
    ));
    // As did FX, which stopped observing the holiday rather than moving it.
    assert!(matches!(
        Calendar::by_id("cme_globex_fx")
            .expect("bundled")
            .day_effect(d(2021, 1, 18)),
        DayEffect::EarlyClose { close_local, .. } if close_local == "12:00"
    ));
}

// A Good Friday carrying the Employment Situation release: equity index runs to
// 08:15 CT, rates and FX to 10:15 CT, and energy and metals do not open at all.
// Measured on 2023-04-07 and 2026-04-03; both years agree.
#[test]
fn a_nonfarm_payrolls_good_friday_splits_the_five_calendars_three_ways() {
    for date in [d(2023, 4, 7), d(2026, 4, 3)] {
        assert!(matches!(
            cme().day_effect(date),
            DayEffect::EarlyClose { close_local, .. } if close_local == "08:15"
        ));
        for id in ["cme_globex_rates", "cme_globex_fx"] {
            assert!(
                matches!(
                    Calendar::by_id(id).expect("bundled").day_effect(date),
                    DayEffect::EarlyClose { close_local, .. } if close_local == "10:15"
                ),
                "{id} on {date}"
            );
        }
        for id in ["cme_globex_energy", "cme_globex_metals"] {
            assert!(
                matches!(
                    Calendar::by_id(id).expect("bundled").day_effect(date),
                    DayEffect::Closed { .. }
                ),
                "{id} on {date}"
            );
            assert!(!Calendar::by_id(id).expect("bundled").is_trading_day(date));
        }
    }
}

// The bond-calendar assumption, refused because the archive refuses it.
// `docs/THETADATA_PLAN.md` §8.1 records Veterans Day as a day the NYSE trades
// and the bond market does not, and that is true of the *cash* market. CBOT
// Treasury futures on Globex traded a full session on every Columbus Day and
// every Veterans Day in the archive, so this table has neither.
#[test]
fn treasury_futures_trade_through_columbus_day_and_veterans_day() {
    let zn = Calendar::by_id("cme_globex_rates").expect("bundled");
    for date in [
        d(2024, 10, 14), // Columbus Day 2024
        d(2023, 10, 9),
        d(2024, 11, 11), // Veterans Day 2024
        d(2025, 11, 11),
    ] {
        assert_eq!(zn.day_effect(date), DayEffect::Normal, "{date}");
        assert!(zn.is_trading_day(date), "{date}");
    }
    // Two-sided: the holidays it DOES observe still fire.
    assert!(matches!(
        zn.day_effect(d(2024, 11, 28)),
        DayEffect::EarlyClose { .. }
    ));
}

// The equity-index July-3 rule is equity-index's alone: ZN and CL traded to
// 16:00 CT on every one of the July 3rds ES closed early on.
#[test]
fn only_equity_index_closes_early_the_day_before_independence_day() {
    let zn = Calendar::by_id("cme_globex_rates").expect("bundled");
    let cl = Calendar::by_id("cme_globex_energy").expect("bundled");
    for date in [d(2017, 7, 3), d(2019, 7, 3), d(2024, 7, 3), d(2025, 7, 3)] {
        assert!(matches!(
            cme().day_effect(date),
            DayEffect::EarlyClose { .. }
        ));
        assert_eq!(zn.day_effect(date), DayEffect::Normal, "{date}");
        assert_eq!(cl.day_effect(date), DayEffect::Normal, "{date}");
    }
}

// None of the four new calendars carries the 15:15-15:30 CT halt, because none
// of the four products ever had it. Same wall clock, same date, five calendars.
#[test]
fn no_commodity_calendar_carries_the_equity_index_halt() {
    let ts = chicago(d(2018, 5, 8), 15, 20, 5);
    assert!(!cme().is_open(ts), "equity index was halted");
    for id in [
        "cme_globex_energy",
        "cme_globex_metals",
        "cme_globex_fx",
        "cme_globex_rates",
    ] {
        assert!(
            Calendar::by_id(id).expect("bundled").is_open(ts),
            "{id} traded straight through"
        );
    }
}

// ---------------------------------------------------------------------------
// The commodity eras, D-TBD(commodity-calendar-eras)
// ---------------------------------------------------------------------------

/// The four commodity tables reach the archive's first date, and each says how.
///
/// Before this change three of them started 2015-09-21 and the fourth
/// 2011-10-03, so 5.3 of the archive's 16.1 years were answered by a template
/// that did not describe them. `valid_from` is 2010-06-06 for all four now —
/// the Sunday the archive opens, on which every one of the four has its first
/// bar, at exactly the open its oldest era claims.
#[test]
fn every_commodity_table_reaches_the_archives_first_date() {
    let first = d(2010, 6, 6);
    for (id, eras) in [
        ("cme_globex_energy", vec![first, d(2015, 9, 21)]),
        ("cme_globex_metals", vec![first, d(2015, 9, 21)]),
        ("cme_globex_fx", vec![first]),
        ("cme_globex_rates", vec![first, d(2011, 10, 3)]),
    ] {
        let cal = Calendar::by_id(id).expect("bundled");
        assert_eq!(cal.valid_from(), first, "{id}");
        assert_eq!(cal.era_starts(), eras, "{id}");
    }
}

/// Energy and metals closed at 16:15 CT until 2015-09-18 and at 16:00 from
/// 2015-09-21 — the same advisory as equity index, and NOT the same as FX or
/// rates, which were already at 16:00 and did not move.
///
/// 16:05 CDT is 21:05 UTC. Friday 2014-05-16 was inside the older era, Friday
/// 2016-05-20 inside the current one. The FX and rates rows are the control
/// that makes the first two mean something: if the era had been added to the
/// wrong tables, these would flip.
#[test]
fn the_energy_and_metals_close_moved_from_sixteen_fifteen_in_2015() {
    let older = chicago(d(2014, 5, 16), 16, 5, 5);
    let newer = chicago(d(2016, 5, 20), 16, 5, 5);
    for id in ["cme_globex_energy", "cme_globex_metals"] {
        let cal = Calendar::by_id(id).expect("bundled");
        assert!(cal.is_open(older), "{id} traded to 16:15 CT in 2014");
        assert!(!cal.is_open(newer), "{id} closed at 16:00 CT in 2016");
    }
    for id in ["cme_globex_fx", "cme_globex_rates"] {
        let cal = Calendar::by_id(id).expect("bundled");
        assert!(!cal.is_open(older), "{id} closed at 16:00 CT in 2014 too");
        assert!(!cal.is_open(newer), "{id}");
    }
}

/// The rates open moved 17:30 -> 17:00 CT on Sunday 2011-10-02, and nothing
/// else about that session changed.
///
/// The boundary is asserted on the session *open instants*, because those are
/// exact: trading day 2011-09-26 opens 17:30 CDT on the Sunday before it and
/// trading day 2011-10-03 opens 17:00 CDT on the Sunday before that. 17:15 CDT
/// is 22:15 UTC, and it is the half hour the change created.
///
/// The close is the second half of the claim and is asserted on a Friday, where
/// a session close is legible: 15:55 CDT trades and 16:05 does not, in the
/// older era as in the current one. FX is the control — it opened at 17:00 in
/// both eras, so only the rates rows may move.
#[test]
fn the_rates_open_moved_from_seventeen_thirty_in_2011() {
    let zn = Calendar::by_id("cme_globex_rates").expect("bundled");
    let e6 = Calendar::by_id("cme_globex_fx").expect("bundled");

    assert_eq!(
        zn.open_intervals(d(2011, 9, 26))[0].0,
        chicago(d(2011, 9, 25), 17, 30, 5),
        "the last session of era 1 opens 17:30 CT"
    );
    assert_eq!(
        zn.open_intervals(d(2011, 10, 3))[0].0,
        chicago(d(2011, 10, 2), 17, 0, 5),
        "the first session of era 2 opens 17:00 CT"
    );
    assert_eq!(
        zn.session_open(d(2010, 6, 7)),
        chicago(d(2010, 6, 6), 17, 30, 5),
        "the archive's first ZN session"
    );

    // Away from the boundary evening the wall-clock reading agrees.
    assert!(!zn.is_open(chicago(d(2011, 9, 25), 17, 15, 5)), "era 1");
    assert!(zn.is_open(chicago(d(2011, 10, 9), 17, 15, 5)), "era 2");
    assert!(!zn.is_open(chicago(d(2010, 6, 8), 17, 15, 5)));
    assert!(zn.is_open(chicago(d(2012, 6, 6), 17, 15, 5)));
    for evening in [d(2011, 9, 25), d(2011, 10, 2), d(2010, 6, 8)] {
        assert!(
            e6.is_open(chicago(evening, 17, 15, 5)),
            "FX opened at 17:00 on {evening}"
        );
    }

    // ONE EVENING is wrong, and it is the cost `Calendar::trading_day`
    // documents rather than a defect this test missed: the era is chosen from
    // the calendar date an instant falls on, because the trading day is what is
    // being computed and the recursion has no base case. On Sunday 2011-10-02
    // that date is still era 1's, whose open is 17:30, so 17:15 is attributed
    // to Sunday and reads closed — while `open_intervals(2011-10-03)` above
    // correctly opens the session at 17:00. Thirty minutes, once, at the only
    // era boundary in the bundled tables that moves an OPEN time.
    assert!(!zn.is_open(chicago(d(2011, 10, 2), 17, 15, 5)));

    // Friday 2010-06-11, in the older era: 16:00 CT was the close then too.
    assert!(zn.is_open(chicago(d(2010, 6, 11), 15, 55, 5)));
    assert!(!zn.is_open(chicago(d(2010, 6, 11), 16, 5, 5)));
}

/// The 2012-2014 closure regime is the same nine dates on every CME calendar
/// this repository holds, and it starts at Thanksgiving **2012**.
///
/// `EVIDENCED` twice on each date: no day session, and no session on the
/// evening before — the Sunday before a Monday holiday, the Wednesday before
/// Thanksgiving. D-0086 encoded it for equity index only and started it in
/// 2013; the archive puts 2012-11-22 in it, for ES and NQ as well as for all
/// four commodity roots.
///
/// The two-sided control is the year on each side: 2012-09-03 was an early
/// close (12:15 CT for energy and metals, 12:00 for FX and rates) and
/// 2014-05-26 was one again (12:00 CT everywhere). A rule that closed the
/// market for the whole decade would pass the first half of this test.
#[test]
fn the_2012_closure_regime_covers_every_cme_calendar() {
    let ids = [
        "cme_globex_equity_index",
        "cme_globex_energy",
        "cme_globex_metals",
        "cme_globex_fx",
        "cme_globex_rates",
    ];
    for date in [
        d(2012, 11, 22),
        d(2013, 1, 21),
        d(2013, 2, 18),
        d(2013, 5, 27),
        d(2013, 7, 4),
        d(2013, 9, 2),
        d(2013, 11, 28),
        d(2014, 1, 20),
        d(2014, 2, 17),
    ] {
        for id in ids {
            let cal = Calendar::by_id(id).expect("bundled");
            assert!(
                matches!(cal.day_effect(date), DayEffect::Closed { .. }),
                "{id} on {date}"
            );
            assert!(!cal.is_trading_day(date), "{id} on {date}");
            assert_eq!(cal.open_intervals(date), Vec::new(), "{id} on {date}");
        }
    }
    // The year after: an early close everywhere, not a closure.
    for id in ids {
        let cal = Calendar::by_id(id).expect("bundled");
        assert!(
            matches!(cal.day_effect(d(2014, 5, 26)),
                DayEffect::EarlyClose { close_local, .. } if close_local == "12:00"),
            "{id} on 2014-05-26"
        );
    }
    // The year before. Equity index is excluded on purpose: 2012-09-03 is in
    // its era 1, which `valid_from = 2012-11-19` says this repository does not
    // model, and its 10:30 CT holiday closes are deliberately unencoded.
    for (id, close) in [
        ("cme_globex_energy", "12:15"),
        ("cme_globex_metals", "12:15"),
        ("cme_globex_fx", "12:00"),
        ("cme_globex_rates", "12:00"),
    ] {
        let cal = Calendar::by_id(id).expect("bundled");
        assert!(
            matches!(cal.day_effect(d(2012, 9, 3)),
                DayEffect::EarlyClose { close_local, .. } if close_local == close),
            "{id} on 2012-09-03"
        );
    }
}

/// Before the closure regime, energy and metals closed at 12:15 CT on a US
/// holiday and FX and rates at 12:00 — a fifteen-minute difference nobody
/// publishes, on every holiday from the archive's first to 2012.
#[test]
fn the_pre_closure_holiday_close_differs_by_exchange() {
    for date in [d(2011, 1, 17), d(2011, 11, 24), d(2012, 1, 16)] {
        for (id, close) in [
            ("cme_globex_energy", "12:15"),
            ("cme_globex_metals", "12:15"),
            ("cme_globex_fx", "12:00"),
            ("cme_globex_rates", "12:00"),
        ] {
            let cal = Calendar::by_id(id).expect("bundled");
            assert!(
                matches!(cal.day_effect(date),
                    DayEffect::EarlyClose { close_local, .. } if close_local == close),
                "{id} on {date}"
            );
        }
    }
}

/// The 2012 Employment-Situation Good Friday, which splits the calendars the
/// same three ways 2023 and 2026 do.
///
/// It is inside `valid_from` for rates and for FX now. Before this change the
/// rates table said 2012-04-06 was fully closed — inside its own `valid_from`,
/// on a date the archive shows trading to 10:15 CT.
#[test]
fn the_2012_nonfarm_payrolls_good_friday_is_dated_on_every_table() {
    let date = d(2012, 4, 6);
    assert!(matches!(
        cme().day_effect(date),
        DayEffect::EarlyClose { close_local, .. } if close_local == "08:15"
    ));
    for id in ["cme_globex_rates", "cme_globex_fx"] {
        assert!(
            matches!(Calendar::by_id(id).expect("bundled").day_effect(date),
                DayEffect::EarlyClose { close_local, .. } if close_local == "10:15"),
            "{id}"
        );
    }
    for id in ["cme_globex_energy", "cme_globex_metals"] {
        let cal = Calendar::by_id(id).expect("bundled");
        assert!(
            matches!(cal.day_effect(date), DayEffect::Closed { .. }),
            "{id}"
        );
        assert!(!cal.is_trading_day(date), "{id}");
    }
}

/// One New Year's Eve in sixteen years closed early, and only for two products.
///
/// A dated one-off rather than a rule, because a rule fitted to a single
/// observation would delete four hours from every other 31 December in the
/// archive — all of which traded in full.
#[test]
fn only_one_new_years_eve_closed_early_and_only_for_fx_and_rates() {
    for id in ["cme_globex_fx", "cme_globex_rates"] {
        let cal = Calendar::by_id(id).expect("bundled");
        assert!(
            matches!(cal.day_effect(d(2010, 12, 31)),
                DayEffect::EarlyClose { close_local, .. } if close_local == "12:15"),
            "{id}"
        );
        // Every other New Year's Eve the archive holds is an ordinary session.
        for year in [2013, 2014, 2015, 2019, 2024] {
            assert_eq!(cal.day_effect(d(year, 12, 31)), DayEffect::Normal, "{id}");
        }
    }
    for id in [
        "cme_globex_equity_index",
        "cme_globex_energy",
        "cme_globex_metals",
    ] {
        assert_eq!(
            Calendar::by_id(id)
                .expect("bundled")
                .day_effect(d(2010, 12, 31)),
            DayEffect::Normal,
            "{id}"
        );
    }
}

// ------------------------------------------------------- the session clock

const MINUTE_NS: i64 = 60 * 1_000_000_000;
const HOUR_NS: i64 = 60 * MINUTE_NS;

/// Hand-derived on PLAIN (17:00→16:00 CT, RTH 08:30–15:15).
///
/// Trading day 2024-01-03 opens 2024-01-02 17:00 CST = 23:00Z (1_704_236_400)
/// and closes 2024-01-03 16:00 CST = 22:00Z (1_704_319_200) — 23 hours.
///
/// The bar under test is the **last** of that session: its interval ends
/// exactly at the close, so `to_close_ns` is zero and `since_open_ns` is the
/// whole 23 hours.
///
/// Its session is `PostRegular`, not `Closed`, and that is the reason the clock
/// asks one nanosecond before `avail_ts`: open intervals are half-open, so a
/// bar ending at 16:00:00 traded entirely inside the session it just closed.
/// Asking at `avail_ts` would report the final bar of every trading day as
/// closed, and "flatten on the last bar" would be a rule that never fires.
#[test]
fn the_session_clock_reads_the_last_bar_of_a_session() {
    let cal = plain();
    let clock = cal.session_clock(utc(1_704_319_200));
    assert_eq!(clock.since_open_ns, 23 * HOUR_NS);
    assert_eq!(clock.to_close_ns, 0);
    assert_eq!(clock.session, SessionId::PostRegular);

    // The first bar of the same session, one minute in: 17:01 CST.
    let first = cal.session_clock(utc(1_704_236_400 + 60));
    assert_eq!(first.since_open_ns, MINUTE_NS);
    assert_eq!(first.to_close_ns, 23 * HOUR_NS - MINUTE_NS);
    assert_eq!(first.session, SessionId::Overnight);
    // Negative before the regular session, which is what lets a rule say
    // "the first half hour of RTH" and mean it.
    assert!(first.since_rth_open_ns < 0, "{first:?}");
}

/// The early close, and the control that makes it mean something.
///
/// SHAPED closes at 12:00 CT on Independence Day. 2024-07-04 was a Thursday, so
/// the rule lands on the day itself: trading day 2024-07-04 opens 2024-07-03
/// 17:00 CDT = 22:00Z (1_720_044_000) and closes 2024-07-04 12:00 CDT = 17:00Z
/// (1_720_112_400) — 19 hours instead of 23.
///
/// A bar available at 11:00 CDT that day (16:00Z, 1_720_108_800) is therefore
/// **one hour** from the close. The identical wall-clock bar on an ordinary
/// Thursday a week earlier — 2024-06-27, 16:00Z, 1_719_504_000 — is **five**.
/// Both are 18 hours from their session open, because an early close moves the
/// close and never the open. The difference between 1 h and 5 h is the holiday,
/// and it is the whole reason a strategy asks a calendar instead of subtracting
/// from 16:00.
#[test]
fn the_session_clock_counts_down_to_an_early_close() {
    let cal = shaped();
    assert!(matches!(
        cal.day_effect(d(2024, 7, 4)),
        DayEffect::EarlyClose { .. }
    ));

    let holiday = cal.session_clock(utc(1_720_108_800));
    assert_eq!(holiday.since_open_ns, 18 * HOUR_NS);
    assert_eq!(holiday.to_close_ns, HOUR_NS);
    assert_eq!(holiday.session, SessionId::Regular);

    let ordinary = cal.session_clock(utc(1_719_504_000));
    assert_eq!(ordinary.since_open_ns, 18 * HOUR_NS);
    assert_eq!(ordinary.to_close_ns, 5 * HOUR_NS);
    assert_eq!(ordinary.session, SessionId::Regular);
}

/// The regular-hours distances are measured against the **scheduled** window,
/// so they stay positive past an early close.
///
/// Deliberate, and the pair of readings says so: on 2024-07-04 the exchange has
/// been shut for an hour by 13:00 CDT (18:00Z, 1_720_116_000) — `to_close_ns`
/// is −1 h and `session` is `Closed` — while `to_rth_close_ns` still reads
/// 2¼ h, because 15:15 CT is when the regular session was *scheduled* to end.
/// A rule that wants "the market is open" asks `session`; one that wants "how
/// far into the trading day are we" asks the RTH clock. Collapsing them would
/// make one of the two questions unaskable.
#[test]
fn regular_hours_are_scheduled_hours_not_actual_ones() {
    let cal = shaped();
    let after_the_early_close = cal.session_clock(utc(1_720_116_000));
    assert_eq!(after_the_early_close.to_close_ns, -HOUR_NS);
    assert_eq!(after_the_early_close.session, SessionId::Closed);
    // 13:00 CDT to the scheduled 15:15 CDT close is 2 h 15 min.
    assert_eq!(
        after_the_early_close.to_rth_close_ns,
        2 * HOUR_NS + 15 * MINUTE_NS
    );
}

/// `session_open` and `session_close` are the same two instants
/// `open_intervals` is built from, so a bucket grid and a coverage check cannot
/// disagree about when a session ran.
#[test]
fn the_session_endpoints_are_the_ones_open_intervals_uses() {
    for cal in [plain(), shaped()] {
        for day in 1..=8 {
            let date = d(2024, 1, day);
            let intervals = cal.open_intervals(date);
            if intervals.is_empty() {
                continue;
            }
            assert_eq!(intervals[0].0, cal.session_open(date), "{date}");
            assert_eq!(
                intervals[intervals.len() - 1].1,
                cal.session_close(date),
                "{date}"
            );
        }
    }
}

/// The halt refusal `resample` makes rests on this, so it has to be true of the
/// fixtures and of the bundled tables.
#[test]
fn halts_are_declared_where_they_exist_and_nowhere_else() {
    assert!(!plain().declares_halts());
    assert!(shaped().declares_halts());
}

/// The tripwire this replaced fired on the D-0077/D-0086 merge, exactly as its
/// author intended: it asserted no bundled calendar declares a halt, and said in
/// its own message that if one ever did, "D-0077's per-day bucket anchor needs
/// revisiting". Era 3a of CME equity index declares one, so it did.
///
/// The revisit is done, and this is the sharper invariant that replaces it: the
/// question a bucket grid has is per **trading day**, not per calendar, because
/// the grid is anchored once a day. A calendar-wide answer would refuse every
/// modern ES bar for a halt that ended on 2021-06-28.
#[test]
fn the_halt_gate_is_asked_per_trading_day_not_per_calendar() {
    let cal = Calendar::by_id("cme_globex_equity_index").expect("bundled table loads");

    // Calendar-wide: yes, some era of it halts. That is the coarse answer now.
    assert!(
        cal.declares_halts(),
        "era 3a of equity index halts 15:15-15:30 CT (D-0086)"
    );

    // Era 3a — the halt is real, and a bucket grid must refuse those days.
    assert!(cal.declares_halts_on(d(2019, 3, 14)));
    assert!(cal.declares_halts_on(d(2021, 6, 25)));

    // Era 3b — CME's SER-8788R removed it effective 2021-06-28, so from that
    // day a bucket grid is safe and `resample` proceeds.
    assert!(!cal.declares_halts_on(d(2021, 6, 28)));
    assert!(!cal.declares_halts_on(d(2024, 1, 3)));

    // And the other bundled tables declare none at all, on any date.
    for id in Calendar::bundled_ids().expect("bundled tables parse") {
        if id == "cme_globex_equity_index" {
            continue;
        }
        let other = Calendar::by_id(&id).expect("bundled table loads");
        assert!(
            !other.declares_halts(),
            "{id} grew a halt; the per-day gate covers it, but its era boundary \
             needs a fixture here the way equity index has one"
        );
    }
}
