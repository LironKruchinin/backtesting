//! Buying the plan: submit → poll → download → verify → place → record.
//!
//! Everything before this module is free and reversible. Everything in it is
//! not: `submit` is the one irreversible act in the crate, and the vendor
//! offers no idempotency key, no cancel, and no refund. The whole module is
//! shaped around making a second submission of the same window impossible
//! rather than merely unlikely.
//!
//! ## Four guards, in order of when they fire
//!
//! 1. **One pull at a time.** [`execute`] takes an exclusive OS lock on
//!    `pull.lock` and holds it throughout. The
//!    fold→reconcile→intend→submit sequence spans network calls, so without
//!    the lock two processes started seconds apart would each read an
//!    identical journal, each see nothing submitted vendor-side, and each
//!    submit. Locking the journal's *appends* cannot help: the race is
//!    between the submissions, not between the writes.
//! 2. **Intent before submission.** The journal records what we are about to
//!    buy, and fsyncs it, before the network call. A crash inside `submit`
//!    then leaves evidence that a purchase may exist.
//! 3. **Reconciliation against the vendor.** Every run lists the account's
//!    recent jobs once and matches them against the plan. This runs even on a
//!    first attempt, so a deleted, truncated, or never-written journal still
//!    cannot cause a double purchase. Anything ambiguous refuses.
//! 4. **Verification before placement.** A delivered file must be exactly the
//!    length the vendor published and hash to exactly the SHA-256 it
//!    published, or it never reaches `raw/`. The catalog hashes whatever is on
//!    disk (D-0017) — so an unverified truncated download would be recorded
//!    and thereafter certified by the manifest as the real slice.
//!
//! ## Two phases, because the vendor queue is FIFO
//!
//! Submission and collection are separate passes. Jobs are processed one at a
//! time in submission order, so submitting all of them first lets the vendor
//! work through the queue while we wait, instead of idling between each
//! submit and its own download. It also makes the 20-submissions-per-minute
//! rate limit a property of one loop rather than a property scattered
//! through the run.
//!
//! ## What "resumable" means here
//!
//! A timeout is not a failure of the purchase. The job is bought, journalled,
//! and downloadable for 30 days from submission, so the remedy is always to
//! run the identical command again — [`intent_id`] is deterministic, so the
//! second run recognises the first run's work instead of repeating it. Every
//! crash point has a corresponding resume path, including the narrow window
//! between renaming a verified file into `raw/` and recording it in the
//! manifest: there the destination is re-verified against the vendor digest
//! and the run proceeds straight to the append, rather than refusing at the
//! "destination already exists" guard that exists for a different situation.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use crate::catalog::{Acquisition, Catalog, is_valid_symbol};
use crate::ingest::clock::{Clock, NANOS_PER_SEC};
use crate::ingest::delivery::{DeliveryError, DeliveryInspector, is_sha256_hex};
use crate::ingest::error::IngestError;
use crate::ingest::journal::{Journal, JournalEntry, JournalEvent, intent_id};
use crate::ingest::plan::{PlannedJob, PullPlan};
use crate::ingest::provider::{BatchProvider, JobSpec, JobState, RemoteFile, RemoteJob};
use crate::ingest::quote::QuoteReport;
use crucible_core::types::{NanoUsd, Ts};

/// Staging area for downloads, under the data dir and never inside `raw/`.
pub const STAGING_DIR: &str = "staging";

/// Where a job's support files (`condition.json`, `manifest.json`,
/// `metadata.json`) are kept after collection. They are provenance, not
/// acquisitions, so the manifest does not record them.
pub const DELIVERY_DIR: &str = "delivery";

/// Lock file guaranteeing one executing pull per archive.
pub const PULL_LOCK_FILE: &str = "pull.lock";

/// How far back reconciliation looks for jobs we may already have bought.
/// One day past the vendor's 30-day download window: anything older cannot be
/// collected anyway, and the journal covers those.
const RECONCILE_LOOKBACK_SECS: i64 = 31 * 86_400;

/// The seams the execute path reaches the outside world through.
///
/// Three, deliberately: the vendor API, the delivered bytes, and the wall
/// clock. With all three faked, the entire state machine — polling,
/// timeouts, checksum refusals, crash resume — runs offline in microseconds.
pub struct Deps<'a> {
    /// The vendor.
    pub provider: &'a mut dyn BatchProvider,
    /// Reader for delivered files.
    pub inspector: &'a dyn DeliveryInspector,
    /// Wall clock and sleeper.
    pub clock: &'a dyn Clock,
}

/// Operator-chosen knobs for one execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteOptions {
    /// Seconds between job-status polls.
    pub poll_secs: u64,
    /// Give up waiting after this many seconds and tell the operator to
    /// re-run. Nothing is lost; the jobs stay downloadable.
    pub timeout_secs: u64,
    /// Minimum seconds between submissions. The vendor caps submissions at 20
    /// per minute per IP, so 3 seconds is exactly at the limit.
    pub submit_spacing_secs: u64,
    /// Permit re-buying a window whose job expired. Off by default, because
    /// it is the only path in this module that spends money on data a
    /// previous run already paid for.
    pub repurchase_expired: bool,
}

impl Default for ExecuteOptions {
    fn default() -> ExecuteOptions {
        ExecuteOptions {
            poll_secs: 15,
            timeout_secs: 3_600,
            submit_spacing_secs: 3,
            repurchase_expired: false,
        }
    }
}

/// One window that made it into the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquiredWindow {
    /// Relative archive path now holding the bytes.
    pub file_path: String,
    /// Vendor job that delivered them.
    pub job_id: String,
    /// Manifest id of the archived bytes (D-0014).
    pub file_blake3: String,
    /// Symbols recorded: the requested key plus the raw symbols observed in
    /// the delivered DBN metadata.
    pub symbols: Vec<String>,
    /// Archived byte length.
    pub size_bytes: u64,
    /// True when the job was recovered by reconciliation rather than
    /// submitted by this run — i.e. we found a purchase already made.
    pub adopted: bool,
}

/// What an execution did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecuteReport {
    /// Windows archived by this run, in plan order.
    pub acquired: Vec<AcquiredWindow>,
    /// Windows the journal already showed as archived.
    pub already_archived: usize,
    /// Jobs submitted by this run (as opposed to adopted).
    pub submitted: usize,
    /// Jobs adopted from the vendor's job list.
    pub adopted: usize,
    /// Symbols seen in delivered metadata that the catalog would reject, and
    /// were therefore left out of the manifest record. Reported rather than
    /// swallowed: omitting a symbol only ever makes coverage *understate*
    /// what we own, so the cost is re-buying it, never silently skipping it.
    pub dropped_symbols: Vec<String>,
    /// Support files (`condition.json`, `symbology.json`, …) that could not be
    /// filed under `delivery/`, one line each. Never fatal — the payload is
    /// already archived by the time these move — but never silent either:
    /// `condition.json` is the data-QA report's input, and an operator who is
    /// not told it went missing finds out a milestone later.
    pub support_file_warnings: Vec<String>,
}

