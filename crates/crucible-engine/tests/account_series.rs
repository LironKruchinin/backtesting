//! Planted controls for the account-evaluation capture
//! (`docs/ACCOUNT_EVAL_SPEC.md` §3, §5; D-0071).
//!
//! CLAUDE.md §7: a detector nobody has watched fire is decoration. Every test
//! here is one half of a pair — one that must fire and one that must not — and
//! each expected number is arithmetic done on paper in the comment above it.
//!
//! Two of the spec's controls live here:
//!
//! - **§5.12, the reconstruction ban.** An OHLC-rebuilt high-water series must
//!   *differ* from the captured one on a fixture with intrabar path ambiguity,
//!   and *agree* on a fixture whose bars are single-tick — which is what proves
//!   the difference is the ambiguity rather than an unrelated bug. Both halves
//!   are below, and the rebuild is parameterised by intrabar convention so the
//!   third assertion can be made: a rebuild that adopts the *engine's* stop
//!   ordering agrees with the capture on the ambiguous fixture too. The
//!   divergence is the convention, exactly, and nothing else.
//! - **§5.6, calendar slicing.** A loss taken at 18:30 CT on a Monday belongs
//!   to Tuesday. A wall-clock slicer books it on Monday, and the two are
//!   asserted to disagree.
//!
//! The 18:30 CT fixture is deliberately set in **July**. In January, 18:30 CST
//! is 00:30 UTC and a UTC slicer accidentally lands on the right trade date; in
//! July, 18:30 CDT is 23:30 UTC and it does not. A wall-clock slicer is
//! therefore wrong for part of the year and right for the rest, which is worse
//! than being wrong always — it is the shape of bug that survives a spot check.

use crucible_core::prelude::*;
use crucible_data::calendar::Calendar;
use crucible_data::ingest::window::{CivilDate, date_of, days_from_civil, start_of};
use crucible_engine::{
    AccountCapture, AccountSeries, BacktestParams, BacktestResult, FreeFills, HighWaterState, run,
    run_capturing,
};

const INITIAL_CASH: NanoUsd = 100_000_000_000_000; // $100,000
const DOLLAR: NanoUsd = 1_000_000_000;

// ---------------------------------------------------------------- fixtures

struct VecFeed {
    events: std::vec::IntoIter<MarketEvent>,
}

impl Feed for VecFeed {
    fn next_event(&mut self) -> Option<MarketEvent> {
        self.events.next()
    }
}

/// Places one order on bar 0 and then never trades again, so every exit is the
/// bracket's doing and nothing else's.
struct EnterOnce {
    placed: bool,
    bracket: Option<Bracket>,
}

impl Strategy for EnterOnce {
    fn warmup_bars(&self) -> usize {
        0
    }

    fn on_event(&mut self, _ev: &MarketEvent, _view: &PortfolioView, actions: &mut Actions) {
        if !self.placed {
            match self.bracket {
                Some(b) => actions.buy_bracketed(Qty(1), b),
                None => actions.buy(Qty(1)),
            }
            self.placed = true;
        }
    }
}

fn es_spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("SYN:ES"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    }
}

fn params() -> BacktestParams {
    BacktestParams {
        initial_cash_nano_usd: INITIAL_CASH,
        bars_per_year: 347_760.0,
    }
}

/// A 1-minute bar whose `avail_ts` is `ts_open + 60 s`.
fn bar_at(ts_open: Ts, o: f64, h: f64, l: f64, c: f64) -> MarketEvent {
    MarketEvent::Bar(Bar {
        instrument: InstrumentId::new("SYN:ES"),
        tf: TimeFrame::M1,
        ts_open,
        open: Price::from_points_f64_lossy(o),
        high: Price::from_points_f64_lossy(h),
        low: Price::from_points_f64_lossy(l),
        close: Price::from_points_f64_lossy(c),
        volume: 100,
        signal_offset: Price::ZERO,
    })
}

