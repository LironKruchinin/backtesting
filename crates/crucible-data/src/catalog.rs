//! Archive catalog — the project's cost-control and reproducibility spine.
//!
//! Answers two questions cheaply and reliably: *what data do we already own?*
//! (so nothing is ever paid for twice — [`Catalog::coverage`]) and *exactly
//! which bytes fed run X?* (so results are reproducible — the manifest id,
//! CLAUDE.md §2.5 / D-0013).
//!
//! ## Layout (under `$CRUCIBLE_DATA_DIR`, which is NEVER inside the repo)
//!
//! ```text
//! raw/{dataset}/{schema}/{symbol}/{yyyy-mm}.dbn.zst   # immutable, append-only
//! curated/bars/{symbol}/{tf}/{yyyy}.parquet           # regenerable from raw
//! manifest.jsonl                                      # one record per acquired slice
//! ```
//!
//! The catalog takes an already-resolved data-dir [`PathBuf`]; reading
//! `$CRUCIBLE_DATA_DIR` is the job of bin targets, never library code.
//!
//! ## Manifest records
//!
//! One JSON object per line of `manifest.jsonl` — see [`ManifestRecord`].
//! Field-name deltas from the original M1 spec sketch, decided when this
//! module was implemented:
//! - `file_sha256` → [`file_blake3`](ManifestRecord::file_blake3) (the spec
//!   allowed blake3; the name now tells the truth — D-0014),
//! - `acquired_at` → [`acquired_ts`](ManifestRecord::acquired_ts) (§4 naming;
//!   caller-supplied, never read from a clock in library code — D-0015),
//! - added a required [`schema_version`](ManifestRecord::schema_version)
//!   field (D-0016).
//!
//! **The manifest id is the file checksum.** `file_blake3` uniquely names the
//! bytes a run read; there is no separate id field (D-0014). Two records may
//! share an id only if their files are byte-identical, which is semantically
//! exact. Results persist these ids (D-0013).
//!
//! Records deserialize with `deny_unknown_fields` and a pinned
//! [`MANIFEST_SCHEMA_VERSION`]; a malformed line, unknown field, or unknown
//! version is a **hard error naming the 1-based line** (D-0016). A manifest
//! typo corrupts reproducibility exactly like a config typo corrupts
//! research (CLAUDE.md §5.5) — nothing here is silently tolerated.
//!
//! ## Rules
//! - **Raw is immutable.** The API is open / append / read / coverage /
//!   verify — there is no delete, no overwrite, no mutation of existing
//!   records. Appending a `file_path` that is already recorded is an error:
//!   corrections are new acquisitions at new paths.
//! - **Coverage check before download.** [`Catalog::coverage`] computes
//!   requested-range minus owned-range per symbol so ingest downloads only
//!   the gap.
//! - **Curated is disposable** and NOT tracked by this manifest. Curated
//!   files are rebuilt from raw; recording the transcoder version that
//!   produced them belongs to the transcode task (M1, later), not here.
//! - **The catalog is the checksum gatekeeper** (D-0017): [`Catalog::append`]
//!   hashes the file on disk itself — callers cannot supply a checksum or
//!   size, so the manifest can never disagree with the bytes we actually
//!   hold. [`Catalog::verify`] re-hashes everything and reports every
//!   finding (an audit must show the extent of damage, not stop at the
//!   first hit).
//!
//! ## Intervals
//!
//! All ranges are half-open `[start_ts, end_ts)` in UTC nanoseconds
//! (D-0015). Adjacent monthly slices `[a, b) + [b, c)` therefore concatenate
//! without gap or overlap ambiguity, and zero-width ranges are invalid.
//!
//! ## Portability
//!
//! `file_path` is always a relative, forward-slash path under `raw/` —
//! validated on append **and** on load (a hand-edited manifest gets no more
//! trust than a caller). The manifest is byte-identical regardless of host
//! OS; `data_dir` is the only absolute root, held in memory only.
//!
//! ## Crash safety
//!
//! Append serializes the record to a single line, writes `line + '\n'` with
//! one `write_all`, then fsyncs. A crash mid-write can leave a torn partial
//! last line; even a complete JSON object without its terminating newline is
//! *detected loudly* at the next [`Catalog::open`] and never repaired
//! silently. Remedy is manual: inspect the tail, confirm against the files on
//! disk, delete the partial line, re-append. The manifest itself is
//! append-only — this code never truncates or rewrites it.
//!
//! Writers take an exclusive OS file lock and reload the manifest while
//! holding it before checking `file_path`. This makes the duplicate-path
//! immutability rule hold even for two catalog handles opened before either
//! one appends (D-0018).
//!
//! The wire framing is LF-only: any carriage return in the manifest is a
//! hard error (a CRLF hand edit would parse identically but silently break
//! the byte-identical portability guarantee — D-0019). Known limitation:
//! the very first append creates `manifest.jsonl` without fsyncing the
//! parent directory, so on some POSIX filesystems a crash in that window can
//! lose the whole (one-line) file rather than tearing it — a loud, visible
//! absence, not silent corruption.
//!
//! ## Determinism hygiene (CLAUDE.md §2.2)
//!
//! No clocks (`acquired_ts` is caller-supplied), no `HashMap` iteration —
//! coverage output is a [`BTreeMap`] with sorted, disjoint gap lists, and
//! verify findings follow manifest record order.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crucible_core::prelude::Ts;
use serde::{Deserialize, Serialize};

/// Manifest record schema version this build reads and writes.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// File name of the manifest, directly under the data dir.
pub const MANIFEST_FILE_NAME: &str = "manifest.jsonl";

