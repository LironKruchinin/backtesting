//! Tranche expansion, resume, and the guards a run must clear before it runs.
//!
//! The Databento path has been proven twice now: **plan, then execute**, with
//! the plan printed and priced before anything is spent (D-0024). Both times
//! the mistake was caught in the plan rather than in the archive. ThetaData
//! costs no money per request, so the analogue of "overspend" is different —
//! filling a disk, hammering a Terminal, or acquiring a tranche nobody can
//! reconcile — but the shape of the protection is identical, and T0 fires only
//! through [`TranchePlan::dry_run_report`].
//!
//! ## Four properties this module owes its caller
//!
//! 1. **Expansion is deterministic and idempotent.** The same
//!    [`TrancheSpec`] produces a byte-identical request list, in the same
//!    order, on any machine and any run. Nothing here reads a clock, a
//!    directory listing, or a hash map's iteration order — the same discipline
//!    §2.2 imposes on result-affecting code, applied to acquisition because an
//!    archive whose contents depend on the order it was fetched in is one whose
//!    *rebuild* differs from its original.
//! 2. **Resume is strictly an inventory diff.** [`TranchePlan::outstanding`]
//!    subtracts what `inventory.jsonl` records and nothing else. Never a
//!    directory listing: a file half-written when a run died is present on disk
//!    and absent from the inventory, so a listing-based resume skips it
//!    silently and a diff-based one re-fetches it.
//! 3. **The disk guard refuses rather than fills.** Below
//!    [`MIN_FREE_BYTES`] free the run refuses to start; at
//!    [`STOP_AND_REPORT_BYTES`] of `external/thetadata/` total it stops and
//!    reports. A tranche that fills the volume takes the blitz's curated
//!    output and the results registry down with it, and those are not
//!    re-fetchable on a subscription clock.
//! 4. **A dry run is free and says everything.** Request count, projected
//!    bytes per tranche, what resume already holds, and which guard would
//!    bite — computed without a single vendor call.
//!
//! ## Projected bytes are labelled as projections
//!
//! The per-request size estimates come from §7.1's measured CSV days scaled by
//! §7.3's **measured** compression ratios, which differ per endpoint by 7.6×
//! between `greeks/eod` and `quote` (D-0057). They are still projections: chain
//! sizes grow through the sample, and 2024 is the dense end. The dry run prints
//! them as a projection with the assumption named, because a number that looks
//! measured and is not is worse than an obvious guess.

use std::collections::BTreeSet;

use super::client::Request;
use super::inventory::Inventory;
use super::schema::Endpoint;
use crate::calendar::{CivilDate, TradingDayCalendar};
use crate::ingest::window::{civil_from_days, days_from_civil};

/// Free space below which a run refuses to start.
///
/// `docs/THETADATA_PLAN.md` §6: the blitz's curated output, results and the
/// registry still need room, and they are on the same volume.
pub const MIN_FREE_BYTES: u64 = 400 * 1024 * 1024 * 1024;

/// Total `external/thetadata/` size at which a run stops and reports.
pub const STOP_AND_REPORT_BYTES: u64 = 1000 * 1024 * 1024 * 1024;

/// Hard ceiling on `external/thetadata/` (§6).
pub const CAP_BYTES: u64 = 1200 * 1024 * 1024 * 1024;

/// One endpoint's per-root-day size projection, in bytes of Parquet.
///
/// Derived from §7.1's measured CSV sizes divided by §7.3's measured
/// per-endpoint compression ratio, then averaged across the nine roots. A
/// projection, never a measurement of the tranche itself.
#[must_use]
pub fn projected_bytes_per_root_day(endpoint: Endpoint) -> u64 {
    match endpoint {
        // 14.50 MB CSV/day across 8 greeks-capable roots at 2.46x -> ~737 kB
        // per root-day.
        Endpoint::OptionGreeksEod => 737_000,
        // 7.09 MB across 9 roots at 4.28x -> ~184 kB.
        Endpoint::OptionEod => 184_000,
        // 3.27 MB across 9 roots at 5.79x -> ~63 kB.
        Endpoint::OptionOpenInterest => 63_000,
        // T1 shapes, from the measured VIX whole-chain 1m day scaled by the
        // eod-size ratio between roots. Coarse, and labelled as such.
        Endpoint::OptionQuote => 1_850_000,
        Endpoint::OptionGreeksFirstOrder => 579_000,
        Endpoint::OptionOhlc => 400_000,
        Endpoint::StockOhlc | Endpoint::StockQuote => 50_000,
    }
}

