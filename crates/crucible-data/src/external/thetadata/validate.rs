//! The merge-blocking clauses of `docs/THETADATA_PLAN.md` §4.
//!
//! Every clause here exists because its absence produces a *plausible number
//! that is wrong*, which is the failure mode this project exists to prevent.
//! Each one carries a planted-bug negative control in the tests below: a
//! detector nobody has watched fire is decoration (CLAUDE.md §7).
//!
//! ## What "validated" means
//!
//! A response arrives as CSV bytes. Validation takes it to a
//! [`ValidatedResponse`] — deduplicated rows, plus a [`ValidationReport`]
//! recording everything an operator or the inventory needs to know about how it
//! got that way. Anything that cannot be resolved *refuses the file*, because
//! ThetaData responses are re-fetchable: a refusal costs one request, and the
//! alternative costs a research result nobody can explain a year later.
//!
//! ## Row identity, and why it is per endpoint
//!
//! The vendor duplicates rows for a reason that is specific to its post-close
//! build pipeline (D-0054), so "what should be unique" is not one rule:
//!
//! | Endpoint kind | Identity | Discriminator |
//! |---|---|---|
//! | daily snapshot (`eod`, `greeks/eod`, `open_interest`) | [`ContractKey`] | the build/update stamp |
//! | intraday option (`quote`, `ohlc`, `greeks/first_order`) | (`ContractKey`, timestamp) | none |
//! | stock | timestamp | none |
//!
//! A *discriminator* is a column that may legitimately differ between two rows
//! carrying the same identity — the vendor ran the build twice. Where an
//! endpoint has one, repeats are deduplicated by keeping the **latest**
//! discriminator value. Where it has none, a repeat is a bug and refuses the
//! file. The intraday case is why an interval endpoint cannot simply dedup by
//! contract: a contract legitimately appears 391 times in a 1-minute day, and
//! collapsing that to one row would silently discard the session.
//!
//! `eod`'s discriminator is `created` — the vendor's post-close *build-run*
//! time. `greeks/eod`'s is `timestamp` — a *per-contract update* time. They
//! are different quantities that happen to occupy the same column position,
//! which is exactly why parsing is by name (§4.1) and why they land as separate
//! columns downstream. Keeping the maximum is right for both, and for the same
//! reason: the later stamp is the vendor's final word, and a later `avail_ts`
//! is the conservative direction (D-0052).
//!
//! ## Why keep-latest and not first-wins
//!
//! First-wins would date every row by whichever build happened to run first,
//! asserting the information existed earlier than the vendor's final answer
//! did. That is the same lookahead argument D-0052 makes about ambiguous
//! Eastern timestamps, and it has the same answer: delay is safe, anticipation
//! is not.
//!
//! ## Completeness is counted in contracts, never rows
//!
//! `rows = 2 × distinct(contract)` through 2021 and `rows = distinct` from
//! 2022. Any accounting keyed on raw rows therefore reports twice the truth for
//! a decade of data and the truth afterwards, with no error message anywhere.
//! An OI-weighted or GEX-style aggregate built that way would double for the
//! affected era — worst below each root's greeks floor, where `eod` is the only
//! source and has no `greeks/eod` to reconcile against.

use std::collections::{BTreeMap, BTreeSet};

use super::error::ThetaError;
use super::schema::{ColumnIndex, ContractKey, Endpoint, is_zero_sentinel};
use crate::calendar::CivilDate;

/// How rows of one endpoint are expected to be unique.
///
/// Returned by [`Endpoint::row_identity`] so the rule lives beside the pinned
/// header it belongs to rather than being re-derived at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowIdentity {
    /// One row per contract per day, disambiguated by a build/update stamp.
    ContractPerDay {
        /// Column carrying the stamp that may legitimately repeat a contract.
        discriminator: &'static str,
    },
    /// One row per contract per interval; no legitimate repeats.
    ContractPerInterval,
    /// One row per interval, with no contract dimension (stock endpoints).
    IntervalOnly,
}

