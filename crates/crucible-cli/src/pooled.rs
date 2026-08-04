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

use crucible_data::calendar::Calendar;
use crucible_data::continuous::{ContinuousError, RollTable, read_roll_table};
use crucible_data::ingest::window::days_from_civil;
use crucible_funnel::walkforward::{FoldPlan, FoldSpec};

use crate::combo::{Series, collect_events_in_window};
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

    let mut planned = Vec::with_capacity(contracts.len());
    for instrument in contracts {
        let window = table
            .front_window(instrument)
            .map_err(|e| (EXIT_USAGE, format!("{instrument}: {e}")))?;
        let (start, end) = (
            crucible_data::ingest::window::civil_from_days(days_from_civil(
                crucible_data::ingest::window::date_of(window.start_ts()),
            ))
            .to_string(),
            crucible_data::ingest::window::civil_from_days(days_from_civil(
                crucible_data::ingest::window::date_of(window.end_ts()),
            ))
            .to_string(),
        );
        let series = collect_events_in_window(loaded, instrument, Some((&start, &end)))?;

        // The D-0071 device: computed once, here, and read by every consumer.
        let day_keys: Vec<i64> = series
            .events
            .iter()
            .map(|event| days_from_civil(calendar.trading_day(event.avail_ts())))
            .collect();
        let mut front_window_days = day_keys.clone();
        front_window_days.dedup();

        // A front window that cannot fit one complete fold is planned as
        // `None` rather than refused: the pool reports it as skipped with a
        // count, and a deliberately short contract is a legitimate member of a
        // pool that simply contributes nothing (D-0119).
        let plan = FoldPlan::build(&day_keys, loaded.grid.max_warmup_bars(), fold_spec).ok();
        planned.push(PooledContractPlan {
            instrument: instrument.clone(),
            series,
            day_keys,
            front_window_days,
            plan,
            fold_needs_sessions: needs,
        });
    }
    Ok(planned)
}
