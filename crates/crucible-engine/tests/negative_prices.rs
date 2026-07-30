//! Half (b) of D-0070's planted fixture: a round-trip **through** a negative
//! price, with every number derived by hand.
//!
//! `crucible-data::transcode` stopped refusing negative prices because
//! `Price` is a signed i64 and `ContractSpec::pnl_nano_usd` is linear in it,
//! so accounting through zero is supposed to be the same arithmetic as
//! accounting anywhere else. "Supposed to be" is not a test. This file is the
//! test: the same scripted-scenario shape as `golden_smoke.rs`, on the CL
//! contract, over the 2020-04-20 session where WTI settled at −$37.63.
//!
//! Golden-value policy applies (CLAUDE.md §7, `testdata/README.md` rule 3):
//! these expectations were computed by hand and are never edited to make a
//! failure pass. A change that shifts them changed execution semantics.

use crucible_core::prelude::*;
use crucible_engine::{BacktestParams, SpreadCrossFills, run};

/// Sells 1 while handling bar 0, buys it back while handling bar 2.
struct ShortThroughZero {
    bar_idx: usize,
}

impl Strategy for ShortThroughZero {
    fn warmup_bars(&self) -> usize {
        0
    }
    fn on_event(&mut self, _ev: &MarketEvent, _view: &PortfolioView, actions: &mut Actions) {
        match self.bar_idx {
            0 => actions.sell(Qty(1)),
            2 => actions.buy(Qty(1)),
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

/// A price in points from decimal text, never through an `f64` — −37.63 is
/// not exact in binary, and this file's whole subject is exactness (D-0027).
fn p(text: &str) -> Price {
    Price::from_points_str(text).expect("a decimal price in points")
}

fn bar(i: i64, o: &str, h: &str, l: &str, c: &str) -> MarketEvent {
    MarketEvent::Bar(Bar {
        instrument: InstrumentId::new("CLK0"),
        tf: TimeFrame::M1,
        ts_open: Ts(i * 60_000_000_000),
        open: p(o),
        high: p(h),
        low: p(l),
        close: p(c),
        volume: 100,
    })
}

/// CL: tick 0.01 points, $1,000 per point per contract. A "point" is one
/// dollar per barrel and a contract is 1,000 barrels, so a one-point move is
/// $1,000 — which is why −37.63 was a $47,000 move from ten dollars.
fn cl_spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("CLK0"),
        tick: p("0.01"),
        point_value_usd: 1_000,
    }
}

/// Hand-derived expectations. CL: tick 0.01, $1,000/pt, half-spread 1 tick
/// (= 0.01 points), fee $1.50/contract/side, initial cash $100,000.
///
/// Bars, in points (dollars per barrel):
///
/// | bar | open | high | low | close |
/// |---|---|---|---|---|
/// | 0 | 11.00 | 11.00 | 10.98 | 10.99 |
/// | 1 | 10.00 | 10.20 | −1.00 | −0.50 |
/// | 2 | −0.50 | 1.00 | −40.32 | −37.63 |
/// | 3 | −37.00 | −30.00 | −38.00 | −36.00 |
///
/// - **bar0.** Sell placed. Nothing has filled, position 0, no mark to apply:
///   equity = 100,000.00
/// - **bar1.** The sell fills at `open − half_spread` = 10.00 − 0.01 = **9.99**
///   (already tick-aligned). Fee $1.50 → cash = 100,000.00 − 1.50 =
///   **99,998.50**. Position −1, average entry 9.99.
///   Mark at close −0.50: unrealized = (−0.50 − 9.99) × 1,000 × (−1)
///   = (−10.49) × (−1,000) = **+10,490.00**.
///   equity = 99,998.50 + 10,490.00 = **110,488.50**
/// - **bar2.** Nothing fills; buy placed. Mark at close −37.63:
///   unrealized = (−37.63 − 9.99) × 1,000 × (−1) = (−47.62) × (−1,000)
///   = **+47,620.00**. equity = 99,998.50 + 47,620.00 = **147,618.50**
/// - **bar3.** The buy fills at `open + half_spread` = −37.00 + 0.01 =
///   **−36.99**. Realized on the closing leg = (−36.99 − 9.99) × 1,000 × (−1)
///   = (−46.98) × (−1,000) = **+46,980.00**. Fee $1.50.
///   cash = 99,998.50 − 1.50 + 46,980.00 = **146,977.00**, position flat, so
///   equity = cash = **146,977.00**
/// - Round-trip net = 46,980.00 − 3.00 (two fees) = **+46,977.00**. One
///   round-trip, win rate 100 %, fees $3.00.
///
/// The point of the arithmetic above is that *nothing about it is special*.
/// Every step is the same `delta × point_value × signed_qty` the ES golden
/// uses; the sign of the price never enters, only the sign of the difference.
#[test]
fn a_round_trip_through_a_negative_price_settles_the_hand_computed_pnl() {
    let mut feed = VecFeed {
        events: vec![
            bar(0, "11.00", "11.00", "10.98", "10.99"),
            bar(1, "10.00", "10.20", "-1.00", "-0.50"),
            bar(2, "-0.50", "1.00", "-40.32", "-37.63"),
            bar(3, "-37.00", "-30.00", "-38.00", "-36.00"),
        ]
        .into_iter(),
    };
    let mut strategy = ShortThroughZero { bar_idx: 0 };
    let mut fills = SpreadCrossFills::from_ticks(1, cl_spec().tick).with_fee(1_500_000_000); // $1.50
    let params = BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000, // $100,000
        bars_per_year: 347_760.0,
    };

