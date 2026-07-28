//! Writing one curated partition, incrementally.
//!
//! Incrementally because transcode fans one raw file out across every
//! instrument it contains, and a sixteen-year `ohlcv-1s` window is hundreds
//! of millions of rows: buffering a partition whole is not an option, and
//! buffering forty of them is less of one. Rows accumulate to
//! [`ROW_GROUP_ROWS`](super::ROW_GROUP_ROWS) and then become a Parquet row
//! group, so live memory is bounded by (open instruments × row-group size)
//! regardless of how long the window is.
//!
//! Placement is **write-to-temp, then rename**. A curated file that exists is
//! a curated file with a footer, so a transcode killed halfway leaves nothing
//! a feed would try to replay. This mirrors `ingest::execute`'s staging rule
//! for the same reason, with one difference that matters: curated files may
//! be replaced, because they are derived and disposable. `raw/` may not, and
//! nothing in this module opens it for writing.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crucible_core::types::{Price, Ts};
use parquet::basic::{Compression, ZstdLevel};
use parquet::data_type::Int64Type;
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::parser::parse_message_type;

use super::meta::PartitionSource;
use super::path::partition_file;
use super::{
    BARS_MESSAGE_TYPE, BarColumns, CURATED_SCHEMA_VERSION, CuratedError, CuratedMeta,
    ROW_GROUP_ROWS, TRANSCODER_VERSION,
};

/// Accumulates bars for one instrument and writes them as one Parquet file.
///
/// Enforces strictly increasing `ts_open` on the way in, so an out-of-order
/// or duplicated bar is impossible to *create*, not merely impossible to
/// read back.
#[derive(Debug)]
pub struct PartitionWriter {
    writer: Option<SerializedFileWriter<File>>,
    tmp_path: PathBuf,
    final_path: PathBuf,
    pending: BarColumns,
    source: PartitionSource,
    row_count: u64,
    first_ts_open: Option<i64>,
    last_ts_open: Option<i64>,
    finished: bool,
}