/// Serde bridge for [`Ts`] (core is dependency-free; serde may not appear
/// there). Wire format: plain JSON integer, UTC nanoseconds.
mod ts_serde {
    use crucible_core::prelude::Ts;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(ts: &Ts, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(ts.0)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Ts, D::Error> {
        i64::deserialize(d).map(Ts)
    }
}

/// Errors from catalog operations. Hand-rolled per CLAUDE.md §5.1.
///
/// Not `Clone`/`PartialEq` (wraps [`io::Error`] / [`serde_json::Error`]);
/// tests match with `matches!`.
#[derive(Debug)]
pub enum CatalogError {
    /// The data dir does not exist or is not a directory. Loud by design: a
    /// typo'd path must not masquerade as an empty archive, or the coverage
    /// check would approve re-downloading everything we already own.
    DataDirMissing {
        /// The path that was expected to be the data directory.
        path: PathBuf,
    },
    /// Filesystem failure reading/writing the manifest or hashing a file.
    Io {
        /// The path being read or written when the failure occurred.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// A manifest line is not a valid record: bad JSON, unknown field,
    /// missing field, blank line, or a torn partial line from a crashed
    /// append. Corruption is loud (D-0016).
    MalformedManifestLine {
        /// 1-based line number in `manifest.jsonl`.
        line: usize,
        /// The underlying parse error.
        source: serde_json::Error,
    },
    /// The final JSONL record has no terminating newline. Even if its JSON is
    /// complete, accepting it would make the next append concatenate two
    /// objects onto one malformed line.
    UnterminatedManifestLine {
        /// 1-based line number of the unterminated tail.
        line: usize,
    },
    /// The manifest contains a carriage return. The wire format is LF-only
    /// (one JSON object per `\n`-terminated line); CRLF means a
    /// non-canonical hand edit and would silently break the byte-identical
    /// portability guarantee.
    CarriageReturnInManifest {
        /// 1-based line number of the first line containing a `\r`.
        line: usize,
    },
    /// A record declares a schema version this build does not understand.
    UnsupportedSchemaVersion {
        /// 1-based line number in `manifest.jsonl`.
        line: usize,
        /// The version the record declared.
        found: u32,
    },
    /// The `file_path` is already recorded. Raw is immutable: corrections
    /// are new acquisitions at new paths, never rewrites.
    DuplicateFilePath {
        /// The offending relative path.
        file_path: String,
    },
    /// `start_ts >= end_ts` — invalid under half-open `[start_ts, end_ts)`
    /// semantics (zero-width included).
    InvalidRange {
        /// Claimed range start (inclusive).
        start_ts: Ts,
        /// Claimed range end (exclusive).
        end_ts: Ts,
    },
    /// `file_path` violates the portability/immutability rules: it must be
    /// a relative, forward-slash path strictly under `raw/`, with no `.`,
    /// `..`, or empty components, no backslashes, and no colons.
    InvalidFilePath {
        /// The offending path.
        file_path: String,
        /// Which rule was violated.
        reason: &'static str,
    },
    /// `file_blake3` is not the canonical lowercase 64-character hex form.
    InvalidFileBlake3 {
        /// The offending checksum text.
        file_blake3: String,
        /// Which canonical-format rule was violated.
        reason: &'static str,
    },
    /// The symbols list is empty or contains an unusable symbol.
    InvalidSymbols {
        /// What was wrong, including the offending symbol if applicable.
        detail: String,
    },
    /// `dataset` or `schema` is empty or contains path separators,
    /// whitespace, or control characters.
    InvalidName {
        /// Which field was invalid (`"dataset"` or `"schema"`).
        field: &'static str,
        /// The offending value.
        value: String,
        /// Which rule was violated.
        reason: &'static str,
    },
    /// An [`Acquisition`] points at a file that does not exist under the
    /// data dir. Nothing gets recorded that we do not actually hold.
    ArchiveFileMissing {
        /// The resolved absolute path that was expected to exist.
        path: PathBuf,
    },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::DataDirMissing { path } => write!(
                f,
                "data dir {} does not exist (refusing to treat a typo'd path as an empty archive)",
                path.display()
            ),
            CatalogError::Io { path, source } => {
                write!(f, "I/O error on {}: {source}", path.display())
            }
            CatalogError::MalformedManifestLine { line, source } => write!(
                f,
                "manifest.jsonl line {line} is not a valid record: {source} \
                 (corruption or a torn append; inspect the tail by hand — never auto-repaired)"
            ),
            CatalogError::UnterminatedManifestLine { line } => write!(
                f,
                "manifest.jsonl line {line} has no terminating newline; refusing to append to a possibly torn record"
            ),
            CatalogError::CarriageReturnInManifest { line } => write!(
                f,
                "manifest.jsonl line {line} contains a carriage return; the manifest is LF-only — \
                 normalize the line endings by hand (never auto-repaired)"
            ),
            CatalogError::UnsupportedSchemaVersion { line, found } => write!(
                f,
                "manifest.jsonl line {line} declares schema_version {found}, \
                 this build understands {MANIFEST_SCHEMA_VERSION}"
            ),
            CatalogError::DuplicateFilePath { file_path } => write!(
                f,
                "file_path {file_path:?} is already recorded; raw is immutable — \
                 corrections are new acquisitions at new paths"
            ),
            CatalogError::InvalidRange { start_ts, end_ts } => write!(
                f,
                "invalid range [{start_ts}, {end_ts}): start must be strictly before end \
                 (half-open, zero-width invalid)"
            ),
            CatalogError::InvalidFilePath { file_path, reason } => {
                write!(f, "invalid file_path {file_path:?}: {reason}")
            }
            CatalogError::InvalidFileBlake3 {
                file_blake3,
                reason,
            } => write!(f, "invalid file_blake3 {file_blake3:?}: {reason}"),
            CatalogError::InvalidSymbols { detail } => write!(f, "invalid symbols: {detail}"),
            CatalogError::InvalidName {
                field,
                value,
                reason,
            } => write!(f, "invalid {field} {value:?}: {reason}"),
            CatalogError::ArchiveFileMissing { path } => write!(
                f,
                "archive file {} does not exist; refusing to record bytes we do not hold",
                path.display()
            ),
        }
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CatalogError::Io { source, .. } => Some(source),
            CatalogError::MalformedManifestLine { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Half-open UTC-nanosecond interval `[start_ts, end_ts)`.
///
/// Invariant: `start_ts < end_ts`, enforced by [`TsRange::new`] — fields are
/// private so an invalid range is unconstructible outside this module.
/// The derived `Ord` is lexicographic `(start_ts, end_ts)`, which is exactly
/// the sort the coverage merge relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TsRange {
    start_ts: Ts,
    end_ts: Ts,
}

impl TsRange {
    /// Builds a validated half-open range.
    ///
    /// # Errors
    /// [`CatalogError::InvalidRange`] if `start_ts >= end_ts`.
    pub fn new(start_ts: Ts, end_ts: Ts) -> Result<TsRange, CatalogError> {
        if start_ts >= end_ts {
            return Err(CatalogError::InvalidRange { start_ts, end_ts });
        }
        Ok(TsRange { start_ts, end_ts })
    }

    /// Range start (inclusive).
    #[must_use]
    pub fn start_ts(&self) -> Ts {
        self.start_ts
    }

    /// Range end (exclusive).
    #[must_use]
    pub fn end_ts(&self) -> Ts {
        self.end_ts
    }

    /// Internal constructor for ranges whose validity is structural (e.g.
    /// gaps emitted by subtraction, record ranges validated at load).
    fn new_unchecked(start_ts: Ts, end_ts: Ts) -> TsRange {
        debug_assert!(start_ts < end_ts, "internal TsRange must be nonempty");
        TsRange { start_ts, end_ts }
    }
}

/// One line of `manifest.jsonl`: a single acquired raw slice.
///
/// `file_blake3` doubles as the slice's **manifest id** (D-0014): results
/// referencing this id name exactly the bytes they read. Serialized field
/// order is declaration order (stable wire format, pinned by a golden test).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestRecord {
    /// Always [`MANIFEST_SCHEMA_VERSION`]; other values are a hard load error.
    pub schema_version: u32,
    /// Databento dataset, e.g. `GLBX.MDP3`.
    pub dataset: String,
    /// Databento schema, e.g. `trades`, `ohlcv-1m`.
    pub schema: String,
    /// Raw symbols this slice covers (at least one). One file may cover
    /// many symbols; coverage credits each of them.
    pub symbols: Vec<String>,
    /// Event-time range start (inclusive), UTC nanoseconds.
    #[serde(with = "ts_serde")]
    pub start_ts: Ts,
    /// Event-time range end (exclusive), UTC nanoseconds.
    #[serde(with = "ts_serde")]
    pub end_ts: Ts,
    /// When the slice was acquired. Supplied by the caller (the ingest bin
    /// owns the wall clock); library code never reads a clock (§2.2).
    #[serde(with = "ts_serde")]
    pub acquired_ts: Ts,
    /// Databento batch job id, for provenance and re-download audit.
    pub databento_job_id: String,
    /// Relative forward-slash path under `raw/`, joined onto the data dir.
    pub file_path: String,
    /// Lowercase-hex blake3 of the file bytes. **The manifest id** (D-0014).
    pub file_blake3: String,
    /// File size in bytes; the cheap first-line tamper check for verify.
    pub size_bytes: u64,
}

/// What the caller knows about a new slice. The checksum and size are
/// deliberately absent: [`Catalog::append`] computes both from the bytes on
/// disk (gatekeeper, D-0017) — a caller-supplied hash could lie and produce
/// a permanently untruthful manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acquisition {
    /// Databento dataset, e.g. `GLBX.MDP3`.
    pub dataset: String,
    /// Databento schema, e.g. `trades`, `ohlcv-1m`.
    pub schema: String,
    /// Raw symbols the slice covers (at least one).
    pub symbols: Vec<String>,
    /// Half-open event-time range of the slice.
    pub range: TsRange,
    /// Acquisition wall-clock time, supplied by the caller (§2.2: no clocks
    /// in library code).
    pub acquired_ts: Ts,
    /// Databento batch job id.
    pub databento_job_id: String,
    /// Relative forward-slash path under `raw/` where the file already sits.
    pub file_path: String,
}

/// A "do we already own this?" query. Matching is exact and case-sensitive
/// on `dataset` and `schema`; a record credits a symbol iff its `symbols`
/// list contains it, wherever its file lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRequest {
    /// Databento dataset to match exactly.
    pub dataset: String,
    /// Databento schema to match exactly.
    pub schema: String,
    /// Symbols to report on (duplicates are collapsed).
    pub symbols: Vec<String>,
    /// The half-open range being requested.
    pub range: TsRange,
}

