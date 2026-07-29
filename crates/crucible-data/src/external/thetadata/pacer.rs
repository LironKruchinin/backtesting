//! One **global** rate limiter for every request a pull makes — never one per
//! task.
//!
//! The Terminal enforces two different ceilings and only advertises one of
//! them. It logs `Max concurrent requests: 8`, and that figure is global across
//! stocks and options rather than per asset class. The second ceiling is
//! undocumented: a `JettyRateLimiter` inside the Terminal drops connections
//! under sustained sequential whole-chain load, and two probe runs died with
//! `HTTP 000` while the process itself stayed alive. Concurrency 8 is therefore
//! necessary and **not sufficient** — a launch-rate limit is the other half.
//!
//! Both limits are properties of the one Terminal process, so both belong to
//! one shared object. A per-task limiter multiplies rather than bounds: eight
//! tasks each politely holding themselves to one request per interval still
//! launch eight per interval between them, which is exactly the pattern that
//! tripped the limiter during probing.
//!
//! ## Why the clock is allowed here
//!
//! §2.2 bans wall-clock reads from result-affecting code, and `clippy.toml`
//! makes that a merge gate. Nothing in this module is result-affecting:
//! pacing changes *when* bytes are fetched, never *what* they are, and the
//! archive a run produces is identical whether the pacer slept 150 ms or 400 ms
//! between launches. That is the same argument D-0025 makes for the ingest
//! runtime. The clock read here is `tokio::time::Instant`, which the lint list
//! does not name, and it is named here so that the omission is a decision on
//! the record rather than an oversight.
//!
//! ## Backoff, `Retry-After`, and the circuit breaker
//!
//! Three distinct responses to three distinct situations, and collapsing them
//! is how a client turns a vendor hiccup into a vendor ban:
//!
//! - **A single failure** — transport drop, 429 or 5xx — is retried with
//!   exponential backoff, capped. Retrying an idempotent read is always safe
//!   here (D-0051): every ThetaData call is a read, and there is no submission
//!   analogue of D-0035's carve-out.
//! - **A 429 carrying `Retry-After`** is obeyed literally. The server has
//!   stated a number; guessing a smaller one is how a soft limit becomes a
//!   hard one. A `Retry-After` beyond [`MAX_RETRY_AFTER`] is treated as a
//!   refusal to serve rather than a delay to sleep through, because a pull
//!   that silently parks for an hour looks identical to a hung one.
//! - **Sustained failure** trips the circuit breaker after
//!   [`CIRCUIT_TRIP_CONSECUTIVE`] consecutive drops. It does **not** pause and
//!   retry forever: it fails the run. Resume is an inventory diff (§6.1 of
//!   `docs/THETADATA_PLAN.md`), so stopping costs one re-run of the requests
//!   that had not completed, and it costs nothing at all for the ones that
//!   had. A client that keeps hammering a Terminal that has stopped answering
//!   is the failure mode this exists to prevent.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, Semaphore};

/// Concurrent requests the Terminal will actually serve in parallel.
///
/// The Terminal's own figure, logged at startup, and global across asset
/// classes. Asking for more only builds a client-side queue in front of the
/// server-side one.
pub const MAX_CONCURRENCY: usize = 8;

/// Minimum wall-clock gap between two request launches, across the whole pull.
///
/// Chosen from measured per-request timings rather than guessed. Whole-chain
/// `eod`/`open_interest` days answer in **0.3–2.7 s** (`docs/THETADATA_PLAN.md`
/// §7.1), so eight in flight sustain roughly 3–13 requests per second on
/// natural completion alone. A 150 ms floor caps launches at 6.7/s, which sits
/// inside that band: it does not throttle the slow end at all, and it takes the
/// peak off the fast end — the burst shape that tripped the `JettyRateLimiter`
/// during probing, when whole-chain requests were issued back to back with no
/// floor whatsoever.
///
/// It is a floor on *launches*, not a sleep per request: with eight permits the
/// steady state is still governed by how fast the Terminal answers.
pub const MIN_LAUNCH_INTERVAL: Duration = Duration::from_millis(150);

