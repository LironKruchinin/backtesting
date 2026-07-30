//! Reading the curated store, and the [`Feed`] the engine replays.
//!
//! [`Feed::next_event`] returns `Option<MarketEvent>` — there is no error
//! channel, by design: a misordered or unreadable feed is a correctness bug,
//! not a recoverable condition, and the engine has nowhere to put an `io`
//! error halfway through a backtest. So [`ParquetBarFeed::open`] does all the
//! failing. By the time the first bar is yielded, every file has been
//! identified, version-checked, cross-checked against the path it was filed
//! under, ordered, overlap-checked, loaded, and verified to be strictly
//! increasing in `ts_open`. Iteration afterwards is infallible arithmetic.
//!
//! That also settles the "mmap'd" wording in the M1 milestone: Parquet pages
//! are encoded and compressed, so there are no bars to map — they have to be
//! decoded. Bars live in RAM (six `i64` columns, ~48 bytes a bar: a decade of
//! ES 1-minute bars is a couple of hundred megabytes), and tick/book data
//! will stream from disk when it arrives.

use std::fs::File;
use std::path::{Path, PathBuf};

use crucible_core::events::{Bar, MarketEvent};
use crucible_core::traits::Feed;
use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};
use parquet::column::reader::ColumnReader;
use parquet::file::reader::{FileReader, SerializedFileReader};

use crate::catalog::TsRange;

use super::meta::CuratedMeta;
use super::path::{decode_instrument, partition_dir};
use super::{BarColumns, CURATED_SCHEMA_VERSION, CuratedError, PARQUET_EXT};

/// Provenance of one curated file a feed replayed.
///
/// Printed alongside results so a number can always be traced to the raw
/// bytes behind it: `source_file_blake3` is the manifest id (D-0014), so
/// `crucible verify` can re-hash exactly what fed the run (D-0013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedSource {
    /// The curated file itself.
    pub curated_path: PathBuf,
    /// Archive-relative path of the raw file it was transcoded from.
    pub source_file_path: String,
    /// blake3 of that raw file — its manifest id.
    pub source_file_blake3: String,
    /// Transcoder semantics that produced it.
    pub transcoder_version: u32,
    /// Rows contributed, after range filtering.
    pub rows: usize,
}

/// Reads a curated file's self-description without touching its columns.
///
/// # Errors
/// [`CuratedError::Io`] if the file cannot be opened, [`CuratedError::Parquet`]
/// if it is not readable as Parquet, and the metadata variants of
/// [`CuratedError`] if it does not describe itself completely.
pub fn read_meta(path: &Path) -> Result<CuratedMeta, CuratedError> {
    let reader = open_reader(path)?;
    meta_of(path, &reader)
}

/// Opens a Parquet reader, mapping both failure shapes onto [`CuratedError`].
fn open_reader(path: &Path) -> Result<SerializedFileReader<File>, CuratedError> {
    let file = File::open(path).map_err(|source| CuratedError::Io {
        path: path.to_path_buf(),
        during: "opening",
        source,
    })?;
    SerializedFileReader::new(file).map_err(|e| CuratedError::Parquet {
        path: path.to_path_buf(),
        during: "reading",
        detail: e.to_string(),
    })
}

fn meta_of(path: &Path, reader: &SerializedFileReader<File>) -> Result<CuratedMeta, CuratedError> {
    let file_meta = reader.metadata().file_metadata();
    let kvs = file_meta
        .key_value_metadata()
        .ok_or(CuratedError::MissingMetadata {
            path: path.to_path_buf(),
            key: "any crucible.* metadata",
        })?;
    let meta = CuratedMeta::from_key_values(path, kvs)?;
    if meta.curated_schema_version != CURATED_SCHEMA_VERSION {
        return Err(CuratedError::UnknownSchemaVersion {
            path: path.to_path_buf(),
            found: meta.curated_schema_version,
            expected: CURATED_SCHEMA_VERSION,
        });
    }
    Ok(meta)
}

/// Reads one `i64` column across every row group.
fn read_i64_column(
    path: &Path,
    reader: &SerializedFileReader<File>,
    column: usize,
    total_rows: usize,
) -> Result<Vec<i64>, CuratedError> {
    let parquet_err = |e: parquet::errors::ParquetError| CuratedError::Parquet {
        path: path.to_path_buf(),
        during: "reading a column of",
        detail: e.to_string(),
    };
    let mut out: Vec<i64> = Vec::with_capacity(total_rows);
    for group in 0..reader.num_row_groups() {
        let row_group = reader.get_row_group(group).map_err(parquet_err)?;
        let rows = usize::try_from(row_group.metadata().num_rows()).unwrap_or(0);
        let ColumnReader::Int64ColumnReader(mut typed) =
            row_group.get_column_reader(column).map_err(parquet_err)?
        else {
            return Err(CuratedError::Parquet {
                path: path.to_path_buf(),
                during: "reading",
                detail: format!("column {column} is not INT64"),
            });
        };
        let mut read_here = 0usize;
        while read_here < rows {
            let (records, _, _) = typed
                .read_records(rows - read_here, None, None, &mut out)
                .map_err(parquet_err)?;
            if records == 0 {
                break;
            }
            read_here += records;
        }
    }
    Ok(out)
}

