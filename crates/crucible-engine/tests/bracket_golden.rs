//! Golden fixtures for stops and targets under `stop_first_intrabar`, with
//! every number derived by hand in the comment above it.
//!
//! These are discriminating tests, not smoke tests: each one states what a
//! *different* intrabar convention would have produced, so a change that
//! quietly flipped the rule cannot pass by moving one expected value. Policy
//! (CLAUDE.md §7, testdata/README.md): hand arithmetic, or it does not merge.
//!
//! The scenario is the same throughout, so the arithmetic stays checkable:
//! ES-like contract (tick 0.25, $50/pt), $100,000 initial cash, one contract,
//! a bracket of **8 ticks (2.00 points) stop / 12 ticks (3.00 points)
//! target**, and an entry that fills at 100.00 on bar 1 — which puts the stop
//! at 98.00 and the target at 103.00 for a long, and at 102.00 / 97.00 for a
//! short.

use crucible_core::prelude::*;
use crucible_engine::{BacktestParams, BacktestResult, FreeFills, SpreadCrossFills, run};

const INITIAL_CASH: NanoUsd = 100_000_000_000_000; // $100,000

/// Places one bracketed order on bar 0 and then never trades again, so every
/// exit in these fixtures is the bracket's doing and nothing else's.
struct EnterOnceBracketed {
    bar: usize,
    side: Side,
    bracket: Option<Bracket>,
}

impl EnterOnceBracketed {
    fn long() -> EnterOnceBracketed {
        EnterOnceBracketed {
            bar: 0,
            side: Side::Buy,
            bracket: Some(Bracket::new(Some(8), Some(12))),
        }
    }

    fn short() -> EnterOnceBracketed {
        EnterOnceBracketed {
            bar: 0,
            side: Side::Sell,
            bracket: Some(Bracket::new(Some(8), Some(12))),
        }
    }
}

impl Strategy for EnterOnceBracketed {
    fn warmup_bars(&self) -> usize {
        0
    }

    fn on_event(&mut self, _ev: &MarketEvent, _view: &PortfolioView, actions: &mut Actions) {
        if self.bar == 0 {
            match (self.side, self.bracket) {
                (Side::Buy, Some(b)) => actions.buy_bracketed(Qty(1), b),
                (Side::Sell, Some(b)) => actions.sell_bracketed(Qty(1), b),
                (Side::Buy, None) => actions.buy(Qty(1)),
                (Side::Sell, None) => actions.sell(Qty(1)),
            }
        }
        self.bar += 1;
    }
}

struct VecFeed {
    events: std::vec::IntoIter<MarketEvent>,
}

impl Feed for VecFeed {
    fn next_event(&mut self) -> Option<MarketEvent> {
        self.events.next()
    }
}

fn es_spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("SYN:ES"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    }
}

fn bar(i: i64, o: f64, h: f64, l: f64, c: f64) -> MarketEvent {
    MarketEvent::Bar(Bar {
        instrument: InstrumentId::new("SYN:ES"),
        tf: TimeFrame::M1,
        ts_open: Ts(i * 60_000_000_000),
        open: Price::from_points_f64_lossy(o),
        high: Price::from_points_f64_lossy(h),
        low: Price::from_points_f64_lossy(l),
        close: Price::from_points_f64_lossy(c),
        volume: 100,
    })
}

/// Bar 0 (the entry is placed here) and bar 1 (the entry fills at its 100.00
/// open, and the bar is quiet enough to touch neither level). The fixture then
/// appends whatever bar 2 it wants to exercise.
fn lead_in() -> Vec<MarketEvent> {
    vec![
        bar(0, 99.50, 100.00, 99.00, 99.75),
        bar(1, 100.00, 100.50, 99.50, 100.00),
    ]
}

fn quiet_tail(i: i64) -> MarketEvent {
    bar(i, 100.00, 100.25, 99.75, 100.00)
}

fn replay<S: Strategy, M: FillModel>(
    events: Vec<MarketEvent>,
    strategy: &mut S,
    fills: &mut M,
) -> BacktestResult {
    let params = BacktestParams {
        initial_cash_nano_usd: INITIAL_CASH,
        bars_per_year: 347_760.0,
    };
    run(
        &mut VecFeed {
            events: events.into_iter(),
        },
        strategy,
        fills,
        &es_spec(),
        &params,
    )
    .expect("well-ordered feed")
}