/// Outcome of an archive integrity audit ([`Catalog::verify`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    /// Findings in manifest record order (deterministic). Empty = clean.
    pub findings: Vec<VerifyFinding>,
}

impl VerifyReport {
    /// True when the archive matches the manifest exactly.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// One integrity problem found by [`Catalog::verify`]. Per file, only the
/// FIRST applicable finding is reported: missing beats size beats checksum
/// (a size mismatch already implies a hash mismatch; skipping the hash
/// keeps verify cheap on damaged archives).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyFinding {
    /// The recorded file is absent (or not a regular file).
    MissingFile {
        /// Relative path from the record.
        file_path: String,
    },
    /// The file exists but its size differs from the record.
    SizeMismatch {
        /// Relative path from the record.
        file_path: String,
        /// Size recorded in the manifest.
        expected: u64,
        /// Size found on disk.
        actual: u64,
    },
    /// The file exists with the recorded size but its bytes hash differently.
    ChecksumMismatch {
        /// Relative path from the record.
        file_path: String,
        /// Lowercase-hex blake3 recorded in the manifest.
        expected: String,
        /// Lowercase-hex blake3 of the bytes on disk.
        actual: String,
    },
}

/// In-memory view of `manifest.jsonl`, rooted at a data directory.
///
/// The API shape *is* the immutability rule: open / append / read /
/// coverage / verify. There is no delete, no overwrite, no mutation of
/// existing records.
#[derive(Debug)]
pub struct Catalog {
    data_dir: PathBuf,
    manifest_path: PathBuf,
    /// File order == append order.
    records: Vec<ManifestRecord>,
    /// For duplicate-`file_path` rejection.
    known_paths: BTreeSet<String>,
}

impl Catalog {
    /// Opens the catalog at `data_dir`, loading and validating the whole
    /// manifest (it is thousands of lines at most — load-all is deliberate
    /// simplicity).
    ///
    /// A missing `manifest.jsonl` inside an *existing* dir is a legitimate
    /// fresh archive: empty catalog, file created lazily on first append.
    /// A missing `data_dir` is a hard error — see
    /// [`CatalogError::DataDirMissing`].
    ///
    /// # Errors
    /// [`CatalogError::DataDirMissing`], [`CatalogError::Io`], or any
    /// per-line validation error ([`CatalogError::MalformedManifestLine`],
    /// [`CatalogError::UnterminatedManifestLine`],
    /// [`CatalogError::CarriageReturnInManifest`],
    /// [`CatalogError::UnsupportedSchemaVersion`], and the same semantic
    /// checks as [`Catalog::append`] — a hand-edited manifest gets no more
    /// trust than a caller).
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Catalog, CatalogError> {
        let data_dir = data_dir.into();
        if !data_dir.is_dir() {
            return Err(CatalogError::DataDirMissing { path: data_dir });
        }
        let manifest_path = data_dir.join(MANIFEST_FILE_NAME);
        let mut catalog = Catalog {
            data_dir,
            manifest_path,
            records: Vec::new(),
            known_paths: BTreeSet::new(),
        };

        let content = match fs::read_to_string(&catalog.manifest_path) {
            Ok(content) => content,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(catalog),
            Err(source) => {
                return Err(CatalogError::Io {
                    path: catalog.manifest_path,
                    source,
                });
            }
        };

        let (records, known_paths) = parse_manifest(&content)?;
        catalog.records = records;
        catalog.known_paths = known_paths;
        Ok(catalog)
    }

    /// The data directory this catalog is rooted at.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// All records in manifest (= append) order.
    #[must_use]
    pub fn records(&self) -> &[ManifestRecord] {
        &self.records
    }

    /// Looks up a record by its manifest id (= `file_blake3`, D-0013/D-0014).
    /// If identical bytes were acquired twice (legal), returns the first
    /// record in file order.
    #[must_use]
    pub fn record_by_id(&self, file_blake3: &str) -> Option<&ManifestRecord> {
        self.records.iter().find(|r| r.file_blake3 == file_blake3)
    }

    /// Validates the acquisition, hashes the file on disk, appends one
    /// record line to `manifest.jsonl` (single write + fsync), and returns
    /// the completed record.
    ///
    /// In-memory state is updated only after the line is durably on disk; a
    /// failed append leaves the catalog consistent with disk-before-the-
    /// attempt.
    ///
    /// # Errors
    /// Validation errors ([`CatalogError::InvalidName`],
    /// [`CatalogError::InvalidSymbols`], [`CatalogError::InvalidFilePath`],
    /// [`CatalogError::DuplicateFilePath`]),
    /// [`CatalogError::ArchiveFileMissing`] if the file is not on disk,
    /// [`CatalogError::Io`], or a manifest validation error if another writer
    /// changed the manifest after this handle was opened.
    pub fn append(&mut self, acq: Acquisition) -> Result<ManifestRecord, CatalogError> {
        validate_name("dataset", &acq.dataset)?;
        validate_name("schema", &acq.schema)?;
        validate_symbols(&acq.symbols)?;
        validate_file_path(&acq.file_path)?;
        if self.known_paths.contains(&acq.file_path) {
            return Err(CatalogError::DuplicateFilePath {
                file_path: acq.file_path,
            });
        }
        // `Path::join` accepts embedded forward slashes on every platform.
        let abs = self.data_dir.join(&acq.file_path);
        if !abs.is_file() {
            return Err(CatalogError::ArchiveFileMissing { path: abs });
        }
        let (file_blake3, size_bytes) = hash_file(&abs)?;

        let record = ManifestRecord {
            schema_version: MANIFEST_SCHEMA_VERSION,
            dataset: acq.dataset,
            schema: acq.schema,
            symbols: acq.symbols,
            start_ts: acq.range.start_ts,
            end_ts: acq.range.end_ts,
            acquired_ts: acq.acquired_ts,
            databento_job_id: acq.databento_job_id,
            file_path: acq.file_path,
            file_blake3,
            size_bytes,
        };

        let io_err = |source| CatalogError::Io {
            path: self.manifest_path.clone(),
            source,
        };
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&self.manifest_path)
            .map_err(io_err)?;
        file.lock().map_err(io_err)?;

        // Another Catalog/process may have appended since this handle was
        // opened. Refresh while holding the writer lock so duplicate-path
        // rejection is a disk-backed invariant, not a stale-memory guess.
        file.seek(SeekFrom::Start(0)).map_err(io_err)?;
        let mut content = String::new();
        file.read_to_string(&mut content).map_err(io_err)?;
        let (records, known_paths) = parse_manifest(&content)?;
        self.records = records;
        self.known_paths = known_paths;
        if self.known_paths.contains(&record.file_path) {
            return Err(CatalogError::DuplicateFilePath {
                file_path: record.file_path,
            });
        }

        let line = serde_json::to_string(&record)
            .expect("INVARIANT: ManifestRecord is plain strings/ints; serialization cannot fail");
        let mut buf = line.into_bytes();
        buf.push(b'\n');
        file.write_all(&buf).map_err(io_err)?;
        file.sync_all().map_err(io_err)?;