impl Endpoint {
    /// What makes a row of this endpoint unique, and what may repeat it.
    #[must_use]
    pub fn row_identity(self) -> RowIdentity {
        match self {
            // `created` is the vendor's post-close build-run time (D-0054).
            Endpoint::OptionEod => RowIdentity::ContractPerDay {
                discriminator: "created",
            },
            // `timestamp` here is the per-contract last-update time, NOT a
            // build stamp — a different quantity in the same column position,
            // which is the whole reason parsing is by name. Measured ratio is
            // 1.000 wherever greeks exist, so this discriminator is expected
            // never to fire; it is present because D-0054's mechanism is a
            // build-pipeline artefact and nothing guarantees where it appears
            // next.
            Endpoint::OptionGreeksEod | Endpoint::OptionOpenInterest => {
                RowIdentity::ContractPerDay {
                    discriminator: "timestamp",
                }
            }
            Endpoint::OptionQuote | Endpoint::OptionOhlc | Endpoint::OptionGreeksFirstOrder => {
                RowIdentity::ContractPerInterval
            }
            Endpoint::StockOhlc | Endpoint::StockQuote => RowIdentity::IntervalOnly,
        }
    }

    /// Whether an all-zero OHLC block means "the vendor has nothing".
    ///
    /// **Only where OHLC is the entire payload.** This distinction was very
    /// nearly got wrong, and the archive settled it: VIX `eod` for 2024-01-02
    /// returns 1,058 contracts of which 672 have `0.00,0.00,0.00,0.00` OHLC
    /// and zero volume — and 614 of those carry a real bid. On `eod` a zero
    /// OHLC block means *this contract did not trade today*, which is ordinary
    /// for most of an option chain, and the NBBO beside it is the real data.
    /// A file-level gate applied there would refuse a good day for nine roots
    /// at once.
    ///
    /// `stock/history/ohlc` is the measured case (SPY 2016-01-04: HTTP 200,
    /// 390 rows of zeros, while QQQ the same day returns real prices), and it
    /// has no NBBO to carry information instead. `option/history/ohlc` is
    /// included by structure rather than by measurement — same columns, same
    /// absence of a quote — and that difference is recorded rather than
    /// smoothed over.
    #[must_use]
    pub fn ohlc_is_the_whole_payload(self) -> bool {
        matches!(self, Endpoint::StockOhlc | Endpoint::OptionOhlc)
    }
}

/// One CSV row, split and kept as vendor text.
///
/// Text rather than parsed numbers on purpose: the dedup key must not be able
/// to collide two genuinely different strikes through a rounding choice made
/// before anyone asked for a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRow {
    /// 1-based row number in the body, excluding the header. For diagnostics.
    pub row: u64,
    /// Fields in header order.
    pub fields: Vec<String>,
}

impl RawRow {
    /// Value of a named column, or `None` when the endpoint has no such column.
    #[must_use]
    pub fn get<'a>(&'a self, index: &ColumnIndex, name: &str) -> Option<&'a str> {
        index
            .position(name)
            .and_then(|p| self.fields.get(p))
            .map(String::as_str)
    }
}

/// Everything a validated response knows about itself.
///
/// These fields are the inventory's per-file record (§6.1). They exist so that
/// an era fingerprint — "this file was 2.000× duplicated, like everything else
/// before 2022" — is recorded at the moment of fetch rather than reconstructed
/// later from data that has already been deduplicated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Rows in the body, excluding the header, before anything was dropped.
    pub raw_rows: u64,
    /// Distinct identities — the unit of completeness accounting.
    pub distinct_rows: u64,
    /// How many identities carried how many discriminator values.
    ///
    /// Keyed by build count: `{2: 2956}` is the classic pre-2022 `eod` day.
    /// Recorded rather than asserted, because 2020-01-02 carries **four**
    /// distinct `created` values while every contract still appears exactly
    /// twice — contracts split across passes — so any check that assumed two
    /// would refuse a good file.
    pub n_builds_distribution: BTreeMap<usize, u64>,
    /// Repeats whose non-discriminator fields were byte-equal: deduplicated
    /// silently, counted here.
    pub identical_pairs: u64,
    /// Repeats whose market fields *differed* across builds.
    ///
    /// Keep-latest still applies, but these are QA signal, not noise: a
    /// revision between build passes is the vendor changing its mind about a
    /// printed value, and an operator should see how often that happens.
    pub conflicting_pairs: u64,
    /// Rows dropped by the zero-sentinel condition (§4.3).
    pub sentinel_rows_dropped: u64,
    /// Kept rows whose whole OHLC block is zero.
    ///
    /// On `eod` this is *ordinary*: it means the contract did not trade that
    /// day, and most of a chain does not trade on most days (D-0055). It is
    /// recorded rather than acted on, as a second era fingerprint beside the
    /// dup rate — a VIX-shaped root runs high and that is fine, but the rate
    /// moving sharply for a root that has always been liquid is a finding
    /// about the vendor's pipeline, and the only way to notice is to have been
    /// writing it down all along.
    pub zero_ohlc_rows: u64,
}

