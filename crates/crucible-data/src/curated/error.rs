//! Failure modes of reading and writing the curated store.
//!
//! Hand-rolled per CLAUDE.md §5.1, in the same shape as
//! [`DeliveryError`](crate::ingest::delivery::DeliveryError): struct variants
//! carrying what a caller needs to act, a manual `Display` that states the
//! consequence and the remedy, and `source()` for wrapped causes.
//!
//! The organising principle matches `ingest`'s: **every uncertainty is a
//! refusal.** There is no variant meaning "this file looked odd, replaying it
//! anyway". Curated data is disposable and rebuildable from `raw/`, so a
//! false refusal costs one `crucible transcode`; a false proceed puts numbers
//! in a research log that nobody can reproduce.

use std::path::PathBuf;

use crucible_core::types::{TimeFrame, Ts};

/// Why curated data could not be written, read, or trusted.
#[derive(Debug)]
pub enum CuratedError {
    /// A filesystem operation failed.
    Io {
        /// Path being operated on.
        path: PathBuf,
        /// What was being attempted, for the message.
        during: &'static str,
        /// Underlying failure.
        source: std::io::Error,
    },
    /// The Parquet layer rejected the file or the operation.
    Parquet {
        /// Path being read or written.
        path: PathBuf,
        /// What was being attempted, for the message.
        during: &'static str,
        /// Explanation from the Parquet implementation.
        detail: String,
    },
    /// An instrument identifier cannot be turned into a path component.
    InvalidInstrument {
        /// The offending identifier.
        instrument: String,
        /// Which rule it broke.
        reason: &'static str,
    },
    /// A directory name under `curated/bars/` is not a valid encoded
    /// instrument, so the partition cannot be attributed to anything.
    UndecodableInstrumentDir {
        /// The directory name as found on disk.
        dir_name: String,
    },
    /// The file carries no curated key-value metadata, or is missing a key.
    MissingMetadata {
        /// Path being read.
        path: PathBuf,
        /// The key that was absent.
        key: &'static str,
    },
    /// A metadata value is present but unusable.
    MalformedMetadata {
        /// Path being read.
        path: PathBuf,
        /// The key whose value could not be understood.
        key: &'static str,
        /// The value as stored.
        value: String,
    },
    /// The file was written by an incompatible version of the curated format.
    UnknownSchemaVersion {
        /// Path being read.
        path: PathBuf,
        /// Version recorded in the file.
        found: u32,
        /// Version this build understands.
        expected: u32,
    },
    /// The file's own metadata contradicts the partition path it lives under.
    PartitionMismatch {
        /// Path being read.
        path: PathBuf,
        /// What the path claims.
        path_says: String,
        /// What the file's metadata claims.
        file_says: String,
    },
    /// The file's metadata does not describe its own columns.
    MetadataContradictsColumns {
        /// Path being read.
        path: PathBuf,
        /// Which claim failed.
        claim: &'static str,
        /// What the metadata recorded.
        recorded: String,
        /// What the columns actually contain.
        actual: String,
    },
    /// Rows are not in strictly increasing `ts_open` order.
    ///
    /// Yielded from a feed this would be a lookahead-ordering violation, so
    /// it is caught at load rather than left for the engine's guard.
    OutOfOrderRows {
        /// Path being read, or the partition when the break is between files.
        path: PathBuf,
        /// The `ts_open` that came first.
        prev: Ts,
        /// The `ts_open` that failed to exceed it.
        next: Ts,
    },
    /// Two partition files claim overlapping `ts_open` ranges.
    OverlappingWindows {
        /// The earlier file.
        earlier: PathBuf,
        /// The file whose range starts inside it.
        later: PathBuf,
        /// Last `ts_open` of the earlier file.
        earlier_last_ts: Ts,
        /// First `ts_open` of the later file.
        later_first_ts: Ts,
    },
    /// A volume does not fit the storage type in one direction or the other.
    VolumeOutOfRange {
        /// Path being read or written.
        path: PathBuf,
        /// The offending value, rendered.
        value: String,
    },
    /// Nothing has been transcoded for this instrument and timeframe.
    NoCuratedData {
        /// Instrument that was asked for.
        instrument: String,
        /// Timeframe that was asked for.
        tf: TimeFrame,
        /// Partition directory that would hold it.
        dir: PathBuf,
    },
    /// Curated data exists, but none of it falls in the requested range.
    EmptyRange {
        /// Instrument that was asked for.
        instrument: String,
        /// Timeframe that was asked for.
        tf: TimeFrame,
        /// Requested start, inclusive.
        start_ts: Ts,
        /// Requested end, exclusive.
        end_ts: Ts,
        /// First `ts_open` actually held.
        held_first_ts: Ts,
        /// Last `ts_open` actually held.
        held_last_ts: Ts,
    },
}

