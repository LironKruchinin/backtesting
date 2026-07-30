//! Turning "I want this data" into "these exact windows must be bought".
//!
//! Planning is pure except for one free provider call
//! ([`BatchProvider::dataset_range`]), and it is where the archive's
//! don't-pay-twice guarantee is actually applied: the requested range is
//! reduced by what the manifest already records before anything is priced.
//!
//! ## Order of operations, and why
//!
//! 1. **Reject unaligned windows.** Both bounds must be UTC midnight. This
//!    is not fussiness — the archive path template names a window, and an
//!    unaligned window has no unambiguous name (see [`super::window`]).
//! 2. **Reject a requested key that cannot be a path component**, via
//!    [`check_symbol_key`](crate::catalog::check_symbol_key). This module owns
//!    the check because this module is the only place a symbol becomes a
//!    directory name — `raw/{dataset}/{schema}/{key}/{stem}.dbn.zst` — and the
//!    check runs before the (free) provider call, so an unarchivable key costs
//!    nothing at all. It used to happen incidentally inside
//!    [`Catalog::coverage`](crate::catalog::Catalog::coverage), which had to
//!    stop banning spaces so that vendor spread names could be recorded and
//!    credited at all (D-0066); the guard moved here rather than disappearing.
//! 3. **Clip to the provider's dataset range**, fetched live rather than
//!    hardcoded. `mbo` begins in 2017 while everything else begins in 2010;
//!    baking those dates into the source means they are wrong the first time
//!    the vendor extends history, and a request for data that does not exist
//!    is an error worth catching before it is priced.
//! 4. **Subtract what we own**, via
//!    [`Catalog::coverage`](crate::catalog::Catalog::coverage) — which still
//!    validates the request (D-0020), because an unusable symbol matches no
//!    record and would otherwise report the whole range as missing and re-buy
//!    it.
//! 5. **Cut the remaining gaps into month-aligned windows** and name them.
//! 6. **Verify the windows tile the gaps exactly.** Pure, free, and the
//!    cheapest possible place to catch an off-by-one in step 5: a plan that
//!    skips an hour buys an archive with a hole, and a plan that overlaps
//!    buys the same bytes twice.
//!
//! Step 6 deserves its own note. It is tempting to check window splitting by
//! comparing the sum of per-window quotes against one aggregate quote from
//! the vendor. That costs a network round trip to test arithmetic we already
//! control, and it cannot distinguish a splitting bug from a pricing
//! subtlety. The invariant check here is deterministic, instant, and
//! strictly more precise.

use std::collections::BTreeMap;

use crate::catalog::{Catalog, CoverageRequest, TsRange, check_symbol_key};
use crate::ingest::error::IngestError;
use crate::ingest::provider::{BatchProvider, QuoteQuery, StypeIn};
use crate::ingest::window;
use crucible_core::types::Ts;

/// How wide a single download window — and therefore a single batch job, a
/// single archive file, and a single manifest row — is allowed to be.
///
/// This is a job-granularity choice, and the vendor's documented behaviour
/// makes it a consequential one. Batch jobs have **no size limit**, are
/// processed **one at a time in a FIFO queue** (so sharding buys no
/// parallelism), are capped at **20 submissions per minute**, and have no
/// idempotency key, no cancel, and no refund. Month windows over a 16-year
/// backfill for seven parents across four schemas would be 5,404 separate
/// purchases; [`WindowSpan::Whole`] makes the same backfill about 28.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowSpan {
    /// One window per calendar month. The recurring archival job's shape:
    /// small, restartable, and naturally aligned to the monthly cron.
    #[default]
    Month,
    /// One window per contiguous coverage gap, however long. The bootstrap
    /// backfill's shape.
    Whole,
}

impl core::fmt::Display for WindowSpan {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            WindowSpan::Month => "month",
            WindowSpan::Whole => "whole",
        })
    }
}

/// What the operator asked for, before the archive has its say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// Vendor dataset, e.g. `GLBX.MDP3`.
    pub dataset: String,
    /// Vendor schema, e.g. `ohlcv-1m`.
    pub schema: String,
    /// Symbol keys to acquire. For parent symbology these are the parents
    /// (`ES.FUT`), not the contracts they expand to.
    pub symbols: Vec<String>,
    /// How to interpret `symbols`.
    pub stype_in: StypeIn,
    /// Half-open range requested, UTC-midnight aligned on both ends.
    pub range: TsRange,
    /// How finely the remaining gaps are cut into download windows.
    pub window_span: WindowSpan,
}

