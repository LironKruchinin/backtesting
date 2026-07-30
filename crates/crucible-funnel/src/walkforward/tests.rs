//! The hand-derived walk-forward fixture.
//!
//! Every expected number below is arithmetic done on paper and written out in
//! the comment above the assertion, per CLAUDE.md §7. Nothing here was
//! obtained by running the code and pasting what it printed — that produces a
//! test that pins a bug as firmly as it pins a behaviour.
//!
//! ## The series
//!
//! 72 one-minute bars, 6 per trading day, 12 days (0–11). Every bar is
//! OHLC-flat (`open == high == low == close`), so a fill at the next bar's
//! open is a fill at a price written in the table below, and no bar's range
//! has to be reasoned about.
//!
//! The strategy is `fast crosses_above slow` / `fast crosses_below slow` with
//! `fast = sma(1)` and `slow = sma(2)`. Those reduce to something checkable
//! by eye: with `fast_t = c_t` and `slow_t = (c_t + c_{t-1})/2`,
//!
//! - `fast > slow` ⟺ `c_t > c_{t-1}`
//! - so `crosses_above` at bar *t* ⟺ `c_t > c_{t-1}` **and** `c_{t-1} <= c_{t-2}`
//! - and `crosses_below` at bar *t* ⟺ `c_t < c_{t-1}` **and** `c_{t-1} >= c_{t-2}`
//!
//! The grid warmup is `max(1, 2) + 1 = 3` bars: 2 for `sma(2)`, plus one for
//! the crossover, which has no opinion until it has two readings.
//!
//! Prices are 100 everywhere except four **episodes**, each occupying exactly
//! one trading day. An episode on day *d* (bars `6d..6d+6`) is:
//!
//! | bar | close | what fires | what the engine does |
//! |---|---|---|---|
//! | 6d+0 | `entry` | `crosses_above` (rose, and the bar before did not) | BUY placed |
//! | 6d+1 | `entry` | nothing (flat bar) | BUY fills at open `entry` |
//! | 6d+2 | `peak` | `crosses_above` again — already long, so no order | marks |
//! | 6d+3 | `exit` | `crosses_below` (fell, prior bar had not) | SELL placed |
//! | 6d+4 | `exit` | nothing | SELL fills at open `exit` |
//! | 6d+5 | `100` | `crosses_below` — but flat, so no order | back to baseline |
//!
//! so the round-trip is `(exit − entry)` points at $50 a point, one contract.
//!
//! | day | bars | entry | peak | exit | net |
//! |---|---|---|---|---|---|
//! | 2 | 12–17 | 110 | 130 | 118 | **+$400** |
//! | 5 | 30–35 | 104 | 114 | 106 | +$100 |
//! | 7 | 42–47 | 106 | 106 | 103 | −$150 |
//! | 9 | 54–59 | 102 | 110 | 106 | +$200 |
//!
//! Day 7's `peak == entry` gives the flat-then-fall shape that produces the
//! loser; the crossover conditions still hold, because `>=` and `<=` are not
//! strict.
//!
//! **Day 2 is the control.** It sits in every fold's *training* window and no
//! fold's test window. Its +$400 is the difference between the whole-run
//! number `crucible combo` would print (+0.55 %) and the out-of-sample number
//! this runner prints (+0.15 %). If a slicing bug ever lets training PnL leak
//! into an out-of-sample statistic, that $400 is what shows up.

use crucible_core::prelude::*;
use crucible_engine::{
    AccountCapture, AccountSeries, BacktestParams, BacktestResult, FreeFills, run_capturing,
};
use crucible_strategies::combo::{ComboSpec, Grid, IndicatorSpec, IntAxis, RuleSource};

use super::folds::{FoldPlan, FoldScheme, FoldSpec};
use super::runner::{RunIdentity, WalkForwardError, run_grid};
use super::window::RunTrace;
use crucible_strategies::combo::ConfigHash;

/// Bars per trading day in the fixture.
const BARS_PER_DAY: usize = 6;
/// $100,000, in nano-USD.
const CASH: NanoUsd = 100_000_000_000_000;
/// An arbitrary annualization: the fixture is not a real calendar, and 252
/// makes the one hand-computed Sharpe come out as a closed form.
const PER_YEAR: f64 = 252.0;

