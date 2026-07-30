//! Golden smoke test: a 4-bar scripted scenario with every number derived by
//! hand. If any engine change shifts these values, the change altered
//! execution semantics — that must be a conscious decision, not a side
//! effect. (Policy: never edit expected values to make a failure pass
//! without writing the arithmetic out by hand first — CLAUDE.md §7.)

use crucible_core::prelude::*;
use crucible_engine::{BacktestParams, SpreadCrossFills, run};

/// Buys 1 while handling bar 0, sells 1 while handling bar 2.
struct Scripted {
    bar_idx: usize,
}

impl Strategy for Scripted {
    fn warmup_bars(&self) -> usize {
        0
    }
    fn on_event(&mut self, _ev: &MarketEvent, _view: &PortfolioView, actions: &mut Actions) {
        match self.bar_idx {
            0 => actions.buy(Qty(1)),
            2 => actions.sell(Qty(1)),
            _ => {}
        }
        self.bar_idx += 1;
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
        signal_offset: Price::ZERO,
    })
}

/// Hand-derived expectations (ES-like: tick 0.25, $50/pt, half-spread 1 tick,
/// fee $1.25/contract/side, initial cash $100,000):
///
/// - Buy placed on bar0 → fills at bar1 open 101 + 0.25 = 101.25, fee $1.25.
///   cash = 99,998.75
/// - bar1 close 102: equity = 99,998.75 + (102 − 101.25)·50 = 100,036.25
/// - bar2 close 103: equity = 99,998.75 + (103 − 101.25)·50 = 100,086.25
/// - Sell placed on bar2 → fills at bar3 open 102 − 0.25 = 101.75:
///   realized (101.75 − 101.25)·50 = $25, fee $1.25 → cash 100,022.50, flat.
/// - Round trip net: 25 − 2.50 = $22.50. Win rate 100%. Fees $2.50 total.
#[test]
fn four_bar_golden_scenario() {
    let spec = ContractSpec {
        instrument: InstrumentId::new("SYN:ES"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    };
    let mut feed = VecFeed {
        events: vec![
            bar(0, 99.50, 100.50, 99.00, 100.00),
            bar(1, 101.00, 102.50, 100.50, 102.00),
            bar(2, 102.50, 103.50, 102.00, 103.00),
            bar(3, 102.00, 102.50, 101.00, 101.50),
        ]
        .into_iter(),
    };
    let mut strategy = Scripted { bar_idx: 0 };
    let mut fills = SpreadCrossFills {
        half_spread_ticks: 1,
        fee_per_contract_nano_usd: 1_250_000_000,
    };
    let params = BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000,
        bars_per_year: 347_760.0,
    };

    let result =
        run(&mut feed, &mut strategy, &mut fills, &spec, &params).expect("well-ordered feed");

    let expected_equity: Vec<i64> = vec![
        100_000_000_000_000, // bar0: nothing filled yet
        100_036_250_000_000, // bar1
        100_086_250_000_000, // bar2
        100_022_500_000_000, // bar3: flat again
    ];
    let got: Vec<i64> = result.equity.iter().map(|&(_, e)| e).collect();
    assert_eq!(got, expected_equity);

    assert_eq!(result.n_fills, 2);
    assert_eq!(result.cancelled_at_eof, 0);
    assert_eq!(result.summary.round_trips, 1);
    assert_eq!(result.summary.win_rate, Some(1.0));
    assert_eq!(result.summary.fees_nano_usd, 2_500_000_000);
    assert_eq!(result.summary.final_equity_nano_usd, 100_022_500_000_000);
}

/// An out-of-order feed must abort the run, not warn and continue.
#[test]
fn out_of_order_feed_is_fatal() {
    let spec = ContractSpec {
        instrument: InstrumentId::new("SYN:ES"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    };
    let mut feed = VecFeed {
        events: vec![
            bar(5, 100.0, 100.0, 100.0, 100.0),
            bar(1, 100.0, 100.0, 100.0, 100.0),
        ]
        .into_iter(),
    };
    let mut strategy = Scripted { bar_idx: 0 };
    let mut fills = SpreadCrossFills {
        half_spread_ticks: 1,
        fee_per_contract_nano_usd: 0,
    };
    let params = BacktestParams {
        initial_cash_nano_usd: 0,
        bars_per_year: 1.0,
    };
    let err = run(&mut feed, &mut strategy, &mut fills, &spec, &params)
        .expect_err("must reject out-of-order events");
    assert!(matches!(
        err,
        crucible_engine::EngineError::OutOfOrderFeed { .. }
    ));
}
