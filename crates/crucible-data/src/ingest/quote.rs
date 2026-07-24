//! Pricing a plan, and the gate that decides whether it may be bought.
//!
//! Quoting is free: `get_cost` and `get_billable_size` are metadata calls
//! that reserve nothing and charge nothing. That is what makes a dry run the
//! sensible default rather than a courtesy — the honest answer to "what will
//! this cost" is available at no cost, so there is no excuse for guessing.
//!
//! ## The entitlement check
//!
//! Each window is priced twice, from two independent directions:
//!
//! - `billable_size × unit_price` — the **metered** price, from the public
//!   rate card, ignoring the account entirely.
//! - `get_cost` — the **billed** price, which the vendor computes with the
//!   account's subscription applied.
//!
//! When they agree, the account is paying per byte. When the billed price
//! collapses toward zero while the metered estimate does not, a flat-rate
//! entitlement is live. That comparison costs nothing and answers the one
//! question that otherwise requires logging into a web portal: *is the
//! subscription actually in force right now?* Asking it before a large
//! download is the difference between a $0 bootstrap and a four-figure one.
//!
//! ## A unit that lies about its name
//!
//! Databento's rate card is quoted "per gigabyte", but the divisor that
//! reproduces its own `get_cost` answers is 2^30, not 10^9 — verified
//! 2026-07-24 across four schemas spanning three orders of magnitude
//! (`ohlcv-1s` 19.674, `trades` 5.666, `mbp-10` 949.28, each matching its
//! quoted cost exactly when divided by 2^30). Using 10^9 would overstate
//! every estimate by 7.4%, which is small enough to look like rounding and
//! large enough to make the entitlement check above fire spuriously.

use crate::ingest::error::IngestError;
use crate::ingest::money::{format_usd, usd_f64_to_nano_ceil};
use crate::ingest::plan::{PlannedJob, PullPlan};
use crate::ingest::provider::BatchProvider;
use crucible_core::types::NanoUsd;

/// Bytes per billable "gigabyte" on the vendor's rate card. See the module
/// docs: the name says GB, the arithmetic says GiB.
pub const BYTES_PER_BILLABLE_GB: u64 = 1 << 30;

/// One priced window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobQuote {
    /// The window this prices.
    pub job: PlannedJob,
    /// Billable uncompressed size in bytes.
    pub billable_bytes: u64,
    /// Price the vendor would actually charge, in nanodollars.
    pub cost_nano_usd: NanoUsd,
}

/// Metered-versus-billed comparison for the whole plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitlementCheck {
    /// List price used for the estimate, USD per billable GB, rendered.
    pub unit_price_usd_per_gb: String,
    /// What the plan would cost at rate-card prices, in nanodollars.
    pub metered_nano_usd: NanoUsd,
    /// What the vendor actually quoted, in nanodollars.
    pub quoted_nano_usd: NanoUsd,
}

impl EntitlementCheck {
    /// True when the vendor quoted materially less than the rate card — the
    /// signature of an active flat-rate entitlement.
    ///
    /// The threshold is half the metered estimate rather than exact equality,
    /// because volume tiering can shade a metered price without any
    /// subscription being involved.
    #[must_use]
    pub fn flat_rate_appears_active(&self) -> bool {
        self.metered_nano_usd > 0 && self.quoted_nano_usd * 2 < self.metered_nano_usd
    }
}

/// A fully priced plan: what it would cost, and what it would skip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteReport {
    /// Vendor dataset.
    pub dataset: String,
    /// Vendor schema.
    pub schema: String,
    /// Priced windows, in plan order.
    pub quotes: Vec<JobQuote>,
    /// Total the vendor would charge, in nanodollars.
    pub total_cost_nano_usd: NanoUsd,
    /// Total billable bytes across all windows.
    pub total_billable_bytes: u64,
    /// Number of already-owned intervals skipped, across all keys.
    pub skipped_owned_intervals: usize,
    /// Metered-versus-billed comparison, when a unit price was available.
    pub entitlement: Option<EntitlementCheck>,
    /// Whether the provider's dataset range narrowed the request.
    pub was_clipped: bool,
    /// Effective range after clipping, rendered for display.
    pub effective_range: String,
}

impl QuoteReport {
    /// True when there is nothing left to buy.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.quotes.is_empty()
    }
}

