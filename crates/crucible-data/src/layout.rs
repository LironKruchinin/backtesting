//! Archive layout — the tree shape, checked rather than assumed.
//!
//! `docs/DATA_LAYOUT.md` is the prose; this module is the enforcement. The
//! layout was a convention held up by every writer independently agreeing with
//! every other writer, which works right up until one of them does not.
//!
//! ## The two invariants, in one line each
//!
//! 1. **No directory ever holds two instruments' data.**
//! 2. **No directory ever holds two kinds of data.**
//!
//! Everything below is those two rules applied to a particular tree. They are
//! what make `rm -rf curated/bars/ESH2024` a safe, complete, obviously-correct
//! operation, and what stop a glob from silently sweeping up a neighbour.
//!
//! ## What this checks, and what it deliberately does not
//!
//! Checked: the *shape* of the tree and of every manifest `file_path`. Not
//! checked: whether the bytes are the bytes we recorded — that is
//! [`Catalog::verify`](crate::catalog::Catalog::verify), which re-hashes
//! everything and is orders of magnitude slower. The two are complementary and
//! neither implies the other: a file can hash correctly at the wrong path, and
//! a file at the right path can be corrupt.
//!
//! ## Nothing here moves anything
//!
//! This module has no mutating API at all, by construction. Manifest
//! `file_path`s are load-bearing — a result cites a manifest id, the id names
//! bytes, and the record names where those bytes live (D-0013, D-0014) — so
//! renaming an archived file breaks the provenance chain of every result that
//! ever read it. A layout violation under `raw/` is corrected the way D-0017
//! corrects everything else: by acquiring a new file at a correct path, never
//! by moving the old one. Under `curated/`, which is disposable, the fix is
//! `rm -rf` and a rebuild.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::catalog::ManifestRecord;

/// Directories sanctioned at the top of the data dir.
///
/// `raw` and `curated` hold data; `staging` and `delivery` are ingest's
/// working areas; `external` holds data that did not come through `pull` and
/// therefore is not in the manifest (D-0017's boundary, and `DATA_PLAN.md`'s
/// Cboe section). `telemetry` holds operational records of acquisition runs —
/// how a run is going and how it ended — which are neither data nor
/// acquisitions and never appear in the manifest (D-0098).
///
/// **`telemetry` is archive-wide on purpose, not per vendor.** A heartbeat
/// answers "is the machine still working?", and that question does not belong
/// to whichever vendor happens to be running: the next Databento tranche wants
/// the same file at the same path, and burying it under `external/thetadata/`
/// would mean a second copy of the same idea the first time anything else runs
/// unattended.
pub const KNOWN_ROOT_DIRS: &[&str] = &[
    "raw",
    "curated",
    "staging",
    "delivery",
    "external",
    "telemetry",
];

/// Files sanctioned at the top of the data dir.
pub const KNOWN_ROOT_FILES: &[&str] = &["manifest.jsonl", "jobs.jsonl", "pull.lock"];

/// Vendor schemas that may name a directory under `raw/{dataset}/`.
///
/// Deliberately a closed set. A typo like `ohclv-1m` would otherwise create a
/// directory that looks plausible, holds real data nobody ever reads, and is
/// discovered when a coverage query quietly reports a gap we already paid to
/// fill. Adding a schema here is a one-line change and a decision-log entry.
pub const KNOWN_SCHEMAS: &[&str] = &[
    "definition",
    "statistics",
    "status",
    "ohlcv-1s",
    "ohlcv-1m",
    "ohlcv-1h",
    "ohlcv-1d",
    "trades",
    "tbbo",
    "bbo-1s",
    "bbo-1m",
    "mbo",
    "mbp-1",
    "mbp-10",
    "imbalance",
];

/// Kinds that may name a directory under `curated/`.
///
/// `bars` and `rolls` exist today; `trades` and `book` are pinned now so the
/// M4 work lands pre-separated rather than being retrofitted into `bars`.
pub const KNOWN_CURATED_KINDS: &[&str] = &["bars", "rolls", "trades", "book"];

