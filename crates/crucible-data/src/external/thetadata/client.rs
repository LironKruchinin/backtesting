//! The one module in the ThetaData path that is async, and the seam that keeps
//! it there.
//!
//! Same shape as [`ingest::databento`](crate::ingest::databento) and for the
//! same reason (D-0025): the vendor client is async, `crucible-data`'s public
//! API is sync, so a **current-thread** runtime is owned privately here and
//! `block_on` appears nowhere else. `crucible-data` still starts no threads of
//! its own — CLAUDE.md §3 reserves that for `crucible-funnel`.
//!
//! ## Concurrency without threads
//!
//! The Terminal permits 8 concurrent requests (it logs the number at startup,
//! and that figure is global across stocks and options, not per asset class).
//! Eight in-flight HTTP requests do not need eight threads: they are IO-bound
//! futures multiplexed onto the single runtime thread by `JoinSet`. The
//! ceiling is a real one — exceeding it makes the Terminal queue rather than
//! parallelise, so raising it buys nothing.
//!
//! ## Ordering is by request index, never by completion
//!
//! [`ThetaClient::fetch_batch`] returns results in the order the requests were
//! given, regardless of which finished first (§2.2). Acquisition is not
//! result-affecting computation, but an archive whose contents depend on
//! network weather is one whose *rebuild* differs from its original, and the
//! whole point of the inventory is that the two agree.
//!
//! ## Bodies are buffered whole
//!
//! A response is read fully into memory before it is parsed. The largest
//! measured single response is a 1-minute whole-chain quote day for SPXW at
//! ~320 MB, so the worst case is bounded by (concurrency × largest response),
//! about 2.5 GB. That is affordable here and buys a sync parse path; streaming
//! would mean either blocking the single runtime thread on file writes or
//! taking tokio's `fs` feature, which offloads onto a blocking thread pool —
//! threads, in the crate that must not have them.

use std::time::Duration;

use reqwest::StatusCode;

use super::error::{STATUS_NO_DATA, ThetaError};
use super::pacer::{self, Pacer};

/// Default address of the v3 Theta Terminal REST server.
///
/// Port 25503, not the 25510 that most published examples show: 25510 was the
/// v2 port, and v3's generated `config.toml` writes 25503. The Terminal logs
/// the bound address on startup, which is the value to trust.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:25503/v3";

/// Concurrent requests the Terminal will actually serve in parallel.
///
/// Re-exported from [`pacer`], which owns every acquisition-side constant so
/// that there is one place to read them and one place to change them.
pub const MAX_CONCURRENCY: usize = pacer::MAX_CONCURRENCY;

/// One GET against the Terminal: an endpoint path plus its query.
///
/// Built rather than formatted by callers so that every parameter is named
/// once, in one place. The Terminal accepts unknown parameters silently, so a
/// typo here is invisible at runtime — the returned column header is the only
/// evidence, and [`super::schema`] checks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Path below the base URL, e.g. `/option/history/greeks/eod`.
    pub path: String,
    /// Query parameters in declaration order.
    pub query: Vec<(String, String)>,
}

impl Request {
    /// Builds a request for `path` with no parameters yet.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Request {
        Request {
            path: path.into(),
            query: Vec::new(),
        }
    }

    /// Appends one query parameter.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Request {
        self.query.push((key.into(), value.into()));
        self
    }

    /// Renders `path?k=v&…` for logging and inventory provenance.
    #[must_use]
    pub fn render(&self) -> String {
        if self.query.is_empty() {
            return self.path.clone();
        }
        let joined = self
            .query
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        format!("{}?{joined}", self.path)
    }
}

/// A sync handle onto the local Theta Terminal.
///
/// Owns a private current-thread runtime; construct one per pull, not one per
/// request.
pub struct ThetaClient {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
    base_url: String,
    concurrency: usize,
    pacer: Pacer,
}

impl std::fmt::Debug for ThetaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThetaClient")
            .field("base_url", &self.base_url)
            .field("concurrency", &self.concurrency)
            .field("pacer", &self.pacer)
            .finish_non_exhaustive()
    }
}