/// The 72 closes, built from the episode table in the module docs.
fn closes() -> Vec<i64> {
    let mut p = vec![100i64; 12 * BARS_PER_DAY];
    let mut episode = |day: usize, entry: i64, peak: i64, exit: i64| {
        let b = day * BARS_PER_DAY;
        p[b] = entry;
        p[b + 1] = entry;
        p[b + 2] = peak;
        p[b + 3] = exit;
        p[b + 4] = exit;
        p[b + 5] = 100;
    };
    episode(2, 110, 130, 118); // +$400, training-only — the control
    episode(5, 104, 114, 106); // +$100, fold 0 test
    episode(7, 106, 106, 103); // −$150, fold 1 test
    episode(9, 102, 110, 106); // +$200, fold 2 test
    p
}

fn events() -> Vec<MarketEvent> {
    closes()
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let price = Price::from_points(c);
            MarketEvent::Bar(Bar {
                instrument: InstrumentId::new("SYN:WF"),
                tf: TimeFrame::M1,
                ts_open: Ts(i64::try_from(i).expect("small fixture index") * 60_000_000_000),
                open: price,
                high: price,
                low: price,
                close: price,
                volume: 1,
            })
        })
        .collect()
}

/// One key per bar: bar *i* belongs to trading day `i / 6`.
fn day_keys(n_bars: usize) -> Vec<i64> {
    (0..n_bars)
        .map(|i| i64::try_from(i / BARS_PER_DAY).expect("small fixture index"))
        .collect()
}

fn spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("SYN:WF"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    }
}

/// `fast crosses_above slow` in / `crosses_below` out, long-only, with the
/// slow period on an axis so the same builder makes both the one-combo
/// fixture and the two-combo §2.6 control.
fn grid(slow_periods: IntAxis) -> Grid {
    ComboSpec::new(
        vec![
            (
                "fast".to_owned(),
                IndicatorSpec::Sma {
                    period: IntAxis::Fixed(1),
                },
            ),
            (
                "slow".to_owned(),
                IndicatorSpec::Sma {
                    period: slow_periods,
                },
            ),
        ],
        &RuleSource {
            enter_long: Some("fast crosses_above slow".to_owned()),
            exit_long: Some("fast crosses_below slow".to_owned()),
            ..RuleSource::default()
        },
        Qty(1),
    )
    .expect("the fixture spec is valid")
    .expand()
    .expect("it expands")
}

fn params() -> BacktestParams {
    BacktestParams {
        initial_cash_nano_usd: CASH,
        bars_per_year: PER_YEAR,
    }
}

fn identity() -> RunIdentity {
    RunIdentity {
        config_hash: ConfigHash::from_bytes([0x5a; 32]),
        root_seed: 42,
    }
}

/// train 4 / test 2 / step 2 trading days, rolling.
fn fold_spec() -> FoldSpec {
    FoldSpec {
        scheme: FoldScheme::Rolling,
        train_days: 4,
        test_days: 2,
        step_days: 2,
    }
}

fn dollars(n: NanoUsd) -> i64 {
    assert_eq!(n % 1_000_000_000, 0, "the fixture deals in whole dollars");
    n / 1_000_000_000
}

/// The fold layout, laid out by hand.
///
/// The grid warmup is 3 bars, which lands inside day 0 (bars 0–5), so day 0
/// is a partial session and is dropped: 3 bars (3, 4, 5) are reported as
/// `partial_day_bars`, and the first evaluable day is day 1 at bar 6. That
/// leaves 11 evaluable days (1–11).
///
/// With train 4 / test 2 / step 2, in evaluable-day indices:
///
/// | fold | train days | train bars | test days | test bars |
/// |---|---|---|---|---|
/// | 0 | 0..4 (cal. 1–4) | 6..30 | 4..6 (cal. 5–6) | 30..42 |
/// | 1 | 2..6 (cal. 3–6) | 18..42 | 6..8 (cal. 7–8) | 42..54 |
/// | 2 | 4..8 (cal. 5–8) | 30..54 | 8..10 (cal. 9–10) | 54..66 |
///
/// A fourth fold would need evaluable days 10..12; there are 11, so it does
/// not exist and evaluable day 10 (calendar day 11, bars 66..72) is an
/// unused tail.
#[test]
fn the_fold_layout_is_the_hand_derived_one() {
    let ev = events();
    let plan = FoldPlan::build(&day_keys(ev.len()), 3, fold_spec()).expect("the fixture plans");

    assert_eq!(plan.partial_day_bars(), 3);
    assert_eq!(plan.days().len(), 11);
    assert_eq!(plan.folds().len(), 3);
    assert_eq!(plan.unused_tail_days(), 1);

    assert_eq!(plan.folds()[0].train.bars, 6..30);
    assert_eq!(plan.folds()[0].test.bars, 30..42);
    assert_eq!(plan.folds()[1].train.bars, 18..42);
    assert_eq!(plan.folds()[1].test.bars, 42..54);
    assert_eq!(plan.folds()[2].train.bars, 30..54);
    assert_eq!(plan.folds()[2].test.bars, 54..66);

    // The test windows tile: no bar is out-of-sample twice, and no bar
    // between the first and last is skipped.
    assert_eq!(plan.oos_bars(), 36);
}