fn bar(i: i64, o: f64, h: f64, l: f64, c: f64) -> MarketEvent {
    bar_at(Ts(i * 60_000_000_000), o, h, l, c)
}

/// A bar that traded at exactly one price — the unambiguous control shape.
fn tick_bar(i: i64, p: f64) -> MarketEvent {
    bar(i, p, p, p, p)
}

fn capture_run(events: Vec<MarketEvent>, day_keys: &[i64], bracket: Option<Bracket>) -> Captured {
    let mut strategy = EnterOnce {
        placed: false,
        bracket,
    };
    let mut fills = FreeFills;
    let mut capture = AccountCapture::new(day_keys, INITIAL_CASH, TimeFrame::M1);
    let result = run_capturing(
        &mut VecFeed {
            events: events.into_iter(),
        },
        &mut strategy,
        &mut fills,
        &es_spec(),
        &params(),
        &mut capture,
    )
    .expect("well-ordered feed and matching keys");
    Captured {
        result,
        series: capture.finish(),
    }
}

struct Captured {
    result: BacktestResult,
    series: AccountSeries,
}

// ------------------------------------------------- §5.12 reconstruction ban

/// The two readings of a bar that touched both protective levels. The engine
/// has exactly one, `stop_first_intrabar` (D-0069); a *reconstruction* is free
/// to pick the other, and that is the whole problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intrabar {
    StopFirst,
    TargetFirst,
}

/// The rebuild a careless implementer writes: walk the OHLC series after the
/// run and infer what the equity path must have been.
///
/// Everything here matches the engine — same entry bar, same levels, same
/// marks at bar closes, same gap rule (a bar that opens beyond a level fills at
/// the opening print) — *except* the branch where one bar touched both levels.
/// That isolation is the point: whatever the two series disagree about is the
/// intrabar ordering and cannot be anything else.
///
/// Long-only, one contract, because that is the fixture.
fn rebuild_high_water(events: &[MarketEvent], ordering: Intrabar) -> HighWaterState {
    let spec = es_spec();
    // Entry fills at bar 1's open of 100.00 ⇒ 8 ticks (2.00) below is 98.00,
    // 12 ticks (3.00) above is 103.00.
    let stop = Price::from_points(98);
    let target = Price::from_points(103);

    let mut hw = HighWaterState::default();
    let mut position: i64 = 0;
    let mut entry = Price::ZERO;
    let mut cum: NanoUsd = 0;

    for (i, ev) in events.iter().enumerate() {
        let MarketEvent::Bar(b) = ev;
        // The entry order is placed on bar 0 and fills at bar 1's open.
        if i == 1 {
            position = 1;
            entry = b.open;
        }
        if position != 0 {
            let exit = if b.open <= stop || b.open > target {
                Some(b.open) // gapped: the opening print settled the ordering
            } else {
                match (b.low <= stop, b.high > target) {
                    (true, true) => Some(match ordering {
                        Intrabar::StopFirst => stop,
                        Intrabar::TargetFirst => target,
                    }),
                    (true, false) => Some(stop),
                    (false, true) => Some(target),
                    (false, false) => None,
                }
            };
            if let Some(px) = exit {
                cum += spec.pnl_nano_usd(px - entry, position);
                position = 0;
            }
        }
        let unrealized = if position == 0 {
            0
        } else {
            spec.pnl_nano_usd(b.close - entry, position)
        };
        hw.observe(cum + unrealized);
    }
    hw
}

/// **Fixture A — the path-ambiguous bar.**
///
/// Long 1 filled at 100.00 on bar 1 ⇒ stop 98.00, target 103.00. Bar 2 opens
/// at 100.00 (between the levels, so the opening print settles nothing), then
/// trades down to 97.50 — through the stop — and up to 104.00 — through the
/// target. The bar does not record which came first.
fn ambiguous_fixture() -> Vec<MarketEvent> {
    vec![
        bar(0, 99.50, 100.00, 99.00, 99.75),
        bar(1, 100.00, 100.50, 99.50, 100.00),
        bar(2, 100.00, 104.00, 97.50, 103.50),
        bar(3, 103.50, 103.75, 103.25, 103.50),
    ]
}

