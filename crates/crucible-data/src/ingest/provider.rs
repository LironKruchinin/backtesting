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

use std::path::{Path, PathBuf};

use crate::catalog::TsRange;
use crucible_core::types::{NanoUsd, Ts};

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

/// Lifecycle of a submitted batch job.
///
/// These are the vendor's four states, spelled without vendor types. Note
/// what is absent: there is no `Cancelled`, because the batch API has no
/// cancel endpoint and no refund — once submitted, a job is paid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Accepted and waiting; jobs are processed one at a time, in order.
    Queued,
    /// At the front of the queue, being prepared.
    Processing,
    /// Finished and downloadable.
    Done,
    /// Past its download window. The bytes must be bought again.
    Expired,
}

impl core::fmt::Display for JobState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            JobState::Queued => "queued",
            JobState::Processing => "processing",
            JobState::Done => "done",
            JobState::Expired => "expired",
        };
        f.write_str(s)
    }
}

/// A batch job as the vendor currently describes it.
///
/// This is the identity used to recognise a job we already paid for. Every
/// field that participates in that match is present; nothing here is
/// cosmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteJob {
    /// Vendor batch job id.
    pub job_id: String,
    /// Where it is in its lifecycle.
    pub state: JobState,
    /// Vendor dataset.
    pub dataset: String,
    /// Vendor schema.
    pub schema: String,
    /// Symbols as submitted.
    pub symbols: Vec<String>,
    /// Input symbology, or `None` when the vendor reports one this crate does
    /// not model. `None` never silently matches: a job that agrees on
    /// everything else but cannot be confirmed on symbology is an ambiguity,
    /// and ambiguities refuse.
    pub stype_in: Option<StypeIn>,
    /// Half-open range the job covers.
    pub range: TsRange,
    /// When the vendor received the submission. The 30-day download window is
    /// measured from here, not from completion.
    pub received_ts: Ts,
    /// When the output stops being downloadable, when the vendor says.
    pub expiration_ts: Option<Ts>,
}

/// One file belonging to a completed batch job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    /// File name as delivered, e.g. `glbx-mdp3-20240101-20240201.ohlcv-1m.dbn.zst`.
    pub filename: String,
    /// Exact byte length the vendor will deliver.
    pub size_bytes: u64,
    /// Vendor-published SHA-256, normalised to bare lowercase hex.
    pub sha256_hex: String,
}

impl RemoteFile {
    /// True when this is the payload rather than a support file
    /// (`condition.json`, `manifest.json`, `metadata.json`).
    #[must_use]
    pub fn is_data_file(&self) -> bool {
        self.filename.ends_with(".dbn.zst")
    }
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
    /// Range the provider will actually serve for `dataset` at `schema`,
    /// given the account's entitlements.
    ///
    /// Planning intersects every requested window with this, so ranges are
    /// clipped from live data rather than from hardcoded constants that
    /// silently rot. It is also how a lapsed subscription becomes visible
    /// before a download rather than after.
    ///
    /// The range is **per schema**, not per dataset, because on `GLBX.MDP3`
    /// they differ: every schema starts 2010-06-06 except `mbo`, which starts
    /// 2017-05-21. A dataset-wide answer would leave a 16-year `mbo` request
    /// unclipped and let seven years of nonexistent data be quoted and bought.
    ///
    /// # Errors
    /// [`ProviderError`] if the range cannot be determined. Planning treats
    /// that as a refusal — it never falls back to an assumed range.
    fn dataset_range(&mut self, dataset: &str, schema: &str) -> Result<TsRange, ProviderError>;

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

    /// Current state of one job. Free to call.
    ///
    /// # Errors
    /// [`ProviderError`] if the job cannot be described.
    fn job(&mut self, job_id: &str) -> Result<RemoteJob, ProviderError>;

    /// Every non-expired job the account submitted at or after `since_ts`.
    ///
    /// This is the safety net under the job journal: it is how a run that lost
    /// its journal — or crashed between submitting and recording the id —
    /// discovers that it has *already bought* a window, instead of buying it
    /// twice. The batch API has no idempotency key, no cancel, and no refund,
    /// so this reconciliation is the only thing that makes a resubmit
    /// avoidable rather than merely unlikely.
    ///
    /// # Errors
    /// [`ProviderError`] if the listing fails, or [`ProviderError::BadResponse`]
    /// if any listed job cannot be represented as a [`RemoteJob`] — an
    /// unreadable listing must refuse, never silently omit the job that would
    /// have prevented a second purchase.
    fn list_jobs(&mut self, since_ts: Ts) -> Result<Vec<RemoteJob>, ProviderError>;

    /// Files belonging to a completed job, data and support alike. Free.
    ///
    /// # Errors
    /// [`ProviderError`] if the listing fails.
    fn list_files(&mut self, job_id: &str) -> Result<Vec<RemoteFile>, ProviderError>;