/// One window that would actually be downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedJob {
    /// Requested symbol key this window belongs to.
    pub key: String,
    /// Half-open window to request.
    pub window: TsRange,
    /// Relative archive path the payload is destined for.
    pub target_file_path: String,
}

impl PlannedJob {
    /// The provider request this window corresponds to.
    #[must_use]
    pub fn quote_query(&self, dataset: &str, schema: &str, stype_in: StypeIn) -> QuoteQuery {
        QuoteQuery {
            dataset: dataset.to_string(),
            schema: schema.to_string(),
            symbols: vec![self.key.clone()],
            stype_in,
            range: self.window,
        }
    }
}

/// The result of planning: what to buy, what we already have, and what the
/// provider would not have served anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullPlan {
    /// Vendor dataset.
    pub dataset: String,
    /// Vendor schema.
    pub schema: String,
    /// Symbology of the request.
    pub stype_in: StypeIn,
    /// Range as requested by the operator.
    pub requested: TsRange,
    /// Range after intersecting with the provider's dataset range. Equal to
    /// `requested` when no clipping occurred.
    pub effective: TsRange,
    /// Windows to download, ordered by key then time.
    pub jobs: Vec<PlannedJob>,
    /// Intervals already recorded in the manifest, per key — the part of the
    /// request that costs nothing because we own it.
    pub already_owned: BTreeMap<String, Vec<TsRange>>,
}

impl PullPlan {
    /// True when the archive already covers everything requested.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Whether the provider's dataset range narrowed the request.
    #[must_use]
    pub fn was_clipped(&self) -> bool {
        self.effective != self.requested
    }
}

/// Plans a pull: what must be bought, given what we already own.
///
/// The only provider call is [`BatchProvider::dataset_range`], which is free
/// and spends nothing.
///
/// # Errors
/// - [`IngestError::InvalidWindow`] if the requested range is not
///   UTC-midnight aligned on both ends.
/// - [`IngestError::Catalog`] if a requested symbol key could not be a path
///   component (checked before the provider call, so it costs nothing).
/// - [`IngestError::Provider`] if the dataset range cannot be fetched.
///   Planning refuses rather than assuming a range.
/// - [`IngestError::WindowOutsideDatasetRange`] if nothing requested is
///   served.
/// - [`IngestError::Catalog`] if the coverage request is invalid (D-0020).
/// - [`IngestError::PlanNotTiled`] if window splitting produced windows that
///   do not exactly tile the coverage gaps.
pub fn plan(
    catalog: &Catalog,
    provider: &mut dyn BatchProvider,
    request: &PullRequest,
) -> Result<PullPlan, IngestError> {
    if !window::is_day_aligned(request.range.start_ts())
        || !window::is_day_aligned(request.range.end_ts())
    {
        return Err(IngestError::InvalidWindow {
            requested: format!(
                "[{}, {})",
                window::date_of(request.range.start_ts()),
                window::date_of(request.range.end_ts())
            ),
            reason: "both bounds must be UTC midnight; the archive path \
                     template cannot name a sub-day window unambiguously",
        });
    }

    // Every requested symbol becomes a `{key}` directory below, so every one of
    // them must survive the path-component rule. Before the provider call by
    // design: a key that cannot be archived must cost nothing, not a quote.
    for key in &request.symbols {
        check_symbol_key(key)?;
    }

    let available = provider
        .dataset_range(&request.dataset, &request.schema)
        .map_err(|source| IngestError::Provider {
            during: "fetching the dataset range",
            source,
        })?;

    let effective = intersect(request.range, available).ok_or_else(|| {
        IngestError::WindowOutsideDatasetRange {
            dataset: request.dataset.clone(),
            requested: request.range,
            available,
        }
    })?;

    let gaps = catalog.coverage(&CoverageRequest {
        dataset: request.dataset.clone(),
        schema: request.schema.clone(),
        symbols: request.symbols.clone(),
        range: effective,
    })?;

    let mut jobs = Vec::new();
    let mut already_owned = BTreeMap::new();

    for (key, key_gaps) in &gaps {
        let mut key_jobs = Vec::new();
        for gap in key_gaps {
            let windows = match request.window_span {
                WindowSpan::Month => window::split_into_month_windows(gap.start_ts(), gap.end_ts()),
                // A gap is already contiguous and day-aligned, so it is its
                // own window; `window_file_stem` names it with the
                // `{yyyy-mm-dd}--{yyyy-mm-dd}` form unless it happens to be
                // exactly one calendar month (D-0026).
                WindowSpan::Whole => vec![(gap.start_ts(), gap.end_ts())],
            };
            for (start_ts, end_ts) in windows {
                let stem = window::window_file_stem(start_ts, end_ts);
                key_jobs.push(PlannedJob {
                    key: key.clone(),
                    window: TsRange::new(start_ts, end_ts)?,
                    target_file_path: format!(
                        "raw/{}/{}/{}/{}.dbn.zst",
                        request.dataset, request.schema, key, stem
                    ),
                });
            }
        }
        verify_tiling(key, key_gaps, &key_jobs)?;
        already_owned.insert(key.clone(), complement(effective, key_gaps));
        jobs.extend(key_jobs);
    }

    Ok(PullPlan {
        dataset: request.dataset.clone(),
        schema: request.schema.clone(),
        stype_in: request.stype_in,
        requested: request.range,
        effective,
        jobs,
        already_owned,
    })
}