        self.known_paths.insert(record.file_path.clone());
        self.records.push(record.clone());
        Ok(record)
    }

    /// Requested range minus owned ranges, per symbol. Pure and in-memory.
    ///
    /// Every requested symbol appears as a key (deduplicated, sorted by the
    /// `BTreeMap`); the value is the sorted, disjoint list of sub-ranges of
    /// `req.range` NOT owned. An empty list means fully covered — visible,
    /// not silently absent.
    #[must_use]
    pub fn coverage(&self, req: &CoverageRequest) -> BTreeMap<String, Vec<TsRange>> {
        let mut out = BTreeMap::new();
        for symbol in &req.symbols {
            if out.contains_key(symbol) {
                continue;
            }
            let owned: Vec<TsRange> = self
                .records
                .iter()
                .filter(|r| {
                    r.dataset == req.dataset
                        && r.schema == req.schema
                        && r.symbols.iter().any(|s| s == symbol)
                })
                // Validated at load/append time: start_ts < end_ts holds.
                .map(|r| TsRange::new_unchecked(r.start_ts, r.end_ts))
                .collect();
            let merged = merge_ranges(owned);
            out.insert(symbol.clone(), subtract_from(req.range, &merged));
        }
        out
    }

    /// Re-checks every record's file on disk: existence, size, blake3.
    ///
    /// Integrity problems go in the report (in record order — an audit shows
    /// the extent of the damage rather than stopping at the first hit);
    /// `Err` is reserved for environmental I/O failures.
    ///
    /// # Errors
    /// [`CatalogError::Io`] if a file exists but cannot be read.
    pub fn verify(&self) -> Result<VerifyReport, CatalogError> {
        let mut findings = Vec::new();
        for record in &self.records {
            let abs = self.data_dir.join(&record.file_path);
            let metadata = match fs::metadata(&abs) {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == io::ErrorKind::NotFound => {
                    findings.push(VerifyFinding::MissingFile {
                        file_path: record.file_path.clone(),
                    });
                    continue;
                }
                Err(source) => return Err(CatalogError::Io { path: abs, source }),
            };
            if !metadata.is_file() {
                findings.push(VerifyFinding::MissingFile {
                    file_path: record.file_path.clone(),
                });
                continue;
            }
            if metadata.len() != record.size_bytes {
                findings.push(VerifyFinding::SizeMismatch {
                    file_path: record.file_path.clone(),
                    expected: record.size_bytes,
                    actual: metadata.len(),
                });
                continue;
            }
            let (actual, _) = hash_file(&abs)?;
            if actual != record.file_blake3 {
                findings.push(VerifyFinding::ChecksumMismatch {
                    file_path: record.file_path.clone(),
                    expected: record.file_blake3.clone(),
                    actual,
                });
            }
        }
        Ok(VerifyReport { findings })
    }
}

/// Parses and validates a complete manifest snapshot. The returned path set
/// is built in the same pass so duplicate paths are rejected before any
/// caller replaces its in-memory state.
fn parse_manifest(content: &str) -> Result<(Vec<ManifestRecord>, BTreeSet<String>), CatalogError> {
    // LF-only framing: a CRLF (or stray CR) manifest would parse identically
    // via `str::lines` but diverge byte-wise from what append writes,
    // silently breaking the byte-identical portability guarantee (D-0019).
    if let Some(idx) = content.split('\n').position(|line| line.contains('\r')) {
        return Err(CatalogError::CarriageReturnInManifest { line: idx + 1 });
    }
    if !content.is_empty() && !content.ends_with('\n') {
        return Err(CatalogError::UnterminatedManifestLine {
            line: content.bytes().filter(|byte| *byte == b'\n').count() + 1,
        });
    }

    let mut records = Vec::new();
    let mut known_paths = BTreeSet::new();
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        // Blank lines, torn tails, unknown fields, and plain bad JSON all
        // surface here; `deny_unknown_fields` is on the struct.
        let record: ManifestRecord =
            serde_json::from_str(line).map_err(|source| CatalogError::MalformedManifestLine {
                line: line_no,
                source,
            })?;
        if record.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchemaVersion {
                line: line_no,
                found: record.schema_version,
            });
        }
        validate_record_semantics(&record)?;
        if !known_paths.insert(record.file_path.clone()) {
            return Err(CatalogError::DuplicateFilePath {
                file_path: record.file_path,
            });
        }
        records.push(record);
    }
    Ok((records, known_paths))
}

/// Semantic validation shared by load and append: the same rules apply to a
/// hand-edited manifest line as to a caller-supplied [`Acquisition`].
fn validate_record_semantics(record: &ManifestRecord) -> Result<(), CatalogError> {
    validate_name("dataset", &record.dataset)?;
    validate_name("schema", &record.schema)?;
    validate_symbols(&record.symbols)?;
    validate_file_path(&record.file_path)?;
    validate_file_blake3(&record.file_blake3)?;
    if record.start_ts >= record.end_ts {
        return Err(CatalogError::InvalidRange {
            start_ts: record.start_ts,
            end_ts: record.end_ts,
        });
    }
    Ok(())
}

/// Canonical blake3 text is exactly 32 bytes encoded as 64 lowercase hex
/// characters. Enforcing it on load keeps manifest ids byte-portable and
/// prevents malformed hand-edited ids from entering persisted results.
fn validate_file_blake3(file_blake3: &str) -> Result<(), CatalogError> {
    let err = |reason| CatalogError::InvalidFileBlake3 {
        file_blake3: file_blake3.to_owned(),
        reason,
    };
    if file_blake3.len() != 64 {
        return Err(err("must contain exactly 64 hex characters"));
    }
    if !file_blake3
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(err("must contain lowercase hexadecimal characters only"));
    }
    Ok(())
}

/// `dataset`/`schema` hygiene: non-empty, no separators/whitespace/control.
fn validate_name(field: &'static str, value: &str) -> Result<(), CatalogError> {
    let err = |reason| CatalogError::InvalidName {
        field,
        value: value.to_owned(),
        reason,
    };
    if value.is_empty() {
        return Err(err("must not be empty"));
    }
    if value.contains(['/', '\\']) {
        return Err(err("must not contain path separators"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(err("must not contain whitespace or control characters"));
    }
    Ok(())
}

/// Symbols hygiene: at least one; each non-empty and path-safe (symbols
/// appear inside ingest's path template). Deliberately loose otherwise —
/// Databento symbology varies and the catalog must not over-guess.
fn validate_symbols(symbols: &[String]) -> Result<(), CatalogError> {
    if symbols.is_empty() {
        return Err(CatalogError::InvalidSymbols {
            detail: "symbols list is empty; a slice must cover at least one symbol".to_owned(),
        });
    }
    for symbol in symbols {
        if symbol.is_empty() {
            return Err(CatalogError::InvalidSymbols {
                detail: "a symbol is empty".to_owned(),
            });
        }
        if symbol.contains(['/', '\\'])
            || symbol.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(CatalogError::InvalidSymbols {
                detail: format!(
                    "symbol {symbol:?} contains a path separator, whitespace, or control character"
                ),
            });
        }
    }
    Ok(())
}

/// `file_path` rules: relative, forward slashes, strictly under `raw/`, no
/// `.`/`..`/empty components, no backslashes, no colons (kills `C:` drive
/// prefixes), no whitespace or control characters (same hygiene as
/// dataset/schema/symbols). Dots *inside* a component (`GLBX.MDP3`,
/// `2026-06.dbn.zst`) are fine — only whole components equal to `.` or `..`
/// are rejected.
fn validate_file_path(file_path: &str) -> Result<(), CatalogError> {
    let err = |reason| CatalogError::InvalidFilePath {
        file_path: file_path.to_owned(),
        reason,
    };
    if file_path.is_empty() {
        return Err(err("must not be empty"));
    }
    if file_path.contains('\\') {
        return Err(err("must use forward slashes, never backslashes"));
    }
    if file_path.contains(':') {
        return Err(err("must not contain ':' (no absolute/drive paths)"));
    }
    if file_path
        .chars()
        .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(err("must not contain whitespace or control characters"));
    }
    if file_path.starts_with('/') {
        return Err(err("must be relative, not absolute"));
    }
    for component in file_path.split('/') {
        if component.is_empty() {
            return Err(err("must not contain empty components"));
        }
        if component == "." || component == ".." {
            return Err(err("must not contain '.' or '..' components"));
        }
    }
    if !file_path.starts_with("raw/") {
        return Err(err(
            "must be under raw/ (the manifest records raw acquisitions only)",
        ));
    }
    Ok(())
}

/// Sorts and merges owned ranges into a disjoint, ascending set. Half-open
/// semantics: touching ranges `[a, b) + [b, c)` merge seamlessly (`<=`).
fn merge_ranges(mut owned: Vec<TsRange>) -> Vec<TsRange> {
    owned.sort_unstable();
    let mut merged: Vec<TsRange> = Vec::with_capacity(owned.len());
    for range in owned {
        match merged.last_mut() {
            Some(last) if range.start_ts <= last.end_ts => {
                if range.end_ts > last.end_ts {
                    last.end_ts = range.end_ts;
                }
            }
            _ => merged.push(range),
        }
    }
    merged
}

