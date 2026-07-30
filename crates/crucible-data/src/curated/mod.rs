//! The curated bar store: Parquet files the engine can replay.
//!
//! **Status: implemented.** [`transcode`](crate::transcode) writes it,
//! [`ParquetBarFeed`] reads it, and the engine cannot tell the result from a
//! [`SyntheticFeed`](crate::SyntheticFeed).
//!
//! ## Layout
//!
//! ```text
//! curated/bars/{instrument}/{tf}/{source_window_stem}.parquet
//! ```
//!
//! e.g. `curated/bars/ESH2024/1m/2024-01.parquet`, transcoded from
//! `raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst`.
//!
//! One raw file fans out into **one curated file per instrument**, each named
//! after the raw window it came from and each recording exactly one
//! `source_file_blake3` — the D-0014 manifest id. A backtest can therefore
//! name the precise bytes it read (D-0013) without a second index to keep in
//! sync, and `rm -rf curated/` is always safe because every file is
//! reproducible from one raw file and one transcoder version.
//!
//! The alternative sketched in `catalog`'s original layout comment,
//! `curated/bars/{symbol}/{tf}/{yyyy}.parquet`, was rejected: two raw windows
//! landing in the same year would force a read-modify-write merge, which is
//! the only place in this codebase curated data would ever have to be merged,
//! and merging is where silent duplication lives (D-0036).
//!
//! `{instrument}` is percent-encoded (see [`path`]) because `InstrumentId`
//! admits characters a filename does not — `SYN:RW` is a legal instrument and
//! an illegal Windows filename.
//!
//! ## Column schema v1
//!
//! | column | physical type | meaning |
//! |---|---|---|
//! | `ts_open` | `INT64` | UTC nanoseconds, interval OPEN (D-0003) |
//! | `open` `high` `low` `close` | `INT64` | nanopoints (1e-9) — DBN-native, byte-for-byte what `Price` holds |
//! | `volume` | `INT64` | contracts |
//!
//! Every column is an integer. There is no `f64` anywhere on the path from a
//! DBN record to a [`Bar`](crucible_core::events::Bar), which is the whole
//! point: Databento's OHLCV fields are already i64 at 1e-9, so
//! [`Price::from_nanos`](crucible_core::types::Price::from_nanos) is an
//! identity, and the round trip is lossless by construction rather than by
//! rounding luck.
//!
//! `volume` is stored **signed** and converted at the Parquet boundary with
//! `try_from` in both directions. An unsigned logical type would buy nothing
//! at futures volumes and cost a reinterpreting cast; a refusal at 2^63
//! contracts is a refusal for data that is corrupt anyway.
//!
//! **`avail_ts` is deliberately not stored.** It is `ts_open + tf`, computed
//! by [`Bar::avail_ts`](crucible_core::events::Bar::avail_ts) and nowhere
//! else (CLAUDE.md §2.1). A persisted ordering key is a key that can come to
//! disagree with the column it was derived from.
//!
//! Instrument and timeframe live in **file-level key-value metadata**, not in
//! columns: one file holds one instrument, so a column would repeat the same
//! string a few million times. See [`meta`] for the full key list.
//!
//! ## Versioning
//!
//! Two independent numbers, because they fail differently (D-0036):
//!
//! - [`CURATED_SCHEMA_VERSION`] describes the *file* — columns and metadata
//!   keys. A mismatch is a **hard refusal**: this build cannot know what the
//!   bytes mean.
//! - [`TRANSCODER_VERSION`] describes the *semantics* of DBN → `Bar`. A
//!   mismatch is readable, so it **warns** and is printed alongside results
//!   rather than blocking them; a cosmetic bump must not invalidate a 50 GB
//!   archive, but nor may it be silent.
//!
//! ## What this module validates, and what it does not
//!
//! [`ParquetBarFeed::open`] resolves every failure a replay could hit
//! **before the first bar is yielded**, because [`Feed::next_event`] returns
//! `Option`, not `Result` — a feed has no way to report an error mid-run, so
//! it must have none left to report. It refuses an unknown schema version,
//! metadata that disagrees with the partition path it was found under,
//! overlapping windows, non-increasing `ts_open`, and metadata that lies
//! about its own row count or endpoints.
//!
//! It does **not** check OHLC sanity (`high >= max(open, close)` and friends)
//! or hunt for gaps, zero-volume runs, or price spikes. That is the data-QA
//! report's job, and doing it here would silently conflate "this file is
//! damaged" with "this market was thin on 2024-01-01". Transcode validates
//! the vendor's semantics once, where the vendor bytes are interpreted; this
//! module validates the file's integrity.
//!
//! [`Feed::next_event`]: crucible_core::traits::Feed::next_event