/// Prices every window in `plan`.
///
/// Spends nothing: only metadata endpoints are called. A window that cannot
/// be priced aborts the whole report — see [`IngestError::CostQuoteUnavailable`].
///
/// # Errors
/// [`IngestError::CostQuoteUnavailable`] if any window cannot be priced, or
/// [`IngestError::Provider`] if billable size cannot be fetched. Either way
/// the caller gets no report at all rather than a partial one, because a
/// total that silently omits an unpriceable window understates the bill.
pub fn quote(
    provider: &mut dyn BatchProvider,
    plan: &PullPlan,
) -> Result<QuoteReport, IngestError> {
    let mut quotes = Vec::with_capacity(plan.jobs.len());
    let mut total_cost_nano_usd: NanoUsd = 0;
    let mut total_billable_bytes: u64 = 0;

    for job in &plan.jobs {
        let query = job.quote_query(&plan.dataset, &plan.schema, plan.stype_in);

        let billable_bytes =
            provider
                .billable_size(&query)
                .map_err(|source| IngestError::Provider {
                    during: "fetching billable size",
                    source,
                })?;

        let cost_nano_usd =
            provider
                .cost(&query)
                .map_err(|source| IngestError::CostQuoteUnavailable {
                    target_file_path: job.target_file_path.clone(),
                    source,
                })?;

        total_cost_nano_usd = total_cost_nano_usd.saturating_add(cost_nano_usd);
        total_billable_bytes = total_billable_bytes.saturating_add(billable_bytes);
        quotes.push(JobQuote {
            job: job.clone(),
            billable_bytes,
            cost_nano_usd,
        });
    }

    // The entitlement check is a diagnostic, so a provider that cannot supply
    // a unit price degrades it to `None` rather than failing the quote.
    let entitlement = match provider.unit_price_usd_per_gb(&plan.dataset, &plan.schema) {
        Ok(unit_price) => {
            let gb = total_billable_bytes as f64 / BYTES_PER_BILLABLE_GB as f64;
            usd_f64_to_nano_ceil(gb * unit_price)
                .ok()
                .map(|metered_nano_usd| EntitlementCheck {
                    unit_price_usd_per_gb: format!("{unit_price:.2}"),
                    metered_nano_usd,
                    quoted_nano_usd: total_cost_nano_usd,
                })
        }
        Err(_) => None,
    };

    Ok(QuoteReport {
        dataset: plan.dataset.clone(),
        schema: plan.schema.clone(),
        quotes,
        total_cost_nano_usd,
        total_billable_bytes,
        skipped_owned_intervals: plan.already_owned.values().map(Vec::len).sum(),
        entitlement,
        was_clipped: plan.was_clipped(),
        effective_range: format!(
            "{} .. {}",
            crate::ingest::window::date_of(plan.effective.start_ts()),
            crate::ingest::window::date_of(plan.effective.end_ts())
        ),
    })
}

/// Decides whether a quoted plan may be bought.
///
/// `cap_nano_usd` is the ceiling the operator explicitly authorised. There is
/// no default: [`IngestError::ExecuteWithoutCap`] is returned when it is
/// absent, because a default spending cap is a number nobody chose.
///
/// The comparison is `quoted <= cap`, which makes a cap of exactly zero
/// meaningful and useful: the recurring archival job runs with
/// `--max-cost-usd 0.00`, so it proceeds while the subscription entitles the
/// data for free and **refuses** the month the entitlement lapses, instead of
/// quietly billing for it.
///
/// Refusal is all-or-nothing. Buying "as much as the cap allows" would leave
/// a partial archive that `Catalog::coverage` will thereafter report as
/// owned — a hole that never heals, which is worse than buying nothing.
///
/// # Errors
/// [`IngestError::ExecuteWithoutCap`] or [`IngestError::CostGateRefused`].
pub fn gate(report: &QuoteReport, cap_nano_usd: Option<NanoUsd>) -> Result<(), IngestError> {
    let Some(cap) = cap_nano_usd else {
        return Err(IngestError::ExecuteWithoutCap);
    };
    if report.total_cost_nano_usd > cap {
        return Err(IngestError::CostGateRefused {
            quoted_nano_usd: report.total_cost_nano_usd,
            cap_nano_usd: cap,
            job_count: report.quotes.len(),
            billable_bytes: report.total_billable_bytes,
        });
    }
    Ok(())
}

/// Human-readable bytes, e.g. `2.4 MiB`.
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