/// What to acquire: the roots, the endpoints, and the span.
///
/// Deliberately plain data with no interior mutability and no clock, so that
/// two runs holding equal specs expand to equal request lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrancheSpec {
    /// Human name, used in reports and in the inventory's `grain` field.
    pub name: String,
    /// Roots, in the order they will be fetched.
    pub roots: Vec<String>,
    /// First session to acquire, inclusive.
    pub start: CivilDate,
    /// Last session to acquire, exclusive.
    pub end: CivilDate,
    /// Endpoints to fetch, and the greeks floor gate for each.
    pub endpoints: Vec<EndpointSpec>,
}

/// One endpoint within a tranche, and when it applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointSpec {
    /// Which endpoint.
    pub endpoint: Endpoint,
    /// Extra query parameters, in declaration order.
    pub extra: Vec<(String, String)>,
    /// Only request on or after this root's greeks floor.
    ///
    /// `true` for `greeks/*`, which answer 472 below a root's floor (§3.4).
    /// Requesting below it is not harmful — a 472 is an ordinary outcome — but
    /// it is thousands of pointless requests per root, and a plan that asks for
    /// work it knows will be empty is a plan that cannot be read.
    pub above_greeks_floor_only: bool,
    /// Only request *below* the floor, for the endpoint that substitutes there.
    pub below_greeks_floor_only: bool,
}

/// One unit of acquisition work: a request plus where its output belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRequest {
    /// Root this is for.
    pub root: String,
    /// The session it covers.
    pub date: CivilDate,
    /// Which endpoint.
    pub endpoint: Endpoint,
    /// The rendered request, and the resume key.
    pub request: Request,
    /// Projected Parquet bytes.
    pub projected_bytes: u64,
}

/// A fully expanded tranche, ready to be dry-run or executed.
#[derive(Debug, Clone)]
pub struct TranchePlan {
    /// The spec this came from.
    pub spec: TrancheSpec,
    /// Every request, in deterministic order.
    pub requests: Vec<PlannedRequest>,
}

/// Per-root greeks floors, as measured by `crucible theta-floors` (§3.4).
///
/// A root with no entry is treated as having greeks for the whole span, which
/// is the permissive direction: the worst case is some 472s that cost nothing
/// but a request each, rather than a silently skipped year.
#[must_use]
pub fn measured_greeks_floor(root: &str) -> Option<CivilDate> {
    let date = |year: i64, month: u32, day: u32| Some(CivilDate { year, month, day });
    match root {
        "SPY" | "IWM" | "SPX" | "SPXW" | "VIX" | "DIA" => date(2017, 1, 3),
        "RUT" => date(2018, 12, 3),
        "NDX" => date(2026, 5, 8),
        // QQQ has greeks at or below the subscription floor.
        _ => None,
    }
}

impl TrancheSpec {
    /// Expands into every request, in deterministic order.
    ///
    /// Order is `root` (as given) → `date` ascending → `endpoint` (as given).
    /// Fixed and documented because it is the order a resumed run replays in,
    /// and two runs that disagreed about it would produce inventories that
    /// could not be diffed against each other.
    ///
    /// Sessions come from the calendar, so a plan never asks for a weekend.
    #[must_use]
    pub fn expand(&self, calendar: &TradingDayCalendar) -> TranchePlan {
        let mut requests = Vec::new();
        for root in &self.roots {
            let floor = measured_greeks_floor(root);
            let mut day = self.start;
            while days_from_civil(day) < days_from_civil(self.end) {
                if calendar.is_trading_day(day) {
                    for spec in &self.endpoints {
                        let above =
                            floor.is_none_or(|f| days_from_civil(day) >= days_from_civil(f));
                        if spec.above_greeks_floor_only && !above {
                            continue;
                        }
                        if spec.below_greeks_floor_only && above {
                            continue;
                        }
                        requests.push(PlannedRequest {
                            root: root.clone(),
                            date: day,
                            endpoint: spec.endpoint,
                            request: build_request(root, day, spec),
                            projected_bytes: projected_bytes_per_root_day(spec.endpoint),
                        });
                    }
                }
                day = civil_from_days(days_from_civil(day) + 1);
            }
        }
        TranchePlan {
            spec: self.clone(),
            requests,
        }
    }
}

