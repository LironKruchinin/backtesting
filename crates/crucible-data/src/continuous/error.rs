//! Failure modes of building, storing, and replaying a continuous series.
//!
//! Hand-rolled per CLAUDE.md §5.1, in the same shape as
//! [`CuratedError`](crate::curated::CuratedError): struct variants carrying
//! what a caller needs to act, a manual `Display` stating the consequence and
//! the remedy, and `source()` for wrapped causes.
//!
//! The organising principle is `curated`'s: **every uncertainty is a
//! refusal.** A roll table is derived and rebuildable, so a false refusal
//! costs one `crucible rolls`; a false proceed stitches two contracts at a
//! price nobody traded and puts the result in a research log.

use std::path::PathBuf;

use crucible_core::types::{TimeFrame, Ts};

use crate::curated::CuratedError;

/// One contract whose expiry statements cannot be put in availability order.
///
/// Two statements about the same contract are a **revision** when their
/// availability windows are disjoint — the vendor said one thing, then said
/// another, and `max(ts_recv)` names which is current (D-0090). They are a
/// **conflict** when the windows overlap, because then there is no "latest": at
/// some availability instant the source asserts both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiryDisagreement {
    /// The contract, as this build spells it.
    pub contract: String,
    /// The earlier-starting statement's expiry.
    pub first: Ts,
    /// The window of availability instants over which `first` was stated.
    pub first_avail: (Ts, Ts),
    /// The later-starting statement's expiry.
    pub second: Ts,
    /// The window of availability instants over which `second` was stated.
    pub second_avail: (Ts, Ts),
}

