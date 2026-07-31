//! The roll table: when the front contract changed, and by how much the two
//! contracts disagreed when it did.
//!
//! ## The roll instant, and why it is not the session it was decided from
//!
//! A roll is decided from a day's volume. That day's volume is not knowable
//! until that day's last bar is — so the instant a roll can take effect is
//!
//! ```text
//! roll_ts = max(front.avail_ts(day), next.avail_ts(day))
//! ```
//!
//! the later of the two bucket availabilities on the deciding day (see
//! [`SessionBar`](super::SessionBar)). And the new contract is front for
//! every bar with `avail_ts` **strictly greater** than `roll_ts` — never
//! equal.
//!
//! That strictness is the same comparison `crucible-engine`'s `replay.rs`
//! makes when it fills orders (`order.placed_ts < event.avail_ts`), and for
//! the same reason: the bars whose availability *is* `roll_ts` are the bars
//! the decision was made from. Letting the roll consume them would mean the
//! series switched contracts using information that arrived at the same
//! instant the decision did — one bar of lookahead, in the one place where
//! it is easiest to hide, because the switch is invisible on a chart of an
//! adjusted series.
//!
//! Concretely: a roll decided on day D's volume cannot take effect before
//! day D's data is available, and takes effect from the first bar available
//! after it. It costs the series one bar of the old contract. That is the
//! honest price.
//!
//! ## Rules in v1
//!
//! - [`RollRule::VolumeCrossover`] — Databento's `.v`. The front contract
//!   rolls to the next on the first session where the next contract's volume
//!   exceeds the front's for `confirm_days` **consecutive** sessions. The
//!   session axis is the front contract's own buckets; a day the next
//!   contract did not trade counts as zero volume and breaks the streak.
//! - [`RollRule::CalendarDaysBeforeExpiry`] — Databento's `.c`. Rolls on the
//!   first session at or after `expiry − days` on which both contracts
//!   traded — where `expiry` is **the expiry known at that session's own
//!   decision instant** (D-0090; see below).
//!
//! ## The calendar rule's fixed point, and why it needs no iteration
//!
//! Since D-0090 an expiry is a function of when you ask:
//! [`ExpiryHistory::as_of`] returns the latest statement whose `ts_recv` does
//! not lie in the asker's future. That makes the calendar rule circular on its
//! face: `due` depends on the expiry, the expiry depends on the decision
//! instant, and the decision instant is the roll that `due` produces.
//!
//! ```text
//! D = g(f(D))   f = expiry as of an instant, g = the roll instant a due date gives
//! ```
//!
//! **The circle is only apparent, because the candidate set does not depend on
//! the expiry.** A candidate is a session both contracts traded, and its
//! decision instant `max(front.avail_ts(day), next.avail_ts(day))` is a property
//! of the bars alone — no expiry enters it. So the equation decomposes into an
//! independent test per candidate: *given what was knowable at this candidate's
//! own instant, was the roll due by this session?* A candidate that passes its
//! own test **is** a solution of `D = g(f(D))`, and the answer is the earliest
//! such candidate, because a roll is the first time a rule fires.
//!
//! `calendar_roll_day` therefore resolves the fixed point in one ascending
//! scan, at most one step per front session, with no iteration to diverge and
//! nothing that could depend on iteration order or on a float (§2.2). There is
//! no "did not converge" branch because there is no loop that could fail to
//! converge: the bound is the session count, finite by construction — a
//! [`ContractSeries`] holds a `Vec`.
//!
//! What this buys is the case plain "latest wins" cannot even see: a correction
//! published *after* the roll it would have moved is invisible to that roll, as
//! it must be, because a backtest could not have read it. Where the archive
//! holds one, [`late_expiry_corrections`] names it instead of absorbing it.
//!
//! **Open interest (`.n`) is deliberately absent, as a variant and not only
//! as an implementation.** It needs the `statistics` schema's open-interest
//! series, which M1 has not curated. A variant that existed and always
//! failed would be a config that typechecks and dies at load; a variant that
//! quietly fell back to volume would be a table labelled `.n` that is not
//! one. It arrives with the data and its own decision-log entry.
//!
//! ## The expiry backstop
//!
//! The rule decides *when* to roll; expiry decides *that* you must. If the
//! configured rule never fires and the front contract **stops trading before
//! the next one does**, the roll is placed on the last session both traded —
//! the last instant at which the gap between them is observable, and
//! therefore the last instant at which an additive adjustment can be measured
//! without inventing a price. If they never overlapped at all, that is
//! [`ContinuousError::NoOverlap`] and the table refuses to exist.
//!
//! The precondition matters as much as the backstop. A front contract that is
//! *still trading* when the data ends has not handed over to anything — the
//! data merely stopped — so the chain ends there and
//! [`RollTable::contracts`] says where. This is what makes a single-month
//! archive of ES produce a one-contract table (ESH4 was front for all of
//! January 2024) instead of a fictional roll on the 31st. Deferred contracts
//! that traded in the window but were never front are simply not in the
//! chain, and [`RollTable::last_ts_open`] describes the chain, not the input,
//! so a replay cannot ask for a window the table does not cover.
//!
//! ## Where a table is stored, and why it is not in the manifest
//!
//! ```text
//! curated/rolls/{root}/{tf}/{rule-slug}.json
//! ```
//!
//! e.g. `curated/rolls/ES/1m/v-confirm1.json`.
//!
//! It lives under `curated/` because it is exactly what D-0036 says curated
//! data is: **derived, disposable, and rebuildable** from `raw/` plus a
//! version of this code. It is not in `manifest.jsonl` for the same reason
//! curated bars are not — the manifest records *acquisitions*, and nobody
//! bought a roll table.
//!
//! It is the one curated artifact that cannot be named after its source
//! window, because it has many: a roll is a statement about two contracts at
//! once, and a table spans every contract of a root. So it is named after
//! what actually determines its contents — the root, the interval, and the
//! rule — and it records the manifest id of every curated file it read, so a
//! result produced through it can still name the precise bytes behind it
//! (D-0013). Rebuilding over a different set of source files replaces it,
//! which is safe because the table also records the `ts_open` span it was
//! built from, and [`ContinuousFeed::open`](super::ContinuousFeed::open)
//! refuses a replay window reaching outside that span rather than quietly
//! returning a series that is missing contracts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};
use serde::{Deserialize, Serialize};

use crate::catalog::{TsRange, ts_serde};
use crate::curated::path::encode_instrument;

use super::error::ContinuousError;
use super::expiry::ExpiryHistory;
use super::session::{ContractSeries, SessionBar};
use super::symbol::{ContractSymbol, DecadeAnchor};

/// Roll-table schema version this build reads and writes.
///
/// A mismatch is a hard refusal, on [`CURATED_SCHEMA_VERSION`]'s argument
/// (D-0036): this build cannot know what the fields mean. Refusing costs one
/// `crucible rolls --write`.
///
/// [`CURATED_SCHEMA_VERSION`]: crate::curated::CURATED_SCHEMA_VERSION
pub const ROLL_TABLE_SCHEMA_VERSION: u32 = 1;

/// Directory under the data dir holding roll tables.
pub const CURATED_ROLLS_DIR: &str = "curated/rolls";

/// Serde bridge for [`Price`]: a plain JSON integer, nanopoints.
///
/// `crucible-core` is dependency-free, so serde may not appear there — the
/// same reason [`ts_serde`] exists in `catalog`.
pub(crate) mod price_serde {
    use crucible_core::types::Price;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(price: &Price, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(price.as_nanos())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Price, D::Error> {
        i64::deserialize(d).map(Price::from_nanos)
    }
}

/// Serde bridge for [`TimeFrame`]: the canonical string spelling (§4), so a
/// hand-read table says `"1m"` and not an opaque discriminant.
pub(crate) mod tf_serde {
    use crucible_core::types::TimeFrame;
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(tf: &TimeFrame, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&tf.to_string())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<TimeFrame, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse().map_err(D::Error::custom)
    }
}

