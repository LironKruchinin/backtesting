//! Validated CSV to Parquet, under `external/thetadata/`.
//!
//! ## Types are chosen by what the column *is*, not by what parses
//!
//! Three classes, assigned by column name against the pinned header (§4.1), so
//! that a vendor reordering cannot change a column's type:
//!
//! - **Money → `i64` nano-USD.** Prices are accounting (§2.3), so they never
//!   become `f64` on the way in. The vendor prints two decimals for options
//!   and three for strikes; both are exact in nano-USD.
//! - **Counts → `i64`.** Volume, trade count, quote sizes, exchange and
//!   condition codes, open interest.
//! - **Statistics → `f64`.** Greeks, implied volatility, `iv_error`, `d1`/`d2`.
//!   These are indicator/statistics space and §2.3 puts them in `f64`
//!   deliberately — they must never flow back into accounting, and the type is
//!   the reminder.
//!
//! Timestamps are the fourth case and the one with a rule attached: the vendor
//! writes Eastern wall clock with no offset (`2024-01-02T17:25:19.675`), so
//! every one goes through [`eastern_wall_clock_to_ts`] and lands as UTC
//! nanoseconds. Ambiguous and nonexistent stamps are refused rather than
//! resolved (D-0052) — a stamp that becomes an `avail_ts` cannot be guessed.
//!
//! `expiration` stays vendor text. It is an identity, not an instant: two
//! different expirations must never be able to collide through a parse, and
//! nothing downstream does arithmetic on it that a date type would help with.
//!
//! ## Placement
//!
//! Write to a temporary name in the destination directory, then rename. The
//! same discipline as the curated writer, for the same reason: a reader must
//! never observe a partially written file at a real path, and rename within a
//! directory is atomic on every filesystem this runs on.
//!
//! ## What the footer carries
//!
//! Provenance that cannot be separated from the bytes it describes: the
//! request, the endpoint, the blake3 of the *raw response*, the row and
//! contract counts, the dup rate, and the schema version. A sidecar can drift
//! from its file; a footer cannot. This is the same argument D-0036 makes for
//! curated bars.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parquet::basic::{Compression, ZstdLevel};
use parquet::data_type::{ByteArray, ByteArrayType, DoubleType, Int64Type};
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::parser::parse_message_type;

use crate::calendar::{CivilDate, eastern_wall_clock_to_ts};

use super::error::ThetaError;
use super::schema::Endpoint;
use super::validate::ValidatedResponse;

/// Bumped when the meaning of a written column changes.
pub const THETA_CURATED_SCHEMA_VERSION: u32 = 1;

/// How one column is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// Vendor text kept verbatim: identities, never arithmetic.
    Text,
    /// An Eastern wall-clock stamp, stored as UTC nanoseconds.
    Timestamp,
    /// Money, stored as `i64` nano-USD.
    Money,
    /// A count or code, stored as `i64`.
    Count,
    /// A statistic, stored as `f64`.
    Statistic,
}

impl ColumnKind {
    /// Classifies a pinned column by name.
    ///
    /// By name and not by position, and exhaustive rather than defaulting: an
    /// unrecognised column is a pin that grew without anyone deciding what it
    /// means, and guessing `Statistic` for it would quietly put a price into
    /// `f64`. The pin and this function are changed together or not at all.
    #[must_use]
    pub fn of(column: &str) -> Option<ColumnKind> {
        Some(match column {
            "symbol" | "expiration" | "right" => ColumnKind::Text,
            "created" | "timestamp" | "last_trade" | "underlying_timestamp" => {
                ColumnKind::Timestamp
            }
            "strike" | "open" | "high" | "low" | "close" | "bid" | "ask" | "vwap"
            | "underlying_price" => ColumnKind::Money,
            "volume" | "count" | "bid_size" | "ask_size" | "bid_exchange" | "ask_exchange"
            | "bid_condition" | "ask_condition" | "open_interest" => ColumnKind::Count,
            "delta" | "theta" | "vega" | "rho" | "epsilon" | "lambda" | "gamma" | "vanna"
            | "charm" | "vomma" | "veta" | "vera" | "speed" | "zomma" | "color" | "ultima"
            | "d1" | "d2" | "dual_delta" | "dual_gamma" | "implied_vol" | "iv_error" => {
                ColumnKind::Statistic
            }
            _ => return None,
        })
    }

    /// The Parquet physical type this kind is written as.
    #[must_use]
    pub fn parquet_type(self) -> &'static str {
        match self {
            ColumnKind::Text => "BYTE_ARRAY",
            ColumnKind::Timestamp | ColumnKind::Money | ColumnKind::Count => "INT64",
            ColumnKind::Statistic => "DOUBLE",
        }
    }
}