/// Why a continuous series could not be built, stored, or replayed.
#[derive(Debug)]
pub enum ContinuousError {
    /// A curated read or write failed.
    Curated(CuratedError),
    /// A filesystem operation failed.
    Io {
        /// Path being operated on.
        path: PathBuf,
        /// What was being attempted, for the message.
        during: &'static str,
        /// Underlying failure.
        source: std::io::Error,
    },
    /// A roll table could not be serialized or parsed.
    Json {
        /// Path being read or written.
        path: PathBuf,
        /// What was being attempted, for the message.
        during: &'static str,
        /// Explanation from `serde_json`.
        detail: String,
    },
    /// A string is not a CME contract symbol this build can order.
    UnparseableSymbol {
        /// The offending text.
        symbol: String,
        /// Which rule it broke.
        reason: &'static str,
    },
    /// Two symbols that must share a root do not.
    RootMismatch {
        /// Root the caller asked for.
        expected: String,
        /// Root the symbol actually carries.
        found: String,
        /// The offending symbol.
        symbol: String,
    },
    /// A rule parameter makes no sense.
    InvalidRule {
        /// What was wrong, phrased for an operator.
        detail: String,
    },
    /// No contract in the input had a single bar, so there is nothing to
    /// stitch.
    NoSeries {
        /// Root that was asked for.
        root: String,
        /// Interval that was asked for.
        tf: TimeFrame,
    },
    /// Two adjacent contracts never traded on a common session, so the price
    /// gap between them was never observable.
    NoOverlap {
        /// Contract being left.
        from: String,
        /// Contract being entered.
        to: String,
    },
    /// A calendar rule needs an expiry the expiry map does not carry.
    MissingExpiry {
        /// Contract with no expiry.
        contract: String,
    },
    /// A roll table declares a schema version this build does not understand.
    UnknownTableVersion {
        /// Path being read.
        path: PathBuf,
        /// Version recorded in the file.
        found: u32,
        /// Version this build understands.
        expected: u32,
    },
    /// A roll table contradicts itself.
    MalformedTable {
        /// Which invariant failed, and what was found instead.
        detail: String,
    },
    /// A contract the roll table names has no curated bars.
    SegmentMissing {
        /// The contract that is front for that segment.
        contract: String,
        /// Why the curated store could not produce it.
        source: CuratedError,
    },
    /// The requested replay window is not the window this table covers.
    RangeNotCovered {
        /// Requested start (inclusive), on `ts_open`.
        requested_start_ts: Ts,
        /// Requested end (exclusive), on `ts_open`.
        requested_end_ts: Ts,
        /// First `ts_open` the table was built from.
        covered_first_ts: Ts,
        /// Last `ts_open` the table was built from.
        covered_last_ts: Ts,
    },
    /// The stitched series holds no bars.
    EmptySeries {
        /// Continuous alias that was asked for.
        alias: String,
        /// Interval that was asked for.
        tf: TimeFrame,
    },
    /// Bars are not in strictly increasing `ts_open` order once stitched.
    OutOfOrderSegments {
        /// Contract whose first bar broke the order.
        contract: String,
        /// The `ts_open` that came first.
        prev: Ts,
        /// The `ts_open` that failed to exceed it.
        next: Ts,
    },
    /// A raw `definition` file could not be opened or decoded.
    Undecodable {
        /// Path being read.
        path: PathBuf,
        /// Explanation from the decoder.
        detail: String,
    },
    /// A definition record states an expiry but not **when** that statement
    /// became knowable, so §2.1's first question — "as known when?" — has no
    /// answer for it.
    ///
    /// The vendor's `ts_recv` is that answer (D-0090). A record missing it
    /// cannot be placed in a revision history, and placing it anyway would mean
    /// guessing whether a roll could have seen it.
    UnavailableExpiry {
        /// The contract the record names.
        contract: String,
        /// The expiry it states.
        expiration: Ts,
    },
    /// One or more contracts are stated to expire at two different instants
    /// over **overlapping** availability windows, so there is no latest
    /// statement to take (D-0090).
    ///
    /// Every offending contract in the root is listed, never the first one
    /// found: a refusal that reports one of two makes the archive look better
    /// than it is, which is exactly how `ZNM2012` sat hidden behind `ZNZ2011`.
    ExpiryConflict {
        /// Every contract whose revisions cannot be ordered, in symbol order.
        conflicts: Vec<ExpiryDisagreement>,
    },
    /// Two definition records for one contract expire in different *years*, so
    /// the one-digit year cannot be resolved from the record at all.
    ///
    /// Asks a coarser question than [`ContinuousError::ExpiryConflict`] on
    /// purpose: an hour's disagreement about when `GCX2021` settled does not
    /// make it a different contract, and the real archive carries such
    /// disagreements (D-0072). A year's does. The cycle reader also has no
    /// decision instant to filter against — a curated partition key is an
    /// archival identity, not a choice made at a point in time — so it reads the
    /// observed span where the roll reader reads the availability history
    /// (D-0090).
    ExpiryYearConflict {
        /// The contract as the vendor spells it.
        contract: String,
        /// Earliest expiry recorded for it.
        earliest: Ts,
        /// Latest expiry recorded for it.
        latest: Ts,
    },
    /// Two contracts a one-digit year code cannot tell apart are less than a
    /// decade apart, so they are not the ten-year repeat that code describes.
    ContractCycleCollision {
        /// The earlier contract.
        first: String,
        /// The later one.
        second: String,
        /// Days between their expiries.
        apart_days: i64,
    },
    /// A vendor symbol and a timestamp did not name one contract.
    ///
    /// The refusal that keeps two contract cycles out of one curated partition
    /// (D-0072). Carries the candidates so an operator can see *why* the answer
    /// was not unique rather than being told it was not.
    UnresolvedContract {
        /// The vendor symbol, as written.
        symbol: String,
        /// The bar timestamp the symbol had to be resolved at.
        ts: Ts,
        /// Contracts of this root, month and year digit that the archive's
        /// `definition` file knows, with the expiry of each. Empty means the
        /// definition file named none at all.
        candidates: Vec<(String, Ts)>,
    },
    /// A string is not a continuous alias in the form §4 pins.
    NotAnAlias {
        /// The offending text.
        text: String,
        /// Which part of the form it broke.
        reason: String,
    },
    /// No roll table exists for a root, interval and rule letter.
    NoRollTable {
        /// The alias that was asked for.
        alias: String,
        /// Directory searched.
        dir: PathBuf,
        /// Rule letter that found nothing.
        rule_letter: char,
    },
    /// More than one stored rule answers to one alias letter.
    ///
    /// `ES.v.0` names "the volume rule", not its `confirm_days`, so an archive
    /// holding `v-confirm1.json` *and* `v-confirm3.json` has two answers to one
    /// question. Same refusal D-0029 and D-0072 make: two candidates stop the
    /// run rather than one being adopted by sort order.
    AmbiguousRollTable {
        /// The alias that was asked for.
        alias: String,
        /// Every stored table that matches, by file stem, sorted.
        candidates: Vec<String>,
    },
}

