//! Two small files that make an unattended pull answerable after it exits.
//!
//! The motivating measurement (D-0092): the first T0 run refused **794** of
//! 83,489 requests, every refusal landed in [`RunReport::refusals`] — an
//! in-memory `Vec` — and the process printed it to a console that no longer
//! exists. The archive was left with a 794-record hole and **no trace of why**:
//! `written` and `empty` both append an inventory line, `refused` deliberately
//! does not, so that a fixed build resumes cleanly. That design is right and is
//! not changed here. What was missing is that the *reason* died with the
//! process.
//!
//! At 0.951 % the refusal rate sat below [`super::run::REFUSAL_RATE_LIMIT`], so
//! the systemic breaker correctly did not trip. A sub-threshold refusal rate is
//! exactly the case that needs a durable record: too small to halt the run, too
//! large to be nothing.
//!
//! # Why two files and not one
//!
//! They answer different questions and have different lifetimes.
//!
//! - [`write_heartbeat`] answers **"is it still going?"** while the pull runs.
//!   Overwritten in place, so it never grows, and small enough that a reader
//!   can `stat` it without opening it — which matters, because the one habit
//!   this project has learned the hard way is that a default Windows reader on
//!   a file a writer holds can end the writer. The heartbeat exists so nobody
//!   ever has that reason to open `inventory.jsonl`.
//! - [`write_last_exit`] answers **"how did it end?"** after the pull is gone.
//!   Written on *every* exit path — success, error, and panic — because the one
//!   exit that most needs explaining is the one nobody planned for.
//!
//! # Neither is a result
//!
//! These are operational telemetry, not research records. `inventory.jsonl`
//! remains the append-only archive of what was acquired (D-0074's shape);
//! these two are overwritten freely and carry no provenance. Nothing downstream
//! may read a number out of them into a result.

use std::io::Write;
use std::path::{Path, PathBuf};

/// How often [`write_heartbeat`] is worth calling, in records.
///
/// Small enough that a stalled pull is obvious within a minute or two at
/// observed throughput (~1.1–1.8 req/s), large enough that the write is
/// nowhere near the cost of a request.
pub const HEARTBEAT_EVERY: u64 = 25;

/// The four counts a run reports about itself.
///
/// Grouped rather than passed loose because `refused` only means something
/// beside `attempted`: 794 is a catastrophe out of 800 and a rounding error out
/// of 83,489, and a signature that lets a caller supply one without the other
/// invites exactly that mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Requests attempted this run.
    pub attempted: u64,
    /// Requests that produced a Parquet file.
    pub written: u64,
    /// Requests the vendor answered 472 for, inventoried as empty.
    pub empty: u64,
    /// Requests refused — the count the inventory deliberately does not carry.
    pub refused: u64,
}

/// The archive-wide directory both telemetry files live in.
///
/// **At the archive root, and not under `external/thetadata/`** (D-0098). A
/// heartbeat answers "is the machine still working?", which is not a question
/// about a vendor: the next Databento tranche wants the same file at the same
/// path. Per-vendor placement was considered and rejected — it would mean a
/// second copy of the same idea the first time anything else runs unattended,
/// and an operator checking on a run would have to know which vendor was
/// running in order to find out whether anything was.
///
/// It sits beside `staging/` and `delivery/`, which set the precedent for
/// operational non-data at the root: not depth-checked, and never in
/// `manifest.jsonl`, because none of the three is an acquisition.
pub const TELEMETRY_DIR: &str = "telemetry";

/// Path of the heartbeat file, under `{data_dir}/telemetry/`.
#[must_use]
pub fn heartbeat_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TELEMETRY_DIR).join("heartbeat.txt")
}

/// Path of the exit record, under `{data_dir}/telemetry/`.
#[must_use]
pub fn last_exit_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TELEMETRY_DIR).join("last_exit.json")
}

/// Overwrites the heartbeat with a timestamp and the counts so far.
///
/// Best-effort by design: a heartbeat that could fail a run would be a
/// monitoring device with the power to break the thing it monitors. Errors are
/// swallowed and the caller is not told, because there is nothing useful it
/// could do — the pull's job is to acquire data, not to report on itself.
///
/// Written whole, with a trailing newline, so a `stat`-sized read never sees a
/// half-written line.
pub fn write_heartbeat(data_dir: &Path, now_ts: i64, c: Counts) {
    let Counts {
        attempted,
        written,
        empty,
        refused,
    } = c;
    let body = format!(
        "ts={now_ts}\nattempted={attempted}\nwritten={written}\nempty={empty}\nrefused={refused}\n"
    );
    let path = heartbeat_path(data_dir);
    // The directory has to exist or the write silently does nothing — and
    // "silently does nothing" is the failure this whole module was added to
    // stop. Created here rather than assumed, because a fresh archive has no
    // `telemetry/` until the first unattended run wants one.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, body);
}

/// Why a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    /// Every outstanding request was attempted.
    Completed,
    /// Stopped early on its own terms — the refusal-rate breaker, a disk gate.
    Halted,
    /// A fatal error propagated out.
    Failed,
    /// The process panicked.
    Panicked,
}

impl ExitKind {
    /// The spelling that goes in the file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ExitKind::Completed => "completed",
            ExitKind::Halted => "halted",
            ExitKind::Failed => "failed",
            ExitKind::Panicked => "panicked",
        }
    }
}

/// Writes the exit record, replacing any previous one.
///
/// JSON so it is machine-readable, one object, no array — this is a snapshot of
/// the last run rather than a log. `reason` is free text and may be empty for a
/// clean completion; `refused` is the count that the inventory, by design, does
/// **not** carry.
///
/// Best-effort for the same reason as the heartbeat, with one exception: the
/// caller gets a `bool` back so a *test* can assert the write happened. Nothing
/// in the run path branches on it.
pub fn write_last_exit(
    data_dir: &Path,
    now_ts: i64,
    kind: ExitKind,
    reason: &str,
    c: Counts,
) -> bool {
    let Counts {
        attempted,
        written,
        empty,
        refused,
    } = c;
    let escaped = reason
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ");
    let body = format!(
        "{{\"schema_version\":1,\"ts\":{now_ts},\"kind\":\"{}\",\"reason\":\"{escaped}\",\
         \"attempted\":{attempted},\"written\":{written},\"empty\":{empty},\"refused\":{refused}}}\n",
        kind.as_str()
    );
    let path = last_exit_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::File::create(path) {
        Ok(mut f) => f.write_all(body.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests;
