//! Failure modes of planning and quoting a pull.
//!
//! Hand-rolled per CLAUDE.md §5.1, in the same shape as
//! [`CatalogError`]: struct variants carrying
//! the values needed to act on the error, a manual `Display`, and `source()`
//! for wrapped causes.
//!
//! The organising principle is that **every uncertainty resolves to a
//! refusal**. There is no variant meaning "could not price this, proceeding
//! anyway": an unobtainable quote, an unreachable dataset range, and an
//! inconsistent plan all stop the run. The cost of a false refusal is a
//! re-run; the cost of a false proceed is money.

use std::path::PathBuf;

use crate::catalog::{CatalogError, TsRange};
use crate::ingest::delivery::DeliveryError;
use crate::ingest::journal::JournalError;
use crate::ingest::money::{MoneyError, format_usd};
use crate::ingest::provider::{JobState, ProviderError};
use crucible_core::types::NanoUsd;

/// Why a pull could not be planned, quoted, or authorised.
#[derive(Debug)]
pub enum IngestError {
    /// The archive catalog rejected a request or could not be read.
    Catalog(CatalogError),
    /// A money value could not be converted or parsed exactly.
    Money(MoneyError),
    /// The provider call failed. Distinguished from a *refusal* to spend:
    /// this is the vendor being unavailable, not us declining.
    Provider {
        /// What we were doing when it failed.
        during: &'static str,
        /// The underlying provider failure.
        source: ProviderError,
    },

    /// The requested range is not usable as a download window.
    InvalidWindow {
        /// Requested range, rendered.
        requested: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The requested range lies entirely outside what the provider serves.
    WindowOutsideDatasetRange {
        /// Dataset asked about.
        dataset: String,
        /// What we asked for.
        requested: TsRange,
        /// What the provider will actually serve.
        available: TsRange,
    },
    /// The planner produced windows that do not tile the gaps they came
    /// from. A bug in window splitting, caught before it is paid for.
    PlanNotTiled {
        /// Symbol key whose windows failed the check.
        key: String,
        /// What the tiling check found.
        detail: String,
    },

    /// A cost quote could not be obtained, so nothing may be bought.
    CostQuoteUnavailable {
        /// Archive path of the window that could not be priced.
        target_file_path: String,
        /// The underlying provider failure.
        source: ProviderError,
    },
    /// The quoted total exceeds the cap the operator authorised.
    CostGateRefused {
        /// What the vendor quoted, in nanodollars.
        quoted_nano_usd: NanoUsd,
        /// The authorised ceiling, in nanodollars.
        cap_nano_usd: NanoUsd,
        /// How many windows would have been bought.
        job_count: usize,
        /// Total billable bytes across those windows.
        billable_bytes: u64,
    },
    /// Execution was requested without an explicit spending cap.
    ExecuteWithoutCap,

    /// The job journal could not be read or written.
    Journal(JournalError),
    /// A delivered file did not check out.
    Delivery(DeliveryError),
    /// Filesystem failure during the execute path.
    Io {
        /// What we were doing.
        during: &'static str,
        /// Path involved.
        path: PathBuf,
        /// Underlying failure.
        source: std::io::Error,
    },

    /// Another `pull` holds the execute lock on this archive.
    PullLockHeld {
        /// Lock file whose holder must finish first.
        path: PathBuf,
    },
    /// The vendor rejected a submission.
    SubmitFailed {
        /// Window that was being bought.
        target_file_path: String,
        /// The underlying provider failure.
        source: ProviderError,
    },
    /// More than one vendor job matches a window we were about to buy.
    ///
    /// Refusing is the only safe answer: adopting the wrong one archives the
    /// wrong bytes, and submitting anyway buys a window we already own.
    AmbiguousReconciliation {
        /// Window that could not be resolved.
        target_file_path: String,
        /// Candidate job ids, sorted.
        job_ids: Vec<String>,
    },
    /// A job did not finish within the time the operator allowed.
    ///
    /// Not a failure of the purchase — the job is recorded and its output
    /// stays downloadable — so the remedy is to run the same command again.
    PollTimedOut {
        /// Vendor batch job id, already journalled.
        job_id: String,
        /// Window being waited on.
        target_file_path: String,
        /// State the job was in when we stopped waiting.
        last_state: JobState,
        /// How long we waited.
        waited_secs: u64,
    },
    /// A job's download window closed before we collected it.
    JobExpired {
        /// Vendor batch job id.
        job_id: String,
        /// Window that was lost.
        target_file_path: String,
    },
    /// The job delivered something other than exactly one payload file.
    UnexpectedDelivery {
        /// Vendor batch job id.
        job_id: String,
        /// Window being collected.
        target_file_path: String,
        /// How many `.dbn.zst` files the job listed.
        data_file_count: usize,
        /// The file names, for the operator to look at.
        detail: String,
    },
    /// A delivered file is not the length the vendor said it would be.
    DeliverySizeMismatch {
        /// Vendor batch job id.
        job_id: String,
        /// Delivered file name.
        filename: String,
        /// Length the vendor published.
        expected_bytes: u64,
        /// Length actually on disk.
        actual_bytes: u64,
    },
    /// A file already occupies the archive path this window would take.
    DestinationOccupied {
        /// Relative archive path.
        file_path: String,
    },
}

impl core::fmt::Display for IngestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IngestError::Catalog(e) => write!(f, "archive catalog: {e}"),
            IngestError::Money(e) => write!(f, "money conversion: {e}"),
            IngestError::Provider { during, source } => {
                write!(f, "provider failed while {during}: {source}")
            }
            IngestError::InvalidWindow { requested, reason } => {
                write!(f, "unusable download window {requested}: {reason}")
            }
            IngestError::WindowOutsideDatasetRange {
                dataset,
                requested,
                available,
            } => write!(
                f,
                "requested range [{}, {}) lies outside what {dataset} serves \
                 [{}, {}) — nothing to download",
                requested.start_ts().0,
                requested.end_ts().0,
                available.start_ts().0,
                available.end_ts().0
            ),
            IngestError::PlanNotTiled { key, detail } => write!(
                f,
                "INTERNAL: planned windows for {key} do not tile the coverage \
                 gaps ({detail}); refusing to quote a plan that may double-buy \
                 or skip data"
            ),
            IngestError::CostQuoteUnavailable {
                target_file_path,
                source,
            } => write!(
                f,
                "could not price {target_file_path}: {source}. Refusing to \
                 proceed — an unpriced download is not a free one"
            ),
            IngestError::CostGateRefused {
                quoted_nano_usd,
                cap_nano_usd,
                job_count,
                billable_bytes,
            } => write!(
                f,
                "refusing to spend: quoted {} for {job_count} window(s) \
                 ({billable_bytes} billable bytes) exceeds --max-cost-usd {}. \
                 Nothing was submitted and nothing was downloaded",
                format_usd(*quoted_nano_usd),
                format_usd(*cap_nano_usd)
            ),
            IngestError::ExecuteWithoutCap => write!(
                f,
                "--execute requires an explicit --max-cost-usd. A default \
                 spending cap is a number nobody chose"
            ),

