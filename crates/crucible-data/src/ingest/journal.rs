//! The job journal: what we have already bought, written down before we buy it.
//!
//! A batch submission is the only irreversible act in this crate. The vendor
//! offers **no idempotency key, no cancel endpoint, and no refund**, so a
//! second submission of the same window is simply a second purchase. What it
//! *does* offer is a 30-day download window measured from submission — which
//! means a job whose id we still know is always recoverable for free, and a
//! job whose id we lost is money we cannot collect.
//!
//! That asymmetry dictates the whole design: **record the intent, then
//! submit**, never the reverse.
//!
//! ## Identity
//!
//! [`intent_id`] is a blake3 digest over `(dataset, schema, key, stype_in,
//! start_ts, end_ts)` — deterministic, clock-free, and independent of how the
//! plan was cut. Re-running the same `pull` command recomputes the same id and
//! therefore *recognises its own previous attempt* instead of starting a fresh
//! one. Note what is absent: `target_file_path` is derivable from those six
//! fields, and `WindowSpan` is not part of the identity because ids are
//! computed per planned window, after the span has already done its work.
//!
//! ## Framing
//!
//! Identical to the manifest's, and for the same reasons: append-only JSONL,
//! LF-only (D-0019), every line `\n`-terminated, one `write_all` then `fsync`,
//! an exclusive OS file lock held across a reload-then-append (D-0018), a
//! required `schema_version` with `deny_unknown_fields` (D-0016), and a
//! malformed line is a hard error naming the line rather than a line quietly
//! skipped. A journal that silently drops a record is a journal that
//! authorises a double purchase.
//!
//! The journal lives at the data-dir root beside `manifest.jsonl` — **outside
//! `raw/`**, which is immutable and records acquisitions, not attempts.

use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::{TsRange, ts_serde};
use crate::ingest::provider::StypeIn;
use crucible_core::types::{NanoUsd, Ts};

/// Wire version of a journal entry. Bump only with a decision-log entry.
pub const JOURNAL_SCHEMA_VERSION: u32 = 1;

/// File name of the job journal, directly under the data dir.
pub const JOURNAL_FILE_NAME: &str = "jobs.jsonl";

/// One recorded step in the life of one download intent.
///
/// Externally tagged, so a line reads `{"intended":{…}}` and an unknown event
/// name fails to parse instead of being ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum JournalEvent {
    /// We are about to submit this window. Written and fsynced **before** the
    /// submission, so a crash in the network call still leaves evidence that
    /// a purchase may exist.
    Intended {
        /// Vendor dataset.
        dataset: String,
        /// Vendor schema.
        schema: String,
        /// Requested symbol key.
        key: String,
        /// Input symbology, wire spelling.
        stype_in: String,
        /// Window start, UTC nanoseconds.
        #[serde(with = "ts_serde")]
        start_ts: Ts,
        /// Window end, UTC nanoseconds, exclusive.
        #[serde(with = "ts_serde")]
        end_ts: Ts,
        /// Archive path this window is destined for.
        target_file_path: String,
        /// What the quote said this would cost, in nanodollars.
        quoted_cost_nano_usd: NanoUsd,
    },
    /// The vendor accepted the job. `adopted` is true when the id was
    /// recovered by reconciling against the vendor's job list rather than
    /// returned by our own `submit` — i.e. we found a purchase we had already
    /// made.
    Submitted {
        /// Vendor batch job id.
        job_id: String,
        /// Whether this id was adopted from the vendor rather than submitted.
        adopted: bool,
    },
    /// The payload is staged locally and has passed size and checksum checks.
    Downloaded {
        /// Vendor batch job id.
        job_id: String,
        /// Delivered file name, as it sits in the staging dir.
        staged_file_name: String,
        /// Byte length as staged.
        size_bytes: u64,
    },
    /// The payload is in `raw/` and the manifest records it. Terminal.
    Appended {
        /// Vendor batch job id.
        job_id: String,
        /// Relative archive path.
        file_path: String,
        /// Manifest id of the archived bytes (D-0014).
        file_blake3: String,
    },
    /// The intent was explicitly given up on, freeing it to be purchased
    /// again. Only ever written on an operator's explicit instruction.
    Abandoned {
        /// Vendor batch job id, when there was one.
        job_id: Option<String>,
        /// Why, in the operator's terms.
        reason: String,
    },
}