/// Attempts made before a retryable failure is surfaced to the caller.
pub const MAX_ATTEMPTS: u32 = 4;

/// First backoff delay; each further attempt doubles it.
pub const BACKOFF_BASE: Duration = Duration::from_millis(500);

/// Ceiling on a single backoff sleep, so the doubling cannot run away.
pub const BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Longest `Retry-After` this client will honour by sleeping.
///
/// Beyond this the header is treated as a refusal rather than a delay: a pull
/// parked for an hour is indistinguishable from a hung one, and the operator
/// should be told rather than left waiting.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// Consecutive transport drops that trip the breaker and fail the run.
///
/// Consecutive, not cumulative: an archive-wide pull will legitimately see
/// scattered failures over hours, and counting those toward a total would
/// eventually trip on nothing but bad luck. A run of five with no success
/// between them is a Terminal that has stopped answering.
pub const CIRCUIT_TRIP_CONSECUTIVE: u32 = 5;

/// The shared launch governor.
///
/// Cheap to clone — every clone shares the same permits, the same launch
/// clock, and the same failure counter, which is the entire point.
#[derive(Clone)]
pub struct Pacer {
    permits: Arc<Semaphore>,
    next_launch: Arc<Mutex<Option<tokio::time::Instant>>>,
    min_launch_interval: Duration,
    consecutive_failures: Arc<AtomicU32>,
    trip_after: u32,
}

impl std::fmt::Debug for Pacer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pacer")
            .field("available_permits", &self.permits.available_permits())
            .field("min_launch_interval", &self.min_launch_interval)
            .field(
                "consecutive_failures",
                &self.consecutive_failures.load(Ordering::Relaxed),
            )
            .field("trip_after", &self.trip_after)
            .finish()
    }
}

impl Pacer {
    /// Builds a pacer with the given ceilings.
    ///
    /// `concurrency` is clamped into `1..=MAX_CONCURRENCY`.
    #[must_use]
    pub fn new(concurrency: usize, min_launch_interval: Duration) -> Pacer {
        Pacer {
            permits: Arc::new(Semaphore::new(concurrency.clamp(1, MAX_CONCURRENCY))),
            next_launch: Arc::new(Mutex::new(None)),
            min_launch_interval,
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            trip_after: CIRCUIT_TRIP_CONSECUTIVE,
        }
    }

    /// The pacer the plan calls for: the Terminal's own concurrency figure and
    /// the measured launch floor.
    #[must_use]
    pub fn standard() -> Pacer {
        Pacer::new(MAX_CONCURRENCY, MIN_LAUNCH_INTERVAL)
    }

    /// Concurrency ceiling in force.
    #[must_use]
    pub fn concurrency(&self) -> usize {
        self.permits.available_permits()
    }

    /// Launch floor in force.
    #[must_use]
    pub fn min_launch_interval(&self) -> Duration {
        self.min_launch_interval
    }

    /// True once the breaker has tripped; every further acquisition refuses.
    #[must_use]
    pub fn is_tripped(&self) -> bool {
        self.consecutive_failures.load(Ordering::Relaxed) >= self.trip_after
    }