/// Reads every column of a curated file and cross-checks it against the
/// metadata that claims to describe it.
fn read_columns(
    path: &Path,
    reader: &SerializedFileReader<File>,
    meta: &CuratedMeta,
) -> Result<BarColumns, CuratedError> {
    let total_rows = usize::try_from(meta.row_count).unwrap_or(usize::MAX);
    let mut columns = Vec::with_capacity(6);
    for index in 0..6 {
        columns.push(read_i64_column(path, reader, index, total_rows)?);
    }
    let mut drain = columns.into_iter();
    let mut next = || drain.next().unwrap_or_default();
    let ts_open = next();
    let open = next();
    let high = next();
    let low = next();
    let close = next();
    let raw_volume = next();

    let mut volume = Vec::with_capacity(raw_volume.len());
    for value in raw_volume {
        volume.push(
            u64::try_from(value).map_err(|_| CuratedError::VolumeOutOfRange {
                path: path.to_path_buf(),
                value: value.to_string(),
            })?,
        );
    }

    let bars = BarColumns {
        ts_open,
        open,
        high,
        low,
        close,
        volume,
    };
    if !bars.columns_agree() {
        return Err(CuratedError::MetadataContradictsColumns {
            path: path.to_path_buf(),
            claim: "column lengths",
            recorded: "all equal".to_owned(),
            actual: format!(
                "{}/{}/{}/{}/{}/{}",
                bars.ts_open.len(),
                bars.open.len(),
                bars.high.len(),
                bars.low.len(),
                bars.close.len(),
                bars.volume.len()
            ),
        });
    }

    // The metadata must describe the bytes it travelled with. A file that
    // misstates its own row count or endpoints has been truncated, merged,
    // or hand-edited, and nothing else it says can be believed either.
    if bars.len() as u64 != meta.row_count {
        return Err(CuratedError::MetadataContradictsColumns {
            path: path.to_path_buf(),
            claim: "row_count",
            recorded: meta.row_count.to_string(),
            actual: bars.len().to_string(),
        });
    }
    let (Some(&first), Some(&last)) = (bars.ts_open.first(), bars.ts_open.last()) else {
        return Err(CuratedError::MetadataContradictsColumns {
            path: path.to_path_buf(),
            claim: "row_count",
            recorded: meta.row_count.to_string(),
            actual: "0".to_owned(),
        });
    };
    if first != meta.first_ts_open.0 || last != meta.last_ts_open.0 {
        return Err(CuratedError::MetadataContradictsColumns {
            path: path.to_path_buf(),
            claim: "ts_open endpoints",
            recorded: format!("[{}, {}]", meta.first_ts_open.0, meta.last_ts_open.0),
            actual: format!("[{first}, {last}]"),
        });
    }

    // Strictly increasing, checked here rather than left to the engine's
    // guard: `Feed` promises nondecreasing availability, and a duplicated
    // `ts_open` is a duplicated bar, which the engine would happily replay.
    for pair in bars.ts_open.windows(2) {
        if pair[1] <= pair[0] {
            return Err(CuratedError::OutOfOrderRows {
                path: path.to_path_buf(),
                prev: Ts(pair[0]),
                next: Ts(pair[1]),
            });
        }
    }
    Ok(bars)
}

/// One curated file, located and described but not yet loaded.
#[derive(Debug)]
struct Located {
    path: PathBuf,
    meta: CuratedMeta,
}

/// A [`Feed`] over the curated store: availability-ordered bars for one
/// instrument and timeframe, concatenated across window files.
#[derive(Debug)]
pub struct ParquetBarFeed {
    instrument: InstrumentId,
    tf: TimeFrame,
    bars: BarColumns,
    cursor: usize,
    sources: Vec<CuratedSource>,
}