/// Renders one request. Parameter order is fixed, because the rendered string
/// is the resume key and a reordered query would look like new work.
fn build_request(root: &str, date: CivilDate, spec: &EndpointSpec) -> Request {
    let compact = format!("{:04}{:02}{:02}", date.year, date.month, date.day);
    let mut request = Request::new(spec.endpoint.path())
        .with("symbol", root)
        .with("expiration", "*")
        .with("start_date", compact.clone())
        .with("end_date", compact);
    for (key, value) in &spec.extra {
        request = request.with(key.clone(), value.clone());
    }
    request
}

/// What a dry run reports, and what the guards say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunReport {
    /// Tranche name.
    pub name: String,
    /// Total requests the spec expands to.
    pub total_requests: u64,
    /// Requests the inventory already records.
    pub already_held: u64,
    /// Requests this run would actually make.
    pub outstanding: u64,
    /// Projected Parquet bytes for the outstanding requests.
    pub projected_bytes: u64,
    /// Per-endpoint breakdown: (endpoint path, outstanding, projected bytes).
    pub by_endpoint: Vec<(&'static str, u64, u64)>,
    /// Free bytes observed on the target volume, when it could be measured.
    pub free_bytes: Option<u64>,
    /// Bytes `external/thetadata/` already holds.
    pub existing_bytes: u64,
    /// Why the run must not start, if it must not.
    pub refusal: Option<String>,
}

impl DryRunReport {
    /// True when every guard is satisfied and execution may proceed.
    #[must_use]
    pub fn may_execute(&self) -> bool {
        self.refusal.is_none()
    }
}

impl core::fmt::Display for DryRunReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "tranche {} — DRY RUN, nothing fetched", self.name)?;
        writeln!(
            f,
            "  {:>10} requests expand from the spec",
            self.total_requests
        )?;
        writeln!(
            f,
            "  {:>10} already in the inventory (resume subtracts these)",
            self.already_held
        )?;
        writeln!(f, "  {:>10} outstanding", self.outstanding)?;
        writeln!(f)?;
        writeln!(f, "  projected Parquet, per endpoint:")?;
        for (path, count, bytes) in &self.by_endpoint {
            writeln!(
                f,
                "    {path:<40} {count:>8} req  {:>10}",
                human_bytes(*bytes)
            )?;
        }
        writeln!(
            f,
            "    {:<40} {:>8} req  {:>10}",
            "TOTAL",
            self.outstanding,
            human_bytes(self.projected_bytes)
        )?;
        writeln!(f)?;
        writeln!(
            f,
            "  Projections, not measurements: §7.1 CSV sizes over §7.3's measured"
        )?;
        writeln!(
            f,
            "  per-endpoint ratios, at 2024 chain sizes — the dense end of the sample."
        )?;
        writeln!(f)?;
        writeln!(
            f,
            "  existing external/thetadata/  {}",
            human_bytes(self.existing_bytes)
        )?;
        match self.free_bytes {
            Some(free) => writeln!(f, "  free on the volume        {}", human_bytes(free))?,
            None => writeln!(f, "  free on the volume        unmeasured")?,
        }
        match &self.refusal {
            Some(why) => writeln!(f, "\n  REFUSED: {why}")?,
            None => writeln!(f, "\n  guards clear — safe to execute")?,
        }
        Ok(())
    }
}

impl TranchePlan {
    /// Requests the inventory does not already record, in plan order.
    ///
    /// # Errors
    /// [`ThetaError::Io`](super::ThetaError::Io) if the inventory cannot be
    /// read. A resume that cannot read the inventory must fail rather than
    /// assume nothing is held — assuming would re-fetch a whole tranche, and
    /// assuming the opposite would skip one.
    pub fn outstanding(
        &self,
        inventory: &Inventory,
    ) -> Result<Vec<PlannedRequest>, super::ThetaError> {
        let held: BTreeSet<String> = inventory.completed_requests()?.into_iter().collect();
        Ok(self
            .requests
            .iter()
            .filter(|r| !held.contains(&r.request.render()))
            .cloned()
            .collect())
    }