impl ValidationReport {
    /// Raw rows per distinct identity — the era fingerprint (§3.1).
    ///
    /// 2.000 through 2021-12-15, 1.000 from 2022-01-03. Returns 0.0 for an
    /// empty body rather than dividing by zero: an empty response has no
    /// duplication rate, and reporting 1.000 would claim it was measured.
    #[must_use]
    pub fn dup_rate(&self) -> f64 {
        if self.distinct_rows == 0 {
            return 0.0;
        }
        // Both counts come from one response and are far below 2^53.
        self.raw_rows as f64 / self.distinct_rows as f64
    }

    /// Fraction of kept rows carrying an all-zero OHLC block.
    ///
    /// `None` when nothing was kept — unmeasured, not zero, for the same
    /// reason every other rate here refuses to report a figure over an empty
    /// sample (§0.4).
    #[must_use]
    pub fn zero_ohlc_rate(&self) -> Option<f64> {
        let kept = self
            .distinct_rows
            .saturating_sub(self.sentinel_rows_dropped);
        (kept > 0).then(|| self.zero_ohlc_rows as f64 / kept as f64)
    }
}

/// A response that passed every §4 clause.
#[derive(Debug, Clone)]
pub struct ValidatedResponse {
    /// Which endpoint's pin the header matched.
    pub endpoint: Endpoint,
    /// Column positions resolved by name.
    pub index: ColumnIndex,
    /// Deduplicated, sentinel-filtered rows, in first-appearance order.
    ///
    /// First-appearance rather than sorted: the vendor emits a chain in a
    /// stable order, and preserving it keeps a golden-raw round-trip a
    /// byte-comparison rather than a set-comparison.
    pub rows: Vec<RawRow>,
    /// What validation observed on the way.
    pub report: ValidationReport,
}

impl ValidatedResponse {
    /// The distinct contracts this response covers.
    ///
    /// Empty for stock endpoints, which have no contract dimension.
    #[must_use]
    pub fn contract_keys(&self) -> BTreeSet<ContractKey> {
        if !self.endpoint.is_contract_scoped() {
            return BTreeSet::new();
        }
        self.rows
            .iter()
            .filter_map(|r| {
                Some(ContractKey {
                    expiration: r.get(&self.index, "expiration")?.to_owned(),
                    strike: r.get(&self.index, "strike")?.to_owned(),
                    right: r.get(&self.index, "right")?.to_owned(),
                })
            })
            .collect()
    }
}