impl PartitionWriter {
    /// Opens a writer for `{data_dir}/curated/bars/{instrument}/{tf}/{window_stem}.parquet`.
    ///
    /// The file is created under a `.parquet.tmp` name and only renamed into
    /// place by [`finish`](Self::finish).
    ///
    /// # Errors
    /// [`CuratedError::InvalidInstrument`] if the instrument cannot name a
    /// path, [`CuratedError::Io`] if the directory or file cannot be created,
    /// and [`CuratedError::Parquet`] if the writer cannot be initialised.
    pub fn create(
        data_dir: &Path,
        source: PartitionSource,
        window_stem: &str,
    ) -> Result<PartitionWriter, CuratedError> {
        let final_path = partition_file(data_dir, &source.instrument, source.tf, window_stem)?;
        let tmp_path = final_path.with_extension(super::PARQUET_TMP_EXT);

        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CuratedError::Io {
                path: parent.to_path_buf(),
                during: "creating curated partition directory",
                source,
            })?;
        }

        let schema =
            Arc::new(
                parse_message_type(BARS_MESSAGE_TYPE).map_err(|e| CuratedError::Parquet {
                    path: tmp_path.clone(),
                    during: "parsing the curated schema",
                    detail: e.to_string(),
                })?,
            );
        let props = Arc::new(
            WriterProperties::builder()
                .set_compression(Compression::ZSTD(ZstdLevel::default()))
                .build(),
        );
        let file = File::create(&tmp_path).map_err(|source| CuratedError::Io {
            path: tmp_path.clone(),
            during: "creating",
            source,
        })?;
        let writer =
            SerializedFileWriter::new(file, schema, props).map_err(|e| CuratedError::Parquet {
                path: tmp_path.clone(),
                during: "opening",
                detail: e.to_string(),
            })?;

        Ok(PartitionWriter {
            writer: Some(writer),
            tmp_path,
            final_path,
            pending: BarColumns::default(),
            source,
            row_count: 0,
            first_ts_open: None,
            last_ts_open: None,
            finished: false,
        })
    }

    /// Where this partition will land once finished.
    #[must_use]
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    /// Appends one bar. Flushes a row group once enough have accumulated.
    ///
    /// # Errors
    /// [`CuratedError::OutOfOrderRows`] if `ts_open` does not exceed the
    /// previous one, [`CuratedError::VolumeOutOfRange`] if the volume cannot
    /// be stored, and [`CuratedError::Parquet`] on a flush failure.
    pub fn push(
        &mut self,
        ts_open: Ts,
        open: Price,
        high: Price,
        low: Price,
        close: Price,
        volume: u64,
    ) -> Result<(), CuratedError> {
        if let Some(prev) = self.last_ts_open
            && ts_open.0 <= prev
        {
            return Err(CuratedError::OutOfOrderRows {
                path: self.final_path.clone(),
                prev: Ts(prev),
                next: ts_open,
            });
        }
        let stored_volume = i64::try_from(volume).map_err(|_| CuratedError::VolumeOutOfRange {
            path: self.final_path.clone(),
            value: volume.to_string(),
        })?;

        self.pending.ts_open.push(ts_open.0);
        self.pending.open.push(open.as_nanos());
        self.pending.high.push(high.as_nanos());
        self.pending.low.push(low.as_nanos());
        self.pending.close.push(close.as_nanos());
        #[expect(
            clippy::cast_sign_loss,
            reason = "stored_volume came from a u64 through try_from; the round trip is exact"
        )]
        self.pending.volume.push(stored_volume as u64);

        if self.first_ts_open.is_none() {
            self.first_ts_open = Some(ts_open.0);
        }
        self.last_ts_open = Some(ts_open.0);
        self.row_count += 1;

        if self.pending.len() >= ROW_GROUP_ROWS {
            self.flush_row_group()?;
        }
        Ok(())
    }

    /// Writes the buffered rows as one Parquet row group and clears the buffer.
    fn flush_row_group(&mut self) -> Result<(), CuratedError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let writer = self.writer.as_mut().ok_or_else(|| CuratedError::Parquet {
            path: self.tmp_path.clone(),
            during: "writing to",
            detail: "the writer was already closed".to_owned(),
        })?;

        let parquet_err = |during: &'static str| {
            let path = self.tmp_path.clone();
            move |e: parquet::errors::ParquetError| CuratedError::Parquet {
                path: path.clone(),
                during,
                detail: e.to_string(),
            }
        };

        // Columns are handed out in schema order, which is the order
        // BARS_MESSAGE_TYPE declares them.
        #[expect(
            clippy::cast_possible_wrap,
            reason = "every value was produced by i64::try_from in push"
        )]
        let volume_i64: Vec<i64> = self.pending.volume.iter().map(|&v| v as i64).collect();
        let columns: [&[i64]; 6] = [
            &self.pending.ts_open,
            &self.pending.open,
            &self.pending.high,
            &self.pending.low,
            &self.pending.close,
            &volume_i64,
        ];

        let mut row_group = writer
            .next_row_group()
            .map_err(parquet_err("starting a row group in"))?;
        let mut index = 0usize;
        while let Some(mut column) = row_group
            .next_column()
            .map_err(parquet_err("opening a column in"))?
        {
            let values = columns.get(index).ok_or_else(|| CuratedError::Parquet {
                path: self.tmp_path.clone(),
                during: "writing to",
                detail: format!("schema yielded more than {} columns", columns.len()),
            })?;
            column
                .typed::<Int64Type>()
                .write_batch(values, None, None)
                .map_err(parquet_err("writing a column of"))?;
            column.close().map_err(parquet_err("closing a column of"))?;
            index += 1;
        }
        row_group
            .close()
            .map_err(parquet_err("closing a row group in"))?;

        self.pending = BarColumns::default();
        Ok(())
    }

    /// Flushes, records the metadata, closes the file, and renames it into
    /// place. Returns `None` when no rows were ever pushed — an empty
    /// partition is not written at all, because a zero-row file has no
    /// `first_ts_open` to record and nothing to replay.
    ///
    /// # Errors
    /// [`CuratedError::Parquet`] on a flush or close failure and
    /// [`CuratedError::Io`] if the file cannot be moved into place.
    pub fn finish(&mut self) -> Result<Option<CuratedMeta>, CuratedError> {
        self.flush_row_group()?;
        let writer = self.writer.take().ok_or_else(|| CuratedError::Parquet {
            path: self.tmp_path.clone(),
            during: "finishing",
            detail: "the writer was already closed".to_owned(),
        })?;

        let meta = match (self.first_ts_open, self.last_ts_open) {
            (Some(first), Some(last)) => Some(CuratedMeta {
                curated_schema_version: CURATED_SCHEMA_VERSION,
                transcoder_version: TRANSCODER_VERSION,
                source: self.source.clone(),
                row_count: self.row_count,
                first_ts_open: Ts(first),
                last_ts_open: Ts(last),
            }),
            _ => None,
        };

        let mut writer = writer;
        if let Some(meta) = &meta {
            for kv in meta.to_key_values() {
                writer.append_key_value_metadata(kv);
            }
        }
        writer.close().map_err(|e| CuratedError::Parquet {
            path: self.tmp_path.clone(),
            during: "closing",
            detail: e.to_string(),
        })?;
        self.finished = true;

        let Some(meta) = meta else {
            // Nothing to publish; the temp file is just noise.
            let _ = std::fs::remove_file(&self.tmp_path);
            return Ok(None);
        };

        // Curated output is derived and replaceable; `rename` onto an
        // existing file is an error on Windows, so clear the way first. This
        // is legal here and nowhere near `raw/`.
        if self.final_path.exists() {
            std::fs::remove_file(&self.final_path).map_err(|source| CuratedError::Io {
                path: self.final_path.clone(),
                during: "replacing",
                source,
            })?;
        }
        std::fs::rename(&self.tmp_path, &self.final_path).map_err(|source| CuratedError::Io {
            path: self.final_path.clone(),
            during: "moving the finished partition into",
            source,
        })?;
        Ok(Some(meta))
    }
}