            IngestError::Journal(e) => write!(f, "job journal: {e}"),
            IngestError::Delivery(e) => write!(f, "delivered file: {e}"),
            IngestError::Io {
                during,
                path,
                source,
            } => write!(f, "while {during} at {}: {source}", path.display()),

            IngestError::PullLockHeld { path } => write!(
                f,
                "another pull is running against this archive (lock: {}). \
                 Two pulls that plan the same window would each find nothing \
                 submitted and each submit it — wait for the other to finish",
                path.display()
            ),
            IngestError::SubmitFailed {
                target_file_path,
                source,
            } => write!(
                f,
                "submitting {target_file_path} failed: {source}. The intent is \
                 journalled; re-run to reconcile it against the vendor's job \
                 list before anything is submitted again"
            ),
            IngestError::AmbiguousReconciliation {
                target_file_path,
                job_ids,
            } => write!(
                f,
                "cannot tell which vendor job covers {target_file_path}: {} \
                 candidates ({}). Refusing — adopting the wrong one archives \
                 the wrong bytes, and submitting buys a window twice. Inspect \
                 them in the Databento portal and record the right one",
                job_ids.len(),
                job_ids.join(", ")
            ),
            IngestError::PollTimedOut {
                job_id,
                target_file_path,
                last_state,
                waited_secs,
            } => write!(
                f,
                "job {job_id} for {target_file_path} was still {last_state} \
                 after {waited_secs}s. It is paid for, recorded, and stays \
                 downloadable for 30 days from submission — re-run the same \
                 command to resume; nothing will be submitted again"
            ),
            IngestError::JobExpired {
                job_id,
                target_file_path,
            } => write!(
                f,
                "job {job_id} for {target_file_path} has expired and can no \
                 longer be downloaded. Re-buying it costs money, so it needs \
                 --repurchase-expired to say so explicitly"
            ),
            IngestError::UnexpectedDelivery {
                job_id,
                target_file_path,
                data_file_count,
                detail,
            } => write!(
                f,
                "job {job_id} delivered {data_file_count} payload file(s) for \
                 {target_file_path}, expected exactly 1 ({detail}). The \
                 archive stores one file per window; nothing was placed. The \
                 job stays downloadable, so this is recoverable"
            ),
            IngestError::DeliverySizeMismatch {
                job_id,
                filename,
                expected_bytes,
                actual_bytes,
            } => write!(
                f,
                "{filename} from job {job_id} is {actual_bytes} bytes; the \
                 vendor said {expected_bytes}. A short read that reached the \
                 archive would be hashed and then certified by the manifest \
                 as the real thing — refusing"
            ),
            IngestError::DestinationOccupied { file_path } => write!(
                f,
                "{file_path} already exists in the archive. Raw is immutable \
                 (D-0017): corrections are new acquisitions at new paths, \
                 never overwrites"
            ),
        }
    }
}

impl std::error::Error for IngestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IngestError::Catalog(e) => Some(e),
            IngestError::Money(e) => Some(e),
            IngestError::Journal(e) => Some(e),
            IngestError::Delivery(e) => Some(e),
            IngestError::Io { source, .. } => Some(source),
            IngestError::Provider { source, .. }
            | IngestError::CostQuoteUnavailable { source, .. }
            | IngestError::SubmitFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<JournalError> for IngestError {
    fn from(e: JournalError) -> IngestError {
        IngestError::Journal(e)
    }
}

impl From<DeliveryError> for IngestError {
    fn from(e: DeliveryError) -> IngestError {
        IngestError::Delivery(e)
    }
}

impl From<CatalogError> for IngestError {
    fn from(e: CatalogError) -> IngestError {
        IngestError::Catalog(e)
    }
}

impl From<MoneyError> for IngestError {
    fn from(e: MoneyError) -> IngestError {
        IngestError::Money(e)
    }
}