/// One line of the journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    /// Wire version; must equal [`JOURNAL_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Identity of the download intent this step belongs to.
    pub intent_id: String,
    /// When the step was recorded, UTC nanoseconds. Caller-supplied — this
    /// crate reads no clock (D-0015).
    #[serde(with = "ts_serde")]
    pub ts: Ts,
    /// What happened.
    pub event: JournalEvent,
}

impl JournalEntry {
    /// Builds an entry at the current schema version.
    #[must_use]
    pub fn new(intent_id: &str, ts: Ts, event: JournalEvent) -> JournalEntry {
        JournalEntry {
            schema_version: JOURNAL_SCHEMA_VERSION,
            intent_id: intent_id.to_string(),
            ts,
            event,
        }
    }
}

/// The folded state of one intent: everything the resume path needs to decide
/// what to do next, and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntentState {
    /// An `Intended` record exists — a submission may or may not have landed.
    pub intended: bool,
    /// The vendor job id, once known.
    pub job_id: Option<String>,
    /// Staged file name, once the payload was downloaded and verified.
    pub staged_file_name: Option<String>,
    /// The archive and manifest are done with this intent.
    pub appended: bool,
    /// The operator gave up on it.
    pub abandoned: bool,
}

impl IntentState {
    /// Nothing more to do: the bytes are archived and recorded.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.appended
    }

    /// A submission exists (ours or adopted) that has not yet been archived.
    #[must_use]
    pub fn is_in_flight(&self) -> bool {
        self.job_id.is_some() && !self.appended && !self.abandoned
    }
}

/// Why the journal could not be read or written.
#[derive(Debug)]
pub enum JournalError {
    /// Filesystem failure.
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying failure.
        source: std::io::Error,
    },
    /// A line is not valid JSON for a [`JournalEntry`].
    MalformedLine {
        /// 1-based line number.
        line: usize,
        /// Parse failure.
        source: serde_json::Error,
    },
    /// The final line has no terminating newline — a torn append.
    UnterminatedLine {
        /// 1-based line number.
        line: usize,
    },
    /// A carriage return appears in the framing (D-0019).
    CarriageReturn {
        /// 1-based line number.
        line: usize,
    },
    /// A line declares a version this build does not understand.
    UnsupportedSchemaVersion {
        /// 1-based line number.
        line: usize,
        /// Version found on that line.
        found: u32,
    },
}

impl core::fmt::Display for JournalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            JournalError::Io { path, source } => {
                write!(f, "job journal at {}: {source}", path.display())
            }
            JournalError::MalformedLine { line, source } => write!(
                f,
                "job journal line {line} is not a valid entry: {source}. \
                 Refusing to act on a journal that cannot be read in full — \
                 a dropped record authorises a second purchase"
            ),
            JournalError::UnterminatedLine { line } => write!(
                f,
                "job journal line {line} has no terminating newline (a torn \
                 append); repair it by hand before running again"
            ),
            JournalError::CarriageReturn { line } => write!(
                f,
                "job journal line {line} contains a carriage return; framing \
                 is LF-only (D-0019)"
            ),
            JournalError::UnsupportedSchemaVersion { line, found } => write!(
                f,
                "job journal line {line} declares schema_version {found}, but \
                 this build understands {JOURNAL_SCHEMA_VERSION}"
            ),
        }
    }
}