/// Validates one raw response body against an endpoint's pin and §4's clauses.
///
/// # Errors
/// - [`ThetaError::UnexpectedColumns`] on any header drift (§4.1).
/// - [`ThetaError::DuplicateRow`] when one identity repeats with the *same*
///   discriminator, or repeats at all where there is none (§4.2).
/// - [`ThetaError::AllZeroSeries`] when every row is the vendor's zero
///   sentinel (§4.3).
/// - [`ThetaError::MalformedRow`] when a row has the wrong field count or is
///   missing a key column.
pub fn validate(
    endpoint: Endpoint,
    body: &[u8],
    request_path: &str,
) -> Result<ValidatedResponse, ThetaError> {
    let text = std::str::from_utf8(body).map_err(|e| ThetaError::MalformedRow {
        path: request_path.to_owned(),
        row: 0,
        detail: format!("the body is not UTF-8: {e}"),
    })?;

    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().unwrap_or_default();
    let index = ColumnIndex::validate(endpoint, header, request_path)?;

    let mut rows = Vec::new();
    for (offset, line) in lines.enumerate() {
        let row = offset as u64 + 1;
        let fields: Vec<String> = split_csv(line);
        if fields.len() != index.len() {
            return Err(ThetaError::MalformedRow {
                path: request_path.to_owned(),
                row,
                detail: format!(
                    "expected {} fields to match the pinned header, found {}",
                    index.len(),
                    fields.len()
                ),
            });
        }
        rows.push(RawRow { row, fields });
    }

    let mut report = ValidationReport {
        raw_rows: rows.len() as u64,
        ..ValidationReport::default()
    };

    let deduped = dedup(endpoint, &index, rows, request_path, &mut report)?;
    let kept = drop_zero_sentinels(&index, deduped, &mut report);

    // §4.3, the file-level gate: a series that is entirely the vendor's
    // "absent" spelling is refused, not archived as a quiet day. SPY
    // 2016-01-04 answers HTTP 200 with 390 rows of `0.0,0.0,0.0,0.0,0,0`
    // while QQQ on the same date returns real prices, so this is per symbol
    // and cannot be inferred from the date.
    //
    // A response whose every row tripped the zero sentinel is the same
    // statement made through a different column: the underlying feed produced
    // nothing all day, so there is no surface here to archive.
    if report.raw_rows > 0 && kept.is_empty() {
        return Err(ThetaError::AllZeroSeries {
            path: request_path.to_owned(),
            rows: report.raw_rows,
        });
    }
    if endpoint.ohlc_is_the_whole_payload() && report.raw_rows > 0 && all_zero_ohlc(&index, &kept) {
        return Err(ThetaError::AllZeroSeries {
            path: request_path.to_owned(),
            rows: report.raw_rows,
        });
    }

    report.zero_ohlc_rows = count_zero_ohlc(&index, &kept);
    report.distinct_rows = kept.len() as u64 + report.sentinel_rows_dropped;
    Ok(ValidatedResponse {
        endpoint,
        index,
        rows: kept,
        report,
    })
}

/// Splits one CSV line, trimming the vendor's optional quoting.
///
/// The vendor never emits an embedded comma in any pinned column — every field
/// is a number, a date, or one of `CALL`/`PUT` — so a full CSV state machine
/// would be answering a question this format does not ask.
fn split_csv(line: &str) -> Vec<String> {
    line.trim_end_matches(['\r', '\n'])
        .split(',')
        .map(|f| f.trim().trim_matches('"').to_owned())
        .collect()
}

/// The identity of one row under its endpoint's rule.
fn identity_of(
    endpoint: Endpoint,
    index: &ColumnIndex,
    row: &RawRow,
    request_path: &str,
) -> Result<String, ThetaError> {
    let need = |name: &str| -> Result<&str, ThetaError> {
        row.get(index, name)
            .ok_or_else(|| ThetaError::MalformedRow {
                path: request_path.to_owned(),
                row: row.row,
                detail: format!("missing the {name} column"),
            })
    };
    match endpoint.row_identity() {
        RowIdentity::ContractPerDay { .. } => Ok(format!(
            "{}|{}|{}",
            need("expiration")?,
            need("strike")?,
            need("right")?
        )),
        RowIdentity::ContractPerInterval => Ok(format!(
            "{}|{}|{}|{}",
            need("expiration")?,
            need("strike")?,
            need("right")?,
            need("timestamp")?
        )),
        RowIdentity::IntervalOnly => Ok(need("timestamp")?.to_owned()),
    }
}

