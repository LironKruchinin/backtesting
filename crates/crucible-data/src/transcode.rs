//! Raw DBN → curated Parquet.
//!
//! **Status: implemented** for bar schemas (`ohlcv-1s`, `ohlcv-1m`,
//! `ohlcv-1h`, `ohlcv-1d`). `trades`, `tbbo`, `mbo`, `definition`, and
//! `statistics` are recognised and skipped with a stated reason — they are
//! not bars, and inventing a bar-shaped reading of them here would be the
//! sort of quiet assumption this project exists to avoid.
//!
//! ## What it is driven by
//!
//! The manifest, not a directory walk. `manifest.jsonl` is the archive's
//! record of what was bought and verified (D-0017), so transcoding from it
//! means a file nobody paid for and nobody checksummed cannot become curated
//! bars by being dropped into `raw/`. It also gives every curated file the
//! `file_blake3` of its source for free, which is the manifest id (D-0014)
//! that a result has to carry (D-0013).
//!
//! ## One raw file, many instruments
//!
//! A parent-symbology pull (`ES.FUT`) resolves to every outright and calendar
//! spread that traded in the window — the January 2024 validation slice
//! contains 41 of them (D-0033). Records are interleaved by time and carry a
//! numeric `instrument_id`, not a symbol, because submissions pin
//! `stype_out=instrument_id` and `map_symbols=false`; the mapping lives in the
//! DBN header. So transcode holds one open [`PartitionWriter`] per
//! instrument, resolves each record through
//! [`Metadata::symbol_map`](databento::dbn::Metadata::symbol_map), and flushes
//! row groups as buffers fill. Live memory is bounded by (open instruments ×
//! [`ROW_GROUP_ROWS`](crate::curated::ROW_GROUP_ROWS)) regardless of how long
//! the window is — which is what makes a sixteen-year `ohlcv-1s` `Whole`
//! window transcodable at all.
//!
//! ## Every uncertainty is a refusal
//!
//! Unlike a pull, a transcode costs nothing to repeat: curated data is
//! derived, disposable, and rebuildable. So where `ingest` had to weigh a
//! refusal against stranding bytes that had already been paid for (D-0033),
//! this module simply stops. A record is refused if:
//!
//! - its `rtype` is not the one the file's schema promises (an `OhlcvMsg`
//!   decodes happily whether it is a 1-second or a 1-day bar, so the schema
//!   is the authority and the record has to agree with it);
//! - any of its OHLC fields is the vendor's `UNDEF_PRICE` sentinel, or is not
//!   strictly positive — a price of zero prices a trade at nothing;
//! - its `ts_event` is not a whole multiple of the bar interval, because
//!   `avail_ts = ts_open + tf` is only meaningful for an aligned open (D-0003);
//! - its `instrument_id` has no symbol for that date, since bars nobody can
//!   name cannot be filed, replayed, or reported;
//! - it repeats or precedes the previous `ts_open` for its instrument.
//!
//! A refusal costs one re-run. A guess costs a research result nobody can
//! reproduce.
//!
//! ## Timestamps
//!
//! Pass through untouched, as UTC nanoseconds. `ohlcv`'s `ts_event` marks the
//! interval START and is stored as `ts_open`; availability is `ts_open + tf`
//! and is computed by [`Bar::avail_ts`](crucible_core::events::Bar::avail_ts),
//! never here (`ingest`'s rules, CLAUDE.md §2.1).
//!
//! [`PartitionWriter`]: crate::curated::PartitionWriter

use std::path::PathBuf;

use crucible_core::types::TimeFrame;

use crate::curated::CuratedError;

/// Vendor bar schemas this transcoder understands, and the interval each one
/// means.
///
/// Databento aggregates only at these four intervals; `5m` and `15m`
/// [`TimeFrame`]s exist for resampling that has not been built yet, and are
/// deliberately absent rather than silently mapped onto something else.
#[must_use]
pub fn timeframe_for_schema(schema: &str) -> Option<TimeFrame> {
    match schema {
        "ohlcv-1s" => Some(TimeFrame::S1),
        "ohlcv-1m" => Some(TimeFrame::M1),
        "ohlcv-1h" => Some(TimeFrame::H1),
        "ohlcv-1d" => Some(TimeFrame::D1),
        _ => None,
    }
}

/// What to transcode, and how insistently.
#[derive(Debug, Clone, Default)]
pub struct TranscodeOptions {
    /// Only write partitions for these instruments. `None` means every
    /// instrument the file resolves to — including calendar spreads, which
    /// are real instruments even though no strategy trades them yet.
    pub symbols: Option<Vec<String>>,
    /// Only transcode manifest records with these ids (`file_blake3`).
    pub manifest_ids: Option<Vec<String>>,
    /// Rewrite partitions that already exist. Without this they are left
    /// alone, so re-running is cheap and idempotent.
    pub force: bool,
}

