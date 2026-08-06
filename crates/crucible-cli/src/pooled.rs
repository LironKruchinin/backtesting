//! Assembling a pooled run's per-contract inputs (block C, C4a).
//!
//! The seam between three things that must not each form their own opinion:
//! the **roll table**, which says when a contract was front (C3a); the
//! **collector**, which reads that window's bars (C2); and the **fold plan**,
//! which is this codebase's sole authority on which sessions are out of sample
//! (D-0062, D-0071).
//!
//! # Everything here is measured per contract
//!
//! Front windows are not equal — ES 2024 measured 64, 70, 66 and 66 sessions
//! over spans of 84 to 98 calendar days — so nothing in a pooled run may be
//! derived as `N × a per-contract constant`. That extrapolation is exactly
//! what produced the false claim that four ES contracts clear a 250-session
//! floor (D-0119).
//!
//! # Why the fold plan is per contract
//!
//! Folds are cut **inside** each contract's front window, never across the
//! pooled concatenation. Cutting across it would put a roll inside a window,
//! and a position carried across a roll either books the raw gap as PnL —
//! which §9 refuses to let go silent — or is flattened at every boundary,
//! which is a new named execution assumption. Inside a front window no roll
//! exists, so the question does not arise (D-0119).
//!
//! # Nothing here is reachable yet
//!
//! D-0117 refuses every `[pooling]` config at validation time, and that
//! refusal lifts only when the orchestration's planted-bug controls are green.
//! This module is built behind it, like the arithmetic (D-0114) and the
//! declaration surface (D-0115) before it.

#![expect(
    dead_code,
    reason = "C4a builds the pooled planner behind D-0117's refusal; C4b wires               the replay and registry claims to it, and C6 lifts the refusal.               Landing it unreachable is the inert-first ordering D-0114 and               D-0115 already used — the alternative is a half-wired pooling               path, which is the one shape that must not exist on main."
)]

use crucible_core::prelude::*;
use crucible_data::calendar::Calendar;
use crucible_data::catalog::TsRange;
use crucible_data::continuous::{ContinuousError, RollTable, read_roll_table};
use crucible_data::ingest::window::{civil_from_days, days_from_civil};
use crucible_engine::{BacktestParams, FreeFills, run};
use crucible_funnel::pooling::{ContractEvaluation, pooled_registration_hash};
use crucible_funnel::walkforward::{FoldPlan, FoldSpec, RunTrace};

use crate::combo::{PooledWindow, Series, SliceFeed, annualization, collect_events_in_window};
use crate::config::LoadedConfig;
use crate::pull::{EXIT_USAGE, data_dir};

/// One pooled contract, planned but not yet replayed.
///
/// `plan` is `None` when the front window could not fit one complete fold at
/// the config's declared geometry. That contract contributes nothing and is
/// reported as skipped with a count — never dropped silently (D-0070's
/// spread-filter pattern, D-0119).
pub(crate) struct PooledContractPlan {
    /// Curated contract symbol.
    pub instrument: String,
    /// Bars of this contract's front window.
    pub series: Series,
    /// One trading-day key per bar, computed **once here** and read by every
    /// consumer — the D-0071 device. Two independent attributions of "which
    /// day" is how one breach lands on two dates in two reports.
    pub day_keys: Vec<i64>,
    /// Distinct trading-day keys of the front window, ascending.
    pub front_window_days: Vec<i64>,
    /// The front window this contract was evaluated on, as INSTANTS. Part of
    /// its run identity (D-0124): a rebuilt roll table can move a front
    /// window, which makes a different run of the same strategy on the same
    /// symbol, and two results that are not comparable must not share a trial.
    pub front_window: TsRange,
    /// Warmup bars this contract's OWN curated history supplied before its
    /// front window opened, and what the grid needed — `Some` only when it
    /// fell short, in which case nothing else here is meaningful and the
    /// contract is skipped with a count (D-0121).
    pub insufficient_warmup: Option<(usize, usize)>,
    /// The trailing trading day the **window** cut, dropped from evaluation —
    /// or `None` when the window ended on a session close, which is what a
    /// roll boundary does.
    pub dropped_tail_day: Option<i64>,
    /// Bars in that dropped day. Reported even when zero (D-0070's
    /// declared-filter-with-a-number pattern): "nothing was cut" and "the
    /// report forgot to say" must not look the same.
    pub dropped_tail_bars: usize,
    /// The fold layout, or `None` when no complete fold fits.
    pub plan: Option<FoldPlan>,
    /// Sessions one fold needed, carried so a skip can say what it missed by.
    pub fold_needs_sessions: usize,
}