/// §4.2 — group by identity, keep the latest build, and account for the rest.
fn dedup(
    endpoint: Endpoint,
    index: &ColumnIndex,
    rows: Vec<RawRow>,
    request_path: &str,
    report: &mut ValidationReport,
) -> Result<Vec<RawRow>, ThetaError> {
    let discriminator = match endpoint.row_identity() {
        RowIdentity::ContractPerDay { discriminator } => Some(discriminator),
        RowIdentity::ContractPerInterval | RowIdentity::IntervalOnly => None,
    };

    // Insertion-ordered groups: `order` preserves first appearance so the
    // output order is the vendor's, and `groups` holds the rows per identity.
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Vec<RawRow>> = BTreeMap::new();
    for row in rows {
        let key = identity_of(endpoint, index, &row, request_path)?;
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row);
    }

    let mut kept = Vec::with_capacity(order.len());
    for key in order {
        let Some(group) = groups.remove(&key) else {
            continue;
        };
        if group.len() == 1 {
            *report.n_builds_distribution.entry(1).or_default() += 1;
            kept.extend(group);
            continue;
        }

        let Some(discriminator) = discriminator else {
            // No column may legitimately repeat this identity. An interval
            // endpoint serving the same (contract, minute) twice, or a stock
            // endpoint serving the same minute twice, is a different bug from
            // D-0054's build passes and must not be quietly collapsed.
            return Err(ThetaError::DuplicateRow {
                path: request_path.to_owned(),
                identity: key,
                occurrences: group.len() as u64,
                discriminator: None,
            });
        };

        // Distinct discriminator values. `(identity, discriminator)` appearing
        // twice is NOT the vendor running two builds — it is one build emitting
        // one contract twice, which no mechanism explains. Refuse the file.
        let mut stamps: BTreeSet<&str> = BTreeSet::new();
        for row in &group {
            let stamp = row
                .get(index, discriminator)
                .ok_or_else(|| ThetaError::MalformedRow {
                    path: request_path.to_owned(),
                    row: row.row,
                    detail: format!("missing the {discriminator} column"),
                })?;
            if !stamps.insert(stamp) {
                return Err(ThetaError::DuplicateRow {
                    path: request_path.to_owned(),
                    identity: key,
                    occurrences: group.len() as u64,
                    discriminator: Some((discriminator, stamp.to_owned())),
                });
            }
        }
        *report
            .n_builds_distribution
            .entry(stamps.len())
            .or_default() += 1;

        // Keep max(discriminator): the final build, and the conservative
        // availability direction (D-0052). Compared as text, which is correct
        // for the vendor's fixed-width stamps and cannot be tripped by a
        // parse choice.
        let winner = group
            .iter()
            .max_by(|a, b| {
                a.get(index, discriminator)
                    .unwrap_or_default()
                    .cmp(b.get(index, discriminator).unwrap_or_default())
            })
            .cloned();

        // Identical vs conflicting, judged on every column *except* the
        // discriminator: that one differing is the definition of a rebuild.
        let disc_pos = index.position(discriminator);
        for other in &group {
            let Some(w) = winner.as_ref() else { continue };
            // Row numbers are unique within a body, so this identifies the
            // winner without comparing addresses of a clone.
            if w.row == other.row {
                continue;
            }
            let same = w
                .fields
                .iter()
                .zip(&other.fields)
                .enumerate()
                .all(|(i, (a, b))| Some(i) == disc_pos || a == b);
            if same {
                report.identical_pairs += 1;
            } else {
                report.conflicting_pairs += 1;
            }
        }

        if let Some(winner) = winner {
            kept.push(winner);
        }
    }
    Ok(kept)
}

/// §4.3 — drop rows carrying the vendor's zero sentinel.
///
/// Only endpoints that actually carry `underlying_price`/`iv_error` can express
/// the condition; for the rest this is a no-op rather than a guess.
fn drop_zero_sentinels(
    index: &ColumnIndex,
    rows: Vec<RawRow>,
    report: &mut ValidationReport,
) -> Vec<RawRow> {
    if index.position("underlying_price").is_none() || index.position("iv_error").is_none() {
        return rows;
    }
    let mut kept = Vec::with_capacity(rows.len());
    for row in rows {
        let underlying = row
            .get(index, "underlying_price")
            .and_then(|v| v.parse::<f64>().ok());
        let iv_error = row
            .get(index, "iv_error")
            .and_then(|v| v.parse::<f64>().ok());
        match (underlying, iv_error) {
            (Some(u), Some(e)) if is_zero_sentinel(u, e) => report.sentinel_rows_dropped += 1,
            _ => kept.push(row),
        }
    }
    kept
}

/// How many rows carry an all-zero OHLC block.
///
/// Zero when the endpoint has no OHLC columns at all, which is a real answer
/// rather than a missing one: `open_interest` and `quote` cannot have a
/// zero-OHLC rate.
fn count_zero_ohlc(index: &ColumnIndex, rows: &[RawRow]) -> u64 {
    let ohlc = ["open", "high", "low", "close"];
    if ohlc.iter().any(|c| index.position(c).is_none()) {
        return 0;
    }
    rows.iter()
        .filter(|row| {
            ohlc.iter().all(|c| {
                row.get(index, c)
                    .and_then(|v| v.parse::<f64>().ok())
                    .is_some_and(|v| v == 0.0)
            })
        })
        .count() as u64
}