impl Drop for PartitionWriter {
    fn drop(&mut self) {
        if !self.finished {
            // An abandoned write leaves no half-file behind for a later run
            // to trip over. Best effort: there is nothing useful to do with a
            // failure here, and the `.tmp` extension already keeps it out of
            // every reader's way.
            drop(self.writer.take());
            let _ = std::fs::remove_file(&self.tmp_path);
        }
    }
}

/// Writes columns and metadata **exactly as given**, with none of
/// [`PartitionWriter`]'s guards.
///
/// Test-only, and the reason it exists is the negative controls: every
/// refusal the reader implements has to be seen firing, and
/// [`PartitionWriter`] makes an out-of-order file, or one whose metadata
/// misstates its own rows, impossible to produce. A detector nobody has
/// watched fire is decoration (CLAUDE.md §7).
#[cfg(test)]
pub(crate) fn write_partition_unchecked(
    path: &Path,
    meta: &CuratedMeta,
    bars: &BarColumns,
) -> Result<(), CuratedError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CuratedError::Io {
            path: parent.to_path_buf(),
            during: "creating curated partition directory",
            source,
        })?;
    }
    let parquet_err = |during: &'static str| {
        let path = path.to_path_buf();
        move |e: parquet::errors::ParquetError| CuratedError::Parquet {
            path: path.clone(),
            during,
            detail: e.to_string(),
        }
    };
    let schema =
        Arc::new(parse_message_type(BARS_MESSAGE_TYPE).map_err(parquet_err("parsing schema of"))?);
    let file = File::create(path).map_err(|source| CuratedError::Io {
        path: path.to_path_buf(),
        during: "creating",
        source,
    })?;
    let mut writer = SerializedFileWriter::new(file, schema, Arc::new(WriterProperties::new()))
        .map_err(parquet_err("opening"))?;

    #[expect(
        clippy::cast_possible_wrap,
        reason = "test fixtures choose their own volumes; wrapping is the fixture's business"
    )]
    let volume: Vec<i64> = bars.volume.iter().map(|&v| v as i64).collect();
    let columns: [&[i64]; 6] = [
        &bars.ts_open,
        &bars.open,
        &bars.high,
        &bars.low,
        &bars.close,
        &volume,
    ];
    let mut row_group = writer
        .next_row_group()
        .map_err(parquet_err("starting a row group in"))?;
    let mut index = 0usize;
    while let Some(mut column) = row_group
        .next_column()
        .map_err(parquet_err("opening a column in"))?
    {
        column
            .typed::<Int64Type>()
            .write_batch(columns[index], None, None)
            .map_err(parquet_err("writing a column of"))?;
        column.close().map_err(parquet_err("closing a column of"))?;
        index += 1;
    }
    row_group
        .close()
        .map_err(parquet_err("closing a row group in"))?;
    for kv in meta.to_key_values() {
        writer.append_key_value_metadata(kv);
    }
    writer.close().map_err(parquet_err("closing"))?;
    Ok(())
}

