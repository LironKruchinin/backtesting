//! Where a contract's expiry comes from — and, since D-0085, **when it was
//! known**.
//!
//! Expiries are an **input** to the roll logic, never something it computes,
//! for the reason CLAUDE.md §2.1 gives for every new data source: "as known
//! when?" has to be answerable. A vendor `definition` record answers it (the
//! exchange published the expiry when it listed the contract, long before
//! any roll decided from it); a formula does not.
//!
//! So there are two sources, and a roll table records which one it used:
//!
//! - [`expiries_from_definitions`] (behind the `databento` feature) reads the
//!   `definition` schema out of the archive. This is the real one.
//! - [`nominal_expiry`] computes the **third Friday of the contract month**,
//!   at UTC midnight. It is a fallback and is wrong in two known ways: it is
//!   a date, not the 09:30-Central settlement instant, and third-Friday is
//!   the equity-index convention (ES, NQ, RTY, YM) — CL, ZN, and 6E all
//!   expire on different rules. It exists so that
//!   [`RollRule::CalendarDaysBeforeExpiry`](super::RollRule::CalendarDaysBeforeExpiry)
//!   is testable without vendor data, and so a volume-crossover table can be
//!   built on a machine that has no `definition` files. A table built from it
//!   says `expiry_source = "nominal-third-friday"`, which is the whole point
//!   of recording the field.
//!
//! The core roll logic in [`roll`](super::roll) never mentions either: it
//! takes an [`ExpiryHistory`] and does not care where the history came from.
//! That is what keeps it unit-testable with synthetic fixtures and free of the
//! `databento` feature.
//!
//! ## The availability rule (D-0085, superseding D-0046)
//!
//! An expiry is not a constant. The exchange amends its calendar and the vendor
//! restates the contract, so one `definition` file can carry two `expiration`
//! values for one `instrument_id`. Measured over all seven archived roots:
//! **4 contracts of 1,002 (0.40 %)** do — `GCX2021` (1 h apart), `ZNZ2011`
//! (24 h), `ZNM2012` (24 h), `6EM2023` (72 h). D-0046 refused the file; that
//! refusal is what stopped `crucible rolls` on GC, ZN and 6E.
//!
//! The replacement is §2.1's own question, asked of the definition schema:
//!
//! ```text
//! expiry(contract, decision_ts) =
//!     records.filter(|r| r.ts_recv <= decision_ts)
//!            .max_by_key(|r| r.ts_recv)
//!            .expiration
//! ```
//!
//! [`ExpiryHistory::as_of`] is that function, and the vendor's `ts_recv` is the
//! `avail_ts` of a definition record.
//!
//! ### The one thing not to get wrong
//!
//! **The key is `max(ts_recv)`. It is never `max(expiration)`.** In 4 of 4 real
//! cases the LATER record carries the EARLIER expiry — the exchange pulled the
//! settlement forward and the vendor restated it — so a `max(expiration)`
//! implementation selects the **stale** record every single time and moves ZN's
//! and 6E's rolls onto the wrong session, silently. The two spellings differ by
//! one word. `the_key_is_max_ts_recv_and_never_max_expiration` in this module's
//! tests fails if anyone writes the other one.
//!
//! This is *not* D-0054's "keep `max(created)`" reasoning transplanted, and the
//! analogy is a trap. D-0052/D-0054 pick the later build because delay is the
//! **conservative** direction for availability. Here the later record moves the
//! expiry *earlier*, which is neither conservative nor aggressive — it is simply
//! correct, because the exchange amended its calendar and the vendor said so.
//! What D-0054 *does* contribute is the shape of the residual refusal: the same
//! contract stated twice at one availability instant is a different bug, and it
//! refuses.
//!
//! ## The second consumer: naming a curated partition (D-0072)
//!
//! Expiries answer a question beyond "when must I roll?". They answer **"which
//! contract is this?"** — and `transcode` needs that answer for every bar it
//! files, because a one-digit CME year code repeats every ten years and every
//! bar window in this archive is sixteen years long. `GCZ4` names the December
//! 2014 gold contract *and* the December 2024 one, and both traded inside one
//! raw file.
//!
//! [`ContractCycles`] is that answer, and it is deliberately the *same* device
//! [`expiries_from_definitions`] already uses to separate `ESM0` from `ESM0`:
//! resolve a one-digit year against the contract's own expiry rather than
//! against a constant. Two consumers, one rule.
//!
//! ### Why the two readers disagree about what a conflict is
//!
//! [`expiries_from_definitions`] builds an availability history and resolves a
//! restatement (D-0085). [`contract_cycles_from_definitions`] does not even ask:
//! it is answering a coarser question, and it has **no decision instant to
//! filter against** — a curated partition key is an archival identity, not a
//! choice made at a point in time — so it keeps the observed *span* and refuses
//! only [`ContinuousError::ExpiryYearConflict`], two expiries in different
//! years, where the identity itself becomes unknowable. Measured on the archive:
//! zero contracts across all seven roots have that, so nothing is waved through.
//!
//! One decode pass, two policies over it. That is the same arrangement D-0072
//! set up, and D-0085 only changed what the roll reader does with the records.
//!
//! ### The window rule, and the evidence for it
//!
//! A contract owns the stretch of time ending at its expiry and beginning at
//! the expiry of the previous contract in the same *family* — same root, same
//! month code, same year digit — which is exactly ten years earlier. Where the
//! family has no earlier member, the window opens
//! [`CONTRACT_CYCLE_DAYS`] before the expiry.
//!
//! The windows therefore tile the timeline without gaps or overlaps, so the
//! answer is unique when it exists. It exists for **every outright bar in the
//! archive**: decoding all seven roots' `ohlcv-1m` windows against their
//! `definition` files leaves zero unresolved records, while showing the defect
//! in the open — 101 of GC's 120 raw symbols, 111 of CL's, and 28 of ZN's have
//! a first and a last bar belonging to *different* contracts.

use crucible_core::types::Ts;

use crate::ingest::window::{CivilDate, NANOS_PER_DAY, date_of, days_from_civil};

use super::error::{ContinuousError, ExpiryDisagreement};
use super::symbol::{ContractSymbol, MonthCode, parse_parts};

/// Name recorded in a [`RollTable`](super::RollTable) built with
/// [`nominal_expiry`].
pub const NOMINAL_EXPIRY_SOURCE: &str = "nominal-third-friday";

/// Name recorded when no expiries were needed at all — a volume-crossover
/// table never asks for one.
pub const NO_EXPIRY_SOURCE: &str = "none";

/// Availability stamped on an expiry that was **computed rather than sourced**.
///
/// [`nominal_expiry`] is a formula over the contract symbol, so it is knowable
/// the instant the symbol is (§2.1's question has the answer "always"). Nothing
/// can filter it out, which is the honest representation: the fallback carries
/// no information about when anybody learned anything.
pub const ALWAYS_KNOWN: Ts = Ts(i64::MIN);

/// One statement a source makes about when a contract expires, and the window
/// of availability instants over which the source repeated it.
///
/// A `definition` schema restates every listed instrument daily, so one
/// statement is thousands of identical records; collapsing them to their first
/// and last `ts_recv` is lossless **for [`ExpiryHistory::as_of`]** exactly when
/// the windows of two different statements do not overlap — which
/// [`ExpiryHistoryBuilder::finish`] checks rather than assumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExpiryRevision {
    /// Earliest instant this expiry was knowable: the `ts_recv` of the first
    /// record stating it. **The key [`ExpiryHistory::as_of`] selects on.**
    pub avail_ts: Ts,
    /// `ts_recv` of the last record stating it. Reporting and the overlap
    /// check; never a selection key.
    pub last_avail_ts: Ts,
    /// The expiry stated. Never a selection key — see the module docs.
    pub expiration: Ts,
    /// How many records stated it, for the report.
    pub records: u64,
}

/// Every expiry a source states for each contract, in availability order.
///
/// Replaces the `BTreeMap<ContractSymbol, Ts>` D-0046 passed around: a single
/// value cannot answer "as known when?", and 4 contracts of this archive's
/// 1,002 need it answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpiryHistory {
    /// Revisions per contract, sorted ascending by `avail_ts`, with disjoint
    /// availability windows (enforced at construction).
    by_contract: std::collections::BTreeMap<ContractSymbol, Vec<ExpiryRevision>>,
}

impl ExpiryHistory {
    /// **The rule.** The expiry a decision made at `decision_ts` is entitled to
    /// see: the latest statement whose availability does not lie in that
    /// decision's future.
    ///
    /// `None` means the source said nothing about this contract, or said it
    /// only after `decision_ts` — and those are different sentences with the
    /// same answer here on purpose. Neither entitles a caller to a number.
    #[must_use]
    pub fn as_of(&self, contract: &ContractSymbol, decision_ts: Ts) -> Option<Ts> {
        self.by_contract
            .get(contract)?
            .iter()
            .rev()
            .find(|r| r.avail_ts <= decision_ts)
            .map(|r| r.expiration)
    }