/// **Fixture B — single-tick bars.** Every bar traded at one price, so no bar
/// can touch both levels and the two conventions cannot differ. The stop still
/// fires, so "they agree" is a statement about a bracket that did something.
///
/// Prices 99.75, 100.00, 99.00, 98.00, 98.00.
fn single_tick_fixture() -> Vec<MarketEvent> {
    vec![
        tick_bar(0, 99.75),
        tick_bar(1, 100.00),
        tick_bar(2, 99.00),
        tick_bar(3, 98.00),
        tick_bar(4, 98.00),
    ]
}

fn bracket_8_12() -> Option<Bracket> {
    Some(Bracket::new(Some(8), Some(12)))
}

/// §5.12, the half that must fire.
///
/// Hand arithmetic on fixture A, one contract at $50/point, `FreeFills`:
///
/// | bar | position | cum (captured, stop-first) | cum (rebuilt, target-first) |
/// |---|---|---|---|
/// | 0 | flat | 0 | 0 |
/// | 1 | long @100, close 100 | 0 | 0 |
/// | 2 | exit | (98 − 100) × 50 = **−$100** | (103 − 100) × 50 = **+$150** |
/// | 3 | flat | −$100 | +$150 |
///
/// Captured: peak $0, max drop **$100**. Rebuilt under target-first: peak
/// **$150**, max drop **$0**. A $250 swing in the peak and the entire drawdown,
/// on one bar, from a fact the data does not contain — and a first-passage
/// metric is *made of* facts like that one. This is why §3.3 forbids the
/// rebuild rather than merely discouraging it.
#[test]
fn a_reconstruction_disagrees_with_the_capture_on_an_ambiguous_bar() {
    let captured = capture_run(ambiguous_fixture(), &[0; 4], bracket_8_12());
    let rebuilt = rebuild_high_water(&ambiguous_fixture(), Intrabar::TargetFirst);

    // The engine says the bar was ambiguous. If this is ever 0 the fixture has
    // stopped exercising the thing it exists for.
    assert_eq!(captured.result.path_sensitive_bars, 1);

    assert_eq!(
        captured.series.high_water,
        HighWaterState {
            peak_cum_nano_usd: 0,
            max_drop_from_peak_nano_usd: 100 * DOLLAR,
        }
    );
    assert_eq!(
        rebuilt,
        HighWaterState {
            peak_cum_nano_usd: 150 * DOLLAR,
            max_drop_from_peak_nano_usd: 0,
        }
    );
    assert_ne!(
        captured.series.high_water, rebuilt,
        "a reconstruction that re-opens the intrabar ordering measures a path \
         the account never took"
    );
}

/// §5.12, and the reason the control above is about the ambiguity rather than
/// about an unrelated bug in the rebuild: give the rebuild the **engine's**
/// convention and it reproduces the captured series exactly, on the very same
/// fixture.
#[test]
fn the_same_rebuild_agrees_once_it_adopts_the_engines_convention() {
    let captured = capture_run(ambiguous_fixture(), &[0; 4], bracket_8_12());
    let rebuilt = rebuild_high_water(&ambiguous_fixture(), Intrabar::StopFirst);
    assert_eq!(captured.series.high_water, rebuilt);
}