/// Writes a whole partition in one call. Convenience for callers that
/// already hold every row — tests, and anything small enough not to care.
///
/// # Errors
/// Whatever [`PartitionWriter::push`] and [`PartitionWriter::finish`] return.
pub fn write_partition(
    data_dir: &Path,
    source: PartitionSource,
    window_stem: &str,
    bars: &BarColumns,
) -> Result<Option<CuratedMeta>, CuratedError> {
    let mut writer = PartitionWriter::create(data_dir, source, window_stem)?;
    for i in 0..bars.len() {
        writer.push(
            Ts(bars.ts_open[i]),
            Price::from_nanos(bars.open[i]),
            Price::from_nanos(bars.high[i]),
            Price::from_nanos(bars.low[i]),
            Price::from_nanos(bars.close[i]),
            bars.volume[i],
        )?;
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curated::path::partition_file;
    use crate::testutil::TempDir;
    use crucible_core::types::{InstrumentId, TimeFrame};

    fn source() -> PartitionSource {
        PartitionSource {
            instrument: InstrumentId::new("ESH4"),
            tf: TimeFrame::M1,
            dataset: "GLBX.MDP3".to_owned(),
            vendor_schema: "ohlcv-1m".to_owned(),
            source_file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_owned(),
            source_file_blake3: "00".repeat(32),
        }
    }

    // A bar that cannot exist in the file cannot be read back from it: the
    // writer is the first of the two ordering guards, and the cheaper one.
    #[test]
    fn the_writer_refuses_a_backward_or_repeated_timestamp() {
        let dir = TempDir::new();
        let mut writer = PartitionWriter::create(dir.path(), source(), "2024-01").expect("create");
        let px = Price::from_points(5000);
        writer.push(Ts(60), px, px, px, px, 1).expect("first bar");
        for repeat in [Ts(60), Ts(30)] {
            let err = writer
                .push(repeat, px, px, px, px, 1)
                .expect_err("must refuse a non-increasing ts_open");
            assert!(matches!(err, CuratedError::OutOfOrderRows { .. }), "{err}");
        }
    }

    // A crash mid-transcode must not leave a file a feed would try to read.
    #[test]
    fn an_unfinished_partition_leaves_nothing_behind() {
        let dir = TempDir::new();
        let final_path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        let tmp_path = final_path.with_extension(super::super::PARQUET_TMP_EXT);
        {
            let mut writer =
                PartitionWriter::create(dir.path(), source(), "2024-01").expect("create");
            let px = Price::from_points(5000);
            writer.push(Ts(60), px, px, px, px, 1).expect("push");
            assert!(
                tmp_path.exists(),
                "the temp file should exist while writing"
            );
        }
        assert!(!tmp_path.exists(), "the temp file must be cleaned up");
        assert!(!final_path.exists(), "nothing may be published");
    }

    // Zero rows is not an empty file, it is no file: there is no first
    // ts_open to record and nothing to replay.
    #[test]
    fn an_empty_partition_is_not_written_at_all() {
        let dir = TempDir::new();
        let mut writer = PartitionWriter::create(dir.path(), source(), "2024-01").expect("create");
        assert_eq!(writer.finish().expect("finish"), None);
        let final_path = partition_file(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("path");
        assert!(!final_path.exists());
    }

    // More rows than ROW_GROUP_ROWS exercises the incremental flush, which is
    // the whole reason a sixteen-year 1s window is transcodable at all.
    #[test]
    fn multiple_row_groups_round_trip() {
        let dir = TempDir::new();
        let n = ROW_GROUP_ROWS + 7;
        let mut writer = PartitionWriter::create(dir.path(), source(), "2024-01").expect("create");
        for i in 0..n {
            let ts = Ts(i as i64 * 60_000_000_000);
            let px = Price::from_nanos(5_000_000_000_000 + i as i64);
            writer.push(ts, px, px, px, px, i as u64).expect("push");
        }
        let meta = writer.finish().expect("finish").expect("rows were written");
        assert_eq!(meta.row_count, n as u64);
        assert_eq!(meta.first_ts_open, Ts(0));
        assert_eq!(meta.last_ts_open, Ts((n as i64 - 1) * 60_000_000_000));

        let feed = crate::curated::ParquetBarFeed::open(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            None,
        )
        .expect("open");
        assert_eq!(feed.len(), n);
    }

    // Curated output is derived: rebuilding must replace, not fail, and must
    // not depend on the platform's rename-over-existing behaviour.
    #[test]
    fn finishing_over_an_existing_partition_replaces_it() {
        let dir = TempDir::new();
        let px = Price::from_points(5000);
        for volume in [1u64, 2u64] {
            let mut writer =
                PartitionWriter::create(dir.path(), source(), "2024-01").expect("create");
            writer.push(Ts(60), px, px, px, px, volume).expect("push");
            writer.finish().expect("finish").expect("one row");
        }
        let mut feed = crate::curated::ParquetBarFeed::open(
            dir.path(),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            None,
        )
        .expect("open");
        let crucible_core::events::MarketEvent::Bar(bar) =
            crucible_core::traits::Feed::next_event(&mut feed).expect("a bar");
        assert_eq!(bar.volume, 2, "the second write must win");
    }
}