    /// The last statement the source makes, ignoring availability entirely.
    ///
    /// **Display and diagnostics only.** Using this to decide a roll is the
    /// D-0046 behaviour D-0085 replaced: it lets a correction published in 2012
    /// move a roll that happened in 2011. It exists so
    /// [`late_expiry_corrections`](super::roll::late_expiry_corrections) can
    /// say when the two would have differed.
    #[must_use]
    pub fn latest(&self, contract: &ContractSymbol) -> Option<Ts> {
        self.by_contract.get(contract)?.last().map(|r| r.expiration)
    }

    /// Every statement about a contract, ascending in availability. Empty when
    /// the source names no such contract.
    #[must_use]
    pub fn revisions(&self, contract: &ContractSymbol) -> &[ExpiryRevision] {
        self.by_contract.get(contract).map_or(&[], Vec::as_slice)
    }

    /// Whether the source names this contract at all.
    ///
    /// Distinct from `as_of(..).is_some()`: a calendar rule with no expiry for a
    /// contract is a refusal, while one whose expiry is not knowable *yet* is
    /// simply not due yet.
    #[must_use]
    pub fn contains(&self, contract: &ContractSymbol) -> bool {
        self.by_contract.contains_key(contract)
    }

    /// How many contracts the source names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_contract.len()
    }

    /// Every contract the source names, in delivery order.
    pub fn contracts(&self) -> impl Iterator<Item = &ContractSymbol> {
        self.by_contract.keys()
    }

    /// Whether the source names no contract at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_contract.is_empty()
    }

    /// Every contract the source restated, with its statements — the 4-in-1,002
    /// the report exists to show. Ascending by contract, so the order reaches
    /// output deterministically (§2.2).
    pub fn restated(&self) -> impl Iterator<Item = (&ContractSymbol, &[ExpiryRevision])> {
        self.by_contract
            .iter()
            .filter(|(_, revisions)| revisions.len() > 1)
            .map(|(symbol, revisions)| (symbol, revisions.as_slice()))
    }

    /// Builds a history from `(contract, avail_ts, expiration)` observations.
    ///
    /// The convenience form of [`ExpiryHistoryBuilder`], for callers that
    /// already hold every observation.
    ///
    /// # Errors
    /// As [`ExpiryHistoryBuilder::finish`].
    pub fn from_observations<I>(observations: I) -> Result<ExpiryHistory, ContinuousError>
    where
        I: IntoIterator<Item = (ContractSymbol, Ts, Ts)>,
    {
        let mut builder = ExpiryHistoryBuilder::new();
        for (contract, avail_ts, expiration) in observations {
            builder.observe(contract, avail_ts, expiration);
        }
        builder.finish()
    }
}

/// Accumulates definition records into an [`ExpiryHistory`] in bounded memory.
///
/// A 16-year `definition` file restates every instrument every session, so
/// keeping one entry per record would cost hundreds of megabytes for a root
/// like CL to answer a question with two possible answers. The builder keeps one
/// entry per *distinct expiry* instead — 1 for 998 of this archive's contracts,
/// 2 for the other 4 — and records the first and last `ts_recv` that stated it.
///
/// Records may arrive in any order; nothing here depends on file order (§2.2).
#[derive(Debug, Default)]
pub struct ExpiryHistoryBuilder {
    /// Per contract, per distinct expiry: `(first ts_recv, last ts_recv, count)`.
    seen: std::collections::BTreeMap<ContractSymbol, std::collections::BTreeMap<Ts, (Ts, Ts, u64)>>,
}

impl ExpiryHistoryBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> ExpiryHistoryBuilder {
        ExpiryHistoryBuilder::default()
    }

    /// Records one statement: this source said, at `avail_ts`, that `contract`
    /// expires at `expiration`.
    pub fn observe(&mut self, contract: ContractSymbol, avail_ts: Ts, expiration: Ts) {
        self.seen
            .entry(contract)
            .or_default()
            .entry(expiration)
            .and_modify(|(first, last, count)| {
                *first = (*first).min(avail_ts);
                *last = (*last).max(avail_ts);
                *count += 1;
            })
            .or_insert((avail_ts, avail_ts, 1));
    }

    /// Orders the statements and checks they can be ordered at all.
    ///
    /// Two different expiries for one contract are a **revision** when their
    /// availability windows are disjoint: the source said one thing and then
    /// said another, `max(ts_recv)` names the current one, and collapsing the
    /// repeats loses nothing. They are a **conflict** when the windows overlap,
    /// because then the source asserts both at some instant and there is no
    /// latest — the same shape of bug D-0054 calls "the same `(contract,
    /// created)` twice".
    ///
    /// # Errors
    /// [`ContinuousError::ExpiryConflict`] listing **every** offending contract
    /// in the input. Returning on the first is how `ZNM2012` stayed hidden
    /// behind `ZNZ2011` from the day the archive landed (D-0085).
    pub fn finish(self) -> Result<ExpiryHistory, ContinuousError> {
        let mut by_contract = std::collections::BTreeMap::new();
        let mut conflicts = Vec::new();
        for (contract, expiries) in self.seen {
            let mut revisions: Vec<ExpiryRevision> = expiries
                .into_iter()
                .map(
                    |(expiration, (avail_ts, last_avail_ts, records))| ExpiryRevision {
                        avail_ts,
                        last_avail_ts,
                        expiration,
                        records,
                    },
                )
                .collect();
            // Sorted by availability, which is what `as_of` scans; the map that
            // produced them was keyed by expiry, which is emphatically not.
            revisions.sort();
            let mut clean = true;
            for pair in revisions.windows(2) {
                if pair[0].last_avail_ts >= pair[1].avail_ts {
                    conflicts.push(ExpiryDisagreement {
                        contract: contract.to_string(),
                        first: pair[0].expiration,
                        first_avail: (pair[0].avail_ts, pair[0].last_avail_ts),
                        second: pair[1].expiration,
                        second_avail: (pair[1].avail_ts, pair[1].last_avail_ts),
                    });
                    clean = false;
                }
            }
            if clean {
                by_contract.insert(contract, revisions);
            }
        }
        if conflicts.is_empty() {
            Ok(ExpiryHistory { by_contract })
        } else {
            Err(ContinuousError::ExpiryConflict { conflicts })
        }
    }
}

/// UTC midnight of the third Friday of a contract's delivery month.
///
/// A **fallback**, with the caveats in the module docs. Never a substitute
/// for the `definition` schema.
#[must_use]
pub fn nominal_expiry(symbol: &ContractSymbol) -> Ts {
    let first = CivilDate {
        year: i64::from(symbol.year()),
        month: symbol.month().month(),
        day: 1,
    };
    let first_day = days_from_civil(first);
    // 1970-01-01 is a Thursday, so `(epoch_day + 4) % 7` numbers the week
    // from Sunday = 0, making Friday 5.
    let weekday = (first_day + 4).rem_euclid(7);
    let to_first_friday = (5 - weekday).rem_euclid(7);
    Ts((first_day + to_first_friday + 14) * NANOS_PER_DAY)
}

/// Nominal expiries for every symbol given, as a one-statement-per-contract
/// history stamped [`ALWAYS_KNOWN`].
///
/// # Panics
/// Never in practice: a computed expiry produces exactly one statement per
/// contract, so [`ExpiryHistoryBuilder::finish`] has no pair to find an overlap
/// between. The `expect` states that, rather than pushing an impossible
/// `Result` onto every caller (CLAUDE.md §5.1).
#[must_use]
pub fn nominal_expiries<'a, I>(symbols: I) -> ExpiryHistory
where
    I: IntoIterator<Item = &'a ContractSymbol>,
{
    ExpiryHistory::from_observations(
        symbols
            .into_iter()
            .map(|s| (s.clone(), ALWAYS_KNOWN, nominal_expiry(s))),
    )
    .expect("INVARIANT: one computed expiry per contract cannot disagree with itself")
}

/// How far back the *first* contract of a family may reach.
///
/// A one-digit CME year code repeats every ten years, so two contracts sharing
/// a family are exactly ten calendar years apart — 3,652 or 3,653 days,
/// depending on how many leap days the decade holds. 3,653 is the larger.
///
/// This constant is only ever the lower bound of a family's **earliest**
/// contract: every later one opens at its predecessor's expiry instead, so the
/// windows tile exactly however far apart two real expiries turn out to be
/// (measured across this archive: 3,647 to 3,657 days, since the expiry *date*
/// drifts within its month). Where it does apply, precision is irrelevant — the
/// bars it is there to catch are a whole cycle out, not a week — and no listed
/// future reaches ten years past its own listing: the longest at CME is crude
/// oil at nine.
pub const CONTRACT_CYCLE_DAYS: i64 = 3_653;

