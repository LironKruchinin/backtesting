//! What a curated file says about itself.
//!
//! Every fact a reader needs that is not a column lives in Parquet's
//! file-level key-value metadata, under a `crucible.` prefix. Two reasons for
//! metadata rather than a sidecar file or a row in `manifest.jsonl`:
//!
//! - It cannot be separated from the bytes it describes. A sidecar can be
//!   deleted, copied without its file, or left behind after a rebuild; the
//!   footer travels with the data.
//! - `manifest.jsonl` records **acquisitions**, and curated files are not
//!   acquisitions — they are derived, disposable, and rebuildable
//!   (`catalog`'s rules). Recording them there would make the archive's
//!   append-only history lie about what was bought.
//!
//! `source_file_blake3` is the load-bearing one: it is the manifest id
//! (D-0014) of the raw file this partition was transcoded from, so a result
//! produced through [`ParquetBarFeed`](super::ParquetBarFeed) can name the
//! exact bytes it read, which is what D-0013 requires of every persisted
//! number.

use std::path::Path;

use crucible_core::types::{InstrumentId, TimeFrame, Ts};
use parquet::file::metadata::KeyValue;

use super::CuratedError;

/// Metadata key: version of the curated file format.
pub const KEY_SCHEMA_VERSION: &str = "crucible.curated_schema_version";
/// Metadata key: version of the DBN → bar transcoding semantics.
pub const KEY_TRANSCODER_VERSION: &str = "crucible.transcoder_version";
/// Metadata key: the instrument every row belongs to.
pub const KEY_INSTRUMENT: &str = "crucible.instrument";
/// Metadata key: the bar interval, spelled as `TimeFrame` spells it.
pub const KEY_TIMEFRAME: &str = "crucible.timeframe";
/// Metadata key: the vendor dataset the source file came from.
pub const KEY_DATASET: &str = "crucible.dataset";
/// Metadata key: the vendor schema the source file was requested under.
pub const KEY_VENDOR_SCHEMA: &str = "crucible.vendor_schema";
/// Metadata key: archive-relative path of the raw file this came from.
pub const KEY_SOURCE_FILE_PATH: &str = "crucible.source_file_path";
/// Metadata key: blake3 of the raw source file — the manifest id (D-0014).
pub const KEY_SOURCE_FILE_BLAKE3: &str = "crucible.source_file_blake3";
/// Metadata key: number of rows in the file.
pub const KEY_ROW_COUNT: &str = "crucible.row_count";
/// Metadata key: first `ts_open` in the file.
pub const KEY_FIRST_TS_OPEN: &str = "crucible.first_ts_open";
/// Metadata key: last `ts_open` in the file.
pub const KEY_LAST_TS_OPEN: &str = "crucible.last_ts_open";

/// Everything about a partition that is fixed before the first row is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionSource {
    /// The single instrument this partition holds.
    pub instrument: InstrumentId,
    /// Bar interval.
    pub tf: TimeFrame,
    /// Vendor dataset, e.g. `GLBX.MDP3`.
    pub dataset: String,
    /// Vendor schema, e.g. `ohlcv-1m`.
    pub vendor_schema: String,
    /// Archive-relative path of the raw file, e.g.
    /// `raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst`.
    pub source_file_path: String,
    /// Lowercase-hex blake3 of that raw file — its manifest id (D-0014).
    pub source_file_blake3: String,
}

/// A curated file's complete self-description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedMeta {
    /// Curated file-format version; must be understood or the read refuses.
    pub curated_schema_version: u32,
    /// Transcoder semantics version; surfaced, never a blocker.
    pub transcoder_version: u32,
    /// Fixed facts about where these bars came from.
    pub source: PartitionSource,
    /// Rows in the file.
    pub row_count: u64,
    /// First `ts_open`.
    pub first_ts_open: Ts,
    /// Last `ts_open`.
    pub last_ts_open: Ts,
}

impl CuratedMeta {
    /// Renders as Parquet key-value metadata, in a fixed order.
    ///
    /// Fixed order because two runs of the same transcode over the same bytes
    /// should differ in nothing a diff can see (CLAUDE.md §2.2).
    #[must_use]
    pub fn to_key_values(&self) -> Vec<KeyValue> {
        let kv = |k: &str, v: String| KeyValue::new(k.to_owned(), v);
        vec![
            kv(KEY_SCHEMA_VERSION, self.curated_schema_version.to_string()),
            kv(KEY_TRANSCODER_VERSION, self.transcoder_version.to_string()),
            kv(KEY_INSTRUMENT, self.source.instrument.as_str().to_owned()),
            kv(KEY_TIMEFRAME, self.source.tf.to_string()),
            kv(KEY_DATASET, self.source.dataset.clone()),
            kv(KEY_VENDOR_SCHEMA, self.source.vendor_schema.clone()),
            kv(KEY_SOURCE_FILE_PATH, self.source.source_file_path.clone()),
            kv(
                KEY_SOURCE_FILE_BLAKE3,
                self.source.source_file_blake3.clone(),
            ),
            kv(KEY_ROW_COUNT, self.row_count.to_string()),
            kv(KEY_FIRST_TS_OPEN, self.first_ts_open.0.to_string()),
            kv(KEY_LAST_TS_OPEN, self.last_ts_open.0.to_string()),
        ]
    }

