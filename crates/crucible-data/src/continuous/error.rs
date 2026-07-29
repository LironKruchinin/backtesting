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
    /// Two definition records give one contract two different expiries.
    ExpiryConflict {
        /// The contract with two answers.
        contract: String,
        /// The expiry recorded first.
        first: Ts,
        /// The expiry recorded second.
        second: Ts,
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
            ContinuousError::ExpiryConflict {
                contract,
                first,
                second,
            } => write!(
                f,
                "{contract} is defined with expiry {first} and also {second}. Every \
                 roll decided from that contract depends on which is true, so picking \
                 one would make the table unreproducible"
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
