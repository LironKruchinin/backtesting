//! Determinism gate: identical inputs ⇒ bit-identical outputs. This is a
//! merge-blocking property (CLAUDE.md §2.2) — if this test fails, something
//! nondeterministic (clock, map iteration order, unseeded randomness,
//! thread-order dependence) leaked into the engine.

use crucible_core::prelude::*;
use crucible_data::SyntheticFeed;
use crucible_engine::{BacktestParams, SpreadCrossFills, run};
use crucible_strategies::SmaCross;

fn one_run() -> crucible_engine::BacktestResult {
    let spec = ContractSpec {
        instrument: InstrumentId::new("SYN:RW"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    };
    let mut feed = SyntheticFeed::random_walk(
        42,
        20_000,
        TimeFrame::M1,
        Price::from_points(5000),
        Price::from_points_f64_lossy(0.25),
        4,
    );
    let mut strategy = SmaCross::new(20, 50, Qty(1));
    let mut fills = SpreadCrossFills::from_ticks(1, spec.tick).with_fee(1_250_000_000);
    let params = BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000,
        bars_per_year: 347_760.0,
    };
    run(&mut feed, &mut strategy, &mut fills, &spec, &params).expect("ordered feed")
}

#[test]
fn identical_inputs_bit_identical_outputs() {
    let a = one_run();
    let b = one_run();
    assert_eq!(a.equity, b.equity, "equity curves must match exactly");
    assert_eq!(a.n_fills, b.n_fills);
    assert_eq!(
        a.summary.final_equity_nano_usd,
        b.summary.final_equity_nano_usd
    );
    // Sanity: the run actually traded, so the test exercises fills.
    assert!(a.n_fills > 0, "expected at least one fill in 20k bars");
}