impl PooledContractPlan {
    /// Trading-day keys that fall in an out-of-sample window, ascending.
    ///
    /// Read off the fold plan rather than recomputed: `FoldPlan` is the sole
    /// boundary authority, and a second derivation of "which days are out of
    /// sample" is the class of bug D-0071 exists to refuse.
    pub(crate) fn oos_day_keys(&self) -> Vec<i64> {
        let Some(plan) = self.plan.as_ref() else {
            return Vec::new();
        };
        let mut keys: Vec<i64> = plan
            .folds()
            .iter()
            .flat_map(|fold| plan.days()[fold.test.days.clone()].iter().copied())
            .collect();
        // Test windows never overlap between folds (D-0062 refuses
        // `step_days < test_days`), so this is already distinct; sorting and
        // deduplicating is defence, not correction, and costs nothing at fold
        // scale.
        keys.sort_unstable();
        keys.dedup();
        keys
    }
}

/// Loads the `.v` roll table for `root` at the config's grain.
///
/// The **volume** rule specifically, because it answers "when was this
/// contract where the trading actually was" by measurement rather than by
/// calendar guess — which is the whole basis of the front-month ruling
/// (D-0119). Every pooled report names the rule that produced the attribution.
///
/// # Errors
/// A message and exit code when the table cannot be read or is malformed.
pub(crate) fn load_volume_roll_table(
    loaded: &LoadedConfig,
    root: &str,
) -> Result<RollTable, (i32, String)> {
    let dir = data_dir().map_err(|msg| (EXIT_USAGE, msg))?;
    let path = dir
        .join("curated")
        .join("rolls")
        .join(root)
        .join(loaded.timeframe.to_string())
        .join("v-confirm1.json");
    let table = read_roll_table(&path).map_err(|e: ContinuousError| {
        (
            EXIT_USAGE,
            format!(
                "cannot read the {root} volume roll table at {}: {e}\n       \
                 Build it with `crucible rolls --root {root} --timeframe {} --write`. A pooled \
                 run needs it to know which sessions each contract was front for (D-0119).",
                path.display(),
                loaded.timeframe
            ),
        )
    })?;
    // The table decides every window below, so it is validated before any of
    // them is derived rather than after something looks wrong.
    table.validate().map_err(|e| {
        (
            EXIT_USAGE,
            format!("the {root} volume roll table is malformed: {e}"),
        )
    })?;
    Ok(table)
}