    /// Downloads every file of `job_id` into `dest_dir`, returning the paths
    /// written. Free within the job's download window.
    ///
    /// Implementations create `dest_dir` if needed and must never write
    /// outside it — `dest_dir` is a staging area, and the archive is reached
    /// only by a later rename that the execute path performs itself.
    ///
    /// # Errors
    /// [`ProviderError`] if the download fails.
    fn download(&mut self, job_id: &str, dest_dir: &Path) -> Result<Vec<PathBuf>, ProviderError>;
}

#[cfg(test)]
pub(crate) mod fake {
    //! A scripted [`BatchProvider`] for tests.
    //!
    //! The important member is `calls`: an ordered log of every method
    //! invoked. Money tests assert against it directly — "no `submit` appears
    //! in the log" is a property you can check, whereas "the code looks like
    //! it does not submit" is not.

    use super::{
        BatchProvider, JobSpec, JobState, Path, PathBuf, ProviderError, QuoteQuery, RemoteFile,
        RemoteJob, SubmittedJob, Ts, TsRange,
    };
    use crate::ingest::window::date_of;
    use crucible_core::types::NanoUsd;
    use std::collections::BTreeMap;

    /// One job as the fake vendor holds it: identity, a state script, its
    /// delivered file list, and the bytes `download` will actually write.
    pub(crate) struct FakeJob {
        pub meta: RemoteJob,
        /// States handed out by successive `job()` calls; the last repeats
        /// forever, so `vec![Queued, Processing, Done]` scripts a poll loop.
        pub states: Vec<JobState>,
        pub cursor: usize,
        pub files: Vec<RemoteFile>,
        /// File name -> bytes written into the staging dir on download.
        pub payloads: BTreeMap<String, Vec<u8>>,
    }

    /// The vendor's delivered-file naming, reproduced so tests exercise a
    /// realistic name rather than the archive's own path template.
    pub(crate) fn vendor_file_name(dataset: &str, schema: &str, range: TsRange) -> String {
        let start = date_of(range.start_ts());
        let end = date_of(range.end_ts());
        format!(
            "{}-{:04}{:02}{:02}-{:04}{:02}{:02}.{schema}.dbn.zst",
            dataset.to_ascii_lowercase().replace('.', "-"),
            start.year,
            start.month,
            start.day,
            end.year,
            end.month,
            end.day,
        )
    }