/// One contract, and the stretch of time bars may carry its vendor spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cycle {
    /// The contract, with an absolute year.
    symbol: ContractSymbol,
    /// Earliest expiry the definition file recorded for it.
    expiry_earliest: Ts,
    /// Latest expiry the definition file recorded for it. Equal to
    /// `expiry_earliest` unless the vendor restated the contract with a
    /// slightly different settlement instant, which it does.
    expiry_latest: Ts,
}

/// Which contract a vendor symbol names at a given instant.
///
/// Built from a root's archived `definition` records. The module docs carry the
/// window rule and the evidence; the short version is that a one-digit year is
/// resolved against the contract's **own expiry**, never against a
/// [`DecadeAnchor`](super::DecadeAnchor) constant, because a constant cannot
/// separate two contracts that one file contains (D-0046, D-0072).
#[derive(Debug, Clone, Default)]
pub struct ContractCycles {
    /// Keyed by (month, year digit) — the family a one-digit spelling lands in.
    /// Values are sorted by expiry, which is what makes the windows tile.
    families: std::collections::BTreeMap<(MonthCode, u32), Vec<Cycle>>,
}

impl ContractCycles {
    /// Builds a resolver from expiry spans: `(contract, earliest, latest)`.
    ///
    /// # Errors
    /// [`ContinuousError::ExpiryYearConflict`] if any contract's two extreme
    /// expiries fall in different calendar years — the case where the year that
    /// resolves the code is itself unknown.
    pub fn from_spans<I>(spans: I) -> Result<ContractCycles, ContinuousError>
    where
        I: IntoIterator<Item = (ContractSymbol, Ts, Ts)>,
    {
        let mut families: std::collections::BTreeMap<(MonthCode, u32), Vec<Cycle>> =
            std::collections::BTreeMap::new();
        for (symbol, earliest, latest) in spans {
            let (lo, hi) = if earliest <= latest {
                (earliest, latest)
            } else {
                (latest, earliest)
            };
            if date_of_year(lo) != date_of_year(hi) {
                return Err(ContinuousError::ExpiryYearConflict {
                    contract: symbol.to_string(),
                    earliest: lo,
                    latest: hi,
                });
            }
            let digit = u32::try_from(symbol.year().rem_euclid(10)).unwrap_or(0);
            families
                .entry((symbol.month(), digit))
                .or_default()
                .push(Cycle {
                    symbol,
                    expiry_earliest: lo,
                    expiry_latest: hi,
                });
        }
        for cycles in families.values_mut() {
            cycles.sort_by(|a, b| {
                (a.expiry_earliest, &a.symbol).cmp(&(b.expiry_earliest, &b.symbol))
            });
            // Members of a family are, by the arithmetic of a one-digit year
            // code, about ten years apart. Two that are not mean the definition
            // file put at least one of them in the wrong decade — and the
            // windows below would then be narrow slivers rather than cycles,
            // so a bar would be filed by rounding. Half a cycle is the
            // threshold because nothing legitimate comes close to it: the
            // narrowest real gap this archive contains is 3,647 days.
            for pair in cycles.windows(2) {
                let apart = pair[1].expiry_earliest.0 - pair[0].expiry_latest.0;
                if apart < CONTRACT_CYCLE_DAYS / 2 * NANOS_PER_DAY {
                    return Err(ContinuousError::ContractCycleCollision {
                        first: pair[0].symbol.to_string(),
                        second: pair[1].symbol.to_string(),
                        apart_days: apart / NANOS_PER_DAY,
                    });
                }
            }
        }
        Ok(ContractCycles { families })
    }

    /// Builds a resolver from an [`ExpiryHistory`], collapsing each contract's
    /// statements to the span they cover.
    ///
    /// The span, not the current statement: naming a curated partition is an
    /// archival identity question with no decision instant to filter against, so
    /// a bar printed in the hour between two restatements must still resolve
    /// (D-0072, D-0085).
    ///
    /// # Errors
    /// [`ContinuousError::ExpiryYearConflict`] if a contract's extreme expiries
    /// fall in different calendar years.
    pub fn from_expiries(history: &ExpiryHistory) -> Result<ContractCycles, ContinuousError> {
        ContractCycles::from_spans(
            history
                .by_contract
                .iter()
                .filter_map(|(symbol, revisions)| {
                    let lo = revisions.iter().map(|r| r.expiration).min()?;
                    let hi = revisions.iter().map(|r| r.expiration).max()?;
                    Some((symbol.clone(), lo, hi))
                }),
        )
    }

    /// Whether any contract is known at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    /// How many contracts the resolver knows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.families.values().map(Vec::len).sum()
    }

    /// Resolves a vendor symbol observed at `ts` into an unambiguous contract.
    ///
    /// A two- or four-digit year is absolute, so it is returned without
    /// consulting anything: no expiry is needed, and demanding one would refuse
    /// far-dated listings the vendor itself spells `CLZ36` precisely to avoid
    /// the ambiguity. Only a one-digit year is resolved against the archive.
    ///
    /// # Errors
    /// [`ContinuousError::UnparseableSymbol`] if `symbol` is not an outright
    /// contract, and [`ContinuousError::UnresolvedContract`] if a one-digit
    /// year has no candidate at all, or none whose cycle contains `ts`.
    pub fn resolve(&self, symbol: &str, ts: Ts) -> Result<ContractSymbol, ContinuousError> {
        self.resolve_window(symbol, ts).map(|(symbol, _, _)| symbol)
    }

    /// [`ContractCycles::resolve`], plus the window over which the answer holds:
    /// `(opens, closes]`, exclusive on the left and inclusive on the right.
    ///
    /// A caller resolving millions of bar records uses the window to know when
    /// it may reuse the previous answer — and, more usefully, the window is the
    /// statement being made: *this* spelling means *this* contract only over
    /// *this* stretch. An unambiguous spelling holds for all time and says so.
    ///
    /// # Errors
    /// As [`ContractCycles::resolve`].
    pub fn resolve_window(
        &self,
        symbol: &str,
        ts: Ts,
    ) -> Result<(ContractSymbol, Ts, Ts), ContinuousError> {
        let parts = parse_parts(symbol)?;
        if !parts.has_ambiguous_year() {
            let resolved = ContractSymbol::new(parts.root, absolute_year(&parts), parts.month)?;
            return Ok((resolved, Ts(i64::MIN), Ts(i64::MAX)));
        }
        let empty: Vec<Cycle> = Vec::new();
        let family = self
            .families
            .get(&(parts.month, parts.year_value))
            .unwrap_or(&empty);
        let mine: Vec<&Cycle> = family
            .iter()
            .filter(|c| c.symbol.root() == parts.root)
            .collect();
        for (i, cycle) in mine.iter().enumerate() {
            // The window opens where the previous contract of this family
            // closed, so the windows tile with no gap and no overlap. The
            // family's earliest member has no predecessor, so it opens one
            // cycle before its own expiry.
            let opens = if i == 0 {
                Ts(cycle.expiry_earliest.0 - CONTRACT_CYCLE_DAYS * NANOS_PER_DAY)
            } else {
                mine[i - 1].expiry_latest
            };
            if ts > opens && ts <= cycle.expiry_latest {
                return Ok((cycle.symbol.clone(), opens, cycle.expiry_latest));
            }
        }
        Err(ContinuousError::UnresolvedContract {
            symbol: symbol.to_owned(),
            ts,
            candidates: mine
                .iter()
                .map(|c| (c.symbol.to_string(), c.expiry_latest))
                .collect(),
        })
    }
}

/// The absolute year an already-unambiguous spelling names.
fn absolute_year(parts: &super::symbol::SymbolParts<'_>) -> i32 {
    let value = i32::try_from(parts.year_value).unwrap_or(0);
    if parts.year_digits == 4 {
        value
    } else {
        2000 + value
    }
}

/// Calendar year of a UTC timestamp.
fn date_of_year(ts: Ts) -> i64 {
    date_of(ts).year
}

#[cfg(feature = "databento")]
pub use imp::{contract_cycles_from_definitions, expiries_from_definitions};

#[cfg(feature = "databento")]
pub(crate) mod imp {
    use std::collections::BTreeMap;
    use std::path::Path;

    use crucible_core::types::Ts;

    use crate::ingest::window::date_of;
    use databento::dbn::decode::{DecodeRecord, dbn::Decoder};
    use databento::dbn::{InstrumentClass, UNDEF_TIMESTAMP, record::InstrumentDefMsg};

    use crate::continuous::error::ContinuousError;
    use crate::continuous::symbol::{ContractSymbol, DecadeAnchor};

    use super::{ContractCycles, ExpiryHistory, ExpiryHistoryBuilder};

