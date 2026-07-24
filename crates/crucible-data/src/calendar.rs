//! Session calendars — **M1 spec, not yet implemented.**
//!
//! CME Globex trades ~23h/day with a daily maintenance break, holiday
//! schedules, and early closes. Sessions matter for: warmup boundaries,
//! "daily" aggregation (a Globex trading day spans two UTC dates), RTH-vs-ETH
//! strategy filters, and `bars_per_year` used in annualization.
//!
//! ## Plan
//! - Static tables (per exchange): weekly session template + holiday/early-
//!   close overrides, checked into the repo as data files with sources cited.
//!   No network calls at runtime.
//! - API sketch: `session_of(ts) -> SessionId`, `is_open(ts) -> bool`,
//!   `trading_day(ts) -> Date` (the exchange's date convention, not UTC's),
//!   `bars_per_year(tf, exchange) -> f64`.
//! - All inputs/outputs UTC nanoseconds; the *only* timezone-aware code in
//!   the workspace lives here (chrono/chrono-tz allowed here when built).
//! - Macro-event calendars (FOMC/CPI/NFP release timestamps as a static CSV)
//!   also enter through this module — they are availability-timestamped
//!   events like everything else, NOT scraped feeds (v1 descope decision,
//!   docs/DECISIONS.md D-0010).

/// Placeholder for the M1 calendar implementation.
#[derive(Debug, Clone, Copy)]
pub struct CalendarPlan;