impl ExecuteReport {
    /// True when nothing was bought or collected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.acquired.is_empty()
    }
}

impl core::fmt::Display for ExecuteReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(
            f,
            "submitted {} job(s), adopted {}, already archived {}",
            self.submitted, self.adopted, self.already_archived
        )?;
        for w in &self.acquired {
            writeln!(
                f,
                "  {} bytes  {}  job {}{}",
                w.size_bytes,
                w.file_path,
                w.job_id,
                if w.adopted { " (adopted)" } else { "" }
            )?;
            writeln!(f, "      manifest id {}", w.file_blake3)?;
            writeln!(f, "      symbols: {}", w.symbols.join(" "))?;
        }
        if !self.dropped_symbols.is_empty() {
            writeln!(
                f,
                "  WARNING: {} symbol(s) in the delivered metadata are not \
                 recordable and were omitted: {}",
                self.dropped_symbols.len(),
                self.dropped_symbols.join(" ")
            )?;
            writeln!(
                f,
                "           coverage for those symbols will read as missing, \
                 so they would be bought again"
            )?;
        }
        for warning in &self.support_file_warnings {
            writeln!(f, "  WARNING: {warning}")?;
        }
        Ok(())
    }
}

/// Buys and archives every window in `plan`.
///
/// Call only after [`gate`](crate::ingest::quote::gate) has authorised the
/// quoted total: this function does not re-check the budget, it spends it.
///
/// # Errors
/// [`IngestError`] for any refusal or failure. Windows already archived
/// before the failure stay archived and journalled — re-running resumes from
/// there and submits nothing twice.
pub fn execute(
    catalog: &mut Catalog,
    deps: &mut Deps<'_>,
    plan: &PullPlan,
    quote: &QuoteReport,
    opts: &ExecuteOptions,
) -> Result<ExecuteReport, IngestError> {
    let data_dir = catalog.data_dir().to_path_buf();
    let _lock = PullLock::acquire(&data_dir)?;
    let mut journal = Journal::open(&data_dir)?;
    let mut report = ExecuteReport::default();

    // Phase A: make sure every window has a job, without ever creating two.
    let assignments = ensure_jobs(&mut journal, deps, plan, quote, opts, &mut report)?;

    // Phase B: collect them, in plan order. The vendor's queue is FIFO, so
    // this is also completion order.
    for assignment in &assignments {
        let Some(job_id) = &assignment.job_id else {
            continue;
        };
        let acquired = collect_window(
            catalog,
            &mut journal,
            deps,
            plan,
            quote,
            &assignment.job,
            &assignment.intent_id,
            job_id,
            opts,
            &mut report,
        )?;
        if let Some(window) = acquired {
            report.acquired.push(window);
        }
    }

    report.dropped_symbols.sort_unstable();
    report.dropped_symbols.dedup();
    Ok(report)
}

/// A planned window paired with the job that will deliver it.
struct Assignment {
    job: PlannedJob,
    intent_id: String,
    /// `None` only when the window was already archived.
    job_id: Option<String>,
}

/// Phase A: for every window, find or create exactly one vendor job.
fn ensure_jobs(
    journal: &mut Journal,
    deps: &mut Deps<'_>,
    plan: &PullPlan,
    quote: &QuoteReport,
    opts: &ExecuteOptions,
    report: &mut ExecuteReport,
) -> Result<Vec<Assignment>, IngestError> {
    let since = Ts(deps.clock.now_ts().0 - RECONCILE_LOOKBACK_SECS * NANOS_PER_SEC);
    let listed = deps
        .provider
        .list_jobs(since)
        .map_err(|source| IngestError::Provider {
            during: "listing existing batch jobs to avoid buying a window twice",
            source,
        })?;

    let mut claimed: BTreeSet<String> = journal.claimed_job_ids();
    let mut out = Vec::with_capacity(plan.jobs.len());
    let mut submitted_any = false;

    for job in &plan.jobs {
        let id = intent_id(
            &plan.dataset,
            &plan.schema,
            &job.key,
            plan.stype_in,
            job.window,
        );
        let state = journal.state_of(&id);

        if state.is_complete() {
            report.already_archived += 1;
            out.push(Assignment {
                job: job.clone(),
                intent_id: id,
                job_id: None,
            });
            continue;
        }

        if let Some(existing) = state.job_id.clone() {
            claimed.insert(existing.clone());
            out.push(Assignment {
                job: job.clone(),
                intent_id: id,
                job_id: Some(existing),
            });
            continue;
        }

        // No job recorded for this intent. Before spending anything, ask the
        // vendor whether one already exists — this is what protects a run
        // whose journal was lost or whose predecessor died mid-`submit`.
        if let Some(found) = reconcile(&listed, plan, job, &claimed)? {
            claimed.insert(found.clone());
            journal.append(JournalEntry::new(
                &id,
                deps.clock.now_ts(),
                JournalEvent::Submitted {
                    job_id: found.clone(),
                    adopted: true,
                },
            ))?;
            report.adopted += 1;
            out.push(Assignment {
                job: job.clone(),
                intent_id: id,
                job_id: Some(found),
            });
            continue;
        }

        // Nothing exists. Record the intent and fsync it, then buy.
        journal.append(JournalEntry::new(
            &id,
            deps.clock.now_ts(),
            JournalEvent::Intended {
                dataset: plan.dataset.clone(),
                schema: plan.schema.clone(),
                key: job.key.clone(),
                stype_in: plan.stype_in.to_string(),
                start_ts: job.window.start_ts(),
                end_ts: job.window.end_ts(),
                target_file_path: job.target_file_path.clone(),
                quoted_cost_nano_usd: quoted_cost(quote, &job.target_file_path),
            },
        ))?;

        if submitted_any && opts.submit_spacing_secs > 0 {
            // The vendor accepts 20 submissions per minute per IP.
            deps.clock.sleep_secs(opts.submit_spacing_secs);
        }
        let spec = JobSpec {
            query: job.quote_query(&plan.dataset, &plan.schema, plan.stype_in),
            target_file_path: job.target_file_path.clone(),
        };
        let submitted =
            deps.provider
                .submit(&spec)
                .map_err(|source| IngestError::SubmitFailed {
                    target_file_path: job.target_file_path.clone(),
                    source,
                })?;
        submitted_any = true;

        journal.append(JournalEntry::new(
            &id,
            deps.clock.now_ts(),
            JournalEvent::Submitted {
                job_id: submitted.job_id.clone(),
                adopted: false,
            },
        ))?;
        claimed.insert(submitted.job_id.clone());
        report.submitted += 1;
        out.push(Assignment {
            job: job.clone(),
            intent_id: id,
            job_id: Some(submitted.job_id),
        });
    }

    Ok(out)
}

