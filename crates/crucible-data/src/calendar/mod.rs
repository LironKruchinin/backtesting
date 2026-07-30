//! Session calendars — which instants an exchange is actually trading.
//!
//! CME Globex trades close to 23 hours a day with a daily maintenance break, a
//! holiday schedule, and early closes. Sessions matter for warmup boundaries,
//! "daily" aggregation (a Globex trading day spans two UTC dates), RTH-vs-ETH
//! strategy filters, and the `bars_per_year` factor used in annualization.
//!
//! ## The shape of a trading day
//!
//! Trading day `D` runs from `open_local` on `D − 1` to `close_local` on `D`,
//! minus any intraday halts. For CME equity index that is 17:00 CT the evening
//! before until 16:00 CT, so **the session that opens on Sunday evening is
//! Monday's trade date** — the exchange's date convention, not UTC's. The gap
//! between one day's close and the next day's open is the maintenance break;
//! it is not modelled separately because it is exactly that gap.
//!
//! A `Closed` holiday removes trading day `D` entirely, which also removes the
//! evening of `D − 1` — that evening exists only to open `D`'s session. The
//! evening of a closed day still opens if `D + 1` is a trading day, which is
//! why Globex reopens at 17:00 on Christmas Day for the 26th.
//!
//! ## Timezone handling, and why it is contained here
//!
//! Exchanges publish hours in local time, so the session boundaries move in
//! UTC twice a year. This is the only module in the workspace that knows what
//! a timezone is (CLAUDE.md §4); `chrono` and `chrono-tz` appear nowhere else.
//! Everything crossing this module's boundary is UTC nanoseconds ([`Ts`]) or a
//! [`CivilDate`].
//!
//! A calendar that could read the wall clock could make the same config produce
//! two different answers, so §2.2 bans it here. `chrono` is requested without
//! its `clock` feature to say so, but `parquet` enables that feature through
//! Cargo's feature unification and `Utc::now()` compiles anyway — the binding
//! rule is the `disallowed-methods` entry in `clippy.toml`, which fails the
//! build (D-0048).
//!
//! One convenient accident makes the daylight-saving problem small: US
//! transitions happen at 02:00 local on a Sunday, and Globex is shut from
//! Friday afternoon to Sunday evening, so **a transition never falls inside a
//! session**. Session lengths in local wall-clock therefore equal real elapsed
//! time, and [`Calendar::bars_per_year`] can count local seconds. The table
//! loader refuses any local time between 01:00 and 03:00 so that this stays
//! true by construction rather than by luck.
//!
//! ## Answering cannot fail
//!
//! Every uncertainty is resolved while the table is parsed. Once a [`Calendar`]
//! exists, [`Calendar::is_open`], [`Calendar::session_of`],
//! [`Calendar::trading_day`] and [`Calendar::bars_per_year`] are total — there
//! is nowhere sensible to put a `Result` inside replay or annualization.
//!
//! ## Still to come
//!
//! Macro-event calendars (FOMC/CPI/NFP release timestamps as a static CSV)
//! enter through this module too — they are availability-timestamped events
//! like everything else, not scraped feeds (D-0010). That is M4 work; nothing
//! here anticipates it beyond leaving the door open.

mod eastern;
mod error;
mod table;

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{Datelike, TimeZone, Timelike};
use crucible_core::types::{InstrumentId, TimeFrame, Ts};

pub use eastern::{EasternTimeError, eastern_wall_clock_to_ts};
pub use error::CalendarError;

use table::{
    CalendarSpec, CalendarTable, Effect, LocalTime, SessionShape, TABLE_SCHEMA_VERSION, add_days,
    is_weekday,
};

/// Re-exported so callers need not reach into `ingest` for the date type the
/// calendar speaks. There is one civil-date type in this crate, by design
/// (CLAUDE.md §4 — no synonyms).
pub use crate::ingest::window::CivilDate;
use crate::ingest::window::days_from_civil;

/// Days in a mean Gregorian year. The same constant `backtest` annualizes with.
const DAYS_PER_YEAR: f64 = 365.2425;

/// The bundled calendar tables, compiled in rather than read from disk.
const BUNDLED: &[(&str, &str)] = &[
    ("cme_globex.toml", include_str!("tables/cme_globex.toml")),
    (
        "us_equity_options.toml",
        include_str!("tables/us_equity_options.toml"),
    ),
];

/// Which named session an instant falls in.
///
/// The distinction exists because "trade only in RTH" and "trade the overnight
/// session" are ordinary strategy filters, and expressing them by hardcoded
/// clock arithmetic in a strategy is how timezone bugs get into research.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionId {
    /// Electronic trading between the session open and the regular open —
    /// the overnight session. Spans two calendar dates.
    Overnight,
    /// The exchange's regular trading hours.
    Regular,
    /// Electronic trading between the regular close and the session close.
    PostRegular,
    /// Not trading: weekend, maintenance break, holiday, or after an early
    /// close.
    Closed,
}

impl SessionId {
    /// True for every variant except [`SessionId::Closed`].
    #[must_use]
    pub const fn is_open(self) -> bool {
        !matches!(self, SessionId::Closed)
    }
}