/// Whether every row's OHLC block is zero — the SPY 2016-01-04 shape.
fn all_zero_ohlc(index: &ColumnIndex, rows: &[RawRow]) -> bool {
    let ohlc = ["open", "high", "low", "close"];
    if ohlc.iter().any(|c| index.position(c).is_none()) {
        return false;
    }
    !rows.is_empty()
        && rows.iter().all(|row| {
            ohlc.iter().all(|c| {
                row.get(index, c)
                    .and_then(|v| v.parse::<f64>().ok())
                    .is_some_and(|v| v == 0.0)
            })
        })
}

// ---------------------------------------------------------------------------
// §4.4 — the reconciliation edges
// ---------------------------------------------------------------------------

/// Result of one (root, day) reconciliation across the endpoints fetched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Contracts in `eod` that `greeks/eod` also carried.
    pub eod_and_greeks: u64,
    /// Contracts `eod` carried that `greeks/eod` did not.
    ///
    /// **Coverage asymmetry, not a failure.** The computed surface (D-0053)
    /// covers them, and this is the expected shape below a root's greeks floor.
    pub eod_without_greeks: u64,
    /// Contracts `open_interest` carried that were also in `eod`.
    pub oi_in_eod: u64,
    /// Contracts in `eod` with no open-interest row.
    ///
    /// Expected and ordinary: OI rows exist only where interest does.
    pub eod_without_oi: u64,
}

impl Reconciliation {
    /// Fraction of `eod` contracts that open interest also covered.
    ///
    /// Returns `None` when `eod` was empty — a coverage fraction over nothing
    /// is not 1.0, it is unmeasured, and reporting the former would let an
    /// empty day pass as fully reconciled.
    #[must_use]
    pub fn oi_coverage(&self) -> Option<f64> {
        let total = self.oi_in_eod + self.eod_without_oi;
        (total > 0).then(|| self.oi_in_eod as f64 / total as f64)
    }
}

/// Reconciles one (root, day)'s `eod`, `greeks/eod` and `open_interest`.
///
/// The two inverted directions are refusals rather than warnings because each
/// contradicts the mechanism that explains the data at all:
///
/// - `greeks/eod` holding a contract `eod` lacks is impossible — `greeks/eod`
///   is derived from the same chain and was measured at exact parity
///   (4,588/4,588, 7,020/7,020, 2,840/2,840).
/// - a contract with open interest and no `eod` row inverts `OI ⊆ eod`, which
///   held on every sampled day.
///
/// A day that violates either is not a day with a small discrepancy; it is a
/// day whose relationship to the vendor's own pipeline is not what we think,
/// and everything computed from it would inherit that.
///
/// # Errors
/// [`ThetaError::ReconciliationInverted`] on either inverted direction.
pub fn reconcile(
    eod: &BTreeSet<ContractKey>,
    greeks: Option<&BTreeSet<ContractKey>>,
    open_interest: Option<&BTreeSet<ContractKey>>,
    context: &str,
) -> Result<Reconciliation, ThetaError> {
    let mut out = Reconciliation::default();

    if let Some(greeks) = greeks {
        let orphans: Vec<&ContractKey> = greeks.difference(eod).collect();
        if let Some(first) = orphans.first() {
            return Err(ThetaError::ReconciliationInverted {
                context: context.to_owned(),
                edge: "greeks/eod ⊆ eod",
                orphans: orphans.len() as u64,
                example: first.to_string(),
            });
        }
        out.eod_and_greeks = eod.intersection(greeks).count() as u64;
        out.eod_without_greeks = eod.difference(greeks).count() as u64;
    }

    if let Some(oi) = open_interest {
        let orphans: Vec<&ContractKey> = oi.difference(eod).collect();
        if let Some(first) = orphans.first() {
            return Err(ThetaError::ReconciliationInverted {
                context: context.to_owned(),
                edge: "open_interest ⊆ eod",
                orphans: orphans.len() as u64,
                example: first.to_string(),
            });
        }
        out.oi_in_eod = eod.intersection(oi).count() as u64;
        out.eod_without_oi = eod.difference(oi).count() as u64;
    }

    Ok(out)
}