    /// One decode pass over a `definition` file, handing every outright record
    /// of `root` to `on_record` as `(contract, ts_recv, expiration)`.
    ///
    /// The `ts_recv` is `None` when the vendor left it null or out of `i64`
    /// range. It is passed through rather than refused here because the two
    /// callers need different answers: the roll reader cannot use a record whose
    /// availability is unknown (§2.1), while the cycle reader never asks
    /// (D-0085).
    ///
    /// Only records whose `instrument_class` is exactly
    /// [`InstrumentClass::Future`] are kept. `InstrumentClass::is_future()` is
    /// **not** used: it returns true for `FutureSpread` too, and a calendar
    /// spread carries an expiry that would then compete with an outright's
    /// under the same parsed symbol.
    ///
    /// Records for other roots, records whose symbol is not an outright
    /// contract, and records with a null `expiration` are skipped silently — a
    /// `definition` file for a parent key legitimately contains thousands of
    /// each.
    fn decode_definitions<F>(
        path: &Path,
        root: &str,
        anchor: DecadeAnchor,
        mut on_record: F,
    ) -> Result<(), ContinuousError>
    where
        F: FnMut(ContractSymbol, Option<Ts>, Ts),
    {
        let undecodable = |detail: String| ContinuousError::Undecodable {
            path: path.to_path_buf(),
            detail,
        };
        let mut decoder = Decoder::from_zstd_file(path).map_err(|e| undecodable(e.to_string()))?;

        while let Some(msg) = decoder
            .decode_record::<InstrumentDefMsg>()
            .map_err(|e| undecodable(e.to_string()))?
        {
            if msg.instrument_class().ok() != Some(InstrumentClass::Future) {
                continue;
            }
            if msg.expiration == UNDEF_TIMESTAMP {
                continue;
            }
            let Ok(raw_symbol) = msg.raw_symbol() else {
                continue;
            };
            // `raw_symbol` borrows the decoder's buffer; own it before the
            // next `decode_record` invalidates it.
            let raw_symbol = raw_symbol.to_owned();
            let expiration = i64::try_from(msg.expiration)
                .map_err(|_| undecodable(format!("expiration {} exceeds i64", msg.expiration)))?;
            let expiry = Ts(expiration);
            // The availability of a definition record IS the vendor's `ts_recv`
            // (D-0085). `UNDEF_TIMESTAMP` is `u64::MAX`, so it fails this
            // conversion and arrives as `None` — the honest answer to "as known
            // when?" when the record does not say.
            let avail_ts = if msg.ts_recv == UNDEF_TIMESTAMP {
                None
            } else {
                i64::try_from(msg.ts_recv).ok().map(Ts)
            };

            // A 16-year `definition` file contains `ESM0` twice — June 2010
            // and June 2020 — so no single [`DecadeAnchor`] can separate them,
            // and D-0046's constant made the two collide into one contract with
            // two expiries a decade apart. The record can separate them:
            // resolve the one-digit year against the contract's *own* expiry
            // instead of a constant. Anchoring on the expiry year
            // rather than reading the year off it directly is deliberate —
            // some products expire in the month before the contract month, so
            // `CLF5` expiring in December 2024 must still resolve to 2025.
            // `anchor` remains the fallback for a timestamp we cannot place,
            // and two- and four-digit years are absolute either way.
            //
            // This is the device D-0072 reuses to name curated partitions:
            // same rule, second consumer.
            let record_anchor =
                i32::try_from(date_of(expiry).year).map_or(anchor, DecadeAnchor::new);
            let Ok(symbol) = ContractSymbol::parse_with_anchor(&raw_symbol, record_anchor) else {
                continue;
            };
            if symbol.root() != root {
                continue;
            }
            on_record(symbol, avail_ts, expiry);
        }
        Ok(())
    }

    /// Reads outright futures expiries out of a `definition` DBN file, as an
    /// availability history (D-0085).
    ///
    /// A contract restated with a *different* expiry resolves: the statements
    /// are ordered by `ts_recv` and [`ExpiryHistory::as_of`] picks the one a
    /// given decision was entitled to see. Only statements whose availability
    /// windows *overlap* refuse, and then every offending contract in the root
    /// is named.
    ///
    /// # Errors
    /// [`ContinuousError::Undecodable`] if the file cannot be opened or
    /// decoded, [`ContinuousError::UnavailableExpiry`] if a record states an
    /// expiry without a `ts_recv`, and [`ContinuousError::ExpiryConflict`] if
    /// one contract's statements cannot be ordered.
    pub fn expiries_from_definitions(
        path: &Path,
        root: &str,
        anchor: DecadeAnchor,
    ) -> Result<ExpiryHistory, ContinuousError> {
        let mut builder = ExpiryHistoryBuilder::new();
        let mut unavailable: Option<(ContractSymbol, Ts)> = None;
        decode_definitions(path, root, anchor, |symbol, avail_ts, expiry| {
            match avail_ts {
                Some(avail_ts) => builder.observe(symbol, avail_ts, expiry),
                // Refused after the pass rather than during it, so the closure
                // stays infallible and the first such record is reported with
                // its contract rather than as a decoder error.
                None => unavailable = unavailable.take().or(Some((symbol, expiry))),
            }
        })?;
        if let Some((contract, expiration)) = unavailable {
            return Err(ContinuousError::UnavailableExpiry {
                contract: contract.to_string(),
                expiration,
            });
        }
        builder.finish()
    }