impl core::fmt::Display for SessionId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            SessionId::Overnight => "overnight",
            SessionId::Regular => "regular",
            SessionId::PostRegular => "post-regular",
            SessionId::Closed => "closed",
        };
        write!(f, "{s}")
    }
}

/// Where a completed bar sits inside its trading session.
///
/// The four distances are the raw material of every "first half-hour", "last
/// hour", "RTH only" and "overnight only" filter, and they exist here — in the
/// one module that knows what a timezone is — so that no strategy ever computes
/// them from a hardcoded UTC offset. That is how timezone bugs get into
/// research: the arithmetic looks right for six months of the year.
///
/// Nanoseconds rather than minutes, and integers rather than `f64`, because a
/// difference of two timestamps is exact and §2.3 puts the conversion into
/// indicator space at the boundary the consumer chooses. `crucible-cli` divides
/// by 60e9 on its way into the rule grammar, which is where `f64` belongs.
///
/// Signs are meaningful and not clamped. `since_rth_open_ns` is **negative**
/// through the overnight session, which is exactly what makes
/// `minutes_since_rth_open > 0 and minutes_since_rth_open <= 30` say "the first
/// half hour of the regular session" and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionClock {
    /// From the trading day's session open to the bar's availability.
    pub since_open_ns: i64,
    /// From the bar's availability to the trading day's session close,
    /// **honouring an early close** — the number is smaller on a holiday eve,
    /// which is the point of asking a calendar instead of a clock.
    pub to_close_ns: i64,
    /// From the scheduled regular-hours open to the bar's availability.
    /// Negative before it.
    pub since_rth_open_ns: i64,
    /// From the bar's availability to the scheduled regular-hours close.
    /// Negative after it.
    pub to_rth_close_ns: i64,
    /// Which named session the bar's **interval** closed inside.
    pub session: SessionId,
}

/// A resolved recurring holiday rule.
#[derive(Debug, Clone)]
struct Holiday {
    name: String,
    rule: table::HolidayRule,
    effect: Effect,
    first_year: Option<i64>,
    last_year: Option<i64>,
    source: String,
}

/// What a calendar does to one trading day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DayEffect {
    /// A full session.
    Normal,
    /// No session at all.
    Closed {
        /// Why, for reporting.
        name: String,
    },
    /// A session ending before the usual close.
    EarlyClose {
        /// Why, for reporting.
        name: String,
        /// Local close time, as written in the table.
        close_local: String,
    },
}

/// A calendar narrowed to the questions it can answer about **dates**.
///
/// Exists because "which dates had a session" and "was the exchange open at
/// this instant" have different answers for different roots off the same
/// holiday set. US index options keep the cash-equity holidays and run
/// different hours, so their dates are knowable here and their instants are
/// not. Handing out a full [`Calendar`] for them would make `is_open`,
/// `session_of` and `bars_per_year` reachable and wrong; this type simply does
/// not have them (D-0042's device, applied to a second kind of value).
#[derive(Debug, Clone)]
pub struct TradingDayCalendar {
    inner: Calendar,
}

impl TradingDayCalendar {
    /// Stable identifier of the underlying calendar.
    #[must_use]
    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// True when `date` had a session.
    #[must_use]
    pub fn is_trading_day(&self, date: CivilDate) -> bool {
        self.inner.is_trading_day(date)
    }

    /// What the calendar does to `date` — closure, early close, or normal.
    ///
    /// Early closes are reported for completeness; for a day-level root the
    /// *time* is not this table's to state, only the fact that the day was
    /// short.
    #[must_use]
    pub fn day_effect(&self, date: CivilDate) -> DayEffect {
        self.inner.day_effect(date)
    }

    /// First date this calendar claims to describe.
    #[must_use]
    pub fn valid_from(&self) -> CivilDate {
        self.inner.valid_from()
    }

    /// Every source the underlying calendar rests on.
    #[must_use]
    pub fn sources(&self) -> &[String] {
        self.inner.sources()
    }
}

/// One exchange's session calendar.
///
/// Construct with [`Calendar::by_id`] or [`Calendar::for_instrument`]. Parsing
/// is cheap and happens per call — there is no global cache, deliberately:
/// shared mutable state is exactly what §2.2 does not want between a config
/// and a number.
#[derive(Debug, Clone)]
pub struct Calendar {
    id: String,
    description: String,
    tz: chrono_tz::Tz,
    roots: Vec<String>,
    day_level_roots: Vec<String>,
    shape: SessionShape,
    open_local: LocalTime,
    close_local: LocalTime,
    halts_local: Vec<(LocalTime, LocalTime)>,
    rth_open_local: LocalTime,
    rth_close_local: LocalTime,
    valid_from: CivilDate,
    reference_start: CivilDate,
    reference_end: CivilDate,
    reference_rationale: String,
    holidays: Vec<Holiday>,
    one_offs: BTreeMap<(i64, u32, u32), (String, Effect)>,
    sources: Vec<String>,
    /// Precomputed so `bars_per_year` is O(1) and a table that cannot be
    /// walked fails at load rather than mid-backtest.
    open_seconds_in_span: i64,
    trading_days_in_span: i64,
}

