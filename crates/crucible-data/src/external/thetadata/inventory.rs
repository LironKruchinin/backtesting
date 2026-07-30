//! `external/thetadata/inventory.jsonl` — what we fetched, from where, and
//! what it looked like when it arrived.
//!
//! ## Why this is not `manifest.jsonl`
//!
//! The manifest makes one narrow promise: *this tool fetched these bytes from
//! Databento and hashed them at the moment of placement* (D-0017). An inventory
//! makes a weaker one: *a vendor served this request on this date and we wrote
//! the result here*. Merging the two would silently downgrade the first to the
//! second for everything already in it, which is why D-0049 keeps them apart
//! and why nothing in this module ever writes to the manifest.
//!
//! ## Resume is a diff, never a directory listing
//!
//! A tranche resumes by asking "which of the requests I am about to make does
//! the inventory already record?" — never "which files exist?". The difference
//! is the failure that matters: a file half-written when a run died is present
//! on disk and absent from the inventory, so a listing-based resume would skip
//! it and a diff-based one re-fetches it. Records are appended **after** the
//! file is durably in place, so a record is a statement about a complete file.
//!
//! ## Append-only, LF-framed, one JSON object per line
//!
//! The same discipline as [`Catalog`](crate::Catalog): open for append, write
//! one line, flush, and never rewrite an earlier line. A line is only ever
//! added, so a reader that gets a truncated final line has hit a crashed write
//! and can discard exactly that line without losing anything before it.
//!
//! ## What a line carries, and why each field earns its place
//!
//! The obvious half is provenance: the rendered request, the file path, its
//! blake3, its size. The rest is the *era fingerprint* — dup rate, build
//! distribution, conflicting pairs, sentinel drops, reconciliation results —
//! recorded at fetch time because it cannot be recovered afterwards. Once a
//! file is deduplicated on the way to disk, nothing on disk remembers that it
//! arrived 2.000× duplicated, and "was this era duplicated?" stops being
//! answerable from the archive. D-0054 makes that question load-bearing.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::ThetaError;
use super::validate::{Reconciliation, ValidationReport};

/// How many times [`Inventory::append`] tries to open the file before failing.
///
/// A reader holding a deny-share handle makes the append's `open` fail with
/// `ERROR_SHARING_VIOLATION`, and that is what most "just check the progress"
/// one-liners do on Windows: `Get-Content` and `System.IO.StreamReader` both
/// open with `FileShare.Read`, which denies writers. The contention lasts
/// milliseconds; the run it killed was two and a half hours old and 830
/// requests in (2026-07-30). Resume being free is not a licence to die on
/// observation — a glance at progress must not be able to end a multi-day
/// tranche.
///
/// Ten attempts at [`APPEND_RETRY_PAUSE`] spans ~4.5 s of contention, which is
/// far longer than any reader that is merely counting lines, and still bounded
/// so a genuinely stuck holder fails the run instead of hanging it.
pub const APPEND_ATTEMPTS: u32 = 10;

/// Pause between the attempts [`APPEND_ATTEMPTS`] allows.
pub const APPEND_RETRY_PAUSE: Duration = Duration::from_millis(500);

/// Whether an I/O error is "another process holds this file with a deny-share
/// handle".
///
/// `ERROR_SHARING_VIOLATION` (32) and `ERROR_LOCK_VIOLATION` (33). Gated to
/// Windows on purpose: POSIX has no mandatory sharing, so nothing there raises
/// these on `open`, and errno 32 is `EPIPE` — retrying *that* would paper over
/// an unrelated failure rather than a transient reader.
#[cfg(windows)]
fn is_sharing_violation(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33))
}

#[cfg(not(windows))]
fn is_sharing_violation(_error: &std::io::Error) -> bool {
    false
}

/// Schema version of an inventory line.
///
/// Bumped whenever the meaning of an existing field changes. A reader refuses
/// a version it does not know rather than guessing, because a silently
/// misread `dup_rate` is exactly the kind of number this file exists to keep
/// honest.
pub const INVENTORY_SCHEMA_VERSION: u32 = 1;