impl ParquetBarFeed {
    /// Loads every curated partition for `instrument`/`tf` that overlaps
    /// `range` (half-open on `ts_open`; `None` means everything held).
    ///
    /// # Errors
    /// [`CuratedError::NoCuratedData`] when nothing has been transcoded,
    /// [`CuratedError::EmptyRange`] when nothing falls in the requested
    /// window, and the refusal variants of [`CuratedError`] for a file that
    /// cannot be trusted: an unknown schema version, metadata contradicting
    /// the partition path or its own columns, overlapping windows, or
    /// non-increasing `ts_open`.
    pub fn open(
        data_dir: &Path,
        instrument: &InstrumentId,
        tf: TimeFrame,
        range: Option<TsRange>,
    ) -> Result<ParquetBarFeed, CuratedError> {
        let dir = partition_dir(data_dir, instrument, tf)?;
        let no_data = || CuratedError::NoCuratedData {
            instrument: instrument.as_str().to_owned(),
            tf,
            dir: dir.clone(),
        };
        if !dir.is_dir() {
            return Err(no_data());
        }

        let mut located = Self::locate(&dir, instrument, tf)?;
        if located.is_empty() {
            return Err(no_data());
        }

        // Sorted by first bar, then by name so the order is total and does
        // not depend on the order the filesystem happened to list entries in
        // (CLAUDE.md §2.2).
        located.sort_by(|a, b| {
            a.meta
                .first_ts_open
                .0
                .cmp(&b.meta.first_ts_open.0)
                .then_with(|| a.path.cmp(&b.path))
        });
        for pair in located.windows(2) {
            if pair[1].meta.first_ts_open.0 <= pair[0].meta.last_ts_open.0 {
                return Err(CuratedError::OverlappingWindows {
                    earlier: pair[0].path.clone(),
                    later: pair[1].path.clone(),
                    earlier_last_ts: pair[0].meta.last_ts_open,
                    later_first_ts: pair[1].meta.first_ts_open,
                });
            }
        }

        let held_first = located.first().map_or(Ts(0), |f| f.meta.first_ts_open);
        let held_last = located.last().map_or(Ts(0), |f| f.meta.last_ts_open);

        let mut bars = BarColumns::default();
        let mut sources = Vec::new();
        for file in &located {
            if !overlaps(&file.meta, range) {
                continue;
            }
            let reader = open_reader(&file.path)?;
            let loaded = read_columns(&file.path, &reader, &file.meta)?;
            let trimmed = trim_to_range(&loaded, range);
            if trimmed.is_empty() {
                continue;
            }
            if let (Some(&prev), Some(&next)) = (bars.ts_open.last(), trimmed.ts_open.first())
                && next <= prev
            {
                return Err(CuratedError::OutOfOrderRows {
                    path: file.path.clone(),
                    prev: Ts(prev),
                    next: Ts(next),
                });
            }
            sources.push(CuratedSource {
                curated_path: file.path.clone(),
                source_file_path: file.meta.source.source_file_path.clone(),
                source_file_blake3: file.meta.source.source_file_blake3.clone(),
                transcoder_version: file.meta.transcoder_version,
                rows: trimmed.len(),
            });
            bars.extend(&trimmed);
        }

        if bars.is_empty() {
            let requested =
                range.unwrap_or(TsRange::new(held_first, held_last.plus_ns(1)).map_err(|_| {
                    CuratedError::NoCuratedData {
                        instrument: instrument.as_str().to_owned(),
                        tf,
                        dir: dir.clone(),
                    }
                })?);
            return Err(CuratedError::EmptyRange {
                instrument: instrument.as_str().to_owned(),
                tf,
                start_ts: requested.start_ts(),
                end_ts: requested.end_ts(),
                held_first_ts: held_first,
                held_last_ts: held_last,
            });
        }

        Ok(ParquetBarFeed {
            instrument: instrument.clone(),
            tf,
            bars,
            cursor: 0,
            sources,
        })
    }

    /// Finds and describes every `.parquet` in a partition directory,
    /// refusing any whose metadata disagrees with where it was filed.
    fn locate(
        dir: &Path,
        instrument: &InstrumentId,
        tf: TimeFrame,
    ) -> Result<Vec<Located>, CuratedError> {
        let entries = std::fs::read_dir(dir).map_err(|source| CuratedError::Io {
            path: dir.to_path_buf(),
            during: "listing",
            source,
        })?;
        let mut located = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| CuratedError::Io {
                path: dir.to_path_buf(),
                during: "listing",
                source,
            })?;
            let path = entry.path();
            // `.parquet.tmp` has extension `tmp`, so unfinished writes are
            // skipped without a special case.
            if path.extension().and_then(|e| e.to_str()) != Some(PARQUET_EXT) {
                continue;
            }
            let reader = open_reader(&path)?;
            let meta = meta_of(&path, &reader)?;
            if &meta.source.instrument != instrument || meta.source.tf != tf {
                return Err(CuratedError::PartitionMismatch {
                    path,
                    path_says: format!("{instrument} {tf}"),
                    file_says: format!("{} {}", meta.source.instrument, meta.source.tf),
                });
            }
            located.push(Located { path, meta });
        }
        Ok(located)
    }

    /// The curated files this feed will replay, in order, with the manifest
    /// id of the raw bytes behind each.
    #[must_use]
    pub fn sources(&self) -> &[CuratedSource] {
        &self.sources
    }

    /// Total bars loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// Whether the feed holds no bars. Never true for a feed that opened
    /// successfully — `open` refuses an empty result.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// `ts_open` of the first bar.
    #[must_use]
    pub fn first_ts_open(&self) -> Option<Ts> {
        self.bars.ts_open.first().copied().map(Ts)
    }

    /// `ts_open` of the last bar.
    #[must_use]
    pub fn last_ts_open(&self) -> Option<Ts> {
        self.bars.ts_open.last().copied().map(Ts)
    }

    /// The instrument every bar carries.
    #[must_use]
    pub fn instrument(&self) -> &InstrumentId {
        &self.instrument
    }

    /// The bar interval.
    #[must_use]
    pub fn tf(&self) -> TimeFrame {
        self.tf
    }
}