/// Finds the first session in `sessions` for which `has_data` is true.
///
/// Pulled out of the CLI so the geometry that broke the first version has a
/// test rather than an anecdote. `sessions` must be **sessions**, not calendar
/// days: a weekend answers the vendor's 472 exactly like a date below a root's
/// floor, so over calendar days the predicate is not monotone and bisection
/// converges on whichever Monday it happens to straddle. The first
/// implementation reported SPY's greeks floor as 2021-03-22 that way — a
/// Monday whose preceding Sunday appeared to confirm it — when the true floor
/// is 2017-01-03 (D-0057).
///
/// Returns the index of the first session with data, `Some(0)` when the whole
/// range has it, and `None` when none does. `has_data` is called O(log n)
/// times and may fail, which aborts the search rather than guessing.
///
/// # Errors
/// Whatever `has_data` returns.
pub fn first_session_with_data<E>(
    sessions: usize,
    mut has_data: impl FnMut(usize) -> Result<bool, E>,
) -> Result<Option<usize>, E> {
    if sessions == 0 {
        return Ok(None);
    }
    let last = sessions - 1;
    if !has_data(last)? {
        return Ok(None);
    }
    if has_data(0)? {
        return Ok(Some(0));
    }
    // Invariant: `lo` has no data, `hi` has it.
    let (mut lo, mut hi) = (0usize, last);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if has_data(mid)? {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(Some(hi))
}

/// §4.4's third edge: the days a calendar expects against the days we hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoverageVsCalendar {
    /// Sessions the calendar expects in the window.
    pub expected_sessions: u64,
    /// Of those, how many the inventory holds.
    pub present_sessions: u64,
    /// Expected sessions with no data, in order. **The finding.**
    pub missing: Vec<CivilDate>,
    /// Dates we hold that the calendar says were not sessions, in order.
    ///
    /// Reported, never refused. This check runs *backwards* as well as
    /// forwards: real data is evidence and a calendar is a claim, so a pile of
    /// these means the calendar is wrong — which is exactly how D-0040
    /// falsified CME's published 15:15 CT halt. `qa` treats the same signal the
    /// same way.
    pub unexpected: Vec<CivilDate>,
}

impl CoverageVsCalendar {
    /// Fraction of expected sessions present, or `None` when none were
    /// expected.
    ///
    /// `None` rather than 1.0 for an empty window, for the same reason
    /// [`Reconciliation::oi_coverage`] does it: a coverage figure over nothing
    /// is unmeasured, and reporting it as perfect is how an empty run passes
    /// for a complete one (§0.4).
    #[must_use]
    pub fn coverage(&self) -> Option<f64> {
        (self.expected_sessions > 0)
            .then(|| self.present_sessions as f64 / self.expected_sessions as f64)
    }

    /// True when every expected session is present and nothing extra is held.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.unexpected.is_empty()
    }
}

/// Compares the sessions a calendar expects over `[start, end)` against `held`.
///
/// This is the edge that could not be computed before `us_equity_options`
/// existed (D-0058): the only bundled calendar described CME Globex, whose
/// trading-day set genuinely differs from the US equity one inside this span —
/// the NYSE was shut for Hurricane Sandy on 2012-10-29 and 30 while Globex
/// traded — so borrowing it would have reported real closures as missing data.
///
/// Takes a [`TradingDayCalendar`](crate::calendar::TradingDayCalendar) and not
/// a `Calendar`, so the five index roots whose *hours* this project cannot
/// state are still answerable about *dates* — and so nothing reachable from
/// here can ask an index root what time it opened (D-0059).
///
/// `end` is exclusive, matching every other range in this crate.
#[must_use]
pub fn coverage_vs_calendar(
    calendar: &crate::calendar::TradingDayCalendar,
    start: CivilDate,
    end: CivilDate,
    held: &BTreeSet<CivilDate>,
) -> CoverageVsCalendar {
    use crate::ingest::window::{civil_from_days, days_from_civil};

    let mut out = CoverageVsCalendar::default();
    let mut day = start;
    let mut expected: BTreeSet<CivilDate> = BTreeSet::new();
    while days_from_civil(day) < days_from_civil(end) {
        if calendar.is_trading_day(day) {
            expected.insert(day);
            out.expected_sessions += 1;
            if held.contains(&day) {
                out.present_sessions += 1;
            } else {
                out.missing.push(day);
            }
        }
        day = civil_from_days(days_from_civil(day) + 1);
    }
    for date in held {
        if days_from_civil(*date) >= days_from_civil(start)
            && days_from_civil(*date) < days_from_civil(end)
            && !expected.contains(date)
        {
            out.unexpected.push(*date);
        }
    }
    out
}

#[cfg(test)]
#[path = "validate/tests.rs"]
mod tests;