/// §5.12, the half that must NOT fire.
///
/// Single-tick bars: 99.75, 100.00, 99.00, 98.00, 98.00. The entry fills at
/// bar 1's open of 100.00; bar 3 prints exactly at the 98.00 stop, which is a
/// touch (a trade at the stop proves the market reached it — D-0069), and the
/// exit is 98.00 under either convention.
///
/// | bar | cum |
/// |---|---|
/// | 0 | 0 |
/// | 1 | 0 |
/// | 2 | (99 − 100) × 50 = −$50 unrealized |
/// | 3 | exit at 98 ⇒ −$100 |
/// | 4 | −$100 |
///
/// peak $0, max drop **$100**, and all three series agree — which is what makes
/// the divergence in the first control attributable to the ambiguity.
#[test]
fn a_reconstruction_agrees_with_the_capture_on_single_tick_bars() {
    let captured = capture_run(single_tick_fixture(), &[0; 5], bracket_8_12());
    let stop_first = rebuild_high_water(&single_tick_fixture(), Intrabar::StopFirst);
    let target_first = rebuild_high_water(&single_tick_fixture(), Intrabar::TargetFirst);

    assert_eq!(
        captured.result.path_sensitive_bars, 0,
        "a single-tick bar cannot be ambiguous"
    );
    assert_eq!(
        captured.result.n_protective_exits, 1,
        "the stop still fired"
    );
    assert_eq!(
        captured.series.high_water,
        HighWaterState {
            peak_cum_nano_usd: 0,
            max_drop_from_peak_nano_usd: 100 * DOLLAR,
        }
    );
    assert_eq!(captured.series.high_water, stop_first);
    assert_eq!(captured.series.high_water, target_first);
}

// ------------------------------------------------------ §5.6 calendar slicing

/// 2024-07-08 is a **Monday**, and 18:30 CDT that evening is 23:30 UTC the same
/// day. CME Globex equity index rolls the trade date at 17:00 CT, so the
/// exchange calls that instant **Tuesday 2024-07-09** while a UTC civil-day
/// slicer calls it Monday 2024-07-08.
fn utc(day: u32, hour: i64, minute: i64) -> Ts {
    let date = CivilDate {
        year: 2024,
        month: 7,
        day,
    };
    Ts(start_of(date).0 + (hour * 3_600 + minute * 60) * 1_000_000_000)
}

/// The trading-day keys the CLI computes: `days_from_civil(
/// Calendar::trading_day(avail_ts))`, one per bar. This is the single producer
/// the reconciliation depends on — see `crucible-engine::series`'s module docs.
fn calendar_keys(events: &[MarketEvent], cal: &Calendar) -> Vec<i64> {
    events
        .iter()
        .map(|ev| days_from_civil(cal.trading_day(ev.avail_ts())))
        .collect()
}

/// The wall-clock slicer this control exists to falsify: the UTC civil date of
/// the same instant.
fn wall_clock_keys(events: &[MarketEvent]) -> Vec<i64> {
    events
        .iter()
        .map(|ev| days_from_civil(date_of(ev.avail_ts())))
        .collect()
}

/// Four bars straddling the 17:00 CT roll on Monday 2024-07-08, all `avail_ts`
/// (a 1m bar's `avail_ts` is its `ts_open` plus one minute):
///
/// | bar | avail_ts (UTC) | CT wall clock | exchange trade date | UTC date |
/// |---|---|---|---|---|
/// | 0 | 07-08 21:00 | 16:00 Mon | 2024-07-08 | 07-08 |
/// | 1 | 07-08 22:00 | **17:00 Mon** — the roll | 2024-07-09 | 07-08 |
/// | 2 | 07-08 23:30 | **18:30 Mon** | 2024-07-09 | 07-08 |
/// | 3 | 07-09 01:00 | 20:00 Mon | 2024-07-09 | 07-09 |
///
/// The strategy buys on bar 0 and fills at bar 1's open. Prices are flat at
/// 100.00 until bar 2 closes at 90.00 — a 10-point mark against a long at
/// $50/point, so **−$500 at 18:30 CT**.
fn straddling_fixture() -> Vec<MarketEvent> {
    let m = 60_000_000_000;
    vec![
        bar_at(Ts(utc(8, 21, 0).0 - m), 100.0, 100.0, 100.0, 100.0),
        bar_at(Ts(utc(8, 22, 0).0 - m), 100.0, 100.0, 100.0, 100.0),
        bar_at(Ts(utc(8, 23, 30).0 - m), 100.0, 100.0, 90.0, 90.0),
        bar_at(Ts(utc(9, 1, 0).0 - m), 90.0, 90.0, 90.0, 90.0),
    ]
}