/// File name of the per-vendor inventory, under `external/thetadata/`.
pub const INVENTORY_FILE: &str = "inventory.jsonl";

/// One completed fetch-and-write.
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryRecord {
    /// Version of this line's schema.
    pub schema_version: u32,
    /// Endpoint path, e.g. `/option/history/greeks/eod`.
    pub endpoint: String,
    /// Root symbol, e.g. `SPY`.
    pub root: String,
    /// Granularity, e.g. `eod` or `1m`.
    pub grain: String,
    /// First date covered, `YYYY-MM-DD`.
    pub start_date: String,
    /// Last date covered, `YYYY-MM-DD`.
    pub end_date: String,
    /// The rendered path and query that produced this file.
    ///
    /// Verbatim, because the Terminal ignores unknown query parameters
    /// silently: what was *asked* is not recoverable from what came back, and
    /// this string is the only record of it.
    pub request: String,
    /// Path of the written file, relative to the data dir.
    pub file_path: String,
    /// blake3 of the written file.
    pub file_blake3: String,
    /// Size of the written file in bytes.
    pub size_bytes: u64,
    /// Rows the vendor sent, before dedup.
    pub row_count: u64,
    /// Distinct identities — the completeness unit (D-0054).
    pub distinct_contracts: u64,
    /// `row_count / distinct_contracts`; the era fingerprint.
    pub dup_rate: f64,
    /// How many identities carried how many build stamps.
    pub n_builds_distribution: BTreeMap<usize, u64>,
    /// Repeats whose market fields differed across builds.
    pub conflicting_pairs: u64,
    /// Rows dropped by the zero-sentinel condition.
    pub sentinel_rows_dropped: u64,
    /// Wall time this request took, in milliseconds, including retries and
    /// any wait on the pacer.
    ///
    /// Recorded per request because T1's projection needs the real latency
    /// **distribution by era**, and an inventory line is already keyed by date
    /// — so the archive measures its own acquisition for free. `0` means the
    /// line predates the field rather than "instant".
    pub fetch_millis: u64,
    /// Fraction of kept rows whose OHLC block was entirely zero, or `None`
    /// where the endpoint has no OHLC or nothing was kept.
    ///
    /// The second era fingerprint. A high rate is ordinary on a thin chain —
    /// most contracts do not trade on most days — so this is never a gate; it
    /// is written down so that a *change* in the rate is visible. D-0055
    /// scoped the all-zero refusal by exactly this distinction, and the number
    /// that settled it (VIX 2024-01-02: 672 of 1,058) would have been
    /// unavailable if nobody had been recording it.
    pub zero_ohlc_rate: Option<f64>,
    /// Reconciliation outcome for this (root, day), when one was computed.
    pub reconciliation: Option<Reconciliation>,
    /// When the fetch completed, as UTC nanoseconds.
    ///
    /// Passed in by the caller, never read from a clock here: `crucible-data`
    /// does not read the wall clock (§2.2, D-0032), and the one implementation
    /// that does lives in a bin target.
    pub fetched_ts: i64,
}