/// Extensions a `raw/` data file may carry.
const RAW_EXTS: &[&str] = &[".dbn.zst", ".dbn"];

/// Extensions a `curated/` data file may carry.
const CURATED_EXTS: &[&str] = &[".parquet", ".json"];

/// Deepest directory level below `raw/`: dataset(1)/schema(2)/symbol(3).
const RAW_DIR_DEPTH: usize = 3;

/// The level a `raw/` data file sits at, one below the deepest directory.
const RAW_FILE_DEPTH: usize = RAW_DIR_DEPTH + 1;

/// Deepest directory level below `curated/`: kind(1)/instrument(2)/tf(3).
const CURATED_DIR_DEPTH: usize = 3;

/// The level a `curated/` data file sits at.
const CURATED_FILE_DEPTH: usize = CURATED_DIR_DEPTH + 1;

/// Why a layout check could not be run at all.
///
/// Distinct from a *finding*: this is "the walk failed", not "the tree is
/// wrong". A tree that cannot be listed has not been shown to be clean.
#[derive(Debug)]
pub enum LayoutError {
    /// A directory could not be listed.
    Io {
        /// Path being walked.
        path: PathBuf,
        /// What was being attempted.
        during: &'static str,
        /// Underlying failure.
        source: std::io::Error,
    },
    /// The data directory does not exist.
    MissingDataDir {
        /// Path that was expected to be a directory.
        path: PathBuf,
    },
}

impl core::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LayoutError::Io {
                path,
                during,
                source,
            } => write!(f, "{during} {}: {source}", path.display()),
            LayoutError::MissingDataDir { path } => write!(
                f,
                "{} is not a directory; the archive root must exist before it \
                 can be checked (an absent archive is not a clean one)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LayoutError::Io { source, .. } => Some(source),
            LayoutError::MissingDataDir { .. } => None,
        }
    }
}

/// One way the tree departs from `docs/DATA_LAYOUT.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// Something at the top of the data dir that the layout does not name.
    UnknownRootEntry {
        /// Its name.
        name: String,
        /// Whether it is a directory.
        is_dir: bool,
    },
    /// An entry under `raw/` that is not at
    /// `raw/{dataset}/{schema}/{symbol}/{window}`.
    RawWrongDepth {
        /// Path relative to the data dir.
        path: String,
        /// How many components deep below `raw/` it sits.
        depth: usize,
        /// How many it should be. Files and directories differ, which is why
        /// this is carried rather than looked up when the message is rendered.
        expected: usize,
    },
    /// A directory under `raw/{dataset}/` whose name is not a known schema.
    UnknownSchemaDir {
        /// The dataset it sits under.
        dataset: String,
        /// The directory name.
        schema: String,
    },
    /// An entry under `curated/` that is not at
    /// `curated/{kind}/{instrument}/{grain}/{file}`.
    CuratedWrongDepth {
        /// Path relative to the data dir.
        path: String,
        /// How many components deep below `curated/` it sits.
        depth: usize,
        /// How many it should be. Files and directories differ.
        expected: usize,
    },
    /// A directory directly under `curated/` whose name is not a known kind.
    UnknownCuratedKind {
        /// The directory name.
        kind: String,
    },
    /// A `curated/bars/` directory named after a contract whose year is written
    /// with a single digit — the spelling that cannot say which decade it means
    /// (D-0072).
    AmbiguousCuratedContract {
        /// The directory name, decoded.
        instrument: String,
        /// What the key would be if the decade were known, using the pinned
        /// [`DecadeAnchor`](crate::continuous::DecadeAnchor) — shown as an
        /// *illustration* of the shape a key must have, never as a repair.
        example: String,
    },
    /// A data file in the wrong tree — Parquet under `raw/`, DBN under
    /// `curated/`.
    WrongTree {
        /// Path relative to the data dir.
        path: String,
        /// Where it is.
        found_under: &'static str,
        /// Where a file with this extension belongs.
        belongs_under: &'static str,
    },
    /// A manifest `file_path` that does not parse as
    /// `raw/{dataset}/{schema}/{symbol}/{window}`.
    ManifestPathUnparseable {
        /// 1-based manifest record number, so the line can be found.
        record: usize,
        /// The path as recorded.
        file_path: String,
        /// Which rule it broke.
        reason: String,
    },
}