/// How the front contract is chosen.
///
/// Externally tagged in JSON — `{"volume_crossover": {"confirm_days": 1}}` —
/// so `deny_unknown_fields` applies to the parameters and a misspelled knob
/// is a hard error rather than a silent default (CLAUDE.md §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RollRule {
    /// Databento's `.v`: roll when the next contract out-trades the front
    /// for `confirm_days` consecutive sessions.
    VolumeCrossover {
        /// Consecutive sessions the next contract must lead before the roll
        /// is taken. `1` is the default and matches the vendor's alias.
        confirm_days: u32,
    },
    /// Databento's `.c`: roll a fixed number of calendar days before the
    /// front contract's expiry.
    CalendarDaysBeforeExpiry {
        /// Calendar days before expiry at which the roll becomes due.
        days: u32,
    },
}

impl Default for RollRule {
    fn default() -> RollRule {
        RollRule::VolumeCrossover { confirm_days: 1 }
    }
}

impl RollRule {
    /// Checks the rule's parameters.
    ///
    /// # Errors
    /// [`ContinuousError::InvalidRule`] for `confirm_days == 0`, which would
    /// mean "roll before observing anything".
    pub fn validate(&self) -> Result<(), ContinuousError> {
        match self {
            RollRule::VolumeCrossover { confirm_days } if *confirm_days == 0 => {
                Err(ContinuousError::InvalidRule {
                    detail: "confirm_days must be at least 1; zero would roll on no evidence"
                        .to_owned(),
                })
            }
            _ => Ok(()),
        }
    }

    /// Whether this rule reads [`RollTableInput::expiries`] at all.
    ///
    /// The volume rule does not: it compares two contracts' session volumes, and
    /// the expiry backstop is a statement about *which contract stops trading
    /// first*, read off the bars. Measured on this archive, `.v` on GC, ZN and
    /// 6E produces byte-identical tables under either expiry (D-0090).
    ///
    /// It is public because the CLI must not *resolve* an input the rule will
    /// not *read* — resolving GC's expiries used to exit 4 on a `.v` build that
    /// then never looked at them. One predicate, so the caller and the builder
    /// cannot drift.
    #[must_use]
    pub const fn reads_expiries(&self) -> bool {
        match self {
            RollRule::VolumeCrossover { .. } => false,
            RollRule::CalendarDaysBeforeExpiry { .. } => true,
        }
    }

    /// The single letter §4 pins for a continuous alias: `v` or `c`.
    #[must_use]
    pub const fn alias_letter(&self) -> char {
        match self {
            RollRule::VolumeCrossover { .. } => 'v',
            RollRule::CalendarDaysBeforeExpiry { .. } => 'c',
        }
    }

    /// Filename-safe identity of this rule, used as the table's file stem.
    #[must_use]
    pub fn slug(&self) -> String {
        match self {
            RollRule::VolumeCrossover { confirm_days } => format!("v-confirm{confirm_days}"),
            RollRule::CalendarDaysBeforeExpiry { days } => format!("c-minus{days}d"),
        }
    }
}

impl core::fmt::Display for RollRule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RollRule::VolumeCrossover { confirm_days } => write!(
                f,
                "volume_crossover (next contract leads for {confirm_days} consecutive session(s))"
            ),
            RollRule::CalendarDaysBeforeExpiry { days } => {
                write!(
                    f,
                    "calendar_days_before_expiry ({days} day(s) before expiry)"
                )
            }
        }
    }
}

/// One handover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollRow {
    /// Contract being left, as the archive spells it (`ESH4`).
    pub from_contract: String,
    /// Contract being entered.
    pub to_contract: String,
    /// The instant the decision became knowable. The new contract is front
    /// for bars with `avail_ts` **strictly after** this; see the module docs.
    #[serde(with = "ts_serde")]
    pub roll_ts: Ts,
    /// `close(to) − close(from)` at `roll_ts`, in nanopoints. The primitive
    /// an additive adjustment accumulates; stored per roll rather than as a
    /// running offset so appending next month's roll does not rewrite every
    /// earlier row.
    #[serde(with = "price_serde")]
    pub adjustment: Price,
}

/// One curated file a table was computed from.
///
/// The provenance chain D-0013 requires, carried through the roll table the
/// same way [`CuratedSource`](crate::curated::CuratedSource) carries it
/// through a feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollSource {
    /// Instrument the curated file holds.
    pub instrument: String,
    /// Archive-relative path of the raw file behind it.
    pub source_file_path: String,
    /// blake3 of that raw file — the manifest id (D-0014).
    pub source_file_blake3: String,
    /// Transcoder semantics that produced the curated bars.
    pub transcoder_version: u32,
    /// Bars contributed.
    pub bars: u64,
}

/// A stitching plan for one product root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollTable {
    /// Always [`ROLL_TABLE_SCHEMA_VERSION`]; anything else is a hard refusal.
    pub schema_version: u32,
    /// Product root, e.g. `ES`.
    pub root: String,
    /// Bar interval the volume comparison was measured at.
    #[serde(with = "tf_serde")]
    pub tf: TimeFrame,
    /// The rule that chose the rolls.
    pub rule: RollRule,
    /// Reference year one-digit contract years were resolved against
    /// (`crucible_data::continuous::symbol` module docs). Stored so the same
    /// bytes always reparse to the same contracts.
    pub decade_anchor: i32,
    /// Where the expiries came from — `"none"`, `"nominal-third-friday"`, or
    /// a `definition` file's manifest id. A calendar rule's every decision
    /// rests on this, so it is recorded rather than assumed.
    pub expiry_source: String,
    /// `ts_open` of the earliest bar behind the table.
    #[serde(with = "ts_serde")]
    pub first_ts_open: Ts,
    /// `ts_open` of the latest bar behind the table.
    #[serde(with = "ts_serde")]
    pub last_ts_open: Ts,
    /// The contract chain, front-most first. Always `rows.len() + 1` long.
    pub contracts: Vec<String>,
    /// Curated provenance, in contract order.
    pub sources: Vec<RollSource>,
    /// The handovers, ascending in `roll_ts`.
    pub rows: Vec<RollRow>,
}

impl RollTable {
    /// The continuous alias this table describes, per §4: `{root}.{v|c}.0`.
    #[must_use]
    pub fn alias(&self) -> InstrumentId {
        InstrumentId::new(format!("{}.{}.0", self.root, self.rule.alias_letter()))
    }

    /// The half-open `ts_open` window this table can answer about:
    /// `[first_ts_open, last_ts_open + tf)`.
    ///
    /// The end is the *availability* end of the last bar expressed back in
    /// `ts_open` terms, so a request for "everything up to and including the
    /// last bar" is covered exactly rather than by one interval short.
    ///
    /// # Errors
    /// [`ContinuousError::MalformedTable`] if the span runs backwards, which
    /// [`RollTable::validate`] also refuses.
    pub fn covered_range(&self) -> Result<TsRange, ContinuousError> {
        TsRange::new(
            self.first_ts_open,
            self.last_ts_open.plus_ns(self.tf.duration_ns()),
        )
        .map_err(|e| ContinuousError::MalformedTable {
            detail: format!("the table's own span is not a range: {e}"),
        })
    }

    /// `requested` narrowed to [`covered_range`](RollTable::covered_range), or
    /// `None` when the two do not overlap at all.
    ///
    /// Not a weakening of the D-0045 refusal, which
    /// [`ContinuousFeed::open`](super::ContinuousFeed::open) still makes
    /// against whatever window it is handed. This is the *caller's* question:
    /// a `--start`/`--end` pair is a pair of dates, and a date is a request for
    /// whatever part of it exists — the same thing
    /// [`ParquetBarFeed`](crate::curated::ParquetBarFeed) does for an outright,
    /// where a window reaching past the first bar simply yields the bars there
    /// are. What must never happen is doing it *quietly*: a caller that narrows
    /// is expected to say so, because a table built before the archive grew
    /// covers less than the archive holds, and silence there is a replay
    /// missing whole contracts (D-0076).
    ///
    /// # Errors
    /// [`ContinuousError::MalformedTable`] via
    /// [`covered_range`](RollTable::covered_range).
    pub fn narrow(&self, requested: TsRange) -> Result<Option<TsRange>, ContinuousError> {
        let covered = self.covered_range()?;
        let start = requested.start_ts().max(covered.start_ts());
        let end = requested.end_ts().min(covered.end_ts());
        Ok(TsRange::new(start, end).ok())
    }