impl InventoryRecord {
    /// Builds a record from a validation report, filling the derived fields.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "an inventory line IS this many independent facts; bundling them \
                  into a struct just to pass them would be the same arguments with \
                  a longer path"
    )]
    pub fn new(
        endpoint: &str,
        root: &str,
        grain: &str,
        start_date: &str,
        end_date: &str,
        request: &str,
        file_path: &str,
        file_blake3: &str,
        size_bytes: u64,
        report: &ValidationReport,
        reconciliation: Option<Reconciliation>,
        fetched_ts: i64,
    ) -> InventoryRecord {
        InventoryRecord {
            schema_version: INVENTORY_SCHEMA_VERSION,
            endpoint: endpoint.to_owned(),
            root: root.to_owned(),
            grain: grain.to_owned(),
            start_date: start_date.to_owned(),
            end_date: end_date.to_owned(),
            request: request.to_owned(),
            file_path: file_path.to_owned(),
            file_blake3: file_blake3.to_owned(),
            size_bytes,
            row_count: report.raw_rows,
            distinct_contracts: report.distinct_rows,
            dup_rate: report.dup_rate(),
            n_builds_distribution: report.n_builds_distribution.clone(),
            conflicting_pairs: report.conflicting_pairs,
            sentinel_rows_dropped: report.sentinel_rows_dropped,
            fetch_millis: 0,
            zero_ohlc_rate: report.zero_ohlc_rate(),
            reconciliation,
            fetched_ts,
        }
    }

    /// The key a resume diff matches on.
    ///
    /// Deliberately the *request*, not the file path: two different requests
    /// must never be considered the same work because they happened to write
    /// to paths that collide, and the same request re-issued must always be
    /// recognised no matter where its output landed.
    #[must_use]
    pub fn resume_key(&self) -> &str {
        &self.request
    }

    /// Renders the record as one JSON object.
    ///
    /// Hand-rolled rather than `serde`-derived so that field order is fixed and
    /// a line diffs cleanly against the one before it. Every string is escaped;
    /// none of the values here can contain a control character, but escaping
    /// them anyway keeps that a property of the writer rather than of the data.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        let builds = self
            .n_builds_distribution
            .iter()
            .map(|(k, v)| format!("\"{k}\":{v}"))
            .collect::<Vec<_>>()
            .join(",");
        let reconciliation = match &self.reconciliation {
            None => "null".to_owned(),
            Some(r) => format!(
                "{{\"eod_and_greeks\":{},\"eod_without_greeks\":{},\"oi_in_eod\":{},\"eod_without_oi\":{}}}",
                r.eod_and_greeks, r.eod_without_greeks, r.oi_in_eod, r.eod_without_oi
            ),
        };
        format!(
            "{{\"schema_version\":{},\"endpoint\":{},\"root\":{},\"grain\":{},\
             \"start_date\":{},\"end_date\":{},\"request\":{},\"file_path\":{},\
             \"file_blake3\":{},\"size_bytes\":{},\"row_count\":{},\
             \"distinct_contracts\":{},\"dup_rate\":{},\"n_builds_distribution\":{{{}}},\
             \"conflicting_pairs\":{},\"sentinel_rows_dropped\":{},\"fetch_millis\":{},\
             \"zero_ohlc_rate\":{},\"reconciliation\":{},\"fetched_ts\":{}}}",
            self.schema_version,
            json_string(&self.endpoint),
            json_string(&self.root),
            json_string(&self.grain),
            json_string(&self.start_date),
            json_string(&self.end_date),
            json_string(&self.request),
            json_string(&self.file_path),
            json_string(&self.file_blake3),
            self.size_bytes,
            self.row_count,
            self.distinct_contracts,
            format_ratio(self.dup_rate),
            builds,
            self.conflicting_pairs,
            self.sentinel_rows_dropped,
            self.fetch_millis,
            self.zero_ohlc_rate
                .map_or_else(|| "null".to_owned(), format_ratio),
            reconciliation,
            self.fetched_ts,
        )
    }
}

/// Escapes a string into a JSON literal.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Renders a ratio with fixed precision.
///
/// Three decimals is the precision the era fingerprint is stated in (§3.1's
/// 2.000 and 1.000), and fixing it means a line's bytes do not change with the
/// platform's shortest-round-trip formatting. A non-finite value writes `null`
/// rather than the JSON-invalid `NaN`.
fn format_ratio(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.3}")
    } else {
        "null".to_owned()
    }
}

/// Append-only reader/writer over `external/thetadata/inventory.jsonl`.
#[derive(Debug)]
pub struct Inventory {
    path: PathBuf,
}

