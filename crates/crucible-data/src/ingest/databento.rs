//! The one file in the workspace that talks to Databento, and the one file
//! that contains `.await`.
//!
//! Everything else — planning, quoting, the spending gate, the execute state
//! machine — is sync, testable, and vendor-free because this module absorbs
//! the async client behind [`BatchProvider`] (D-0025). The runtime is
//! **current-thread**: `crucible-data` spawns no worker pool, since CLAUDE.md
//! §3 reserves thread-spawning for `crucible-funnel`. The honest caveat is
//! that any HTTP client resolves DNS on a blocking thread; that is not
//! result-affecting parallelism, but it should not be discovered by surprise.
//!
//! ## Submission parameters, and why they are hardcoded here
//!
//! The archive stores exactly one file per window, so a job must deliver
//! exactly one file. Databento's default `split_duration` is **`day`**, which
//! would shatter one month into ~31 files and one 16-year backfill into
//! ~5,800. Every splitting knob is therefore pinned off:
//!
//! | parameter | value | why |
//! |---|---|---|
//! | `split_duration` | `None` | one file per job |
//! | `split_size` | `None` | one file per job |
//! | `split_symbols` | `false` | one file per job |
//! | `encoding` | `Dbn` | the archive's native format |
//! | `compression` | `Zstd` | `.dbn.zst`, as the path template says |
//! | `stype_out` | `InstrumentId` | raw symbols stay in the DBN header |
//! | `delivery` | `Download` | the only mechanism the vendor supports |
//!
//! These are vendor spellings of an archive invariant, which is why they live
//! here and not on the trait: a different vendor would express "one file per
//! window" differently, and the seam should not have to know.
//!
//! ## The API key
//!
//! Read from the process environment by the caller, passed in once, and never
//! stored anywhere this crate can print. The key travels in an
//! `Authorization` header, never in a URL or a query string, so nothing a
//! [`ProviderError`] carries — status, server message, transport cause chain
//! — can put key material in front of a `Display`.

use std::path::{Path, PathBuf};

use databento::dbn::{SType, Schema};
use databento::historical::batch::{
    BatchJob, DownloadParams, JobState as VendorJobState, ListJobsParams, SplitDuration,
    SubmitJobParams,
};
use databento::historical::metadata::{GetBillableSizeParams, GetCostParams};
use databento::historical::{Client, DateTimeRange};
use databento::{Error as VendorError, Symbols};

use crate::catalog::TsRange;
use crate::ingest::money::usd_f64_to_nano_ceil;
use crate::ingest::provider::{
    BatchProvider, JobSpec, JobState, ProviderError, QuoteQuery, RemoteFile, RemoteJob, StypeIn,
    SubmittedJob,
};
use crucible_core::types::{NanoUsd, Ts};

/// How many times a retryable call is retried before giving up.
///
/// The metadata endpoints cap at 20 requests per second, which a wide plan
/// reaches while quoting. Retrying here keeps that vendor detail out of
/// `quote()`, which should not know what an HTTP 429 is.
const RETRIES: u32 = 4;

/// Fallback wait when a 429 arrives without a `Retry-After`.
const RATE_LIMIT_FALLBACK_SECS: u64 = 2;

/// Whether a failed call may simply be tried again.
///
/// This distinction is the difference between a resilient client and a
/// double purchase, so it is a type rather than a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Retry {
    /// Safe to repeat. Reading a job's state or listing files has no effect
    /// on the account, so a dropped connection is just a lost packet.
    Idempotent,
    /// **Repeating this could buy the data twice.** A transport failure on a
    /// submission is *ambiguous*: the connection may have died after the
    /// server accepted the job. The only safe response is to surface it and
    /// let the next run reconcile against the vendor's job list — which is
    /// exactly what the journal and `ingest::execute` are built to do.
    ///
    /// A 429 is still retried even here: throttling means the request was
    /// *rejected*, so it definitively did not create anything.
    OnlyIfRejected,
}

/// A [`BatchProvider`] backed by the real Databento historical API.
pub struct DatabentoProvider {
    runtime: tokio::runtime::Runtime,
    client: Client,
}