impl Calendar {
    /// Every calendar id this build carries, sorted.
    ///
    /// # Errors
    /// [`CalendarError`] if a bundled table does not parse — a build bug.
    pub fn bundled_ids() -> Result<Vec<String>, CalendarError> {
        let mut ids: Vec<String> = Self::all()?.into_iter().map(|c| c.id).collect();
        ids.sort();
        Ok(ids)
    }

    /// Loads a bundled calendar by id.
    ///
    /// # Errors
    /// [`CalendarError::UnknownCalendar`] if nothing carries that id, or a
    /// parse/validation failure in a bundled table.
    pub fn by_id(id: &str) -> Result<Calendar, CalendarError> {
        let all = Self::all()?;
        let mut known: Vec<String> = all.iter().map(|c| c.id.clone()).collect();
        known.sort();
        all.into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| CalendarError::UnknownCalendar {
                requested: id.to_owned(),
                known,
            })
    }

    /// The calendar governing `instrument`, matched on its root symbol.
    ///
    /// Returns `None` for anything no bundled table claims — synthetic
    /// instruments, equities, an unlisted root. Callers are expected to fall
    /// back rather than guess: a wrong calendar is worse than no calendar.
    ///
    /// # Errors
    /// [`CalendarError`] if a bundled table does not parse — a build bug.
    pub fn for_instrument(instrument: &InstrumentId) -> Result<Option<Calendar>, CalendarError> {
        Ok(Self::all()?
            .into_iter()
            .find(|c| c.governs(instrument.as_str())))
    }

    /// Stable identifier of this calendar.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// One line on what this calendar governs.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Every source this calendar rests on, deduplicated and sorted: the
    /// calendar-level citations, the session template's, and one per holiday
    /// and dated exception.
    ///
    /// A calendar is a set of claims about a real exchange, and §2.5's rule
    /// that an unreproducible number is a rumour applies to them too. This is
    /// what makes those claims checkable without reading the table.
    #[must_use]
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    /// First date this template actually describes the exchange.
    ///
    /// Sessions get restructured; CME moved the equity-index close twice inside
    /// this archive's window. Asking about an earlier instant still returns an
    /// answer — the functions here are total — but it is this era's answer to a
    /// different era's question. Callers replaying or inspecting a span that
    /// starts earlier should say so; `crucible qa` and `crucible backtest` do.
    #[must_use]
    pub fn valid_from(&self) -> CivilDate {
        self.valid_from
    }

    /// Why [`Calendar::reference_span`] is the span it is.
    #[must_use]
    pub fn reference_rationale(&self) -> &str {
        &self.reference_rationale
    }

    /// The recurring holiday rules, as `(name, source)`, in table order.
    #[must_use]
    pub fn holiday_citations(&self) -> Vec<(&str, &str)> {
        self.holidays
            .iter()
            .map(|h| (h.name.as_str(), h.source.as_str()))
            .collect()
    }

    /// Root symbols this calendar claims.
    #[must_use]
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// True when this calendar claims `instrument`.
    ///
    /// Accepts the bare root (`ES`), an outright contract (`ESH4`, `ESH24`),
    /// a parent key (`ES.FUT`), and a vendor continuous alias (`ES.v.0`).
    /// Deliberately strict about the contract suffix: matching on a bare prefix
    /// would let `ESG` (a different product) inherit `ES`'s session.
    #[must_use]
    pub fn governs(&self, instrument: &str) -> bool {
        Self::matches_any(&self.roots, instrument)
    }

    /// True when this calendar governs the instrument's **trading days**,
    /// whether or not it describes its hours.
    ///
    /// A superset of [`Calendar::governs`]: every root whose hours are
    /// described also has its dates described, but not the reverse.
    #[must_use]
    pub fn governs_days(&self, instrument: &str) -> bool {
        self.governs(instrument) || Self::matches_any(&self.day_level_roots, instrument)
    }

    /// Whether any root in `roots` is the instrument's product root.
    ///
    /// The root is taken from [`parse_parts`], the one place a contract symbol
    /// is split into root + month + year (D-0072). This module used to carry
    /// its own rule — "a month code and one or two year digits" — and a third
    /// spelling of the same idea is a third thing to keep in step: when curated
    /// keys grew a four-digit year, that rule silently stopped recognising
    /// `ESH2024` as an ES contract and the CME calendar quietly stopped
    /// governing it, which changes the annualization factor and therefore every
    /// Sharpe (D-0039). One parser, one answer.
    ///
    /// Names that are not contracts at all (`ES.FUT`, `SPY`, `VIX`, a
    /// continuous alias) match a root only by equality, which is what they mean.
    fn matches_any(roots: &[String], instrument: &str) -> bool {
        let candidate = strip_series_suffix(instrument);
        let root_of = crate::continuous::parse_parts(candidate)
            .map(|parts| parts.root)
            .unwrap_or(candidate);
        roots.iter().any(|root| root_of == root)
    }

    /// Narrows this calendar to the questions it can answer about **dates**.
    ///
    /// The returned view exposes no `is_open`, no `session_of` and no
    /// `bars_per_year`, because for a day-level root those answers would be
    /// confidently wrong. Same device as [`AdjustedPrice`] (D-0042): the way
    /// to stop a value being used where it does not belong is to make the call
    /// not exist, not to write a comment asking people not to make it.
    ///
    /// [`AdjustedPrice`]: crate::continuous::AdjustedPrice
    #[must_use]
    pub fn into_trading_days(self) -> TradingDayCalendar {
        TradingDayCalendar { inner: self }
    }

    /// The day-level calendar governing an instrument, if one does.
    ///
    /// Answers for index-option roots that [`Calendar::for_instrument`]
    /// declines, which is the point: a coverage check needs the trading-day
    /// set for all nine ThetaData roots, and only four of them have hours this
    /// project can state (D-0058).
    ///
    /// # Errors
    /// [`CalendarError`] if a bundled table does not parse.
    pub fn trading_days_for(
        instrument: &InstrumentId,
    ) -> Result<Option<TradingDayCalendar>, CalendarError> {
        Ok(Self::all()?
            .into_iter()
            .find(|c| c.governs_days(instrument.as_str()))
            .map(Calendar::into_trading_days))
    }

    /// The trading day the exchange attributes `ts` to.
    ///
    /// Total by construction, and **not** necessarily a trading day: Saturday
    /// and holidays have an attribution too. Ask [`Calendar::is_trading_day`]
    /// before treating the answer as a session.
    #[must_use]
    pub fn trading_day(&self, ts: Ts) -> CivilDate {
        let local = self.local_of(ts);
        let date = CivilDate {
            year: i64::from(local.year()),
            month: local.month(),
            day: local.day(),
        };
        let seconds = i64::from(local.hour()) * 3600
            + i64::from(local.minute()) * 60
            + i64::from(local.second());
        // Only an overnight market attributes the evening to tomorrow. A
        // same-day market's trade date is simply the calendar date — rolling
        // it forward at 09:30 would label an entire session with the next
        // day's date.
        if self.shape == SessionShape::Overnight && seconds >= self.open_local.seconds_of_day() {
            add_days(date, 1)
        } else {
            date
        }
    }

    /// True when `date` has a session at all.
    #[must_use]
    pub fn is_trading_day(&self, date: CivilDate) -> bool {
        is_weekday(date) && !matches!(self.day_effect(date), DayEffect::Closed { .. })
    }

    /// What this calendar does to `date`.
    #[must_use]
    pub fn day_effect(&self, date: CivilDate) -> DayEffect {
        if let Some((name, effect)) = self.one_offs.get(&(date.year, date.month, date.day)) {
            return Self::render_effect(name, effect);
        }
        // A rule anchored in one year can land in its neighbour: New Year's
        // Day 2022 fell on a Saturday and was observed on Friday 2021-12-31.
        for year in [date.year - 1, date.year, date.year + 1] {
            for holiday in &self.holidays {
                if holiday.first_year.is_some_and(|first| year < first)
                    || holiday.last_year.is_some_and(|last| year > last)
                {
                    continue;
                }
                if holiday.rule.date_in_year(year) == Some(date) {
                    return Self::render_effect(&holiday.name, &holiday.effect);
                }
            }
        }
        DayEffect::Normal
    }

    fn render_effect(name: &str, effect: &Effect) -> DayEffect {
        match effect {
            Effect::Closed => DayEffect::Closed {
                name: name.to_owned(),
            },
            Effect::EarlyClose { close_local } => DayEffect::EarlyClose {
                name: name.to_owned(),
                close_local: close_local.clone(),
            },
        }
    }

    /// The open intervals of trading day `date`, as `[start, end)` in UTC
    /// nanoseconds, in order. Empty when `date` is not a trading day.
    ///
    /// This is the primitive everything else is built on, and the one the
    /// data-QA report uses to decide which missing bars are holes and which
    /// are the market being shut.
    #[must_use]
    pub fn open_intervals(&self, date: CivilDate) -> Vec<(Ts, Ts)> {
        if !self.is_trading_day(date) {
            return Vec::new();
        }
        // The same two instants [`Calendar::session_open`] and
        // [`Calendar::session_close`] answer with, so a bucket grid anchored on
        // the open can never disagree with the intervals a coverage check walks
        // (D-0077 — one definition, per D-0071's argument).
        let start = self.session_open(date);
        let end = self.session_close(date);
        if end <= start {
            return Vec::new();
        }

        let mut intervals = Vec::with_capacity(self.halts_local.len() + 1);
        let mut cursor = start;
        for (from, to) in &self.halts_local {
            let halt_from = self.instant(date, *from);
            let halt_to = self.instant(date, *to);
            if halt_to <= cursor || halt_from >= end {
                continue;
            }
            if halt_from > cursor {
                intervals.push((cursor, halt_from));
            }
            cursor = halt_to.max(cursor);
        }
        if cursor < end {
            intervals.push((cursor, end));
        }
        intervals
    }

    /// The instant trading day `date`'s session **opens**, per the session
    /// template.
    ///
    /// Total, and deliberately independent of [`Calendar::day_effect`]: an
    /// early close moves the close, never the open, and a *closed* day still
    /// has a well-defined template open even though nothing trades in it.
    /// [`Calendar::open_intervals`] is the question "when was this exchange
    /// actually trading"; this is the question "where does this trading day's
    /// clock start", which is what a session-anchored bucket grid needs
    /// (D-0077) and what a "minutes since the open" reading is measured from.
    ///
    /// Answering for a non-trading day is not a bug for the same reason
    /// [`Calendar::trading_day`] answers for a Saturday: the function is total
    /// because there is nowhere sensible to put a `Result` inside replay. Ask
    /// [`Calendar::is_trading_day`] if the distinction matters.
    #[must_use]
    pub fn session_open(&self, date: CivilDate) -> Ts {
        let open_date = match self.shape {
            SessionShape::Overnight => add_days(date, -1),
            SessionShape::SameDay => date,
        };
        self.instant(open_date, self.open_local)
    }

    /// The instant trading day `date`'s session **closes**, honouring an early
    /// close.
    ///
    /// Total, like [`Calendar::session_open`], and the counterpart to it: the
    /// pair bracket one trading day's clock. A closed day still answers, with
    /// the template's close.
    #[must_use]
    pub fn session_close(&self, date: CivilDate) -> Ts {
        let close = match self.day_effect(date) {
            // Validated at load, so this cannot fail; parsing again is cheaper
            // than carrying a second representation around.
            DayEffect::EarlyClose { close_local, .. } => {
                LocalTime::parse(&close_local, &self.id, "close_local").unwrap_or(self.close_local)
            }
            _ => self.close_local,
        };
        self.instant(date, close)
    }

    /// The regular-hours window of trading day `date`, as `[open, close)`.
    ///
    /// Both instants come from the session template, so they are the *scheduled*
    /// regular hours. On an early-close day the real close can land inside this
    /// window — CME's 12:00 CT holiday close is before the 15:00 CT regular
    /// close — and that is exactly why the two are separate questions:
    /// "how far into the regular session are we" is a clock reading, and
    /// "was the exchange still open" is [`Calendar::session_close`].
    #[must_use]
    pub fn regular_hours(&self, date: CivilDate) -> (Ts, Ts) {
        (
            self.instant(date, self.rth_open_local),
            self.instant(date, self.rth_close_local),
        )
    }

    /// Where a bar that became available at `avail_ts` sits in its session.
    ///
    /// Total, like everything else here. Two instants are involved and the
    /// difference is deliberate:
    ///
    /// - the **distances** are measured from `avail_ts`, the instant the bar's
    ///   information exists and a decision on it is taken (§2.1);
    /// - the **session** is asked at `avail_ts − 1ns`, the last instant the
    ///   bar's interval covers, because open intervals are half-open and a bar
    ///   ending exactly at 16:00 CT traded entirely inside the session it just
    ///   closed. Asking at `avail_ts` would report the final regular-hours bar
    ///   of every day as [`SessionId::Closed`], and "exit on the last bar of
    ///   RTH" would be a rule that never fires.
    ///
    /// The **trading day** is `trading_day(avail_ts)` — D-0062's expression,
    /// spelled the way D-0071 requires so that this clock and a fold boundary
    /// can never attribute one bar to two different dates. The two instants can
    /// only name different sessions for a bar whose own interval straddles the
    /// session open, which no interval-aligned bar has and which
    /// [`resample`](crate::curated::resample) refuses outright.
    #[must_use]
    pub fn session_clock(&self, avail_ts: Ts) -> SessionClock {
        let last_instant = Ts(avail_ts.0 - 1);
        let day = self.trading_day(avail_ts);
        let (rth_open, rth_close) = self.regular_hours(day);
        SessionClock {
            since_open_ns: avail_ts.0 - self.session_open(day).0,
            to_close_ns: self.session_close(day).0 - avail_ts.0,
            since_rth_open_ns: avail_ts.0 - rth_open.0,
            to_rth_close_ns: rth_close.0 - avail_ts.0,
            session: self.session_of(last_instant),
        }
    }

    /// True when this calendar declares an intraday halt.
    ///
    /// Neither bundled table does (D-0040 removed the 15:15–15:30 CT one the
    /// CME contract-spec page claims, because our own archive falsifies it for
    /// the current era). Exposed because a halt is a session boundary, and code
    /// that anchors a bucket grid on the session open would silently let a
    /// bucket span one — so [`resample`](crate::curated::resample) refuses a
    /// calendar that declares halts rather than producing a bar whose
    /// constituents sit either side of a break (D-0077).
    #[must_use]
    pub fn declares_halts(&self) -> bool {
        !self.halts_local.is_empty()
    }

    /// Which session `ts` falls in.
    #[must_use]
    pub fn session_of(&self, ts: Ts) -> SessionId {
        let day = self.trading_day(ts);
        if !self
            .open_intervals(day)
            .iter()
            .any(|(start, end)| ts >= *start && ts < *end)
        {
            return SessionId::Closed;
        }
        let rth_open = self.instant(day, self.rth_open_local);
        let rth_close = self.instant(day, self.rth_close_local);
        if ts < rth_open {
            SessionId::Overnight
        } else if ts < rth_close {
            SessionId::Regular
        } else {
            SessionId::PostRegular
        }
    }

    /// True when the exchange is trading at `ts`.
    #[must_use]
    pub fn is_open(&self, ts: Ts) -> bool {
        self.session_of(ts).is_open()
    }

    /// Trading days per year, averaged over this calendar's reference span.
    #[must_use]
    pub fn trading_days_per_year(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an annualization factor is statistics space (§2.3), not accounting"
        )]
        let days = self.trading_days_in_span as f64;
        days / self.reference_years()
    }

    /// Seconds the exchange is open per year, averaged over the reference span.
    #[must_use]
    pub fn open_seconds_per_year(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an annualization factor is statistics space (§2.3), not accounting"
        )]
        let seconds = self.open_seconds_in_span as f64;
        seconds / self.reference_years()
    }

    /// Bar intervals of length `tf` in one year of this calendar's sessions.
    ///
    /// The `exchange` argument in the original spec sketch is this receiver:
    /// a calendar *is* an exchange session template, so the pair travels
    /// together and cannot be mismatched.
    ///
    /// Daily bars are counted as **trading days**, not as open seconds divided
    /// by 86 400 — a daily bar exists once per session however long the session
    /// happens to be.
    ///
    /// ## What this number is, and is not
    ///
    /// It answers "how many `tf` intervals does a year of sessions contain",
    /// which is the right denominator for turning a per-bar statistic into an
    /// annual one. It is **not** a count of bars that will exist in a file:
    /// `ohlcv` data has no bar for an interval that did not trade, so a thin
    /// contract yields fewer. For liquid front-month index futures on `1m` the
    /// two are within a percent; on `1s`, or on a back month, they are not.
    /// `crucible backtest` prints both when they disagree (D-0039).
    #[must_use]
    pub fn bars_per_year(&self, tf: TimeFrame) -> f64 {
        if tf == TimeFrame::D1 {
            return self.trading_days_per_year();
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "an annualization factor is statistics space (§2.3), not accounting"
        )]
        let tf_seconds = tf.duration_ns() as f64 / 1e9;
        self.open_seconds_per_year() / tf_seconds
    }

    /// The span `bars_per_year` averages over, as `[start, end)`.
    #[must_use]
    pub fn reference_span(&self) -> (CivilDate, CivilDate) {
        (self.reference_start, self.reference_end)
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "an annualization factor is statistics space (§2.3), not accounting"
    )]
    fn reference_years(&self) -> f64 {
        let days = days_from_civil(self.reference_end) - days_from_civil(self.reference_start);
        days as f64 / DAYS_PER_YEAR
    }

    /// UTC instant of a local wall-clock time on a civil date.
    ///
    /// Infallible because the loader rejects local times in the daylight-saving
    /// transition window and refuses dates chrono cannot represent.
    fn instant(&self, date: CivilDate, time: LocalTime) -> Ts {
        let naive = naive_datetime(date, time)
            .expect("INVARIANT: the loader validated every date and time in this table");
        let local =
            self.tz.from_local_datetime(&naive).single().expect(
                "INVARIANT: the loader rejects local times inside the DST transition window",
            );
        Ts(local
            .timestamp_nanos_opt()
            .expect("INVARIANT: the loader bounds dates to the representable nanosecond range"))
    }

    fn local_of(&self, ts: Ts) -> chrono::DateTime<chrono_tz::Tz> {
        chrono::DateTime::from_timestamp_nanos(ts.0).with_timezone(&self.tz)
    }

    /// Parses every bundled table, in table order.
    fn all() -> Result<Vec<Calendar>, CalendarError> {
        let mut out = Vec::new();
        for (name, text) in BUNDLED {
            out.extend(Self::parse_table(name, text)?);
        }
        Ok(out)
    }

    /// Parses one table's text. Shared by [`Calendar::all`] and by the tests,
    /// which drive the whole engine from hand-written fixture tables so that
    /// the behaviour under test does not move when a holiday rule is corrected.
    pub(crate) fn parse_table(
        name: &'static str,
        text: &str,
    ) -> Result<Vec<Calendar>, CalendarError> {
        let parsed: CalendarTable =
            toml::from_str(text).map_err(|e| CalendarError::Unparseable {
                table: name,
                detail: e.to_string(),
            })?;
        if parsed.schema_version != TABLE_SCHEMA_VERSION {
            return Err(CalendarError::UnknownSchemaVersion {
                table: name,
                found: parsed.schema_version,
                expected: TABLE_SCHEMA_VERSION,
            });
        }
        parsed
            .calendar
            .into_iter()
            .map(|spec| Calendar::from_spec(name, spec))
            .collect()
    }

    /// Validates a parsed spec into a usable calendar.
    fn from_spec(table: &'static str, spec: CalendarSpec) -> Result<Calendar, CalendarError> {
        // Owned, so the closure does not hold a borrow of `spec` across the
        // moves below.
        let id = spec.id.clone();
        let invalid = |reason: String| CalendarError::Invalid {
            table,
            calendar: id.clone(),
            reason,
        };

        let tz = chrono_tz::Tz::from_str(&spec.timezone).map_err(|_| {
            CalendarError::UnknownTimezone {
                calendar: spec.id.clone(),
                timezone: spec.timezone.clone(),
            }
        })?;
        if spec.roots.is_empty() {
            return Err(invalid("it claims no root symbols".to_owned()));
        }
        if spec.sources.is_empty() {
            return Err(invalid(
                "it cites no sources; an uncited calendar is a rumour about the market".to_owned(),
            ));
        }

        let open_local =
            LocalTime::parse(&spec.session.open_local, &spec.id, "session.open_local")?;
        let close_local =
            LocalTime::parse(&spec.session.close_local, &spec.id, "session.close_local")?;
        let rth_open_local = LocalTime::parse(
            &spec.session.rth_open_local,
            &spec.id,
            "session.rth_open_local",
        )?;
        let rth_close_local = LocalTime::parse(
            &spec.session.rth_close_local,
            &spec.id,
            "session.rth_close_local",
        )?;
        // A day-level claim is a claim about an exchange this table does not
        // otherwise describe, so it must carry its own citation. Enforced at
        // load rather than trusted, because the whole value of `day_level_roots`
        // is that someone checked (D-0059).
        if !spec.day_level_roots.is_empty() && spec.day_level_source.is_none() {
            return Err(invalid(format!(
                "declares day_level_roots {:?} but no day_level_source. A calendar that \
                 claims another exchange's trading days must cite where that came from",
                spec.day_level_roots
            )));
        }
        // A root cannot be both: `roots` already implies its dates, and listing
        // it twice would make `governs` and `governs_days` disagree about which
        // answer is authoritative.
        if let Some(both) = spec.day_level_roots.iter().find(|r| spec.roots.contains(r)) {
            return Err(invalid(format!(
                "root {both} appears in both roots and day_level_roots; hour-level \
                 already implies day-level"
            )));
        }

        match spec.session.shape {
            SessionShape::Overnight if close_local >= open_local => {
                return Err(invalid(format!(
                    "the session closes at {}:{:02} and opens at {}:{:02}; an `overnight` trading \
                     day must open on the previous calendar day and close on its own. If this \
                     market opens and closes on the same day, say `shape = \"same_day\"`",
                    close_local.hour, close_local.minute, open_local.hour, open_local.minute
                )));
            }
            SessionShape::SameDay if close_local <= open_local => {
                return Err(invalid(format!(
                    "the session opens at {}:{:02} and closes at {}:{:02}; a `same_day` trading \
                     day must close after it opens",
                    open_local.hour, open_local.minute, close_local.hour, close_local.minute
                )));
            }
            _ => {}
        }
        if rth_close_local <= rth_open_local {
            return Err(invalid(
                "the regular-hours window ends no later than it starts".to_owned(),
            ));
        }

        let mut halts_local = Vec::with_capacity(spec.session.halt_local.len());
        for pair in &spec.session.halt_local {
            let from = LocalTime::parse(&pair[0], &spec.id, "session.halt_local")?;
            let to = LocalTime::parse(&pair[1], &spec.id, "session.halt_local")?;
            if to <= from {
                return Err(invalid(format!(
                    "halt {}:{:02}-{}:{:02} ends no later than it starts",
                    from.hour, from.minute, to.hour, to.minute
                )));
            }
            halts_local.push((from, to));
        }
        halts_local.sort();
        for pair in halts_local.windows(2) {
            if pair[1].0 < pair[0].1 {
                return Err(invalid("two halts overlap".to_owned()));
            }
        }

        let valid_from = parse_date(&spec.valid_from)
            .ok_or_else(|| invalid(format!("valid_from {:?} is not a date", spec.valid_from)))?;
        let reference_start = parse_date(&spec.reference_span.start).ok_or_else(|| {
            invalid(format!(
                "reference_span.start {:?} is not a date",
                spec.reference_span.start
            ))
        })?;
        let reference_end = parse_date(&spec.reference_span.end).ok_or_else(|| {
            invalid(format!(
                "reference_span.end {:?} is not a date",
                spec.reference_span.end
            ))
        })?;
        if days_from_civil(reference_start) < days_from_civil(valid_from) {
            return Err(invalid(format!(
                "reference_span starts {reference_start}, before valid_from {valid_from}:                  bars_per_year would average in sessions this template does not describe"
            )));
        }
        if days_from_civil(reference_end) <= days_from_civil(reference_start) {
            return Err(invalid(
                "reference_span ends no later than it starts, so bars_per_year would divide by \
                 zero or worse"
                    .to_owned(),
            ));
        }

        // Every citation in the file ends up in one list, so `sources()` is a
        // complete answer to "on what authority does this calendar say that".
        let mut sources = spec.sources;
        sources.push(spec.session.source);

        let close_local_normal = close_local;
        let mut holidays = Vec::with_capacity(spec.holiday.len());
        for holiday in spec.holiday {
            if holiday.source.trim().is_empty() {
                return Err(invalid(format!(
                    "holiday {:?} cites no source",
                    holiday.name
                )));
            }
            if let Effect::EarlyClose { close_local } = &holiday.effect {
                let close = LocalTime::parse(close_local, &spec.id, "holiday.effect.close_local")?;
                // An "early" close after the normal one would make the session
                // longer than a normal day, and one at or before the open would
                // make it negative. Both are typos, and both would be invisible
                // in the output.
                if close >= close_local_normal {
                    return Err(invalid(format!(
                        "holiday {:?} closes at {close_local}, which is not earlier than the                          session's own close",
                        holiday.name
                    )));
                }
            }
            sources.push(holiday.source.clone());
            holidays.push(Holiday {
                name: holiday.name,
                rule: holiday.rule,
                effect: holiday.effect,
                first_year: holiday.first_year,
                last_year: holiday.last_year,
                source: holiday.source,
            });
        }

        let mut one_offs = BTreeMap::new();
        for one_off in spec.one_off {
            if one_off.source.trim().is_empty() {
                return Err(invalid(format!(
                    "one-off {:?} cites no source",
                    one_off.name
                )));
            }
            let date = parse_date(&one_off.date)
                .ok_or_else(|| invalid(format!("one_off.date {:?} is not a date", one_off.date)))?;
            if let Effect::EarlyClose { close_local } = &one_off.effect {
                let close = LocalTime::parse(close_local, &spec.id, "one_off.effect.close_local")?;
                if close >= close_local_normal {
                    return Err(invalid(format!(
                        "one-off {:?} closes at {close_local}, which is not earlier than the                          session's own close",
                        one_off.name
                    )));
                }
            }
            sources.push(one_off.source);
            if one_offs
                .insert(
                    (date.year, date.month, date.day),
                    (one_off.name.clone(), one_off.effect),
                )
                .is_some()
            {
                return Err(invalid(format!(
                    "two one-off entries claim {}; which one wins would depend on file order",
                    one_off.date
                )));
            }
        }

        sources.sort();
        sources.dedup();

        let mut calendar = Calendar {
            id: spec.id,
            description: spec.description,
            tz,
            roots: spec.roots,
            day_level_roots: spec.day_level_roots,
            shape: spec.session.shape,
            open_local,
            close_local,
            halts_local,
            rth_open_local,
            rth_close_local,
            valid_from,
            reference_start,
            reference_end,
            reference_rationale: spec.reference_span.rationale,
            holidays,
            one_offs,
            sources,
            open_seconds_in_span: 0,
            trading_days_in_span: 0,
        };

        // Walk the reference span once, now, so `bars_per_year` is O(1) and a
        // table that cannot be walked fails here rather than mid-backtest.
        let (seconds, days) = calendar.walk_reference_span(table)?;
        calendar.open_seconds_in_span = seconds;
        calendar.trading_days_in_span = days;
        Ok(calendar)
    }

    fn walk_reference_span(&self, table: &'static str) -> Result<(i64, i64), CalendarError> {
        let first = days_from_civil(self.reference_start);
        let last = days_from_civil(self.reference_end);
        let mut seconds = 0i64;
        let mut days = 0i64;
        for offset in first..last {
            let date = crate::ingest::window::civil_from_days(offset);
            if naive_datetime(date, self.open_local).is_none() {
                return Err(CalendarError::Invalid {
                    table,
                    calendar: self.id.clone(),
                    reason: format!("reference span contains {date}, which is not representable"),
                });
            }
            let intervals = self.open_intervals(date);
            if intervals.is_empty() {
                continue;
            }
            days += 1;
            for (start, end) in intervals {
                seconds += (end.0 - start.0) / 1_000_000_000;
            }
        }
        if days == 0 {
            return Err(CalendarError::Invalid {
                table,
                calendar: self.id.clone(),
                reason: "the reference span contains no trading day at all".to_owned(),
            });
        }
        Ok((seconds, days))
    }
}

/// `YYYY-MM-DD` to a civil date, rejecting anything chrono cannot hold.
fn parse_date(text: &str) -> Option<CivilDate> {
    let date = crate::ingest::window::parse_civil_date(text)?;
    chrono::NaiveDate::from_ymd_opt(i32::try_from(date.year).ok()?, date.month, date.day)?;
    Some(date)
}

fn naive_datetime(date: CivilDate, time: LocalTime) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDate::from_ymd_opt(i32::try_from(date.year).ok()?, date.month, date.day)?
        .and_hms_opt(time.hour, time.minute, time.second)
}

/// Strips a vendor series suffix so `ES.FUT` and `ES.v.0` both reduce to `ES`.
fn strip_series_suffix(instrument: &str) -> &str {
    match instrument.split_once('.') {
        Some((root, _)) => root,
        None => instrument,
    }
}

#[cfg(test)]
mod tests;