impl Inventory {
    /// Opens (without creating) the inventory under `data_dir`.
    #[must_use]
    pub fn open(data_dir: &Path) -> Inventory {
        Inventory {
            path: data_dir
                .join("external")
                .join("thetadata")
                .join(INVENTORY_FILE),
        }
    }

    /// Where this inventory lives.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one record, creating the file and its parents if needed.
    ///
    /// Flushed before returning: a record that is only in the OS cache is a
    /// record that a power cut turns into a file the next run will not know it
    /// already has.
    ///
    /// A sharing violation on the open is retried — see [`APPEND_ATTEMPTS`].
    ///
    /// # Errors
    /// [`ThetaError::Io`] if the directory, the file, or the write fails.
    pub fn append(&self, record: &InventoryRecord) -> Result<(), ThetaError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ThetaError::Io {
                path: parent.to_path_buf(),
                during: "creating the inventory directory",
                source,
            })?;
        }
        let mut file = self.open_for_append()?;
        let mut line = record.to_json_line();
        line.push('\n');
        file.write_all(line.as_bytes())
            .map_err(|source| ThetaError::Io {
                path: self.path.clone(),
                during: "appending to the inventory",
                source,
            })?;
        file.flush().map_err(|source| ThetaError::Io {
            path: self.path.clone(),
            during: "flushing the inventory",
            source,
        })
    }

    /// Opens the inventory for append, retrying a sharing violation.
    ///
    /// Only a sharing violation is retried. Everything else — a missing
    /// directory, a permission denial, a read-only volume — fails on the first
    /// attempt, because those do not clear on their own and a retry loop would
    /// only delay the report by five seconds.
    ///
    /// # Errors
    /// [`ThetaError::Io`] once the attempts are spent, carrying the last real
    /// OS error so the cause stays visible.
    fn open_for_append(&self) -> Result<std::fs::File, ThetaError> {
        let mut attempts_left = APPEND_ATTEMPTS;
        loop {
            let source = match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                Ok(file) => return Ok(file),
                Err(source) => source,
            };
            attempts_left -= 1;
            let shared = is_sharing_violation(&source);
            if attempts_left == 0 || !shared {
                return Err(ThetaError::Io {
                    path: self.path.clone(),
                    during: if shared {
                        "opening the inventory for append: another process held a deny-share \
                         handle for the whole retry window. Observe a live acquisition with a \
                         FileShare.ReadWrite open, a copy, or a file count — never a plain \
                         Get-Content (docs/RUNBOOK_BLITZ.md)"
                    } else {
                        "opening the inventory for append"
                    },
                    source,
                });
            }
            std::thread::sleep(APPEND_RETRY_PAUSE);
        }
    }

    /// The `request` string of every complete record, for a resume diff.
    ///
    /// A truncated final line — the signature of a run that died mid-write — is
    /// **skipped, not repaired**: the file it describes may or may not be
    /// complete, so the honest move is to leave the request looking un-done and
    /// let the next run redo it. Redoing one request costs a request; trusting
    /// a half-written line costs a gap nobody sees.
    ///
    /// # Errors
    /// [`ThetaError::Io`] if the file exists but cannot be read.
    pub fn completed_requests(&self) -> Result<Vec<String>, ThetaError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path).map_err(|source| ThetaError::Io {
            path: self.path.clone(),
            during: "opening the inventory",
            source,
        })?;
        let mut out = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|source| ThetaError::Io {
                path: self.path.clone(),
                during: "reading the inventory",
                source,
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // A complete line is a complete JSON object. Anything else is the
            // tail of an interrupted write.
            if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                continue;
            }
            if let Some(request) = extract_string_field(trimmed, "request") {
                out.push(request);
            }
        }
        Ok(out)
    }

    /// Every complete record, in file order.
    ///
    /// Torn final lines are skipped for the same reason
    /// [`Inventory::completed_requests`] skips them: a half-written line
    /// describes a file whose state is unknown, and the honest move is to leave
    /// it looking un-done.
    ///
    /// # Errors
    /// [`ThetaError::Io`] if the file exists but cannot be read.
    pub fn read_all(&self) -> Result<Vec<InventoryRecord>, ThetaError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path).map_err(|source| ThetaError::Io {
            path: self.path.clone(),
            during: "opening the inventory",
            source,
        })?;
        let mut out = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|source| ThetaError::Io {
                path: self.path.clone(),
                during: "reading the inventory",
                source,
            })?;
            let trimmed = line.trim();
            if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                continue;
            }
            if let Some(record) = parse_line(trimmed) {
                out.push(record);
            }
        }
        Ok(out)
    }

    /// Which of `requests` the inventory does **not** already record.
    ///
    /// This is resume. Order is preserved so a resumed tranche runs in the same
    /// sequence as a fresh one.
    ///
    /// # Errors
    /// [`ThetaError::Io`] if the inventory cannot be read.
    pub fn outstanding(&self, requests: &[String]) -> Result<Vec<String>, ThetaError> {
        let done: std::collections::BTreeSet<String> =
            self.completed_requests()?.into_iter().collect();
        Ok(requests
            .iter()
            .filter(|r| !done.contains(*r))
            .cloned()
            .collect())
    }
}