/// Builds the Parquet message type for an endpoint's pinned header.
///
/// Every column is `optional`, which is not laziness: the vendor leaves fields
/// blank (a contract with no last trade) and the alternative to a null is a
/// sentinel, which is the whole class of bug §4.3 exists to catch. A null says
/// "absent"; a zero says "zero", and this vendor has already proved it does not
/// distinguish them.
///
/// # Errors
/// [`ThetaError::UnexpectedColumns`] if the pin contains a column
/// [`ColumnKind::of`] does not classify.
pub fn message_type(endpoint: Endpoint) -> Result<String, ThetaError> {
    let mut out = String::from("message thetadata {\n");
    for column in endpoint.pinned_header() {
        let kind = ColumnKind::of(column).ok_or_else(|| ThetaError::UnexpectedColumns {
            path: endpoint.path().to_owned(),
            expected: vec![format!(
                "a classified column, but `{column}` has no ColumnKind"
            )],
            found: endpoint
                .pinned_header()
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        })?;
        let suffix = if kind == ColumnKind::Text {
            " (UTF8)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  optional {} {}{};\n",
            kind.parquet_type(),
            column,
            suffix
        ));
    }
    out.push('}');
    Ok(out)
}

/// Parses a decimal price into `i64` nano-USD without going through `f64`.
///
/// `f64` would be accurate enough for two decimal places and is still refused:
/// §2.3 puts the price path in integers, and a conversion that is "accurate
/// enough" is the shape of every rounding bug that ever reached production.
/// The vendor prints at most three decimals, so nine is generous.
///
/// # Errors
/// [`ThetaError::MalformedRow`] if the text is not a decimal number.
pub fn parse_nano_usd(text: &str, request_path: &str, row: u64) -> Result<i64, ThetaError> {
    let bad = |detail: String| ThetaError::MalformedRow {
        path: request_path.to_owned(),
        row,
        detail,
    };
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, frac) = match digits.split_once('.') {
        Some((w, f)) => (w, f),
        None => (digits, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return Err(bad(format!("`{text}` is not a price")));
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad(format!("`{text}` is not a decimal number")));
    }
    if frac.len() > 9 {
        return Err(bad(format!(
            "`{text}` carries more precision than nano-USD can hold"
        )));
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .map_err(|_| bad(format!("`{text}` overflows i64 whole dollars")))?
    };
    let mut scaled = String::from(frac);
    while scaled.len() < 9 {
        scaled.push('0');
    }
    let nanos: i64 = if scaled.is_empty() {
        0
    } else {
        scaled
            .parse()
            .map_err(|_| bad(format!("`{text}` has an unparseable fraction")))?
    };
    let magnitude = whole
        .checked_mul(1_000_000_000)
        .and_then(|w| w.checked_add(nanos))
        .ok_or_else(|| bad(format!("`{text}` overflows nano-USD")))?;
    Ok(if negative { -magnitude } else { magnitude })
}

/// Parses the vendor's `YYYY-MM-DDTHH:MM:SS.mmm` Eastern stamp into UTC nanos.
///
/// # Errors
/// [`ThetaError::MalformedRow`] on a shape this does not recognise, and
/// [`ThetaError::Timestamp`] when the wall-clock instant is ambiguous or does
/// not exist (D-0052).
pub fn parse_eastern_stamp(text: &str, request_path: &str, row: u64) -> Result<i64, ThetaError> {
    let bad = |detail: String| ThetaError::MalformedRow {
        path: request_path.to_owned(),
        row,
        detail,
    };
    let text = text.trim();
    let (date_part, time_part) = text
        .split_once('T')
        .ok_or_else(|| bad(format!("`{text}` is not an ISO-8601 local timestamp")))?;

    let mut date_fields = date_part.split('-');
    let year: i64 = next_number(&mut date_fields, text, &bad)?;
    let month: u32 = u32::try_from(next_number(&mut date_fields, text, &bad)?)
        .map_err(|_| bad(format!("`{text}` has an impossible month")))?;
    let day: u32 = u32::try_from(next_number(&mut date_fields, text, &bad)?)
        .map_err(|_| bad(format!("`{text}` has an impossible day")))?;

    let (clock, frac) = match time_part.split_once('.') {
        Some((c, f)) => (c, f),
        None => (time_part, ""),
    };
    let mut clock_fields = clock.split(':');
    let hour: u32 = u32::try_from(next_number(&mut clock_fields, text, &bad)?)
        .map_err(|_| bad(format!("`{text}` has an impossible hour")))?;
    let minute: u32 = u32::try_from(next_number(&mut clock_fields, text, &bad)?)
        .map_err(|_| bad(format!("`{text}` has an impossible minute")))?;
    let second: u32 = u32::try_from(next_number(&mut clock_fields, text, &bad)?)
        .map_err(|_| bad(format!("`{text}` has an impossible second")))?;

    // Fractional seconds are left-aligned: `.5` is 500ms, not 5ns.
    let mut padded = String::from(frac);
    if padded.len() > 9 {
        return Err(bad(format!("`{text}` carries sub-nanosecond precision")));
    }
    while padded.len() < 9 {
        padded.push('0');
    }
    let nanos: u32 = if padded.is_empty() {
        0
    } else {
        padded
            .parse()
            .map_err(|_| bad(format!("`{text}` has an unparseable fraction")))?
    };

    let date = CivilDate { year, month, day };
    eastern_wall_clock_to_ts(date, hour, minute, second, nanos)
        .map(|ts| ts.0)
        .map_err(|source| ThetaError::Timestamp {
            path: request_path.to_owned(),
            row,
            source,
        })
}