    let result =
        run(&mut feed, &mut strategy, &mut fills, &cl_spec(), &params).expect("well-ordered feed");

    let expected_equity: Vec<i64> = vec![
        100_000_000_000_000, // bar0: nothing filled yet
        110_488_500_000_000, // bar1: short from 9.99, marked at −0.50
        147_618_500_000_000, // bar2: marked at the −37.63 settle
        146_977_000_000_000, // bar3: covered at −36.99, flat
    ];
    let got: Vec<i64> = result.equity.iter().map(|&(_, e)| e).collect();
    assert_eq!(got, expected_equity);

    assert_eq!(result.n_fills, 2);
    assert_eq!(result.cancelled_at_eof, 0);
    assert_eq!(result.summary.round_trips, 1);
    assert_eq!(result.summary.win_rate, Some(1.0));
    assert_eq!(result.summary.fees_nano_usd, 3_000_000_000);
    assert_eq!(result.summary.final_equity_nano_usd, 146_977_000_000_000);
}

/// The mirror image, and the reason the sign of the *price* must not matter:
/// the trader on the other side of the trade above lost exactly what this one
/// made, less the same fees.
///
/// Long 1 filled at 10.00 + 0.01 = **10.01**, covered at −37.00 − 0.01 =
/// **−37.01**. Realized = (−37.01 − 10.01) × 1,000 × (+1) = (−47.02) × 1,000 =
/// **−47,020.00**. Two fees at $1.50 → cash = 100,000.00 − 3.00 − 47,020.00 =
/// **52,977.00**. Round-trip net −47,023.00, win rate 0 %.
#[test]
fn the_losing_side_of_the_same_trade_is_the_exact_mirror() {
    struct LongThroughZero {
        bar_idx: usize,
    }
    impl Strategy for LongThroughZero {
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

    let mut feed = VecFeed {
        events: vec![
            bar(0, "11.00", "11.00", "10.98", "10.99"),
            bar(1, "10.00", "10.20", "-1.00", "-0.50"),
            bar(2, "-0.50", "1.00", "-40.32", "-37.63"),
            bar(3, "-37.00", "-30.00", "-38.00", "-36.00"),
        ]
        .into_iter(),
    };
    let mut strategy = LongThroughZero { bar_idx: 0 };
    let mut fills = SpreadCrossFills::from_ticks(1, cl_spec().tick).with_fee(1_500_000_000);
    let params = BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000,
        bars_per_year: 347_760.0,
    };

    let result =
        run(&mut feed, &mut strategy, &mut fills, &cl_spec(), &params).expect("well-ordered feed");

    assert_eq!(result.summary.final_equity_nano_usd, 52_977_000_000_000);
    assert_eq!(result.summary.round_trips, 1);
    assert_eq!(result.summary.win_rate, Some(0.0));

    // The two sides differ by exactly the four fees and the two half-spreads
    // they each paid: 146,977.00 + 52,977.00 = 199,954.00, which is
    // 200,000.00 − 6.00 in fees − 40.00 in crossed spread
    // (0.01 × 1,000 × 4 legs). A negative price is not a special case; it is
    // an ordinary i64 that happens to be below zero.
    assert_eq!(
        146_977_000_000_000_i64 + 52_977_000_000_000_i64,
        200_000_000_000_000_i64 - 6_000_000_000 - 40_000_000_000
    );
}

/// The one place a sign could have been dropped: rounding a negative fill
/// price onto the tick grid. `round_to_tick` rounds half **away from zero**,
/// symmetrically, so −37.635 goes to −37.64 exactly as 37.635 goes to 37.64.
/// Half-up rounding (`(x + half) / tick`) would have pulled the negative side
/// toward zero and quietly flattered every short.
#[test]
fn tick_rounding_is_symmetric_about_zero() {
    let tick = p("0.01");
    assert_eq!(p("-37.635").round_to_tick(tick), p("-37.64"));
    assert_eq!(p("37.635").round_to_tick(tick), p("37.64"));
    assert_eq!(p("-37.634").round_to_tick(tick), p("-37.63"));
    assert_eq!(p("-0.004").round_to_tick(tick), Price::ZERO);
    assert_eq!(p("0").round_to_tick(tick), Price::ZERO);
}