/// Plans every contract of a pool: front window, bars, day keys, folds.
///
/// # Errors
/// A message and exit code naming the contract that failed. A pooled run
/// refuses rather than silently pooling the contracts that happened to load:
/// a sample quietly missing a contract reports a smaller pool than it declared
/// (D-0119).
pub(crate) fn plan_pool(
    loaded: &LoadedConfig,
    table: &RollTable,
    contracts: &[String],
    fold_spec: FoldSpec,
) -> Result<Vec<PooledContractPlan>, (i32, String)> {
    let calendar = Calendar::for_instrument(&loaded.spec.instrument)
        .map_err(|e| {
            (
                EXIT_USAGE,
                format!("bundled calendar tables are broken: {e}"),
            )
        })?
        .ok_or_else(|| {
            (
                EXIT_USAGE,
                format!(
                    "no bundled calendar claims {}, so its trading days cannot be resolved and \
                     a pooled session count would be a guess",
                    loaded.spec.instrument
                ),
            )
        })?;
    let needs = fold_spec.train_days + fold_spec.test_days;

    // One calendar governs the whole pool, and it is asserted rather than
    // assumed.
    //
    // The calendar above is resolved from `loaded.spec.instrument`, which
    // `config.rs` builds from `instruments[0]` — so a pool spanning two
    // calendars would silently take its trading days, its session clock and
    // its `bars_per_year` from whichever contract was typed first. Every
    // pooled session count reads that calendar, so the error would be in the
    // denominator of the admission floor.
    //
    // Two things have to hold, and only one of them is now structural. The
    // config's root check guarantees a single *root* (C4b-ii) — but "one root
    // resolves to one calendar" is a property of the bundled TABLES, not of
    // that check: `Calendar::for_instrument` takes the FIRST table whose
    // `roots` list claims the symbol, so two tables claiming one root would
    // resolve by table order. `no_two_bundled_calendars_claim_the_same_root`
    // in `crucible-data` is the tripwire for that half; this loop is the
    // tripwire for a pool that reaches here spanning two of them anyway.
    for instrument in contracts {
        let id = InstrumentId::new(instrument);
        let governing = Calendar::for_instrument(&id).map_err(|e| {
            (
                EXIT_USAGE,
                format!("bundled calendar tables are broken: {e}"),
            )
        })?;
        match governing {
            Some(other) if other.id() == calendar.id() => {}
            Some(other) => {
                return Err((
                    EXIT_USAGE,
                    format!(
                        "{instrument} is governed by calendar {other}, but the pool's calendar \
                         is {ours} (taken from {first}). A pooled run counts sessions on one \
                         calendar; two would make the admission floor's denominator depend on \
                         which contract was declared first",
                        other = other.id(),
                        ours = calendar.id(),
                        first = loaded.spec.instrument,
                    ),
                ));
            }
            None => {
                return Err((
                    EXIT_USAGE,
                    format!(
                        "no bundled calendar claims {instrument}, so its trading days cannot be \
                         resolved and its contribution to the pooled session count would be a \
                         guess"
                    ),
                ));
            }
        }
    }

    let mut planned = Vec::with_capacity(contracts.len());
    for instrument in contracts {
        let window = table
            .front_window(instrument)
            .map_err(|e| (EXIT_USAGE, format!("{instrument}: {e}")))?;
        // Carried through as INSTANTS. C3a's window is
        // `[roll_ts + 1ns, next_roll_ts + 1ns)` and a `roll_ts` is an instant
        // inside a session, so a civil-date round trip here moved both ends to
        // UTC midnight — the middle of a CME trading day, which opens 17:00 CT
        // the evening before. The replay then began and ended mid-session and
        // the roll date went wholesale to the incoming contract: exactly the
        // silent off-by-one `front_window`'s own doc is about, reintroduced one
        // layer above the function that was careful about it.
        //
        // Read over a LEAD-IN and judged over the front window (D-0121). The
        // lead-in reaches back into this contract's own curated history so the
        // grid's warmup is spent BEFORE the evaluation window opens, which is
        // what makes the pooled session count a property of the pool rather
        // than of the grid: without it, warmup comes out of the front window
        // and evaluable sessions fall as indicator periods rise, so "does this
        // pool clear 250 sessions?" would depend on the Bollinger length.
        let pooled = PooledWindow {
            lead_in: TsRange::new(Ts(0), window.end_ts())
                .map_err(|e| (EXIT_USAGE, format!("{instrument}: lead-in range: {e}")))?,
            evaluation: window,
        };
        let mut series = collect_events_in_window(loaded, instrument, Some(pooled))?;

        // Exactly `needed` bars of warmup, never more and never fewer, so
        // every contract in the pool enters its evaluation window having
        // consumed an identical number of bars (§2.6). Trimming the surplus is
        // what makes that literal rather than approximate.
        let needed = loaded.grid.max_warmup_bars();
        let trim = warmup_trim(&series.events, window.start_ts(), needed);
        let Ok(drop_head) = trim else {
            let (available, needed) = trim.expect_err("INVARIANT: the Ok arm is bound above");
            // Skipped, counted, and NOT evaluated on a shorter window. The
            // alternative D-0121 names and refuses is letting evaluation start
            // late: that reintroduces the grid-dependence above for exactly the
            // contracts nearest the chain head, and does it without a count.
            planned.push(PooledContractPlan {
                instrument: instrument.clone(),
                series,
                day_keys: Vec::new(),
                front_window_days: Vec::new(),
                front_window: window,
                insufficient_warmup: Some((available, needed)),
                dropped_tail_day: None,
                dropped_tail_bars: 0,
                plan: None,
                fold_needs_sessions: needs,
            });
            continue;
        };
        series.events.drain(..drop_head);

        // The D-0071 device: computed once, here, and read by every consumer.
        let mut day_keys: Vec<i64> = series
            .events
            .iter()
            .map(|event| days_from_civil(calendar.trading_day(event.avail_ts())))
            .collect();
        // The front window's days are the EVALUATION portion only — the first
        // `needed` bars are warmup and are not sessions this contract offers.
        let mut front_window_days = day_keys[needed..].to_vec();
        front_window_days.dedup();

        // A trading day the WINDOW cut is dropped and counted — the symmetric
        // twin of the leading partial session `FoldPlan` already drops so that
        // its "60 sessions" is sixty sessions (D-0062).
        //
        // Measured on all 66 curated ES contracts before this was written: 65
        // lose nothing here, because a `roll_ts` IS a session close. A roll is
        // decided from a day's volume, which is not knowable until that day's
        // last bar, so the window ends at close + 1ns and the final day is
        // whole — checked arithmetically on ESH2020 -> ESM2020, whose roll_ts
        // is 2020-03-16T21:00:00Z, exactly era 3a's 16:00 CT close.
        //
        // The one contract that does lose a day is the chain's LAST, whose
        // window ends at the archive edge rather than at a roll: ESU2026's
        // final trading day held 120 of 1380 minutes, because the archive stops
        // at 2026-07-28T00:00:00Z — 19:00 CT, two hours into a session that
        // opened at 17:00.
        //
        // That is not a rare tail case, it is the common one for the pools
        // anyone builds first: the last contract is always the most recent, so
        // a pool testing recent data contains it by construction, and the error
        // counts a fragment as a whole session toward the admission floor —
        // in the flattering direction. Dropping it here makes the property
        // something the code enforces rather than something re-measured every
        // time data lands.
        let (dropped_tail_day, dropped_tail_bars) =
            match trailing_fragment(&calendar, window, &day_keys) {
                Some((day, bars)) => {
                    // `day_keys` is availability-ordered, so the cut day's bars
                    // are exactly the tail.
                    day_keys.truncate(day_keys.len() - bars);
                    series.events.truncate(series.events.len() - bars);
                    front_window_days.pop();
                    (Some(day), bars)
                }
                None => (None, 0),
            };

        // A front window that cannot fit one complete fold is planned as
        // `None` rather than refused: the pool reports it as skipped with a
        // count, and a deliberately short contract is a legitimate member of a
        // pool that simply contributes nothing (D-0119).
        // `needed` bars of warmup sit at the head of `day_keys`, so the fold
        // plan's evaluation window opens exactly at `front_start` — never late.
        let plan = FoldPlan::build(&day_keys, needed, fold_spec).ok();
        planned.push(PooledContractPlan {
            instrument: instrument.clone(),
            series,
            day_keys,
            front_window_days,
            front_window: window,
            insufficient_warmup: None,
            dropped_tail_day,
            dropped_tail_bars,
            plan,
            fold_needs_sessions: needs,
        });
    }
    Ok(planned)
}