/// §5.6 — the two slicers **disagree**, and the calendar is the one that is
/// right.
///
/// The −$500 taken at 18:30 CT Monday belongs to Tuesday 2024-07-09. A UTC
/// slicer books it on Monday 2024-07-08, which mis-dates the daily PnL,
/// mis-orders the ratchet on every end-of-day account, and mis-attributes the
/// best day the consistency rules key on: three wrong numbers from one wrong
/// boundary.
#[test]
fn the_calendar_and_a_wall_clock_slicer_disagree_on_an_1830_ct_loss() {
    let cal = Calendar::for_instrument(&InstrumentId::new("ESU4"))
        .expect("bundled tables parse")
        .expect("a CME calendar governs ES");
    let events = straddling_fixture();

    let monday = days_from_civil(CivilDate {
        year: 2024,
        month: 7,
        day: 8,
    });
    let tuesday = monday + 1;

    let cal_keys = calendar_keys(&events, &cal);
    let utc_keys = wall_clock_keys(&events);
    assert_eq!(cal_keys, vec![monday, tuesday, tuesday, tuesday]);
    assert_eq!(utc_keys, vec![monday, monday, monday, tuesday]);
    assert_ne!(
        cal_keys, utc_keys,
        "if these ever agree the fixture has stopped straddling the roll"
    );

    let by_calendar = capture_run(events.clone(), &cal_keys, None).series;
    let by_wall_clock = capture_run(events, &utc_keys, None).series;

    // The calendar books the whole −$500 to Tuesday, and Monday closes flat.
    assert_eq!(by_calendar.days.len(), 2);
    assert_eq!(by_calendar.days[0].trading_day_key, monday);
    assert_eq!(by_calendar.days[0].close_pnl_nano_usd, 0);
    assert_eq!(by_calendar.days[1].trading_day_key, tuesday);
    assert_eq!(by_calendar.days[1].close_pnl_nano_usd, -500 * DOLLAR);
    assert_eq!(by_calendar.days[1].trough_from_open_nano_usd, -500 * DOLLAR);

    // The wall clock books it to Monday, and Tuesday closes flat. Same money,
    // different day — and a daily loss limit is a question about a day.
    assert_eq!(by_wall_clock.days.len(), 2);
    assert_eq!(by_wall_clock.days[0].close_pnl_nano_usd, -500 * DOLLAR);
    assert_eq!(by_wall_clock.days[1].close_pnl_nano_usd, 0);

    assert_ne!(
        by_calendar.days[0].close_pnl_nano_usd, by_wall_clock.days[0].close_pnl_nano_usd,
        "the two slicers must disagree, or this control is decoration"
    );
}

/// The other half of §5.6: the calendar's attribution matches the date derived
/// by hand, rather than merely differing from the UTC one.
///
/// 17:00 CDT on 2024-07-08 is 22:00 UTC. Every instant from there on is
/// Tuesday's session; 21:00 UTC (16:00 CDT) is still Monday's.
#[test]
fn the_calendar_puts_the_roll_at_1700_ct_to_the_minute() {
    let cal = Calendar::for_instrument(&InstrumentId::new("ESU4"))
        .expect("bundled tables parse")
        .expect("a CME calendar governs ES");
    let monday = CivilDate {
        year: 2024,
        month: 7,
        day: 8,
    };
    let tuesday = CivilDate {
        year: 2024,
        month: 7,
        day: 9,
    };

    // 21:59:59 UTC = 16:59:59 CDT — still Monday's trade date.
    assert_eq!(
        cal.trading_day(Ts(utc(8, 21, 59).0 + 59_000_000_000)),
        monday
    );
    // 22:00:00 UTC = 17:00:00 CDT — Tuesday's session opens.
    assert_eq!(cal.trading_day(utc(8, 22, 0)), tuesday);
    assert_eq!(cal.trading_day(utc(8, 23, 30)), tuesday);
}