    /// Checks every invariant a reader depends on.
    ///
    /// Applied to a table this build just computed *and* to one read off
    /// disk: a hand-edited table gets no more trust than a caller, exactly
    /// as `catalog` treats a hand-edited manifest.
    ///
    /// # Errors
    /// [`ContinuousError::MalformedTable`] naming the invariant that failed,
    /// or [`ContinuousError::InvalidRule`] for an impossible rule.
    pub fn validate(&self) -> Result<(), ContinuousError> {
        let bad = |detail: String| ContinuousError::MalformedTable { detail };
        self.rule.validate()?;
        if self.root.is_empty() {
            return Err(bad("the product root is empty".to_owned()));
        }
        if self.contracts.is_empty() {
            return Err(bad("no contracts, so no series".to_owned()));
        }
        if self.contracts.len() != self.rows.len() + 1 {
            return Err(bad(format!(
                "{} contracts describe {} segments, but there are {} rolls",
                self.contracts.len(),
                self.contracts.len(),
                self.rows.len()
            )));
        }
        if self.first_ts_open > self.last_ts_open {
            return Err(bad(format!(
                "the span [{}, {}] runs backwards",
                self.first_ts_open, self.last_ts_open
            )));
        }
        let anchor = DecadeAnchor::new(self.decade_anchor);
        let mut previous: Option<ContractSymbol> = None;
        for contract in &self.contracts {
            let symbol = ContractSymbol::parse_with_anchor(contract, anchor)?;
            if symbol.root() != self.root {
                return Err(ContinuousError::RootMismatch {
                    expected: self.root.clone(),
                    found: symbol.root().to_owned(),
                    symbol: contract.clone(),
                });
            }
            if let Some(prev) = &previous
                && prev >= &symbol
            {
                return Err(bad(format!(
                    "{prev} does not deliver before {symbol}; a chain that goes \
                     backwards in delivery is not a continuous series"
                )));
            }
            previous = Some(symbol);
        }
        let mut last_roll: Option<Ts> = None;
        for (index, row) in self.rows.iter().enumerate() {
            if self.contracts.get(index).map(String::as_str) != Some(row.from_contract.as_str())
                || self.contracts.get(index + 1).map(String::as_str)
                    != Some(row.to_contract.as_str())
            {
                return Err(bad(format!(
                    "roll {index} goes {} -> {} but the chain says {:?} -> {:?}",
                    row.from_contract,
                    row.to_contract,
                    self.contracts.get(index),
                    self.contracts.get(index + 1)
                )));
            }
            if let Some(prev) = last_roll
                && row.roll_ts <= prev
            {
                return Err(bad(format!(
                    "roll {index} is stamped {} after {prev}; rolls must be strictly \
                     ordered or the segments they bound overlap",
                    row.roll_ts
                )));
            }
            last_roll = Some(row.roll_ts);
        }
        Ok(())
    }
}

/// Everything [`build_roll_table`] needs, gathered so validation happens once.
#[derive(Debug)]
pub struct RollTableInput<'a> {
    /// Product root the table is for.
    pub root: &'a str,
    /// Interval the series were measured at.
    pub tf: TimeFrame,
    /// The rule to apply.
    pub rule: RollRule,
    /// Anchor used to parse one-digit contract years.
    pub decade_anchor: DecadeAnchor,
    /// Contract expiries, with when each became knowable. Read **only** by a
    /// calendar rule ([`RollRule::reads_expiries`]); the volume rule runs with
    /// an empty history and produces the same table it would with a full one.
    pub expiries: &'a ExpiryHistory,
    /// Where those expiries came from, for the record.
    pub expiry_source: &'a str,
    /// One series per contract, keyed by delivery so iteration order is the
    /// order contracts roll in. A `BTreeMap` and not a `HashMap`: this
    /// ordering reaches the output (CLAUDE.md §2.2).
    pub series: &'a BTreeMap<ContractSymbol, ContractSeries>,
    /// Curated provenance for those series.
    pub sources: &'a [RollSource],
}

/// Computes a roll table.
///
/// # Errors
/// [`ContinuousError::InvalidRule`] for impossible parameters,
/// [`ContinuousError::NoSeries`] when no contract traded,
/// [`ContinuousError::RootMismatch`] when a series belongs to another
/// product, [`ContinuousError::MissingExpiry`] when a calendar rule has no
/// expiry to work from, and [`ContinuousError::NoOverlap`] when two adjacent
/// contracts never traded on a common session.
pub fn build_roll_table(input: &RollTableInput<'_>) -> Result<RollTable, ContinuousError> {
    input.rule.validate()?;

    // Contracts that actually traded, in delivery order. A contract the
    // symbology mapped but that never printed a bar is not part of any chain
    // — the same trap D-0033 describes for transcode.
    let chain: Vec<(&ContractSymbol, &ContractSeries)> = input
        .series
        .iter()
        .filter(|(_, series)| !series.sessions.is_empty())
        .collect();
    for (symbol, _) in &chain {
        if symbol.root() != input.root {
            return Err(ContinuousError::RootMismatch {
                expected: input.root.to_owned(),
                found: symbol.root().to_owned(),
                symbol: symbol.to_string(),
            });
        }
    }
    let Some((_, first_series)) = chain.first() else {
        return Err(ContinuousError::NoSeries {
            root: input.root.to_owned(),
            tf: input.tf,
        });
    };

    let mut contracts = vec![first_series.instrument.as_str().to_owned()];
    let mut rows: Vec<RollRow> = Vec::new();
    let mut front = 0usize;
    // Rolls are searched strictly after the previous roll's day, which also
    // guarantees strictly increasing `roll_ts`: a bucket's availability lies
    // inside its own UTC day.
    let mut after_day = i64::MIN;

    while front + 1 < chain.len() {
        let next = front + 1;
        let (front_symbol, front_series) = chain[front];
        let (_, next_series) = chain[next];

        let day = match input.rule {
            RollRule::VolumeCrossover { confirm_days } => {
                volume_crossover_day(front_series, next_series, confirm_days, after_day)
            }
            RollRule::CalendarDaysBeforeExpiry { days } => {
                // A contract the source names nothing about is a refusal; one
                // whose expiry is not knowable *yet* is simply not due yet, and
                // the scan handles that. The two are different sentences and
                // `as_of` cannot tell them apart, which is why the question is
                // asked here.
                if !input.expiries.contains(front_symbol) {
                    return Err(ContinuousError::MissingExpiry {
                        contract: front_series.instrument.as_str().to_owned(),
                    });
                }
                calendar_roll_day(
                    front_series,
                    next_series,
                    front_symbol,
                    input.expiries,
                    days,
                    after_day,
                )
            }
        };
        // The backstop, and its own precondition. A contract that stops
        // trading must be left, so a rule that never fired falls back to the
        // last session the gap was observable on. But a front contract that
        // is *still trading* at the end of the data has not handed over to
        // anything — the data merely stopped — and inventing a roll there
        // would put a handover in the series that never happened. So the
        // chain ends instead, and `contracts` says where.
        let day = match day {
            Some(day) => day,
            None if front_series.last_ts_open < next_series.last_ts_open => {
                last_common_day(front_series, next_series, after_day).ok_or_else(|| {
                    ContinuousError::NoOverlap {
                        from: front_series.instrument.as_str().to_owned(),
                        to: next_series.instrument.as_str().to_owned(),
                    }
                })?
            }
            None => break,
        };

        let (front_bucket, next_bucket) = (
            front_series
                .session(day)
                .ok_or_else(|| ContinuousError::MalformedTable {
                    detail: format!("no {day} session for {}", front_series.instrument),
                })?,
            next_series
                .session(day)
                .ok_or_else(|| ContinuousError::MalformedTable {
                    detail: format!("no {day} session for {}", next_series.instrument),
                })?,
        );
        rows.push(RollRow {
            from_contract: front_series.instrument.as_str().to_owned(),
            to_contract: next_series.instrument.as_str().to_owned(),
            roll_ts: Ts(front_bucket.avail_ts.0.max(next_bucket.avail_ts.0)),
            adjustment: next_bucket.close - front_bucket.close,
        });
        contracts.push(next_series.instrument.as_str().to_owned());
        after_day = day;
        front = next;
    }

    let table = RollTable {
        schema_version: ROLL_TABLE_SCHEMA_VERSION,
        root: input.root.to_owned(),
        tf: input.tf,
        rule: input.rule,
        decade_anchor: input.decade_anchor.year(),
        expiry_source: input.expiry_source.to_owned(),
        first_ts_open: first_series.first_ts_open,
        // The span is the *chain's*, not the input's: contracts the chain
        // never reached contribute no bars, so claiming their dates would
        // let a replay ask for a window this table cannot describe.
        last_ts_open: chain[front].1.last_ts_open,
        contracts,
        sources: input.sources.to_vec(),
        rows,
    };
    table.validate()?;
    Ok(table)
}

