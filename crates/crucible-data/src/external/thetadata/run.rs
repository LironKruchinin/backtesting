//! The acquisition loop: fetch a chunk concurrently, then take each response
//! the rest of the way in request order.
//!
//! ```text
//! pacer ─► fetch ─► validate ─► transcode ─► staging ─► rename ─► inventory
//!   (8 in flight)   └────────── serial, in request order ──────────┘
//! ```
//!
//! The fetch half is concurrent and the processing half is not, which is the
//! right split: the first is IO-bound and the pacer already governs it, while
//! the second appends to one inventory file and must stay ordered. The first
//! version of this loop fetched serially and measured **0.84-0.96 req/s**
//! against a projection of ~5.3 — a serial caller simply cannot use concurrency
//! the pacer is willing to grant, and throughput collapses to 1/latency.
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

/// Requests handed to one `fetch_batch` call.
///
/// The pacer already caps in-flight requests at 8 and paces their launches, so
/// this is not a concurrency knob — it is how much work is queued before the
/// results are processed. 64 keeps the eight permits saturated across a spread
/// of response times without holding more than a few hundred MB of buffered
/// bodies (§7.1's largest T0 response is ~4 MB).
///
/// It also bounds the blast radius of a kill: a chunk's bodies are fetched
/// before any of them is written, so an interrupted run loses at most one
/// chunk's fetches — which resume re-does from the inventory diff for free.
pub const FETCH_BATCH: usize = 64;

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
    #[must_use]
    pub fn refusal_rate_is_systemic(&self) -> bool {
        self.attempted >= REFUSAL_MIN_SAMPLE && self.refusal_rate() > REFUSAL_RATE_LIMIT
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

    for chunk in outstanding.chunks(FETCH_BATCH) {
        // Fetch the chunk CONCURRENTLY, then process the results in request
        // order. The first version called `fetch` per request and measured
        // 0.84-0.96 req/s against a projection of ~5.3 — because a serial
        // caller cannot use concurrency the pacer is willing to grant, and
        // throughput collapses to 1/latency. Results come back in request
        // order, so ordering, resume and the refusal ledger are unchanged.
        let bodies =
            client.fetch_batch(&chunk.iter().map(|r| r.request.clone()).collect::<Vec<_>>());

        for (request, body) in chunk.iter().zip(bodies) {
            report.attempted += 1;
            progress(request, report.attempted, total);

            let rendered = request.request.render();
            let outcome = process_one(body, request, data_dir, &golden, &rendered, &mut report);

            match outcome {
                Err(e) => return Err(e),
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
                        return Ok(report);
                    }
                }
                Ok(Outcome::Empty) => {
                    report.empty += 1;
                    inventory.append(&empty_record(request, &rendered, now_ts))?;
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
                    inventory.append(&InventoryRecord::new(
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
                    ))?;
                }
            }
        }
    }
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
