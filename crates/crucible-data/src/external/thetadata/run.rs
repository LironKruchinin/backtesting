//! The acquisition loop: stream completions, taking each response all the way
//! to its inventory line the moment it arrives.
//!
//! ```text
//! pacer ─► fetch ─► validate ─► transcode ─► staging ─► rename ─► inventory
//!   (8 in flight)   └── per response, as it completes, in ANY order ──┘
//! ```
//!
//! **Nothing waits on a slower neighbour**, and getting there took two wrong
//! shapes. Fetching one request at a time measured **0.84–0.96 req/s** against
//! a projection of 5.3, because a serial caller cannot use concurrency the
//! pacer is willing to grant. Fetching in barriered chunks of 64 was worse in
//! the tail: a chunk costs its slowest member, and with SPY `eod` days of
//! comparable size answering in **22.7 s, 4.8 s and 1.7 s**, one draw stalled
//! sixty-three writes for five minutes with the CPU flat.
//!
//! Out-of-order processing is safe because **resume keys on the rendered
//! request string, never on position** (§6.1). The inventory is append-only and
//! its order is arrival order, which no reader depends on.
//!
//! **The inventory line is appended last, after the file is durably in place.**
//! That ordering is the whole contract, and it is the same one D-0017 gives the
//! Databento archive: a record is a statement about a file that exists. Reverse
//! it and a run that dies between the two leaves a line claiming a file nobody
//! wrote, which resume would then skip forever — a hole that no later check can
//! find, because every later check trusts the inventory. In the other order the
//! worst case is an orphan file with no line, which resume simply re-fetches
//! and overwrites.
//!
//! **That ordering constraint has now paid for itself twice, measured.** The T0
//! tranche was killed mid-run on two separate occasions — once to fix serial
//! fetching, once to fix a chunk-size barrier — and both times left **zero
//! orphan `.partial` files** (508 and 572 inventory lines respectively), with
//! the restart planning exactly 82,981 of 83,489 requests: precisely what had
//! not been done, and nothing that had. Write-to-temp-then-rename and
//! append-after-placement are what make a kill a non-event rather than a
//! forensic exercise.
//!
//! ## One bad day must not kill a 60,000-request run
//!
//! A validation failure refuses **that file**, records it in the refusal ledger,
//! and continues. Anything else would mean a single malformed vendor day costs
//! the whole tranche — and the refusal is not lost, because it is neither
//! written to the inventory (so resume retries it) nor swallowed (so the report
//! names it).
//!
//! ## But a systemic failure must
//!
//! Refusing one day in ten thousand is a vendor hiccup. Refusing one in five is
//! this build misreading the feed, and continuing would produce a tranche whose
//! gaps are our own bug wearing the vendor's clothes. Past
//! [`REFUSAL_RATE_LIMIT`], measured over at least [`REFUSAL_MIN_SAMPLE`]
//! attempts so a bad first handful cannot trip it, the run halts and says so.
//! That is a finding, not a failure to retry.
//!
//! ## Empty is recorded, not retried forever
//!
//! A vendor 472 is an ordinary outcome (§3.4), and a run that did not record it
//! would ask the same empty question on every resume until the subscription
//! ends. It gets an inventory line with no file — `file_path` empty, row count
//! zero — which is the honest statement: *asked, answered, nothing there*.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::client::ThetaClient;
use super::error::ThetaError;
use super::inventory::{Inventory, InventoryRecord};
use super::plan::{PlannedRequest, TranchePlan};
use super::schema::Endpoint;
use super::telemetry::{self, Counts, ExitKind, HEARTBEAT_EVERY};
use super::transcode::{TranscodeSource, write_parquet};
use super::validate::{ValidationReport, validate};
use crate::calendar::CivilDate;

/// Fraction of attempts that may be refused before the run halts.
///
/// 2 %. Chosen rather than derived: nothing has been measured that would set it,
/// and a threshold nobody can justify should at least be one somebody wrote
/// down. Loose enough that scattered vendor bad days do not stop a tranche,
/// tight enough that a systematic misread cannot quietly become the archive.
pub const REFUSAL_RATE_LIMIT: f64 = 0.02;