/// Whether `window` cut its final trading day, and how many bars that day holds.
///
/// A front window is half-open, so the final session is **whole** exactly when
/// the window extends past that session's close. A roll boundary always does:
/// `roll_ts` is the availability of the deciding day's last bar — a session
/// close — and the window ends at `roll_ts + 1ns`. What does not is the chain's
/// last contract, whose window ends where the archive does.
///
/// Returns `(trading_day_key, bars)` for a cut day, or `None` when nothing was
/// cut. Separated from [`plan_pool`] so it can be tested without an archive:
/// the planner needs a `LoadedConfig` and curated Parquet, and a check that
/// only runs against the archive is a check nobody runs.
fn trailing_fragment(
    calendar: &Calendar,
    window: TsRange,
    day_keys: &[i64],
) -> Option<(i64, usize)> {
    let &last = day_keys.last()?;
    let close = calendar.session_close(civil_from_days(last));
    // Half-open: a bar whose `avail_ts` IS the close belongs to the session, so
    // the session survives only if the window reaches strictly past it.
    if window.end_ts() > close {
        return None;
    }
    let bars = day_keys.iter().rev().take_while(|&&k| k == last).count();
    Some((last, bars))
}

#[cfg(test)]
mod trailing_fragment_tests {
    use super::*;
    use crucible_data::ingest::window::CivilDate;

    fn es() -> Calendar {
        Calendar::for_instrument(&InstrumentId::new("ESH2024"))
            .expect("bundled tables parse")
            .expect("ES is governed")
    }

    const FRIDAY: CivilDate = CivilDate {
        year: 2024,
        month: 3,
        day: 8,
    };

    /// **The converse, and it is written first.** Every roll boundary ends one
    /// nanosecond past a session close, and 65 of this archive's 66 ES
    /// contracts end that way — so an implementation that dropped a day
    /// unconditionally would pass the cut-window test below while silently
    /// shortening every contract in the pool by a session. That is the same
    /// failure shape the D4 converse caught: a fix applied everywhere looks
    /// identical to a fix applied where it belongs.
    #[test]
    fn a_window_ending_past_a_session_close_drops_nothing() {
        let cal = es();
        let key = days_from_civil(FRIDAY);
        let close = cal.session_close(FRIDAY);
        let window = TsRange::new(cal.session_open(FRIDAY), close.plus_ns(1)).expect("range");
        assert_eq!(
            trailing_fragment(&cal, window, &[key - 1, key, key, key]),
            None,
            "a roll boundary is a session close; nothing was cut"
        );
    }

    /// A window stopping mid-session drops that day, with its bar count.
    #[test]
    fn a_window_cut_mid_session_drops_the_fragment_with_a_count() {
        let cal = es();
        let key = days_from_civil(FRIDAY);
        let close = cal.session_close(FRIDAY);
        let window = TsRange::new(
            cal.session_open(FRIDAY),
            close.plus_ns(-3_600 * 1_000_000_000),
        )
        .expect("range");
        assert_eq!(
            trailing_fragment(&cal, window, &[key - 1, key, key, key]),
            Some((key, 3)),
            "three bars of the cut day, counted rather than absorbed"
        );
    }

    /// The measured case this exists for, with the archive's real numbers.
    ///
    /// `ESU2026` is the chain's last contract; its front window ends at
    /// `last_ts_open + tf` = 2026-07-28T00:00:00Z, which is 19:00 CT — two
    /// hours into a trading day that opened at 17:00. Measured against the
    /// archive: 120 of 1380 minutes. This is the common case rather than a
    /// rare one, because the chain's last contract is always the most recent,
    /// so any pool testing recent data contains it by construction.
    #[test]
    fn the_archive_edge_is_a_cut_day_and_not_a_roll() {
        let cal = es();
        let day = CivilDate {
            year: 2026,
            month: 7,
            day: 28,
        };
        let key = days_from_civil(day);
        let archive_edge = Ts(1_785_196_800_000_000_000);
        assert!(
            archive_edge < cal.session_close(day),
            "positive control on the fixture: the archive edge must really fall \
             before this session's close, or the test proves nothing"
        );
        let window = TsRange::new(Ts(1_781_557_200_000_000_001), archive_edge).expect("range");
        assert_eq!(
            trailing_fragment(&cal, window, &[key - 3, key - 1, key]),
            Some((key, 1))
        );
    }
}