/// First front session strictly after `after_day` on which the next contract
/// has out-traded the front for `confirm_days` consecutive front sessions.
fn volume_crossover_day(
    front: &ContractSeries,
    next: &ContractSeries,
    confirm_days: u32,
    after_day: i64,
) -> Option<i64> {
    let mut streak: u32 = 0;
    for bucket in front.sessions.iter().filter(|s| s.day > after_day) {
        // A day the next contract did not trade is a day it traded nothing,
        // which breaks the streak rather than skipping it. Anything else
        // would let a holiday confirm a roll.
        let next_volume = next.session(bucket.day).map_or(0, |s| s.volume);
        if next_volume > bucket.volume {
            streak += 1;
            if streak >= confirm_days {
                return Some(bucket.day);
            }
        } else {
            streak = 0;
        }
    }
    None
}

/// First session both contracts traded on at or after `expiry − days`, where
/// `expiry` is the one knowable **at that session's own decision instant**.
///
/// The fixed-point resolution the module docs argue for, in one ascending scan:
/// each candidate is tested against what was knowable when the candidate's own
/// bars became available, so a candidate that passes is self-consistent and the
/// earliest one is the roll.
///
/// A session whose decision instant precedes every statement about the contract
/// simply is not due — the rule cannot fire on an expiry nobody had published
/// yet — so the scan continues rather than refusing. A contract the history does
/// not name at all is refused by the caller before the scan starts.
fn calendar_roll_day(
    front: &ContractSeries,
    next: &ContractSeries,
    contract: &ContractSymbol,
    history: &ExpiryHistory,
    days: u32,
    after_day: i64,
) -> Option<i64> {
    for bucket in front.sessions.iter().filter(|s| s.day > after_day) {
        let Some(next_bucket) = next.session(bucket.day) else {
            continue;
        };
        // Exactly the instant `build_roll_table` will stamp on the roll if this
        // candidate wins, and it is computed from the bars alone — which is what
        // makes the test self-consistent rather than circular.
        let decision_ts = Ts(bucket.avail_ts.0.max(next_bucket.avail_ts.0));
        let Some(expiry) = history.as_of(contract, decision_ts) else {
            continue;
        };
        let expiry_day =
            crate::ingest::window::days_from_civil(crate::ingest::window::date_of(expiry));
        if bucket.day >= expiry_day - i64::from(days) {
            return Some(bucket.day);
        }
    }
    None
}

/// A restatement of a contract's expiry that landed **after** the roll it would
/// have moved.
///
/// The lookahead D-0090's availability filter refuses to absorb, made countable.
/// Plain "latest wins" cannot report this, because it never looks at when a
/// statement arrived; it just quietly uses the correction. Measured on this
/// archive: **zero** — every correction landed between 7 days and 18 months
/// before its own roll — which is the number this type exists to keep honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LateExpiryCorrection {
    /// The contract being left at that roll.
    pub contract: String,
    /// The roll instant the table recorded.
    pub roll_ts: Ts,
    /// The expiry the roll was decided on — the one knowable at `roll_ts`.
    pub used: Ts,
    /// The expiry the source ends up stating.
    pub latest: Ts,
    /// When the correction became knowable. Strictly after `roll_ts`, which is
    /// the whole point.
    pub corrected_avail_ts: Ts,
}

/// Every roll in `table` that a later expiry statement would have moved.
///
/// Empty for a rule that reads no expiries ([`RollRule::reads_expiries`]) — a
/// correction cannot be late for a decision that never consulted it.
///
/// # Errors
/// [`ContinuousError::UnparseableSymbol`] if a contract in the table cannot be
/// parsed against `anchor`, which [`RollTable::validate`] already refuses.
pub fn late_expiry_corrections(
    table: &RollTable,
    history: &ExpiryHistory,
    anchor: DecadeAnchor,
) -> Result<Vec<LateExpiryCorrection>, ContinuousError> {
    let mut out = Vec::new();
    if !table.rule.reads_expiries() {
        return Ok(out);
    }
    for row in &table.rows {
        let symbol = ContractSymbol::parse_with_anchor(&row.from_contract, anchor)?;
        let (Some(used), Some(latest)) =
            (history.as_of(&symbol, row.roll_ts), history.latest(&symbol))
        else {
            continue;
        };
        if used == latest {
            continue;
        }
        let corrected_avail_ts = history
            .revisions(&symbol)
            .iter()
            .find(|r| r.expiration == latest)
            .map_or(row.roll_ts, |r| r.avail_ts);
        out.push(LateExpiryCorrection {
            contract: row.from_contract.clone(),
            roll_ts: row.roll_ts,
            used,
            latest,
            corrected_avail_ts,
        });
    }
    Ok(out)
}

/// Last session strictly after `after_day` on which both contracts traded —
/// the last instant the gap between them is observable.
fn last_common_day(front: &ContractSeries, next: &ContractSeries, after_day: i64) -> Option<i64> {
    front
        .sessions
        .iter()
        .rev()
        .find(|s: &&SessionBar| s.day > after_day && next.session(s.day).is_some())
        .map(|s| s.day)
}

/// Where a table for this root, interval, and rule lives under the data dir.
///
/// # Errors
/// [`ContinuousError::Curated`] if the root cannot name a path component.
pub fn roll_table_path(
    data_dir: &Path,
    root: &str,
    tf: TimeFrame,
    rule: &RollRule,
) -> Result<PathBuf, ContinuousError> {
    let encoded = encode_instrument(root)?;
    Ok(data_dir
        .join(CURATED_ROLLS_DIR)
        .join(encoded)
        .join(tf.to_string())
        .join(format!("{}.json", rule.slug())))
}

/// Every stored roll table for a root and interval whose rule matches
/// `rule_letter` (`v` or `c`), by path, sorted.
///
/// The lookup a continuous alias needs, and it is a *search* rather than a
/// path join because [`ContinuousAlias`](super::ContinuousAlias) names the
/// rule and not its parameters: `ES.v.0` says "volume roll", while the file is
/// `v-confirm1.json` or `v-confirm3.json`. Returning every match and letting
/// the caller refuse a tie is the D-0029 shape — a second candidate stops the
/// run rather than losing to sort order.
///
/// An absent directory is an empty result, not an error: a roll table is
/// curated data and not building one is a normal state of the archive.
///
/// # Errors
/// [`ContinuousError::Curated`] if the root cannot name a path component, and
/// [`ContinuousError::Io`] if the directory exists but cannot be listed.
pub fn roll_tables_for(
    data_dir: &Path,
    root: &str,
    tf: TimeFrame,
    rule_letter: char,
) -> Result<Vec<PathBuf>, ContinuousError> {
    let encoded = encode_instrument(root)?;
    let dir = data_dir
        .join(CURATED_ROLLS_DIR)
        .join(encoded)
        .join(tf.to_string());
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir).map_err(|source| ContinuousError::Io {
        path: dir.clone(),
        during: "listing",
        source,
    })?;
    let prefix = format!("{rule_letter}-");
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ContinuousError::Io {
            path: dir.clone(),
            during: "listing",
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| stem.starts_with(&prefix))
        {
            out.push(path);
        }
    }
    // Sorted so the *listing* in a refusal is stable; the caller still refuses
    // a tie rather than taking the first (CLAUDE.md §2.2).
    out.sort();
    Ok(out)
}