// ---------------------------------------------------- capture is observation

/// Capture must not move a single number. It reads the marks the loop already
/// produced; if a hash ever moves because a capture was switched on, the
/// capture has stopped being an observation.
#[test]
fn capturing_changes_no_number_in_the_result() {
    let events = ambiguous_fixture();
    let plain = {
        let mut strategy = EnterOnce {
            placed: false,
            bracket: bracket_8_12(),
        };
        let mut fills = FreeFills;
        run(
            &mut VecFeed {
                events: events.clone().into_iter(),
            },
            &mut strategy,
            &mut fills,
            &es_spec(),
            &params(),
        )
        .expect("well-ordered feed")
    };
    let captured = capture_run(events, &[0; 4], bracket_8_12()).result;

    assert_eq!(plain.equity, captured.equity);
    assert_eq!(plain.n_fills, captured.n_fills);
    assert_eq!(plain.n_protective_exits, captured.n_protective_exits);
    assert_eq!(plain.path_sensitive_bars, captured.path_sensitive_bars);
    assert_eq!(plain.closed_trades, captured.closed_trades);
    assert_eq!(plain.fee_events, captured.fee_events);
    assert_eq!(plain.summary, captured.summary);
}

/// The day summaries have to add up to the run, or one of them is lying.
///
/// Over the whole run, the sum of every day's `close_pnl_nano_usd` is the
/// account's total PnL — because each day's close is measured from the
/// previous day's close and the first is measured from the anchor. This is the
/// telescoping identity the day-block bootstrap depends on.
#[test]
fn the_day_records_telescope_to_the_runs_total_pnl() {
    let events = straddling_fixture();
    let cal = Calendar::for_instrument(&InstrumentId::new("ESU4"))
        .expect("bundled tables parse")
        .expect("a CME calendar governs ES");
    let keys = calendar_keys(&events, &cal);
    let c = capture_run(events, &keys, None);

    let total: NanoUsd = c.series.days.iter().map(|d| d.close_pnl_nano_usd).sum();
    let final_equity = c.result.equity.last().expect("bars replayed").1;
    assert_eq!(total, final_equity - INITIAL_CASH);
    assert_eq!(total, -500 * DOLLAR);

    // And every bar is accounted for exactly once.
    let bars: usize = c
        .series
        .days
        .iter()
        .map(|d| d.bars.end - d.bars.start)
        .sum();
    assert_eq!(bars, c.result.equity.len());
    assert_eq!(c.series.marks, c.result.equity.len());
}

/// Keys that do not describe the replayed series are refused rather than
/// padded — the whole day grid after the mismatch would otherwise be silently
/// off by one.
#[test]
fn a_run_whose_keys_are_too_short_is_refused() {
    let mut strategy = EnterOnce {
        placed: false,
        bracket: None,
    };
    let mut fills = FreeFills;
    let short = [0i64, 0];
    let mut capture = AccountCapture::new(&short, INITIAL_CASH, TimeFrame::M1);
    let err = run_capturing(
        &mut VecFeed {
            events: straddling_fixture().into_iter(),
        },
        &mut strategy,
        &mut fills,
        &es_spec(),
        &params(),
        &mut capture,
    )
    .expect_err("four bars, two keys");
    assert!(
        format!("{err}").contains("ran out of trading-day keys"),
        "unexpected error: {err}"
    );
}