/// Leading bars to drop so that **exactly** `needed` warmup bars precede
/// `front_start`.
///
/// `Ok(drop)` when this contract's own history can supply the warmup;
/// `Err((available, needed))` when it cannot, which is a skip and never a
/// shortened evaluation window (D-0121).
///
/// Trimming the surplus rather than keeping it is what makes §2.6 literal
/// across a pool: every contract enters its evaluation window having consumed
/// an identical number of bars, so no contract's indicators are warmer than
/// another's. And because the warmup sits at the head, `FoldPlan::build`'s
/// evaluation window opens exactly at `front_start` — never late, which is the
/// third option D-0121 names and refuses.
///
/// Separated from [`plan_pool`] so it is testable without an archive, for the
/// same reason [`crate::combo::eval_range`] was: the planner needs a
/// `LoadedConfig` and curated Parquet, and a check that runs only against the
/// archive is a check nobody runs.
fn warmup_trim(
    events: &[MarketEvent],
    front_start: Ts,
    needed: usize,
) -> Result<usize, (usize, usize)> {
    let available = events.partition_point(|event| event.avail_ts() < front_start);
    if available < needed {
        return Err((available, needed));
    }
    Ok(available - needed)
}

#[cfg(test)]
mod warmup_trim_tests {
    use super::*;
    use crucible_data::SyntheticFeed;

    /// 1,000 bars; `front_start` is bar 500's availability, so 500 bars of
    /// history precede the evaluation window.
    fn bars() -> Vec<MarketEvent> {
        let mut feed = SyntheticFeed::random_walk(
            7,
            1_000,
            TimeFrame::M1,
            Price::from_points_str("4500").expect("price"),
            Price::from_points_str("0.25").expect("tick"),
            4,
        );
        std::iter::from_fn(|| feed.next_event()).collect()
    }

    /// **The converse, written first.** A contract with ample lead-in must lose
    /// NOTHING from its evaluation window. Without this, a lead-in that
    /// silently shortened every contract — or one that dropped the whole
    /// history — passes the skip test below while being wrong everywhere, which
    /// is the failure shape the D4 and trailing-fragment converses both caught.
    #[test]
    fn ample_lead_in_costs_the_evaluation_window_nothing() {
        let events = bars();
        let front_start = events[500].avail_ts();
        let drop = warmup_trim(&events, front_start, 10).expect("ample");
        assert_eq!(drop, 490, "490 surplus, keeping exactly 10");

        let kept = &events[drop..];
        let still_before = kept.partition_point(|e| e.avail_ts() < front_start);
        assert_eq!(still_before, 10, "exactly the warmup the grid asked for");
        assert_eq!(
            kept.len() - still_before,
            events.len() - 500,
            "every evaluation bar survives — the trim touches the head only"
        );
    }

    /// Condition 2, asserted directly: evaluation opens exactly at
    /// `front_start`, which is what lets `FoldPlan` treat the head as warmup.
    #[test]
    fn evaluation_opens_exactly_at_front_start_not_at_warmup_end() {
        let events = bars();
        let front_start = events[500].avail_ts();
        let drop = warmup_trim(&events, front_start, 40).expect("ample");
        let kept = &events[drop..];
        assert_eq!(
            kept[40].avail_ts(),
            front_start,
            "the bar after the warmup IS the front window's first bar"
        );
    }

    /// A contract whose own history is too short is refused, with both numbers,
    /// rather than evaluated on a window shortened by the difference.
    #[test]
    fn a_short_history_is_a_skip_carrying_both_numbers() {
        let events = bars();
        let front_start = events[500].avail_ts();
        assert_eq!(warmup_trim(&events, front_start, 600), Err((500, 600)));
        // The chain head, measured: ESM2010 has zero bars before its front
        // window, because its curated data and its front window both begin
        // 2010-06-06.
        let none_before = events[0].avail_ts();
        assert_eq!(warmup_trim(&events, none_before, 40), Err((0, 40)));
    }

    /// The exact boundary: enough and not one bar more is enough, and drops
    /// nothing. An off-by-one here would skip a contract that qualifies.
    #[test]
    fn exactly_enough_history_qualifies_and_drops_nothing() {
        let events = bars();
        let front_start = events[500].avail_ts();
        assert_eq!(warmup_trim(&events, front_start, 500), Ok(0));
        assert_eq!(warmup_trim(&events, front_start, 501), Err((500, 501)));
    }
}

