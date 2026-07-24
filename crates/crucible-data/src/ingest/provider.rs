//! The vendor seam: a **sync** interface to a batch market-data provider.
//!
//! Everything above this trait — planning, coverage subtraction, cost
//! quoting, the spending gate — is ordinary sync code with no network and no
//! `async`. The one implementation that talks to Databento lives behind the
//! non-default `databento` cargo feature and owns a private current-thread
//! tokio runtime, so `.await` appears in exactly one file in the workspace
//! (D-0025).
//!
//! Two properties of this trait are load-bearing and easy to erode:
//!
//! - **It never mentions a `databento::` type.** If it did, every test fake
//!   and every downstream crate would inherit the async client's dependency
//!   graph, which is the containment this seam exists to provide.
//! - **[`BatchProvider::submit`] is declared here even though the quote path
//!   never calls it.** That is deliberate. It is what makes
//!   "a dry run submits nothing" an assertion about observable behaviour
//!   rather than a statement about code that does not exist yet, so the
//!   invariant is already guarded on the day someone writes the execute path.
//!
//! Methods take `&mut self` to match the real client's own subclient
//! accessors, which keeps the implementation free of interior mutability.

use crate::catalog::TsRange;
use crucible_core::types::NanoUsd;

/// Input symbology for a request — how the `symbols` list should be read.
///
/// These render to Databento's wire spellings; do not invent others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StypeIn {
    /// Raw exchange contract symbols, e.g. `ESU6`.
    RawSymbol,
    /// Parent symbology, e.g. `ES.FUT` — expands to every listed contract.
    Parent,
    /// Vendor continuous series, e.g. `ES.v.0`.
    ///
    /// Accepted by the type because the vendor supports it, but the archive
    /// never stores pre-stitched continuous data: roll assumptions are
    /// constructed locally so they stay ours and stay explicit (see the
    /// `ingest` module docs).
    Continuous,
}

impl core::fmt::Display for StypeIn {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            StypeIn::RawSymbol => "raw_symbol",
            StypeIn::Parent => "parent",
            StypeIn::Continuous => "continuous",
        };
        f.write_str(s)
    }
}

/// A single priced request: what we would ask the vendor for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteQuery {
    /// Vendor dataset, e.g. `GLBX.MDP3`.
    pub dataset: String,
    /// Vendor schema, e.g. `ohlcv-1m`.
    pub schema: String,
    /// Symbols to request, read according to `stype_in`.
    pub symbols: Vec<String>,
    /// How to interpret `symbols`.
    pub stype_in: StypeIn,
    /// Half-open range being requested.
    pub range: TsRange,
}

/// A batch job we would submit. Distinct from [`QuoteQuery`] because
/// submitting costs money and quoting does not, and the type system should
/// not let one be mistaken for the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    /// What to download.
    pub query: QuoteQuery,
    /// Relative archive path the payload is destined for, for provenance.
    pub target_file_path: String,
}

/// Identifier returned by the vendor for an accepted batch job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedJob {
    /// Vendor batch job id, recorded in the manifest for re-download audit.
    pub job_id: String,
}

/// Why a provider call failed.
///
/// Deliberately coarse and deliberately free of vendor types. The mapper
/// from the real client copies status and the server's message only — never
/// the request URL or headers, so API key material cannot reach a `Display`.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderError {
    /// Credentials rejected (HTTP 401).
    Unauthorized,
    /// Authenticated but not permitted this data (HTTP 403).
    Forbidden {
        /// Server-supplied explanation, key material already stripped.
        detail: String,
    },
    /// Throttled (HTTP 429).
    RateLimited {
        /// Server-suggested wait, when supplied.
        retry_after_secs: Option<u64>,
    },
    /// Network or TLS failure.
    Transport {
        /// Explanation, key material already stripped.
        detail: String,
    },
    /// The server answered, but not with something we can use.
    BadResponse {
        /// Explanation, key material already stripped.
        detail: String,
    },
}