impl DatabentoProvider {
    /// Builds a provider from an API key.
    ///
    /// The key is consumed here and never stored by this crate; the client
    /// keeps whatever it needs internally.
    ///
    /// # Errors
    /// [`ProviderError::Unauthorized`] if the key is malformed, or
    /// [`ProviderError::Transport`] if the runtime or HTTP client cannot be
    /// built.
    pub fn new(api_key: &str) -> Result<DatabentoProvider, ProviderError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ProviderError::Transport {
                detail: format!("could not start the HTTP runtime: {e}"),
            })?;
        let client = Client::builder()
            .key(api_key)
            .map_err(|_| ProviderError::Unauthorized)?
            .build()
            .map_err(map_error)?;
        Ok(DatabentoProvider { runtime, client })
    }

    /// Runs one vendor call to completion on the private runtime, retrying
    /// according to `retry`.
    ///
    /// The future is rebuilt per attempt — a future cannot be polled after it
    /// resolves — and boxed so the closure can name a lifetime tied to the
    /// borrowed client. This is the only `block_on` in the workspace.
    ///
    /// ## Why transport retries are not optional here
    ///
    /// The runtime is current-thread (D-0025), so between our `block_on`
    /// calls **nothing drives reqwest's background tasks** — including the
    /// one that reaps idle pooled connections. The poll loop leaves 15-second
    /// gaps, during which the server quietly closes a keep-alive socket that
    /// the pool then hands out anyway, and the next request dies with
    /// "connection closed before message completed". Observed on the very
    /// first live pull, on the first `get_job_details` after a quote.
    ///
    /// The alternative fix — disabling connection pooling through the SDK's
    /// `http_client_builder` — needs `reqwest` as a direct dependency pinned
    /// to whatever version the client happens to use, which is the version
    /// skew this module avoids elsewhere by consuming `databento::dbn`.
    /// Retrying is cheaper, needs no dependency, and is the correct behaviour
    /// for a flaky network regardless of the pool.
    fn block_on_retrying<T, F>(&mut self, retry: Retry, mut make: F) -> Result<T, ProviderError>
    where
        F: for<'c> FnMut(
            &'c mut Client,
        )
            -> std::pin::Pin<Box<dyn Future<Output = databento::Result<T>> + 'c>>,
    {
        let runtime = &self.runtime;
        let client = &mut self.client;
        let mut attempt = 0;
        loop {
            let mapped = match runtime.block_on(make(&mut *client)) {
                Ok(value) => return Ok(value),
                Err(e) => map_error(e),
            };
            let wait = match (&mapped, retry) {
                // Throttling means the request was rejected outright, so it
                // created nothing and is always safe to repeat.
                (ProviderError::RateLimited { retry_after_secs }, _) => {
                    retry_after_secs.unwrap_or(RATE_LIMIT_FALLBACK_SECS)
                }
                // A dropped connection on a read costs nothing to repeat.
                (ProviderError::Transport { .. }, Retry::Idempotent) => 1 << attempt,
                // A dropped connection on a submission is ambiguous. Stop.
                _ => return Err(mapped),
            };
            if attempt >= RETRIES {
                return Err(mapped);
            }
            attempt += 1;
            std::thread::sleep(std::time::Duration::from_secs(wait));
        }
    }
}

/// Flattens an error's `source` chain into one line.
///
/// `reqwest`'s own `Display` for a transport failure is
/// "error sending request for url (…)" with the actual reason — DNS, TLS,
/// connect refused, timeout — hidden one or more levels down in `source()`.
/// Reporting only the top line turns every network problem into the same
/// unactionable sentence, which is exactly what happened the first time this
/// code met one.
fn cause_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut cursor = err.source();
    while let Some(cause) = cursor {
        parts.push(cause.to_string());
        cursor = cause.source();
    }
    parts.join(": ")
}