impl ThetaClient {
    /// Opens a client against `base_url` with the given concurrency ceiling.
    ///
    /// `concurrency` is clamped to at least 1 and at most [`MAX_CONCURRENCY`];
    /// asking for more than the Terminal serves only builds a client-side
    /// queue in front of the server-side one.
    ///
    /// # Errors
    /// [`ThetaError::TerminalUnreachable`] if the runtime or HTTP client
    /// cannot be constructed.
    pub fn open(
        base_url: impl Into<String>,
        concurrency: usize,
    ) -> Result<ThetaClient, ThetaError> {
        let base_url = base_url.into();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ThetaError::TerminalUnreachable {
                base_url: base_url.clone(),
                detail: format!("could not start the local runtime: {e}"),
            })?;
        let client = reqwest::Client::builder()
            // Whole-chain 1-minute days run to hundreds of MB and minutes of
            // wall clock; the default has no timeout, but being explicit means
            // a wedged Terminal fails the run instead of hanging it forever.
            .timeout(Duration::from_secs(1_800))
            .build()
            .map_err(|e| ThetaError::TerminalUnreachable {
                base_url: base_url.clone(),
                detail: format!("could not build the HTTP client: {e}"),
            })?;
        let concurrency = concurrency.clamp(1, MAX_CONCURRENCY);
        Ok(ThetaClient {
            runtime,
            client,
            base_url,
            concurrency,
            pacer: Pacer::new(concurrency, pacer::MIN_LAUNCH_INTERVAL),
        })
    }

    /// The launch governor this client shares across every request it makes.
    #[must_use]
    pub fn pacer(&self) -> &Pacer {
        &self.pacer
    }

    /// The base URL this client talks to.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Concurrency ceiling in force.
    #[must_use]
    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Fetches one request, retrying transport failures and 5xx/429.
    ///
    /// # Errors
    /// Any [`ThetaError`]; [`ThetaError::NoData`] is an ordinary outcome that
    /// callers record rather than retry.
    pub fn fetch(&self, request: &Request) -> Result<Vec<u8>, ThetaError> {
        let url = self.url_of(request);
        let client = self.client.clone();
        let rendered = request.render();
        let pacer = self.pacer.clone();
        self.runtime
            .block_on(async move { fetch_one(&client, &pacer, &url, &rendered).await })
    }

    /// Fetches many requests with at most [`Self::concurrency`] in flight,
    /// returning results **in request order**.
    ///
    /// Every request is spawned at once and the [`Pacer`] — not this loop —
    /// decides when each may launch. The previous shape processed a fixed-size
    /// chunk at a time and waited for all of it before starting the next, which
    /// made every chunk cost the *slowest* request in it: with a measured
    /// 0.3–2.7 s spread (§7.1 of `docs/THETADATA_PLAN.md`), seven fast requests
    /// idled behind one slow one, roughly halving throughput on a tranche that
    /// runs for hours. A shared semaphore keeps eight in flight continuously
    /// instead, and a permit freed by a fast request is taken immediately.
    ///
    /// The whole batch is driven by one `block_on`; nothing here outlives the
    /// call. Results are collected by request index and never by completion
    /// order (§2.2) — an archive whose contents depended on network weather
    /// would rebuild differently than it was built, and the inventory's whole
    /// purpose is that the two agree.
    pub fn fetch_batch(&self, requests: &[Request]) -> Vec<Result<Vec<u8>, ThetaError>> {
        let mut out: Vec<Option<Result<Vec<u8>, ThetaError>>> =
            (0..requests.len()).map(|_| None).collect();

        // Spawning happens INSIDE `block_on`. `JoinSet::spawn` needs an active
        // runtime context and panics without one — "there is no reactor
        // running" — and the panic is only reachable when a caller actually
        // batches, which is why it survived until the first real tranche.
        self.runtime.block_on(async {
            let mut set = tokio::task::JoinSet::new();
            for (index, request) in requests.iter().enumerate() {
                let url = self.url_of(request);
                let rendered = request.render();
                let client = self.client.clone();
                let pacer = self.pacer.clone();
                set.spawn(async move {
                    let result = fetch_one(&client, &pacer, &url, &rendered).await;
                    (index, result)
                });
            }
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok((index, result)) => {
                        if let Some(slot) = out.get_mut(index) {
                            *slot = Some(result);
                        }
                    }
                    Err(e) => {
                        // A task cannot panic here without a bug in this
                        // module; surface it rather than losing a slot.
                        debug_assert!(false, "thetadata fetch task failed: {e}");
                    }
                }
            }
        });

        out.into_iter()
            .enumerate()
            .map(|(index, slot)| {
                slot.unwrap_or_else(|| {
                    Err(ThetaError::Transport {
                        path: requests
                            .get(index)
                            .map_or_else(|| "<unknown>".to_owned(), Request::render),
                        attempts: 0,
                        detail: "the request was never driven to completion".to_owned(),
                    })
                })
            })
            .collect()
    }

    /// Joins the base URL and a request into a full URL.
    fn url_of(&self, request: &Request) -> String {
        format!("{}{}", self.base_url, request.render())
    }
}