/// Subtracts a merged (disjoint, ascending) owned set from `request`,
/// returning the uncovered gaps in ascending order. Every emitted gap has
/// positive width by construction.
fn subtract_from(request: TsRange, merged: &[TsRange]) -> Vec<TsRange> {
    let mut gaps = Vec::new();
    let mut cursor = request.start_ts;
    for owned in merged {
        if owned.end_ts <= cursor {
            continue; // Entirely behind the cursor: contributes nothing.
        }
        if owned.start_ts >= request.end_ts {
            break; // Entirely past the request: so is everything after it.
        }
        if owned.start_ts > cursor {
            gaps.push(TsRange::new_unchecked(cursor, owned.start_ts));
        }
        cursor = owned.end_ts;
        if cursor >= request.end_ts {
            return gaps;
        }
    }
    if cursor < request.end_ts {
        gaps.push(TsRange::new_unchecked(cursor, request.end_ts));
    }
    gaps
}

/// Streams a file through blake3, returning `(lowercase_hex, size_bytes)`.
fn hash_file(path: &Path) -> Result<(String, u64), CatalogError> {
    let io_err = |source| CatalogError::Io {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(io_err)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let size_bytes = io::copy(&mut reader, &mut hasher).map_err(io_err)?;
    Ok((hasher.finalize().to_hex().to_string(), size_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    /// blake3 of the 8 bytes `crucible`, lowercase hex.
    ///
    /// Derivation (testdata/README.md rule 2, adapted for cryptographic
    /// hashes where hand arithmetic is impossible — the chain is independent
    /// cross-derivation): the blake3 crate (v1.8.5) was first validated
    /// byte-for-byte against the official published BLAKE3 test vector for
    /// empty input (BLAKE3-team/BLAKE3 `test_vectors/test_vectors.json`,
    /// `input_len: 0` → `af1349b9f5f9a1a6a0404dea36dcc949...`); this
    /// constant is that validated implementation's output for `b"crucible"`.
    /// `append_computes_checksum_and_size_from_disk` additionally proves the
    /// bytes hashed by `append` are exactly the file's bytes.
    const CRUCIBLE_B3: &str = "928dd37a6e46738feec183bbb107329ab538b96b5155ab9a936605e90bd102be";

    const FIXTURE_REL_PATH: &str = "raw/GLBX.MDP3/trades/ESU6/2026-06.dbn.zst";

    fn ts_range(start: i64, end: i64) -> TsRange {
        TsRange::new(Ts(start), Ts(end)).expect("test range is valid")
    }

    /// Writes `bytes` at `dir/rel_path`, creating parent dirs.
    fn plant_file(dir: &Path, rel_path: &str, bytes: &[u8]) {
        let abs = dir.join(rel_path);
        fs::create_dir_all(abs.parent().expect("rel path has a parent"))
            .expect("create fixture dirs");
        fs::write(&abs, bytes).expect("write fixture file");
    }

    /// A valid acquisition for `rel_path` over `[100, 200)`.
    fn acq(rel_path: &str) -> Acquisition {
        Acquisition {
            dataset: "GLBX.MDP3".to_owned(),
            schema: "trades".to_owned(),
            symbols: vec!["ESU6".to_owned()],
            range: ts_range(100, 200),
            acquired_ts: Ts(150),
            databento_job_id: "test-job-1".to_owned(),
            file_path: rel_path.to_owned(),
        }
    }

    /// The record `acq(FIXTURE_REL_PATH)` produces for an 8-byte
    /// `b"crucible"` file — used by the serde tests without touching disk.
    fn record() -> ManifestRecord {
        ManifestRecord {
            schema_version: MANIFEST_SCHEMA_VERSION,
            dataset: "GLBX.MDP3".to_owned(),
            schema: "trades".to_owned(),
            symbols: vec!["ESU6".to_owned()],
            start_ts: Ts(100),
            end_ts: Ts(200),
            acquired_ts: Ts(150),
            databento_job_id: "test-job-1".to_owned(),
            file_path: FIXTURE_REL_PATH.to_owned(),
            file_blake3: CRUCIBLE_B3.to_owned(),
            size_bytes: 8,
        }
    }

    /// Catalog owning the given `[start, end)` slices, all for symbol ESU6
    /// in (GLBX.MDP3, trades), one tiny file per slice.
    fn catalog_with_ranges(dir: &TempDir, ranges: &[(i64, i64)]) -> Catalog {
        let mut catalog = Catalog::open(dir.path()).expect("open temp catalog");
        for (i, (start, end)) in ranges.iter().enumerate() {
            let rel = format!("raw/GLBX.MDP3/trades/ESU6/slice-{i}.dbn.zst");
            plant_file(dir.path(), &rel, b"x");
            let mut a = acq(&rel);
            a.range = ts_range(*start, *end);
            catalog.append(a).expect("append slice");
        }
        catalog
    }

    /// Gaps for ESU6 in (GLBX.MDP3, trades) over `[start, end)`.
    fn gaps(catalog: &Catalog, start: i64, end: i64) -> Vec<TsRange> {
        let req = CoverageRequest {
            dataset: "GLBX.MDP3".to_owned(),
            schema: "trades".to_owned(),
            symbols: vec!["ESU6".to_owned()],
            range: ts_range(start, end),
        };
        let mut cov = catalog.coverage(&req);
        cov.remove("ESU6").expect("requested symbol present as key")
    }

    // ---------------------------------------------------------------- serde

    #[test]
    fn record_serde_roundtrip() {
        let original = record();
        let line = serde_json::to_string(&original).expect("serialize");
        let parsed: ManifestRecord = serde_json::from_str(&line).expect("deserialize");
        assert_eq!(parsed, original);
    }

    #[test]
    fn record_wire_format_is_pinned() {
        // Golden wire format: field names and declaration order, hand-written.
        // Changing this line is a manifest schema change and needs a
        // decision-log entry + schema_version thinking, not a casual edit.
        let expected = concat!(
            r#"{"schema_version":1,"#,
            r#""dataset":"GLBX.MDP3","#,
            r#""schema":"trades","#,
            r#""symbols":["ESU6"],"#,
            r#""start_ts":100,"#,
            r#""end_ts":200,"#,
            r#""acquired_ts":150,"#,
            r#""databento_job_id":"test-job-1","#,
            r#""file_path":"raw/GLBX.MDP3/trades/ESU6/2026-06.dbn.zst","#,
            r#""file_blake3":"928dd37a6e46738feec183bbb107329ab538b96b5155ab9a936605e90bd102be","#,
            r#""size_bytes":8}"#,
        );
        assert_eq!(
            serde_json::to_string(&record()).expect("serialize"),
            expected
        );
    }

    #[test]
    fn record_rejects_unknown_field() {
        // Negative control for deny_unknown_fields: a field this build does
        // not know must be a hard error, never silently dropped.
        let line = serde_json::to_string(&record()).expect("serialize");
        let with_extra = line.replacen('{', r#"{"surprise":1,"#, 1);
        assert!(serde_json::from_str::<ManifestRecord>(&with_extra).is_err());
    }

    #[test]
    fn record_rejects_missing_field() {
        // A record without file_blake3 is not a record.
        let line = serde_json::to_string(&record()).expect("serialize");
        let without_hash = line.replace(
            r#","file_blake3":"928dd37a6e46738feec183bbb107329ab538b96b5155ab9a936605e90bd102be""#,
            "",
        );
        assert_ne!(line, without_hash, "the field to remove must be present");
        assert!(serde_json::from_str::<ManifestRecord>(&without_hash).is_err());
    }

    // ------------------------------------------------------------ open/load

    #[test]
    fn open_missing_data_dir_is_hard_error() {
        let dir = TempDir::new();
        let missing = dir.path().join("no-such-subdir");
        let result = Catalog::open(&missing);
        assert!(matches!(
            result,
            Err(CatalogError::DataDirMissing { path }) if path == missing
        ));
    }

    #[test]
    fn open_empty_dir_is_empty_catalog() {
        let dir = TempDir::new();
        let catalog = Catalog::open(dir.path()).expect("open");
        assert!(catalog.records().is_empty());
        // Opening must not write: queries never mutate the archive.
        assert!(!dir.path().join(MANIFEST_FILE_NAME).exists());
    }

    #[test]
    fn open_loads_records_in_file_order() {
        let dir = TempDir::new();
        let mut first = record();
        first.file_path = "raw/a.dbn.zst".to_owned();
        let mut second = record();
        second.file_path = "raw/b.dbn.zst".to_owned();
        let content = format!(
            "{}\n{}\n",
            serde_json::to_string(&first).expect("serialize"),
            serde_json::to_string(&second).expect("serialize"),
        );
        fs::write(dir.path().join(MANIFEST_FILE_NAME), content).expect("write manifest");

        let catalog = Catalog::open(dir.path()).expect("open");
        assert_eq!(catalog.records().len(), 2);
        assert_eq!(catalog.records()[0], first);
        assert_eq!(catalog.records()[1], second);
    }

    #[test]
    fn malformed_line_is_hard_error_with_line_number() {
        // Mandatory negative control (CLAUDE.md §7): corruption detection
        // must demonstrably fire, and it must name the line.
        let dir = TempDir::new();
        let content = format!(
            "{}\nnot json{{\n",
            serde_json::to_string(&record()).expect("serialize"),
        );
        fs::write(dir.path().join(MANIFEST_FILE_NAME), content).expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::MalformedManifestLine { line: 2, .. })
        ));
    }

    #[test]
    fn blank_line_is_hard_error() {
        // A blank line is not a record; tolerating it would normalize a
        // corrupted manifest.
        let dir = TempDir::new();
        let content = format!(
            "{}\n\n",
            serde_json::to_string(&record()).expect("serialize")
        );
        fs::write(dir.path().join(MANIFEST_FILE_NAME), content).expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::MalformedManifestLine { line: 2, .. })
        ));
    }

    #[test]
    fn partial_last_line_is_hard_error() {
        // Simulated torn append (crash mid-write): detected loudly at open,
        // never silently repaired. Pins the crash-safety story.
        let dir = TempDir::new();
        let full = serde_json::to_string(&record()).expect("serialize");
        let torn = &full[..full.len() / 2];
        let content = format!("{full}\n{torn}");
        fs::write(dir.path().join(MANIFEST_FILE_NAME), content).expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::UnterminatedManifestLine { line: 2 })
        ));
    }

    #[test]
    fn complete_last_line_without_newline_is_hard_error() {
        // A crash can happen after the JSON bytes but before the newline.
        // Accepting this apparently valid record would make the next append
        // concatenate two JSON objects on one line.
        let dir = TempDir::new();
        let line = serde_json::to_string(&record()).expect("serialize");
        fs::write(dir.path().join(MANIFEST_FILE_NAME), line).expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::UnterminatedManifestLine { line: 1 })
        ));
    }

    #[test]
    fn unsupported_schema_version_is_hard_error() {
        let dir = TempDir::new();
        let line = serde_json::to_string(&record())
            .expect("serialize")
            .replacen(r#""schema_version":1"#, r#""schema_version":2"#, 1);
        fs::write(dir.path().join(MANIFEST_FILE_NAME), format!("{line}\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::UnsupportedSchemaVersion { line: 1, found: 2 })
        ));
    }

    #[test]
    fn open_validates_semantics_of_hand_edited_lines() {
        // A hand-edited manifest gets no more trust than a caller.
        // (a) Backwards range: [200, 100) is invalid under half-open rules.
        let dir = TempDir::new();
        let mut bad = record();
        bad.start_ts = Ts(200);
        bad.end_ts = Ts(100);
        let line = serde_json::to_string(&bad).expect("serialize");
        fs::write(dir.path().join(MANIFEST_FILE_NAME), format!("{line}\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::InvalidRange { .. })
        ));

        // (b) Duplicate file_path across two lines: immutability violation.
        let dir2 = TempDir::new();
        let line = serde_json::to_string(&record()).expect("serialize");
        fs::write(
            dir2.path().join(MANIFEST_FILE_NAME),
            format!("{line}\n{line}\n"),
        )
        .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir2.path()),
            Err(CatalogError::DuplicateFilePath { .. })
        ));

        // (c) A manifest id is canonical lowercase blake3, never arbitrary
        // text that merely happened to deserialize.
        let dir3 = TempDir::new();
        let mut bad = record();
        bad.file_blake3 = "A".repeat(64);
        let line = serde_json::to_string(&bad).expect("serialize");
        fs::write(dir3.path().join(MANIFEST_FILE_NAME), format!("{line}\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir3.path()),
            Err(CatalogError::InvalidFileBlake3 { .. })
        ));

        // (d) A backslashed/absolute file_path must be refused on load, or a
        // hand edit could smuggle in the exact portability violations append
        // rejects (negative control: the load-path validators must fire).
        let dir4 = TempDir::new();
        let mut bad = record();
        bad.file_path = "raw\\esu6.dbn.zst".to_owned();
        let line = serde_json::to_string(&bad).expect("serialize");
        fs::write(dir4.path().join(MANIFEST_FILE_NAME), format!("{line}\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir4.path()),
            Err(CatalogError::InvalidFilePath { .. })
        ));

        // (e) An empty symbols list must be refused on load.
        let dir5 = TempDir::new();
        let mut bad = record();
        bad.symbols = vec![];
        let line = serde_json::to_string(&bad).expect("serialize");
        fs::write(dir5.path().join(MANIFEST_FILE_NAME), format!("{line}\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir5.path()),
            Err(CatalogError::InvalidSymbols { .. })
        ));

        // (f) A whitespace-bearing dataset must be refused on load.
        let dir6 = TempDir::new();
        let mut bad = record();
        bad.dataset = "GLBX MDP3".to_owned();
        let line = serde_json::to_string(&bad).expect("serialize");
        fs::write(dir6.path().join(MANIFEST_FILE_NAME), format!("{line}\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir6.path()),
            Err(CatalogError::InvalidName {
                field: "dataset",
                ..
            })
        ));
    }

    #[test]
    fn crlf_manifest_is_hard_error() {
        // The wire format is LF-only: a CRLF hand edit parses identically
        // via str::lines but diverges byte-wise from what append writes,
        // silently breaking byte-identical portability (D-0019).
        let dir = TempDir::new();
        let line = serde_json::to_string(&record()).expect("serialize");
        fs::write(dir.path().join(MANIFEST_FILE_NAME), format!("{line}\r\n"))
            .expect("write manifest");
        assert!(matches!(
            Catalog::open(dir.path()),
            Err(CatalogError::CarriageReturnInManifest { line: 1 })
        ));
    }

    // --------------------------------------------------------------- append

    #[test]
    fn append_computes_checksum_and_size_from_disk() {
        let dir = TempDir::new();
        // b"crucible" is 8 bytes.
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        let rec = catalog.append(acq(FIXTURE_REL_PATH)).expect("append");

        assert_eq!(rec.size_bytes, 8);
        // Golden: the vector-validated constant (derivation on CRUCIBLE_B3).
        assert_eq!(rec.file_blake3, CRUCIBLE_B3);
        // Plumbing: append hashed exactly the file's bytes — re-read the
        // file independently and hash it here.
        let bytes = fs::read(dir.path().join(FIXTURE_REL_PATH)).expect("read fixture back");
        assert_eq!(rec.file_blake3, blake3::hash(&bytes).to_hex().to_string());
    }

    #[test]
    fn append_then_reload_roundtrips() {
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        plant_file(dir.path(), "raw/other.dbn.zst", b"other bytes");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        let first = catalog.append(acq(FIXTURE_REL_PATH)).expect("append 1");
        let second = catalog.append(acq("raw/other.dbn.zst")).expect("append 2");
        drop(catalog);

        let reloaded = Catalog::open(dir.path()).expect("reopen");
        assert_eq!(reloaded.records(), &[first, second]);
    }

    #[test]
    fn append_creates_manifest_lazily() {
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        plant_file(dir.path(), "raw/other.dbn.zst", b"other bytes");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        let manifest = dir.path().join(MANIFEST_FILE_NAME);
        assert!(!manifest.exists(), "no manifest before first append");

        catalog.append(acq(FIXTURE_REL_PATH)).expect("append 1");
        assert!(manifest.exists(), "manifest created by first append");
        catalog.append(acq("raw/other.dbn.zst")).expect("append 2");
        let content = fs::read_to_string(&manifest).expect("read manifest");
        assert_eq!(content.lines().count(), 2);
    }

    #[test]
    fn append_rejects_duplicate_file_path() {
        // Immutability negative control: the second record for the same path
        // must be refused and must leave no trace.
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("first append");
        assert!(matches!(
            catalog.append(acq(FIXTURE_REL_PATH)),
            Err(CatalogError::DuplicateFilePath { .. })
        ));
        assert_eq!(catalog.records().len(), 1);
        let content =
            fs::read_to_string(dir.path().join(MANIFEST_FILE_NAME)).expect("read manifest");
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn append_rejects_duplicate_file_path_from_stale_handle() {
        // Both handles start with an empty view. The second append must
        // refresh under the manifest lock and observe the first append.
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut first = Catalog::open(dir.path()).expect("open first");
        let mut stale = Catalog::open(dir.path()).expect("open stale second");

        first.append(acq(FIXTURE_REL_PATH)).expect("first append");
        assert!(matches!(
            stale.append(acq(FIXTURE_REL_PATH)),
            Err(CatalogError::DuplicateFilePath { .. })
        ));
        assert_eq!(stale.records().len(), 1, "stale view was refreshed");
        let content =
            fs::read_to_string(dir.path().join(MANIFEST_FILE_NAME)).expect("read manifest");
        assert_eq!(content.lines().count(), 1, "no duplicate was written");
    }

    #[test]
    fn append_rejects_missing_archive_file() {
        let dir = TempDir::new();
        let mut catalog = Catalog::open(dir.path()).expect("open");
        assert!(matches!(
            catalog.append(acq(FIXTURE_REL_PATH)),
            Err(CatalogError::ArchiveFileMissing { .. })
        ));
        assert!(catalog.records().is_empty());
        assert!(
            !dir.path().join(MANIFEST_FILE_NAME).exists(),
            "nothing written"
        );
    }

    #[test]
    fn range_construction_rejects_invalid() {
        // Half-open [start, end): zero-width and backwards are both invalid.
        assert!(matches!(
            TsRange::new(Ts(200), Ts(200)),
            Err(CatalogError::InvalidRange { .. })
        ));
        assert!(matches!(
            TsRange::new(Ts(200), Ts(100)),
            Err(CatalogError::InvalidRange { .. })
        ));
    }

    #[test]
    fn append_rejects_bad_file_paths() {
        let dir = TempDir::new();
        let mut catalog = Catalog::open(dir.path()).expect("open");
        // Path validation runs before the existence check, so no files needed.
        let bad_paths = [
            "",                  // empty
            "/x",                // absolute
            "C:/x",              // drive prefix (colon)
            "raw\\esu6.dbn.zst", // backslash
            "raw/../secrets",    // traversal
            "raw/./x",           // dot component
            "raw//x",            // empty component
            "raw/",              // empty remainder
            "raw",               // not under raw/
            "curated/x.parquet", // not under raw/
            "raw/my file.zst",   // whitespace
            "raw/x\t.zst",       // control/tab
        ];
        for bad in bad_paths {
            let mut a = acq(FIXTURE_REL_PATH);
            a.file_path = bad.to_owned();
            assert!(
                matches!(catalog.append(a), Err(CatalogError::InvalidFilePath { .. })),
                "path {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn append_rejects_bad_symbols_and_names() {
        let dir = TempDir::new();
        let mut catalog = Catalog::open(dir.path()).expect("open");

        for bad_symbols in [
            vec![],
            vec![String::new()],
            vec!["ES/U6".to_owned()],
            vec!["ES U6".to_owned()],
        ] {
            let mut a = acq(FIXTURE_REL_PATH);
            a.symbols = bad_symbols.clone();
            assert!(
                matches!(catalog.append(a), Err(CatalogError::InvalidSymbols { .. })),
                "symbols {bad_symbols:?} must be rejected"
            );
        }

        let mut a = acq(FIXTURE_REL_PATH);
        a.dataset = String::new();
        assert!(matches!(
            catalog.append(a),
            Err(CatalogError::InvalidName {
                field: "dataset",
                ..
            })
        ));
        let mut a = acq(FIXTURE_REL_PATH);
        a.schema = "oh no".to_owned();
        assert!(matches!(
            catalog.append(a),
            Err(CatalogError::InvalidName {
                field: "schema",
                ..
            })
        ));
    }

    // ------------------------------------------------------------- coverage
    // All requests are [100, 200) for ESU6 in (GLBX.MDP3, trades) unless
    // noted. Expected gaps are hand-derived; the arithmetic is stated per
    // test (testdata/README.md rule 2).

    #[test]
    fn coverage_empty_manifest_is_full_gap() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[]);
        // Nothing owned: the gap is the whole request, [100, 200).
        assert_eq!(gaps(&catalog, 100, 200), vec![ts_range(100, 200)]);
    }

    #[test]
    fn coverage_exact_cover_has_no_gap() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(100, 200)]);
        // Owned [100,200) == requested [100,200): no gap, and the symbol is
        // still present as a key (covered is visible, not absent).
        assert_eq!(gaps(&catalog, 100, 200), Vec::<TsRange>::new());
    }

    #[test]
    fn coverage_partial_overlap_left() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(100, 150)]);
        // Owned [100,150) covers the left half; missing is [150,200).
        assert_eq!(gaps(&catalog, 100, 200), vec![ts_range(150, 200)]);
    }

    #[test]
    fn coverage_partial_overlap_right() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(150, 200)]);
        // Owned [150,200) covers the right half; missing is [100,150).
        assert_eq!(gaps(&catalog, 100, 200), vec![ts_range(100, 150)]);
    }

    #[test]
    fn coverage_owned_contains_request() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(50, 300)]);
        // Owned [50,300) ⊃ requested [100,200): fully covered.
        assert_eq!(gaps(&catalog, 100, 200), Vec::<TsRange>::new());
    }

    #[test]
    fn coverage_request_contains_owned() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(130, 160)]);
        // Owned [130,160) sits inside [100,200): missing [100,130) and [160,200).
        assert_eq!(
            gaps(&catalog, 100, 200),
            vec![ts_range(100, 130), ts_range(160, 200)]
        );
    }

    #[test]
    fn coverage_multiple_disjoint_owned() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(100, 120), (140, 160)]);
        // Owned [100,120) and [140,160): missing [120,140) and [160,200).
        assert_eq!(
            gaps(&catalog, 100, 200),
            vec![ts_range(120, 140), ts_range(160, 200)]
        );
    }

    #[test]
    fn coverage_out_of_order_backfill_merges() {
        // Backfill order:
        // recent slice acquired first, older slice appended after it.
        // [140,200) then [100,160): union is [100,200), so no gap.
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(140, 200), (100, 160)]);
        assert_eq!(gaps(&catalog, 100, 200), Vec::<TsRange>::new());
    }

    #[test]
    fn coverage_adjacent_ranges_merge_seamlessly() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(100, 150), (150, 200)]);
        // The half-open payoff: [100,150) + [150,200) tile [100,200) exactly —
        // no phantom 1-ns gap at 150.
        assert_eq!(gaps(&catalog, 100, 200), Vec::<TsRange>::new());
    }

    #[test]
    fn coverage_overlapping_owned_ranges_merge() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(100, 160), (140, 200)]);
        // [100,160) ∪ [140,200) = [100,200): fully covered.
        assert_eq!(gaps(&catalog, 100, 200), Vec::<TsRange>::new());
    }

    #[test]
    fn coverage_ignores_ranges_outside_request() {
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(0, 100), (200, 300)]);
        // [0,100) touches the request only at its open edge and [200,300)
        // starts exactly at the exclusive end: neither contributes, so the
        // whole [100,200) is missing.
        assert_eq!(gaps(&catalog, 100, 200), vec![ts_range(100, 200)]);
    }

    #[test]
    fn coverage_multi_symbol_record_credits_each_symbol() {
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        let mut a = acq(FIXTURE_REL_PATH);
        a.symbols = vec!["ESU6".to_owned(), "NQU6".to_owned()];
        catalog.append(a).expect("append");

        let req = CoverageRequest {
            dataset: "GLBX.MDP3".to_owned(),
            schema: "trades".to_owned(),
            symbols: vec!["ESU6".to_owned(), "NQU6".to_owned()],
            range: ts_range(100, 200),
        };
        let cov = catalog.coverage(&req);
        // One record listing both symbols covers [100,200) for each.
        assert_eq!(cov.len(), 2);
        assert_eq!(cov["ESU6"], Vec::<TsRange>::new());
        assert_eq!(cov["NQU6"], Vec::<TsRange>::new());
    }

    #[test]
    fn coverage_per_symbol_gaps_differ() {
        let dir = TempDir::new();
        // ESU6 owns [100,200); NQU6 owns nothing.
        let catalog = catalog_with_ranges(&dir, &[(100, 200)]);
        let req = CoverageRequest {
            dataset: "GLBX.MDP3".to_owned(),
            schema: "trades".to_owned(),
            // Deliberately unsorted request order (and a duplicate) — output
            // keys must come back sorted and deduplicated (determinism §2.2).
            symbols: vec!["NQU6".to_owned(), "ESU6".to_owned(), "NQU6".to_owned()],
            range: ts_range(100, 200),
        };
        let cov = catalog.coverage(&req);
        let keys: Vec<&str> = cov.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["ESU6", "NQU6"]);
        assert_eq!(cov["ESU6"], Vec::<TsRange>::new());
        assert_eq!(cov["NQU6"], vec![ts_range(100, 200)]);
    }

    #[test]
    fn coverage_filters_on_dataset_and_schema() {
        // Negative control: wrong-schema or wrong-dataset data must NEVER
        // suppress a download.
        let dir = TempDir::new();
        let catalog = catalog_with_ranges(&dir, &[(100, 200)]); // (GLBX.MDP3, trades)

        for (dataset, schema) in [
            ("GLBX.MDP3", "ohlcv-1m"), // right dataset, wrong schema
            ("XNAS.ITCH", "trades"),   // wrong dataset, right schema
        ] {
            let req = CoverageRequest {
                dataset: dataset.to_owned(),
                schema: schema.to_owned(),
                symbols: vec!["ESU6".to_owned()],
                range: ts_range(100, 200),
            };
            let cov = catalog.coverage(&req);
            assert_eq!(
                cov["ESU6"],
                vec![ts_range(100, 200)],
                "({dataset}, {schema}) must not be credited by (GLBX.MDP3, trades)"
            );
        }
    }

    // --------------------------------------------------------------- verify

    #[test]
    fn verify_clean_archive_is_clean() {
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        plant_file(dir.path(), "raw/other.dbn.zst", b"other bytes");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append 1");
        catalog.append(acq("raw/other.dbn.zst")).expect("append 2");
        assert!(catalog.verify().expect("verify").is_clean());
    }

    #[test]
    fn verify_detects_planted_corruption() {
        // Mandatory negative control (CLAUDE.md §7): plant a known
        // corruption and assert the detector fires. Same length (8 bytes,
        // last byte e→f) so this exercises the checksum path, not size.
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append");

        fs::write(dir.path().join(FIXTURE_REL_PATH), b"cruciblf").expect("corrupt file");
        let report = catalog.verify().expect("verify");
        assert_eq!(report.findings.len(), 1);
        match &report.findings[0] {
            VerifyFinding::ChecksumMismatch {
                file_path,
                expected,
                actual,
            } => {
                assert_eq!(file_path, FIXTURE_REL_PATH);
                assert_eq!(expected, CRUCIBLE_B3);
                // The observed hash is blake3 of the corrupted bytes.
                assert_eq!(actual, &blake3::hash(b"cruciblf").to_hex().to_string());
                assert_ne!(expected, actual);
            }
            other => panic!("expected ChecksumMismatch, got {other:?}"),
        }
    }

    #[test]
    fn verify_detects_missing_file() {
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append");

        // The TEST deletes via std::fs — the catalog API has no delete.
        fs::remove_file(dir.path().join(FIXTURE_REL_PATH)).expect("delete file");
        let report = catalog.verify().expect("verify");
        assert_eq!(
            report.findings,
            vec![VerifyFinding::MissingFile {
                file_path: FIXTURE_REL_PATH.to_owned()
            }]
        );
    }

    #[test]
    fn verify_detects_directory_at_recorded_path() {
        // The not-a-regular-file branch must fire as MissingFile, not fall
        // through to a confusing SizeMismatch or an Io abort (negative
        // control: plausible after the manual archive surgery the
        // crash-safety doc directs).
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append");

        // The TEST swaps the file for a directory via std::fs — the catalog
        // API cannot.
        fs::remove_file(dir.path().join(FIXTURE_REL_PATH)).expect("delete file");
        fs::create_dir(dir.path().join(FIXTURE_REL_PATH)).expect("dir at recorded path");
        let report = catalog.verify().expect("verify");
        assert_eq!(
            report.findings,
            vec![VerifyFinding::MissingFile {
                file_path: FIXTURE_REL_PATH.to_owned()
            }]
        );
    }

    #[test]
    fn verify_detects_size_change_as_size_mismatch() {
        // Precedence pinned: a 9-byte file against an 8-byte record is a
        // SizeMismatch — the checksum is not even computed.
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append");

        fs::write(dir.path().join(FIXTURE_REL_PATH), b"crucibleX").expect("grow file");
        let report = catalog.verify().expect("verify");
        assert_eq!(
            report.findings,
            vec![VerifyFinding::SizeMismatch {
                file_path: FIXTURE_REL_PATH.to_owned(),
                expected: 8,
                actual: 9,
            }]
        );
    }

    #[test]
    fn verify_reports_all_findings_in_record_order() {
        // The audit reports the full extent of the damage, in manifest
        // record order — it does not stop at the first hit.
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        plant_file(dir.path(), "raw/other.dbn.zst", b"other bytes");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append 1");
        catalog.append(acq("raw/other.dbn.zst")).expect("append 2");

        fs::write(dir.path().join(FIXTURE_REL_PATH), b"cruciblf").expect("corrupt first");
        fs::remove_file(dir.path().join("raw/other.dbn.zst")).expect("delete second");
        let report = catalog.verify().expect("verify");
        assert_eq!(report.findings.len(), 2);
        assert!(matches!(
            &report.findings[0],
            VerifyFinding::ChecksumMismatch { file_path, .. } if file_path == FIXTURE_REL_PATH
        ));
        assert!(matches!(
            &report.findings[1],
            VerifyFinding::MissingFile { file_path } if file_path == "raw/other.dbn.zst"
        ));
    }

    // ----------------------------------------------------------- manifest id

    #[test]
    fn record_by_id_finds_record() {
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append");

        let found = catalog.record_by_id(CRUCIBLE_B3).expect("id resolves");
        assert_eq!(found.file_path, FIXTURE_REL_PATH);
        assert!(catalog.record_by_id("deadbeef").is_none());
    }

    #[test]
    fn record_by_id_duplicate_bytes_returns_first() {
        // Identical bytes at two paths legally share a manifest id (the id
        // names the bytes); lookup returns the first record in file order.
        let dir = TempDir::new();
        plant_file(dir.path(), FIXTURE_REL_PATH, b"crucible");
        plant_file(dir.path(), "raw/copy.dbn.zst", b"crucible");
        let mut catalog = Catalog::open(dir.path()).expect("open");
        catalog.append(acq(FIXTURE_REL_PATH)).expect("append 1");
        catalog.append(acq("raw/copy.dbn.zst")).expect("append 2");

        let found = catalog.record_by_id(CRUCIBLE_B3).expect("id resolves");
        assert_eq!(found.file_path, FIXTURE_REL_PATH, "first in file order");
    }
}