/// Maps a vendor error onto the seam's coarse vocabulary.
///
/// What is copied: the HTTP status, the server's message, and the transport
/// error's own cause chain. What is never copied: request headers. The API
/// key travels in an `Authorization` header, never in a URL or a query
/// string, so nothing here can put key material in front of a `Display`.
fn map_error(err: VendorError) -> ProviderError {
    match err {
        VendorError::Auth(detail) => {
            let _ = detail;
            ProviderError::Unauthorized
        }
        VendorError::Api(api) => {
            let status = api.status_code.as_u16();
            match status {
                401 => ProviderError::Unauthorized,
                403 => ProviderError::Forbidden {
                    detail: api.message,
                },
                429 => ProviderError::RateLimited {
                    retry_after_secs: None,
                },
                _ => ProviderError::BadResponse {
                    detail: format!("HTTP {status}: {}", api.message),
                },
            }
        }
        VendorError::Http(e) => ProviderError::Transport {
            detail: cause_chain(&e),
        },
        VendorError::Io(e) => ProviderError::Transport {
            detail: cause_chain(&e),
        },
        other => ProviderError::BadResponse {
            detail: cause_chain(&other),
        },
    }
}

fn schema_of(schema: &str) -> Result<Schema, ProviderError> {
    schema
        .parse::<Schema>()
        .map_err(|_| ProviderError::BadResponse {
            detail: format!("{schema:?} is not a schema this vendor serves"),
        })
}

fn stype_of(stype_in: StypeIn) -> SType {
    match stype_in {
        StypeIn::RawSymbol => SType::RawSymbol,
        StypeIn::Parent => SType::Parent,
        StypeIn::Continuous => SType::Continuous,
    }
}

fn stype_back(stype: SType) -> Option<StypeIn> {
    match stype {
        SType::RawSymbol => Some(StypeIn::RawSymbol),
        SType::Parent => Some(StypeIn::Parent),
        SType::Continuous => Some(StypeIn::Continuous),
        _ => None,
    }
}

fn to_ts(dt: time::OffsetDateTime) -> Result<Ts, ProviderError> {
    i64::try_from(dt.unix_timestamp_nanos()).map_or(
        Err(ProviderError::BadResponse {
            detail: format!("timestamp {dt} does not fit in i64 nanoseconds"),
        }),
        |n| Ok(Ts(n)),
    )
}

fn to_offset(ts: Ts) -> Result<time::OffsetDateTime, ProviderError> {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ts.0)).map_err(|e| {
        ProviderError::BadResponse {
            detail: format!("nanosecond timestamp {} is out of range: {e}", ts.0),
        }
    })
}

fn to_date_time_range(range: TsRange) -> Result<DateTimeRange, ProviderError> {
    Ok(DateTimeRange {
        start: to_offset(range.start_ts())?,
        end: to_offset(range.end_ts())?,
    })
}

fn symbols_of(query: &QuoteQuery) -> Symbols {
    Symbols::Symbols(query.symbols.clone())
}

fn job_state(state: VendorJobState) -> JobState {
    match state {
        VendorJobState::Queued => JobState::Queued,
        VendorJobState::Processing => JobState::Processing,
        VendorJobState::Done => JobState::Done,
        VendorJobState::Expired => JobState::Expired,
    }
}

/// Converts a vendor job description into the seam's [`RemoteJob`].
///
/// A job that cannot be represented is an error rather than an omission: the
/// listing exists to stop a second purchase, and a silently dropped entry is
/// precisely the entry that would have prevented one.
fn remote_job(job: &BatchJob) -> Result<RemoteJob, ProviderError> {
    // The API returns `symbols` as one comma-joined string, which the client
    // deserializes into a single-element vector. Splitting it back out keeps
    // reconciliation's symbol-set comparison correct: today every planned
    // window carries exactly one key, so this is invisible — but a future
    // caller that batched keys into one job would otherwise find that no
    // vendor job ever matched, and quietly buy every window a second time.
    let symbols = match &job.symbols {
        Symbols::Symbols(list) => list
            .iter()
            .flat_map(|s| s.split(','))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect(),
        Symbols::All => vec!["ALL_SYMBOLS".to_string()],
        Symbols::Ids(ids) => ids.iter().map(ToString::to_string).collect(),
    };
    let range = TsRange::new(to_ts(job.start)?, to_ts(job.end)?).map_err(|_| {
        ProviderError::BadResponse {
            detail: format!("job {} reports an empty or backwards range", job.id),
        }
    })?;
    let expiration_ts = match job.ts_expiration {
        Some(dt) => Some(to_ts(dt)?),
        None => None,
    };
    Ok(RemoteJob {
        job_id: job.id.clone(),
        state: job_state(job.state),
        dataset: job.dataset.clone(),
        schema: job.schema.to_string(),
        symbols,
        stype_in: stype_back(job.stype_in),
        range,
        received_ts: to_ts(job.ts_received)?,
        expiration_ts,
    })
}