/// One curated partition that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenPartition {
    /// Instrument the partition holds.
    pub instrument: String,
    /// Where it landed.
    pub path: PathBuf,
    /// Bars written.
    pub rows: u64,
}

/// What happened to one manifest record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
    /// Bars were written.
    Transcoded {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Interval the schema resolved to.
        tf: TimeFrame,
        /// Partitions produced.
        partitions: Vec<WrittenPartition>,
        /// Partitions left alone because they already held these bytes.
        skipped: usize,
    },
    /// Every partition already existed and `--force` was not given.
    AlreadyCurated {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// How many partitions were found in place.
        partitions: usize,
    },
    /// Not transcoded, and why.
    Skipped {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// The reason, phrased for an operator.
        reason: String,
    },
}

impl RecordOutcome {
    /// Archive-relative path of the record this outcome describes.
    #[must_use]
    pub fn source_file_path(&self) -> &str {
        match self {
            RecordOutcome::Transcoded {
                source_file_path, ..
            }
            | RecordOutcome::AlreadyCurated {
                source_file_path, ..
            }
            | RecordOutcome::Skipped {
                source_file_path, ..
            } => source_file_path,
        }
    }
}

/// The result of a whole transcode run, in manifest order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscodeReport {
    /// One entry per manifest record considered.
    pub outcomes: Vec<RecordOutcome>,
}

impl TranscodeReport {
    /// Total bars written across every partition.
    #[must_use]
    pub fn rows_written(&self) -> u64 {
        self.outcomes
            .iter()
            .map(|outcome| match outcome {
                RecordOutcome::Transcoded { partitions, .. } => {
                    partitions.iter().map(|p| p.rows).sum()
                }
                _ => 0,
            })
            .sum()
    }

    /// Total partitions written.
    #[must_use]
    pub fn partitions_written(&self) -> usize {
        self.outcomes
            .iter()
            .map(|outcome| match outcome {
                RecordOutcome::Transcoded { partitions, .. } => partitions.len(),
                _ => 0,
            })
            .sum()
    }
}

impl core::fmt::Display for TranscodeReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.outcomes.is_empty() {
            return writeln!(f, "nothing in the manifest matched.");
        }
        for outcome in &self.outcomes {
            match outcome {
                RecordOutcome::Transcoded {
                    source_file_path,
                    tf,
                    partitions,
                    skipped,
                } => {
                    let rows: u64 = partitions.iter().map(|p| p.rows).sum();
                    writeln!(
                        f,
                        "  {source_file_path}\n    -> {} partition(s), {rows} bar(s) at {tf}",
                        partitions.len()
                    )?;
                    // Never let a bounded run read as a complete one.
                    if *skipped > 0 {
                        writeln!(
                            f,
                            "       {skipped} partition(s) already held these bytes and were \
                             left alone; --force to rebuild"
                        )?;
                    }
                }
                RecordOutcome::AlreadyCurated {
                    source_file_path,
                    partitions,
                } => writeln!(
                    f,
                    "  {source_file_path}\n    already curated ({partitions} partition(s)); \
                     --force to rebuild"
                )?,
                RecordOutcome::Skipped {
                    source_file_path,
                    reason,
                } => writeln!(f, "  {source_file_path}\n    skipped: {reason}")?,
            }
        }
        writeln!(
            f,
            "\n{} partition(s), {} bar(s) written.",
            self.partitions_written(),
            self.rows_written()
        )
    }
}

/// Why a transcode could not finish.
#[derive(Debug)]
pub enum TranscodeError {
    /// A curated read or write failed.
    Curated(CuratedError),
    /// The raw file could not be opened or decoded as DBN.
    Undecodable {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Explanation from the decoder.
        detail: String,
    },
    /// The DBN header carries no usable symbology, so records cannot be
    /// attributed to instruments.
    NoSymbolMap {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Explanation from the decoder.
        detail: String,
    },
    /// A record could not be trusted. Named precisely enough to find it.
    BadRecord {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Zero-based index of the record within the file.
        record_index: u64,
        /// What was wrong.
        reason: String,
    },
    /// A curated file for this window already exists but was produced from
    /// different raw bytes.
    SourceConflict {
        /// The curated file already in place.
        curated_path: PathBuf,
        /// Manifest id recorded in it.
        existing_blake3: String,
        /// Manifest id of the record being transcoded.
        incoming_blake3: String,
    },
}