/// Whether a status is worth another attempt.
fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// One request with retries, launched through the shared [`Pacer`].
///
/// Split out so both `fetch` and `fetch_batch` use exactly the same status
/// handling, and so that every retry re-acquires from the pacer rather than
/// bypassing it — a retry storm that skipped the launch floor would be the one
/// burst most likely to be happening while the Terminal is already unhappy.
async fn fetch_one(
    client: &reqwest::Client,
    pacer: &Pacer,
    url: &str,
    rendered: &str,
) -> Result<Vec<u8>, ThetaError> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let Some(_permit) = pacer.acquire().await else {
            return Err(ThetaError::CircuitOpen {
                path: rendered.to_owned(),
                consecutive_failures: pacer.consecutive_failures(),
            });
        };

        let sent = client.get(url).send().await;
        let response = match sent {
            Ok(response) => response,
            Err(e) => {
                pacer.record_failure();
                if attempt >= pacer::MAX_ATTEMPTS {
                    return Err(ThetaError::Transport {
                        path: rendered.to_owned(),
                        attempts: attempt,
                        detail: e.to_string(),
                    });
                }
                drop(_permit);
                tokio::time::sleep(Pacer::backoff_for(attempt)).await;
                continue;
            }
        };

        let status = response.status();
        if is_retryable(status) && attempt < pacer::MAX_ATTEMPTS {
            // A 429 that names its own delay is obeyed literally: the server
            // has stated a number, and guessing a smaller one is how a soft
            // limit becomes a hard one. Anything else backs off exponentially.
            let stated = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(pacer::parse_retry_after);
            let delay = stated.unwrap_or_else(|| Pacer::backoff_for(attempt));
            pacer.record_failure();
            drop(_permit);
            tokio::time::sleep(delay).await;
            continue;
        }

        let body = response.bytes().await.map_err(|e| ThetaError::Transport {
            path: rendered.to_owned(),
            attempts: attempt,
            detail: format!("reading the body: {e}"),
        })?;

        // The breaker counts *transport* health, not vendor semantics: a 472
        // "no data" is a complete, correct answer and must not look like a
        // failing Terminal. Below a root's greeks floor every request answers
        // 472, and a breaker that counted those would trip on working data.
        pacer.record_success();
        return classify(status, &body, rendered);
    }
}