/// Normalises a vendor digest to bare lowercase hex.
///
/// The API prefixes hashes `sha256:`; the seam's contract is the bare digest,
/// so the prefix is stripped exactly once and nothing else is assumed.
fn normalise_digest(hash: &str) -> String {
    hash.strip_prefix("sha256:")
        .unwrap_or(hash)
        .to_ascii_lowercase()
}

impl BatchProvider for DatabentoProvider {
    fn dataset_range(&mut self, dataset: &str, schema: &str) -> Result<TsRange, ProviderError> {
        let wanted = schema_of(schema)?;
        let dataset = dataset.to_string();
        let range = self.block_on_retrying(Retry::Idempotent, |client| {
            let dataset = dataset.clone();
            Box::pin(async move { client.metadata().get_dataset_range(dataset).await })
        })?;
        // Per-schema when the vendor reports it, dataset-wide otherwise.
        let (start, end) = range
            .range_by_schema
            .get(&wanted)
            .map_or((range.start, range.end), |r| (r.start, r.end));
        TsRange::new(to_ts(start)?, to_ts(end)?).map_err(|_| ProviderError::BadResponse {
            detail: format!("{schema} reports an empty availability range"),
        })
    }

    fn billable_size(&mut self, query: &QuoteQuery) -> Result<u64, ProviderError> {
        let params = GetBillableSizeParams::builder()
            .dataset(&query.dataset)
            .symbols(symbols_of(query))
            .schema(schema_of(&query.schema)?)
            .date_time_range(to_date_time_range(query.range)?)
            .stype_in(stype_of(query.stype_in))
            .build();
        self.block_on_retrying(Retry::Idempotent, |client| {
            let params = params.clone();
            Box::pin(async move { client.metadata().get_billable_size(&params).await })
        })
    }

    fn cost(&mut self, query: &QuoteQuery) -> Result<NanoUsd, ProviderError> {
        let params = GetCostParams::builder()
            .dataset(&query.dataset)
            .symbols(symbols_of(query))
            .schema(schema_of(&query.schema)?)
            .date_time_range(to_date_time_range(query.range)?)
            .stype_in(stype_of(query.stype_in))
            .build();
        let usd = self.block_on_retrying(Retry::Idempotent, |client| {
            let params = params.clone();
            Box::pin(async move { client.metadata().get_cost(&params).await })
        })?;
        // D-0027: money leaves `f64` at the API boundary, rounding up.
        usd_f64_to_nano_ceil(usd).map_err(|e| ProviderError::BadResponse {
            detail: format!("unusable cost quote: {e}"),
        })
    }

    fn unit_price_usd_per_gb(&mut self, dataset: &str, schema: &str) -> Result<f64, ProviderError> {
        let wanted = schema_of(schema)?;
        let dataset = dataset.to_string();
        let modes = self.block_on_retrying(Retry::Idempotent, |client| {
            let dataset = dataset.clone();
            Box::pin(async move { client.metadata().list_unit_prices(dataset).await })
        })?;
        modes
            .iter()
            .find(|m| m.mode == databento::historical::metadata::FeedMode::Historical)
            .and_then(|m| m.unit_prices.get(&wanted).copied())
            .ok_or_else(|| ProviderError::BadResponse {
                detail: format!("no historical unit price published for {schema}"),
            })
    }