impl core::fmt::Display for TranscodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TranscodeError::Curated(e) => write!(f, "{e}"),
            TranscodeError::Undecodable {
                source_file_path,
                detail,
            } => write!(f, "could not decode {source_file_path}: {detail}"),
            TranscodeError::NoSymbolMap {
                source_file_path,
                detail,
            } => write!(
                f,
                "{source_file_path} carries no instrument-id symbology ({detail}). \
                 Its records name instruments by number, so without the header's \
                 mapping there is no way to say whose bars these are"
            ),
            TranscodeError::BadRecord {
                source_file_path,
                record_index,
                reason,
            } => write!(
                f,
                "{source_file_path} record #{record_index}: {reason}. Refusing to \
                 curate a file this build cannot read with confidence — nothing \
                 was written, and re-running after the cause is understood costs \
                 nothing"
            ),
            TranscodeError::SourceConflict {
                curated_path,
                existing_blake3,
                incoming_blake3,
            } => write!(
                f,
                "{} was transcoded from manifest id {existing_blake3} but this \
                 record is {incoming_blake3}. Two raw windows computing one \
                 curated path is a naming bug, not something to overwrite",
                curated_path.display()
            ),
        }
    }
}

impl std::error::Error for TranscodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TranscodeError::Curated(e) => Some(e),
            _ => None,
        }
    }
}

impl From<CuratedError> for TranscodeError {
    fn from(e: CuratedError) -> TranscodeError {
        TranscodeError::Curated(e)
    }
}

#[cfg(feature = "databento")]
pub use imp::transcode;

#[cfg(feature = "databento")]
mod imp {
    use std::collections::BTreeMap;
    use std::path::Path;