fn equity(result: &BacktestResult) -> Vec<NanoUsd> {
    result.equity.iter().map(|&(_, e)| e).collect()
}

/// **Fixture (a): one bar touches both the stop and the target.**
///
/// Long 1 filled at 100.00 on bar 1 ⇒ stop 98.00, target 103.00.
/// Bar 2 opens at 100.00 — between the levels, so the opening print settles
/// nothing — then trades down to 97.50 (through the stop) and up to 104.00
/// (through the target). The bar does not record which came first.
///
/// `stop_first_intrabar` fills the STOP, at its level, under `FreeFills`:
/// - realized = (98.00 − 100.00) × $50 × 1 = **−$100.00**
/// - cash = 100,000 − 100 = **$99,900.00**
///
/// A target-first convention would have paid (103.00 − 100.00) × $50 =
/// **+$150.00**, for $100,150.00 — a $250 swing on one bar, from a fact the
/// data does not contain. That is the whole reason the count below exists.
///
/// Equity, one point per bar: bar 0 nothing filled (100,000); bar 1 long at
/// 100.00 marked at its 100.00 close, unrealized 0 (100,000); bar 2 flat after
/// the stop (99,900); bar 3 flat (99,900).
#[test]
fn both_levels_touched_in_one_bar_fills_the_stop() {
    let mut events = lead_in();
    events.push(bar(2, 100.00, 104.00, 97.50, 103.50));
    events.push(quiet_tail(3));

    let result = replay(events, &mut EnterOnceBracketed::long(), &mut FreeFills);

    assert_eq!(
        equity(&result),
        vec![
            100_000_000_000_000,
            100_000_000_000_000,
            99_900_000_000_000,
            99_900_000_000_000,
        ]
    );
    // Not 100_150_000_000_000, which is the target-first reading.
    assert_eq!(result.summary.final_equity_nano_usd, 99_900_000_000_000);
    assert_eq!(result.n_fills, 2); // the entry, and the stop
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.summary.round_trips, 1);
    assert_eq!(result.summary.win_rate, Some(0.0));
    assert_eq!(result.summary.fees_nano_usd, 0); // FreeFills charges nothing
    assert_eq!(result.cancelled_at_eof, 0);

    // The flag: exactly one bar's outcome rested on the convention.
    assert_eq!(result.path_sensitive_bars, 1);
}

/// The same bar, mirrored for a short. Short 1 filled at 100.00 ⇒ stop 102.00,
/// target 97.00. Bar 2 opens at 100.00, trades up to 102.50 (through the stop)
/// and down to 96.50 (through the target).
///
/// The worst case inverts with the position: for a short, the stop is *above*.
/// - realized = (102.00 − 100.00) × $50 × (−1) = **−$100.00** ⇒ $99,900.00
/// - the target reading would have been (97.00 − 100.00) × $50 × (−1) =
///   **+$150.00** ⇒ $100,150.00
#[test]
fn both_levels_touched_fills_the_stop_for_a_short_too() {
    let mut events = lead_in();
    events.push(bar(2, 100.00, 102.50, 96.50, 97.00));
    events.push(quiet_tail(3));

    let result = replay(events, &mut EnterOnceBracketed::short(), &mut FreeFills);

    assert_eq!(result.summary.final_equity_nano_usd, 99_900_000_000_000);
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.path_sensitive_bars, 1);
}

/// **Fixture (b): the bar gaps through the stop.**
///
/// Long 1 at 100.00 ⇒ stop 98.00, target 103.00. Bar 2 **opens at 96.00**,
/// already two points below the stop, and then rallies to 104.00 — through the
/// target as well.
///
/// The opening print is the first trade of the bar, so the stop was reached
/// before anything else could happen, and it fills **at 96.00, not at 98.00**:
/// - realized = (96.00 − 100.00) × $50 × 1 = **−$200.00** ⇒ cash $99,800.00
/// - filling at the level would have said −$100.00 ⇒ $99,900.00, a price the
///   market never offered on this bar
///
/// And it is **not** path-sensitive even though both levels lie inside the
/// bar: the open settled the ordering, so nothing was guessed.
#[test]
fn a_gap_through_the_stop_fills_at_the_opening_print() {
    let mut events = lead_in();
    events.push(bar(2, 96.00, 104.00, 95.00, 103.00));
    events.push(quiet_tail(3));

    let result = replay(events, &mut EnterOnceBracketed::long(), &mut FreeFills);

    assert_eq!(
        equity(&result),
        vec![
            100_000_000_000_000,
            100_000_000_000_000,
            99_800_000_000_000,
            99_800_000_000_000,
        ]
    );
    // Not 99_900_000_000_000 (the level) and not 100_150_000_000_000 (the
    // target, which the bar also reached — later).
    assert_eq!(result.summary.final_equity_nano_usd, 99_800_000_000_000);
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.path_sensitive_bars, 0);
}