/// Pulls one bare (unquoted) field out of a JSON line: numbers and `null`.
///
/// Returns `None` for a missing field and for `null`, which are the same thing
/// to every caller here — the field was not measured.
fn extract_bare_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("\"{field}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    let value = rest[..end].trim();
    (!value.is_empty() && value != "null").then_some(value)
}

/// Reads one line back into a record, as far as the gate report needs it.
///
/// Deliberately partial and deliberately not `serde`: the fields reconstructed
/// here are the ones the validation gate reads, and a line this cannot parse is
/// a line this crate did not write. `n_builds_distribution` and the
/// reconciliation block are not reconstructed — the gate recomputes those from
/// the per-file counts rather than trusting a round-tripped copy.
fn parse_line(line: &str) -> Option<InventoryRecord> {
    let string = |f: &str| extract_string_field(line, f);
    let number = |f: &str| extract_bare_field(line, f).and_then(|v| v.parse::<u64>().ok());
    let float = |f: &str| extract_bare_field(line, f).and_then(|v| v.parse::<f64>().ok());
    Some(InventoryRecord {
        schema_version: u32::try_from(number("schema_version")?).ok()?,
        endpoint: string("endpoint")?,
        root: string("root")?,
        grain: string("grain").unwrap_or_default(),
        start_date: string("start_date")?,
        end_date: string("end_date").unwrap_or_default(),
        request: string("request")?,
        file_path: string("file_path").unwrap_or_default(),
        file_blake3: string("file_blake3").unwrap_or_default(),
        size_bytes: number("size_bytes").unwrap_or(0),
        row_count: number("row_count").unwrap_or(0),
        distinct_contracts: number("distinct_contracts").unwrap_or(0),
        dup_rate: float("dup_rate").unwrap_or(0.0),
        n_builds_distribution: BTreeMap::new(),
        conflicting_pairs: number("conflicting_pairs").unwrap_or(0),
        sentinel_rows_dropped: number("sentinel_rows_dropped").unwrap_or(0),
        fetch_millis: number("fetch_millis").unwrap_or(0),
        zero_ohlc_rate: float("zero_ohlc_rate"),
        reconciliation: None,
        fetched_ts: extract_bare_field(line, "fetched_ts")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0),
    })
}

/// Pulls one string field out of a JSON line without a full parser.
///
/// The writer above is the only producer of these lines and it escapes exactly
/// five characters, so this reader handles exactly those. It is deliberately
/// not a general JSON parser: an inventory line this cannot read is a line this
/// crate did not write, and treating it as unknown is the correct outcome.
fn extract_string_field(line: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\":\"");
    let start = line.find(&needle)? + needle.len();
    let mut out = String::new();
    let mut chars = line[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
#[path = "inventory/tests.rs"]
mod tests;