/// Pulls the next `-`/`:`-separated integer out of a split iterator.
fn next_number<'a, F>(
    fields: &mut impl Iterator<Item = &'a str>,
    whole: &str,
    bad: &F,
) -> Result<i64, ThetaError>
where
    F: Fn(String) -> ThetaError,
{
    let field = fields
        .next()
        .ok_or_else(|| bad(format!("`{whole}` is missing a component")))?;
    field
        .parse()
        .map_err(|_| bad(format!("`{whole}` has a non-numeric component `{field}`")))
}

/// Everything the footer records about where a file came from.
#[derive(Debug, Clone)]
pub struct TranscodeSource {
    /// The rendered request that produced the response.
    pub request: String,
    /// blake3 of the raw response body, hex.
    pub response_blake3: String,
}

/// Writes one validated response as Parquet at `destination`.
///
/// Returns the number of bytes written.
///
/// # Errors
/// [`ThetaError::Io`] or [`ThetaError::Parquet`] on any write failure, and
/// whatever the per-cell parsers return on malformed data.
pub fn write_parquet(
    response: &ValidatedResponse,
    source: &TranscodeSource,
    destination: &Path,
    request_path: &str,
) -> Result<u64, ThetaError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ThetaError::Io {
            path: parent.to_path_buf(),
            during: "creating the thetadata output directory",
            source,
        })?;
    }
    let temp = temp_sibling(destination);
    let columns = encode_columns(response, request_path)?;

    let parquet_err = |during: &'static str| {
        let path = temp.clone();
        move |e: parquet::errors::ParquetError| ThetaError::Parquet {
            path: path.clone(),
            during,
            detail: e.to_string(),
        }
    };

    let message = message_type(response.endpoint)?;
    let schema =
        Arc::new(parse_message_type(&message).map_err(parquet_err("parsing the schema of"))?);
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let file = File::create(&temp).map_err(|source| ThetaError::Io {
        path: temp.clone(),
        during: "creating",
        source,
    })?;
    let mut writer = SerializedFileWriter::new(file, schema, Arc::new(properties))
        .map_err(parquet_err("opening"))?;

    {
        let mut group = writer
            .next_row_group()
            .map_err(parquet_err("starting a row group in"))?;
        let mut i = 0usize;
        while let Some(mut column) = group
            .next_column()
            .map_err(parquet_err("opening a column in"))?
        {
            match &columns[i] {
                Encoded::Int(values, defs) => column
                    .typed::<Int64Type>()
                    .write_batch(values, Some(defs), None)
                    .map_err(parquet_err("writing a column of"))?,
                Encoded::Float(values, defs) => column
                    .typed::<DoubleType>()
                    .write_batch(values, Some(defs), None)
                    .map_err(parquet_err("writing a column of"))?,
                Encoded::Text(values, defs) => column
                    .typed::<ByteArrayType>()
                    .write_batch(values, Some(defs), None)
                    .map_err(parquet_err("writing a column of"))?,
            };
            column.close().map_err(parquet_err("closing a column of"))?;
            i += 1;
        }
        group
            .close()
            .map_err(parquet_err("closing a row group in"))?;
    }

    for kv in footer(response, source) {
        writer.append_key_value_metadata(kv);
    }
    writer.close().map_err(parquet_err("closing"))?;

    let size = std::fs::metadata(&temp)
        .map_err(|source| ThetaError::Io {
            path: temp.clone(),
            during: "measuring",
            source,
        })?
        .len();
    std::fs::rename(&temp, destination).map_err(|source| ThetaError::Io {
        path: destination.to_path_buf(),
        during: "placing",
        source,
    })?;
    Ok(size)
}