/// One combo, walked forward, with every number derived above.
///
/// The equity curve, in whole dollars, is flat at 100,000 until day 2 and
/// then steps at each round-trip close: 100,400 from bar 16, 100,500 from
/// bar 34, 100,350 from bar 46, 100,550 from bar 58.
///
/// | window | anchor bar (equity) | last bar (equity) | net | on $100k |
/// |---|---|---|---|---|
/// | fold 0 train (6..30) | 5 (100,000) | 29 (100,400) | +$400 | +0.40 % |
/// | fold 0 test (30..42) | 29 (100,400) | 41 (100,500) | +$100 | +0.10 % |
/// | fold 1 train (18..42) | 17 (100,400) | 41 (100,500) | +$100 | +0.10 % |
/// | fold 1 test (42..54) | 41 (100,500) | 53 (100,350) | −$150 | −0.15 % |
/// | fold 2 train (30..54) | 29 (100,400) | 53 (100,350) | −$50 | −0.05 % |
/// | fold 2 test (54..66) | 53 (100,350) | 65 (100,550) | +$200 | +0.20 % |
#[test]
fn every_fold_reports_the_window_it_names() {
    let ev = events();
    let g = grid(IntAxis::Fixed(2));
    assert_eq!(g.len(), 1);
    assert_eq!(g.max_warmup_bars(), 3);

    let plan = FoldPlan::build(&day_keys(ev.len()), g.max_warmup_bars(), fold_spec())
        .expect("the fixture plans");
    let report = run_grid(&ev, &g, &plan, &spec(), &params(), &identity(), &FreeFills)
        .expect("the fixture runs");

    let combo = &report.combos[0];
    assert_eq!(combo.folds.len(), 3);

    let pct = |x: f64, want: f64| assert!((x - want).abs() < 1e-9, "{x} != {want}");

    pct(combo.folds[0].is.total_return_pct, 0.40);
    pct(combo.folds[0].oos.total_return_pct, 0.10);
    pct(combo.folds[1].is.total_return_pct, 0.10);
    pct(combo.folds[1].oos.total_return_pct, -0.15);
    pct(combo.folds[2].is.total_return_pct, -0.05);
    pct(combo.folds[2].oos.total_return_pct, 0.20);

    // One round-trip closes inside each test window; the training windows
    // overlap, so fold 2's holds the two that closed at bars 34 and 46.
    assert_eq!(combo.folds[0].oos.round_trips, 1);
    assert_eq!(combo.folds[1].oos.round_trips, 1);
    assert_eq!(combo.folds[2].oos.round_trips, 1);
    assert_eq!(combo.folds[2].is.round_trips, 2);
    assert_eq!(combo.folds[1].oos.win_rate, Some(0.0)); // the −$150 one
    assert_eq!(combo.folds[0].oos.win_rate, Some(1.0));

    // Fold 0's test window drew down from $100,500 (bar 32, marked at 114)
    // to $100,100: 400/100,500 = 0.398009950248756…%.
    pct(
        combo.folds[0].oos.max_drawdown_pct,
        100.0 * 400.0 / 100_500.0,
    );

    // `FreeFills` charges nothing, so every window's cost is zero — which is
    // exactly why it is a screening model and not a result (D-0006).
    assert!(combo.folds.iter().all(|f| f.oos.fees_nano_usd == 0));
}