    use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};
    use databento::dbn::decode::{DbnMetadata, DecodeRecord, dbn::Decoder};
    use databento::dbn::{RType, Record, SymbolIndex, UNDEF_PRICE, record::OhlcvMsg};

    use crate::catalog::{Catalog, ManifestRecord};
    use crate::curated::path::window_stem_of;
    use crate::curated::{CuratedMeta, PartitionSource, PartitionWriter, read_meta};

    use super::{
        RecordOutcome, TranscodeError, TranscodeOptions, TranscodeReport, WrittenPartition,
        timeframe_for_schema,
    };

    /// The DBN record type an interval's bars must carry.
    fn rtype_for(tf: TimeFrame) -> Option<RType> {
        match tf {
            TimeFrame::S1 => Some(RType::Ohlcv1S),
            TimeFrame::M1 => Some(RType::Ohlcv1M),
            TimeFrame::H1 => Some(RType::Ohlcv1H),
            TimeFrame::D1 => Some(RType::Ohlcv1D),
            TimeFrame::M5 | TimeFrame::M15 => None,
        }
    }

    /// Transcodes every manifest record matching `opts` into curated Parquet.
    ///
    /// # Errors
    /// [`TranscodeError`] on an undecodable file, a record that cannot be
    /// trusted, or a curated write failure. Nothing partial is published: a
    /// failed record leaves its `.parquet.tmp` files removed and its previous
    /// curated output untouched.
    pub fn transcode(
        catalog: &Catalog,
        opts: &TranscodeOptions,
    ) -> Result<TranscodeReport, TranscodeError> {
        let mut report = TranscodeReport::default();
        for record in catalog.records() {
            if let Some(ids) = &opts.manifest_ids
                && !ids.contains(&record.file_blake3)
            {
                continue;
            }
            let Some(tf) = timeframe_for_schema(&record.schema) else {
                report.outcomes.push(RecordOutcome::Skipped {
                    source_file_path: record.file_path.clone(),
                    reason: format!(
                        "schema `{}` is not a bar schema; transcode handles \
                         ohlcv-1s/1m/1h/1d",
                        record.schema
                    ),
                });
                continue;
            };
            report
                .outcomes
                .push(transcode_record(catalog.data_dir(), record, tf, opts)?);
        }
        Ok(report)
    }

    /// Transcodes one raw file into one curated partition per instrument.
    fn transcode_record(
        data_dir: &Path,
        record: &ManifestRecord,
        tf: TimeFrame,
        opts: &TranscodeOptions,
    ) -> Result<RecordOutcome, TranscodeError> {
        let Some(window_stem) = window_stem_of(&record.file_path) else {
            return Ok(RecordOutcome::Skipped {
                source_file_path: record.file_path.clone(),
                reason: "the archive path has no window stem to name a partition after".to_owned(),
            });
        };
        let raw_path = data_dir.join(&record.file_path);
        let undecodable = |detail: String| TranscodeError::Undecodable {
            source_file_path: record.file_path.clone(),
            detail,
        };

        let mut decoder =
            Decoder::from_zstd_file(&raw_path).map_err(|e| undecodable(e.to_string()))?;
        let symbol_map =
            decoder
                .metadata()
                .symbol_map()
                .map_err(|e| TranscodeError::NoSymbolMap {
                    source_file_path: record.file_path.clone(),
                    detail: e.to_string(),
                })?;

        let expected_rtype = rtype_for(tf).ok_or_else(|| TranscodeError::Undecodable {
            source_file_path: record.file_path.clone(),
            detail: format!("{tf} bars are not a vendor schema"),
        })?;
        let interval_ns = tf.duration_ns();

        // BTreeMap, not HashMap: the report is part of the output, and
        // iteration order must not depend on hashing (CLAUDE.md §2.2).
        let mut writers: BTreeMap<String, PartitionWriter> = BTreeMap::new();
        let mut index: u64 = 0;
        let bad = |reason: String, index: u64| TranscodeError::BadRecord {
            source_file_path: record.file_path.clone(),
            record_index: index,
            reason,
        };

        // Which instruments are already curated is decided per partition, at
        // the moment the first bar for one shows up. It cannot be decided up
        // front from the header: the symbology maps every contract the parent
        // key *resolves to*, and most of them never trade a bar in the window
        // — the January 2024 ES.FUT slice maps 41 and produces 16. Treating
        // the mapped set as the expected set makes a completed transcode look
        // permanently unfinished.
        let mut already: Vec<String> = Vec::new();

        while let Some(msg) = decoder
            .decode_record::<OhlcvMsg>()
            .map_err(|e| undecodable(e.to_string()))?
        {
            let header = msg.header();
            if header.rtype != expected_rtype as u8 {
                return Err(bad(
                    format!(
                        "rtype {} is not the {} this file's schema promises",
                        header.rtype, expected_rtype as u8
                    ),
                    index,
                ));
            }
            let ts_event = i64::try_from(header.ts_event)
                .map_err(|_| bad(format!("ts_event {} exceeds i64", header.ts_event), index))?;
            if ts_event % interval_ns != 0 {
                return Err(bad(
                    format!(
                        "ts_event {ts_event} is not a whole multiple of the {tf} interval, \
                         so `avail_ts = ts_open + tf` would not describe when this bar \
                         became knowable"
                    ),
                    index,
                ));
            }
            for (name, value) in [
                ("open", msg.open),
                ("high", msg.high),
                ("low", msg.low),
                ("close", msg.close),
            ] {
                if value == UNDEF_PRICE {
                    return Err(bad(format!("{name} is the UNDEF_PRICE sentinel"), index));
                }
                if value <= 0 {
                    return Err(bad(
                        format!("{name} is {value}, not a positive price"),
                        index,
                    ));
                }
            }
            let symbol = symbol_map.get_for_rec(msg).ok_or_else(|| {
                bad(
                    format!(
                        "instrument_id {} has no symbol on this date in the file's own \
                         header mappings",
                        header.instrument_id
                    ),
                    index,
                )
            })?;

            let wanted = opts
                .symbols
                .as_ref()
                .is_none_or(|filter| filter.iter().any(|s| s == symbol));
            if wanted && !already.iter().any(|s| s == symbol) {
                let writer = match writers.get_mut(symbol.as_str()) {
                    Some(writer) => Some(writer),
                    None => {
                        let source = PartitionSource {
                            instrument: InstrumentId::new(symbol.as_str()),
                            tf,
                            dataset: record.dataset.clone(),
                            vendor_schema: record.schema.clone(),
                            source_file_path: record.file_path.clone(),
                            source_file_blake3: record.file_blake3.clone(),
                        };
                        // Existing output from *these* bytes is left alone
                        // unless asked; from different bytes it is a refusal.
                        match existing_partition(data_dir, &source, window_stem, record)? {
                            Some(()) if !opts.force => {
                                already.push(symbol.clone());
                                None
                            }
                            _ => Some(writers.entry(symbol.clone()).or_insert(
                                PartitionWriter::create(data_dir, source, window_stem)?,
                            )),
                        }
                    }
                };
                if let Some(writer) = writer {
                    writer.push(
                        Ts(ts_event),
                        Price::from_nanos(msg.open),
                        Price::from_nanos(msg.high),
                        Price::from_nanos(msg.low),
                        Price::from_nanos(msg.close),
                        msg.volume,
                    )?;
                }
            }
            index += 1;
        }

        let mut partitions = Vec::with_capacity(writers.len());
        for (instrument, mut writer) in writers {
            if let Some(meta) = writer.finish()? {
                partitions.push(WrittenPartition {
                    instrument,
                    path: writer.final_path().to_path_buf(),
                    rows: meta.row_count,
                });
            }
        }

        if partitions.is_empty() && !already.is_empty() {
            return Ok(RecordOutcome::AlreadyCurated {
                source_file_path: record.file_path.clone(),
                partitions: already.len(),
            });
        }
        Ok(RecordOutcome::Transcoded {
            source_file_path: record.file_path.clone(),
            tf,
            partitions,
            skipped: already.len(),
        })
    }

    /// Reads an existing partition, if any, and refuses when it came from a
    /// different raw file.
    fn existing_meta(
        data_dir: &Path,
        instrument: &str,
        tf: TimeFrame,
        window_stem: &str,
    ) -> Result<Option<CuratedMeta>, TranscodeError> {
        let path = crate::curated::path::partition_file(
            data_dir,
            &InstrumentId::new(instrument),
            tf,
            window_stem,
        )?;
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(read_meta(&path)?))
    }

    /// Reports whether a partition for this instrument and window already
    /// exists, refusing when the one on disk came from different raw bytes.
    fn existing_partition(
        data_dir: &Path,
        source: &PartitionSource,
        window_stem: &str,
        record: &ManifestRecord,
    ) -> Result<Option<()>, TranscodeError> {
        let Some(meta) =
            existing_meta(data_dir, source.instrument.as_str(), source.tf, window_stem)?
        else {
            return Ok(None);
        };
        if meta.source.source_file_blake3 != record.file_blake3 {
            let curated_path = crate::curated::path::partition_file(
                data_dir,
                &source.instrument,
                source.tf,
                window_stem,
            )?;
            return Err(TranscodeError::SourceConflict {
                curated_path,
                existing_blake3: meta.source.source_file_blake3,
                incoming_blake3: record.file_blake3.clone(),
            });
        }
        Ok(Some(()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use std::fs::File;
        use std::num::NonZeroU64;

        use crate::SyntheticFeed;
        use crate::catalog::{Acquisition, TsRange};
        use crate::curated::ParquetBarFeed;
        use crate::testutil::TempDir;
        use crucible_core::events::MarketEvent;
        use crucible_core::traits::Feed;
        use databento::dbn::encode::EncodeRecord;
        use databento::dbn::encode::dbn::Encoder;
        use databento::dbn::{
            MappingInterval, Metadata, RecordHeader, SType, Schema, SymbolMapping,
        };

        /// 2024-01-01T00:00:00Z, and minute-aligned — which every `ohlcv-1m`
        /// `ts_event` from the vendor is, and which transcode insists on.
        const JAN1: i64 = 1_704_067_200_000_000_000;
        const FEB1: i64 = 1_706_745_600_000_000_000;
        const MIN: i64 = 60_000_000_000;
        const RAW_PATH: &str = "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst";

        /// One bar to encode: instrument id, ts_event, open/high/low/close, volume.
        #[derive(Clone, Copy)]
        struct Row(u32, i64, i64, i64, i64, i64, u64);

        fn january(day: u8) -> time::Date {
            time::Date::from_calendar_date(2024, time::Month::January, day).expect("a January date")
        }

        /// Writes a `.dbn.zst` whose header maps ids to symbols exactly the way
        /// a real parent-symbology delivery does: `stype_out=instrument_id`, so
        /// `raw_symbol` is the contract and the interval's `symbol` is the id.
        #[expect(
            clippy::cast_sign_loss,
            reason = "fixture timestamps are positive by construction"
        )]
        fn write_dbn(path: &Path, symbols: &[(u32, &str)], rows: &[Row], rtype: u8) {
            std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
            let mappings: Vec<SymbolMapping> = symbols
                .iter()
                .map(|(id, symbol)| SymbolMapping {
                    raw_symbol: (*symbol).to_owned(),
                    intervals: vec![MappingInterval {
                        start_date: january(1),
                        end_date: january(31),
                        symbol: id.to_string(),
                    }],
                })
                .collect();
            let metadata = Metadata::builder()
                .dataset("GLBX.MDP3".to_owned())
                .schema(Some(Schema::Ohlcv1M))
                .start(JAN1 as u64)
                .end(NonZeroU64::new(FEB1 as u64))
                .stype_in(Some(SType::Parent))
                .stype_out(SType::InstrumentId)
                .symbols(vec!["ES.FUT".to_owned()])
                .mappings(mappings)
                .build();

            let file = File::create(path).expect("create dbn");
            let mut encoder = Encoder::with_zstd(file, &metadata).expect("encoder");
            for row in rows {
                let msg = OhlcvMsg {
                    hd: RecordHeader::new::<OhlcvMsg>(rtype, 1, row.0, row.1 as u64),
                    open: row.2,
                    high: row.3,
                    low: row.4,
                    close: row.5,
                    volume: row.6,
                };
                encoder.encode_record(&msg).expect("encode");
            }
            drop(encoder);
        }

        /// A catalog holding exactly one record for the file just written.
        fn catalog_for(dir: &TempDir, schema: &str, rel_path: &str) -> Catalog {
            let mut catalog = Catalog::open(dir.path()).expect("open catalog");
            catalog
                .append(Acquisition {
                    dataset: "GLBX.MDP3".to_owned(),
                    schema: schema.to_owned(),
                    symbols: vec!["ES.FUT".to_owned()],
                    range: TsRange::new(Ts(JAN1), Ts(FEB1)).expect("valid range"),
                    acquired_ts: Ts(JAN1),
                    databento_job_id: "TEST-JOB".to_owned(),
                    file_path: rel_path.to_owned(),
                })
                .expect("append");
            catalog
        }

        fn ohlcv_1m_rtype() -> u8 {
            RType::Ohlcv1M as u8
        }

        fn one_bar(id: u32, minute: i64) -> Row {
            #[expect(
                clippy::cast_sign_loss,
                reason = "minute indices in fixtures are small and non-negative"
            )]
            let volume = 100 + minute as u64;
            Row(
                id,
                JAN1 + minute * MIN,
                5_000_000_000_000,
                5_001_000_000_000,
                4_999_000_000_000,
                5_000_500_000_000,
                volume,
            )
        }

        fn setup(symbols: &[(u32, &str)], rows: &[Row], rtype: u8) -> (TempDir, Catalog) {
            let dir = TempDir::new();
            write_dbn(&dir.path().join(RAW_PATH), symbols, rows, rtype);
            let catalog = catalog_for(&dir, "ohlcv-1m", RAW_PATH);
            (dir, catalog)
        }

        fn bars_of(dir: &TempDir, symbol: &str) -> Vec<MarketEvent> {
            let mut feed =
                ParquetBarFeed::open(dir.path(), &InstrumentId::new(symbol), TimeFrame::M1, None)
                    .expect("open the curated partition");
            std::iter::from_fn(|| feed.next_event()).collect()
        }

        /// No curated `.parquet` anywhere under the data dir.
        fn nothing_published(dir: &TempDir) -> bool {
            fn walk(path: &Path) -> bool {
                let Ok(entries) = std::fs::read_dir(path) else {
                    return true;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if !walk(&path) {
                            return false;
                        }
                    } else if path.extension().and_then(|e| e.to_str()) == Some("parquet") {
                        return false;
                    }
                }
                true
            }
            walk(&dir.path().join("curated"))
        }

        // -------------------------------------------------------- the golden

        // Synthetic bars through a real DBN encode, a real transcode, and a
        // real Parquet read must come back byte-identical. No vendor data is
        // involved, so this runs in CI (testdata/README.md rule 1).
        //
        // Timestamps are re-stamped onto a minute-aligned grid: SyntheticFeed's
        // fixed base is 2023-11-14T22:13:20Z, twenty seconds off a minute
        // boundary, and transcode refuses an unaligned `ts_event` because
        // `avail_ts = ts_open + tf` would not describe when such a bar became
        // knowable. The prices — the part Parquet has to carry losslessly —
        // are the generator's own.
        #[test]
        fn synthetic_bars_survive_dbn_transcode_and_parquet_bit_identically() {
            let mut generator = SyntheticFeed::random_walk(
                42,
                240,
                TimeFrame::M1,
                Price::from_points(5000),
                Price::from_points_f64_lossy(0.25),
                4,
            );
            let generated: Vec<MarketEvent> =
                std::iter::from_fn(|| generator.next_event()).collect();

            let rows: Vec<Row> = generated
                .iter()
                .enumerate()
                .map(|(i, MarketEvent::Bar(bar))| {
                    Row(
                        7001,
                        JAN1 + i as i64 * MIN,
                        bar.open.as_nanos(),
                        bar.high.as_nanos(),
                        bar.low.as_nanos(),
                        bar.close.as_nanos(),
                        bar.volume,
                    )
                })
                .collect();
            let expected: Vec<MarketEvent> = rows
                .iter()
                .map(|row| {
                    MarketEvent::Bar(crucible_core::events::Bar {
                        instrument: InstrumentId::new("ESH4"),
                        tf: TimeFrame::M1,
                        ts_open: Ts(row.1),
                        open: Price::from_nanos(row.2),
                        high: Price::from_nanos(row.3),
                        low: Price::from_nanos(row.4),
                        close: Price::from_nanos(row.5),
                        volume: row.6,
                    })
                })
                .collect();

            let (dir, catalog) = setup(&[(7001, "ESH4")], &rows, ohlcv_1m_rtype());
            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.partitions_written(), 1);
            assert_eq!(report.rows_written(), 240);
            assert_eq!(bars_of(&dir, "ESH4"), expected);
        }

        #[test]
        fn one_raw_file_fans_out_into_one_partition_per_instrument() {
            let rows = vec![
                one_bar(1, 0),
                one_bar(2, 0),
                one_bar(1, 1),
                one_bar(2, 1),
                one_bar(2, 2),
            ];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESH4-ESM4")], &rows, ohlcv_1m_rtype());
            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.partitions_written(), 2);
            assert_eq!(report.rows_written(), 5);
            assert_eq!(bars_of(&dir, "ESH4").len(), 2);
            // Calendar spreads are instruments too, and dropping them here
            // would quietly discard bars the archive paid for (D-0033).
            assert_eq!(bars_of(&dir, "ESH4-ESM4").len(), 3);
        }

        #[test]
        fn a_symbol_filter_writes_only_what_was_asked_for() {
            let rows = vec![one_bar(1, 0), one_bar(2, 0)];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESM4")], &rows, ohlcv_1m_rtype());
            let opts = TranscodeOptions {
                symbols: Some(vec!["ESH4".to_owned()]),
                ..TranscodeOptions::default()
            };
            let report = transcode(&catalog, &opts).expect("transcode");
            assert_eq!(report.partitions_written(), 1);
            assert_eq!(bars_of(&dir, "ESH4").len(), 1);
            assert!(
                ParquetBarFeed::open(dir.path(), &InstrumentId::new("ESM4"), TimeFrame::M1, None)
                    .is_err()
            );
        }

        #[test]
        fn a_non_bar_schema_is_skipped_with_a_reason() {
            let dir = TempDir::new();
            let rel = "raw/GLBX.MDP3/trades/ES.FUT/2024-01.dbn.zst";
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(&path, b"not really dbn").expect("write");
            let catalog = catalog_for(&dir, "trades", rel);

            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.rows_written(), 0);
            assert!(matches!(
                report.outcomes.as_slice(),
                [RecordOutcome::Skipped { reason, .. }] if reason.contains("not a bar schema")
            ));
        }

        // ------------------------------------------------- negative controls

        fn refusal_reason(rows: &[Row], symbols: &[(u32, &str)], rtype: u8) -> String {
            let (dir, catalog) = setup(symbols, rows, rtype);
            let err = transcode(&catalog, &TranscodeOptions::default())
                .expect_err("must refuse this file");
            assert!(
                nothing_published(&dir),
                "a refused record must publish nothing: {err}"
            );
            err.to_string()
        }

        #[test]
        fn an_undefined_price_is_refused_and_nothing_is_published() {
            let mut bad = one_bar(1, 1);
            bad.3 = UNDEF_PRICE;
            let reason = refusal_reason(&[one_bar(1, 0), bad], &[(1, "ESH4")], ohlcv_1m_rtype());
            assert!(reason.contains("UNDEF_PRICE"), "{reason}");
        }

        #[test]
        fn a_nonpositive_price_is_refused() {
            let mut bad = one_bar(1, 0);
            bad.4 = 0;
            let reason = refusal_reason(&[bad], &[(1, "ESH4")], ohlcv_1m_rtype());
            assert!(reason.contains("not a positive price"), "{reason}");
        }

        // An OhlcvMsg decodes whether it is a 1-second or a 1-day bar, so the
        // file's schema is the authority and the record has to agree.
        #[test]
        fn a_record_from_the_wrong_interval_is_refused() {
            let reason = refusal_reason(&[one_bar(1, 0)], &[(1, "ESH4")], RType::Ohlcv1S as u8);
            assert!(reason.contains("rtype"), "{reason}");
        }

        #[test]
        fn an_unaligned_timestamp_is_refused() {
            let mut bad = one_bar(1, 0);
            bad.1 += 20_000_000_000; // twenty seconds into the minute
            let reason = refusal_reason(&[bad], &[(1, "ESH4")], ohlcv_1m_rtype());
            assert!(reason.contains("whole multiple"), "{reason}");
        }

        #[test]
        fn an_instrument_id_with_no_symbol_is_refused() {
            // Id 9 trades, but the header maps only id 1.
            let reason = refusal_reason(
                &[one_bar(1, 0), one_bar(9, 1)],
                &[(1, "ESH4")],
                ohlcv_1m_rtype(),
            );
            assert!(reason.contains("no symbol"), "{reason}");
        }

        #[test]
        fn a_repeated_timestamp_for_one_instrument_is_refused() {
            let reason = refusal_reason(
                &[one_bar(1, 0), one_bar(1, 0)],
                &[(1, "ESH4")],
                ohlcv_1m_rtype(),
            );
            assert!(reason.contains("ts_open"), "{reason}");
        }

        // ------------------------------------------------------- idempotency

        // The regression this guards: a parent key's symbology maps every
        // contract it *resolves to*, and most of them never trade a bar in the
        // window — the real January 2024 ES.FUT slice maps 41 and produces 16.
        // Deciding "already curated" from the mapped set therefore made every
        // completed transcode look unfinished, and re-ran forever. Here id 2 is
        // mapped and never trades.
        #[test]
        fn a_second_run_writes_nothing_and_force_rebuilds() {
            let rows = vec![one_bar(1, 0), one_bar(1, 1)];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESM4")], &rows, ohlcv_1m_rtype());

            let first = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(first.partitions_written(), 1);

            let again = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(again.partitions_written(), 0);
            assert!(matches!(
                again.outcomes.as_slice(),
                [RecordOutcome::AlreadyCurated { partitions: 1, .. }]
            ));

            let forced = transcode(
                &catalog,
                &TranscodeOptions {
                    force: true,
                    ..TranscodeOptions::default()
                },
            )
            .expect("transcode");
            assert_eq!(forced.partitions_written(), 1);
            assert_eq!(bars_of(&dir, "ESH4").len(), 2);
        }

        // Two raw windows computing one curated path is a naming bug. Silently
        // overwriting would leave a partition whose provenance metadata names
        // bytes it does not contain.
        #[test]
        fn a_partition_from_different_raw_bytes_is_refused() {
            let dir = TempDir::new();
            write_dbn(
                &dir.path().join(RAW_PATH),
                &[(1, "ESH4")],
                &[one_bar(1, 0)],
                ohlcv_1m_rtype(),
            );
            let catalog = catalog_for(&dir, "ohlcv-1m", RAW_PATH);
            transcode(&catalog, &TranscodeOptions::default()).expect("first transcode");

            // Same window, different bytes, recorded under a second path so the
            // manifest accepts it (raw is immutable, D-0017).
            let other_rel = "raw/GLBX.MDP3/ohlcv-1m/ES.FUT2/2024-01.dbn.zst";
            write_dbn(
                &dir.path().join(other_rel),
                &[(1, "ESH4")],
                &[one_bar(1, 0), one_bar(1, 1)],
                ohlcv_1m_rtype(),
            );
            let mut catalog = Catalog::open(dir.path()).expect("reopen");
            catalog
                .append(Acquisition {
                    dataset: "GLBX.MDP3".to_owned(),
                    schema: "ohlcv-1m".to_owned(),
                    symbols: vec!["ES.FUT".to_owned()],
                    range: TsRange::new(Ts(JAN1), Ts(FEB1)).expect("range"),
                    acquired_ts: Ts(JAN1),
                    databento_job_id: "TEST-JOB-2".to_owned(),
                    file_path: other_rel.to_owned(),
                })
                .expect("append");

            let err = transcode(
                &catalog,
                &TranscodeOptions {
                    manifest_ids: None,
                    symbols: None,
                    force: true,
                },
            )
            .expect_err("must refuse a foreign source");
            assert!(
                matches!(err, TranscodeError::SourceConflict { .. }),
                "{err}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_vendor_bar_schemas_map_to_a_timeframe() {
        assert_eq!(timeframe_for_schema("ohlcv-1s"), Some(TimeFrame::S1));
        assert_eq!(timeframe_for_schema("ohlcv-1m"), Some(TimeFrame::M1));
        assert_eq!(timeframe_for_schema("ohlcv-1h"), Some(TimeFrame::H1));
        assert_eq!(timeframe_for_schema("ohlcv-1d"), Some(TimeFrame::D1));
        // Not bars. Mapping any of these onto a timeframe would be inventing
        // an aggregation nobody chose.
        for schema in [
            "trades",
            "tbbo",
            "mbo",
            "mbp-10",
            "definition",
            "statistics",
        ] {
            assert_eq!(timeframe_for_schema(schema), None, "{schema}");
        }
        // Databento does not aggregate at these; the TimeFrame variants exist
        // for resampling that has not been built.
        for schema in ["ohlcv-5m", "ohlcv-15m", ""] {
            assert_eq!(timeframe_for_schema(schema), None, "{schema}");
        }
    }

    #[test]
    fn an_empty_report_says_so_rather_than_printing_nothing() {
        let report = TranscodeReport::default();
        assert!(
            report
                .to_string()
                .contains("nothing in the manifest matched")
        );
        assert_eq!(report.rows_written(), 0);
        assert_eq!(report.partitions_written(), 0);
    }

    #[test]
    fn a_report_totals_partitions_and_rows() {
        let report = TranscodeReport {
            outcomes: vec![
                RecordOutcome::Transcoded {
                    source_file_path: "raw/a.dbn.zst".to_owned(),
                    tf: TimeFrame::M1,
                    partitions: vec![
                        WrittenPartition {
                            instrument: "ESH4".to_owned(),
                            path: PathBuf::from("curated/bars/ESH4/1m/2024-01.parquet"),
                            rows: 7,
                        },
                        WrittenPartition {
                            instrument: "ESM4".to_owned(),
                            path: PathBuf::from("curated/bars/ESM4/1m/2024-01.parquet"),
                            rows: 3,
                        },
                    ],
                    skipped: 5,
                },
                RecordOutcome::Skipped {
                    source_file_path: "raw/b.dbn.zst".to_owned(),
                    reason: "schema `trades` is not a bar schema".to_owned(),
                },
            ],
        };
        assert_eq!(report.partitions_written(), 2);
        assert_eq!(report.rows_written(), 10);
        let text = report.to_string();
        assert!(text.contains("raw/a.dbn.zst"), "{text}");
        assert!(text.contains("2 partition(s), 10 bar(s) written"), "{text}");
        assert!(text.contains("not a bar schema"), "{text}");
        // A bounded run must never read as a complete one.
        assert!(
            text.contains("5 partition(s) already held these bytes"),
            "{text}"
        );
    }
}