impl core::fmt::Display for CuratedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CuratedError::Io {
                path,
                during,
                source,
            } => write!(f, "{during} {}: {source}", path.display()),
            CuratedError::Parquet {
                path,
                during,
                detail,
            } => write!(f, "{during} {}: {detail}", path.display()),
            CuratedError::InvalidInstrument { instrument, reason } => write!(
                f,
                "instrument {instrument:?} cannot name a curated partition: {reason}"
            ),
            CuratedError::UndecodableInstrumentDir { dir_name } => write!(
                f,
                "curated/bars/{dir_name} is not a percent-encoded instrument; \
                 refusing to guess which instrument its bars belong to"
            ),
            CuratedError::MissingMetadata { path, key } => write!(
                f,
                "{} carries no {key} — it was not written by this tool, or was \
                 truncated before its footer. Rebuild it: crucible transcode --force",
                path.display()
            ),
            CuratedError::MalformedMetadata { path, key, value } => write!(
                f,
                "{} records {key} = {value:?}, which is not a usable value",
                path.display()
            ),
            CuratedError::UnknownSchemaVersion {
                path,
                found,
                expected,
            } => write!(
                f,
                "{} uses curated schema v{found}; this build understands v{expected}. \
                 Curated data is disposable — rebuild it from raw: \
                 crucible transcode --force",
                path.display()
            ),
            CuratedError::PartitionMismatch {
                path,
                path_says,
                file_says,
            } => write!(
                f,
                "{} is filed under {path_says} but its metadata says {file_says}. \
                 Refusing to replay bars as an instrument they are not",
                path.display()
            ),
            CuratedError::MetadataContradictsColumns {
                path,
                claim,
                recorded,
                actual,
            } => write!(
                f,
                "{} records {claim} = {recorded} but its columns say {actual}; \
                 a file whose metadata does not describe its own bytes cannot be \
                 trusted to describe anything else",
                path.display()
            ),
            CuratedError::OutOfOrderRows { path, prev, next } => write!(
                f,
                "{} has ts_open {next} after {prev}. Bars must be strictly \
                 increasing: replaying them in this order would let the engine \
                 see the same interval twice, or see it out of sequence",
                path.display()
            ),
            CuratedError::OverlappingWindows {
                earlier,
                later,
                earlier_last_ts,
                later_first_ts,
            } => write!(
                f,
                "{} ends at {earlier_last_ts} and {} starts at {later_first_ts}: \
                 the windows overlap, so replaying both would double-count bars. \
                 Rebuild the partition: crucible transcode --force",
                earlier.display(),
                later.display()
            ),
            CuratedError::VolumeOutOfRange { path, value } => write!(
                f,
                "{} carries volume {value}, which does not fit the curated \
                 storage type; the record is corrupt",
                path.display()
            ),
            CuratedError::NoCuratedData {
                instrument,
                tf,
                dir,
            } => write!(
                f,
                "no curated {tf} bars for {instrument} ({} does not exist). \
                 Curated data is built from the archive:\n\
                 \x20      crucible transcode",
                dir.display()
            ),
            CuratedError::EmptyRange {
                instrument,
                tf,
                start_ts,
                end_ts,
                held_first_ts,
                held_last_ts,
            } => write!(
                f,
                "no curated {tf} bars for {instrument} in [{start_ts}, {end_ts}); \
                 the partition holds [{held_first_ts}, {held_last_ts}]. An empty \
                 backtest is a result nobody can interpret, so this refuses \
                 rather than reporting zero trades"
            ),
        }
    }
}

impl std::error::Error for CuratedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CuratedError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