/// The headline number, and the reason this runner exists.
///
/// The whole run made +$550. Out of sample it made +$150. The missing $400 is
/// day 2's round-trip, which closed inside every fold's *training* window and
/// no fold's test window — it is the number a whole-run report quotes and a
/// walk-forward report must not.
#[test]
fn the_out_of_sample_headline_excludes_the_training_windows() {
    let ev = events();
    let g = grid(IntAxis::Fixed(2));
    let plan =
        FoldPlan::build(&day_keys(ev.len()), g.max_warmup_bars(), fold_spec()).expect("plans");
    let report =
        run_grid(&ev, &g, &plan, &spec(), &params(), &identity(), &FreeFills).expect("runs");
    let combo = &report.combos[0];

    assert_eq!(dollars(combo.whole_run.final_equity_nano_usd), 100_550);
    assert_eq!(dollars(combo.oos_pooled.final_equity_nano_usd), 100_150);
    assert_eq!(
        dollars(combo.whole_run.final_equity_nano_usd)
            - dollars(combo.oos_pooled.final_equity_nano_usd),
        400,
        "the difference is exactly day 2's training-window round-trip"
    );

    // Three round-trips out of sample, two of them winners: +$100, −$150,
    // +$200.
    assert_eq!(combo.oos_pooled.round_trips, 3);
    assert_eq!(combo.oos_pooled.win_rate, Some(2.0 / 3.0));

    // The pooled out-of-sample drawdown runs from the $100,500 mark at bar 32
    // to the $99,950 trough at bar 45: 550/100,500 = 0.547263681592039…%.
    assert!(
        (combo.oos_pooled.max_drawdown_pct - 100.0 * 550.0 / 100_500.0).abs() < 1e-9,
        "{}",
        combo.oos_pooled.max_drawdown_pct
    );

    // The training windows union to bars 6..54, anchored at bar 5: $100,000
    // to $100,350, three round-trips.
    assert_eq!(dollars(combo.is_pooled.final_equity_nano_usd), 100_350);
    assert_eq!(combo.is_pooled.round_trips, 3);
}

/// One Sharpe, in closed form, because a Sharpe nobody has done on paper is a
/// number nobody has checked.
///
/// Fold 1's out-of-sample window is bars 42..54 anchored at bar 41. Rebased on
/// $100,000 the curve is 100,000 for four points, then 99,850 for nine, so of
/// the twelve per-bar returns exactly one is nonzero: −0.0015.
///
/// - mean = −0.0015 / 12 = −0.000125
/// - Σ(r − mean)² = 11 × (0.000125)² + (−0.001375)² = 2.0625 × 10⁻⁶
/// - var = that / 11 = 1.875 × 10⁻⁷, so sd = 1/(2000√3)
/// - mean/sd = −0.000125 × 2000√3 = −1/(2√3)
/// - Sharpe = −1/(2√3) × √252 = −6√7/(2√3) = −3√(7/3) = **−√21**
#[test]
fn a_fold_sharpe_is_hand_derivable() {
    let ev = events();
    let g = grid(IntAxis::Fixed(2));
    let plan =
        FoldPlan::build(&day_keys(ev.len()), g.max_warmup_bars(), fold_spec()).expect("plans");
    let report =
        run_grid(&ev, &g, &plan, &spec(), &params(), &identity(), &FreeFills).expect("runs");

    let sharpe = report.combos[0].folds[1]
        .oos
        .sharpe_naive
        .expect("twelve returns, one of them nonzero");
    assert!(
        (sharpe + 21.0_f64.sqrt()).abs() < 1e-12,
        "{sharpe} is not −√21"
    );
}

/// The §2.6 control, in walk-forward terms.
///
/// Alone, the `slow = 2` combo has a 3-bar warmup and captures day 2's +$400.
/// Placed in a grid beside a `slow = 20` combo, the shared warmup becomes 21
/// bars, day 2 falls inside it, and the two orders it would have placed are
/// dropped and counted. Its whole-run equity falls from $100,550 to $100,150
/// — it gains nothing from being short, and what it gave up is visible rather
/// than absorbed.
#[test]
fn a_short_warmup_combo_gains_nothing_from_being_short() {
    let ev = events();
    let keys = day_keys(ev.len());

    let alone = grid(IntAxis::Fixed(2));
    assert_eq!(alone.max_warmup_bars(), 3);
    let alone_plan = FoldPlan::build(&keys, alone.max_warmup_bars(), fold_spec()).expect("plans");
    let alone_report = run_grid(
        &ev,
        &alone,
        &alone_plan,
        &spec(),
        &params(),
        &identity(),
        &FreeFills,
    )
    .expect("runs");

    let mixed = grid(IntAxis::List(vec![2, 20]));
    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed.max_warmup_bars(), 21); // 20 + 1 for the crossover
    let mixed_plan = FoldPlan::build(&keys, mixed.max_warmup_bars(), fold_spec()).expect("plans");
    let mixed_report = run_grid(
        &ev,
        &mixed,
        &mixed_plan,
        &spec(),
        &params(),
        &identity(),
        &FreeFills,
    )
    .expect("runs");

    let short_alone = &alone_report.combos[0];
    let short_in_grid = &mixed_report.combos[0];
    assert_eq!(short_alone.label, short_in_grid.label, "the same combo");

    // The head start exists — the fixture has to actually show it.
    assert_eq!(
        dollars(short_alone.whole_run.final_equity_nano_usd),
        100_550
    );
    assert_eq!(short_alone.suppressed_intents, 0);

    // And in the grid it is gone. Two intents are dropped, at bars 12 and 14:
    // the second exists only *because* the first was dropped — the inner
    // strategy sees a flat book at bar 14 and asks to enter again, where the
    // unaligned run was already long and asked for nothing. The count is what
    // the combo wanted given the aligned portfolio, which is the honest
    // reading of "what being early would have bought it".
    assert_eq!(short_in_grid.suppressed_intents, 2);
    assert_eq!(
        dollars(short_in_grid.whole_run.final_equity_nano_usd),
        100_150
    );

    // Both combos in the grid are cut at the same bars, by construction:
    // there is one plan, and every combo reports one result per fold of it.
    assert_eq!(
        mixed_report.combos[0].folds.len(),
        mixed_report.combos[1].folds.len()
    );
    assert_eq!(
        mixed_report.combos[0].own_warmup_bars, 2,
        "its own warmup is still reported, it is just not what it ran with"
    );
    assert_eq!(mixed_report.combos[1].own_warmup_bars, 20);
}