/// Overlap of two half-open ranges, or `None` when they are disjoint.
fn intersect(a: TsRange, b: TsRange) -> Option<TsRange> {
    let start = a.start_ts().max(b.start_ts());
    let end = a.end_ts().min(b.end_ts());
    if start < end {
        // Non-empty by the check above, so `new` cannot fail.
        TsRange::new(start, end).ok()
    } else {
        None
    }
}

/// `outer` minus `holes`, where `holes` is sorted, disjoint, and contained in
/// `outer` — exactly what `Catalog::coverage` returns.
fn complement(outer: TsRange, holes: &[TsRange]) -> Vec<TsRange> {
    let mut out = Vec::new();
    let mut cursor = outer.start_ts();
    for hole in holes {
        if hole.start_ts() > cursor
            && let Ok(range) = TsRange::new(cursor, hole.start_ts())
        {
            out.push(range);
        }
        cursor = cursor.max(hole.end_ts());
    }
    if cursor < outer.end_ts()
        && let Ok(range) = TsRange::new(cursor, outer.end_ts())
    {
        out.push(range);
    }
    out
}

/// Checks that `jobs` exactly tile `gaps`: same total duration, each window
/// inside some gap, and no two windows overlapping.
///
/// A violation is an internal bug, not user error, which is why the message
/// says so — but it is checked at runtime anyway, because the consequence of
/// shipping it is a paid download that either double-buys or leaves a hole
/// the manifest will thereafter claim is covered.
fn verify_tiling(key: &str, gaps: &[TsRange], jobs: &[PlannedJob]) -> Result<(), IngestError> {
    let gap_total: i128 = gaps
        .iter()
        .map(|g| i128::from(g.end_ts().0 - g.start_ts().0))
        .sum();
    let job_total: i128 = jobs
        .iter()
        .map(|j| i128::from(j.window.end_ts().0 - j.window.start_ts().0))
        .sum();
    if gap_total != job_total {
        return Err(IngestError::PlanNotTiled {
            key: key.to_string(),
            detail: format!("gaps total {gap_total} ns but planned windows total {job_total} ns"),
        });
    }

    for job in jobs {
        let inside_a_gap = gaps
            .iter()
            .any(|g| g.start_ts() <= job.window.start_ts() && job.window.end_ts() <= g.end_ts());
        if !inside_a_gap {
            return Err(IngestError::PlanNotTiled {
                key: key.to_string(),
                detail: format!("window {} is not inside any gap", job.target_file_path),
            });
        }
    }

    for pair in jobs.windows(2) {
        if pair[0].window.end_ts() > pair[1].window.start_ts() {
            return Err(IngestError::PlanNotTiled {
                key: key.to_string(),
                detail: format!(
                    "windows {} and {} overlap",
                    pair[0].target_file_path, pair[1].target_file_path
                ),
            });
        }
    }

    Ok(())
}