/// Turns a status and body into either bytes or the right [`ThetaError`].
fn classify(status: StatusCode, body: &[u8], rendered: &str) -> Result<Vec<u8>, ThetaError> {
    let head = || {
        let text = String::from_utf8_lossy(body);
        let trimmed = text.trim();
        trimmed.chars().take(400).collect::<String>()
    };

    if status.is_success() {
        return Ok(body.to_vec());
    }
    match status.as_u16() {
        STATUS_NO_DATA => Err(ThetaError::NoData {
            path: rendered.to_owned(),
            message: head(),
        }),
        403 => Err(ThetaError::NotEntitled {
            path: rendered.to_owned(),
            message: head(),
        }),
        400 => Err(ThetaError::BadRequest {
            path: rendered.to_owned(),
            message: head(),
        }),
        404 | 410 => Err(ThetaError::UnknownEndpoint {
            path: rendered.to_owned(),
            status: status.as_u16(),
        }),
        other => Err(ThetaError::UnexpectedStatus {
            path: rendered.to_owned(),
            status: other,
            body_head: head(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_is_stable_and_ordered() {
        let request = Request::new("/option/history/greeks/eod")
            .with("symbol", "SPY")
            .with("expiration", "*")
            .with("start_date", "20240102");
        assert_eq!(
            request.render(),
            "/option/history/greeks/eod?symbol=SPY&expiration=*&start_date=20240102"
        );
    }

    #[test]
    fn a_pathless_request_renders_bare() {
        assert_eq!(
            Request::new("/option/list/symbols").render(),
            "/option/list/symbols"
        );
    }

    // 472 is the vendor's "nothing here", and it is an ordinary outcome for
    // any root before its greeks history begins. Mistaking it for a failure
    // would abort a pull over a date that is legitimately empty; mistaking it
    // for success would record a gap as covered.
    #[test]
    fn the_vendor_no_data_status_is_its_own_outcome() {
        let err = classify(
            StatusCode::from_u16(STATUS_NO_DATA).expect("valid status"),
            b"No data found for your request",
            "/option/history/greeks/eod?symbol=SPY",
        )
        .expect_err("472 is not success");
        assert!(err.is_no_data(), "{err}");
    }

    // A retired API version and a misspelled path are client bugs, and must
    // not be recorded as an empty archive slice.
    #[test]
    fn retired_and_missing_endpoints_are_client_bugs() {
        for status in [404u16, 410] {
            let err = classify(
                StatusCode::from_u16(status).expect("valid status"),
                b"We have upgraded to API v3.",
                "/v2/hist/option/eod",
            )
            .expect_err("not success");
            assert!(matches!(err, ThetaError::UnknownEndpoint { .. }), "{err}");
            assert!(!err.is_no_data(), "must not look like an empty result");
        }
    }

    #[test]
    fn entitlement_refusals_keep_the_vendor_wording() {
        let err = classify(
            StatusCode::FORBIDDEN,
            b"Requesting an index endpoint requiring a value subscription, but you only have a FREE subscription.",
            "/index/history/price?symbol=SPX",
        )
        .expect_err("403 is not success");
        match err {
            ThetaError::NotEntitled { message, .. } => {
                assert!(message.contains("FREE subscription"), "{message}");
            }
            other => panic!("expected NotEntitled, got {other}"),
        }
    }

    #[test]
    fn success_returns_the_body_verbatim() {
        let body = b"symbol,expiration\n\"SPY\",\"2024-01-19\"\n";
        let out = classify(StatusCode::OK, body, "/x").expect("success");
        assert_eq!(out, body);
    }

    // The base URL is pinned to a literal loopback IPv4 address, and two
    // plausible-looking "improvements" are both wrong:
    //
    // - `localhost` may resolve to `::1` first, while the Terminal's listener
    //   answers on IPv4 — an intermittent connection failure that looks like a
    //   dead Terminal.
    // - The machine's LAN address (10.100.102.7 here) works only by accident:
    //   same-host traffic to a local address bypasses inbound filtering, so it
    //   would sail past the firewall rule that exists to keep this API off the
    //   network — and would break the moment the pull ran anywhere else.
    #[test]
    fn the_base_url_stays_a_literal_loopback_address() {
        assert_eq!(DEFAULT_BASE_URL, "http://127.0.0.1:25503/v3");
        assert!(
            !DEFAULT_BASE_URL.contains("localhost"),
            "a hostname can resolve to ::1 while the Terminal binds IPv4"
        );
        assert!(
            DEFAULT_BASE_URL.starts_with("http://127.0.0.1:"),
            "the client must never reach the Terminal over a routable address"
        );
    }

    // REGRESSION. `fetch_batch` spawned its tasks outside `block_on`, so
    // `JoinSet::spawn` found no runtime and panicked with "there is no reactor
    // running". It survived review, unit tests and a whole tranche's worth of
    // design because *nothing called it* — the driver fetched one at a time —
    // and the first batched request took the process down.
    //
    // No live Terminal needed: an unreachable port exercises the spawn, the
    // pacer, the retry ladder and the result-ordering path, and asserts the
    // failure arrives as N ordered `Err`s rather than as a panic. A batch that
    // cannot connect must still answer once per request, in request order.
    #[test]
    fn fetch_batch_spawns_inside_a_runtime_and_answers_once_per_request() {
        // Port 1 is reserved and nothing listens there; the client fails fast.
        let client = ThetaClient::open("http://127.0.0.1:1/v3", 4).expect("open");
        let requests: Vec<Request> = (0..3)
            .map(|n| Request::new("/option/history/eod").with("symbol", format!("R{n}")))
            .collect();

        let results = client.fetch_batch(&requests);

        assert_eq!(results.len(), 3, "one answer per request, always");
        for (index, result) in results.iter().enumerate() {
            // Either failure is correct here and which one arrives is a race:
            // three requests times four attempts is twelve drops, so the
            // breaker's five-consecutive threshold trips partway through. That
            // it trips at all against a genuinely dead endpoint is D-0056
            // working end to end rather than only in its unit tests.
            let path = match result {
                Err(ThetaError::Transport { path, .. } | ThetaError::CircuitOpen { path, .. }) => {
                    path
                }
                other => panic!("expected a connection failure at {index}, got {other:?}"),
            };
            // Results come back by request index, never by completion order
            // (§2.2) — the property that keeps a rebuild identical to the
            // original build.
            assert!(path.contains(&format!("R{index}")), "{path} at {index}");
        }
    }

    #[test]
    fn an_empty_batch_is_not_a_special_case() {
        let client = ThetaClient::open("http://127.0.0.1:1/v3", 4).expect("open");
        assert!(client.fetch_batch(&[]).is_empty());
    }

    #[test]
    fn concurrency_is_clamped_to_what_the_terminal_serves() {
        let client = ThetaClient::open(DEFAULT_BASE_URL, 64).expect("open");
        assert_eq!(client.concurrency(), MAX_CONCURRENCY);
        let client = ThetaClient::open(DEFAULT_BASE_URL, 0).expect("open");
        assert_eq!(client.concurrency(), 1);
    }
}