/// Two runs of the same inputs produce bit-identical results (CLAUDE.md
/// §2.2). Checked on the raw integer nano-USD and on the float statistics'
/// bit patterns, because "bit-identical" is the claim and `==` on `f64` is
/// how you check it.
#[test]
fn two_runs_are_bit_identical() {
    let ev = events();
    let g = grid(IntAxis::List(vec![2, 3, 20]));
    let plan =
        FoldPlan::build(&day_keys(ev.len()), g.max_warmup_bars(), fold_spec()).expect("plans");
    let go =
        || run_grid(&ev, &g, &plan, &spec(), &params(), &identity(), &FreeFills).expect("runs");

    let (a, b) = (go(), go());
    assert_eq!(a.combos.len(), b.combos.len());
    for (x, y) in a.combos.iter().zip(&b.combos) {
        assert_eq!(x.id, y.id);
        assert_eq!(
            x.oos_pooled.final_equity_nano_usd,
            y.oos_pooled.final_equity_nano_usd
        );
        assert_eq!(
            x.oos_pooled.sharpe_naive.map(f64::to_bits),
            y.oos_pooled.sharpe_naive.map(f64::to_bits)
        );
        assert_eq!(
            x.oos_pooled.max_drawdown_pct.to_bits(),
            y.oos_pooled.max_drawdown_pct.to_bits()
        );
        for (fx, fy) in x.folds.iter().zip(&y.folds) {
            assert_eq!(
                fx.seed, fy.seed,
                "a seed that moves between runs is not a seed"
            );
            assert_eq!(
                fx.oos.total_return_pct.to_bits(),
                fy.oos.total_return_pct.to_bits()
            );
        }
    }
}

/// Every (combo, fold) gets its own seed, and no two collide.
#[test]
fn derived_seeds_are_distinct_per_combo_and_fold() {
    let ev = events();
    let g = grid(IntAxis::List(vec![2, 3, 20]));
    let plan =
        FoldPlan::build(&day_keys(ev.len()), g.max_warmup_bars(), fold_spec()).expect("plans");
    let report =
        run_grid(&ev, &g, &plan, &spec(), &params(), &identity(), &FreeFills).expect("runs");

    let mut seeds: Vec<u64> = report
        .combos
        .iter()
        .flat_map(|c| c.folds.iter().map(|f| f.seed))
        .collect();
    let total = seeds.len();
    assert!(total >= 3, "the fixture must have several runs to collide");
    seeds.sort_unstable();
    seeds.dedup();
    assert_eq!(seeds.len(), total, "two runs shared a seed");
}

/// A plan laid out behind a different warmup than the grid's would score
/// combos on bars their neighbours never saw. The runner refuses rather than
/// trusting the caller to have passed the matching one.
#[test]
fn a_plan_from_another_grid_is_refused() {
    let ev = events();
    let g = grid(IntAxis::Fixed(2));
    let wrong = FoldPlan::build(&day_keys(ev.len()), 21, fold_spec()).expect("plans");
    let err = run_grid(&ev, &g, &wrong, &spec(), &params(), &identity(), &FreeFills)
        .expect_err("a mismatched warmup must refuse");
    assert_eq!(
        err,
        WalkForwardError::PlanWarmupMismatch {
            plan_warmup: 21,
            grid_warmup: 3,
        }
    );
    assert!(err.to_string().contains("§2.6"));
}