/// Replays one combo across every planned contract of a pool.
///
/// # What this does NOT do, and the omissions are the design
///
/// **It does not decide which trades are out of sample.** [`RunTrace::pooled`]
/// already attributes a round-trip to the window containing its **closing**
/// fill (D-0063: a round-trip opened in a training window and closed in a test
/// window is a test-window trade), and its bar index comes from `closed_ts`.
/// Counting them again here would be a second derivation of the same fact,
/// which is exactly the class of bug D-0071 exists to refuse — the one that
/// puts a breach on two different dates in two reports.
///
/// **It does not decide which sessions are out of sample.** Those come off
/// [`PooledContractPlan::oos_day_keys`], which reads the fold plan.
/// [`FoldPlan`] stays the sole boundary authority.
///
/// **It does not claim registry runs.** That seam is named below and left
/// empty for C4c.
///
/// # §2.6 across a pool
///
/// Every contract is replayed through `Grid::aligned_strategy`, so the
/// suppression window is the grid's max warmup for all of them, and
/// [`plan_pool`] has already trimmed each series so exactly that many bars
/// precede the evaluation window (D-0121). No contract's indicators are warmer
/// than another's, and no contract's evaluation opens later than another's
/// relative to its own front window.
///
/// Results are returned in `plans` order, which is the caller's declaration
/// order, so the merge is by run identity rather than by completion order
/// (§2.2). Nothing here is parallel; when it is, the same rule applies and
/// `crucible-funnel::scheduler` already collects by index rather than by
/// finish.
pub(crate) fn replay_pool(
    loaded: &LoadedConfig,
    plans: &[PooledContractPlan],
    combo_index: usize,
) -> Vec<PooledRun> {
    plans
        .iter()
        .map(|contract| {
            // A contract that could not be planned contributes no evidence and
            // is evaluated as zero — `pool_evaluations` turns it into a skip
            // with the reason, printed even when the count is zero (D-0070).
            let Some(fold_plan) = contract.plan.as_ref() else {
                return PooledRun {
                    registration_hash: pooled_registration_hash(
                        &loaded.config_hash.to_string(),
                        &contract.instrument,
                        contract.front_window.start_ts().0,
                        contract.front_window.end_ts().0,
                    ),
                    evaluation: ContractEvaluation {
                        instrument: contract.instrument.clone(),
                        front_window_days: contract.front_window_days.clone(),
                        oos_day_keys: Vec::new(),
                        oos_trades: 0,
                        insufficient_warmup: contract.insufficient_warmup,
                        folds: 0,
                        fold_needs_sessions: contract.fold_needs_sessions,
                    },
                };
            };

            // ---- RUN IDENTITY, BEFORE THE REPLAY (D-0074, D-0124) -----------
            //
            // Computed here, ahead of the replay, because D-0074's
            // insert-before-run means the identity a run will be recorded
            // under exists before the run does: a crash then leaves a
            // claimed-and-unfinished row rather than a run nobody knows
            // happened. The position is the contract — an identity derived
            // *after* the loop produced numbers would be insert-after-run
            // wearing the right name.
            //
            // Design B: a third composite in the effective `config_hash`
            // rather than a wider `RunKey`, because `RunKey` is persisted and
            // `TrialKey` derived, so a contract field would be a
            // persisted-shape change under §8's reader-first rule. N
            // contracts give N identities and therefore N trials (D-0114);
            // the same contract twice gives one.
            //
            // What remains for the orchestration (C5/C6) is to hand this to
            // `Registry::claim` — the identity is settled here so that call
            // has nothing left to decide.
            let registration_hash = pooled_registration_hash(
                &loaded.config_hash.to_string(),
                &contract.instrument,
                contract.front_window.start_ts().0,
                contract.front_window.end_ts().0,
            );
            // -----------------------------------------------------------------

            let events = &contract.series.events;
            let mut strategy = loaded.grid.aligned_strategy(combo_index);
            let mut feed = SliceFeed { events, at: 0 };
            let params = BacktestParams {
                initial_cash_nano_usd: loaded.initial_cash_nano_usd,
                bars_per_year: annualization(loaded, events),
            };
            let expect = "INVARIANT: a collected bar series is already availability-ordered";
            let result = if loaded.file.execution.fill_model == "free_fills" {
                run(
                    &mut feed,
                    &mut strategy,
                    &mut FreeFills,
                    &loaded.spec,
                    &params,
                )
                .expect(expect)
            } else {
                let mut fills = loaded.spread_cross_fills();
                run(&mut feed, &mut strategy, &mut fills, &loaded.spec, &params).expect(expect)
            };

            let trace = RunTrace::new(&result.equity, &result.closed_trades, &result.fee_events);
            let test_windows: Vec<std::ops::Range<usize>> = fold_plan
                .folds()
                .iter()
                .map(|fold| fold.test.bars.clone())
                .collect();
            let oos = trace.pooled(
                &test_windows,
                params.initial_cash_nano_usd,
                params.bars_per_year,
            );

            PooledRun {
                registration_hash,
                evaluation: ContractEvaluation {
                    instrument: contract.instrument.clone(),
                    front_window_days: contract.front_window_days.clone(),
                    oos_day_keys: contract.oos_day_keys(),
                    // Read off the pooled summary rather than recounted: the
                    // window that owns a round-trip is the one containing its
                    // closing fill, and that decision is made in exactly one place.
                    oos_trades: oos.round_trips,
                    insufficient_warmup: contract.insufficient_warmup,
                    folds: fold_plan.folds().len(),
                    fold_needs_sessions: contract.fold_needs_sessions,
                },
            }
        })
        .collect()
}