impl core::fmt::Display for Finding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Finding::UnknownRootEntry { name, is_dir } => write!(
                f,
                "{}{} is not part of the archive layout. Known: {} and {}",
                name,
                if *is_dir { "/" } else { "" },
                KNOWN_ROOT_DIRS.join("/, ") + "/",
                KNOWN_ROOT_FILES.join(", ")
            ),
            Finding::RawWrongDepth {
                path,
                depth,
                expected,
            } => write!(
                f,
                "{path} sits {depth} component(s) below raw/, not {expected}. Raw data \
                 lives at raw/{{dataset}}/{{schema}}/{{symbol}}/{{window}} — files \
                 {RAW_FILE_DEPTH} deep, directories no more than {RAW_DIR_DEPTH}"
            ),
            Finding::UnknownSchemaDir { dataset, schema } => write!(
                f,
                "raw/{dataset}/{schema}/ is not a known vendor schema, so nothing \
                 will ever look in it. A typo here is data that was paid for and \
                 is invisible to coverage"
            ),
            Finding::CuratedWrongDepth {
                path,
                depth,
                expected,
            } => write!(
                f,
                "{path} sits {depth} component(s) below curated/, not {expected}. Curated \
                 data lives at curated/{{kind}}/{{instrument}}/{{grain}}/{{file}} — files \
                 {CURATED_FILE_DEPTH} deep, directories no more than {CURATED_DIR_DEPTH}"
            ),
            Finding::UnknownCuratedKind { kind } => write!(
                f,
                "curated/{kind}/ is not a known curated kind. Known: {}",
                KNOWN_CURATED_KINDS.join(", ")
            ),
            Finding::AmbiguousCuratedContract {
                instrument,
                example,
            } => write!(
                f,
                "curated/bars/{instrument}/ spells its delivery year with one digit, \
                 which repeats every ten years — and the archive's bar windows are \
                 sixteen years long, so this directory can hold two contracts whose \
                 bars concatenate in perfect order and pass every check (D-0072). A \
                 curated contract key carries a four-digit year, e.g. {example}. \
                 Curated data is disposable: delete and rebuild it —\n\
                 \x20      rm -rf curated/bars && crucible transcode"
            ),
            Finding::WrongTree {
                path,
                found_under,
                belongs_under,
            } => write!(
                f,
                "{path} is under {found_under}/ but its extension belongs under \
                 {belongs_under}/. One directory, one kind of data"
            ),
            Finding::ManifestPathUnparseable {
                record,
                file_path,
                reason,
            } => write!(
                f,
                "manifest record {record} records file_path {file_path:?}: {reason}"
            ),
        }
    }
}

/// What one layout check found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutReport {
    /// Every departure from the layout, in walk order (roots alphabetical,
    /// then depth-first, then name) followed by manifest findings in record
    /// order — the same "report everything, in a stable order" contract
    /// [`Catalog::verify`](crate::catalog::Catalog::verify) has.
    pub findings: Vec<Finding>,
    /// Data files seen under `raw/`.
    pub raw_files: usize,
    /// Data files seen under `curated/`.
    pub curated_files: usize,
    /// Manifest records checked.
    pub records: usize,
    /// Distinct `raw/{dataset}/{schema}` pairs seen, sorted.
    pub schemas_seen: Vec<String>,
    /// Distinct `curated/{kind}` names seen, sorted.
    pub kinds_seen: Vec<String>,
}