/// Attempts required before the refusal rate is allowed to halt anything.
///
/// Without a floor, three refusals in the first four requests would trip a
/// 2 % limit — the small-sample trap, and exactly the shape of §0.4's "a test
/// whose failure mode produces the desired answer".
pub const REFUSAL_MIN_SAMPLE: u64 = 200;

/// How many requests are streamed before the run pauses to check its own
/// health.
///
/// **Not a batch.** Within a window every response is processed the instant it
/// arrives, so nothing waits on a slower neighbour — that is the whole point of
/// streaming, and a 64-request barrier once stalled this run for five minutes
/// with the CPU flat. The window exists for two narrower reasons: it bounds how
/// many futures are alive at once, and it gives the refusal-rate breaker a
/// place to stop the run rather than draining 80,000 fetches it has already
/// decided not to keep.
///
/// At 2048 the one barrier per window costs a single worst draw — call it 30 s
/// against a window that takes over an hour — which is under 1 % and buys clean
/// abort semantics.
pub const BREAKER_CHECKPOINT: usize = 2048;

/// One request that did not produce a file, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The rendered request.
    pub request: String,
    /// What went wrong, rendered.
    pub reason: String,
}

/// What one tranche run did.
#[derive(Debug, Clone, Default)]
pub struct RunReport {
    /// Requests attempted this run.
    pub attempted: u64,
    /// Requests that produced a Parquet file.
    pub written: u64,
    /// Requests the vendor answered 472 for, recorded as empty.
    pub empty: u64,
    /// Requests refused. Not written to the inventory, so resume retries them.
    pub refusals: Vec<Refusal>,
    /// Bytes of Parquet written.
    pub bytes_written: u64,
    /// Golden-raw samples kept this run.
    pub golden_kept: u64,
    /// Set when the run stopped early, with the reason.
    pub halted: Option<String>,
    /// Wall time of every request this run, in milliseconds.
    ///
    /// Kept per request rather than as a running mean because the *shape* is
    /// what matters: measured SPY `eod` days of comparable size answered in
    /// 22.7 s, 4.8 s and 1.7 s, and a mean over that hides exactly the tail
    /// that governs how a barrier-free design beats a barriered one. T1's
    /// projection wants this distribution by era; the inventory carries it per
    /// line, and this is the in-run summary.
    pub latencies_ms: Vec<u64>,
}