    /// Reads the contract cycles a root's `definition` file describes.
    ///
    /// The input to [`ContractCycles::resolve`], and therefore to every curated
    /// partition key `transcode` writes (D-0072). It reads the observed *span*
    /// of each contract's expiries and never their availability, because a
    /// partition key is an archival identity rather than a decision made at an
    /// instant — which is also why a null `ts_recv` cannot fail this reader.
    ///
    /// # Errors
    /// [`ContinuousError::Undecodable`] if the file cannot be opened or
    /// decoded, and [`ContinuousError::ExpiryYearConflict`] if one contract is
    /// given expiries in two different years — the disagreement that makes the
    /// contract's identity, not merely its roll date, unknowable.
    pub fn contract_cycles_from_definitions(
        path: &Path,
        root: &str,
        anchor: DecadeAnchor,
    ) -> Result<ContractCycles, ContinuousError> {
        let mut spans: BTreeMap<ContractSymbol, (Ts, Ts)> = BTreeMap::new();
        decode_definitions(path, root, anchor, |symbol, _avail_ts, expiry| {
            spans
                .entry(symbol)
                .and_modify(|(lo, hi)| {
                    *lo = (*lo).min(expiry);
                    *hi = (*hi).max(expiry);
                })
                .or_insert((expiry, expiry));
        })?;
        ContractCycles::from_spans(spans.into_iter().map(|(s, (lo, hi))| (s, lo, hi)))
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use super::*;

        use std::fs::File;

        use crate::testutil::TempDir;
        use databento::dbn::encode::EncodeRecord;
        use databento::dbn::encode::dbn::Encoder;
        use databento::dbn::record::str_to_c_chars;
        use databento::dbn::{Metadata, RecordHeader, SType, Schema, rtype};

        const JAN1: i64 = 1_704_067_200_000_000_000;
        /// The same instant in the vendor's unsigned nanoseconds.
        const JAN1_NS: u64 = 1_704_067_200_000_000_000;

        /// Writes a `definition` DBN file: `(raw_symbol, class, expiration)`,
        /// every record stamped with the same `ts_recv`.
        ///
        /// Same availability for every row is the *hard* case for D-0085 and so
        /// the right default for a fixture that is not about availability: two
        /// different expiries written this way cannot be ordered and must
        /// refuse. Use [`write_definitions_with_recv`] to write a restatement.
        ///
        /// `pub(crate)` because `transcode`'s fixtures need one too — a bar
        /// window cannot be transcoded without the expiries that name its
        /// contracts (D-0072), so every transcode fixture now plants the
        /// definition file a real archive would have.
        pub(crate) fn write_definitions(path: &Path, rows: &[(&str, InstrumentClass, u64)]) {
            let stamped: Vec<(&str, InstrumentClass, u64, u64)> = rows
                .iter()
                .map(|(symbol, class, expiration)| (*symbol, *class, *expiration, JAN1_NS))
                .collect();
            write_definitions_with_recv(path, &stamped);
        }

        /// Writes a `definition` DBN file:
        /// `(raw_symbol, class, expiration, ts_recv)`.
        ///
        /// The fourth column is the whole of D-0085: it is when the vendor said
        /// the third one.
        pub(crate) fn write_definitions_with_recv(
            path: &Path,
            rows: &[(&str, InstrumentClass, u64, u64)],
        ) {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            let metadata = Metadata::builder()
                .dataset("GLBX.MDP3".to_owned())
                .schema(Some(Schema::Definition))
                .start(1_704_067_200_000_000_000)
                .stype_in(Some(SType::Parent))
                .stype_out(SType::InstrumentId)
                .symbols(vec!["ES.FUT".to_owned()])
                .build();
            let file = File::create(path).expect("create");
            let mut encoder = Encoder::with_zstd(file, &metadata).expect("encoder");
            for (index, (symbol, class, expiration, ts_recv)) in rows.iter().enumerate() {
                let mut msg = InstrumentDefMsg {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "fixture instrument ids are tiny"
                    )]
                    hd: RecordHeader::new::<InstrumentDefMsg>(
                        rtype::INSTRUMENT_DEF,
                        1,
                        index as u32 + 1,
                        1_704_067_200_000_000_000,
                    ),
                    ts_recv: *ts_recv,
                    ..InstrumentDefMsg::default()
                };
                msg.raw_symbol = str_to_c_chars(symbol).expect("short enough");
                msg.instrument_class = *class as i8;
                msg.expiration = *expiration;
                msg.leg_count = if matches!(class, InstrumentClass::FutureSpread) {
                    2
                } else {
                    0
                };
                encoder.encode_record(&msg).expect("encode");
            }
            drop(encoder);
        }

        #[test]
        fn outright_futures_are_kept_and_everything_else_is_skipped() {
            let dir = TempDir::new();
            let path = dir
                .path()
                .join("raw/GLBX.MDP3/definition/ES.FUT/2024-01.dbn.zst");
            let jan1 = JAN1_NS;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, jan1),
                    // A calendar spread. `is_future()` would let this in, and
                    // it parses as no outright anyway — belt and braces.
                    ("ESH4-ESM4", InstrumentClass::FutureSpread, jan1 + 1),
                    // Another root.
                    ("NQH4", InstrumentClass::Future, jan1 + 2),
                    // A null expiration.
                    ("ESM4", InstrumentClass::Future, UNDEF_TIMESTAMP),
                    ("ESU4", InstrumentClass::Future, jan1 + 3),
                ],
            );
            let history =
                expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT).expect("import");
            let names: Vec<String> = history.contracts().map(ToString::to_string).collect();
            assert_eq!(names, vec!["ESH2024", "ESU2024"]);
            assert_eq!(
                history.as_of(&ContractSymbol::parse("ESH4").expect("valid"), Ts(JAN1)),
                Some(Ts(JAN1))
            );
        }

        // The archive's real `GC.FUT` definition file gives `GCX2021` two
        // expiries one hour apart, and `6EM2023` two seventy-two hours apart —
        // and in every real case the two arrive at DIFFERENT `ts_recv`, which is
        // what makes them a restatement rather than a contradiction (D-0085).
        // The roll reader resolves them to the later statement; the cycle reader
        // keeps the span, because an hour cannot make a contract a different
        // contract and refusing would make every GC bar untranscodable (D-0072).
        #[test]
        fn an_hours_restatement_resolves_for_the_roll_reader_and_spans_for_the_cycle_reader() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            let hour: u64 = 3_600_000_000_000;
            let day: u64 = 86_400_000_000_000;
            write_definitions_with_recv(
                &path,
                &[
                    // Stated first, expiring an hour LATER — the real direction.
                    ("ESH4", InstrumentClass::Future, JAN1_NS + hour, JAN1_NS),
                    // Restated a day later, expiring an hour EARLIER.
                    ("ESH4", InstrumentClass::Future, JAN1_NS, JAN1_NS + day),
                ],
            );

            let history =
                expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT).expect("resolves");
            let esh4 = ContractSymbol::parse("ESH4").expect("valid");
            let hour_i64 = i64::try_from(hour).expect("fits");
            let day_i64 = i64::try_from(day).expect("fits");
            // Before the restatement, the first statement stands.
            assert_eq!(
                history.as_of(&esh4, Ts(JAN1 + day_i64 - 1)),
                Some(Ts(JAN1 + hour_i64))
            );
            // From the restatement onward, the later statement does — and it is
            // the EARLIER expiry, which is why the key is ts_recv.
            assert_eq!(history.as_of(&esh4, Ts(JAN1 + day_i64)), Some(Ts(JAN1)));

            let cycles = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect("the cycle reader tolerates an hour");
            assert_eq!(cycles.len(), 1);
            // And the span it kept reaches the *later* expiry, so a bar printed
            // in the disputed hour still resolves.
            assert_eq!(
                cycles
                    .resolve("ESH4", Ts(JAN1 + hour_i64))
                    .expect("resolves")
                    .to_string(),
                "ESH2024"
            );
        }

        // Two expiries in different YEARS is where the identity itself becomes
        // unknowable, and that is the one the cycle reader refuses. The control
        // for the tolerance above not being a blanket one.
        #[test]
        fn expiries_a_year_apart_refuse_the_cycle_reader_too() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            // JAN1_NS is 2024-01-01 and 2024 is a leap year, so 365 days would
            // land on 2024-12-31 and still be the same year. 400 does not.
            let year = 400 * 24 * 3_600_000_000_000_u64;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, JAN1_NS),
                    ("ESH4", InstrumentClass::Future, JAN1_NS + year),
                ],
            );
            let err = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect_err("a year is not an hour");
            assert!(
                matches!(err, ContinuousError::ExpiryYearConflict { .. }),
                "{err}"
            );
        }

        // Repeated definitions are the norm — the schema restates every
        // instrument daily — so agreeing repeats must be fine.
        #[test]
        fn a_repeated_definition_that_agrees_is_fine() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            let jan1 = JAN1_NS;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, jan1),
                    ("ESH4", InstrumentClass::Future, jan1),
                ],
            );
            let found =
                expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT).expect("import");
            assert_eq!(found.len(), 1);
            let esh4 = ContractSymbol::parse("ESH4").expect("valid");
            assert_eq!(
                found.revisions(&esh4).len(),
                1,
                "one statement, however many times it was repeated"
            );
            assert_eq!(found.revisions(&esh4)[0].records, 2);
        }

        // The residual refusal, and the only one D-0085 keeps: two expiries at
        // ONE availability instant. `write_definitions` stamps every row with
        // the same `ts_recv`, so there is no later statement to take, and
        // `max(ts_recv)` has nothing to pick — D-0054's "the same
        // `(contract, created)` twice is a different bug" in this schema.
        #[test]
        fn two_expiries_at_one_availability_instant_still_refuse() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            let jan1 = JAN1_NS;
            write_definitions(
                &path,
                &[
                    ("ESH4", InstrumentClass::Future, jan1),
                    ("ESH4", InstrumentClass::Future, jan1 + 1),
                ],
            );
            let err = expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect_err("must refuse two simultaneous answers");
            assert!(
                matches!(err, ContinuousError::ExpiryConflict { .. }),
                "{err}"
            );
        }

        // A record that states an expiry without saying when it was knowable
        // has no answer to §2.1's first question, so the roll reader refuses it
        // — while the cycle reader, which never asks, still reads the file.
        #[test]
        fn an_expiry_with_no_ts_recv_refuses_the_roll_reader_only() {
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            write_definitions_with_recv(
                &path,
                &[("ESH4", InstrumentClass::Future, JAN1_NS, UNDEF_TIMESTAMP)],
            );
            let err = expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect_err("as known when?");
            match &err {
                ContinuousError::UnavailableExpiry { contract, .. } => {
                    assert_eq!(contract, "ESH2024", "{err}");
                }
                other => panic!("wrong refusal: {other}"),
            }
            let cycles = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect("identity does not need an availability");
            assert_eq!(cycles.len(), 1);
        }

        // The whole point, end to end through a real DBN encode: the two
        // `ESM0`s of a sixteen-year window separate into two contracts, and a
        // bar carrying that spelling resolves by *when it printed*.
        #[test]
        fn one_vendor_spelling_resolves_to_two_contracts_by_timestamp() {
            const JUN_2010: u64 = 1_276_867_800_000_000_000;
            const JUN_2020: u64 = 1_592_573_400_000_000_000;
            let dir = TempDir::new();
            let path = dir.path().join("raw/def.dbn.zst");
            write_definitions(
                &path,
                &[
                    ("ESM0", InstrumentClass::Future, JUN_2010),
                    ("ESM0", InstrumentClass::Future, JUN_2020),
                ],
            );
            let cycles = contract_cycles_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
                .expect("two contracts, not a collision");
            assert_eq!(cycles.len(), 2);

            // A bar from March 2010 and one from March 2020 carry the same
            // four characters and are ten years and one contract apart.
            let mar_2010 = Ts(1_267_401_600_000_000_000);
            let mar_2020 = Ts(1_583_020_800_000_000_000);
            assert_eq!(
                cycles
                    .resolve("ESM0", mar_2010)
                    .expect("resolves")
                    .to_string(),
                "ESM2010"
            );
            assert_eq!(
                cycles
                    .resolve("ESM0", mar_2020)
                    .expect("resolves")
                    .to_string(),
                "ESM2020"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::window::date_of;

    fn expiry_date(symbol: &str) -> String {
        let s = ContractSymbol::parse(symbol).expect("valid symbol");
        date_of(nominal_expiry(&s)).to_string()
    }

    // Hand-derived against a wall calendar, and cross-checked against the
    // real CME equity-index expiries for 2024:
    //   ESH4 -> 2024-03-15 (2024-03-01 was itself a Friday)
    //   ESM4 -> 2024-06-21 (2024-06-01 was a Saturday; first Friday the 7th)
    //   ESU4 -> 2024-09-20 (2024-09-01 was a Sunday; first Friday the 6th)
    //   ESZ4 -> 2024-12-20 (2024-12-01 was a Sunday; first Friday the 6th)
    #[test]
    fn nominal_expiry_is_the_third_friday_of_the_delivery_month() {
        assert_eq!(expiry_date("ESH4"), "2024-03-15");
        assert_eq!(expiry_date("ESM4"), "2024-06-21");
        assert_eq!(expiry_date("ESU4"), "2024-09-20");
        assert_eq!(expiry_date("ESZ4"), "2024-12-20");
    }

    // A month starting on a Friday and one starting on a Thursday bracket the
    // wrap-around in the weekday arithmetic.
    #[test]
    fn the_weekday_arithmetic_wraps_correctly() {
        // 2025-08-01 was a Friday -> third Friday is the 15th.
        assert_eq!(expiry_date("ESQ25"), "2025-08-15");
        // 2026-01-01 is a Thursday -> first Friday the 2nd, third the 16th.
        assert_eq!(expiry_date("ESF26"), "2026-01-16");
    }

    #[test]
    fn a_map_can_be_built_for_a_whole_chain() {
        let symbols: Vec<ContractSymbol> = ["ESH4", "ESM4", "ESU4"]
            .iter()
            .map(|s| ContractSymbol::parse(s).expect("valid"))
            .collect();
        let history = nominal_expiries(&symbols);
        assert_eq!(history.len(), 3);
        assert!(
            history.contracts().eq(symbols.iter()),
            "contracts are in delivery order"
        );
        // A computed expiry is knowable whenever the symbol is, so nothing can
        // filter it out — including a decision at the dawn of the epoch.
        for symbol in &symbols {
            assert_eq!(
                history.as_of(symbol, Ts(i64::MIN)),
                Some(nominal_expiry(symbol))
            );
        }
    }

    // ---------------------------------------- the availability rule (D-0085)

    const HOUR: i64 = 3_600_000_000_000;
    const A_DAY: i64 = 24 * HOUR;

    fn contract(name: &str) -> ContractSymbol {
        ContractSymbol::parse(name).expect("valid symbol")
    }

    /// One contract's raw records, as `(ts_recv, expiration)` in nanoseconds.
    fn history_of(name: &str, records: &[(i64, i64)]) -> ExpiryHistory {
        ExpiryHistory::from_observations(
            records
                .iter()
                .map(|(recv, expiry)| (contract(name), Ts(*recv), Ts(*expiry))),
        )
        .expect("orderable")
    }

    /// The architect's rule, written out verbatim over raw records — the
    /// reference [`ExpiryHistory::as_of`]'s collapsed form must reproduce.
    fn brute_force_as_of(records: &[(i64, i64)], decision_ts: i64) -> Option<i64> {
        records
            .iter()
            .filter(|(recv, _)| *recv <= decision_ts)
            .max_by_key(|(recv, _)| *recv)
            .map(|(_, expiry)| *expiry)
    }

    /// `ZNZ2011`'s shape: stated once, restated 24 h EARLIER a week later.
    /// Numbers are round rather than real; the direction is the real one, and
    /// the direction is the whole trap.
    const RESTATED_EARLIER: [(i64, i64); 2] = [
        (100 * A_DAY, 200 * A_DAY),
        (107 * A_DAY, 200 * A_DAY - A_DAY),
    ];

    // ======================================================================
    // THE TRAP CONTROL. In 4 of 4 real cases the LATER record carries the
    // EARLIER expiry, so an implementation keyed on `max(expiration)` — the
    // shape D-0054's "keep max(created)" invites by analogy — selects the STALE
    // record every single time and moves ZN's and 6E's rolls onto the wrong
    // session, silently. This test fails on that implementation and passes on
    // `max(ts_recv)`; the two spellings differ by one word.
    // ======================================================================
    #[test]
    fn the_key_is_max_ts_recv_and_never_max_expiration() {
        let history = history_of("ZNZ2011", &RESTATED_EARLIER);
        let zn = contract("ZNZ2011");
        let after_everything = Ts(1_000 * A_DAY);

        let answer = history.as_of(&zn, after_everything).expect("known by then");
        let by_ts_recv = Ts(RESTATED_EARLIER[1].1);
        let by_expiration = Ts(RESTATED_EARLIER[0].1);

        assert_eq!(
            answer, by_ts_recv,
            "the rule keys on max(ts_recv): the later STATEMENT wins"
        );
        assert_ne!(
            answer, by_expiration,
            "keying on max(expiration) would pick the stale record — the trap"
        );
        // Stated as arithmetic so a reader cannot mistake which is which: the
        // record that wins is the one that arrived later AND expires earlier.
        assert!(
            RESTATED_EARLIER[1].0 > RESTATED_EARLIER[0].0
                && RESTATED_EARLIER[1].1 < RESTATED_EARLIER[0].1,
            "the fixture must have the real data's direction, or it proves nothing"
        );
        assert_eq!(history.latest(&zn), Some(by_ts_recv));
    }

    // The availability filter, biting. A correction whose `ts_recv` falls after
    // the decision instant must NOT be used, and the pre-correction expiry must
    // be the answer — because a backtest standing at that instant could not have
    // read the correction. This is the reason the rule is not plain latest-wins.
    #[test]
    fn a_correction_that_lands_after_the_decision_is_not_used() {
        let history = history_of("ZNZ2011", &RESTATED_EARLIER);
        let zn = contract("ZNZ2011");
        let correction_ts = RESTATED_EARLIER[1].0;

        // One nanosecond before the correction: the original statement stands.
        assert_eq!(
            history.as_of(&zn, Ts(correction_ts - 1)),
            Some(Ts(RESTATED_EARLIER[0].1)),
            "a correction that has not landed cannot move a decision"
        );
        // At the instant it lands: it is available, so it is used. The boundary
        // is inclusive, exactly as the rule is written (`ts_recv <= decision`).
        assert_eq!(
            history.as_of(&zn, Ts(correction_ts)),
            Some(Ts(RESTATED_EARLIER[1].1))
        );
        // And before anything at all was said, there is no expiry — not the
        // first one, and certainly not the last.
        assert_eq!(history.as_of(&zn, Ts(RESTATED_EARLIER[0].0 - 1)), None);
        assert!(history.contains(&zn), "the contract is still known ABOUT");
    }

    // THE THIRD SIDE (CLAUDE.md §7). The two tests above show `max(ts_recv)` and
    // `max(expiration)` disagree. This shows they AGREE the moment the
    // correction moves the expiry LATER instead of earlier — so what makes them
    // differ is the *direction* of the correction and nothing else about the
    // fixture. That is also why the real archive is dangerous: its four
    // corrections all move the expiry earlier, so the wrong key is wrong every
    // time rather than half the time.
    #[test]
    fn the_two_keys_agree_when_a_correction_moves_the_expiry_later() {
        let later: [(i64, i64); 2] = [
            (100 * A_DAY, 200 * A_DAY),
            (107 * A_DAY, 200 * A_DAY + A_DAY),
        ];
        let history = history_of("ZNZ2011", &later);
        let zn = contract("ZNZ2011");
        let answer = history.as_of(&zn, Ts(1_000 * A_DAY)).expect("known");

        let by_expiration = later.iter().map(|(_, e)| *e).max().expect("non-empty");
        assert_eq!(
            answer,
            Ts(by_expiration),
            "with the correction pointing the other way the two keys coincide"
        );
        // And the same fixture with the real direction does not — the two-sided
        // pair that makes this a diagnosis rather than an observation.
        let real = history_of("ZNZ2011", &RESTATED_EARLIER);
        let real_answer = real.as_of(&zn, Ts(1_000 * A_DAY)).expect("known");
        assert_ne!(
            real_answer,
            Ts(RESTATED_EARLIER
                .iter()
                .map(|(_, e)| *e)
                .max()
                .expect("non-empty"))
        );
    }

    // The collapsed representation (one entry per distinct expiry, with the
    // first and last `ts_recv` that stated it) exists so a 16-year definition
    // file does not cost hundreds of megabytes. It is only allowed to exist
    // because it answers `as_of` identically to the full record list — proved
    // here against the rule written out verbatim, at every interesting instant
    // including both boundaries.
    #[test]
    fn the_collapsed_history_answers_exactly_as_the_raw_records_would() {
        // Two statements, each repeated the way a daily-restated schema repeats
        // them, with the second overwriting the first.
        let mut raw: Vec<(i64, i64)> = Vec::new();
        for day in 100..107 {
            raw.push((day * A_DAY, 200 * A_DAY));
        }
        for day in 107..115 {
            raw.push((day * A_DAY, 200 * A_DAY - A_DAY));
        }
        let history = history_of("ZNZ2011", &raw);
        let zn = contract("ZNZ2011");
        assert_eq!(
            history.revisions(&zn).len(),
            2,
            "fifteen records, two statements"
        );
        assert_eq!(history.revisions(&zn)[0].records, 7);
        assert_eq!(history.revisions(&zn)[1].records, 8);

        for day in 95..120 {
            for offset in [-1, 0, 1] {
                let at = day * A_DAY + offset;
                assert_eq!(
                    history.as_of(&zn, Ts(at)).map(|t| t.0),
                    brute_force_as_of(&raw, at),
                    "collapsed and raw disagree at {at}"
                );
            }
        }
    }

    // The residual refusal, and item (4) of the ruling: `ZNM2012` sat unreported
    // behind `ZNZ2011` for as long as the reader returned on the first hit, and
    // a refusal that reports one of two makes the archive look better than it
    // is. Two conflicts in one root must both be named.
    #[test]
    fn a_refusal_names_every_conflicting_contract_and_not_the_first() {
        let err = ExpiryHistory::from_observations([
            // Same instant, two expiries: unorderable.
            (contract("ZNZ2011"), Ts(100 * A_DAY), Ts(200 * A_DAY)),
            (contract("ZNZ2011"), Ts(100 * A_DAY), Ts(201 * A_DAY)),
            // A second contract with the same problem, later in symbol order.
            (contract("ZNM2012"), Ts(300 * A_DAY), Ts(400 * A_DAY)),
            (contract("ZNM2012"), Ts(300 * A_DAY), Ts(401 * A_DAY)),
            // And one that is perfectly fine, so the refusal is about the
            // conflicts rather than about the file.
            (contract("ZNH2012"), Ts(200 * A_DAY), Ts(250 * A_DAY)),
        ])
        .expect_err("two contracts cannot be ordered");
        match &err {
            ContinuousError::ExpiryConflict { conflicts } => {
                let names: Vec<&str> = conflicts.iter().map(|c| c.contract.as_str()).collect();
                // Delivery order, and BOTH of them: December 2011 then June
                // 2012. The clean March 2012 contract is not accused.
                assert_eq!(names, vec!["ZNZ2011", "ZNM2012"], "{err}");
            }
            other => panic!("wrong refusal: {other}"),
        }
        // The message a human reads must name both too, not just the payload.
        let text = err.to_string();
        assert!(
            text.contains("ZNZ2011") && text.contains("ZNM2012"),
            "{text}"
        );
        assert!(!text.contains("ZNH2012"), "{text}");
    }

    // Overlapping availability windows are the case that cannot be ordered even
    // though the instants differ: the source says A from day 100 to 110 and B
    // from day 105 to 115, so at day 107 it asserts both and there is no latest.
    // Disjoint windows in the same shape resolve, which is the control for this
    // check not being "any two expiries refuse".
    #[test]
    fn overlapping_statements_refuse_while_disjoint_ones_resolve() {
        let overlapping = ExpiryHistory::from_observations([
            (contract("ZNZ2011"), Ts(100 * A_DAY), Ts(200 * A_DAY)),
            (contract("ZNZ2011"), Ts(110 * A_DAY), Ts(200 * A_DAY)),
            (contract("ZNZ2011"), Ts(105 * A_DAY), Ts(199 * A_DAY)),
            (contract("ZNZ2011"), Ts(115 * A_DAY), Ts(199 * A_DAY)),
        ])
        .expect_err("windows overlap, so neither statement is the latest");
        assert!(
            matches!(overlapping, ContinuousError::ExpiryConflict { .. }),
            "{overlapping}"
        );

        let disjoint = ExpiryHistory::from_observations([
            (contract("ZNZ2011"), Ts(100 * A_DAY), Ts(200 * A_DAY)),
            (contract("ZNZ2011"), Ts(104 * A_DAY), Ts(200 * A_DAY)),
            (contract("ZNZ2011"), Ts(105 * A_DAY), Ts(199 * A_DAY)),
            (contract("ZNZ2011"), Ts(115 * A_DAY), Ts(199 * A_DAY)),
        ])
        .expect("one window ends before the other begins");
        assert_eq!(disjoint.revisions(&contract("ZNZ2011")).len(), 2);
    }

    // Order of arrival must not reach the answer (§2.2): a definition file is
    // decoded in file order, and nothing may depend on it.
    #[test]
    fn the_history_does_not_depend_on_the_order_records_arrive_in() {
        let forwards = history_of("ZNZ2011", &RESTATED_EARLIER);
        let mut reversed = RESTATED_EARLIER;
        reversed.reverse();
        let backwards = history_of("ZNZ2011", &reversed);
        assert_eq!(forwards, backwards);
    }

    // `restated()` is what the CLI prints, so the 4-in-1,002 are visible rather
    // than resolved in silence. A contract stated once must not appear in it.
    #[test]
    fn only_restated_contracts_are_reported_as_restated() {
        let history = ExpiryHistory::from_observations([
            (contract("ZNZ2011"), Ts(100 * A_DAY), Ts(200 * A_DAY)),
            (contract("ZNZ2011"), Ts(107 * A_DAY), Ts(199 * A_DAY)),
            (contract("ZNH2012"), Ts(200 * A_DAY), Ts(250 * A_DAY)),
        ])
        .expect("orderable");
        let names: Vec<String> = history.restated().map(|(s, _)| s.to_string()).collect();
        assert_eq!(names, vec!["ZNZ2011"]);
        assert_eq!(history.len(), 2, "both contracts are still known");
    }

    // The cycle reader's input comes off the same history and must keep the
    // SPAN, not the current statement: a bar printed in the disputed hour still
    // has to resolve to a partition (D-0072).
    #[test]
    fn cycles_built_from_a_history_span_every_statement() {
        let history = history_of("GCX2021", &RESTATED_EARLIER);
        let cycles = ContractCycles::from_expiries(&history).expect("same year");
        assert_eq!(cycles.len(), 1);
        // The later (stale) expiry is the span's end, so a bar between the two
        // still names GCX2021 rather than falling outside every cycle.
        let (_, _, closes) = cycles
            .resolve_window("GCX1", Ts(199 * A_DAY))
            .expect("resolves");
        assert_eq!(closes, Ts(RESTATED_EARLIER[0].1));
    }

    // ------------------------------------------- the cycle resolver (D-0072)

    use super::{CONTRACT_CYCLE_DAYS, ContractCycles};
    use crate::continuous::symbol::MonthCode;
    use crate::ingest::window::NANOS_PER_DAY;

    /// The two December gold contracts a sixteen-year window contains, with
    /// their real expiry days as epoch days (from the archive's own
    /// `definition` file): 2014-12-29 is day 16,433 and 2024-12-27 is 20,084.
    fn gold_z_cycles() -> ContractCycles {
        ContractCycles::from_spans([
            (
                ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid"),
                Ts(16_433 * NANOS_PER_DAY),
                Ts(16_433 * NANOS_PER_DAY),
            ),
            (
                ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid"),
                Ts(20_084 * NANOS_PER_DAY),
                Ts(20_084 * NANOS_PER_DAY),
            ),
        ])
        .expect("no year conflict")
    }

    /// A day, as this archive's timestamps count them.
    fn day(epoch_day: i64) -> Ts {
        Ts(epoch_day * NANOS_PER_DAY)
    }

    // THE fix, in one test. `GCZ4` is two contracts; which one a bar belongs to
    // is decided by when the bar printed, against the contract's own expiry.
    // 2014-01-02 is epoch day 16,072; 2024-01-02 is 19,724.
    #[test]
    fn one_spelling_two_contracts_resolved_by_the_bars_own_timestamp() {
        let cycles = gold_z_cycles();
        assert_eq!(
            cycles.resolve("GCZ4", day(16_072)).expect("resolves"),
            ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid")
        );
        assert_eq!(
            cycles.resolve("GCZ4", day(19_724)).expect("resolves"),
            ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid")
        );
    }

    // The boundary is the earlier contract's expiry, exclusive on the left and
    // inclusive on the right — the same half-open convention the roll instant
    // uses (D-0041), so the codebase has one rule and not two.
    #[test]
    fn the_cycle_boundary_is_the_previous_expiry_exclusive() {
        let cycles = gold_z_cycles();
        let expiry_2014 = 16_433;
        assert_eq!(
            cycles
                .resolve("GCZ4", day(expiry_2014))
                .expect("resolves")
                .year(),
            2014,
            "a bar AT the 2014 expiry is still the 2014 contract"
        );
        assert_eq!(
            cycles
                .resolve("GCZ4", day(expiry_2014 + 1))
                .expect("resolves")
                .year(),
            2024,
            "one day later it is the 2024 contract"
        );
    }

    // The refusal (2) of D-0072. The 2014 contract is the family's earliest, so
    // its window opens one cycle before its own expiry; a bar before that
    // belongs to a contract the definition file does not name, and filing it
    // anywhere would be a guess.
    #[test]
    fn a_bar_outside_every_cycle_is_refused_rather_than_filed() {
        let cycles = gold_z_cycles();
        let opens = 16_433 - CONTRACT_CYCLE_DAYS;
        assert!(cycles.resolve("GCZ4", day(opens)).is_err());
        assert!(cycles.resolve("GCZ4", day(opens + 1)).is_ok());
        // And past the last known expiry, which is the 2034 contract's turf.
        let err = cycles
            .resolve("GCZ4", day(20_085))
            .expect_err("no cycle owns this");
        assert!(
            matches!(err, ContinuousError::UnresolvedContract { .. }),
            "{err}"
        );
        // The message names the candidates rather than merely saying "no".
        let text = err.to_string();
        assert!(
            text.contains("GCZ2014") && text.contains("GCZ2024"),
            "{text}"
        );
    }

    // The `GCZ4`-shaped case with NO expiry available: the refusal must fire,
    // and it must not fall back to a `DecadeAnchor` constant. Under the
    // constant this would have quietly answered `GCZ2024` for every bar,
    // including the 2014 ones — which is exactly the bug (D-0072).
    #[test]
    fn a_one_digit_year_with_no_definition_at_all_is_refused() {
        let none = ContractCycles::default();
        assert!(none.is_empty());
        for epoch_day in [16_072, 19_724] {
            let err = none
                .resolve("GCZ4", day(epoch_day))
                .expect_err("nothing may resolve without an expiry");
            match &err {
                ContinuousError::UnresolvedContract { candidates, .. } => {
                    assert!(candidates.is_empty(), "{err}");
                }
                other => panic!("wrong refusal: {other}"),
            }
            assert!(
                err.to_string().contains("crucible pull"),
                "the refusal must say how to fix it: {err}"
            );
        }
        // The anchor would have answered — that is what makes it the wrong
        // device here, and the assertion that says so.
        assert_eq!(
            ContractSymbol::parse("GCZ4").expect("valid").year(),
            2024,
            "the constant always has an answer, and it is right half the time"
        );
    }

    // THE THIRD SIDE (CLAUDE.md §7). The two sides above show an aliased key
    // and a resolved key differ. This shows they AGREE the moment the resolver
    // is handed the expiry that separates them — so the difference is the year
    // resolution and nothing else: same root, same month, same everything but
    // the digit that could not say which decade.
    #[test]
    fn the_resolved_and_anchored_keys_agree_once_the_expiry_says_which_decade() {
        let cycles = gold_z_cycles();
        let anchored = ContractSymbol::parse("GCZ4").expect("valid");
        let resolved_2024 = cycles.resolve("GCZ4", day(19_724)).expect("resolves");
        let resolved_2014 = cycles.resolve("GCZ4", day(16_072)).expect("resolves");

        // Side one: on the 2014 bar they disagree, which is the defect.
        assert_ne!(anchored, resolved_2014);
        // Side two: on the 2024 bar they agree, so the anchor is not simply
        // broken — it is right for one of the two cycles.
        assert_eq!(anchored, resolved_2024);
        // The third side: the disagreement is *only* the year. Everything the
        // resolver did not have to guess is identical.
        assert_eq!(anchored.root(), resolved_2014.root());
        assert_eq!(anchored.month(), resolved_2014.month());
        assert_eq!(
            anchored.year() - resolved_2014.year(),
            10,
            "one decade — the period of a one-digit year code"
        );
        assert_eq!(
            ContractSymbol::parse_with_anchor("GCZ4", crate::continuous::DecadeAnchor::new(2015))
                .expect("valid"),
            resolved_2014,
            "give the anchor the decade the expiry knows and the two coincide"
        );
    }

    // Two- and four-digit years are absolute, so they resolve with no archive
    // at all. Demanding an expiry for them would refuse the far-dated listings
    // the vendor spells `CLZ36` precisely to avoid the ambiguity — 16 of them
    // trade in the archive's CL window.
    #[test]
    fn an_unambiguous_spelling_needs_no_expiry() {
        let none = ContractCycles::default();
        assert_eq!(
            none.resolve("CLZ36", day(19_724)).expect("absolute").year(),
            2036
        );
        assert_eq!(
            none.resolve("GCZ2014", day(0)).expect("absolute").year(),
            2014
        );
    }

    // A resolver for one root must not answer for another, even when the month
    // and digit line up. `NQZ4` is not a gold contract.
    #[test]
    fn a_resolver_does_not_answer_for_a_foreign_root() {
        let cycles = gold_z_cycles();
        let err = cycles
            .resolve("NQZ4", day(19_724))
            .expect_err("wrong root has no candidates here");
        match err {
            ContinuousError::UnresolvedContract { candidates, .. } => {
                assert!(candidates.is_empty());
            }
            other => panic!("wrong refusal: {other}"),
        }
    }

    // A family's members are one decade apart because that is the period of the
    // code that cannot distinguish them. Two that are not mean the definition
    // file dated one of them into the wrong decade, and the windows would then
    // be slivers a bar lands in by rounding.
    #[test]
    fn two_cycles_less_than_a_decade_apart_refuse_construction() {
        let err = ContractCycles::from_spans([
            (
                ContractSymbol::new("GC", 2014, MonthCode::Z).expect("valid"),
                day(16_433),
                day(16_433),
            ),
            (
                ContractSymbol::new("GC", 2024, MonthCode::Z).expect("valid"),
                day(16_433 + 400),
                day(16_433 + 400),
            ),
        ])
        .expect_err("400 days is not a decade");
        assert!(
            matches!(err, ContinuousError::ContractCycleCollision { .. }),
            "{err}"
        );
        // And the real spacing is accepted, so the check is not simply always on.
        assert!(gold_z_cycles().len() == 2);
    }

    #[test]
    fn a_spread_is_not_a_contract_the_resolver_will_name() {
        let cycles = gold_z_cycles();
        for bad in ["GCZ4-GCZ5", "CL:BF F0-G0-H0", "SYN:RW", "ES.v.0"] {
            assert!(
                matches!(
                    cycles.resolve(bad, day(19_724)),
                    Err(ContinuousError::UnparseableSymbol { .. })
                ),
                "{bad}"
            );
        }
    }
}

#[cfg(all(test, feature = "databento"))]
mod decade_tests {
    use super::ContractSymbol;
    use super::imp::expiries_from_definitions;

    use crate::continuous::symbol::{DecadeAnchor, MonthCode};
    use crate::testutil::TempDir;
    use databento::dbn::InstrumentClass;

    // The exact values the real 16-year ES definition archive produced, which
    // is how this defect was found: the June 2010 and June 2020 contracts are
    // BOTH called `ESM0`. 1_276_867_800 s is 2010-06-18T13:30:00Z and
    // 1_592_573_400 s is 2020-06-19T13:30:00Z — both 08:30 Central.
    const JUN_2010: u64 = 1_276_867_800_000_000_000;
    const JUN_2020: u64 = 1_592_573_400_000_000_000;

    // A single `DecadeAnchor` cannot separate them, so before this each file
    // spanning both decades collapsed both into one contract with two expiries
    // a decade apart. Each record's own expiry can separate them.
    #[test]
    fn a_one_digit_year_resolves_against_each_records_own_expiry() {
        let dir = TempDir::new();
        let path = dir.path().join("defs.dbn.zst");
        super::imp::tests::write_definitions(
            &path,
            &[
                ("ESM0", InstrumentClass::Future, JUN_2010),
                ("ESM0", InstrumentClass::Future, JUN_2020),
            ],
        );

        let history = expiries_from_definitions(&path, "ES", DecadeAnchor::DEFAULT)
            .expect("each record dates itself, so there is no conflict");
        assert_eq!(history.len(), 2, "two contracts, not one collision");

        let years: Vec<i32> = history.contracts().map(ContractSymbol::year).collect();
        assert_eq!(years, vec![2010, 2020]);
        for symbol in history.contracts() {
            assert_eq!(symbol.month(), MonthCode::M);
            assert_eq!(symbol.root(), "ES");
            assert_eq!(
                history.revisions(symbol).len(),
                1,
                "one statement each: two contracts, not one restated"
            );
        }
        assert_eq!(
            history
                .contracts()
                .filter_map(|s| history.latest(s))
                .map(|t| t.0)
                .collect::<Vec<_>>(),
            vec![JUN_2010 as i64, JUN_2020 as i64]
        );
    }
}