/// The gap rule is symmetric, and this is why rule 1 is not "the stop always
/// wins". Bar 2 **opens at 104.00**, above the 103.00 target, then falls to
/// 97.00 — through the stop.
///
/// A resting sell limit at 103.00 cannot have been passed unfilled, so the
/// target filled first, at the opening print:
/// - realized = (104.00 − 100.00) × $50 = **+$200.00** ⇒ cash $100,200.00
/// - filling at the target level would say +$150.00; awarding the stop would
///   say −$100.00, which requires the price to have travelled from 104.00 to
///   97.00 without touching a limit at 103.00
#[test]
fn a_gap_through_the_target_fills_at_the_opening_print() {
    let mut events = lead_in();
    events.push(bar(2, 104.00, 105.00, 97.00, 98.00));
    events.push(quiet_tail(3));

    let result = replay(events, &mut EnterOnceBracketed::long(), &mut FreeFills);

    assert_eq!(result.summary.final_equity_nano_usd, 100_200_000_000_000);
    assert_eq!(result.summary.win_rate, Some(1.0));
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.path_sensitive_bars, 0);
}

/// The path-sensitivity counter's negative control: the same fixture with a
/// bar 2 that touches **only** the target reports the same exit count and a
/// zero flag. A counter that fired on every bracketed exit would be
/// indistinguishable from one that worked, and useless.
///
/// Bar 2: open 100.00, high 104.00, low 99.00 — the 98.00 stop is untouched.
/// Target fills at 103.00 ⇒ (103.00 − 100.00) × $50 = **+$150.00** ⇒
/// $100,150.00.
#[test]
fn touching_one_level_only_is_not_path_sensitive() {
    let mut events = lead_in();
    events.push(bar(2, 100.00, 104.00, 99.00, 103.50));
    events.push(quiet_tail(3));

    let result = replay(events, &mut EnterOnceBracketed::long(), &mut FreeFills);

    assert_eq!(result.summary.final_equity_nano_usd, 100_150_000_000_000);
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.path_sensitive_bars, 0);
}

/// The other negative control: an identical run with **no bracket** never
/// reports a protective exit and never flags a bar, however violent bar 2 is.
/// A strategy that does not use stops cannot acquire path-sensitivity by
/// standing next to the feature.
#[test]
fn an_unbracketed_run_reports_no_protective_exits_at_all() {
    let mut events = lead_in();
    events.push(bar(2, 100.00, 104.00, 97.50, 103.50));
    events.push(quiet_tail(3));

    let mut strategy = EnterOnceBracketed {
        bar: 0,
        side: Side::Buy,
        bracket: None,
    };
    let result = replay(events, &mut strategy, &mut FreeFills);

    assert_eq!(result.n_fills, 1); // the entry, and nothing else
    assert_eq!(result.n_protective_exits, 0);
    assert_eq!(result.path_sensitive_bars, 0);
    assert_eq!(result.summary.round_trips, 0);
}

/// A bracket protects the bar its entry filled on.
///
/// The entry fills at bar 1's 100.00 open, so the stop rests from that instant
/// and bar 1's own low tests it. Here bar 1 dips to 97.00, and the position is
/// stopped out on the bar it was opened on: (98.00 − 100.00) × $50 =
/// **−$100.00** ⇒ $99,900.00, and one equity point per bar throughout.
///
/// Deferring the bracket to bar 2 would leave every entry naked for exactly one
/// bar — and the bar you entered on is the one a stop is for.
#[test]
fn the_bracket_is_live_on_the_bar_the_entry_filled_on() {
    let events = vec![
        bar(0, 99.50, 100.00, 99.00, 99.75),
        bar(1, 100.00, 100.50, 97.00, 99.00),
        quiet_tail(2),
    ];

    let result = replay(events, &mut EnterOnceBracketed::long(), &mut FreeFills);

    assert_eq!(
        equity(&result),
        vec![
            100_000_000_000_000,
            99_900_000_000_000, // entry and exit both happened on bar 1
            99_900_000_000_000,
        ]
    );
    assert_eq!(result.n_fills, 2);
    assert_eq!(result.n_protective_exits, 1);
}