impl std::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JournalError::Io { source, .. } => Some(source),
            JournalError::MalformedLine { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Deterministic identity of a download intent.
///
/// blake3 over the six fields that decide *what bytes are being bought*,
/// separated by a character that cannot occur in any of them (all four string
/// fields are validated ASCII without control characters), so no two distinct
/// intents can collide by concatenation.
#[must_use]
pub fn intent_id(
    dataset: &str,
    schema: &str,
    key: &str,
    stype_in: StypeIn,
    range: TsRange,
) -> String {
    let canonical = format!(
        "{dataset}\u{1f}{schema}\u{1f}{key}\u{1f}{stype_in}\u{1f}{}\u{1f}{}",
        range.start_ts().0,
        range.end_ts().0
    );
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

/// An append-only record of every download intent and what became of it.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    entries: Vec<JournalEntry>,
}

impl Journal {
    /// Opens (or lazily creates) the journal under `data_dir`.
    ///
    /// A missing file is an empty journal; a corrupt one is a hard error.
    ///
    /// # Errors
    /// [`JournalError`] if the file exists but cannot be read or parsed.
    pub fn open(data_dir: &Path) -> Result<Journal, JournalError> {
        let path = data_dir.join(JOURNAL_FILE_NAME);
        let entries = match std::fs::read_to_string(&path) {
            Ok(text) => parse(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(source) => {
                return Err(JournalError::Io {
                    path: path.clone(),
                    source,
                });
            }
        };
        Ok(Journal { path, entries })
    }

    /// Path of the journal file, whether or not it exists yet.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every entry, in file order.
    #[must_use]
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Folds the events recorded for `intent_id` into the state the resume
    /// path acts on.
    #[must_use]
    pub fn state_of(&self, intent_id: &str) -> IntentState {
        let mut state = IntentState::default();
        for entry in self.entries.iter().filter(|e| e.intent_id == intent_id) {
            match &entry.event {
                JournalEvent::Intended { .. } => state.intended = true,
                JournalEvent::Submitted { job_id, .. } => {
                    state.job_id = Some(job_id.clone());
                    state.abandoned = false;
                }
                JournalEvent::Downloaded {
                    staged_file_name, ..
                } => state.staged_file_name = Some(staged_file_name.clone()),
                JournalEvent::Appended { .. } => state.appended = true,
                JournalEvent::Abandoned { .. } => {
                    state.abandoned = true;
                    state.job_id = None;
                    state.staged_file_name = None;
                }
            }
        }
        state
    }

    /// Every vendor job id this journal has ever claimed, abandoned or not.
    ///
    /// Reconciliation consults this so one vendor job can never be adopted by
    /// two different intents — which would append the same bytes twice under
    /// two different archive paths.
    #[must_use]
    pub fn claimed_job_ids(&self) -> BTreeSet<String> {
        self.entries
            .iter()
            .filter_map(|e| match &e.event {
                JournalEvent::Submitted { job_id, .. }
                | JournalEvent::Downloaded { job_id, .. }
                | JournalEvent::Appended { job_id, .. } => Some(job_id.clone()),
                JournalEvent::Abandoned { job_id, .. } => job_id.clone(),
                JournalEvent::Intended { .. } => None,
            })
            .collect()
    }

    /// Appends one entry and fsyncs it before returning.
    ///
    /// Takes an exclusive lock and re-reads the file under it, so a journal
    /// that grew or got corrupted underneath a long-lived handle is noticed
    /// before this entry is written on top of it.
    ///
    /// # Errors
    /// [`JournalError`] on any I/O or parse failure.
    pub fn append(&mut self, entry: JournalEntry) -> Result<(), JournalError> {
        let mut line = serde_json::to_string(&entry).map_err(|source| {
            // Entries are plain strings, ints, and bools; serialization
            // cannot fail. Reported rather than panicked so a future field
            // addition surfaces as an error instead of an abort.
            JournalError::MalformedLine { line: 0, source }
        })?;
        line.push('\n');

        let io_err = |source: std::io::Error| JournalError::Io {
            path: self.path.clone(),
            source,
        };

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&self.path)
            .map_err(io_err)?;
        file.lock().map_err(io_err)?;

        let mut existing = String::new();
        file.seek(SeekFrom::Start(0)).map_err(io_err)?;
        file.read_to_string(&mut existing).map_err(io_err)?;
        self.entries = parse(&existing)?;

        file.write_all(line.as_bytes()).map_err(io_err)?;
        file.sync_all().map_err(io_err)?;

        self.entries.push(entry);
        Ok(())
    }
}

/// Parses the whole journal, refusing anything it cannot read exactly.
fn parse(text: &str) -> Result<Vec<JournalEntry>, JournalError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if !text.ends_with('\n') {
        return Err(JournalError::UnterminatedLine {
            line: text.split('\n').count(),
        });
    }
    let mut out = Vec::new();
    // Split on '\n' rather than using `lines()`, which silently strips a
    // trailing '\r' and would make the CRLF check below unfireable. The
    // final newline is removed first so the split has no empty tail.
    let body = &text[..text.len() - 1];
    for (idx, raw) in body.split('\n').enumerate() {
        let line = idx + 1;
        if raw.contains('\r') {
            return Err(JournalError::CarriageReturn { line });
        }
        let entry: JournalEntry = serde_json::from_str(raw)
            .map_err(|source| JournalError::MalformedLine { line, source })?;
        if entry.schema_version != JOURNAL_SCHEMA_VERSION {
            return Err(JournalError::UnsupportedSchemaVersion {
                line,
                found: entry.schema_version,
            });
        }
        out.push(entry);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::plan::range_from_dates;
    use crate::testutil::TempDir;

    fn range() -> TsRange {
        range_from_dates((2024, 1, 1), (2024, 2, 1)).expect("valid range")
    }

    fn intended() -> JournalEvent {
        JournalEvent::Intended {
            dataset: "GLBX.MDP3".to_string(),
            schema: "ohlcv-1m".to_string(),
            key: "ES.FUT".to_string(),
            stype_in: "parent".to_string(),
            start_ts: range().start_ts(),
            end_ts: range().end_ts(),
            target_file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_string(),
            quoted_cost_nano_usd: 140_000_000,
        }
    }

    // The identity property the whole no-double-purchase design rests on:
    // the same request always computes the same id, on any machine, with no
    // clock and no dependence on how the plan was cut.
    #[test]
    fn intent_ids_are_deterministic_and_field_sensitive() {
        let a = intent_id("GLBX.MDP3", "ohlcv-1m", "ES.FUT", StypeIn::Parent, range());
        let b = intent_id("GLBX.MDP3", "ohlcv-1m", "ES.FUT", StypeIn::Parent, range());
        assert_eq!(a, b, "same inputs must give the same id");
        assert_eq!(a.len(), 64, "blake3 hex");

        // Every field participates.
        assert_ne!(
            a,
            intent_id("GLBX.MDP3", "ohlcv-1s", "ES.FUT", StypeIn::Parent, range())
        );
        assert_ne!(
            a,
            intent_id("GLBX.MDP3", "ohlcv-1m", "NQ.FUT", StypeIn::Parent, range())
        );
        assert_ne!(
            a,
            intent_id(
                "GLBX.MDP3",
                "ohlcv-1m",
                "ES.FUT",
                StypeIn::RawSymbol,
                range()
            )
        );
        assert_ne!(
            a,
            intent_id(
                "GLBX.MDP3",
                "ohlcv-1m",
                "ES.FUT",
                StypeIn::Parent,
                range_from_dates((2024, 1, 1), (2024, 3, 1)).expect("range")
            )
        );
    }

    // Concatenation must not be able to forge a collision: the separator is a
    // control character that no validated field may contain.
    #[test]
    fn intent_ids_do_not_collide_across_field_boundaries() {
        let a = intent_id("GLBX", "MDP3.ohlcv", "ES.FUT", StypeIn::Parent, range());
        let b = intent_id("GLBX.MDP3", "ohlcv", "ES.FUT", StypeIn::Parent, range());
        assert_ne!(a, b);
    }

    #[test]
    fn open_missing_journal_is_empty_and_writes_nothing() {
        let dir = TempDir::new();
        let journal = Journal::open(dir.path()).expect("open");
        assert!(journal.entries().is_empty());
        assert!(!journal.path().exists(), "open must not create the file");
    }

    #[test]
    fn append_then_reload_round_trips() {
        let dir = TempDir::new();
        let mut journal = Journal::open(dir.path()).expect("open");
        journal
            .append(JournalEntry::new("intent-a", Ts(100), intended()))
            .expect("append intended");
        journal
            .append(JournalEntry::new(
                "intent-a",
                Ts(200),
                JournalEvent::Submitted {
                    job_id: "job-1".to_string(),
                    adopted: false,
                },
            ))
            .expect("append submitted");

        let reloaded = Journal::open(dir.path()).expect("reopen");
        assert_eq!(reloaded.entries().len(), 2);
        assert_eq!(reloaded.entries(), journal.entries());

        let state = reloaded.state_of("intent-a");
        assert!(state.intended);
        assert_eq!(state.job_id.as_deref(), Some("job-1"));
        assert!(state.is_in_flight());
        assert!(!state.is_complete());
    }

    #[test]
    fn folding_reaches_the_terminal_state_and_ignores_other_intents() {
        let dir = TempDir::new();
        let mut journal = Journal::open(dir.path()).expect("open");
        for (intent, ts, event) in [
            ("a", 100, intended()),
            (
                "a",
                200,
                JournalEvent::Submitted {
                    job_id: "job-1".to_string(),
                    adopted: false,
                },
            ),
            (
                "b",
                250,
                JournalEvent::Submitted {
                    job_id: "job-2".to_string(),
                    adopted: true,
                },
            ),
            (
                "a",
                300,
                JournalEvent::Downloaded {
                    job_id: "job-1".to_string(),
                    staged_file_name: "glbx-mdp3-20240101-20240201.ohlcv-1m.dbn.zst".to_string(),
                    size_bytes: 42,
                },
            ),
            (
                "a",
                400,
                JournalEvent::Appended {
                    job_id: "job-1".to_string(),
                    file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_string(),
                    file_blake3: "0".repeat(64),
                },
            ),
        ] {
            journal
                .append(JournalEntry::new(intent, Ts(ts), event))
                .expect("append");
        }

        let a = journal.state_of("a");
        assert!(a.is_complete());
        assert!(!a.is_in_flight(), "an appended intent is not in flight");
        assert_eq!(
            a.staged_file_name.as_deref(),
            Some("glbx-mdp3-20240101-20240201.ohlcv-1m.dbn.zst")
        );

        let b = journal.state_of("b");
        assert!(!b.intended, "b never recorded an intent");
        assert_eq!(b.job_id.as_deref(), Some("job-2"));

        assert_eq!(
            journal.claimed_job_ids().into_iter().collect::<Vec<_>>(),
            vec!["job-1".to_string(), "job-2".to_string()]
        );
    }

    #[test]
    fn abandoning_clears_the_job_and_is_recorded() {
        let dir = TempDir::new();
        let mut journal = Journal::open(dir.path()).expect("open");
        journal
            .append(JournalEntry::new("a", Ts(1), intended()))
            .expect("append");
        journal
            .append(JournalEntry::new(
                "a",
                Ts(2),
                JournalEvent::Submitted {
                    job_id: "job-1".to_string(),
                    adopted: false,
                },
            ))
            .expect("append");
        journal
            .append(JournalEntry::new(
                "a",
                Ts(3),
                JournalEvent::Abandoned {
                    job_id: Some("job-1".to_string()),
                    reason: "expired before download".to_string(),
                },
            ))
            .expect("append");

        let state = journal.state_of("a");
        assert!(state.abandoned);
        assert!(state.job_id.is_none());
        assert!(!state.is_in_flight());
        // Abandoned or not, the id stays claimed: it must never be adopted
        // by some other intent later.
        assert!(journal.claimed_job_ids().contains("job-1"));
    }

    // Framing, mirroring the manifest suite. Every one of these is a case
    // where quietly skipping the line would authorise a second purchase.
    #[test]
    fn corrupt_journals_are_hard_errors_naming_the_line() {
        let dir = TempDir::new();
        let path = dir.path().join(JOURNAL_FILE_NAME);
        let good = "{\"schema_version\":1,\"intent_id\":\"a\",\"ts\":1,\
                    \"event\":{\"submitted\":{\"job_id\":\"j\",\"adopted\":false}}}";

        std::fs::write(&path, format!("{good}\n{{ nope\n")).expect("write");
        assert!(matches!(
            Journal::open(dir.path()),
            Err(JournalError::MalformedLine { line: 2, .. })
        ));

        std::fs::write(&path, format!("{good}\n{good}")).expect("write");
        assert!(matches!(
            Journal::open(dir.path()),
            Err(JournalError::UnterminatedLine { line: 2 })
        ));

        std::fs::write(&path, format!("{good}\r\n")).expect("write");
        assert!(matches!(
            Journal::open(dir.path()),
            Err(JournalError::CarriageReturn { line: 1 })
        ));

        std::fs::write(
            &path,
            format!(
                "{}\n",
                good.replace("\"schema_version\":1", "\"schema_version\":2")
            ),
        )
        .expect("write");
        assert!(matches!(
            Journal::open(dir.path()),
            Err(JournalError::UnsupportedSchemaVersion { line: 1, found: 2 })
        ));

        let unknown = good.replace("\"intent_id\"", "\"intent\",\"intent_id\"");
        std::fs::write(&path, format!("{unknown}\n")).expect("write");
        assert!(
            Journal::open(dir.path()).is_err(),
            "an unknown field must not be tolerated"
        );
    }

    // The wire format is a compatibility promise; changing it is a schema
    // change needing a decision-log entry. Hand-written, not generated.
    #[test]
    fn entry_wire_format_is_pinned() {
        let entry = JournalEntry::new(
            "abc",
            Ts(1_700_000_000_000_000_000),
            JournalEvent::Submitted {
                job_id: "job-1".to_string(),
                adopted: true,
            },
        );
        assert_eq!(
            serde_json::to_string(&entry).expect("serialize"),
            "{\"schema_version\":1,\"intent_id\":\"abc\",\"ts\":1700000000000000000,\
             \"event\":{\"submitted\":{\"job_id\":\"job-1\",\"adopted\":true}}}"
        );
    }
}