impl core::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProviderError::Unauthorized => {
                write!(f, "provider rejected the credentials (401)")
            }
            ProviderError::Forbidden { detail } => {
                write!(f, "provider refused the request (403): {detail}")
            }
            ProviderError::RateLimited {
                retry_after_secs: Some(s),
            } => write!(f, "provider rate-limited the request; retry after {s}s"),
            ProviderError::RateLimited {
                retry_after_secs: None,
            } => write!(f, "provider rate-limited the request"),
            ProviderError::Transport { detail } => write!(f, "transport failure: {detail}"),
            ProviderError::BadResponse { detail } => write!(f, "unusable response: {detail}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// A batch market-data provider, as the sync world sees it.
pub trait BatchProvider {
    /// Range the provider will actually serve for `dataset`, given the
    /// account's entitlements.
    ///
    /// Planning intersects every requested window with this, so ranges are
    /// clipped from live data rather than from hardcoded constants that
    /// silently rot. It is also how a lapsed subscription becomes visible
    /// before a download rather than after.
    ///
    /// # Errors
    /// [`ProviderError`] if the range cannot be determined. Planning treats
    /// that as a refusal — it never falls back to an assumed range.
    fn dataset_range(&mut self, dataset: &str) -> Result<TsRange, ProviderError>;

    /// Billable uncompressed size, in bytes, for a request. Free to call.
    ///
    /// # Errors
    /// [`ProviderError`] if the size cannot be determined.
    fn billable_size(&mut self, query: &QuoteQuery) -> Result<u64, ProviderError>;

    /// Price of a request, already converted to exact nanodollars. Free to
    /// call, and returns `0` when the account's entitlement covers it.
    ///
    /// # Errors
    /// [`ProviderError`] if the cost cannot be determined. An unavailable
    /// quote is a refusal, never an assumption of zero.
    fn cost(&mut self, query: &QuoteQuery) -> Result<NanoUsd, ProviderError>;

    /// List price in USD per gigabyte for `schema`, ignoring entitlements.
    ///
    /// Compared against [`BatchProvider::cost`] to detect whether a flat-rate
    /// entitlement is active: when the metered estimate and the quoted cost
    /// agree we are paying per byte, and when the quote collapses to zero the
    /// subscription is live. That comparison is the cheapest available check
    /// that a subscription is actually in force before a large download.
    ///
    /// # Errors
    /// [`ProviderError`] if unit prices cannot be fetched.
    fn unit_price_usd_per_gb(&mut self, dataset: &str, schema: &str) -> Result<f64, ProviderError>;

    /// Submits a batch job. **This spends money.**
    ///
    /// Nothing on the quote path calls this; it is declared so that
    /// "a dry run submits nothing" is testable as observable behaviour.
    ///
    /// # Errors
    /// [`ProviderError`] if the job is rejected.
    fn submit(&mut self, spec: &JobSpec) -> Result<SubmittedJob, ProviderError>;
}

#[cfg(test)]
pub(crate) mod fake {
    //! A scripted [`BatchProvider`] for tests.
    //!
    //! The important member is `calls`: an ordered log of every method
    //! invoked. Money tests assert against it directly — "no `submit` appears
    //! in the log" is a property you can check, whereas "the code looks like
    //! it does not submit" is not.

    use super::{BatchProvider, JobSpec, ProviderError, QuoteQuery, SubmittedJob, TsRange};
    use crucible_core::types::NanoUsd;

    /// Canonical key for a scripted response, so a test can pin a cost to a
    /// specific `(schema, key, window)` without depending on field order.
    pub(crate) fn query_key(q: &QuoteQuery) -> String {
        format!(
            "{}|{}|{}|{}|{}..{}",
            q.dataset,
            q.schema,
            q.symbols.join(","),
            q.stype_in,
            q.range.start_ts().0,
            q.range.end_ts().0
        )
    }

    /// Scripted provider. Unscripted quote lookups fall back to `default_*`.
    pub(crate) struct FakeProvider {
        /// Ordered log of method calls, as `"method:detail"`.
        pub calls: Vec<String>,
        /// Range reported by `dataset_range`, or an error if `None`.
        pub dataset_range: Option<TsRange>,
        /// Per-query cost overrides in nanodollars.
        pub costs: std::collections::BTreeMap<String, NanoUsd>,
        /// Per-query billable-size overrides in bytes.
        pub sizes: std::collections::BTreeMap<String, u64>,
        /// Cost for queries with no override.
        pub default_cost: NanoUsd,
        /// Billable size for queries with no override.
        pub default_size: u64,
        /// Unit price reported by `unit_price_usd_per_gb`.
        pub unit_price: f64,
        /// When set, `cost` fails with this instead of answering.
        pub cost_error: Option<ProviderError>,
    }

    impl FakeProvider {
        /// A provider that answers every quote with the given flat values.
        pub fn new(range: TsRange) -> FakeProvider {
            FakeProvider {
                calls: Vec::new(),
                dataset_range: Some(range),
                costs: std::collections::BTreeMap::new(),
                sizes: std::collections::BTreeMap::new(),
                default_cost: 0,
                default_size: 0,
                unit_price: 70.0,
                cost_error: None,
            }
        }

        /// Pins the cost and billable size for one exact query.
        pub fn with_quote(mut self, query: &QuoteQuery, cost: NanoUsd, size: u64) -> FakeProvider {
            let key = query_key(query);
            self.costs.insert(key.clone(), cost);
            self.sizes.insert(key, size);
            self
        }

        /// True if the log contains no money-spending call.
        pub fn spent_nothing(&self) -> bool {
            !self.calls.iter().any(|c| c.starts_with("submit"))
        }
    }

    impl BatchProvider for FakeProvider {
        fn dataset_range(&mut self, dataset: &str) -> Result<TsRange, ProviderError> {
            self.calls.push(format!("dataset_range:{dataset}"));
            self.dataset_range.ok_or(ProviderError::Transport {
                detail: "scripted failure".to_string(),
            })
        }

        fn billable_size(&mut self, query: &QuoteQuery) -> Result<u64, ProviderError> {
            let key = query_key(query);
            self.calls.push(format!("billable_size:{key}"));
            Ok(self.sizes.get(&key).copied().unwrap_or(self.default_size))
        }

        fn cost(&mut self, query: &QuoteQuery) -> Result<NanoUsd, ProviderError> {
            let key = query_key(query);
            self.calls.push(format!("cost:{key}"));
            if let Some(err) = &self.cost_error {
                return Err(err.clone());
            }
            Ok(self.costs.get(&key).copied().unwrap_or(self.default_cost))
        }

        fn unit_price_usd_per_gb(
            &mut self,
            dataset: &str,
            schema: &str,
        ) -> Result<f64, ProviderError> {
            self.calls.push(format!("unit_price:{dataset}/{schema}"));
            Ok(self.unit_price)
        }

        fn submit(&mut self, spec: &JobSpec) -> Result<SubmittedJob, ProviderError> {
            self.calls.push(format!("submit:{}", spec.target_file_path));
            Ok(SubmittedJob {
                job_id: format!("job-{}", self.calls.len()),
            })
        }
    }
}