impl LayoutReport {
    /// True when the tree matches `docs/DATA_LAYOUT.md` exactly.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

impl core::fmt::Display for LayoutReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(
            f,
            "layout: {} raw file(s), {} curated file(s), {} manifest record(s)",
            self.raw_files, self.curated_files, self.records
        )?;
        if !self.schemas_seen.is_empty() {
            writeln!(f, "  raw schemas     {}", self.schemas_seen.join(", "))?;
        }
        if !self.kinds_seen.is_empty() {
            writeln!(f, "  curated kinds   {}", self.kinds_seen.join(", "))?;
        }
        if self.findings.is_empty() {
            writeln!(f, "  layout clean")
        } else {
            writeln!(f, "  {} finding(s):", self.findings.len())?;
            for finding in &self.findings {
                writeln!(f, "    {finding}")?;
            }
            Ok(())
        }
    }
}

/// Checks the archive tree and every manifest path against the layout.
///
/// Reports **every** finding rather than stopping at the first: an operator
/// deciding whether a tree is salvageable needs the extent of the problem, not
/// its first instance.
///
/// # Errors
/// [`LayoutError`] if the data directory is missing or cannot be walked. A
/// walk that failed is not a clean result.
pub fn check(data_dir: &Path, records: &[ManifestRecord]) -> Result<LayoutReport, LayoutError> {
    if !data_dir.is_dir() {
        return Err(LayoutError::MissingDataDir {
            path: data_dir.to_path_buf(),
        });
    }
    let mut report = LayoutReport {
        records: records.len(),
        ..LayoutReport::default()
    };
    let mut schemas: BTreeSet<String> = BTreeSet::new();
    let mut kinds: BTreeSet<String> = BTreeSet::new();

    for (name, is_dir) in sorted_entries(data_dir)? {
        if is_dir {
            if !KNOWN_ROOT_DIRS.contains(&name.as_str()) {
                report
                    .findings
                    .push(Finding::UnknownRootEntry { name, is_dir });
                continue;
            }
            match name.as_str() {
                "raw" => check_raw(data_dir, &mut report, &mut schemas)?,
                "curated" => check_curated(data_dir, &mut report, &mut kinds)?,
                // `staging/`, `delivery/`, `external/` and `telemetry/` are
                // working areas, foreign data, and operational records. They
                // have shapes, but not ones this project writes, so imposing
                // one would be inventing a rule to enforce it. `telemetry/` is
                // deliberately NOT depth-checked for the same reason
                // `staging/` is not (D-0098): its contents are overwritten
                // freely and nothing downstream reads a number out of them.
                _ => {}
            }
        } else if !KNOWN_ROOT_FILES.contains(&name.as_str()) {
            report
                .findings
                .push(Finding::UnknownRootEntry { name, is_dir });
        }
    }

    report.schemas_seen = schemas.into_iter().collect();
    report.kinds_seen = kinds.into_iter().collect();

    for (index, record) in records.iter().enumerate() {
        if let Err(reason) = parse_raw_path(&record.file_path) {
            report.findings.push(Finding::ManifestPathUnparseable {
                record: index + 1,
                file_path: record.file_path.clone(),
                reason,
            });
        }
    }
    Ok(report)
}

/// The pieces of a well-formed raw path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPath<'a> {
    /// Vendor dataset, e.g. `GLBX.MDP3`.
    pub dataset: &'a str,
    /// Vendor schema, e.g. `ohlcv-1m`.
    pub schema: &'a str,
    /// Symbol key the window was requested under, e.g. `ES.FUT`.
    pub symbol: &'a str,
    /// File name, e.g. `2024-01.dbn.zst`.
    pub window: &'a str,
}