/// The costs are leg-dependent, and this is the end-to-end proof under
/// `spread_cross` (1 tick half-spread, $1.25/contract/side).
///
/// The entry buys at bar 1's 100.00 open **plus** the half-spread: 100.25, fee
/// $1.25. Levels are measured from what it actually paid, not from 100.00:
/// stop 100.25 − 2.00 = **98.25**, target 100.25 + 3.00 = **103.25**.
///
/// Bar 2 rises to 104.00, through the target, and never reaches the stop. A
/// target is a resting limit the market came to, so it does **not** cross the
/// spread — it fills at 103.25 exactly, and pays only the commission:
/// - realized = (103.25 − 100.25) × $50 = **+$150.00**
/// - cash = 100,000 − 1.25 (entry fee) + 150 − 1.25 (exit fee) =
///   **$100,147.50**
/// - fees = **$2.50**
#[test]
fn spread_cross_prices_a_target_exit_at_the_level_plus_commission() {
    let mut events = lead_in();
    events.push(bar(2, 100.00, 104.00, 99.50, 103.75));
    events.push(quiet_tail(3));

    let mut fills = SpreadCrossFills::from_ticks(1, es_spec().tick).with_fee(1_250_000_000);
    let result = replay(events, &mut EnterOnceBracketed::long(), &mut fills);

    assert_eq!(result.summary.final_equity_nano_usd, 100_147_500_000_000);
    assert_eq!(result.summary.fees_nano_usd, 2_500_000_000);
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.path_sensitive_bars, 0);
}

/// The same entry, stopped out instead. A stop **is** a market order from the
/// instant it is touched, so it pays the half-spread on the way out:
///
/// - entry 100.25 (fee $1.25); stop 98.25
/// - bar 2's low of 98.00 reaches it, so it triggers and sells at
///   98.25 − 0.25 = **98.00**, fee $1.25
/// - realized = (98.00 − 100.25) × $50 = **−$112.50**
/// - cash = 100,000 − 1.25 − 112.50 − 1.25 = **$99,885.00**
///
/// Note the target at 103.25 is *not* reached by bar 2's 103.00 high — a print
/// at 103.00 is below the level, so this bar is unambiguous and the flag stays
/// zero even though the bar looks wide.
#[test]
fn spread_cross_prices_a_stop_exit_a_half_spread_worse_than_its_level() {
    let mut events = lead_in();
    events.push(bar(2, 100.00, 103.00, 98.00, 99.00));
    events.push(quiet_tail(3));

    let mut fills = SpreadCrossFills::from_ticks(1, es_spec().tick).with_fee(1_250_000_000);
    let result = replay(events, &mut EnterOnceBracketed::long(), &mut fills);

    assert_eq!(result.summary.final_equity_nano_usd, 99_885_000_000_000);
    assert_eq!(result.summary.fees_nano_usd, 2_500_000_000);
    assert_eq!(result.n_protective_exits, 1);
    assert_eq!(result.path_sensitive_bars, 0);
}

/// Determinism, for the path a bracket adds: the same series replayed twice is
/// bit-identical, counters included (CLAUDE.md §2.2).
#[test]
fn a_bracketed_run_is_bit_identical_on_replay() {
    let series = || {
        let mut events = lead_in();
        events.push(bar(2, 100.00, 104.00, 97.50, 103.50));
        events.push(bar(3, 103.50, 104.00, 96.00, 96.50));
        events
    };
    let a = replay(series(), &mut EnterOnceBracketed::long(), &mut FreeFills);
    let b = replay(series(), &mut EnterOnceBracketed::long(), &mut FreeFills);
    assert_eq!(a.equity, b.equity);
    assert_eq!(a.n_protective_exits, b.n_protective_exits);
    assert_eq!(a.path_sensitive_bars, b.path_sensitive_bars);
    assert_eq!(a.closed_trades, b.closed_trades);
}