/// One pooled contract's run: what it is registered as, and what it measured.
///
/// The identity travels WITH the evidence rather than being recomputed by
/// whoever records it — a second derivation of "which run is this" is the same
/// class of bug D-0071 refuses for "which day is this".
pub(crate) struct PooledRun {
    /// Effective registration hash for this contract's run (D-0124).
    pub registration_hash: String,
    /// The evidence admission judges on.
    pub evaluation: ContractEvaluation,
}

#[cfg(test)]
pub(super) mod replay_pool_tests {
    use super::*;
    use crate::combo::collect_events_for;
    use crucible_funnel::walkforward::FoldScheme;

    /// A real grid over a SYNTHETIC source, so the replay is exercised end to
    /// end with no archive. `combo-smoke.toml` declares `source = "synthetic"`.
    pub(super) fn loaded() -> LoadedConfig {
        crate::config::load(std::path::Path::new("../../configs/combo-smoke.toml"))
            .expect("the shipped smoke config loads")
    }

    /// One planned contract over `events`, with `train`/`test` sessions and a
    /// day key every `per_day` bars.
    pub(super) fn plan_of(
        loaded: &LoadedConfig,
        name: &str,
        events: Vec<MarketEvent>,
        per_day: usize,
        spec: FoldSpec,
    ) -> PooledContractPlan {
        let warmup = loaded.grid.max_warmup_bars();
        let day_keys: Vec<i64> = (0..events.len()).map(|i| (i / per_day) as i64).collect();
        let mut front_window_days = day_keys[warmup..].to_vec();
        front_window_days.dedup();
        let plan = FoldPlan::build(&day_keys, warmup, spec).ok();
        PooledContractPlan {
            instrument: name.to_owned(),
            series: Series {
                events,
                data_manifest_ids: Vec::new(),
                description: format!("fixture {name}"),
                caveats: Vec::new(),
            },
            day_keys,
            front_window_days,
            front_window: TsRange::new(Ts(1), Ts(2)).expect("range"),
            insufficient_warmup: None,
            dropped_tail_day: None,
            dropped_tail_bars: 0,
            plan,
            fold_needs_sessions: spec.train_days + spec.test_days,
        }
    }

    fn spec() -> FoldSpec {
        FoldSpec {
            scheme: FoldScheme::Rolling,
            train_days: 5,
            test_days: 2,
            step_days: 2,
        }
    }