/// The quoted cost of one window, for the journal record. Absent quotes
/// record as zero rather than failing — the gate has already run, and a
/// missing display figure must not stop a purchase that was authorised.
fn quoted_cost(quote: &QuoteReport, target_file_path: &str) -> NanoUsd {
    quote
        .quotes
        .iter()
        .find(|q| q.job.target_file_path == target_file_path)
        .map_or(0, |q| q.cost_nano_usd)
}

/// Finds the one vendor job that already covers `job`, if there is one.
///
/// Matching is on what decides *which bytes were bought*: dataset, schema,
/// symbol set, symbology, and the window's civil start and end dates. Dates
/// rather than raw nanoseconds because a vendor is entitled to normalise a
/// midnight boundary, and a normalisation that broke the match would cause
/// the exact double purchase this exists to prevent.
///
/// # Errors
/// [`IngestError::AmbiguousReconciliation`] when more than one candidate
/// matches, or when the single candidate cannot be confirmed on symbology.
fn reconcile(
    listed: &[RemoteJob],
    plan: &PullPlan,
    job: &PlannedJob,
    claimed: &BTreeSet<String>,
) -> Result<Option<String>, IngestError> {
    let wanted_symbols = vec![job.key.clone()];
    let start = crate::ingest::window::date_of(job.window.start_ts());
    let end = crate::ingest::window::date_of(job.window.end_ts());

    let mut confirmed = Vec::new();
    let mut unconfirmed = Vec::new();
    for candidate in listed {
        if claimed.contains(&candidate.job_id)
            || candidate.dataset != plan.dataset
            || candidate.schema != plan.schema
            || sorted(&candidate.symbols) != sorted(&wanted_symbols)
            || crate::ingest::window::date_of(candidate.range.start_ts()) != start
            || crate::ingest::window::date_of(candidate.range.end_ts()) != end
        {
            continue;
        }
        match candidate.stype_in {
            Some(s) if s == plan.stype_in => confirmed.push(candidate.job_id.clone()),
            // A symbology we cannot read is not a mismatch and not a match.
            // It is an ambiguity, and ambiguities refuse.
            None => unconfirmed.push(candidate.job_id.clone()),
            Some(_) => {}
        }
    }

    let mut all: Vec<String> = confirmed
        .iter()
        .chain(unconfirmed.iter())
        .cloned()
        .collect();
    all.sort_unstable();

    match (confirmed.len(), unconfirmed.len()) {
        (0, 0) => Ok(None),
        (1, 0) => Ok(confirmed.into_iter().next()),
        _ => Err(IngestError::AmbiguousReconciliation {
            target_file_path: job.target_file_path.clone(),
            job_ids: all,
        }),
    }
}

fn sorted(items: &[String]) -> Vec<String> {
    let mut out = items.to_vec();
    out.sort_unstable();
    out.dedup();
    out
}