pub mod error;
pub mod meta;
pub mod path;
mod read;
mod write;

pub use error::CuratedError;
pub use meta::{CuratedMeta, PartitionSource};
pub use read::{
    CuratedSource, ParquetBarFeed, Resolution, list_instruments, read_meta, resolve_instrument,
};
pub use write::{PartitionWriter, write_partition};

/// Directory under the data dir holding every curated bar partition.
pub const CURATED_BARS_DIR: &str = "curated/bars";

/// File extension of a curated partition.
pub const PARQUET_EXT: &str = "parquet";

/// Extension of a partition still being written. Renamed into place only
/// after the footer is flushed, so a crashed transcode leaves no file a feed
/// would try to replay.
pub const PARQUET_TMP_EXT: &str = "parquet.tmp";

/// Version of the curated *file format*: columns and metadata keys.
///
/// Bumped whenever a reader of the previous version would misread the bytes.
/// [`ParquetBarFeed::open`] refuses anything it does not recognise.
pub const CURATED_SCHEMA_VERSION: u32 = 1;

/// Version of the DBN → [`Bar`](crucible_core::events::Bar) *semantics*.
///
/// Bumped whenever the same raw file would transcode to different bars.
/// Recorded in every file and surfaced in results; unlike
/// [`CURATED_SCHEMA_VERSION`] it does not block a read, because curated data
/// written by an older transcoder is still perfectly decodable — it just is
/// not what this build would produce, and the operator has to know that.
pub const TRANSCODER_VERSION: u32 = 1;

/// Rows buffered before a Parquet row group is flushed.
///
/// Bounds transcode's memory: one raw `ohlcv-1s` window covering sixteen
/// years expands to hundreds of millions of rows across ~40 instruments, and
/// buffering any of it whole is not an option. At six `i64` columns this is
/// ~3 MB of live buffer per open instrument writer.
pub const ROW_GROUP_ROWS: usize = 64 * 1024;

/// The Parquet message type for [`CURATED_SCHEMA_VERSION`] 1.
///
/// Spelled out rather than built programmatically so that the on-disk schema
/// is greppable and reviewable as one block of text.
pub(crate) const BARS_MESSAGE_TYPE: &str = "
message crucible_bars {
  REQUIRED INT64 ts_open;
  REQUIRED INT64 open;
  REQUIRED INT64 high;
  REQUIRED INT64 low;
  REQUIRED INT64 close;
  REQUIRED INT64 volume;
}
";

/// Bar columns in memory: the shape both the writer and the reader speak.
///
/// Columnar rather than `Vec<Bar>` on purpose — a `Bar` owns an
/// [`InstrumentId`](crucible_core::types::InstrumentId), which is a `String`,
/// and a partition holds one instrument. Materialising bars on demand costs
/// one clone per bar instead of one allocation per bar held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BarColumns {
    /// Interval open, UTC nanoseconds. Strictly increasing.
    pub ts_open: Vec<i64>,
    /// Open price, nanopoints.
    pub open: Vec<i64>,
    /// High price, nanopoints.
    pub high: Vec<i64>,
    /// Low price, nanopoints.
    pub low: Vec<i64>,
    /// Close price, nanopoints.
    pub close: Vec<i64>,
    /// Volume in contracts.
    pub volume: Vec<u64>,
}

impl BarColumns {
    /// Number of bars held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ts_open.len()
    }

    /// Whether any bars are held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ts_open.is_empty()
    }

    /// Appends every row of `other`, which must start after `self` ends.
    fn extend(&mut self, other: &BarColumns) {
        self.ts_open.extend_from_slice(&other.ts_open);
        self.open.extend_from_slice(&other.open);
        self.high.extend_from_slice(&other.high);
        self.low.extend_from_slice(&other.low);
        self.close.extend_from_slice(&other.close);
        self.volume.extend_from_slice(&other.volume);
    }

    /// True when every column has the same length — the invariant every
    /// constructor in this module maintains and every reader re-checks.
    fn columns_agree(&self) -> bool {
        let n = self.ts_open.len();
        self.open.len() == n
            && self.high.len() == n
            && self.low.len() == n
            && self.close.len() == n
            && self.volume.len() == n
    }
}