/// Whether a file's `ts_open` span intersects a half-open request.
fn overlaps(meta: &CuratedMeta, range: Option<TsRange>) -> bool {
    match range {
        None => true,
        Some(r) => meta.first_ts_open.0 < r.end_ts().0 && meta.last_ts_open.0 >= r.start_ts().0,
    }
}

/// Restricts loaded columns to `[start_ts, end_ts)` on `ts_open`.
fn trim_to_range(bars: &BarColumns, range: Option<TsRange>) -> BarColumns {
    let Some(range) = range else {
        return bars.clone();
    };
    let start = bars.ts_open.partition_point(|&ts| ts < range.start_ts().0);
    let end = bars.ts_open.partition_point(|&ts| ts < range.end_ts().0);
    BarColumns {
        ts_open: bars.ts_open[start..end].to_vec(),
        open: bars.open[start..end].to_vec(),
        high: bars.high[start..end].to_vec(),
        low: bars.low[start..end].to_vec(),
        close: bars.close[start..end].to_vec(),
        volume: bars.volume[start..end].to_vec(),
    }
}

impl Feed for ParquetBarFeed {
    fn next_event(&mut self) -> Option<MarketEvent> {
        let i = self.cursor;
        let &ts_open = self.bars.ts_open.get(i)?;
        self.cursor += 1;
        Some(MarketEvent::Bar(Bar {
            instrument: self.instrument.clone(),
            tf: self.tf,
            ts_open: Ts(ts_open),
            open: Price::from_nanos(self.bars.open[i]),
            high: Price::from_nanos(self.bars.high[i]),
            low: Price::from_nanos(self.bars.low[i]),
            close: Price::from_nanos(self.bars.close[i]),
            volume: self.bars.volume[i],
            signal_offset: Price::ZERO,
        }))
    }
}

/// Every instrument that has curated bars at `tf`, sorted.
///
/// Used by the CLI to say "you asked for ESH5, the archive has ESH4" instead
/// of only "not found".
///
/// # Errors
/// [`CuratedError::Io`] if `curated/bars` cannot be listed, and
/// [`CuratedError::UndecodableInstrumentDir`] if it contains a directory that
/// is not a percent-encoded instrument.
pub fn list_instruments(data_dir: &Path, tf: TimeFrame) -> Result<Vec<String>, CuratedError> {
    let root = data_dir.join(super::CURATED_BARS_DIR);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&root).map_err(|source| CuratedError::Io {
        path: root.clone(),
        during: "listing",
        source,
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| CuratedError::Io {
            path: root.clone(),
            during: "listing",
            source,
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let decoded =
            decode_instrument(name).ok_or_else(|| CuratedError::UndecodableInstrumentDir {
                dir_name: name.to_owned(),
            })?;
        if path.join(tf.to_string()).is_dir() {
            out.push(decoded);
        }
    }
    out.sort();
    Ok(out)
}

/// What a requested instrument name resolved to in the curated store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The name is on disk exactly as typed, or names exactly one contract.
    Exact(String),
    /// A shorthand that names more than one curated contract.
    ///
    /// Not an error this module chooses to swallow: `GCZ4` genuinely names two
    /// contracts a decade apart (D-0072), and answering with either would be a
    /// coin toss reported as a backtest.
    Ambiguous(Vec<String>),
    /// Nothing curated matches, with everything that is there to suggest.
    Missing(Vec<String>),
}

