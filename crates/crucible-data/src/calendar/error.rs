//! Failure modes of the session calendar.
//!
//! Hand-rolled per CLAUDE.md §5.1, in the same shape as
//! [`CuratedError`](crate::curated::CuratedError): struct variants carrying
//! what a caller needs to act, a manual `Display` naming the consequence and
//! the remedy, and `source()` for wrapped causes.
//!
//! Every variant here is a **load-time** failure. Once a [`Calendar`] exists,
//! answering it cannot fail: `is_open`, `session_of`, `trading_day` and
//! `bars_per_year` are total functions. That is deliberate — a calendar is
//! consulted from inside replay and annualization, where there is no sensible
//! place to put a `Result`, so every uncertainty is resolved while the table is
//! being parsed and validated.
//!
//! [`Calendar`]: crate::calendar::Calendar

use std::fmt;

/// Why a calendar table could not be loaded.
#[derive(Debug)]
pub enum CalendarError {
    /// The table is not valid TOML, or does not match the expected schema.
    ///
    /// For the bundled tables this is a build-breaking bug rather than an
    /// operator error, and a unit test parses every bundled table so it is
    /// caught in CI rather than in a research run.
    Unparseable {
        /// Which table failed.
        table: &'static str,
        /// What the TOML layer said.
        detail: String,
    },
    /// The table parsed but declares a schema version this build cannot read.
    UnknownSchemaVersion {
        /// Which table failed.
        table: &'static str,
        /// Version recorded in the table.
        found: u32,
        /// Version this build understands.
        expected: u32,
    },
    /// The table parsed but describes something impossible.
    Invalid {
        /// Which table failed.
        table: &'static str,
        /// Calendar id within the table, or `"<file>"` for a whole-file rule.
        calendar: String,
        /// What is wrong, phrased as the rule that was broken.
        reason: String,
    },
    /// A local clock time is not `HH:MM` or `HH:MM:SS`.
    MalformedLocalTime {
        /// Calendar id the time belongs to.
        calendar: String,
        /// Which field carried it.
        field: &'static str,
        /// The text as written.
        value: String,
    },
    /// A local clock time falls inside the daylight-saving transition window,
    /// where it names either two instants or none.
    ///
    /// No exchange schedules a session boundary at 2 a.m. local, so this is a
    /// typo in the table rather than a situation to model. Rejecting it here is
    /// what lets the runtime conversion be infallible.
    AmbiguousLocalTime {
        /// Calendar id the time belongs to.
        calendar: String,
        /// Which field carried it.
        field: &'static str,
        /// The text as written.
        value: String,
    },
    /// The IANA timezone named by a calendar is not in the bundled database.
    UnknownTimezone {
        /// Calendar id that named it.
        calendar: String,
        /// The name as written.
        timezone: String,
    },
    /// No bundled calendar has the requested id.
    UnknownCalendar {
        /// The id that was asked for.
        requested: String,
        /// Every id that does exist, sorted.
        known: Vec<String>,
    },
}

impl fmt::Display for CalendarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalendarError::Unparseable { table, detail } => write!(
                f,
                "calendar table {table} could not be parsed: {detail}. It is compiled \
                 into this binary, so this is a build bug, not a configuration error"
            ),
            CalendarError::UnknownSchemaVersion {
                table,
                found,
                expected,
            } => write!(
                f,
                "calendar table {table} declares schema_version {found}; this build \
                 reads v{expected}"
            ),
            CalendarError::Invalid {
                table,
                calendar,
                reason,
            } => write!(f, "calendar {calendar} in {table} is unusable: {reason}"),
            CalendarError::MalformedLocalTime {
                calendar,
                field,
                value,
            } => write!(
                f,
                "calendar {calendar}: {field} = {value:?} is not a local clock time \
                 (expected HH:MM or HH:MM:SS)"
            ),
            CalendarError::AmbiguousLocalTime {
                calendar,
                field,
                value,
            } => write!(
                f,
                "calendar {calendar}: {field} = {value:?} falls in the daylight-saving \
                 transition window, where a local time names two instants or none. \
                 Session boundaries are never scheduled there; fix the table"
            ),
            CalendarError::UnknownTimezone { calendar, timezone } => write!(
                f,
                "calendar {calendar} names timezone {timezone:?}, which is not in the \
                 bundled IANA database"
            ),
            CalendarError::UnknownCalendar { requested, known } => write!(
                f,
                "no bundled calendar is called {requested:?}; this build carries: {}",
                known.join(", ")
            ),
        }
    }
}

impl std::error::Error for CalendarError {}