/// The stored roll table a continuous alias names, with the path it came from.
///
/// One lookup, three outcomes, and two of them refuse:
///
/// - nothing stored → [`ContinuousError::NoRollTable`], naming the `crucible
///   rolls` invocation that would build it;
/// - two or more stored → [`ContinuousError::AmbiguousRollTable`], listing
///   them. `ES.v.0` says "the volume rule" and not `confirm_days`, so an
///   archive holding two volume tables genuinely has two answers;
/// - exactly one → read and validated, like any other table off disk.
///
/// # Errors
/// The two above, plus [`ContinuousError::Io`] / [`ContinuousError::Json`] /
/// [`ContinuousError::MalformedTable`] from reading the file itself.
pub fn roll_table_for_alias(
    data_dir: &Path,
    alias: &super::ContinuousAlias,
    tf: TimeFrame,
) -> Result<(RollTable, PathBuf), ContinuousError> {
    let mut found = roll_tables_for(data_dir, alias.root(), tf, alias.rule_letter())?;
    match found.len() {
        0 => Err(ContinuousError::NoRollTable {
            alias: alias.to_string(),
            dir: data_dir
                .join(CURATED_ROLLS_DIR)
                .join(encode_instrument(alias.root())?)
                .join(tf.to_string()),
            rule_letter: alias.rule_letter(),
        }),
        1 => {
            let path = found.remove(0);
            let table = read_roll_table(&path)?;
            Ok((table, path))
        }
        _ => Err(ContinuousError::AmbiguousRollTable {
            alias: alias.to_string(),
            candidates: found
                .iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                .map(str::to_owned)
                .collect(),
        }),
    }
}

/// Writes a table to its canonical path, atomically, and returns that path.
///
/// Write-to-temp-then-rename, matching
/// [`PartitionWriter`](crate::curated::PartitionWriter): a roll table that
/// exists is a complete one, so a killed run leaves nothing a feed would try
/// to read.
///
/// # Errors
/// [`ContinuousError::MalformedTable`] if the table does not validate,
/// [`ContinuousError::Json`] if it cannot be serialized, and
/// [`ContinuousError::Io`] for any filesystem failure.
pub fn write_roll_table(data_dir: &Path, table: &RollTable) -> Result<PathBuf, ContinuousError> {
    table.validate()?;
    let path = roll_table_path(data_dir, &table.root, table.tf, &table.rule)?;
    let tmp = path.with_extension("json.tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ContinuousError::Io {
            path: parent.to_path_buf(),
            during: "creating the roll-table directory",
            source,
        })?;
    }
    let mut text = serde_json::to_string_pretty(table).map_err(|e| ContinuousError::Json {
        path: path.clone(),
        during: "serializing",
        detail: e.to_string(),
    })?;
    text.push('\n');
    std::fs::write(&tmp, text.as_bytes()).map_err(|source| ContinuousError::Io {
        path: tmp.clone(),
        during: "writing",
        source,
    })?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|source| ContinuousError::Io {
            path: path.clone(),
            during: "replacing",
            source,
        })?;
    }
    std::fs::rename(&tmp, &path).map_err(|source| ContinuousError::Io {
        path: path.clone(),
        during: "moving the finished roll table into",
        source,
    })?;
    Ok(path)
}