    /// Everything a dry run reports, computed without one vendor call.
    ///
    /// # Errors
    /// [`ThetaError::Io`](super::ThetaError::Io) if the inventory cannot be
    /// read.
    pub fn dry_run_report(
        &self,
        inventory: &Inventory,
        existing_bytes: u64,
        free_bytes: Option<u64>,
    ) -> Result<DryRunReport, super::ThetaError> {
        let outstanding = self.outstanding(inventory)?;
        let projected: u64 = outstanding.iter().map(|r| r.projected_bytes).sum();

        let mut by_endpoint: Vec<(&'static str, u64, u64)> = Vec::new();
        for request in &outstanding {
            let path = request.endpoint.path();
            match by_endpoint.iter_mut().find(|(p, _, _)| *p == path) {
                Some(entry) => {
                    entry.1 += 1;
                    entry.2 += request.projected_bytes;
                }
                None => by_endpoint.push((path, 1, request.projected_bytes)),
            }
        }

        let refusal = guard(existing_bytes, free_bytes, projected);
        Ok(DryRunReport {
            name: self.spec.name.clone(),
            total_requests: self.requests.len() as u64,
            already_held: self.requests.len() as u64 - outstanding.len() as u64,
            outstanding: outstanding.len() as u64,
            projected_bytes: projected,
            by_endpoint,
            free_bytes,
            existing_bytes,
            refusal,
        })
    }
}

/// The disk guards, in the order they bite.
///
/// Checked against the projection rather than only against what is already on
/// disk: a run that would clear the floor on its first file and cross it on its
/// last has not passed, and finding out mid-tranche means a half-acquired
/// tranche plus a full volume.
fn guard(existing_bytes: u64, free_bytes: Option<u64>, projected: u64) -> Option<String> {
    if existing_bytes >= STOP_AND_REPORT_BYTES {
        return Some(format!(
            "external/thetadata/ holds {} and the stop-and-report threshold is {} (§6). \
             Report before acquiring more; the hard cap is {}",
            human_bytes(existing_bytes),
            human_bytes(STOP_AND_REPORT_BYTES),
            human_bytes(CAP_BYTES)
        ));
    }
    if existing_bytes.saturating_add(projected) > CAP_BYTES {
        return Some(format!(
            "{} held plus {} projected exceeds the {} cap (§6)",
            human_bytes(existing_bytes),
            human_bytes(projected),
            human_bytes(CAP_BYTES)
        ));
    }
    match free_bytes {
        // Unmeasurable free space is not "fine": the guard exists precisely
        // for the case nobody is watching, and a run that cannot check must
        // not proceed on the assumption that it would have passed.
        None => Some(
            "free space on the target volume could not be measured, so the \
             400 GB floor cannot be checked (§6)"
                .to_owned(),
        ),
        Some(free) if free < MIN_FREE_BYTES => Some(format!(
            "{} free is already below the {} floor (§6)",
            human_bytes(free),
            human_bytes(MIN_FREE_BYTES)
        )),
        Some(free) if free.saturating_sub(projected) < MIN_FREE_BYTES => Some(format!(
            "{} free minus {} projected would leave less than the {} floor (§6)",
            human_bytes(free),
            human_bytes(projected),
            human_bytes(MIN_FREE_BYTES)
        )),
        Some(_) => None,
    }
}

/// Bytes as a human-readable string. Display only.
#[must_use]
pub fn human_bytes(bytes: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "display only; the imprecision starts far above any real archive"
    )]
    let b = bytes as f64;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MiB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.1} kiB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// The T0 tranche, exactly as `docs/THETADATA_PLAN.md` §2 specifies it.
///
/// `greeks/eod` above each root's floor, `eod` **always** (it is the
/// reconciliation partner above the floor and the only source below it), and
/// `open_interest` always.
#[must_use]
pub fn t0(start: CivilDate, end: CivilDate) -> TrancheSpec {
    TrancheSpec {
        name: "T0".to_owned(),
        roots: [
            "SPY", "QQQ", "IWM", "SPX", "SPXW", "NDX", "VIX", "RUT", "DIA",
        ]
        .iter()
        .map(|r| (*r).to_owned())
        .collect(),
        start,
        end,
        endpoints: vec![
            EndpointSpec {
                endpoint: Endpoint::OptionEod,
                extra: Vec::new(),
                above_greeks_floor_only: false,
                below_greeks_floor_only: false,
            },
            EndpointSpec {
                endpoint: Endpoint::OptionGreeksEod,
                extra: Vec::new(),
                above_greeks_floor_only: true,
                below_greeks_floor_only: false,
            },
            EndpointSpec {
                endpoint: Endpoint::OptionOpenInterest,
                extra: Vec::new(),
                above_greeks_floor_only: false,
                below_greeks_floor_only: false,
            },
        ],
    }
}

#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