/// And a plan laid out over a different series entirely.
#[test]
fn a_plan_over_another_series_is_refused() {
    let ev = events();
    let g = grid(IntAxis::Fixed(2));
    let plan = FoldPlan::build(&day_keys(ev.len()), 3, fold_spec()).expect("plans");
    let err = run_grid(
        &ev[..60],
        &g,
        &plan,
        &spec(),
        &params(),
        &identity(),
        &FreeFills,
    )
    .expect_err("a mismatched series must refuse");
    assert_eq!(
        err,
        WalkForwardError::PlanSeriesMismatch {
            plan_bars: 72,
            series_bars: 60,
        }
    );
}

// ------------------------------------------------------------------------
// One producer of "which trading day", two consumers (D-0071)
// ------------------------------------------------------------------------

/// A feed over the shared fixture, so a test can drive one replay itself
/// rather than through [`run_grid`].
struct SliceFeed<'a> {
    events: &'a [MarketEvent],
    at: usize,
}

impl Feed for SliceFeed<'_> {
    fn next_event(&mut self) -> Option<MarketEvent> {
        let ev = self.events.get(self.at)?.clone();
        self.at += 1;
        Some(ev)
    }
}

/// One replay of the fixture's single combo, with the account-evaluation
/// series captured against `capture_keys`.
fn replay_capturing(
    events: &[MarketEvent],
    capture_keys: &[i64],
) -> (BacktestResult, AccountSeries) {
    let g = grid(IntAxis::Fixed(2));
    let mut strategy = g.aligned_strategy(0);
    let mut fills = FreeFills;
    let mut capture = AccountCapture::new(capture_keys, CASH, TimeFrame::M1);
    let result = run_capturing(
        &mut SliceFeed { events, at: 0 },
        &mut strategy,
        &mut fills,
        &spec(),
        &params(),
        &mut capture,
    )
    .expect("the fixture replays");
    (result, capture.finish())
}

/// The wall-clock slicer this control exists to falsify.
///
/// The fixture's sessions are 6 bars long and begin at bar indices 0, 6, 12 …
/// A wall-clock day boundary does not land there — that is the whole content
/// of the 17:00 CT roll — so this one cuts three bars earlier, mid-session:
/// wall-clock day *k* is bars `6k − 3 .. 6k + 3`.
fn wall_clock_keys(n_bars: usize) -> Vec<i64> {
    (0..n_bars)
        .map(|i| i64::try_from((i + 3) / BARS_PER_DAY).expect("small fixture index"))
        .collect()
}

/// Keys of the days that closed at least `dollars` down — a daily loss limit,
/// which is a question about a *day* and therefore about a boundary.
fn days_losing_at_least(series: &AccountSeries, dollars: i64) -> Vec<i64> {
    series
        .days
        .iter()
        .filter(|r| r.close_pnl_nano_usd <= -dollars * 1_000_000_000)
        .map(|r| r.trading_day_key)
        .collect()
}

/// The day record carrying `key`, or a panic naming it.
fn day_pnl(series: &AccountSeries, key: i64) -> NanoUsd {
    series
        .days
        .iter()
        .find(|r| r.trading_day_key == key)
        .unwrap_or_else(|| panic!("the capture has no day {key}"))
        .close_pnl_nano_usd
}