/// Milliseconds of a duration, saturating.
fn millis_of(elapsed: core::time::Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

impl RunReport {
    /// Refusals as a fraction of attempts.
    #[must_use]
    pub fn refusal_rate(&self) -> f64 {
        if self.attempted == 0 {
            return 0.0;
        }
        self.refusals.len() as f64 / self.attempted as f64
    }

    /// Whether the refusal rate has become a finding rather than noise.
    /// This run's counts, for telemetry.
    #[must_use]
    pub fn counts(&self) -> Counts {
        Counts {
            attempted: self.attempted,
            written: self.written,
            empty: self.empty,
            refused: self.refusals.len() as u64,
        }
    }

    /// Writes the exit record for this run (D-0092).
    ///
    /// Every exit path calls it, because the exit that most needs explaining is
    /// the one nobody planned for. Best-effort: the return value is discarded
    /// here and asserted only in `telemetry`'s own tests.
    pub fn write_exit(&self, data_dir: &Path, now_ts: i64, kind: ExitKind, reason: &str) {
        let _ = telemetry::write_last_exit(data_dir, now_ts, kind, reason, self.counts());
    }

    #[must_use]
    pub fn refusal_rate_is_systemic(&self) -> bool {
        self.attempted >= REFUSAL_MIN_SAMPLE && self.refusal_rate() > REFUSAL_RATE_LIMIT
    }

    /// Latency percentiles in milliseconds: (min, median, p95, max).
    ///
    /// `None` when nothing was measured — unmeasured, never zero, the same
    /// refusal to report a figure over an empty sample every other rate here
    /// makes (§0.4).
    #[must_use]
    pub fn latency_percentiles(&self) -> Option<(u64, u64, u64, u64)> {
        if self.latencies_ms.is_empty() {
            return None;
        }
        let mut sorted = self.latencies_ms.clone();
        sorted.sort_unstable();
        let at = |q: f64| {
            let index = ((sorted.len() as f64 - 1.0) * q).round() as usize;
            sorted[index.min(sorted.len() - 1)]
        };
        Some((sorted[0], at(0.50), at(0.95), sorted[sorted.len() - 1]))
    }
}

impl core::fmt::Display for RunReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "  attempted      {}", self.attempted)?;
        writeln!(f, "  written        {}", self.written)?;
        writeln!(f, "  empty (472)    {}", self.empty)?;
        writeln!(f, "  refused        {}", self.refusals.len())?;
        writeln!(f, "  golden kept    {}", self.golden_kept)?;
        writeln!(
            f,
            "  bytes written  {}",
            super::plan::human_bytes(self.bytes_written)
        )?;
        if !self.refusals.is_empty() {
            writeln!(
                f,
                "\n  refusal ledger (not inventoried — resume retries these):"
            )?;
            for refusal in self.refusals.iter().take(40) {
                writeln!(f, "    {} — {}", refusal.request, refusal.reason)?;
            }
            if self.refusals.len() > 40 {
                writeln!(f, "    … and {} more", self.refusals.len() - 40)?;
            }
        }
        if let Some((min, median, p95, max)) = self.latency_percentiles() {
            // Percentiles, never a mean: the mean is exactly what hides the
            // tail that governs whether a barrier-free design is worth having.
            writeln!(
                f,
                "  latency ms     min {min}  median {median}  p95 {p95}  max {max}"
            )?;
        }
        if let Some(why) = &self.halted {
            writeln!(f, "\n  HALTED: {why}")?;
        }
        Ok(())
    }
}

/// Where a request's Parquet belongs, per `docs/DATA_LAYOUT.md`.
///
/// `external/thetadata/options/{root}/{type}/{grain}/{date}.parquet`. The type
/// segment is the endpoint's own name so that two endpoints for one root-day
/// cannot collide, which is the failure a flat `{root}/{date}` layout invites.
#[must_use]
pub fn output_path(data_dir: &Path, request: &PlannedRequest) -> PathBuf {
    data_dir
        .join("external")
        .join("thetadata")
        .join("options")
        .join(&request.root)
        .join(type_segment(request.endpoint))
        .join("daily")
        .join(format!(
            "{:04}-{:02}-{:02}.parquet",
            request.date.year, request.date.month, request.date.day
        ))
}

/// Path segment naming an endpoint's data type.
#[must_use]
pub fn type_segment(endpoint: Endpoint) -> &'static str {
    match endpoint {
        Endpoint::OptionEod => "eod",
        Endpoint::OptionGreeksEod => "greeks_eod",
        Endpoint::OptionOpenInterest => "open_interest",
        Endpoint::OptionQuote => "quote",
        Endpoint::OptionOhlc => "ohlc",
        Endpoint::OptionGreeksFirstOrder => "greeks_first_order",
        Endpoint::StockOhlc => "stock_ohlc",
        Endpoint::StockQuote => "stock_quote",
    }
}

/// Where a golden-raw sample belongs.
#[must_use]
pub fn golden_path(data_dir: &Path, request: &PlannedRequest) -> PathBuf {
    data_dir
        .join("external")
        .join("thetadata")
        .join("golden_raw")
        .join(&request.root)
        .join(type_segment(request.endpoint))
        .join(format!(
            "{:04}-{:02}-{:02}.csv",
            request.date.year, request.date.month, request.date.day
        ))
}