/// Phase B for one window: wait for the job, collect it, archive it.
#[expect(
    clippy::too_many_arguments,
    reason = "every argument is a distinct collaborator or identity; bundling \
              them into a struct would only move the list"
)]
fn collect_window(
    catalog: &mut Catalog,
    journal: &mut Journal,
    deps: &mut Deps<'_>,
    plan: &PullPlan,
    quote: &QuoteReport,
    job: &PlannedJob,
    intent: &str,
    job_id: &str,
    opts: &ExecuteOptions,
    report: &mut ExecuteReport,
) -> Result<Option<AcquiredWindow>, IngestError> {
    let state = journal.state_of(intent);
    if state.is_complete() {
        return Ok(None);
    }

    let adopted = journal.entries().iter().any(|e| {
        e.intent_id == intent && matches!(&e.event, JournalEvent::Submitted { adopted: true, .. })
    });

    let mut job_id = job_id.to_string();
    match wait_for_job(deps, job, &job_id, opts) {
        Ok(()) => {}
        // The only path in this module that buys data a previous run already
        // paid for, and it exists only because the operator said so on the
        // command line. `Abandoned` is what releases the intent; nothing
        // writes it implicitly.
        Err(IngestError::JobExpired { .. }) if opts.repurchase_expired => {
            job_id = repurchase(deps, journal, plan, quote, job, intent, &job_id, report)?;
            wait_for_job(deps, job, &job_id, opts)?;
        }
        Err(other) => return Err(other),
    }
    let job_id = job_id.as_str();

    let files = deps
        .provider
        .list_files(job_id)
        .map_err(|source| IngestError::Provider {
            during: "listing the files of a completed job",
            source,
        })?;
    let data_file = exactly_one_data_file(&files, job, job_id)?;
    if !is_sha256_hex(&data_file.sha256_hex) {
        return Err(IngestError::Delivery(DeliveryError::MalformedDigest {
            digest: data_file.sha256_hex.clone(),
        }));
    }

    let data_dir = catalog.data_dir().to_path_buf();
    let dest = data_dir.join(&job.target_file_path);
    let staging = data_dir.join(STAGING_DIR).join(intent);

    let observed = if dest.is_file() {
        if state.staged_file_name.is_none() {
            // A fresh intent must never overwrite the archive. Raw is
            // immutable (D-0017), and `fs::rename` replaces silently on
            // Windows.
            return Err(IngestError::DestinationOccupied {
                file_path: job.target_file_path.clone(),
            });
        }
        // Resuming: a previous run renamed the verified payload into place
        // and died before recording it. Re-verify the bytes that are there —
        // a match means finish the append, a mismatch means corruption, and
        // neither means "refuse because the file exists".
        deps.inspector.verify_sha256(&dest, &data_file.sha256_hex)?;
        deps.inspector.observed_symbols(&dest)?
    } else {
        download_and_verify(deps, journal, intent, job_id, &data_file, &staging)?;
        let staged = staging.join(&data_file.filename);
        // Read the symbols out of the delivery *before* it moves: the archive
        // renames it to the window's name, and an undecodable payload should
        // refuse while it is still outside `raw/`.
        let observed = deps.inspector.observed_symbols(&staged)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|source| IngestError::Io {
                during: "creating the archive directory",
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::rename(&staged, &dest).map_err(|source| IngestError::Io {
            during: "moving the verified payload into the archive",
            path: dest.clone(),
            source,
        })?;
        observed
    };

    let (symbols, dropped) = merge_symbols(&job.key, &observed);
    report.dropped_symbols.extend(dropped);

    let record = catalog.append(Acquisition {
        dataset: plan.dataset.clone(),
        schema: plan.schema.clone(),
        symbols: symbols.clone(),
        range: job.window,
        acquired_ts: deps.clock.now_ts(),
        databento_job_id: job_id.to_string(),
        file_path: job.target_file_path.clone(),
    })?;

    journal.append(JournalEntry::new(
        intent,
        deps.clock.now_ts(),
        JournalEvent::Appended {
            job_id: job_id.to_string(),
            file_path: record.file_path.clone(),
            file_blake3: record.file_blake3.clone(),
        },
    ))?;

    report.support_file_warnings.extend(keep_support_files(
        &data_dir,
        &staging,
        job_id,
        &data_file.filename,
    ));

    Ok(Some(AcquiredWindow {
        file_path: record.file_path,
        job_id: job_id.to_string(),
        file_blake3: record.file_blake3,
        symbols,
        size_bytes: record.size_bytes,
        adopted,
    }))
}

/// Gives up on an expired job and buys the window again.
///
/// Only reachable with `--repurchase-expired`. The `Abandoned` record is what
/// releases the intent — without it the reconciler would keep finding the
/// dead job and the window could never be re-bought, which is the correct
/// default.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors collect_window's collaborators; a bundle struct would \
              only relocate the list"
)]
fn repurchase(
    deps: &mut Deps<'_>,
    journal: &mut Journal,
    plan: &PullPlan,
    quote: &QuoteReport,
    job: &PlannedJob,
    intent: &str,
    expired_job_id: &str,
    report: &mut ExecuteReport,
) -> Result<String, IngestError> {
    journal.append(JournalEntry::new(
        intent,
        deps.clock.now_ts(),
        JournalEvent::Abandoned {
            job_id: Some(expired_job_id.to_string()),
            reason: "download window closed; re-purchase authorised by --repurchase-expired"
                .to_string(),
        },
    ))?;
    journal.append(JournalEntry::new(
        intent,
        deps.clock.now_ts(),
        JournalEvent::Intended {
            dataset: plan.dataset.clone(),
            schema: plan.schema.clone(),
            key: job.key.clone(),
            stype_in: plan.stype_in.to_string(),
            start_ts: job.window.start_ts(),
            end_ts: job.window.end_ts(),
            target_file_path: job.target_file_path.clone(),
            quoted_cost_nano_usd: quoted_cost(quote, &job.target_file_path),
        },
    ))?;
    let spec = JobSpec {
        query: job.quote_query(&plan.dataset, &plan.schema, plan.stype_in),
        target_file_path: job.target_file_path.clone(),
    };
    let submitted = deps
        .provider
        .submit(&spec)
        .map_err(|source| IngestError::SubmitFailed {
            target_file_path: job.target_file_path.clone(),
            source,
        })?;
    journal.append(JournalEntry::new(
        intent,
        deps.clock.now_ts(),
        JournalEvent::Submitted {
            job_id: submitted.job_id.clone(),
            adopted: false,
        },
    ))?;
    report.submitted += 1;
    Ok(submitted.job_id)
}

/// Polls until the job is downloadable, or refuses.
fn wait_for_job(
    deps: &mut Deps<'_>,
    job: &PlannedJob,
    job_id: &str,
    opts: &ExecuteOptions,
) -> Result<(), IngestError> {
    let started = deps.clock.now_ts();
    loop {
        let remote = deps
            .provider
            .job(job_id)
            .map_err(|source| IngestError::Provider {
                during: "checking a batch job's state",
                source,
            })?;
        match remote.state {
            JobState::Done => return Ok(()),
            JobState::Expired => {
                return Err(IngestError::JobExpired {
                    job_id: job_id.to_string(),
                    target_file_path: job.target_file_path.clone(),
                });
            }
            JobState::Queued | JobState::Processing => {}
        }
        let waited = elapsed_secs(started, deps.clock.now_ts());
        if waited >= opts.timeout_secs {
            return Err(IngestError::PollTimedOut {
                job_id: job_id.to_string(),
                target_file_path: job.target_file_path.clone(),
                last_state: remote.state,
                waited_secs: waited,
            });
        }
        deps.clock.sleep_secs(opts.poll_secs);
    }
}

fn elapsed_secs(from: Ts, to: Ts) -> u64 {
    let ns = (to.0 - from.0).max(0);
    u64::try_from(ns / NANOS_PER_SEC).unwrap_or(u64::MAX)
}