    /// A deterministic stand-in for a real SHA-256: 64 lowercase hex
    /// characters derived from the file name, so distinct files get distinct
    /// digests without hashing anything.
    pub(crate) fn fake_digest(name: &str) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in name.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("{h:016x}").repeat(4)
    }

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

        /// Jobs the fake vendor knows about, keyed by job id. Populated by
        /// `submit`, and pre-populated by `with_existing_job` to model a job
        /// that was paid for before the journal was lost.
        pub jobs: BTreeMap<String, FakeJob>,
        /// Every `JobSpec` submitted, in order.
        pub submits: Vec<JobSpec>,
        /// State script applied to newly submitted jobs.
        pub next_states: Vec<JobState>,
        /// When set, `submit` fails with this instead of accepting.
        pub submit_error: Option<ProviderError>,
        /// Timestamp stamped onto submitted jobs' `received_ts`.
        pub received_ts: Ts,
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
                jobs: BTreeMap::new(),
                submits: Vec::new(),
                next_states: vec![JobState::Done],
                submit_error: None,
                received_ts: Ts(1_700_000_000_000_000_000),
            }
        }

        /// Scripts the lifecycle newly submitted jobs will report.
        pub fn with_states(mut self, states: &[JobState]) -> FakeProvider {
            self.next_states = states.to_vec();
            self
        }

        /// Registers a job that already exists vendor-side without this run
        /// having submitted it — the situation a lost journal leaves behind.
        pub fn with_existing_job(
            mut self,
            job_id: &str,
            query: &QuoteQuery,
            target_file_path: &str,
        ) -> FakeProvider {
            let job = FakeProvider::build_job(job_id, query, target_file_path, self.received_ts);
            self.jobs.insert(job_id.to_string(), job);
            self
        }

        /// Overrides the delivered file list for a job — how a test scripts a
        /// wrong size, a missing payload, or two data files.
        pub fn with_files(mut self, job_id: &str, files: Vec<RemoteFile>) -> FakeProvider {
            if let Some(job) = self.jobs.get_mut(job_id) {
                job.files = files;
            }
            self
        }

        /// Scripts the lifecycle of an already-registered job.
        pub fn with_job_states(mut self, job_id: &str, states: &[JobState]) -> FakeProvider {
            if let Some(job) = self.jobs.get_mut(job_id) {
                job.states = states.to_vec();
            }
            self
        }

        /// Forgets the symbology of an existing job, modelling a vendor that
        /// reports one this crate does not model.
        pub fn with_unknown_stype(mut self, job_id: &str) -> FakeProvider {
            if let Some(job) = self.jobs.get_mut(job_id) {
                job.meta.stype_in = None;
            }
            self
        }

        /// The exact payload bytes the fake vendor delivers for a window.
        pub fn payload_for(target_file_path: &str) -> Vec<u8> {
            format!("DBN\u{1}fake payload for {target_file_path}").into_bytes()
        }

        /// Builds the fake vendor's view of an accepted job, including the one
        /// data file and three support files a real DBN job delivers.
        fn build_job(
            job_id: &str,
            query: &QuoteQuery,
            target_file_path: &str,
            received_ts: Ts,
        ) -> FakeJob {
            let data_name = vendor_file_name(&query.dataset, &query.schema, query.range);
            let payload = FakeProvider::payload_for(target_file_path);
            let mut files = vec![RemoteFile {
                filename: data_name.clone(),
                size_bytes: payload.len() as u64,
                sha256_hex: fake_digest(&data_name),
            }];
            let mut payloads = BTreeMap::new();
            payloads.insert(data_name, payload);
            for support in ["condition.json", "manifest.json", "metadata.json"] {
                let bytes = format!("{{\"support\":\"{support}\"}}").into_bytes();
                files.push(RemoteFile {
                    filename: support.to_string(),
                    size_bytes: bytes.len() as u64,
                    sha256_hex: fake_digest(support),
                });
                payloads.insert(support.to_string(), bytes);
            }
            FakeJob {
                meta: RemoteJob {
                    job_id: job_id.to_string(),
                    state: JobState::Done,
                    dataset: query.dataset.clone(),
                    schema: query.schema.clone(),
                    symbols: query.symbols.clone(),
                    stype_in: Some(query.stype_in),
                    range: query.range,
                    received_ts,
                    expiration_ts: Some(Ts(received_ts.0 + 30 * 86_400 * 1_000_000_000)),
                },
                states: vec![JobState::Done],
                cursor: 0,
                files,
                payloads,
            }
        }

        /// How many `submit` calls this provider has accepted. The number the
        /// crash-resume tests assert on.
        pub fn submit_count(&self) -> usize {
            self.submits.len()
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
        fn dataset_range(&mut self, dataset: &str, schema: &str) -> Result<TsRange, ProviderError> {
            self.calls.push(format!("dataset_range:{dataset}/{schema}"));
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
            if let Some(err) = &self.submit_error {
                return Err(err.clone());
            }
            self.submits.push(spec.clone());
            let job_id = format!("job-{}", self.submits.len());
            let mut job = FakeProvider::build_job(
                &job_id,
                &spec.query,
                &spec.target_file_path,
                self.received_ts,
            );
            job.states.clone_from(&self.next_states);
            self.jobs.insert(job_id.clone(), job);
            Ok(SubmittedJob { job_id })
        }

        fn job(&mut self, job_id: &str) -> Result<RemoteJob, ProviderError> {
            self.calls.push(format!("job:{job_id}"));
            let job = self
                .jobs
                .get_mut(job_id)
                .ok_or(ProviderError::BadResponse {
                    detail: format!("no such job {job_id}"),
                })?;
            let idx = job.cursor.min(job.states.len().saturating_sub(1));
            let state = job.states.get(idx).copied().unwrap_or(JobState::Done);
            job.cursor += 1;
            let mut meta = job.meta.clone();
            meta.state = state;
            Ok(meta)
        }

        fn list_jobs(&mut self, since_ts: Ts) -> Result<Vec<RemoteJob>, ProviderError> {
            self.calls.push(format!("list_jobs:{}", since_ts.0));
            Ok(self
                .jobs
                .values()
                .filter(|j| j.meta.received_ts >= since_ts)
                .map(|j| j.meta.clone())
                .collect())
        }

        fn list_files(&mut self, job_id: &str) -> Result<Vec<RemoteFile>, ProviderError> {
            self.calls.push(format!("list_files:{job_id}"));
            self.jobs
                .get(job_id)
                .map(|j| j.files.clone())
                .ok_or(ProviderError::BadResponse {
                    detail: format!("no such job {job_id}"),
                })
        }

        fn download(
            &mut self,
            job_id: &str,
            dest_dir: &Path,
        ) -> Result<Vec<PathBuf>, ProviderError> {
            self.calls.push(format!("download:{job_id}"));
            let job = self.jobs.get(job_id).ok_or(ProviderError::BadResponse {
                detail: format!("no such job {job_id}"),
            })?;
            std::fs::create_dir_all(dest_dir).map_err(|e| ProviderError::Transport {
                detail: e.to_string(),
            })?;
            let mut written = Vec::new();
            for (name, bytes) in &job.payloads {
                let path = dest_dir.join(name);
                std::fs::write(&path, bytes).map_err(|e| ProviderError::Transport {
                    detail: e.to_string(),
                })?;
                written.push(path);
            }
            Ok(written)
        }
    }
}