    /// **The converse, and it runs first.** Two contracts carrying the SAME
    /// bars must produce identical evidence.
    ///
    /// Without it, a replay that leaked state between contracts — a strategy
    /// reused instead of rebuilt, an equity curve continued rather than
    /// restarted — would still pass a test that only checked one contract, and
    /// would corrupt every pool of more than one. It is also §2.6 stated
    /// operationally: identical input, identical treatment.
    #[test]
    fn two_contracts_with_the_same_bars_produce_identical_evidence() {
        let loaded = loaded();
        let events = collect_events_for(&loaded, "SYN:RW")
            .expect("synthetic")
            .events;
        let plans = vec![
            plan_of(&loaded, "AAA", events.clone(), 40, spec()),
            plan_of(&loaded, "BBB", events, 40, spec()),
        ];
        let out = replay_pool(&loaded, &plans, 0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].evaluation.oos_trades, out[1].evaluation.oos_trades);
        assert_eq!(
            out[0].evaluation.oos_day_keys,
            out[1].evaluation.oos_day_keys
        );
        assert_eq!(out[0].evaluation.folds, out[1].evaluation.folds);
        assert!(
            out[0].evaluation.folds > 0,
            "the fixture must actually fold"
        );
        // Order is declaration order, never completion order (§2.2).
        assert_eq!(out[0].evaluation.instrument, "AAA");
        assert_eq!(out[1].evaluation.instrument, "BBB");
    }

    /// `oos_trades` counts the TEST windows, not the whole run — which is what
    /// makes it out-of-sample evidence rather than a run total.
    ///
    /// The third case is what gives it teeth: the same replay over the same
    /// bars, with a geometry whose test windows cover strictly fewer sessions,
    /// must not report more trades. A count read off the whole run would be
    /// identical under both and this would not fire.
    #[test]
    fn oos_trades_counts_the_test_windows_and_not_the_whole_run() {
        let loaded = loaded();
        let events = collect_events_for(&loaded, "SYN:RW")
            .expect("synthetic")
            .events;
        let wide = FoldSpec {
            scheme: FoldScheme::Rolling,
            train_days: 2,
            test_days: 5,
            step_days: 5,
        };
        // `step_days > test_days` is legal (D-0062 refuses only the reverse),
        // so this geometry reaches roughly one session in five — a large,
        // deliberate gap rather than a marginal one.
        let narrow = FoldSpec {
            scheme: FoldScheme::Rolling,
            train_days: 5,
            test_days: 1,
            step_days: 5,
        };
        let w = replay_pool(
            &loaded,
            &[plan_of(&loaded, "W", events.clone(), 40, wide)],
            0,
        );
        let n = replay_pool(&loaded, &[plan_of(&loaded, "N", events, 40, narrow)], 0);
        assert!(
            w[0].evaluation.folds > 0 && n[0].evaluation.folds > 0,
            "both geometries must fold"
        );
        assert!(
            n[0].evaluation.oos_day_keys.len() < w[0].evaluation.oos_day_keys.len(),
            "the narrow geometry must cover fewer sessions, or the comparison is empty"
        );
        // STRICT, and the strictness is the whole control. A replay that
        // counted the whole run rather than the test windows returns the same
        // number for both geometries, so `<=` would still hold and this test
        // would be decoration — which is exactly what the mutation control
        // caught when it was first written that way.
        assert!(
            n[0].evaluation.oos_trades < w[0].evaluation.oos_trades,
            "fewer out-of-sample sessions must yield strictly fewer out-of-sample trades: \
             narrow {} over {} session(s), wide {} over {}",
            n[0].evaluation.oos_trades,
            n[0].evaluation.oos_day_keys.len(),
            w[0].evaluation.oos_trades,
            w[0].evaluation.oos_day_keys.len()
        );
    }

    /// A contract that could not be planned contributes nothing AND keeps its
    /// reason, so `pool_evaluations` can render the right skip rather than
    /// inferring one from a zero.
    #[test]
    fn an_unplannable_contract_contributes_nothing_and_carries_its_reason() {
        let loaded = loaded();
        let events = collect_events_for(&loaded, "SYN:RW")
            .expect("synthetic")
            .events;
        let mut short = plan_of(&loaded, "SHORT", events, 40, spec());
        short.plan = None;
        short.insufficient_warmup = Some((3, 40));
        let out = replay_pool(&loaded, &[short], 0);
        assert_eq!(out[0].evaluation.oos_trades, 0);
        assert!(out[0].evaluation.oos_day_keys.is_empty());
        assert_eq!(out[0].evaluation.folds, 0);
        assert_eq!(
            out[0].evaluation.insufficient_warmup,
            Some((3, 40)),
            "the reason survives the replay — a zero with no reason is what \
             D-0070's pattern refuses"
        );
    }
}

#[cfg(test)]
mod pooled_identity_tests {
    use super::*;
    use crate::combo::collect_events_for;
    use crucible_funnel::walkforward::FoldScheme;

    /// The seam is WIRED, not merely present: a pool of two contracts comes
    /// back with two different registration hashes, so `Registry::claim` will
    /// charge two trials (D-0114, D-0124).
    ///
    /// `pooling`'s own tests prove the hash function separates its inputs.
    /// This proves `replay_pool` actually calls it with the contract that was
    /// replayed — a correct function invoked with a constant would pass those
    /// and fail this.
    #[test]
    fn each_contract_in_a_pool_carries_its_own_registration_hash() {
        let loaded = super::replay_pool_tests::loaded();
        let events = collect_events_for(&loaded, "SYN:RW")
            .expect("synthetic")
            .events;
        let spec = FoldSpec {
            scheme: FoldScheme::Rolling,
            train_days: 5,
            test_days: 2,
            step_days: 2,
        };
        let plans = vec![
            super::replay_pool_tests::plan_of(&loaded, "ESH2024", events.clone(), 40, spec),
            super::replay_pool_tests::plan_of(&loaded, "ESM2024", events, 40, spec),
        ];
        let out = replay_pool(&loaded, &plans, 0);
        assert_ne!(
            out[0].registration_hash, out[1].registration_hash,
            "two contracts must be two runs and therefore two trials"
        );
        assert_eq!(out[0].registration_hash.len(), 64);
        // The converse: the identity is a function of the contract, so the
        // same pool replayed again reproduces it. A hash that varied per call
        // would satisfy the assertion above and destroy the trial count.
        let again = replay_pool(&loaded, &plans, 0);
        assert_eq!(out[0].registration_hash, again[0].registration_hash);
        assert_eq!(out[1].registration_hash, again[1].registration_hash);
    }
}
