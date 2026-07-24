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

use crate::catalog::{CatalogError, TsRange};
use crate::ingest::money::{MoneyError, format_usd};
use crate::ingest::provider::ProviderError;
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
        }
    }
}

impl std::error::Error for IngestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IngestError::Catalog(e) => Some(e),
            IngestError::Money(e) => Some(e),
            IngestError::Provider { source, .. }
            | IngestError::CostQuoteUnavailable { source, .. } => Some(source),
            _ => None,
        }
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