/// Picks the single payload out of a delivery, or refuses.
///
/// A job also delivers `condition.json`, `manifest.json`, and `metadata.json`;
/// those are provenance. Exactly one `.dbn.zst` is required because the
/// archive stores one file per window — the submission sets
/// `split_duration = none` precisely so that holds.
fn exactly_one_data_file(
    files: &[RemoteFile],
    job: &PlannedJob,
    job_id: &str,
) -> Result<RemoteFile, IngestError> {
    let data: Vec<&RemoteFile> = files.iter().filter(|f| f.is_data_file()).collect();
    if data.len() == 1 {
        return Ok(data[0].clone());
    }
    Err(IngestError::UnexpectedDelivery {
        job_id: job_id.to_string(),
        target_file_path: job.target_file_path.clone(),
        data_file_count: data.len(),
        detail: files
            .iter()
            .map(|f| f.filename.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    })
}

/// Downloads into staging and refuses anything that does not check out.
fn download_and_verify(
    deps: &mut Deps<'_>,
    journal: &mut Journal,
    intent: &str,
    job_id: &str,
    data_file: &RemoteFile,
    staging: &Path,
) -> Result<(), IngestError> {
    // A stale staging directory is a previous run's abandoned partial
    // download. It is outside `raw/`, so clearing it is safe and keeps a
    // half-written file from being mistaken for a delivery.
    if staging.exists() {
        std::fs::remove_dir_all(staging).map_err(|source| IngestError::Io {
            during: "clearing a stale staging directory",
            path: staging.to_path_buf(),
            source,
        })?;
    }
    std::fs::create_dir_all(staging).map_err(|source| IngestError::Io {
        during: "creating the staging directory",
        path: staging.to_path_buf(),
        source,
    })?;

    deps.provider
        .download(job_id, staging)
        .map_err(|source| IngestError::Provider {
            during: "downloading a completed job",
            source,
        })?;

    let staged = staging.join(&data_file.filename);
    let actual_bytes = std::fs::metadata(&staged)
        .map_err(|source| IngestError::Io {
            during: "measuring the downloaded payload",
            path: staged.clone(),
            source,
        })?
        .len();
    if actual_bytes != data_file.size_bytes {
        return Err(IngestError::DeliverySizeMismatch {
            job_id: job_id.to_string(),
            filename: data_file.filename.clone(),
            expected_bytes: data_file.size_bytes,
            actual_bytes,
        });
    }
    deps.inspector
        .verify_sha256(&staged, &data_file.sha256_hex)?;

    journal.append(JournalEntry::new(
        intent,
        deps.clock.now_ts(),
        JournalEvent::Downloaded {
            job_id: job_id.to_string(),
            staged_file_name: data_file.filename.clone(),
            size_bytes: actual_bytes,
        },
    ))?;
    Ok(())
}

/// Moves the job's support files out of staging and drops the staging dir.
///
/// Best-effort by design: the payload is already archived and recorded, and
/// failing the whole acquisition because a 200-byte `condition.json` could not
/// be filed would be the tail wagging the dog. Best-effort is not the same as
/// silent, though — `condition.json` is what the data-QA report reads to tell
/// a genuine exchange holiday from a hole in the data — so every failure comes
/// back as a line for [`ExecuteReport::support_file_warnings`]. This crate
/// never prints (CLAUDE.md §3); the CLI does.
///
/// Staging is only removed when every file made it out. A rename that failed
/// (an antivirus scanner holding the handle, a full disk) leaves the file in
/// staging, and deleting the directory afterwards would destroy the very thing
/// the warning is about.
fn keep_support_files(
    data_dir: &Path,
    staging: &Path,
    job_id: &str,
    payload_name: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !staging.is_dir() {
        return warnings;
    }
    let kept = data_dir.join(DELIVERY_DIR).join(job_id);
    match std::fs::create_dir_all(&kept) {
        Err(source) => warnings.push(format!(
            "job {job_id}: could not create {} to keep its support files: {source}",
            kept.display()
        )),
        Ok(()) => match std::fs::read_dir(staging) {
            Err(source) => warnings.push(format!(
                "job {job_id}: could not read staging {} to keep its support files: {source}",
                staging.display()
            )),
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name == std::ffi::OsStr::new(payload_name) {
                        continue;
                    }
                    if let Err(source) = std::fs::rename(entry.path(), kept.join(&name)) {
                        warnings.push(format!(
                            "job {job_id}: could not keep {}: {source}",
                            name.to_string_lossy()
                        ));
                    }
                }
            }
        },
    }
    if warnings.is_empty() {
        if let Err(source) = std::fs::remove_dir_all(staging) {
            warnings.push(format!(
                "job {job_id}: support files were kept but staging {} could not be removed: {source}",
                staging.display()
            ));
        }
    } else {
        warnings.push(format!(
            "job {job_id}: staging {} left in place so nothing above is lost",
            staging.display()
        ));
    }
    warnings
}

/// Builds the manifest's symbol list: the requested key first, then every
/// recordable raw symbol the delivery declared.
///
/// Returns the list and the symbols that had to be left out. Dropping is the
/// right failure mode here: a missing symbol makes `coverage` report data we
/// own as missing, which costs a re-purchase, whereas refusing the append
/// would strand a file that has already been paid for and correctly placed.
fn merge_symbols(key: &str, observed: &[String]) -> (Vec<String>, Vec<String>) {
    let mut kept = vec![key.to_string()];
    let mut dropped = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert(key);

    let mut extra: Vec<String> = Vec::new();
    for symbol in observed {
        if seen.contains(symbol.as_str()) {
            continue;
        }
        if is_valid_symbol(symbol) {
            extra.push(symbol.clone());
            seen.insert(symbol.as_str());
        } else {
            dropped.push(symbol.clone());
        }
    }
    extra.sort_unstable();
    extra.dedup();
    kept.extend(extra);
    dropped.sort_unstable();
    dropped.dedup();
    (kept, dropped)
}

/// Exclusive, process-wide lock over one archive's execute path.
struct PullLock {
    file: std::fs::File,
}