    /// Consecutive failures recorded since the last success.
    #[must_use]
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// Records a completed request that succeeded, closing the breaker.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Records a transport-level failure; returns the new consecutive count.
    pub fn record_failure(&self) -> u32 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Waits for a permit **and** for the launch floor, in that order.
    ///
    /// Returns `None` when the breaker has tripped, which the caller turns into
    /// a run-ending error rather than a retry.
    ///
    /// The floor is applied while holding the permit so that the gap is between
    /// launches rather than between arrivals: two tasks that both waited on a
    /// busy semaphore must not be released into the same instant.
    pub async fn acquire(&self) -> Option<PacerPermit> {
        if self.is_tripped() {
            return None;
        }
        let permit = Arc::clone(&self.permits).acquire_owned().await.ok()?;

        let mut next = self.next_launch.lock().await;
        let now = tokio::time::Instant::now();
        let launch_at = match *next {
            Some(at) if at > now => at,
            _ => now,
        };
        *next = Some(launch_at + self.min_launch_interval);
        drop(next);

        if launch_at > now {
            tokio::time::sleep_until(launch_at).await;
        }
        Some(PacerPermit { _permit: permit })
    }

    /// Backoff delay before attempt `attempt` (1-based), capped.
    #[must_use]
    pub fn backoff_for(attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(16);
        BACKOFF_BASE.saturating_mul(1u32 << shift).min(BACKOFF_CAP)
    }
}