impl core::fmt::Display for QuoteReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "pull plan — {} / {}", self.dataset, self.schema)?;
        writeln!(f, "effective range: {}", self.effective_range)?;
        if self.was_clipped {
            writeln!(
                f,
                "  ^ narrowed by the provider's dataset range; the rest does not exist"
            )?;
        }
        if self.skipped_owned_intervals > 0 {
            writeln!(
                f,
                "{} interval(s) already in the manifest — not quoted, not bought",
                self.skipped_owned_intervals
            )?;
        }
        writeln!(f)?;

        if self.quotes.is_empty() {
            writeln!(f, "  nothing to download — the archive already covers this")?;
            return Ok(());
        }

        writeln!(
            f,
            "  {:>3}  {:>12}  {:>13}   target",
            "#", "billable", "cost"
        )?;
        for (i, q) in self.quotes.iter().enumerate() {
            writeln!(
                f,
                "  {:>3}  {:>12}  {:>13}   {}",
                i + 1,
                format_bytes(q.billable_bytes),
                format_usd(q.cost_nano_usd),
                q.job.target_file_path
            )?;
        }
        writeln!(
            f,
            "       {:>12}  {:>13}   {} window(s)",
            format_bytes(self.total_billable_bytes),
            format_usd(self.total_cost_nano_usd),
            self.quotes.len()
        )?;

        if let Some(e) = &self.entitlement {
            writeln!(f)?;
            writeln!(
                f,
                "  entitlement check: metered estimate {} at ${}/GB",
                format_usd(e.metered_nano_usd),
                e.unit_price_usd_per_gb
            )?;
            if e.flat_rate_appears_active() {
                writeln!(
                    f,
                    "                     vendor quotes {} — a flat-rate entitlement IS active",
                    format_usd(e.quoted_nano_usd)
                )?;
            } else {
                writeln!(
                    f,
                    "                     vendor quotes {} — NO flat-rate discount is active",
                    format_usd(e.quoted_nano_usd)
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, TsRange};
    use crate::ingest::plan::{PullRequest, plan, range_from_dates};
    use crate::ingest::provider::StypeIn;
    use crate::ingest::provider::fake::FakeProvider;
    use crate::testutil::TempDir;

    fn dates(start: (i64, u32, u32), end: (i64, u32, u32)) -> TsRange {
        range_from_dates(start, end).expect("valid test range")
    }

    fn glbx() -> TsRange {
        dates((2010, 6, 6), (2026, 7, 24))
    }

    fn request(range: TsRange) -> PullRequest {
        PullRequest {
            dataset: "GLBX.MDP3".to_string(),
            schema: "ohlcv-1m".to_string(),
            symbols: vec!["ES.FUT".to_string()],
            stype_in: StypeIn::Parent,
            range,
        }
    }

    /// The provider request planning will make for one ES window.
    fn es_query(range: TsRange) -> crate::ingest::provider::QuoteQuery {
        crate::ingest::provider::QuoteQuery {
            dataset: "GLBX.MDP3".to_string(),
            schema: "ohlcv-1m".to_string(),
            symbols: vec!["ES.FUT".to_string()],
            stype_in: StypeIn::Parent,
            range,
        }
    }

    /// Plans January+February 2024 with each month priced separately.
    fn quoted(
        jan_cost: NanoUsd,
        feb_cost: NanoUsd,
        jan_bytes: u64,
        feb_bytes: u64,
    ) -> (QuoteReport, FakeProvider) {
        let dir = TempDir::new();
        let catalog = Catalog::open(dir.path()).expect("open catalog");
        let mut provider = FakeProvider::new(glbx())
            .with_quote(
                &es_query(dates((2024, 1, 1), (2024, 2, 1))),
                jan_cost,
                jan_bytes,
            )
            .with_quote(
                &es_query(dates((2024, 2, 1), (2024, 3, 1))),
                feb_cost,
                feb_bytes,
            );
        let plan = plan(
            &catalog,
            &mut provider,
            &request(dates((2024, 1, 1), (2024, 3, 1))),
        )
        .expect("plan succeeds");
        let report = quote(&mut provider, &plan).expect("quote succeeds");
        (report, provider)
    }

    // Hand-derived: $1.25 + $1.75 = $3.00 exactly, in nanodollars.
    #[test]
    fn totals_are_the_exact_sum_of_window_quotes() {
        let (report, _) = quoted(1_250_000_000, 1_750_000_000, 1000, 2000);
        assert_eq!(report.quotes.len(), 2);
        assert_eq!(report.total_cost_nano_usd, 3_000_000_000);
        assert_eq!(report.total_billable_bytes, 3000);
        assert_eq!(format_usd(report.total_cost_nano_usd), "$3.0000");
    }

    // THE money test: pricing a plan must never submit anything.
    #[test]
    fn quoting_never_submits() {
        let (_, provider) = quoted(1_250_000_000, 1_750_000_000, 1000, 2000);
        assert!(
            provider.spent_nothing(),
            "quote path called a money-spending method: {:?}",
            provider.calls
        );
        assert!(!provider.calls.iter().any(|c| c.starts_with("submit")));
    }

    // Negative control for the test above: if the seam could not observe a
    // submission, `quoting_never_submits` would pass vacuously forever.
    #[test]
    fn the_fake_provider_does_observe_a_submission() {
        use crate::ingest::provider::{BatchProvider, JobSpec, QuoteQuery};
        let mut provider = FakeProvider::new(glbx());
        assert!(provider.spent_nothing());
        provider
            .submit(&JobSpec {
                query: QuoteQuery {
                    dataset: "GLBX.MDP3".to_string(),
                    schema: "ohlcv-1m".to_string(),
                    symbols: vec!["ES.FUT".to_string()],
                    stype_in: StypeIn::Parent,
                    range: dates((2024, 1, 1), (2024, 2, 1)),
                },
                target_file_path: "raw/x.dbn.zst".to_string(),
            })
            .expect("fake submit succeeds");
        assert!(
            !provider.spent_nothing(),
            "the spend detector must actually fire when a submit happens"
        );
    }

    #[test]
    fn the_gate_allows_a_total_equal_to_the_cap() {
        let (report, _) = quoted(1_250_000_000, 1_750_000_000, 1000, 2000);
        assert!(gate(&report, Some(3_000_000_000)).is_ok(), "exactly at cap");
        assert!(gate(&report, Some(3_000_000_001)).is_ok(), "under cap");
    }

    // One nanodollar over is over. The display would round both to $3.0000.
    #[test]
    fn the_gate_refuses_one_nano_over_the_cap_and_submits_nothing() {
        let (report, provider) = quoted(1_250_000_000, 1_750_000_000, 1000, 2000);
        let err = gate(&report, Some(2_999_999_999)).expect_err("must refuse");
        match err {
            IngestError::CostGateRefused {
                quoted_nano_usd,
                cap_nano_usd,
                job_count,
                billable_bytes,
            } => {
                assert_eq!(quoted_nano_usd, 3_000_000_000);
                assert_eq!(cap_nano_usd, 2_999_999_999);
                assert_eq!(job_count, 2);
                assert_eq!(billable_bytes, 3000);
            }
            other => panic!("wrong error: {other:?}"),
        }
        assert!(provider.spent_nothing());
    }

    // The property that makes the monthly cron safe: under an entitlement
    // everything quotes zero, so a zero cap passes; the month the
    // subscription lapses, the same command refuses instead of billing.
    #[test]
    fn a_zero_cap_passes_while_entitled_and_refuses_when_not() {
        let (entitled, _) = quoted(0, 0, 1000, 2000);
        assert!(
            gate(&entitled, Some(0)).is_ok(),
            "entitled data quotes $0 and must pass a $0 cap"
        );

        let (metered, _) = quoted(1, 0, 1000, 2000);
        assert!(
            gate(&metered, Some(0)).is_err(),
            "a single nanodollar must trip a $0 cap"
        );
    }

    #[test]
    fn execute_without_a_cap_is_refused() {
        let (report, _) = quoted(0, 0, 1000, 2000);
        assert!(matches!(
            gate(&report, None),
            Err(IngestError::ExecuteWithoutCap)
        ));
    }

    // An unpriceable window aborts the report. It must never be silently
    // treated as free and folded into a total that then passes the gate.
    #[test]
    fn an_unavailable_quote_is_a_refusal_not_an_assumption_of_zero() {
        use crate::ingest::provider::ProviderError;
        let dir = TempDir::new();
        let catalog = Catalog::open(dir.path()).expect("open catalog");
        let mut provider = FakeProvider::new(glbx());
        let plan = plan(
            &catalog,
            &mut provider,
            &request(dates((2024, 1, 1), (2024, 2, 1))),
        )
        .expect("plan succeeds");

        provider.cost_error = Some(ProviderError::RateLimited {
            retry_after_secs: Some(30),
        });
        let err = quote(&mut provider, &plan).expect_err("must refuse");

        assert!(matches!(err, IngestError::CostQuoteUnavailable { .. }));
        assert!(provider.spent_nothing());
    }

    // Hand-derived: 2 GiB at $70/GB = $140.00. Under an entitlement the
    // vendor quotes $0, and the mismatch is what reveals the subscription.
    #[test]
    fn the_entitlement_check_distinguishes_metered_from_flat_rate() {
        let gib = BYTES_PER_BILLABLE_GB;
        let (metered, _) = quoted(70_000_000_000, 70_000_000_000, gib, gib);
        let check = metered.entitlement.as_ref().expect("unit price available");
        assert_eq!(check.metered_nano_usd, 140_000_000_000);
        assert_eq!(check.quoted_nano_usd, 140_000_000_000);
        assert!(
            !check.flat_rate_appears_active(),
            "identical metered and quoted prices mean pay-as-you-go"
        );

        let (entitled, _) = quoted(0, 0, gib, gib);
        let check = entitled.entitlement.as_ref().expect("unit price available");
        assert_eq!(check.metered_nano_usd, 140_000_000_000);
        assert_eq!(check.quoted_nano_usd, 0);
        assert!(
            check.flat_rate_appears_active(),
            "a quote far below the rate card means a subscription is live"
        );
    }

    // Owning the whole request costs nothing and asks the vendor nothing:
    // the archive answers the question without a single priced call.
    #[test]
    fn a_fully_owned_plan_quotes_nothing_and_renders_as_such() {
        use crate::catalog::Acquisition;
        use crucible_core::types::Ts;

        let dir = TempDir::new();
        let mut catalog = Catalog::open(dir.path()).expect("open catalog");
        let rel = "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst";
        let abs = dir.path().join(rel);
        std::fs::create_dir_all(abs.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&abs, b"owned bytes").expect("write fixture");
        catalog
            .append(Acquisition {
                dataset: "GLBX.MDP3".to_string(),
                schema: "ohlcv-1m".to_string(),
                symbols: vec!["ES.FUT".to_string()],
                range: dates((2024, 1, 1), (2024, 2, 1)),
                acquired_ts: Ts(1_700_000_000_000_000_000),
                databento_job_id: "test-job".to_string(),
                file_path: rel.to_string(),
            })
            .expect("append owned slice");

        let mut provider = FakeProvider::new(glbx());
        let plan = plan(
            &catalog,
            &mut provider,
            &request(dates((2024, 1, 1), (2024, 2, 1))),
        )
        .expect("plan succeeds");
        let report = quote(&mut provider, &plan).expect("quote succeeds");

        assert!(report.is_empty());
        assert_eq!(report.total_cost_nano_usd, 0);
        assert_eq!(report.total_billable_bytes, 0);
        assert_eq!(report.skipped_owned_intervals, 1);
        assert!(
            gate(&report, Some(0)).is_ok(),
            "nothing to buy passes a $0 cap"
        );
        assert!(
            report.to_string().contains("already covers this"),
            "{report}"
        );
        // Not one cost or size call was made -- the manifest answered it.
        assert!(!provider.calls.iter().any(|c| c.starts_with("cost:")));
        assert!(
            !provider
                .calls
                .iter()
                .any(|c| c.starts_with("billable_size:"))
        );
    }

    #[test]
    fn the_report_renders_the_numbers_a_human_needs_to_decide() {
        let (report, _) = quoted(1_250_000_000, 1_750_000_000, 1_048_576, 2_097_152);
        let text = report.to_string();
        assert!(text.contains("GLBX.MDP3 / ohlcv-1m"), "{text}");
        assert!(text.contains("1.00 MiB"), "{text}");
        assert!(text.contains("$1.2500"), "{text}");
        assert!(text.contains("$3.0000"), "{text}");
        assert!(
            text.contains("raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"),
            "{text}"
        );
        assert!(text.contains("entitlement check"), "{text}");
    }

    #[test]
    fn byte_formatting_is_readable_at_every_scale() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1_048_576), "1.00 MiB");
        assert_eq!(format_bytes(BYTES_PER_BILLABLE_GB), "1.00 GiB");
    }
}