/// Parses `raw/{dataset}/{schema}/{symbol}/{window}`.
///
/// This is the shape [`Catalog::append`](crate::catalog::Catalog::append)
/// records and the shape the walk expects to find on disk; parsing it in one
/// place is what keeps the two from drifting.
///
/// # Errors
/// A human-readable reason the path does not match.
pub fn parse_raw_path(file_path: &str) -> Result<RawPath<'_>, String> {
    let parts: Vec<&str> = file_path.split('/').collect();
    if parts.first() != Some(&"raw") {
        return Err("must start with raw/ — the manifest records acquisitions only".to_owned());
    }
    if parts.len() != 5 {
        return Err(format!(
            "has {} component(s); expected 5 \
             (raw/{{dataset}}/{{schema}}/{{symbol}}/{{window}})",
            parts.len()
        ));
    }
    if parts.iter().any(|p| p.is_empty()) {
        return Err("has an empty path component".to_owned());
    }
    let schema = parts[2];
    if !KNOWN_SCHEMAS.contains(&schema) {
        return Err(format!(
            "names schema {schema:?}, which is not a known vendor schema"
        ));
    }
    if !RAW_EXTS.iter().any(|e| parts[4].ends_with(e)) {
        return Err(format!(
            "names file {:?}, which is not DBN ({})",
            parts[4],
            RAW_EXTS.join(" or ")
        ));
    }
    Ok(RawPath {
        dataset: parts[1],
        schema,
        symbol: parts[3],
        window: parts[4],
    })
}

fn check_raw(
    data_dir: &Path,
    report: &mut LayoutReport,
    schemas: &mut BTreeSet<String>,
) -> Result<(), LayoutError> {
    for (rel, is_dir) in walk(&data_dir.join("raw"), "raw")? {
        let depth = rel.split('/').count() - 1; // components below `raw/`
        if is_dir {
            if depth > RAW_DIR_DEPTH {
                report.findings.push(Finding::RawWrongDepth {
                    path: format!("{rel}/"),
                    depth,
                    expected: RAW_DIR_DEPTH,
                });
                continue;
            }
            if depth == 2 {
                let parts: Vec<&str> = rel.split('/').collect();
                let (dataset, schema) = (parts[1], parts[2]);
                schemas.insert(format!("{dataset}/{schema}"));
                if !KNOWN_SCHEMAS.contains(&schema) {
                    report.findings.push(Finding::UnknownSchemaDir {
                        dataset: dataset.to_owned(),
                        schema: schema.to_owned(),
                    });
                }
            }
            continue;
        }
        report.raw_files += 1;
        if CURATED_EXTS.iter().any(|e| rel.ends_with(e)) {
            report.findings.push(Finding::WrongTree {
                path: rel.clone(),
                found_under: "raw",
                belongs_under: "curated",
            });
        }
        if depth != RAW_FILE_DEPTH {
            report.findings.push(Finding::RawWrongDepth {
                path: rel,
                depth,
                expected: RAW_FILE_DEPTH,
            });
        }
    }
    Ok(())
}

fn check_curated(
    data_dir: &Path,
    report: &mut LayoutReport,
    kinds: &mut BTreeSet<String>,
) -> Result<(), LayoutError> {
    for (rel, is_dir) in walk(&data_dir.join("curated"), "curated")? {
        let depth = rel.split('/').count() - 1;
        if is_dir {
            if depth > CURATED_DIR_DEPTH {
                report.findings.push(Finding::CuratedWrongDepth {
                    path: format!("{rel}/"),
                    depth,
                    expected: CURATED_DIR_DEPTH,
                });
                continue;
            }
            if depth == 1 {
                let kind = rel.split('/').nth(1).unwrap_or_default();
                kinds.insert(kind.to_owned());
                if !KNOWN_CURATED_KINDS.contains(&kind) {
                    report.findings.push(Finding::UnknownCuratedKind {
                        kind: kind.to_owned(),
                    });
                }
            }
            // The instrument level of `curated/bars/`. Checked here rather than
            // trusted to the writer, because the writer that produced the whole
            // archive's `curated/bars/` was the one that got it wrong (D-0072).
            if depth == 2
                && let Some(finding) = ambiguous_contract_dir(&rel)
            {
                report.findings.push(finding);
            }
            continue;
        }
        report.curated_files += 1;
        if RAW_EXTS.iter().any(|e| rel.ends_with(e)) {
            report.findings.push(Finding::WrongTree {
                path: rel.clone(),
                found_under: "curated",
                belongs_under: "raw",
            });
        }
        if depth != CURATED_FILE_DEPTH {
            report.findings.push(Finding::CuratedWrongDepth {
                path: rel,
                depth,
                expected: CURATED_FILE_DEPTH,
            });
        }
    }
    Ok(())
}

