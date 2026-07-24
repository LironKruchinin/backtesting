//! `crucible` — CLI entry point.
//!
//! v0 ships one real command, `demo`, which runs the reference strategy on a
//! seeded random walk under two fill models. It exists to prove the vertical
//! slice end-to-end, to show the cost-of-costs lesson in one screen, and to
//! give CI a determinism hash (`demo --hash-only`).

use crucible_core::prelude::*;
use crucible_data::SyntheticFeed;
use crucible_engine::{BacktestParams, BacktestResult, FreeFills, SpreadCrossFills, run};
use crucible_strategies::SmaCross;

/// 1-minute bars per year on a ~23h/252-day futures session.
const BARS_PER_YEAR_1M: f64 = 347_760.0;
const DEMO_SEED: u64 = 42;
const DEMO_BARS: usize = 100_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("demo") => {
            let hash_only = args.iter().any(|a| a == "--hash-only");
            demo(hash_only);
        }
        Some("help") | None => help(),
        Some(other) => {
            eprintln!("unknown command: {other}\n");
            help();
            std::process::exit(2);
        }
    }
}

fn help() {
    println!(
        "crucible — a backtesting engine designed to reject strategies\n\n\
         USAGE:\n  crucible <command>\n\n\
         COMMANDS:\n\
         \x20 demo [--hash-only]   run the reference SMA-cross demo on synthetic data\n\
         \x20 help                 show this message\n\n\
         PLANNED (see docs/MILESTONES.md):\n\
         \x20 pull        M1  batch-download entitled Databento windows into the archive\n\
         \x20 transcode   M1  DBN -> curated Parquet\n\
         \x20 screen      M3  stage 0-1 signal triage / coarse grid\n\
         \x20 funnel      M3  full staged evaluation of a config\n\
         \x20 report      M3  render verdict scorecards"
    );
}

fn es_like_spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("SYN:RW"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    }
}

fn one_run<M: FillModel>(fill_model: &mut M) -> BacktestResult {
    let spec = es_like_spec();
    let mut feed = SyntheticFeed::random_walk(
        DEMO_SEED,
        DEMO_BARS,
        TimeFrame::M1,
        Price::from_points(5000),
        Price::from_points_f64_lossy(0.25),
        4,
    );
    let mut strategy = SmaCross::new(20, 50, Qty(1));
    let params = BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000, // $100k
        bars_per_year: BARS_PER_YEAR_1M,
    };
    run(&mut feed, &mut strategy, fill_model, &spec, &params)
        .expect("INVARIANT: SyntheticFeed yields ordered events")
}

fn demo(hash_only: bool) {
    let free = one_run(&mut FreeFills);
    let costed = one_run(&mut SpreadCrossFills {
        half_spread_ticks: 1,
        fee_per_contract_nano_usd: 1_250_000_000, // $1.25/contract/side
    });

    if hash_only {
        let mut h = Fnv64::new();
        for r in [&free, &costed] {
            for &(ts, eq) in &r.equity {
                h.write_i64(ts.0);
                h.write_i64(eq);
            }
        }
        println!("{:016x}", h.finish());
        return;
    }

    println!("Crucible demo — SMA(20/50) cross, 1 contract");
    println!(
        "{DEMO_BARS} synthetic 1m bars (seeded random walk, seed {DEMO_SEED}); \
         ES-like spec: 0.25 tick, $50/pt\n"
    );
    println!(
        "  {:<22} {:>14} {:>9} {:>8} {:>7} {:>7} {:>8}",
        "fill model", "final equity", "return", "max DD", "trades", "win%", "Sharpe"
    );
    print_row("free_fills (S0-S1)", &free);
    print_row("spread_cross 1t+fees", &costed);

    let gap = free.summary.final_equity_nano_usd - costed.summary.final_equity_nano_usd;
    println!("\n  Cost of pretending execution is free: {}", usd(gap));
    println!(
        "\n  Reading: the data is a random walk — there is NO edge to find. Whatever\n\
         \x20 free_fills shows is luck; spread_cross shows the same trades after paying\n\
         \x20 the half-spread and fees. Any engine change that makes this demo look\n\
         \x20 profitable under costs has introduced a bug, not an edge."
    );
}

fn print_row(name: &str, r: &BacktestResult) {
    let s = &r.summary;
    let win = s
        .win_rate
        .map_or_else(|| "  n/a".to_owned(), |w| format!("{:5.1}", w * 100.0));
    let sharpe = s
        .sharpe_naive
        .map_or_else(|| "   n/a".to_owned(), |x| format!("{x:6.2}"));
    println!(
        "  {:<22} {:>14} {:>8.2}% {:>7.2}% {:>7} {:>7} {:>8}",
        name,
        usd(s.final_equity_nano_usd),
        s.total_return_pct,
        s.max_drawdown_pct,
        s.round_trips,
        win,
        sharpe
    );
}

fn usd(n: NanoUsd) -> String {
    format!("${:.2}", nano_usd_to_f64(n))
}

/// FNV-1a 64-bit, hand-rolled: stable across Rust versions (unlike
/// `DefaultHasher`), which is exactly what a CI determinism hash needs.
struct Fnv64 {
    state: u64,
}

impl Fnv64 {
    const fn new() -> Self {
        Fnv64 {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }
    fn write_i64(&mut self, v: i64) {
        for b in v.to_le_bytes() {
            self.state ^= u64::from(b);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    const fn finish(&self) -> u64 {
        self.state
    }
}