/// Convenience for building a day-aligned range from civil dates.
///
/// # Errors
/// [`IngestError::InvalidWindow`] if the dates do not form a non-empty range.
pub fn range_from_dates(
    start: (i64, u32, u32),
    end: (i64, u32, u32),
) -> Result<TsRange, IngestError> {
    let to_ts = |(year, month, day): (i64, u32, u32)| -> Ts {
        window::start_of(window::CivilDate { year, month, day })
    };
    TsRange::new(to_ts(start), to_ts(end)).map_err(|_| IngestError::InvalidWindow {
        requested: format!(
            "[{:04}-{:02}-{:02}, {:04}-{:02}-{:02})",
            start.0, start.1, start.2, end.0, end.1, end.2
        ),
        reason: "end must be strictly after start",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Acquisition;
    use crate::ingest::provider::fake::FakeProvider;
    use crate::testutil::TempDir;

    fn dates(start: (i64, u32, u32), end: (i64, u32, u32)) -> TsRange {
        range_from_dates(start, end).expect("valid test range")
    }

    /// GLBX.MDP3's real availability, so clipping tests exercise the shape
    /// planning will actually meet.
    fn glbx_range() -> TsRange {
        dates((2010, 6, 6), (2026, 7, 24))
    }

    fn empty_catalog(dir: &TempDir) -> Catalog {
        Catalog::open(dir.path()).expect("open empty catalog")
    }

    fn request(symbols: &[&str], range: TsRange) -> PullRequest {
        PullRequest {
            dataset: "GLBX.MDP3".to_string(),
            schema: "ohlcv-1m".to_string(),
            symbols: symbols.iter().map(|s| (*s).to_string()).collect(),
            stype_in: StypeIn::Parent,
            range,
            window_span: WindowSpan::Month,
        }
    }

    /// Records an owned slice so coverage has something to subtract.
    fn own(catalog: &mut Catalog, dir: &TempDir, key: &str, range: TsRange, stem: &str) {
        let rel = format!("raw/GLBX.MDP3/ohlcv-1m/{key}/{stem}.dbn.zst");
        let abs = dir.path().join(&rel);
        std::fs::create_dir_all(abs.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&abs, b"owned bytes").expect("write fixture");
        catalog
            .append(Acquisition {
                dataset: "GLBX.MDP3".to_string(),
                schema: "ohlcv-1m".to_string(),
                symbols: vec![key.to_string()],
                range,
                acquired_ts: Ts(1_700_000_000_000_000_000),
                databento_job_id: "test-job".to_string(),
                file_path: rel,
            })
            .expect("append owned slice");
    }

    #[test]
    fn a_requested_key_that_cannot_be_a_path_is_refused_before_the_provider() {
        // The money guard, and the planted control for D-0066's narrowing: the
        // *key* becomes `{key}` in `raw/{dataset}/{schema}/{key}/{stem}.dbn.zst`,
        // so a key carrying a space, a separator or a colon must stop the pull —
        // and stop it before even the free provider call, let alone a quote.
        // (Before D-0066 this happened incidentally inside `coverage`, which now
        // has to accept spaces so vendor spread names can be credited at all.)
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        for bad_key in ["CL:BF F0-G0-H0", "ES FUT", "ES/FUT", "ES:FUT", ".."] {
            let mut provider = FakeProvider::new(glbx_range());
            let result = plan(
                &catalog,
                &mut provider,
                &request(&[bad_key], dates((2024, 1, 1), (2024, 2, 1))),
            );
            assert!(
                matches!(result, Err(IngestError::Catalog(_))),
                "key {bad_key:?} must be refused, got {result:?}"
            );
            assert!(
                provider.calls.is_empty(),
                "key {bad_key:?} must be refused before the provider is touched, \
                 calls: {:?}",
                provider.calls
            );
        }
        // A second key being bad refuses the whole plan, not just its own share.
        let mut provider = FakeProvider::new(glbx_range());
        assert!(matches!(
            plan(
                &catalog,
                &mut provider,
                &request(&["ES.FUT", "ES FUT"], dates((2024, 1, 1), (2024, 2, 1))),
            ),
            Err(IngestError::Catalog(_))
        ));
        assert!(provider.calls.is_empty());
    }

    #[test]
    fn empty_archive_plans_the_whole_request_in_month_windows() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(glbx_range());

        let plan = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2024, 1, 1), (2024, 4, 1))),
        )
        .expect("plan succeeds");

        assert_eq!(plan.jobs.len(), 3, "Jan, Feb, Mar");
        assert_eq!(
            plan.jobs
                .iter()
                .map(|j| j.target_file_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-02.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-03.dbn.zst",
            ]
        );
        assert!(!plan.was_clipped());
        // Planning prices nothing and buys nothing: the only provider call
        // it is allowed to make is the free dataset-range lookup.
        assert!(provider.spent_nothing());
        assert_eq!(provider.calls, vec!["dataset_range:GLBX.MDP3/ohlcv-1m"]);
    }

    // The don't-pay-twice property, end to end: own February, plan the
    // quarter, get January and March only.
    #[test]
    fn planning_subtracts_what_the_manifest_already_records() {
        let dir = TempDir::new();
        let mut catalog = empty_catalog(&dir);
        own(
            &mut catalog,
            &dir,
            "ES.FUT",
            dates((2024, 2, 1), (2024, 3, 1)),
            "2024-02",
        );
        let mut provider = FakeProvider::new(glbx_range());

        let plan = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2024, 1, 1), (2024, 4, 1))),
        )
        .expect("plan succeeds");

        assert_eq!(
            plan.jobs
                .iter()
                .map(|j| j.target_file_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-03.dbn.zst",
            ]
        );
        assert_eq!(
            plan.already_owned.get("ES.FUT"),
            Some(&vec![dates((2024, 2, 1), (2024, 3, 1))])
        );
    }

    #[test]
    fn a_fully_owned_request_plans_nothing() {
        let dir = TempDir::new();
        let mut catalog = empty_catalog(&dir);
        own(
            &mut catalog,
            &dir,
            "ES.FUT",
            dates((2024, 1, 1), (2024, 2, 1)),
            "2024-01",
        );
        let mut provider = FakeProvider::new(glbx_range());

        let plan = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2024, 1, 1), (2024, 2, 1))),
        )
        .expect("plan succeeds");

        assert!(plan.is_empty(), "nothing left to buy");
        assert_eq!(plan.jobs.len(), 0);
    }

    // Availability is fetched, not hardcoded: a request reaching back before
    // the dataset exists is clipped rather than bought.
    #[test]
    fn requests_are_clipped_to_the_live_dataset_range() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(glbx_range());

        let plan = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2010, 5, 1), (2010, 8, 1))),
        )
        .expect("plan succeeds");

        assert!(plan.was_clipped());
        assert_eq!(plan.effective, dates((2010, 6, 6), (2010, 8, 1)));
        // June is partial (starts the 6th) so it is NOT named 2010-06.
        assert_eq!(
            plan.jobs
                .iter()
                .map(|j| j.target_file_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2010-06-06--2010-07-01.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2010-07.dbn.zst",
            ]
        );
    }

    // The mbo case: asking for 16 years of a schema that starts in 2017.
    #[test]
    fn a_request_entirely_before_the_dataset_starts_is_an_error() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(dates((2017, 5, 21), (2026, 7, 24)));

        let err = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2010, 6, 6), (2017, 1, 1))),
        )
        .expect_err("must refuse");

        assert!(matches!(err, IngestError::WindowOutsideDatasetRange { .. }));
    }

    #[test]
    fn unaligned_windows_are_refused_before_anything_is_priced() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(glbx_range());

        let start = window::start_of(window::CivilDate {
            year: 2024,
            month: 1,
            day: 1,
        });
        let unaligned =
            TsRange::new(Ts(start.0 + 1), Ts(start.0 + window::NANOS_PER_DAY)).expect("range");

        let err = plan(&catalog, &mut provider, &request(&["ES.FUT"], unaligned))
            .expect_err("must refuse");

        assert!(matches!(err, IngestError::InvalidWindow { .. }));
        // Refused before even asking the provider for a range.
        assert!(provider.calls.is_empty());
    }

    // A provider that cannot say what it serves is a refusal, never an
    // assumption that everything is available.
    #[test]
    fn an_unavailable_dataset_range_is_a_refusal() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(glbx_range());
        provider.dataset_range = None;

        let err = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2024, 1, 1), (2024, 2, 1))),
        )
        .expect_err("must refuse");

        assert!(matches!(err, IngestError::Provider { .. }));
    }

    // D-0020 reaching through: a malformed symbol must be an error, not a
    // confident "you own none of it" that funds a download.
    #[test]
    fn a_malformed_symbol_is_an_error_not_a_full_gap() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(glbx_range());

        let err = plan(
            &catalog,
            &mut provider,
            &request(&["ES FUT"], dates((2024, 1, 1), (2024, 2, 1))),
        )
        .expect_err("must refuse");

        assert!(matches!(err, IngestError::Catalog(_)));
    }

    #[test]
    fn multiple_keys_are_planned_independently_and_ordered() {
        let dir = TempDir::new();
        let mut catalog = empty_catalog(&dir);
        own(
            &mut catalog,
            &dir,
            "NQ.FUT",
            dates((2024, 1, 1), (2024, 2, 1)),
            "2024-01",
        );
        let mut provider = FakeProvider::new(glbx_range());

        let plan = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT", "NQ.FUT"], dates((2024, 1, 1), (2024, 3, 1))),
        )
        .expect("plan succeeds");

        assert_eq!(
            plan.jobs
                .iter()
                .map(|j| j.target_file_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-02.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/NQ.FUT/2024-02.dbn.zst",
            ]
        );
    }

    // A one-day validation slice and a later full-month pull of the same
    // month must not collide on file_path -- the second would fail in
    // Catalog::append AFTER being paid for.
    #[test]
    fn a_partial_slice_does_not_block_a_later_full_month() {
        let dir = TempDir::new();
        let mut catalog = empty_catalog(&dir);
        own(
            &mut catalog,
            &dir,
            "ES.FUT",
            dates((2024, 1, 2), (2024, 1, 3)),
            "2024-01-02--2024-01-03",
        );
        let mut provider = FakeProvider::new(glbx_range());

        let plan = plan(
            &catalog,
            &mut provider,
            &request(&["ES.FUT"], dates((2024, 1, 1), (2024, 2, 1))),
        )
        .expect("plan succeeds");

        let paths: Vec<&str> = plan
            .jobs
            .iter()
            .map(|j| j.target_file_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01-01--2024-01-02.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01-03--2024-02-01.dbn.zst",
            ]
        );
        assert!(
            !paths.contains(&"raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"),
            "must not reuse the whole-month name over an already-owned day"
        );
    }

    // The backfill shape: 5,404 month-sized purchases collapse to one job per
    // contiguous gap. Sharding buys nothing -- the vendor processes jobs one
    // at a time -- and every extra submission is another chance to buy twice.
    #[test]
    fn whole_span_emits_one_window_per_contiguous_gap() {
        let dir = TempDir::new();
        let mut catalog = empty_catalog(&dir);
        own(
            &mut catalog,
            &dir,
            "ES.FUT",
            dates((2018, 3, 1), (2018, 4, 1)),
            "2018-03",
        );
        let mut provider = FakeProvider::new(glbx_range());

        let mut req = request(&["ES.FUT"], dates((2010, 6, 6), (2026, 7, 1)));
        req.window_span = WindowSpan::Whole;
        let whole_plan = plan(&catalog, &mut provider, &req).expect("plan succeeds");

        // One owned month splits the 16 years into exactly two gaps, so
        // exactly two jobs -- against 193 under `Month`.
        assert_eq!(
            whole_plan
                .jobs
                .iter()
                .map(|j| j.target_file_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2010-06-06--2018-03-01.dbn.zst",
                "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2018-04-01--2026-07-01.dbn.zst",
            ]
        );

        let mut month_req = request(&["ES.FUT"], dates((2010, 6, 6), (2026, 7, 1)));
        month_req.window_span = WindowSpan::Month;
        let month_plan = plan(&catalog, &mut provider, &month_req).expect("plan succeeds");
        // 2010-06 .. 2026-06 inclusive is 193 months; one is owned.
        assert_eq!(month_plan.jobs.len(), 192);
    }

    // A gap that happens to be exactly one calendar month gets the same name
    // under either span -- coverage subtracts ranges, not paths, so the two
    // modes must never disagree about what a window is called (D-0026).
    #[test]
    fn whole_span_still_names_a_single_month_as_a_month() {
        let dir = TempDir::new();
        let catalog = empty_catalog(&dir);
        let mut provider = FakeProvider::new(glbx_range());

        let mut req = request(&["ES.FUT"], dates((2024, 1, 1), (2024, 2, 1)));
        req.window_span = WindowSpan::Whole;
        let plan = plan(&catalog, &mut provider, &req).expect("plan succeeds");

        assert_eq!(plan.jobs.len(), 1);
        assert_eq!(
            plan.jobs[0].target_file_path,
            "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"
        );
    }
}