/// Whether a `curated/bars/{instrument}` directory names a contract whose year
/// is written with a single digit (D-0072).
///
/// Only `bars/` is checked, and only the instrument level. `rolls/` is keyed by
/// **root** (`curated/rolls/ES/…`), which carries no year at all; a root is not
/// a contract and must not be flagged as one.
///
/// Anything that is not an outright contract — `SYN%3ARW`, `ES.v.0`, a spread —
/// is not flagged either. A spread's key is still the vendor's spelling
/// (D-0070), and a directory that does not name a contract has no delivery year
/// to be ambiguous about.
fn ambiguous_contract_dir(rel: &str) -> Option<Finding> {
    let mut parts = rel.split('/');
    if parts.next() != Some("curated") || parts.next() != Some("bars") {
        return None;
    }
    let component = parts.next()?;
    let instrument = crate::curated::path::decode_instrument(component)?;
    let parsed = crate::continuous::parse_parts(&instrument).ok()?;
    if !parsed.has_ambiguous_year() {
        return None;
    }
    // The example is what the *pinned* anchor would have said, which is exactly
    // the guess this check exists to forbid — so it is labelled as a shape, not
    // offered as a fix. There is deliberately no `--fix` (D-0049).
    let example = crate::continuous::ContractSymbol::parse(&instrument)
        .map_or_else(|_| "GCZ2024".to_owned(), |s| s.to_string());
    Some(Finding::AmbiguousCuratedContract {
        instrument,
        example,
    })
}

/// Names and kinds of one directory's entries, sorted.
///
/// Sorted because `read_dir` order is filesystem-dependent and a report whose
/// finding order changes between machines is not a report anyone can diff
/// (CLAUDE.md §2.2).
fn sorted_entries(dir: &Path) -> Result<Vec<(String, bool)>, LayoutError> {
    let reader = std::fs::read_dir(dir).map_err(|source| LayoutError::Io {
        path: dir.to_path_buf(),
        during: "listing",
        source,
    })?;
    let mut out = Vec::new();
    for entry in reader {
        let entry = entry.map_err(|source| LayoutError::Io {
            path: dir.to_path_buf(),
            during: "reading an entry of",
            source,
        })?;
        let is_dir = entry.path().is_dir();
        out.push((entry.file_name().to_string_lossy().into_owned(), is_dir));
    }
    out.sort();
    Ok(out)
}

/// Walk yielding every `(path_relative_to_data_dir, is_dir)` under `root`.
///
/// The result is sorted, which is both what makes the report stable across
/// filesystems (§2.2) and what puts a directory immediately before its own
/// contents — so a reader sees the misplaced folder before the files in it.
fn walk(root: &Path, prefix: &str) -> Result<Vec<(String, bool)>, LayoutError> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), prefix.to_owned())];
    while let Some((dir, rel)) = stack.pop() {
        for (name, is_dir) in sorted_entries(&dir)?.into_iter().rev() {
            let child_rel = format!("{rel}/{name}");
            if is_dir {
                out.push((child_rel.clone(), true));
                stack.push((dir.join(&name), child_rel));
            } else {
                out.push((child_rel, false));
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests;