/// Resolves an instrument name a human typed against the curated store.
///
/// Curated contracts are keyed by their canonical four-digit spelling
/// (`ESH2024`, D-0072), but the vendor's one-digit spelling is what an operator
/// reads off a symbology listing, and `ESH4` is what this project's own docs
/// used for years. So a shorthand still works — **when it is unambiguous**:
///
/// - an exact directory name wins outright, with no search;
/// - otherwise every curated contract the shorthand could name is collected,
///   and one match resolves;
/// - **two or more matches refuse.** `GCZ4` is December-2014 gold *and*
///   December-2024 gold, and this project does not answer an ambiguous
///   question with one of its answers.
///
/// Names that are not contracts at all (`SYN:RW`, `ES.v.0`, a spread) only ever
/// match exactly, since they have no year to be short about.
///
/// # Errors
/// [`CuratedError::Io`] or [`CuratedError::UndecodableInstrumentDir`] via
/// [`list_instruments`].
pub fn resolve_instrument(
    data_dir: &Path,
    tf: TimeFrame,
    requested: &str,
) -> Result<Resolution, CuratedError> {
    let available = list_instruments(data_dir, tf)?;
    if available.iter().any(|name| name == requested) {
        return Ok(Resolution::Exact(requested.to_owned()));
    }
    let Ok(wanted) = crate::continuous::parse_parts(requested) else {
        return Ok(Resolution::Missing(available));
    };
    // A shorthand names a contract whose root and month match and whose year
    // agrees on the digits that were written — one digit means "≡ mod 10", two
    // mean "≡ mod 100".
    let modulus = match wanted.year_digits {
        1 => 10,
        2 => 100,
        _ => return Ok(Resolution::Missing(available)),
    };
    let mut matches: Vec<String> = available
        .iter()
        .filter(|name| {
            crate::continuous::parse_parts(name).is_ok_and(|have| {
                have.year_digits == 4
                    && have.root == wanted.root
                    && have.month == wanted.month
                    && have.year_value % modulus == wanted.year_value
            })
        })
        .cloned()
        .collect();
    matches.sort();
    match matches.len() {
        0 => Ok(Resolution::Missing(available)),
        1 => Ok(Resolution::Exact(matches.remove(0))),
        _ => Ok(Resolution::Ambiguous(matches)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SyntheticFeed;
    use crate::curated::meta::PartitionSource;
    use crate::curated::path::partition_file;
    use crate::curated::write::{write_partition, write_partition_unchecked};
    use crate::curated::{CURATED_SCHEMA_VERSION, TRANSCODER_VERSION};
    use crate::testutil::TempDir;

    const MIN: i64 = 60_000_000_000;

    fn source_for(instrument: &str) -> PartitionSource {
        PartitionSource {
            instrument: InstrumentId::new(instrument),
            tf: TimeFrame::M1,
            dataset: "GLBX.MDP3".to_owned(),
            vendor_schema: "ohlcv-1m".to_owned(),
            source_file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_owned(),
            source_file_blake3: "2304cedfe99de7811ef2fa90cf9ff9f6a3bcfc0cca22ee134f4e031647e16ecc"
                .to_owned(),
        }
    }

    /// Rows as `(ts_open, open, high, low, close, volume)`, all nanos.
    fn cols(rows: &[(i64, i64, i64, i64, i64, u64)]) -> BarColumns {
        BarColumns {
            ts_open: rows.iter().map(|r| r.0).collect(),
            open: rows.iter().map(|r| r.1).collect(),
            high: rows.iter().map(|r| r.2).collect(),
            low: rows.iter().map(|r| r.3).collect(),
            close: rows.iter().map(|r| r.4).collect(),
            volume: rows.iter().map(|r| r.5).collect(),
        }
    }

    fn three_bars() -> BarColumns {
        // Hand-picked so every field is distinguishable if any two were ever
        // transposed: prices differ from each other and from ts/volume.
        cols(&[
            (
                MIN,
                5_000_250_000_000,
                5_001_000_000_000,
                4_999_750_000_000,
                5_000_750_000_000,
                137,
            ),
            (
                2 * MIN,
                5_000_750_000_000,
                5_002_250_000_000,
                5_000_500_000_000,
                5_002_000_000_000,
                412,
            ),
            (
                3 * MIN,
                5_002_000_000_000,
                5_002_500_000_000,
                4_998_000_000_000,
                4_998_250_000_000,
                9,
            ),
        ])
    }

    fn meta_for(source: PartitionSource, bars: &BarColumns) -> CuratedMeta {
        CuratedMeta {
            curated_schema_version: CURATED_SCHEMA_VERSION,
            transcoder_version: TRANSCODER_VERSION,
            source,
            row_count: bars.len() as u64,
            first_ts_open: Ts(bars.ts_open.first().copied().unwrap_or(0)),
            last_ts_open: Ts(bars.ts_open.last().copied().unwrap_or(0)),
        }
    }

    fn drain(feed: &mut ParquetBarFeed) -> Vec<MarketEvent> {
        std::iter::from_fn(|| feed.next_event()).collect()
    }

    fn open_es(dir: &TempDir, range: Option<TsRange>) -> Result<ParquetBarFeed, CuratedError> {
        ParquetBarFeed::open(dir.path(), &InstrumentId::new("ESH4"), TimeFrame::M1, range)
    }

    // ---------------------------------------------------------------- fidelity

    #[test]
    fn every_field_survives_the_round_trip() {
        let dir = TempDir::new();
        let bars = three_bars();
        write_partition(dir.path(), source_for("ESH4"), "2024-01", &bars)
            .expect("write")
            .expect("rows were written");

        let events = drain(&mut open_es(&dir, None).expect("open"));
        assert_eq!(events.len(), 3);
        let MarketEvent::Bar(first) = &events[0];
        assert_eq!(first.instrument, InstrumentId::new("ESH4"));
        assert_eq!(first.tf, TimeFrame::M1);
        assert_eq!(first.ts_open, Ts(MIN));
        assert_eq!(first.open, Price::from_nanos(5_000_250_000_000));
        assert_eq!(first.high, Price::from_nanos(5_001_000_000_000));
        assert_eq!(first.low, Price::from_nanos(4_999_750_000_000));
        assert_eq!(first.close, Price::from_nanos(5_000_750_000_000));
        assert_eq!(first.volume, 137);
        // avail_ts is derived, never stored: a 1m bar opening at t is
        // knowable at t+60s (D-0003).
        assert_eq!(first.avail_ts(), Ts(MIN + MIN));
    }

    // The engine must not be able to tell a replayed archive from a generated
    // one. This is the golden test the whole curated format exists to pass.
    #[test]
    fn a_synthetic_feed_survives_parquet_bit_identically() {
        let dir = TempDir::new();
        let mut generator = SyntheticFeed::random_walk(
            42,
            500,
            TimeFrame::M1,
            Price::from_points(5000),
            Price::from_points_f64_lossy(0.25),
            4,
        );
        let expected: Vec<MarketEvent> = std::iter::from_fn(|| generator.next_event()).collect();
        assert_eq!(expected.len(), 500);

        let mut bars = BarColumns::default();
        for event in &expected {
            let MarketEvent::Bar(bar) = event;
            bars.ts_open.push(bar.ts_open.0);
            bars.open.push(bar.open.as_nanos());
            bars.high.push(bar.high.as_nanos());
            bars.low.push(bar.low.as_nanos());
            bars.close.push(bar.close.as_nanos());
            bars.volume.push(bar.volume);
        }

        // "SYN:RW" also exercises the percent-encoded partition directory —
        // a colon is a legal instrument and an illegal Windows filename.
        let source = PartitionSource {
            instrument: InstrumentId::new("SYN:RW"),
            tf: TimeFrame::M1,
            dataset: "SYNTHETIC".to_owned(),
            vendor_schema: "ohlcv-1m".to_owned(),
            source_file_path: "raw/none".to_owned(),
            source_file_blake3: "00".repeat(32),
        };
        write_partition(dir.path(), source, "seed-42", &bars)
            .expect("write")
            .expect("rows were written");

        let mut feed = ParquetBarFeed::open(
            dir.path(),
            &InstrumentId::new("SYN:RW"),
            TimeFrame::M1,
            None,
        )
        .expect("open");
        assert_eq!(
            drain(&mut feed),
            expected,
            "replayed bars must be identical"
        );
    }

    #[test]
    fn windows_concatenate_in_time_order_whatever_the_directory_says() {
        let dir = TempDir::new();
        // Written second-window-first on purpose: ordering must come from the
        // data, not from the order the filesystem lists entries in.
        write_partition(
            dir.path(),
            source_for("ESH4"),
            "2024-02",
            &cols(&[(10 * MIN, 1, 1, 1, 1, 1), (11 * MIN, 1, 1, 1, 1, 1)]),
        )
        .expect("write")
        .expect("rows");
        write_partition(
            dir.path(),
            source_for("ESH4"),
            "2024-01",
            &cols(&[(MIN, 1, 1, 1, 1, 1), (2 * MIN, 1, 1, 1, 1, 1)]),
        )
        .expect("write")
        .expect("rows");

        let mut feed = open_es(&dir, None).expect("open");
        assert_eq!(feed.sources().len(), 2);
        assert_eq!(feed.sources()[0].rows, 2);
        let ts: Vec<i64> = drain(&mut feed)
            .iter()
            .map(|MarketEvent::Bar(b)| b.ts_open.0)
            .collect();
        assert_eq!(ts, vec![MIN, 2 * MIN, 10 * MIN, 11 * MIN]);
    }

    #[test]
    fn the_range_filter_is_half_open() {
        let dir = TempDir::new();
        write_partition(dir.path(), source_for("ESH4"), "2024-01", &three_bars())
            .expect("write")
            .expect("rows");

        // [2*MIN, 3*MIN) selects exactly the middle bar: start inclusive, end
        // exclusive, matching every other range in the archive (D-0015).
        let range = TsRange::new(Ts(2 * MIN), Ts(3 * MIN)).expect("valid range");
        let mut feed = open_es(&dir, Some(range)).expect("open");
        let ts: Vec<i64> = drain(&mut feed)
            .iter()
            .map(|MarketEvent::Bar(b)| b.ts_open.0)
            .collect();
        assert_eq!(ts, vec![2 * MIN]);
    }

    #[test]
    fn provenance_names_the_raw_bytes_behind_every_bar() {
        let dir = TempDir::new();
        write_partition(dir.path(), source_for("ESH4"), "2024-01", &three_bars())
            .expect("write")
            .expect("rows");
        let feed = open_es(&dir, None).expect("open");
        let source = &feed.sources()[0];
        assert_eq!(
            source.source_file_path,
            "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"
        );
        // The manifest id (D-0014) — so `crucible verify` can re-hash exactly
        // the bytes that produced a result (D-0013).
        assert_eq!(
            source.source_file_blake3,
            "2304cedfe99de7811ef2fa90cf9ff9f6a3bcfc0cca22ee134f4e031647e16ecc"
        );
        assert_eq!(source.transcoder_version, TRANSCODER_VERSION);
    }

    #[test]
    fn instruments_can_be_listed_for_a_timeframe() {
        let dir = TempDir::new();
        for symbol in ["ESM4", "ESH4", "SYN:RW"] {
            write_partition(
                dir.path(),
                PartitionSource {
                    instrument: InstrumentId::new(symbol),
                    ..source_for(symbol)
                },
                "2024-01",
                &cols(&[(MIN, 1, 1, 1, 1, 1)]),
            )
            .expect("write")
            .expect("rows");
        }
        let found = list_instruments(dir.path(), TimeFrame::M1).expect("list");
        assert_eq!(found, vec!["ESH4", "ESM4", "SYN:RW"]);
        assert!(
            list_instruments(dir.path(), TimeFrame::S1)
                .expect("list")
                .is_empty()
        );
    }

    // ------------------------------------------------------- negative controls

    #[test]
    fn a_missing_partition_names_the_command_that_builds_it() {
        let dir = TempDir::new();
        let err = open_es(&dir, None).expect_err("nothing has been transcoded");
        assert!(matches!(err, CuratedError::NoCuratedData { .. }), "{err}");
        assert!(
            err.to_string().contains("crucible transcode"),
            "the error must say how to fix it: {err}"
        );
    }

    #[test]
    fn an_unknown_schema_version_refuses_rather_than_guessing() {
        let dir = TempDir::new();
        let bars = three_bars();
        let mut meta = meta_for(source_for("ESH4"), &bars);
        meta.curated_schema_version = CURATED_SCHEMA_VERSION + 1;
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        write_partition_unchecked(&path, &meta, &bars).expect("write");

        let err = open_es(&dir, None).expect_err("must refuse a future format");
        assert!(
            matches!(err, CuratedError::UnknownSchemaVersion { found, expected, .. }
                if found == CURATED_SCHEMA_VERSION + 1 && expected == CURATED_SCHEMA_VERSION),
            "{err}"
        );
    }

    // A file filed under one instrument that claims to hold another would
    // silently attribute someone else's bars to the run.
    #[test]
    fn metadata_disagreeing_with_the_partition_path_refuses() {
        let dir = TempDir::new();
        let bars = three_bars();
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        write_partition_unchecked(&path, &meta_for(source_for("ESM4"), &bars), &bars)
            .expect("write");
        let err = open_es(&dir, None).expect_err("must refuse a mislabelled partition");
        assert!(
            matches!(err, CuratedError::PartitionMismatch { .. }),
            "{err}"
        );
    }

    #[test]
    fn overlapping_windows_refuse_rather_than_double_counting() {
        let dir = TempDir::new();
        write_partition(
            dir.path(),
            source_for("ESH4"),
            "2024-01",
            &cols(&[(MIN, 1, 1, 1, 1, 1), (3 * MIN, 1, 1, 1, 1, 1)]),
        )
        .expect("write")
        .expect("rows");
        write_partition(
            dir.path(),
            source_for("ESH4"),
            "2024-01-02--2024-01-03",
            &cols(&[(2 * MIN, 1, 1, 1, 1, 1), (4 * MIN, 1, 1, 1, 1, 1)]),
        )
        .expect("write")
        .expect("rows");

        let err = open_es(&dir, None).expect_err("must refuse overlapping windows");
        assert!(
            matches!(err, CuratedError::OverlappingWindows { .. }),
            "{err}"
        );
    }

    #[test]
    fn out_of_order_rows_refuse() {
        let dir = TempDir::new();
        let bars = cols(&[
            (2 * MIN, 1, 1, 1, 1, 1),
            (MIN, 1, 1, 1, 1, 1),
            (3 * MIN, 1, 1, 1, 1, 1),
        ]);
        let mut meta = meta_for(source_for("ESH4"), &bars);
        meta.first_ts_open = Ts(2 * MIN);
        meta.last_ts_open = Ts(3 * MIN);
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        write_partition_unchecked(&path, &meta, &bars).expect("write");

        let err = open_es(&dir, None).expect_err("must refuse misordered rows");
        assert!(
            matches!(err, CuratedError::OutOfOrderRows { prev, next, .. }
                if prev == Ts(2 * MIN) && next == Ts(MIN)),
            "{err}"
        );
    }

    #[test]
    fn a_repeated_timestamp_refuses() {
        let dir = TempDir::new();
        let bars = cols(&[(MIN, 1, 1, 1, 1, 1), (MIN, 1, 1, 1, 1, 1)]);
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        write_partition_unchecked(&path, &meta_for(source_for("ESH4"), &bars), &bars)
            .expect("write");
        let err = open_es(&dir, None).expect_err("must refuse a duplicated bar");
        assert!(matches!(err, CuratedError::OutOfOrderRows { .. }), "{err}");
    }

    #[test]
    fn metadata_that_lies_about_its_own_bytes_refuses() {
        let dir = TempDir::new();
        let bars = three_bars();
        for mutate in [
            (|m: &mut CuratedMeta| m.row_count = 99) as fn(&mut CuratedMeta),
            |m: &mut CuratedMeta| m.first_ts_open = Ts(1),
            |m: &mut CuratedMeta| m.last_ts_open = Ts(1),
        ] {
            let mut meta = meta_for(source_for("ESH4"), &bars);
            mutate(&mut meta);
            let path = partition_file(
                dir.path(),
                &InstrumentId::new("ESH4"),
                TimeFrame::M1,
                "2024-01",
            )
            .expect("path");
            write_partition_unchecked(&path, &meta, &bars).expect("write");
            let err = open_es(&dir, None).expect_err("must refuse self-contradicting metadata");
            assert!(
                matches!(err, CuratedError::MetadataContradictsColumns { .. }),
                "{err}"
            );
        }
    }

    #[test]
    fn a_file_without_curated_metadata_refuses() {
        let dir = TempDir::new();
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
        // A Parquet file written by anything else: no crucible.* keys at all.
        let schema = std::sync::Arc::new(
            parquet::schema::parser::parse_message_type(crate::curated::BARS_MESSAGE_TYPE)
                .expect("schema"),
        );
        let file = std::fs::File::create(&path).expect("create");
        let mut writer = parquet::file::writer::SerializedFileWriter::new(
            file,
            schema,
            std::sync::Arc::new(parquet::file::properties::WriterProperties::new()),
        )
        .expect("writer");
        let mut group = writer.next_row_group().expect("row group");
        while let Some(mut column) = group.next_column().expect("column") {
            column
                .typed::<parquet::data_type::Int64Type>()
                .write_batch(&[1i64], None, None)
                .expect("write");
            column.close().expect("close");
        }
        group.close().expect("close");
        writer.close().expect("close");

        let err = open_es(&dir, None).expect_err("must refuse a foreign parquet file");
        assert!(matches!(err, CuratedError::MissingMetadata { .. }), "{err}");
    }

    #[test]
    fn a_truncated_file_refuses() {
        let dir = TempDir::new();
        write_partition(dir.path(), source_for("ESH4"), "2024-01", &three_bars())
            .expect("write")
            .expect("rows");
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        let bytes = std::fs::read(&path).expect("read");
        std::fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate");

        let err = open_es(&dir, None).expect_err("must refuse a truncated file");
        assert!(matches!(err, CuratedError::Parquet { .. }), "{err}");
    }

    // Curated pages are zstd-compressed, so damage inside one fails to
    // decompress rather than decoding into plausible-looking prices. The
    // stronger guarantee lives upstream: curated data is rebuildable from
    // `raw/`, whose bytes the manifest blake3-verifies (D-0017).
    #[test]
    fn a_damaged_page_refuses() {
        let dir = TempDir::new();
        write_partition(dir.path(), source_for("ESH4"), "2024-01", &three_bars())
            .expect("write")
            .expect("rows");
        let path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        let mut bytes = std::fs::read(&path).expect("read");
        // Well past the 4-byte magic, well before the footer: the data pages.
        for byte in &mut bytes[8..72] {
            *byte = !*byte;
        }
        std::fs::write(&path, &bytes).expect("corrupt");

        let err = open_es(&dir, None).expect_err("must refuse a damaged file");
        assert!(matches!(err, CuratedError::Parquet { .. }), "{err}");
    }

    // An empty backtest reports zero trades, which reads as "the strategy did
    // nothing" rather than "you asked for a window with no data in it".
    #[test]
    fn a_range_holding_no_bars_refuses() {
        let dir = TempDir::new();
        write_partition(dir.path(), source_for("ESH4"), "2024-01", &three_bars())
            .expect("write")
            .expect("rows");
        let range = TsRange::new(Ts(100 * MIN), Ts(200 * MIN)).expect("valid range");
        let err = open_es(&dir, Some(range)).expect_err("must refuse an empty window");
        assert!(
            matches!(err, CuratedError::EmptyRange { held_first_ts, held_last_ts, .. }
                if held_first_ts == Ts(MIN) && held_last_ts == Ts(3 * MIN)),
            "{err}"
        );
    }

    #[test]
    fn a_stray_directory_under_curated_bars_refuses_to_be_guessed_at() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path().join("curated/bars/not%an%encoding/1m")).expect("mkdir");
        let err = list_instruments(dir.path(), TimeFrame::M1)
            .expect_err("must refuse an undecodable directory");
        assert!(
            matches!(err, CuratedError::UndecodableInstrumentDir { .. }),
            "{err}"
        );
    }
}