    fn submit(&mut self, spec: &JobSpec) -> Result<SubmittedJob, ProviderError> {
        let query = &spec.query;
        let params = SubmitJobParams::builder()
            .dataset(&query.dataset)
            .symbols(symbols_of(query))
            .schema(schema_of(&query.schema)?)
            .date_time_range(to_date_time_range(query.range)?)
            .encoding(databento::dbn::Encoding::Dbn)
            .compression(databento::dbn::Compression::Zstd)
            // One file per window. The vendor default is `Day`, which would
            // deliver ~31 files for a month and break the archive's one
            // file per window invariant.
            .split_duration(SplitDuration::None)
            .split_symbols(false)
            .stype_in(stype_of(query.stype_in))
            .stype_out(SType::InstrumentId)
            .build();
        // The one call in this file that must not be retried blindly: a
        // dropped connection here may mean the job WAS created, and a retry
        // would buy the window twice. The journal's `Intended` record plus
        // next-run reconciliation resolve that ambiguity for free.
        let job = self.block_on_retrying(Retry::OnlyIfRejected, |client| {
            let params = params.clone();
            Box::pin(async move { client.batch().submit_job(&params).await })
        })?;
        Ok(SubmittedJob { job_id: job.id })
    }

    fn job(&mut self, job_id: &str) -> Result<RemoteJob, ProviderError> {
        let id = job_id.to_string();
        let job = self.block_on_retrying(Retry::Idempotent, |client| {
            let id = id.clone();
            Box::pin(async move { client.batch().get_job_details(&id).await })
        })?;
        remote_job(&job)
    }

    fn list_jobs(&mut self, since_ts: Ts) -> Result<Vec<RemoteJob>, ProviderError> {
        let since = to_offset(since_ts)?;
        // Every state, including `Expired`. The default filter omits expired
        // jobs, and an expired job we cannot see is a window we would buy a
        // second time without ever being asked.
        let params = ListJobsParams::builder()
            .states(vec![
                VendorJobState::Queued,
                VendorJobState::Processing,
                VendorJobState::Done,
                VendorJobState::Expired,
            ])
            .since(since)
            .build();
        let jobs = self.block_on_retrying(Retry::Idempotent, |client| {
            let params = params.clone();
            Box::pin(async move { client.batch().list_jobs(&params).await })
        })?;
        jobs.iter().map(remote_job).collect()
    }

    fn list_files(&mut self, job_id: &str) -> Result<Vec<RemoteFile>, ProviderError> {
        let id = job_id.to_string();
        let files = self.block_on_retrying(Retry::Idempotent, |client| {
            let id = id.clone();
            Box::pin(async move { client.batch().list_files(&id).await })
        })?;
        Ok(files
            .iter()
            .map(|f| RemoteFile {
                filename: f.filename.clone(),
                size_bytes: f.size,
                sha256_hex: normalise_digest(&f.hash),
            })
            .collect())
    }

    fn download(&mut self, job_id: &str, dest_dir: &Path) -> Result<Vec<PathBuf>, ProviderError> {
        std::fs::create_dir_all(dest_dir).map_err(|e| ProviderError::Transport {
            detail: e.to_string(),
        })?;
        let params = DownloadParams::builder()
            .output_dir(dest_dir.to_path_buf())
            .job_id(job_id)
            .build();
        let written = self.block_on_retrying(Retry::Idempotent, |client| {
            let params = params.clone();
            Box::pin(async move { client.batch().download(&params).await })
        })?;

        // The client writes into `dest_dir/{job_id}/`; the seam promises
        // `dest_dir` itself, so flatten before returning.
        let nested = dest_dir.join(job_id);
        let mut flattened = Vec::with_capacity(written.len());
        for path in written {
            let Some(name) = path.file_name() else {
                continue;
            };
            let target = dest_dir.join(name);
            if path != target {
                std::fs::rename(&path, &target).map_err(|e| ProviderError::Transport {
                    detail: format!("staging {}: {e}", target.display()),
                })?;
            }
            flattened.push(target);
        }
        let _ = std::fs::remove_dir(&nested);
        Ok(flattened)
    }
}