/// **The reconciliation, and it has teeth.** Walk-forward's fold attribution
/// and account-eval's day slicing read the *same* trading-day key slice, so a
/// day's PnL is the same number in both reports — to the nanodollar.
///
/// Two independent attributions of "which day" is how a daily-loss-limit
/// breach lands on a different date in two reports, and neither report looks
/// wrong on its own. The device that prevents it is the one D-0015, D-0060 and
/// D-0062 already use: the layer that may hold a calendar computes the keys
/// once, and every layer below takes them as data.
///
/// The identity checked here, for every evaluable trading day *d*:
///
/// ```text
/// DayRecord[d].close_pnl  ==  window(day d's bars).final − .initial
/// ```
///
/// Both sides are `equity[last bar of d] − equity[last bar of d−1]`: the fold
/// machinery gets there by anchoring a window at `start − 1` (D-0063) and the
/// capture by opening each day at the previous day's close. They agree because
/// they are the same convention applied to the same boundaries — and they are
/// the same boundaries because there is one producer.
#[test]
fn day_slicing_and_fold_attribution_reconcile_to_the_nanodollar() {
    let ev = events();
    let keys = day_keys(ev.len());
    let g = grid(IntAxis::Fixed(2));
    let plan = FoldPlan::build(&keys, g.max_warmup_bars(), fold_spec()).expect("the fixture plans");
    let (result, series) = replay_capturing(&ev, &keys);
    let trace = RunTrace::new(&result.equity, &result.closed_trades, &result.fee_events);

    for (d, &key) in plan.days().iter().enumerate() {
        let bars = plan.day_start_bar(d)..plan.day_start_bar(d + 1);
        let by_fold_machinery = {
            let s = trace.window(bars.clone(), CASH, PER_YEAR);
            s.final_equity_nano_usd - s.initial_equity_nano_usd
        };
        let record = series
            .days
            .iter()
            .find(|r| r.trading_day_key == key)
            .expect("the capture saw every day the plan did");
        assert_eq!(
            record.bars, bars,
            "day {key} occupies different bars in the two consumers"
        );
        assert_eq!(
            record.close_pnl_nano_usd, by_fold_machinery,
            "day {key} has two different PnLs"
        );
    }

    // And a fold's out-of-sample PnL is the sum of its days'. The fixture's
    // fold 1 test window is calendar days 7–8, holding the −$150 round-trip
    // (day 7) and a flat day: −$150 + $0 = −$150.
    let fold1 = &plan.folds()[1];
    let oos = trace.window(fold1.test.bars.clone(), CASH, PER_YEAR);
    let summed: NanoUsd = plan.days()[fold1.test.days.clone()]
        .iter()
        .map(|&key| day_pnl(&series, key))
        .sum();
    assert_eq!(
        summed,
        oos.final_equity_nano_usd - oos.initial_equity_nano_usd
    );
    assert_eq!(dollars(summed), -150);
}

/// **A planted daily-loss-limit breach lands on the same day in both
/// consumers.** The reconciliation above shows the two agree about every day's
/// PnL; this shows the agreement survives the question the boundary exists
/// for, which is the one that is asked of a *named* day.
///
/// The fixture's only losing session is day 7's −$150 round-trip (module
/// docs), so a $100 daily loss limit has exactly one answer. Computed twice,
/// from opposite ends:
///
/// - **account-sliced** — scan `AccountSeries::days` for a
///   `close_pnl_nano_usd` at or below −$100. Answer: day **7**.
/// - **fold-attributed** — never look at a `DayRecord`: take each day's bar
///   range from `FoldPlan` and ask `RunTrace::window` what the equity did
///   across it. Answer: day **7**, and −$150 to the nanodollar.
///
/// And it is out-of-sample: day 7 sits in fold 1's *test* window, so the
/// breach is one a report would quote.
///
/// One producer of "which day", so one date. Two producers is how a breach
/// lands on 2024-07-08 in the walk-forward report and 2024-07-09 in the
/// account report, with neither of them looking wrong on its own.
#[test]
fn a_planted_daily_loss_limit_breach_lands_on_the_same_day_in_both_consumers() {
    /// The planted limit, in whole dollars.
    const DLL_USD: i64 = 100;

    let ev = events();
    let keys = day_keys(ev.len());
    let g = grid(IntAxis::Fixed(2));
    let plan = FoldPlan::build(&keys, g.max_warmup_bars(), fold_spec()).expect("the fixture plans");
    let (result, series) = replay_capturing(&ev, &keys);
    let trace = RunTrace::new(&result.equity, &result.closed_trades, &result.fee_events);

    // The account-sliced answer: the capture's own day records.
    assert_eq!(
        days_losing_at_least(&series, DLL_USD),
        vec![7],
        "the capture must find exactly one breaching session"
    );
    assert_eq!(dollars(day_pnl(&series, 7)), -150);

    // The fold-attributed answer: the same question asked of the windows the
    // fold machinery cuts, without consulting a `DayRecord` at all.
    let mut by_fold_machinery: Vec<i64> = Vec::new();
    for (d, &key) in plan.days().iter().enumerate() {
        let s = trace.window(
            plan.day_start_bar(d)..plan.day_start_bar(d + 1),
            CASH,
            PER_YEAR,
        );
        let pnl = s.final_equity_nano_usd - s.initial_equity_nano_usd;
        if pnl <= -DLL_USD * 1_000_000_000 {
            by_fold_machinery.push(key);
            assert_eq!(
                pnl,
                day_pnl(&series, key),
                "day {key} breached by a different amount in the two consumers"
            );
        }
    }
    assert_eq!(
        by_fold_machinery,
        vec![7],
        "the two consumers must name the same breaching day"
    );

    // Out-of-sample: fold 1's test window is the one that holds it.
    let fold1 = &plan.folds()[1];
    assert!(
        plan.days()[fold1.test.days.clone()].contains(&7),
        "the planted breach must land in a test window, or it is not a number \
         any report would quote"
    );
}