impl core::fmt::Display for ContinuousError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ContinuousError::Curated(e) => write!(f, "{e}"),
            ContinuousError::Io {
                path,
                during,
                source,
            } => write!(f, "{during} {}: {source}", path.display()),
            ContinuousError::Json {
                path,
                during,
                detail,
            } => write!(f, "{during} {}: {detail}", path.display()),
            ContinuousError::UnparseableSymbol { symbol, reason } => write!(
                f,
                "{symbol:?} is not a contract symbol this build can order: {reason}. \
                 Ordering contracts is how a roll table decides what follows what, \
                 so a symbol it cannot place is refused rather than sorted by text"
            ),
            ContinuousError::RootMismatch {
                expected,
                found,
                symbol,
            } => write!(
                f,
                "{symbol:?} has root {found:?}, not {expected:?}; one roll table \
                 stitches one root, and mixing them would splice unrelated markets"
            ),
            ContinuousError::InvalidRule { detail } => write!(f, "invalid roll rule: {detail}"),
            ContinuousError::NoSeries { root, tf } => write!(
                f,
                "no curated {tf} bars for any {root} contract, so there is nothing to \
                 stitch. Build them first:\n\x20      crucible transcode"
            ),
            ContinuousError::NoOverlap { from, to } => write!(
                f,
                "{from} and {to} never traded on a common session, so the price gap \
                 between them was never observable. Stitching them would require \
                 inventing a price; refusing instead"
            ),
            ContinuousError::MissingExpiry { contract } => write!(
                f,
                "no expiry known for {contract}, and a calendar rule is defined \
                 entirely in terms of one. Import expiries from the `definition` \
                 schema, or use the volume-crossover rule, which needs none"
            ),
            ContinuousError::UnknownTableVersion {
                path,
                found,
                expected,
            } => write!(
                f,
                "{} declares roll-table schema v{found}; this build understands \
                 v{expected}. A roll table is derived and disposable — rebuild it:\n\
                 \x20      crucible rolls --write",
                path.display()
            ),
            ContinuousError::MalformedTable { detail } => write!(
                f,
                "roll table is self-contradictory: {detail}. A table whose rows do not \
                 describe one chain of contracts cannot say which contract was front \
                 when, which is the only question it exists to answer"
            ),
            ContinuousError::SegmentMissing { contract, source } => write!(
                f,
                "the roll table says {contract} was the front contract for one segment, \
                 but its bars are not in the curated store ({source}). Replaying the \
                 rest would silently drop that stretch of the series"
            ),
            ContinuousError::RangeNotCovered {
                requested_start_ts,
                requested_end_ts,
                covered_first_ts,
                covered_last_ts,
            } => write!(
                f,
                "this roll table was built from bars spanning [{covered_first_ts}, \
                 {covered_last_ts}] but the replay asks for [{requested_start_ts}, \
                 {requested_end_ts}). Outside the table's span there is no roll \
                 information, so the series would quietly be missing contracts \
                 rather than short of data. Rebuild the table over the wider window"
            ),
            ContinuousError::EmptySeries { alias, tf } => write!(
                f,
                "no {tf} bars for {alias} in the requested window. An empty backtest \
                 reports zero trades, which reads as \"the strategy did nothing\" \
                 rather than \"there was nothing to trade\""
            ),
            ContinuousError::OutOfOrderSegments {
                contract,
                prev,
                next,
            } => write!(
                f,
                "stitching {contract} put ts_open {next} after {prev}. Segments must \
                 concatenate in time: replaying them in this order would let the \
                 engine see an interval twice, or out of sequence"
            ),
            ContinuousError::Undecodable { path, detail } => {
                write!(f, "could not decode {}: {detail}", path.display())
            }
            ContinuousError::UnavailableExpiry {
                contract,
                expiration,
            } => write!(
                f,
                "a definition record says {contract} expires at {expiration} but carries \
                 no ts_recv, so there is no answer to \"as known when?\" (§2.1). An \
                 expiry whose availability is unknown cannot be filtered against a roll's \
                 decision instant, and using it anyway would be a guess about what a \
                 backtest could have seen"
            ),
            ContinuousError::ExpiryConflict { conflicts } => {
                write!(
                    f,
                    "{} contract(s) are stated to expire at two instants over \
                     OVERLAPPING availability windows, so neither statement is the \
                     latest and there is nothing for max(ts_recv) to pick:",
                    conflicts.len()
                )?;
                for c in conflicts {
                    write!(
                        f,
                        "\n\x20     {} expires {} (stated {}..{}) and also {} (stated \
                         {}..{})",
                        c.contract,
                        c.first,
                        c.first_avail.0,
                        c.first_avail.1,
                        c.second,
                        c.second_avail.0,
                        c.second_avail.1
                    )?;
                }
                write!(
                    f,
                    "\n\x20  Every roll decided from such a contract depends on which is \
                     true at the deciding instant, so picking one would make the table \
                     unreproducible. Disjoint windows are a revision and resolve fine; \
                     these overlap"
                )
            }
            ContinuousError::ExpiryYearConflict {
                contract,
                earliest,
                latest,
            } => write!(
                f,
                "{contract} is defined expiring at {earliest} and also at {latest}, \
                 which fall in different years. The year is what resolves a one-digit \
                 contract code, so the definition file does not say which contract \
                 this is — and a partition key that names the wrong decade is the \
                 corruption this check exists to prevent"
            ),
            ContinuousError::ContractCycleCollision {
                first,
                second,
                apart_days,
            } => write!(
                f,
                "{first} and {second} share a one-digit year code but expire \
                 {apart_days} day(s) apart. That code repeats every ten years, so two \
                 contracts it cannot distinguish must be about a decade apart; these \
                 are not, which means the definition file dated at least one of them \
                 into the wrong decade. Resolving a bar against these would file it \
                 under a contract chosen by rounding"
            ),
            ContinuousError::UnresolvedContract {
                symbol,
                ts,
                candidates,
            } => {
                if candidates.is_empty() {
                    write!(
                        f,
                        "{symbol} has a one-digit year and the archive's `definition` \
                         file names no contract it could be. A one-digit year repeats \
                         every ten years, so without an expiry to anchor it there is \
                         no way to say which decade this bar belongs to. Acquire the \
                         `definition` schema for this root over the same span:\n\
                         \x20      crucible pull --schema definition --symbols <ROOT>.FUT …"
                    )
                } else {
                    write!(
                        f,
                        "{symbol} at {ts} matches no contract cycle. Known contracts \
                         for this root, month and year digit: {}. A bar outside every \
                         cycle means the definition file and the bar file disagree \
                         about what traded, and guessing would file it under a \
                         contract that was not trading",
                        candidates
                            .iter()
                            .map(|(name, expiry)| format!("{name} (expires {expiry})"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            ContinuousError::NotAnAlias { text, reason } => write!(
                f,
                "{text:?} is not a continuous alias: {reason}. CLAUDE.md §4 pins the \
                 spelling to {{root}}.{{v|c}}.0 — `ES.v.0` for the volume roll, \
                 `ES.c.0` for the calendar roll"
            ),
            ContinuousError::NoRollTable {
                alias,
                dir,
                rule_letter,
            } => write!(
                f,
                "no `{rule_letter}` roll table for {alias} in {}. A roll table is \
                 curated data — derived, disposable, and not built until asked \
                 for. Build it:\n\x20      crucible rolls --root <ROOT> --timeframe \
                 <TF>{} --write",
                dir.display(),
                if *rule_letter == 'c' {
                    " --calendar-days <N>"
                } else {
                    ""
                }
            ),
            ContinuousError::AmbiguousRollTable { alias, candidates } => write!(
                f,
                "{alias} names {} stored roll tables: {}. The alias says which *rule*, \
                 never its parameters, so two stored tables are two answers to one \
                 question and picking by sort order would put an unstated assumption \
                 under a research number. Delete the one you do not want, or name the \
                 contract chain directly",
                candidates.len(),
                candidates.join(", ")
            ),
        }
    }
}

impl std::error::Error for ContinuousError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ContinuousError::Curated(e) | ContinuousError::SegmentMissing { source: e, .. } => {
                Some(e)
            }
            ContinuousError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<CuratedError> for ContinuousError {
    fn from(e: CuratedError) -> ContinuousError {
        ContinuousError::Curated(e)
    }
}