    /// Parses Parquet key-value metadata read back from `path`.
    ///
    /// # Errors
    /// [`CuratedError::MissingMetadata`] for an absent key and
    /// [`CuratedError::MalformedMetadata`] for one that cannot be parsed. A
    /// file that does not describe itself completely is refused rather than
    /// defaulted: every default here would be a guess about what the bytes
    /// mean.
    pub fn from_key_values(path: &Path, kvs: &[KeyValue]) -> Result<CuratedMeta, CuratedError> {
        let get = |key: &'static str| -> Result<&str, CuratedError> {
            kvs.iter()
                .find(|kv| kv.key == key)
                .and_then(|kv| kv.value.as_deref())
                .ok_or(CuratedError::MissingMetadata {
                    path: path.to_path_buf(),
                    key,
                })
        };
        let malformed = |key: &'static str, value: &str| CuratedError::MalformedMetadata {
            path: path.to_path_buf(),
            key,
            value: value.to_owned(),
        };
        let num = |key: &'static str| -> Result<u64, CuratedError> {
            let raw = get(key)?;
            raw.parse::<u64>().map_err(|_| malformed(key, raw))
        };
        let ts = |key: &'static str| -> Result<Ts, CuratedError> {
            let raw = get(key)?;
            raw.parse::<i64>().map(Ts).map_err(|_| malformed(key, raw))
        };
        let version = |key: &'static str| -> Result<u32, CuratedError> {
            let raw = get(key)?;
            raw.parse::<u32>().map_err(|_| malformed(key, raw))
        };

        let tf_raw = get(KEY_TIMEFRAME)?;
        let tf = tf_raw
            .parse::<TimeFrame>()
            .map_err(|_| malformed(KEY_TIMEFRAME, tf_raw))?;

        Ok(CuratedMeta {
            curated_schema_version: version(KEY_SCHEMA_VERSION)?,
            transcoder_version: version(KEY_TRANSCODER_VERSION)?,
            source: PartitionSource {
                instrument: InstrumentId::new(get(KEY_INSTRUMENT)?),
                tf,
                dataset: get(KEY_DATASET)?.to_owned(),
                vendor_schema: get(KEY_VENDOR_SCHEMA)?.to_owned(),
                source_file_path: get(KEY_SOURCE_FILE_PATH)?.to_owned(),
                source_file_blake3: get(KEY_SOURCE_FILE_BLAKE3)?.to_owned(),
            },
            row_count: num(KEY_ROW_COUNT)?,
            first_ts_open: ts(KEY_FIRST_TS_OPEN)?,
            last_ts_open: ts(KEY_LAST_TS_OPEN)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> CuratedMeta {
        CuratedMeta {
            curated_schema_version: super::super::CURATED_SCHEMA_VERSION,
            transcoder_version: super::super::TRANSCODER_VERSION,
            source: PartitionSource {
                instrument: InstrumentId::new("ESH4"),
                tf: TimeFrame::M1,
                dataset: "GLBX.MDP3".to_owned(),
                vendor_schema: "ohlcv-1m".to_owned(),
                source_file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_owned(),
                source_file_blake3:
                    "2304cedfe99de7811ef2fa90cf9ff9f6a3bcfc0cca22ee134f4e031647e16ecc".to_owned(),
            },
            row_count: 3,
            first_ts_open: Ts(1_704_067_200_000_000_000),
            last_ts_open: Ts(1_704_067_320_000_000_000),
        }
    }

    #[test]
    fn key_values_round_trip() {
        let meta = sample();
        let kvs = meta.to_key_values();
        let back = CuratedMeta::from_key_values(Path::new("x.parquet"), &kvs).expect("parses");
        assert_eq!(back, meta);
    }

    #[test]
    fn key_order_is_fixed() {
        let kvs = sample().to_key_values();
        let keys: Vec<&str> = kvs.iter().map(|kv| kv.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                KEY_SCHEMA_VERSION,
                KEY_TRANSCODER_VERSION,
                KEY_INSTRUMENT,
                KEY_TIMEFRAME,
                KEY_DATASET,
                KEY_VENDOR_SCHEMA,
                KEY_SOURCE_FILE_PATH,
                KEY_SOURCE_FILE_BLAKE3,
                KEY_ROW_COUNT,
                KEY_FIRST_TS_OPEN,
                KEY_LAST_TS_OPEN,
            ]
        );
    }

    #[test]
    fn a_missing_key_is_named() {
        let mut kvs = sample().to_key_values();
        kvs.retain(|kv| kv.key != KEY_INSTRUMENT);
        let err = CuratedMeta::from_key_values(Path::new("x.parquet"), &kvs)
            .expect_err("must refuse incomplete metadata");
        assert!(
            matches!(err, CuratedError::MissingMetadata { key, .. } if key == KEY_INSTRUMENT),
            "{err}"
        );
    }

    #[test]
    fn an_unparseable_value_is_named_with_its_key() {
        for (key, bad) in [
            (KEY_SCHEMA_VERSION, "one"),
            (KEY_TIMEFRAME, "2m"),
            (KEY_ROW_COUNT, "-1"),
            (KEY_FIRST_TS_OPEN, "yesterday"),
        ] {
            let mut kvs = sample().to_key_values();
            for kv in &mut kvs {
                if kv.key == key {
                    kv.value = Some(bad.to_owned());
                }
            }
            let err = CuratedMeta::from_key_values(Path::new("x.parquet"), &kvs)
                .expect_err("must refuse a malformed value");
            assert!(
                matches!(&err, CuratedError::MalformedMetadata { key: k, .. } if *k == key),
                "{key}: {err}"
            );
        }
    }
}