/// Reads and validates a table.
///
/// # Errors
/// [`ContinuousError::Io`] if the file cannot be read,
/// [`ContinuousError::Json`] if it is not a roll table,
/// [`ContinuousError::UnknownTableVersion`] if it was written by an
/// incompatible build, and [`ContinuousError::MalformedTable`] if it
/// contradicts itself.
pub fn read_roll_table(path: &Path) -> Result<RollTable, ContinuousError> {
    let text = std::fs::read_to_string(path).map_err(|source| ContinuousError::Io {
        path: path.to_path_buf(),
        during: "reading",
        source,
    })?;
    let table: RollTable = serde_json::from_str(&text).map_err(|e| ContinuousError::Json {
        path: path.to_path_buf(),
        during: "parsing",
        detail: e.to_string(),
    })?;
    if table.schema_version != ROLL_TABLE_SCHEMA_VERSION {
        return Err(ContinuousError::UnknownTableVersion {
            path: path.to_path_buf(),
            found: table.schema_version,
            expected: ROLL_TABLE_SCHEMA_VERSION,
        });
    }
    table.validate()?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuous::session::SessionAccumulator;
    use crate::testutil::TempDir;
    use crucible_core::events::Bar;

    const MIN: i64 = 60_000_000_000;
    const DAY: i64 = 86_400 * 1_000_000_000;

    /// One 1-minute bar at `ts_open`, flat at `close` nanopoints.
    fn bar(instrument: &str, ts_open: i64, close: i64, volume: u64) -> Bar {
        Bar {
            instrument: InstrumentId::new(instrument),
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

    /// A contract that trades one bar per day for `days.len()` days starting
    /// at UTC day `start_day`. Each entry is `(volume, close_points)`.
    ///
    /// The bar opens at 12:00 so its `avail_ts` (12:01) lands in the same UTC
    /// day, keeping the fixtures free of the documented midnight skew.
    fn series(instrument: &str, start_day: i64, days: &[(u64, i64)]) -> ContractSeries {
        let mut acc = SessionAccumulator::new(InstrumentId::new(instrument), TimeFrame::M1);
        for (index, (volume, close)) in days.iter().enumerate() {
            let ts_open = (start_day + index as i64) * DAY + 12 * 3_600_000_000_000;
            acc.push(&bar(instrument, ts_open, close * 1_000_000_000, *volume))
                .expect("ordered");
        }
        acc.finish().expect("at least one bar")
    }

    fn chain_of(entries: Vec<(&str, ContractSeries)>) -> BTreeMap<ContractSymbol, ContractSeries> {
        entries
            .into_iter()
            .map(|(symbol, s)| (ContractSymbol::parse(symbol).expect("valid symbol"), s))
            .collect()
    }

    fn build(
        series: &BTreeMap<ContractSymbol, ContractSeries>,
        rule: RollRule,
        expiries: &ExpiryHistory,
    ) -> Result<RollTable, ContinuousError> {
        build_roll_table(&RollTableInput {
            root: "ES",
            tf: TimeFrame::M1,
            rule,
            decade_anchor: DecadeAnchor::DEFAULT,
            expiries,
            expiry_source: "test",
            series,
            sources: &[],
        })
    }

    fn no_expiries() -> ExpiryHistory {
        ExpiryHistory::default()
    }

    /// A history stating one expiry per contract, knowable from the beginning
    /// of time — the shape every pre-D-0090 fixture implied.
    fn always(entries: &[(&str, Ts)]) -> ExpiryHistory {
        ExpiryHistory::from_observations(entries.iter().map(|(symbol, expiry)| {
            (
                ContractSymbol::parse(symbol).expect("valid"),
                crate::continuous::expiry::ALWAYS_KNOWN,
                *expiry,
            )
        }))
        .expect("one statement per contract cannot disagree with itself")
    }

    // A clean roll. ESH4 leads on days 0 and 1; ESM4 out-trades it from day 2
    // onward, so with confirm_days = 1 the roll is decided on day 2. Both
    // contracts' day-2 buckets are available at day 2, 12:01, so
    // roll_ts = 2 * DAY + 12h + 1m. The gap is close(ESM4) - close(ESH4)
    // = 105 - 100 = +5 points = 5e9 nanopoints.
    #[test]
    fn a_clean_crossover_rolls_on_the_first_leading_session() {
        let series = chain_of(vec![
            (
                "ESH4",
                series("ESH4", 0, &[(100, 100), (100, 100), (10, 100)]),
            ),
            ("ESM4", series("ESM4", 0, &[(1, 105), (2, 105), (50, 105)])),
        ]);
        let table = build(&series, RollRule::default(), &no_expiries()).expect("build");
        assert_eq!(table.rows.len(), 1);
        let row = &table.rows[0];
        assert_eq!(row.from_contract, "ESH4");
        assert_eq!(row.to_contract, "ESM4");
        assert_eq!(row.roll_ts, Ts(2 * DAY + 12 * 3_600_000_000_000 + MIN));
        assert_eq!(row.adjustment, Price::from_points(5));
        assert_eq!(table.contracts, vec!["ESH4", "ESM4"]);
        assert_eq!(table.alias().as_str(), "ES.v.0");
    }

    // The no-lookahead claim, stated as an assertion: the roll instant is the
    // availability of the deciding session's last bar, never its ts_open, and
    // never earlier than either contract's data.
    #[test]
    fn the_roll_instant_is_never_before_the_data_that_decided_it() {
        let front = series("ESH4", 0, &[(100, 100), (10, 100)]);
        let back = series("ESM4", 0, &[(1, 105), (50, 105)]);
        let series = chain_of(vec![("ESH4", front.clone()), ("ESM4", back.clone())]);
        let table = build(&series, RollRule::default(), &no_expiries()).expect("build");
        let roll_ts = table.rows[0].roll_ts;
        let deciding_day = 1;
        let f = front.session(deciding_day).expect("front session");
        let b = back.session(deciding_day).expect("next session");
        assert_eq!(roll_ts, Ts(f.avail_ts.0.max(b.avail_ts.0)));
        assert!(roll_ts >= f.avail_ts && roll_ts >= b.avail_ts);
        // And strictly after every ts_open it consumed: a bar opening at
        // 12:00 on the deciding day is only knowable at 12:01.
        assert!(roll_ts.0 > deciding_day * DAY + 12 * 3_600_000_000_000);
    }

    // Volume crossing back and forth is exactly what confirm_days is for.
    // ESM4 leads on day 1, loses on day 2, then leads on days 3 and 4.
    //   confirm_days = 1 -> rolls on day 1
    //   confirm_days = 2 -> day 1's streak is broken on day 2, so the first
    //                       two-in-a-row ends on day 4
    #[test]
    fn confirm_days_requires_consecutive_sessions() {
        let series = chain_of(vec![
            (
                "ESH4",
                series(
                    "ESH4",
                    0,
                    &[(100, 100), (10, 100), (100, 100), (10, 100), (10, 100)],
                ),
            ),
            (
                "ESM4",
                series(
                    "ESM4",
                    0,
                    &[(1, 105), (50, 105), (1, 105), (50, 105), (50, 105)],
                ),
            ),
        ]);
        let one = build(
            &series,
            RollRule::VolumeCrossover { confirm_days: 1 },
            &no_expiries(),
        )
        .expect("build");
        assert_eq!(one.rows[0].roll_ts, Ts(DAY + 12 * 3_600_000_000_000 + MIN));

        let two = build(
            &series,
            RollRule::VolumeCrossover { confirm_days: 2 },
            &no_expiries(),
        )
        .expect("build");
        assert_eq!(
            two.rows[0].roll_ts,
            Ts(4 * DAY + 12 * 3_600_000_000_000 + MIN)
        );
    }

    // A day the next contract did not trade is zero volume, not a skipped
    // day: otherwise a holiday could confirm a roll nobody observed.
    #[test]
    fn a_missing_session_breaks_the_streak() {
        let mut back = series("ESM4", 0, &[(50, 105)]);
        // Day 1 absent, day 2 present.
        back.sessions.retain(|s| s.day != 1);
        let mut later = series("ESM4", 2, &[(50, 105), (50, 105)]);
        back.sessions.append(&mut later.sessions);
        back.last_ts_open = later.last_ts_open;
        let series = chain_of(vec![
            (
                "ESH4",
                series("ESH4", 0, &[(1, 100), (1, 100), (1, 100), (1, 100)]),
            ),
            ("ESM4", back),
        ]);
        let table = build(
            &series,
            RollRule::VolumeCrossover { confirm_days: 2 },
            &no_expiries(),
        )
        .expect("build");
        // Day 0 leads, day 1 is missing (zero) and breaks it, days 2 and 3
        // lead: the two-in-a-row completes on day 3.
        assert_eq!(
            table.rows[0].roll_ts,
            Ts(3 * DAY + 12 * 3_600_000_000_000 + MIN)
        );
    }

    // Two rolls, so the cumulative adjustment has something to accumulate.
    // Gaps: ESM4 - ESH4 = 105 - 100 = +5; ESU4 - ESM4 = 102 - 105 = -3.
    #[test]
    fn a_three_contract_chain_records_both_gaps() {
        let series = chain_of(vec![
            ("ESH4", series("ESH4", 0, &[(100, 100), (10, 100)])),
            (
                "ESM4",
                series("ESM4", 0, &[(1, 105), (50, 105), (100, 105), (10, 105)]),
            ),
            ("ESU4", series("ESU4", 2, &[(1, 102), (50, 102)])),
        ]);
        let table = build(&series, RollRule::default(), &no_expiries()).expect("build");
        assert_eq!(table.contracts, vec!["ESH4", "ESM4", "ESU4"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].adjustment, Price::from_points(5));
        assert_eq!(table.rows[1].adjustment, Price::from_points(-3));
        assert!(table.rows[0].roll_ts < table.rows[1].roll_ts);
    }

    // The backstop: the rule never fires, but a contract that expires must
    // still be left, and the last common session is the last instant the gap
    // is observable.
    #[test]
    fn a_rule_that_never_fires_falls_back_to_the_last_common_session() {
        let series = chain_of(vec![
            ("ESH4", series("ESH4", 0, &[(100, 100), (100, 100)])),
            ("ESM4", series("ESM4", 0, &[(1, 105), (1, 105), (1, 105)])),
        ]);
        let table = build(&series, RollRule::default(), &no_expiries()).expect("build");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(
            table.rows[0].roll_ts,
            Ts(DAY + 12 * 3_600_000_000_000 + MIN)
        );
    }

    // The precondition on the backstop, and the case a one-month archive
    // actually hits: ESH4 out-trades ESM4 every session and is still trading
    // when the data ends, so there is no evidence it ever handed over. The
    // chain is one contract long and the span is ESH4's, not ESM4's.
    #[test]
    fn a_front_contract_still_trading_at_the_end_ends_the_chain() {
        let series = chain_of(vec![
            (
                "ESH4",
                series("ESH4", 0, &[(100, 100), (100, 100), (100, 100)]),
            ),
            ("ESM4", series("ESM4", 0, &[(1, 105), (1, 105)])),
        ]);
        let table = build(&series, RollRule::default(), &no_expiries()).expect("build");
        assert_eq!(table.contracts, vec!["ESH4"]);
        assert!(table.rows.is_empty());
        assert_eq!(
            table.last_ts_open,
            Ts(2 * DAY + 12 * 3_600_000_000_000),
            "the span describes the chain, not every contract that traded"
        );
    }

    #[test]
    fn contracts_that_never_overlap_refuse_to_be_stitched() {
        let series = chain_of(vec![
            ("ESH4", series("ESH4", 0, &[(100, 100)])),
            ("ESM4", series("ESM4", 5, &[(100, 105)])),
        ]);
        let err = build(&series, RollRule::default(), &no_expiries())
            .expect_err("must refuse to invent a price");
        assert!(matches!(err, ContinuousError::NoOverlap { .. }), "{err}");
    }

    // Calendar rule: ESH4 expires on day 10, so `days = 8` makes the roll due
    // from day 2, and day 2 is the first session both contracts traded on or
    // after that.
    #[test]
    fn a_calendar_rule_rolls_at_the_first_session_on_or_after_the_due_day() {
        let series = chain_of(vec![
            (
                "ESH4",
                series("ESH4", 0, &[(100, 100), (100, 100), (100, 100), (100, 100)]),
            ),
            (
                "ESM4",
                series("ESM4", 0, &[(1, 105), (1, 105), (1, 105), (1, 105)]),
            ),
        ]);
        let expiries = always(&[("ESH4", Ts(10 * DAY))]);
        let table = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &expiries,
        )
        .expect("build");
        assert_eq!(
            table.rows[0].roll_ts,
            Ts(2 * DAY + 12 * 3_600_000_000_000 + MIN)
        );
        assert_eq!(table.alias().as_str(), "ES.c.0");
    }

    // ------------------------------------ the availability rule (D-0090)

    /// Four sessions of both contracts, so a calendar rule has somewhere to
    /// land. Day 0..3, one bar each, available at 12:01.
    fn four_sessions() -> BTreeMap<ContractSymbol, ContractSeries> {
        chain_of(vec![
            (
                "ESH4",
                series("ESH4", 0, &[(100, 100), (100, 100), (100, 100), (100, 100)]),
            ),
            (
                "ESM4",
                series("ESM4", 0, &[(1, 105), (1, 105), (1, 105), (1, 105)]),
            ),
        ])
    }

    /// The decision instant of session `day` in [`four_sessions`]: both
    /// contracts' buckets close at 12:01.
    fn decision_ts(day: i64) -> Ts {
        Ts(day * DAY + 12 * 3_600_000_000_000 + MIN)
    }

    /// One nanosecond before `ts` — a statement that is knowable *just* in time.
    fn before(ts: Ts) -> Ts {
        Ts(ts.0 - 1)
    }

    /// A history for ESH4 from `(ts_recv, expiry)` pairs.
    fn esh4_history(records: &[(Ts, Ts)]) -> ExpiryHistory {
        ExpiryHistory::from_observations(records.iter().map(|(recv, expiry)| {
            (
                ContractSymbol::parse("ESH4").expect("valid"),
                *recv,
                *expiry,
            )
        }))
        .expect("orderable")
    }

    // ======================================================================
    // THE TRAP CONTROL, at the level that matters — a roll DATE. The vendor
    // states ESH4 expiring on day 10, then restates it on day 9 as expiring on
    // day 9 instead: later record, earlier expiry, the direction all four real
    // cases take. With `days = 8`:
    //   max(ts_recv)     -> expiry day 9  -> due day 1 -> roll on day 1
    //   max(expiration)  -> expiry day 10 -> due day 2 -> roll on day 2
    // An implementation keyed on the expiry moves this roll a session late, and
    // that is exactly what would have happened to ZN and 6E.
    // ======================================================================
    #[test]
    fn a_restatement_moves_the_calendar_roll_and_the_key_is_ts_recv() {
        let series = four_sessions();
        let history = esh4_history(&[
            // Said on day 0: expires day 10.
            (before(decision_ts(0)), Ts(10 * DAY)),
            // Restated during day 1, before that session's decision instant:
            // expires day 9 after all.
            (before(decision_ts(1)), Ts(9 * DAY)),
        ]);
        let table = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &history,
        )
        .expect("build");
        assert_eq!(
            table.rows[0].roll_ts,
            decision_ts(1),
            "the restated (earlier) expiry was knowable, so the roll is due a \
             session earlier — max(ts_recv)"
        );
        assert_ne!(
            table.rows[0].roll_ts,
            decision_ts(2),
            "day 2 is what max(expiration) would give: the stale record's due day"
        );
    }

    // The availability filter, biting at roll level. Same fixture, but the
    // correction lands *after* every session it would have moved: the roll must
    // be decided on the pre-correction expiry, because a backtest standing there
    // could not have read the restatement. Plain latest-wins would silently roll
    // on day 1 — one session of lookahead, invisible on an adjusted chart.
    #[test]
    fn a_correction_landing_after_its_own_roll_does_not_move_it() {
        let series = four_sessions();
        let history = esh4_history(&[
            (before(decision_ts(0)), Ts(10 * DAY)),
            // Published a year later. Real, true, and unreadable at the roll.
            (Ts(400 * DAY), Ts(9 * DAY)),
        ]);
        let table = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &history,
        )
        .expect("build");
        assert_eq!(
            table.rows[0].roll_ts,
            decision_ts(2),
            "the roll used the only expiry it could have known"
        );

        // And the detector says so, rather than the omission being invisible.
        let late =
            late_expiry_corrections(&table, &history, DecadeAnchor::DEFAULT).expect("parses");
        assert_eq!(late.len(), 1, "{late:?}");
        assert_eq!(late[0].contract, "ESH4");
        assert_eq!(late[0].used, Ts(10 * DAY));
        assert_eq!(late[0].latest, Ts(9 * DAY));
        assert_eq!(late[0].corrected_avail_ts, Ts(400 * DAY));
        assert!(late[0].corrected_avail_ts > late[0].roll_ts);
    }

    // The same detector on the same shape of data, with the correction landing
    // BEFORE the roll: nothing was withheld, so nothing is reported. A detector
    // that fired on every restatement would be noise; one that never fires is
    // decoration (CLAUDE.md §7).
    #[test]
    fn a_correction_landing_before_its_own_roll_is_not_reported_as_late() {
        let series = four_sessions();
        let history = esh4_history(&[
            (before(decision_ts(0)), Ts(10 * DAY)),
            (before(decision_ts(1)), Ts(9 * DAY)),
        ]);
        let table = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &history,
        )
        .expect("build");
        let late =
            late_expiry_corrections(&table, &history, DecadeAnchor::DEFAULT).expect("parses");
        assert!(late.is_empty(), "{late:?}");
    }

    // A session whose decision instant precedes every statement is not due —
    // the rule cannot fire on an expiry nobody had published yet — so the scan
    // continues rather than refusing or guessing. Here the expiry only becomes
    // knowable during day 2, and day 2 is where the roll lands even though the
    // due day (1) had already passed.
    #[test]
    fn a_rule_cannot_fire_on_an_expiry_that_was_not_published_yet() {
        let series = four_sessions();
        let history = esh4_history(&[(decision_ts(2), Ts(9 * DAY))]);
        let table = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &history,
        )
        .expect("build");
        assert_eq!(
            table.rows[0].roll_ts,
            decision_ts(2),
            "overdue the instant it became knowable, and not before"
        );
    }

    // The fixed point, exercised where it could plausibly wobble: the expiry is
    // restated three times, twice moving the due day back past sessions already
    // scanned. The answer is the earliest session that is due *by its own*
    // decision instant — day 3, since day 0 saw expiry day 20 (due 12), days 1
    // and 2 saw expiry day 30 (due 22), and only day 3 sees expiry day 10
    // (due 2). One ascending pass decides it; there is nothing to iterate.
    #[test]
    fn the_calendar_fixed_point_is_the_first_self_consistent_session() {
        let series = four_sessions();
        let history = esh4_history(&[
            (before(decision_ts(0)), Ts(20 * DAY)),
            (before(decision_ts(1)), Ts(30 * DAY)),
            (before(decision_ts(3)), Ts(10 * DAY)),
        ]);
        let table = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &history,
        )
        .expect("build");
        assert_eq!(table.rows[0].roll_ts, decision_ts(3));
        // And the solution really is self-consistent: the expiry knowable at
        // the roll instant is the one whose due day the roll satisfies.
        let esh4 = ContractSymbol::parse("ESH4").expect("valid");
        let used = history
            .as_of(&esh4, table.rows[0].roll_ts)
            .expect("known by then");
        assert_eq!(used, Ts(10 * DAY));
    }

    // ======================================================================
    // The `.v` control. `build_roll_table` must read `expiries` only in the
    // calendar arm, so a volume table is byte-identical under any history —
    // including an absurd one. That is what makes it safe for the CLI to skip
    // resolving expiries entirely for `.v`, which is what stopped `rolls --root
    // GC` exiting 4 for an input it never read (D-0090).
    // ======================================================================
    #[test]
    fn the_volume_rule_builds_the_same_table_under_any_expiry_history() {
        let series = four_sessions();
        let rule = RollRule::VolumeCrossover { confirm_days: 1 };
        assert!(!rule.reads_expiries());

        let none = build(&series, rule, &no_expiries()).expect("build");
        let sane = build(&series, rule, &esh4_history(&[(Ts(0), Ts(10 * DAY))])).expect("build");
        let absurd = build(
            &series,
            rule,
            &esh4_history(&[
                (Ts(i64::MIN / 2), Ts(-500 * DAY)),
                (Ts(i64::MAX / 2), Ts(50_000 * DAY)),
            ]),
        )
        .expect("build");

        assert_eq!(none.rows, sane.rows);
        assert_eq!(none.rows, absurd.rows);
        assert_eq!(none.contracts, absurd.contracts);
        // And a late-correction report is meaningless for a rule that read
        // nothing, so it is empty rather than invented.
        assert!(
            late_expiry_corrections(
                &absurd,
                &esh4_history(&[(Ts(0), Ts(10 * DAY))]),
                DecadeAnchor::DEFAULT
            )
            .expect("parses")
            .is_empty()
        );
    }

    // The other half of that claim: the calendar rule DOES read them, so the
    // same three histories must not all agree. Without this the control above
    // would pass on a build that ignored expiries everywhere.
    #[test]
    fn the_calendar_rule_does_read_the_history_it_is_given() {
        let series = four_sessions();
        let rule = RollRule::CalendarDaysBeforeExpiry { days: 8 };
        assert!(rule.reads_expiries());
        let early = build(&series, rule, &esh4_history(&[(Ts(0), Ts(9 * DAY))])).expect("build");
        let late = build(&series, rule, &esh4_history(&[(Ts(0), Ts(11 * DAY))])).expect("build");
        assert_ne!(early.rows[0].roll_ts, late.rows[0].roll_ts);
    }

    #[test]
    fn a_calendar_rule_without_an_expiry_refuses() {
        let series = chain_of(vec![
            ("ESH4", series("ESH4", 0, &[(100, 100), (100, 100)])),
            ("ESM4", series("ESM4", 0, &[(1, 105), (1, 105)])),
        ]);
        let err = build(
            &series,
            RollRule::CalendarDaysBeforeExpiry { days: 8 },
            &no_expiries(),
        )
        .expect_err("must refuse to guess an expiry");
        assert!(
            matches!(err, ContinuousError::MissingExpiry { .. }),
            "{err}"
        );
    }

    #[test]
    fn zero_confirm_days_is_refused() {
        let err = RollRule::VolumeCrossover { confirm_days: 0 }
            .validate()
            .expect_err("must refuse to roll on no evidence");
        assert!(matches!(err, ContinuousError::InvalidRule { .. }), "{err}");
    }

    #[test]
    fn a_root_with_no_curated_bars_says_how_to_get_them() {
        let empty = BTreeMap::new();
        let err =
            build(&empty, RollRule::default(), &no_expiries()).expect_err("nothing to stitch");
        assert!(matches!(err, ContinuousError::NoSeries { .. }), "{err}");
        assert!(err.to_string().contains("crucible transcode"), "{err}");
    }

    // ------------------------------------------------------------ round trip

    fn sample_table() -> RollTable {
        RollTable {
            schema_version: ROLL_TABLE_SCHEMA_VERSION,
            root: "ES".to_owned(),
            tf: TimeFrame::M1,
            rule: RollRule::VolumeCrossover { confirm_days: 1 },
            decade_anchor: 2025,
            expiry_source: "none".to_owned(),
            first_ts_open: Ts(0),
            last_ts_open: Ts(3 * DAY),
            contracts: vec!["ESH4".to_owned(), "ESM4".to_owned()],
            sources: vec![RollSource {
                instrument: "ESH4".to_owned(),
                source_file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_owned(),
                source_file_blake3: "00".repeat(32),
                transcoder_version: 1,
                bars: 42,
            }],
            rows: vec![RollRow {
                from_contract: "ESH4".to_owned(),
                to_contract: "ESM4".to_owned(),
                roll_ts: Ts(2 * DAY),
                adjustment: Price::from_points(5),
            }],
        }
    }

    #[test]
    fn a_table_survives_a_json_round_trip_on_disk() {
        let dir = TempDir::new();
        let table = sample_table();
        let path = write_roll_table(dir.path(), &table).expect("write");
        assert!(
            path.ends_with("curated/rolls/ES/1m/v-confirm1.json"),
            "{}",
            path.display()
        );
        assert_eq!(read_roll_table(&path).expect("read"), table);
    }

    // The wire format is part of the contract: a table written today must be
    // readable by the build that reads it next month, and the rule's tagging
    // is the part most likely to be "improved" by accident.
    #[test]
    fn the_rule_is_externally_tagged_and_denies_unknown_knobs() {
        let text = serde_json::to_string(&sample_table()).expect("serialize");
        assert!(
            text.contains(r#""rule":{"volume_crossover":{"confirm_days":1}}"#),
            "{text}"
        );
        let typo = text.replace("confirm_days", "confirm_dayz");
        assert!(
            serde_json::from_str::<RollTable>(&typo).is_err(),
            "a misspelled knob must be a hard error, never a silent default"
        );
    }

    #[test]
    fn an_unknown_schema_version_refuses_rather_than_guessing() {
        let dir = TempDir::new();
        let mut table = sample_table();
        table.schema_version = ROLL_TABLE_SCHEMA_VERSION + 1;
        let path = roll_table_path(dir.path(), "ES", TimeFrame::M1, &table.rule).expect("path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &path,
            serde_json::to_string(&table).expect("serialize").as_bytes(),
        )
        .expect("write");
        let err = read_roll_table(&path).expect_err("must refuse a future format");
        assert!(
            matches!(err, ContinuousError::UnknownTableVersion { .. }),
            "{err}"
        );
    }

    // Negative controls for validate(): a hand-edited table gets no more
    // trust than a caller (catalog's rule, applied here).
    #[test]
    fn a_self_contradicting_table_refuses() {
        /// One hand-edit to a valid table, applied in place.
        type Mutation = fn(&mut RollTable);
        let mutations: [(&str, Mutation); 4] = [
            ("broken chain", |t| {
                t.rows[0].to_contract = "ESU4".to_owned();
            }),
            ("contract count", |t| {
                t.contracts.push("ESU4".to_owned());
            }),
            ("backwards delivery", |t| {
                t.contracts = vec!["ESM4".to_owned(), "ESH4".to_owned()];
                t.rows[0].from_contract = "ESM4".to_owned();
                t.rows[0].to_contract = "ESH4".to_owned();
            }),
            ("backwards span", |t| {
                t.first_ts_open = Ts(10);
                t.last_ts_open = Ts(1);
            }),
        ];
        for (name, mutate) in mutations {
            let mut table = sample_table();
            mutate(&mut table);
            let err = table.validate().expect_err(name);
            assert!(
                matches!(err, ContinuousError::MalformedTable { .. }),
                "{name}: {err}"
            );
        }
    }

    #[test]
    fn rolls_that_do_not_advance_refuse() {
        let mut table = sample_table();
        table.contracts.push("ESU4".to_owned());
        table.rows.push(RollRow {
            from_contract: "ESM4".to_owned(),
            to_contract: "ESU4".to_owned(),
            roll_ts: table.rows[0].roll_ts,
            adjustment: Price::ZERO,
        });
        let err = table.validate().expect_err("two rolls at one instant");
        assert!(
            matches!(err, ContinuousError::MalformedTable { .. }),
            "{err}"
        );
    }

    #[test]
    fn a_table_mixing_roots_refuses() {
        let mut table = sample_table();
        table.contracts[1] = "NQM4".to_owned();
        table.rows[0].to_contract = "NQM4".to_owned();
        let err = table.validate().expect_err("one table, one root");
        assert!(matches!(err, ContinuousError::RootMismatch { .. }), "{err}");
    }
}