/// **The negative control the ruling earns.** Hand ONE consumer a wall-clock
/// slicer and the reconciliation breaks — with the daily-loss question landing
/// on a day the other consumer does not have.
///
/// Hand arithmetic. Every episode peaks *inside* its session and gives some of
/// it back before the close, so a boundary drawn three bars early cuts each one
/// at its high — booking the run-up to one day and the give-back to the next.
///
/// Equity in whole dollars (the entry fills at the episode's second bar, the
/// exit at its fifth):
///
/// | bars | 6–11 | 12–13 | 14 | 15–29 | 30–31 | 32 | 33–41 |
/// |---|---|---|---|---|---|---|---|
/// | equity | 100,000 | 100,000 | 101,000 | 100,400 | 100,400 | 100,900 | 100,500 |
///
/// (bar 14 marks day 2's long at 130: +20 pts × $50 = +$1,000; bar 32 marks
/// day 5's long at 114: +10 × $50 = +$500.)
///
/// - **Calendar day 2** is bars 12–17: `equity[17] − equity[11]` =
///   100,400 − 100,000 = **+$400**. Calendar day 3 is bars 18–23: **$0**.
/// - **Wall-clock day 2** is bars 9–14: `equity[14] − equity[8]` =
///   101,000 − 100,000 = **+$1,000**. Wall-clock day 3 is bars 15–20:
///   `equity[20] − equity[14]` = 100,400 − 101,000 = **−$600**.
/// - **Calendar day 5** is bars 30–35: `equity[35] − equity[29]` =
///   100,500 − 100,400 = **+$100**. Calendar day 6 is bars 36–41: **$0**.
/// - **Wall-clock day 5** is bars 27–32: `equity[32] − equity[26]` =
///   100,900 − 100,400 = **+$500**. Wall-clock day 6 is bars 33–38:
///   `equity[38] − equity[32]` = 100,500 − 100,900 = **−$400**.
///
/// So a $300 daily loss limit fires **twice** under the wall-clock slicer and
/// on **no** calendar day at all. Same money, same trades, same engine — one
/// boundary moved three bars, and a strategy whose worst session was −$150
/// acquires a −$600 one.
#[test]
fn a_wall_clock_slicer_books_a_daily_loss_the_calendar_never_had() {
    let ev = events();
    let calendar = day_keys(ev.len());
    let wall_clock = wall_clock_keys(ev.len());
    assert_ne!(calendar, wall_clock);

    let (_, by_calendar) = replay_capturing(&ev, &calendar);
    let (_, by_wall_clock) = replay_capturing(&ev, &wall_clock);

    assert_eq!(dollars(day_pnl(&by_calendar, 2)), 400);
    assert_eq!(dollars(day_pnl(&by_calendar, 3)), 0);
    assert_eq!(dollars(day_pnl(&by_wall_clock, 2)), 1_000);
    assert_eq!(dollars(day_pnl(&by_wall_clock, 3)), -600);

    assert_eq!(dollars(day_pnl(&by_calendar, 5)), 100);
    assert_eq!(dollars(day_pnl(&by_calendar, 6)), 0);
    assert_eq!(dollars(day_pnl(&by_wall_clock, 5)), 500);
    assert_eq!(dollars(day_pnl(&by_wall_clock, 6)), -400);

    // The planted daily-loss-limit breach: $300, tested against every day.
    assert!(
        days_losing_at_least(&by_calendar, 300).is_empty(),
        "the calendar has no $300 losing session"
    );
    assert_eq!(
        days_losing_at_least(&by_wall_clock, 300),
        vec![3, 6],
        "the wall-clock slicer must manufacture the breaches, or this control is decoration"
    );

    // And the reconciliation above genuinely fails on these keys: at least one
    // day's PnL disagrees with the day carrying the same label in the other
    // consumer.
    let disagreements = by_wall_clock
        .days
        .iter()
        .filter(|w| {
            by_calendar
                .days
                .iter()
                .find(|c| c.trading_day_key == w.trading_day_key)
                .is_none_or(|c| c.close_pnl_nano_usd != w.close_pnl_nano_usd)
        })
        .count();
    assert!(
        disagreements > 0,
        "two slicers that never disagree are the same slicer"
    );
}