/// Proof that a request was allowed to launch. Dropping it returns the permit.
#[derive(Debug)]
pub struct PacerPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Parses a `Retry-After` header value into a delay this client will honour.
///
/// Only the delta-seconds form is accepted. The HTTP-date form is legal and is
/// deliberately **not** supported: reading it requires trusting the local clock
/// against the server's, and a skewed machine would compute a negative delay
/// and hammer the very endpoint that asked it to stop. An unparseable or
/// over-long value returns `None`, which the caller treats as "the server is
/// refusing" rather than "sleep zero".
#[must_use]
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    let delay = Duration::from_secs(seconds);
    (delay <= MAX_RETRY_AFTER).then_some(delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private current-thread runtime for the async assertions.
    ///
    /// Deliberately hand-rolled rather than `#[tokio::test]`: that macro needs
    /// tokio's `macros` feature, and pausing the clock needs `test-util`, both
    /// of which would be compiled into the shipped `thetadata` build to serve
    /// tests alone. The timings below are real and small instead.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("INVARIANT: a current-thread runtime with no I/O cannot fail to build")
            .block_on(future)
    }

    #[test]
    fn concurrency_is_clamped_to_what_the_terminal_serves() {
        assert_eq!(Pacer::new(64, MIN_LAUNCH_INTERVAL).concurrency(), 8);
        assert_eq!(Pacer::new(0, MIN_LAUNCH_INTERVAL).concurrency(), 1);
    }

    #[test]
    fn the_recorded_constants_are_the_ones_the_plan_documents() {
        // These are not arbitrary: MAX_CONCURRENCY is the Terminal's own
        // startup figure and MIN_LAUNCH_INTERVAL is derived from the measured
        // 0.3-2.7s request band in docs/THETADATA_PLAN.md §7.1. A change here
        // without a change there leaves the plan lying about the run.
        assert_eq!(MAX_CONCURRENCY, 8);
        assert_eq!(MIN_LAUNCH_INTERVAL, Duration::from_millis(150));
        assert_eq!(MAX_ATTEMPTS, 4);
        assert_eq!(CIRCUIT_TRIP_CONSECUTIVE, 5);
    }

    #[test]
    fn backoff_doubles_and_then_stops_doubling() {
        assert_eq!(Pacer::backoff_for(1), Duration::from_millis(500));
        assert_eq!(Pacer::backoff_for(2), Duration::from_secs(1));
        assert_eq!(Pacer::backoff_for(3), Duration::from_secs(2));
        assert_eq!(Pacer::backoff_for(4), Duration::from_secs(4));
        // Capped, and a wild attempt number must not overflow the shift.
        assert_eq!(Pacer::backoff_for(30), BACKOFF_CAP);
        assert_eq!(Pacer::backoff_for(u32::MAX), BACKOFF_CAP);
    }

    #[test]
    fn the_breaker_trips_on_consecutive_failures_and_a_success_clears_it() {
        let pacer = Pacer::standard();
        for _ in 0..CIRCUIT_TRIP_CONSECUTIVE - 1 {
            pacer.record_failure();
        }
        assert!(!pacer.is_tripped(), "one short of the trip point");
        pacer.record_success();
        assert_eq!(pacer.consecutive_failures(), 0, "a success clears the run");

        for _ in 0..CIRCUIT_TRIP_CONSECUTIVE {
            pacer.record_failure();
        }
        assert!(pacer.is_tripped());
    }

    // The counter is shared, not copied. A clone that counted its own failures
    // would mean eight tasks each needing five drops before anything tripped —
    // forty drops to notice a dead Terminal.
    #[test]
    fn clones_share_one_breaker_and_one_set_of_permits() {
        let pacer = Pacer::standard();
        let clone = pacer.clone();
        for _ in 0..CIRCUIT_TRIP_CONSECUTIVE {
            clone.record_failure();
        }
        assert!(pacer.is_tripped(), "the original sees the clone's failures");
    }

    #[test]
    fn a_tripped_breaker_refuses_to_launch_anything_further() {
        block_on(async {
            let pacer = Pacer::standard();
            assert!(pacer.acquire().await.is_some(), "healthy pacer launches");
            for _ in 0..CIRCUIT_TRIP_CONSECUTIVE {
                pacer.record_failure();
            }
            assert!(
                pacer.acquire().await.is_none(),
                "a tripped breaker must stop the run, not slow it down"
            );
        });
    }

    // The floor is global. Eight sequential launches must span at least seven
    // intervals — the property a per-task limiter loses, and losing it is what
    // tripped the vendor's JettyRateLimiter during probing. A short floor keeps
    // the test honest about the arithmetic without making it slow.
    #[test]
    fn launches_are_spaced_by_the_global_floor() {
        block_on(async {
            let floor = Duration::from_millis(10);
            let pacer = Pacer::new(MAX_CONCURRENCY, floor);
            let start = tokio::time::Instant::now();
            let mut held = Vec::new();
            for _ in 0..MAX_CONCURRENCY {
                held.push(pacer.acquire().await.expect("healthy"));
            }
            let spanned = tokio::time::Instant::now() - start;
            let expected = floor * (MAX_CONCURRENCY as u32 - 1);
            assert!(
                spanned >= expected,
                "eight launches spanned {spanned:?}, expected at least {expected:?}"
            );
        });
    }

    // Permits bound how many are in flight; the floor bounds how fast they
    // start. Neither substitutes for the other, so a third launch must wait for
    // a permit even with the floor set to zero.
    #[test]
    fn the_permit_ceiling_binds_independently_of_the_floor() {
        block_on(async {
            let pacer = Pacer::new(2, Duration::ZERO);
            let first = pacer.acquire().await.expect("healthy");
            let _second = pacer.acquire().await.expect("healthy");
            assert_eq!(pacer.concurrency(), 0, "both permits are out");

            let waiting = tokio::time::timeout(Duration::from_millis(50), pacer.acquire()).await;
            assert!(waiting.is_err(), "a third must block on the permit");

            drop(first);
            assert!(
                pacer.acquire().await.is_some(),
                "returning a permit releases the waiter"
            );
        });
    }

    #[test]
    fn retry_after_is_honoured_in_seconds_and_refused_when_absurd() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("  5 "), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after("0"), Some(Duration::ZERO));
        // Beyond the ceiling: a refusal to serve, not a nap to take.
        assert_eq!(parse_retry_after("3600"), None);
    }

    // The HTTP-date form is legal HTTP and deliberately unsupported. Parsing it
    // means trusting this machine's clock against the server's; a skewed clock
    // yields a negative delay, which becomes "retry immediately" against an
    // endpoint that just asked for a pause.
    #[test]
    fn the_http_date_form_of_retry_after_is_refused_rather_than_guessed() {
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("-5"), None);
        assert_eq!(parse_retry_after("soon"), None);
    }
}