/// A temporary name beside the destination, so the rename stays intra-directory.
fn temp_sibling(destination: &Path) -> PathBuf {
    let name = destination.file_name().map_or_else(
        || "thetadata".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    destination.with_file_name(format!(".{name}.partial"))
}

/// One column's worth of encoded values plus its definition levels.
enum Encoded {
    Int(Vec<i64>, Vec<i16>),
    Float(Vec<f64>, Vec<i16>),
    Text(Vec<ByteArray>, Vec<i16>),
}

/// Encodes every column of a validated response.
fn encode_columns(
    response: &ValidatedResponse,
    request_path: &str,
) -> Result<Vec<Encoded>, ThetaError> {
    let mut out = Vec::with_capacity(response.index.len());
    for column in response.endpoint.pinned_header() {
        let kind = ColumnKind::of(column).ok_or_else(|| ThetaError::UnexpectedColumns {
            path: request_path.to_owned(),
            expected: vec![format!(
                "a classified column, but `{column}` has no ColumnKind"
            )],
            found: vec![(*column).to_owned()],
        })?;
        let mut defs = Vec::with_capacity(response.rows.len());
        match kind {
            ColumnKind::Text => {
                let mut values = Vec::with_capacity(response.rows.len());
                for row in &response.rows {
                    let raw = row.get(&response.index, column).unwrap_or_default().trim();
                    if raw.is_empty() {
                        defs.push(0);
                    } else {
                        defs.push(1);
                        values.push(ByteArray::from(raw.as_bytes()));
                    }
                }
                out.push(Encoded::Text(values, defs));
            }
            ColumnKind::Statistic => {
                let mut values = Vec::with_capacity(response.rows.len());
                for row in &response.rows {
                    let raw = row.get(&response.index, column).unwrap_or_default().trim();
                    if raw.is_empty() {
                        defs.push(0);
                        continue;
                    }
                    let value = raw.parse::<f64>().map_err(|_| ThetaError::MalformedRow {
                        path: request_path.to_owned(),
                        row: row.row,
                        detail: format!("`{raw}` is not a number in column {column}"),
                    })?;
                    defs.push(1);
                    values.push(value);
                }
                out.push(Encoded::Float(values, defs));
            }
            ColumnKind::Money | ColumnKind::Count | ColumnKind::Timestamp => {
                let mut values = Vec::with_capacity(response.rows.len());
                for row in &response.rows {
                    let raw = row.get(&response.index, column).unwrap_or_default().trim();
                    if raw.is_empty() {
                        defs.push(0);
                        continue;
                    }
                    let value = match kind {
                        ColumnKind::Money => parse_nano_usd(raw, request_path, row.row)?,
                        ColumnKind::Timestamp => parse_eastern_stamp(raw, request_path, row.row)?,
                        _ => raw.parse::<i64>().map_err(|_| ThetaError::MalformedRow {
                            path: request_path.to_owned(),
                            row: row.row,
                            detail: format!("`{raw}` is not an integer in column {column}"),
                        })?,
                    };
                    defs.push(1);
                    values.push(value);
                }
                out.push(Encoded::Int(values, defs));
            }
        }
    }
    Ok(out)
}

/// The footer's key-value provenance.
fn footer(
    response: &ValidatedResponse,
    source: &TranscodeSource,
) -> Vec<parquet::file::metadata::KeyValue> {
    let kv = |k: &str, v: String| parquet::file::metadata::KeyValue {
        key: k.to_owned(),
        value: Some(v),
    };
    vec![
        kv(
            "theta_schema_version",
            THETA_CURATED_SCHEMA_VERSION.to_string(),
        ),
        kv("endpoint", response.endpoint.path().to_owned()),
        kv("request", source.request.clone()),
        kv("response_blake3", source.response_blake3.clone()),
        kv("raw_rows", response.report.raw_rows.to_string()),
        kv("distinct_rows", response.report.distinct_rows.to_string()),
        kv("dup_rate", format!("{:.3}", response.report.dup_rate())),
        kv(
            "conflicting_pairs",
            response.report.conflicting_pairs.to_string(),
        ),
        kv(
            "sentinel_rows_dropped",
            response.report.sentinel_rows_dropped.to_string(),
        ),
    ]
}

#[cfg(test)]
#[path = "transcode/tests.rs"]
mod tests;