/// Decides which requests are golden-raw samples: **one day per (root, type,
/// year)**, per §6.
///
/// The first session of each year that the plan actually contains, so the
/// choice is deterministic and needs no clock, no randomness and no second
/// pass. Returned as a set of rendered requests so the run loop's decision is a
/// lookup rather than a re-derivation.
#[must_use]
pub fn golden_sample_set(plan: &TranchePlan) -> BTreeSet<String> {
    let mut seen: BTreeSet<(String, &'static str, i64)> = BTreeSet::new();
    let mut chosen = BTreeSet::new();
    for request in &plan.requests {
        let key = (
            request.root.clone(),
            type_segment(request.endpoint),
            request.date.year,
        );
        if seen.insert(key) {
            chosen.insert(request.request.render());
        }
    }
    chosen
}

/// What one request produced.
enum Outcome {
    Written {
        bytes: u64,
        report: ValidationReport,
    },
    Empty,
    Refused(String),
}

/// Runs the outstanding half of a plan.
///
/// `now_ts` is passed in rather than read: `crucible-data` does not read the
/// wall clock (§2.2, D-0032), and the inventory's `fetched_ts` is the caller's
/// to supply.
///
/// # Errors
/// Only for failures that end the run — a tripped pacer breaker, or an
/// inventory that cannot be written. Per-file problems become refusals.
pub fn run_tranche(
    client: &ThetaClient,
    plan: &TranchePlan,
    outstanding: &[PlannedRequest],
    inventory: &Inventory,
    data_dir: &Path,
    now_ts: i64,
    mut progress: impl FnMut(&PlannedRequest, u64, u64),
) -> Result<RunReport, ThetaError> {
    let golden = golden_sample_set(plan);
    let mut report = RunReport::default();
    let total = outstanding.len() as u64;

    for window in outstanding.chunks(BREAKER_CHECKPOINT) {
        let mut fatal: Option<ThetaError> = None;
        let requests: Vec<_> = window.iter().map(|r| r.request.clone()).collect();

        // Streaming: each response is taken all the way to its inventory line
        // the moment it arrives, so no request waits on a slower neighbour.
        client.fetch_streaming(&requests, |index, body, elapsed| {
            if fatal.is_some() || report.halted.is_some() {
                // The run is already over. Remaining fetches drain without
                // being written, and because they were never inventoried,
                // resume picks them up untouched.
                return;
            }
            let Some(request) = window.get(index) else {
                return;
            };
            report.attempted += 1;
            report.latencies_ms.push(millis_of(elapsed));
            progress(request, report.attempted, total);

            let rendered = request.request.render();
            let outcome = process_one(body, request, data_dir, &golden, &rendered, &mut report);

            // Operational heartbeat (D-0092). Best-effort and never branched
            // on: a monitor that can break the thing it monitors is worse than
            // no monitor. `refused` is the count the inventory deliberately
            // does not carry, which is the whole reason this file exists.
            if report.attempted.is_multiple_of(HEARTBEAT_EVERY) {
                telemetry::write_heartbeat(data_dir, now_ts, report.counts());
            }

            match outcome {
                Err(e) => fatal = Some(e),
                Ok(Outcome::Refused(reason)) => {
                    report.refusals.push(Refusal {
                        request: rendered,
                        reason,
                    });
                    // A systemic refusal rate is a finding about this build,
                    // not a reason to keep going and archive the result.
                    if report.refusal_rate_is_systemic() {
                        report.halted = Some(format!(
                            "refusal rate {:.1}% over {} attempts exceeds the {:.1}% limit — \
                             that is this build misreading the feed, not a vendor hiccup. \
                             Nothing refused was inventoried, so a fixed build resumes cleanly",
                            report.refusal_rate() * 100.0,
                            report.attempted,
                            REFUSAL_RATE_LIMIT * 100.0
                        ));
                    }
                }
                Ok(Outcome::Empty) => {
                    report.empty += 1;
                    let mut record = empty_record(request, &rendered, now_ts);
                    record.fetch_millis = millis_of(elapsed);
                    if let Err(e) = inventory.append(&record) {
                        fatal = Some(e);
                    }
                }
                Ok(Outcome::Written {
                    bytes,
                    report: validation,
                }) => {
                    report.written += 1;
                    report.bytes_written += bytes;
                    let path = output_path(data_dir, request);
                    let relative = path
                        .strip_prefix(data_dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    // The file is on disk under its real name by now. Only
                    // here is an inventory line honest.
                    let mut record = InventoryRecord::new(
                        request.endpoint.path(),
                        &request.root,
                        "daily",
                        &render_date(request.date),
                        &render_date(request.date),
                        &rendered,
                        &relative,
                        &file_blake3(&path).unwrap_or_default(),
                        bytes,
                        &validation,
                        None,
                        now_ts,
                    );
                    record.fetch_millis = millis_of(elapsed);
                    if let Err(e) = inventory.append(&record) {
                        fatal = Some(e);
                    }
                }
            }
        });

        if let Some(e) = fatal {
            report.write_exit(data_dir, now_ts, ExitKind::Failed, &e.to_string());
            return Err(e);
        }
        if let Some(reason) = report.halted.clone() {
            report.write_exit(data_dir, now_ts, ExitKind::Halted, &reason);
            return Ok(report);
        }
    }
    report.write_exit(data_dir, now_ts, ExitKind::Completed, "");
    Ok(report)
}

/// One fetched body, taken the rest of the way. Errors that end the run
/// propagate; everything else becomes an [`Outcome`].
fn process_one(
    body: Result<Vec<u8>, ThetaError>,
    request: &PlannedRequest,
    data_dir: &Path,
    golden: &BTreeSet<String>,
    rendered: &str,
    report: &mut RunReport,
) -> Result<Outcome, ThetaError> {
    let body = match body {
        Ok(body) => body,
        Err(e) if e.is_no_data() => return Ok(Outcome::Empty),
        // A tripped breaker means the Terminal has stopped answering; more
        // requests will not change that (D-0056).
        Err(e @ ThetaError::CircuitOpen { .. }) => return Err(e),
        Err(e) => return Ok(Outcome::Refused(e.to_string())),
    };

    let validated = match validate(request.endpoint, &body, rendered) {
        Ok(validated) => validated,
        Err(e) => return Ok(Outcome::Refused(e.to_string())),
    };

    // Golden-raw is kept from the same bytes that were just validated, before
    // any transcode, which is what makes it a fidelity reference rather than a
    // second copy of our own output (§6).
    if golden.contains(rendered) {
        let path = golden_path(data_dir, request);
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_ok()
            && std::fs::write(&path, &body).is_ok()
        {
            report.golden_kept += 1;
        }
    }

    let source = TranscodeSource {
        request: rendered.to_owned(),
        response_blake3: blake3::hash(&body).to_hex().to_string(),
    };
    match write_parquet(
        &validated,
        &source,
        &output_path(data_dir, request),
        rendered,
    ) {
        Ok(bytes) => Ok(Outcome::Written {
            bytes,
            report: validated.report,
        }),
        // A write failure that is not about this file — a full disk — would
        // recur on every subsequent request and the refusal-rate breaker will
        // catch it within 200 attempts.
        Err(e) => Ok(Outcome::Refused(e.to_string())),
    }
}

/// An inventory line for a request the vendor had nothing for.
fn empty_record(request: &PlannedRequest, rendered: &str, now_ts: i64) -> InventoryRecord {
    InventoryRecord::new(
        request.endpoint.path(),
        &request.root,
        "daily",
        &render_date(request.date),
        &render_date(request.date),
        rendered,
        // No file, stated as no file. Resume keys on the request, so this is
        // enough to stop the same empty question being asked every run.
        "",
        "",
        0,
        &ValidationReport::default(),
        None,
        now_ts,
    )
}

/// blake3 of a written file, for the inventory.
fn file_blake3(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(blake3::hash(&bytes).to_hex().to_string())
}

/// `YYYY-MM-DD`.
fn render_date(date: CivilDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

#[cfg(test)]
#[path = "run/tests.rs"]
mod tests;