impl PullLock {
    fn acquire(data_dir: &Path) -> Result<PullLock, IngestError> {
        let path = data_dir.join(PULL_LOCK_FILE);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| IngestError::Io {
                during: "opening the pull lock",
                path: path.clone(),
                source,
            })?;
        match file.try_lock() {
            Ok(()) => Ok(PullLock { file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(IngestError::PullLockHeld { path }),
            Err(std::fs::TryLockError::Error(source)) => Err(IngestError::Io {
                during: "locking the pull lock",
                path,
                source,
            }),
        }
    }
}

impl Drop for PullLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CoverageRequest, TsRange};
    use crate::ingest::clock::fake::FakeClock;
    use crate::ingest::delivery::fake::FakeInspector;
    use crate::ingest::plan::{PullRequest, WindowSpan, plan, range_from_dates};
    use crate::ingest::provider::StypeIn;
    use crate::ingest::provider::fake::{FakeProvider, fake_digest, vendor_file_name};
    use crate::ingest::quote::quote;
    use crate::testutil::TempDir;

    const DATASET: &str = "GLBX.MDP3";
    const SCHEMA: &str = "ohlcv-1m";
    const KEY: &str = "ES.FUT";
    const TARGET: &str = "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst";

    fn jan() -> TsRange {
        range_from_dates((2024, 1, 1), (2024, 2, 1)).expect("valid range")
    }

    fn glbx() -> TsRange {
        range_from_dates((2010, 6, 6), (2026, 7, 24)).expect("valid range")
    }

    fn request() -> PullRequest {
        PullRequest {
            dataset: DATASET.to_string(),
            schema: SCHEMA.to_string(),
            symbols: vec![KEY.to_string()],
            stype_in: StypeIn::Parent,
            range: jan(),
            window_span: WindowSpan::Month,
        }
    }

    fn es_query() -> crate::ingest::provider::QuoteQuery {
        crate::ingest::provider::QuoteQuery {
            dataset: DATASET.to_string(),
            schema: SCHEMA.to_string(),
            symbols: vec![KEY.to_string()],
            stype_in: StypeIn::Parent,
            range: jan(),
        }
    }

    fn delivered_name() -> String {
        vendor_file_name(DATASET, SCHEMA, jan())
    }

    fn clock() -> FakeClock {
        FakeClock::new(Ts(1_700_000_000_000_000_000))
    }

    /// An inspector that accepts the delivery and reports two ES contracts.
    fn inspector() -> FakeInspector {
        FakeInspector::new().with_symbols(&delivered_name(), &["ESM4", "ESH4"])
    }

    fn opts() -> ExecuteOptions {
        ExecuteOptions {
            poll_secs: 15,
            timeout_secs: 600,
            // Zero so the happy-path tests do not narrate a sleep they do not
            // care about; the pacing test sets it explicitly.
            submit_spacing_secs: 0,
            repurchase_expired: false,
        }
    }

    /// Plans, quotes, and executes one January pull against a temp archive.
    fn run(
        dir: &TempDir,
        provider: &mut FakeProvider,
        inspector: &FakeInspector,
        clock: &FakeClock,
        opts: &ExecuteOptions,
    ) -> Result<ExecuteReport, IngestError> {
        let mut catalog = Catalog::open(dir.path()).expect("open catalog");
        let plan = plan(&catalog, provider, &request())?;
        let quoted = quote(provider, &plan)?;
        let mut deps = Deps {
            provider,
            inspector,
            clock,
        };
        execute(&mut catalog, &mut deps, &plan, &quoted, opts)
    }

    fn coverage_gaps(dir: &TempDir, symbol: &str) -> Vec<TsRange> {
        let catalog = Catalog::open(dir.path()).expect("open catalog");
        catalog
            .coverage(&CoverageRequest {
                dataset: DATASET.to_string(),
                schema: SCHEMA.to_string(),
                symbols: vec![symbol.to_string()],
                range: jan(),
            })
            .expect("coverage")
            .remove(symbol)
            .unwrap_or_default()
    }

    fn manifest_len(dir: &TempDir) -> usize {
        Catalog::open(dir.path()).expect("open").records().len()
    }

    /// Writes the journal entries a crashed predecessor would have left.
    fn seed_journal(dir: &TempDir, events: Vec<(Ts, JournalEvent)>) -> String {
        let id = intent_id(DATASET, SCHEMA, KEY, StypeIn::Parent, jan());
        let mut journal = Journal::open(dir.path()).expect("open journal");
        for (ts, event) in events {
            journal
                .append(JournalEntry::new(&id, ts, event))
                .expect("append");
        }
        id
    }

    fn intended_event() -> JournalEvent {
        JournalEvent::Intended {
            dataset: DATASET.to_string(),
            schema: SCHEMA.to_string(),
            key: KEY.to_string(),
            stype_in: "parent".to_string(),
            start_ts: jan().start_ts(),
            end_ts: jan().end_ts(),
            target_file_path: TARGET.to_string(),
            quoted_cost_nano_usd: 140_000_000,
        }
    }

    // ---------------------------------------------------------------- happy

    #[test]
    fn a_pull_archives_the_window_and_the_next_pull_is_a_no_op() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx());
        let report = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");

        assert_eq!(report.acquired.len(), 1);
        assert_eq!(report.submitted, 1);
        assert_eq!(report.adopted, 0);
        let acquired = &report.acquired[0];
        assert_eq!(acquired.file_path, TARGET);
        assert!(!acquired.adopted);

        // The bytes are where the plan said they would be.
        let dest = dir.path().join(TARGET);
        assert!(dest.is_file(), "payload must land at the planned path");
        assert_eq!(
            std::fs::read(&dest).expect("read"),
            FakeProvider::payload_for(TARGET)
        );

        // The manifest describes them, and `verify` agrees.
        assert_eq!(manifest_len(&dir), 1);
        let catalog = Catalog::open(dir.path()).expect("open");
        assert!(catalog.verify().expect("verify").is_clean());
        assert_eq!(
            catalog
                .record_by_id(&acquired.file_blake3)
                .map(|r| &r.file_path),
            Some(&TARGET.to_string())
        );

        // Coverage now owns January, so re-running plans nothing and the
        // vendor is never asked to sell it again.
        assert!(coverage_gaps(&dir, KEY).is_empty(), "January is owned");
        let again = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");
        assert!(again.is_empty(), "second pull acquires nothing");
        assert_eq!(
            provider.submit_count(),
            1,
            "a covered window must never be bought again"
        );

        // Support files are kept as provenance; staging is cleaned up.
        let intent = intent_id(DATASET, SCHEMA, KEY, StypeIn::Parent, jan());
        assert!(
            !dir.path().join(STAGING_DIR).join(&intent).exists(),
            "staging must not survive a completed acquisition"
        );
        let kept = dir.path().join(DELIVERY_DIR).join("job-1");
        assert!(kept.join("condition.json").is_file(), "condition.json kept");
        assert!(kept.join("metadata.json").is_file(), "metadata.json kept");
    }

    // The union that makes `coverage("ESH4")` truthful. Without it the
    // manifest would credit only ES.FUT and every contract would look
    // unowned forever.
    #[test]
    fn the_manifest_records_the_key_and_the_contracts_it_expanded_to() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx());
        let inspector = FakeInspector::new().with_symbols(
            &delivered_name(),
            // Deliberately unsorted, with the key repeated, one real CME spread
            // name (spaces and colons — recordable since D-0066) and one symbol
            // the catalog still rejects outright.
            &["ESM4", "ES.FUT", "ESH4", "CL:BF F0-G0-H0", "ES\tBAD"],
        );
        let report = run(&dir, &mut provider, &inspector, &clock(), &opts()).expect("execute");

        assert_eq!(
            report.acquired[0].symbols,
            vec![
                "ES.FUT".to_string(),
                "CL:BF F0-G0-H0".to_string(),
                "ESH4".to_string(),
                "ESM4".to_string()
            ],
            "requested key first, then observed contracts, sorted and deduped"
        );
        assert_eq!(report.dropped_symbols, vec!["ES\tBAD".to_string()]);
        assert!(
            report.to_string().contains("ES\tBAD"),
            "an omitted symbol must be visible, not swallowed: {report}"
        );

        // The point of the union: a contract-level query is now covered — and
        // so is the spread, which the old whitespace ban would have dropped
        // (D-0066: 21,736 symbols were lost exactly here).
        assert!(coverage_gaps(&dir, "ESH4").is_empty());
        assert!(coverage_gaps(&dir, "CL:BF F0-G0-H0").is_empty());
        assert_eq!(coverage_gaps(&dir, "ESU4"), vec![jan()], "not in this file");
    }

    // ------------------------------------------------- never buy it twice

    // The crash that costs money: the intent is recorded, the submission
    // landed vendor-side, and the process died before the id was written.
    #[test]
    fn a_submission_lost_before_it_was_recorded_is_adopted_not_repeated() {
        let dir = TempDir::new();
        seed_journal(&dir, vec![(Ts(1), intended_event())]);
        let mut provider =
            FakeProvider::new(glbx()).with_existing_job("job-already-paid", &es_query(), TARGET);

        let report = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");

        assert_eq!(provider.submit_count(), 0, "MUST NOT buy it again");
        assert_eq!(report.adopted, 1);
        assert_eq!(report.submitted, 0);
        assert_eq!(report.acquired[0].job_id, "job-already-paid");
        assert!(report.acquired[0].adopted);
        assert_eq!(manifest_len(&dir), 1);
    }

    // The same protection with no journal at all — deleted, or never written
    // because the crash beat the first fsync.
    #[test]
    fn a_lost_journal_still_finds_the_purchase_vendor_side() {
        let dir = TempDir::new();
        let mut provider =
            FakeProvider::new(glbx()).with_existing_job("job-already-paid", &es_query(), TARGET);

        let report = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");

        assert_eq!(provider.submit_count(), 0);
        assert_eq!(report.adopted, 1);
    }

    #[test]
    fn two_candidate_jobs_refuse_rather_than_guess() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-a", &es_query(), TARGET)
            .with_existing_job("job-b", &es_query(), TARGET);

        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");

        match err {
            IngestError::AmbiguousReconciliation { job_ids, .. } => {
                assert_eq!(job_ids, vec!["job-a".to_string(), "job-b".to_string()]);
            }
            other => panic!("wrong error: {other:?}"),
        }
        assert_eq!(provider.submit_count(), 0, "ambiguity must not submit");
        assert_eq!(manifest_len(&dir), 0);
    }

    // A candidate we cannot confirm is an ambiguity, not a miss: treating it
    // as "not ours" would submit, and treating it as ours could archive the
    // wrong bytes.
    #[test]
    fn a_candidate_with_unreadable_symbology_refuses() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-a", &es_query(), TARGET)
            .with_unknown_stype("job-a");

        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");
        assert!(matches!(err, IngestError::AmbiguousReconciliation { .. }));
        assert_eq!(provider.submit_count(), 0);
    }

    // A job for a different window must NOT be adopted -- the negative
    // control for the matcher, without which reconciliation would silently
    // archive February's bytes as January.
    #[test]
    fn a_job_for_another_window_is_not_adopted() {
        let dir = TempDir::new();
        let mut other = es_query();
        other.range = range_from_dates((2024, 2, 1), (2024, 3, 1)).expect("range");
        let mut provider = FakeProvider::new(glbx()).with_existing_job(
            "job-february",
            &other,
            "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-02.dbn.zst",
        );

        let report = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");
        assert_eq!(provider.submit_count(), 1, "January was genuinely unbought");
        assert_eq!(report.adopted, 0);
    }

    // Amendment to the plan, and the bug it was written to prevent: the
    // window between renaming a verified payload into raw/ and recording it.
    #[test]
    fn a_crash_between_placement_and_the_manifest_resumes_instead_of_refusing() {
        let dir = TempDir::new();
        seed_journal(
            &dir,
            vec![
                (Ts(1), intended_event()),
                (
                    Ts(2),
                    JournalEvent::Submitted {
                        job_id: "job-paid".to_string(),
                        adopted: false,
                    },
                ),
                (
                    Ts(3),
                    JournalEvent::Downloaded {
                        job_id: "job-paid".to_string(),
                        staged_file_name: delivered_name(),
                        size_bytes: FakeProvider::payload_for(TARGET).len() as u64,
                    },
                ),
            ],
        );
        // The predecessor got as far as the rename.
        let dest = dir.path().join(TARGET);
        std::fs::create_dir_all(dest.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&dest, FakeProvider::payload_for(TARGET)).expect("plant");

        let mut provider =
            FakeProvider::new(glbx()).with_existing_job("job-paid", &es_query(), TARGET);
        let report = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");

        assert_eq!(provider.submit_count(), 0, "already paid for");
        assert_eq!(report.acquired.len(), 1, "the stranded file is adopted");
        assert_eq!(manifest_len(&dir), 1);
        let catalog = Catalog::open(dir.path()).expect("open");
        assert!(catalog.verify().expect("verify").is_clean());
        assert!(coverage_gaps(&dir, KEY).is_empty());
    }

    // The same guard, doing its original job: a fresh intent must never
    // overwrite the archive, because `fs::rename` replaces silently on
    // Windows and raw is immutable (D-0017).
    #[test]
    fn an_occupied_destination_with_no_prior_attempt_refuses() {
        let dir = TempDir::new();
        let dest = dir.path().join(TARGET);
        std::fs::create_dir_all(dest.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&dest, b"someone else's bytes").expect("plant");

        let mut provider = FakeProvider::new(glbx());
        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");

        assert!(matches!(err, IngestError::DestinationOccupied { .. }));
        assert_eq!(
            std::fs::read(&dest).expect("read"),
            b"someone else's bytes",
            "the existing file must be untouched"
        );
        assert_eq!(manifest_len(&dir), 0);
    }

    // --------------------------------------------------- delivery refusals

    #[test]
    fn a_checksum_mismatch_never_reaches_the_archive() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx());
        let inspector = FakeInspector::new().with_bad_checksum(&delivered_name());

        let err = run(&dir, &mut provider, &inspector, &clock(), &opts()).expect_err("refuse");

        assert!(matches!(
            err,
            IngestError::Delivery(DeliveryError::ChecksumMismatch { .. })
        ));
        assert!(
            !dir.path().join(TARGET).exists(),
            "a file that failed verification must not be in raw/"
        );
        assert_eq!(manifest_len(&dir), 0);
    }

    #[test]
    fn a_short_download_never_reaches_the_archive() {
        let dir = TempDir::new();
        let name = delivered_name();
        // The vendor claims more bytes than it will deliver.
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-x", &es_query(), TARGET)
            .with_files(
                "job-x",
                vec![RemoteFile {
                    filename: name.clone(),
                    size_bytes: 999_999,
                    sha256_hex: fake_digest(&name),
                }],
            );

        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");

        match err {
            IngestError::DeliverySizeMismatch {
                expected_bytes,
                actual_bytes,
                ..
            } => {
                assert_eq!(expected_bytes, 999_999);
                assert_eq!(actual_bytes, FakeProvider::payload_for(TARGET).len() as u64);
            }
            other => panic!("wrong error: {other:?}"),
        }
        assert!(!dir.path().join(TARGET).exists());
        assert_eq!(manifest_len(&dir), 0);
    }

    #[test]
    fn a_delivery_that_is_not_exactly_one_payload_refuses() {
        let name = delivered_name();
        let support = RemoteFile {
            filename: "metadata.json".to_string(),
            size_bytes: 2,
            sha256_hex: fake_digest("metadata.json"),
        };

        // Zero payload files: the window resolved to nothing, or the split
        // parameters did not take.
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-x", &es_query(), TARGET)
            .with_files("job-x", vec![support.clone()]);
        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");
        assert!(matches!(
            err,
            IngestError::UnexpectedDelivery {
                data_file_count: 0,
                ..
            }
        ));

        // Two payload files: the archive stores one file per window, so this
        // must stop rather than silently archive half the data.
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-x", &es_query(), TARGET)
            .with_files(
                "job-x",
                vec![
                    RemoteFile {
                        filename: name.clone(),
                        size_bytes: 1,
                        sha256_hex: fake_digest(&name),
                    },
                    RemoteFile {
                        filename: "second.dbn.zst".to_string(),
                        size_bytes: 1,
                        sha256_hex: fake_digest("second"),
                    },
                    support,
                ],
            );
        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");
        assert!(matches!(
            err,
            IngestError::UnexpectedDelivery {
                data_file_count: 2,
                ..
            }
        ));
        assert_eq!(manifest_len(&dir), 0);
    }

    #[test]
    fn a_digest_that_is_not_a_digest_refuses_rather_than_skipping_the_check() {
        let dir = TempDir::new();
        let name = delivered_name();
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-x", &es_query(), TARGET)
            .with_files(
                "job-x",
                vec![RemoteFile {
                    filename: name,
                    size_bytes: 1,
                    sha256_hex: "not-a-digest".to_string(),
                }],
            );

        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");
        assert!(matches!(
            err,
            IngestError::Delivery(DeliveryError::MalformedDigest { .. })
        ));
    }

    // ------------------------------------------------------------- polling

    #[test]
    fn polling_waits_through_queued_and_processing() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx()).with_states(&[
            JobState::Queued,
            JobState::Queued,
            JobState::Processing,
            JobState::Done,
        ]);
        let clock = clock();
        run(&dir, &mut provider, &inspector(), &clock, &opts()).expect("execute");

        // Three non-terminal states, so three 15-second waits.
        assert_eq!(clock.slept_secs(), 45);
        assert_eq!(manifest_len(&dir), 1);
    }

    // A job that outlasts the operator's patience is not a lost purchase:
    // it is recorded, still downloadable, and resumed by re-running.
    #[test]
    fn a_slow_job_times_out_and_the_rerun_collects_it_without_rebuying() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx()).with_states(&[JobState::Processing]);
        let timeout = ExecuteOptions {
            timeout_secs: 60,
            ..opts()
        };

        let err =
            run(&dir, &mut provider, &inspector(), &clock(), &timeout).expect_err("must time out");
        match err {
            IngestError::PollTimedOut {
                last_state,
                waited_secs,
                ..
            } => {
                assert_eq!(last_state, JobState::Processing);
                assert!(waited_secs >= 60);
            }
            other => panic!("wrong error: {other:?}"),
        }
        assert_eq!(provider.submit_count(), 1);
        assert_eq!(manifest_len(&dir), 0);

        // The job finishes; the same command picks it up.
        provider.jobs.get_mut("job-1").expect("job exists").states = vec![JobState::Done];
        let report = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute");
        assert_eq!(provider.submit_count(), 1, "resume must not re-buy");
        assert_eq!(report.acquired.len(), 1);
        assert_eq!(report.submitted, 0, "the job was already recorded");
        assert_eq!(manifest_len(&dir), 1);
    }

    #[test]
    fn an_expired_job_refuses_unless_the_operator_authorises_re_buying() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx())
            .with_existing_job("job-dead", &es_query(), TARGET)
            .with_job_states("job-dead", &[JobState::Expired]);

        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");
        assert!(matches!(err, IngestError::JobExpired { .. }));
        assert_eq!(provider.submit_count(), 0, "expiry alone must not re-buy");

        // With explicit authorisation the window is bought again, exactly once.
        let allow = ExecuteOptions {
            repurchase_expired: true,
            ..opts()
        };
        let report = run(&dir, &mut provider, &inspector(), &clock(), &allow).expect("execute");
        assert_eq!(provider.submit_count(), 1);
        assert_eq!(report.acquired.len(), 1);
        assert_eq!(manifest_len(&dir), 1);
    }

    // -------------------------------------------------------------- lock

    // Two pulls started seconds apart would each fold an identical journal,
    // each reconcile against a vendor list containing nothing, and each
    // submit. The journal's own append lock cannot prevent that; the race is
    // between the submissions.
    #[test]
    fn a_second_pull_is_locked_out_while_one_is_running() {
        let dir = TempDir::new();
        let held = PullLock::acquire(dir.path()).expect("first lock");

        let mut provider = FakeProvider::new(glbx());
        let err = run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect_err("refuse");
        assert!(matches!(err, IngestError::PullLockHeld { .. }));
        assert_eq!(provider.submit_count(), 0, "a locked-out pull buys nothing");

        drop(held);
        let mut provider = FakeProvider::new(glbx());
        run(&dir, &mut provider, &inspector(), &clock(), &opts()).expect("execute after release");
        assert_eq!(provider.submit_count(), 1);
    }

    // ----------------------------------------------------------- pacing

    #[test]
    fn submissions_are_paced_under_the_vendors_rate_limit() {
        let dir = TempDir::new();
        let mut provider = FakeProvider::new(glbx());
        let paced = ExecuteOptions {
            submit_spacing_secs: 3,
            ..opts()
        };
        let clock = clock();

        let mut catalog = Catalog::open(dir.path()).expect("open");
        let mut req = request();
        req.range = range_from_dates((2024, 1, 1), (2024, 4, 1)).expect("range");
        let plan = plan(&catalog, &mut provider, &req).expect("plan");
        assert_eq!(plan.jobs.len(), 3);
        let quoted = quote(&mut provider, &plan).expect("quote");
        let inspector = FakeInspector::new();
        let mut deps = Deps {
            provider: &mut provider,
            inspector: &inspector,
            clock: &clock,
        };
        execute(&mut catalog, &mut deps, &plan, &quoted, &paced).expect("execute");

        // Three submissions, two gaps between them: 20/minute exactly.
        assert_eq!(clock.slept_secs(), 6);
        assert_eq!(provider.submit_count(), 3);
        assert_eq!(manifest_len(&dir), 3);
    }
}
