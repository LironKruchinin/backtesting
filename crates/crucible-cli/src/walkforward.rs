//! `crucible walk-forward` — a grid, cut into train/test folds.
//!
//! Same config, same shared bar series and same fairness rules as `crucible
//! combo`. The difference is what the numbers are computed on: `combo`
//! reports one summary per combo over the whole replay, warmup prefix and
//! all; this command reports in-sample and out-of-sample statistics per fold,
//! each measured on its own window, and a headline that pools only the
//! out-of-sample ones. That is the caveat D-0061 left open, closed.
//!
//! What it deliberately does not do: pick a winner. The fold detail is
//! printed in **grid-index order**, never ranked by out-of-sample
//! performance, because a table sorted by the number you are about to quote
//! is a selection step wearing a report's clothes. Ranking, trial counting
//! and verdicts arrive with the funnel (M3).
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the grid replayed and every fold was reported |
//! | 2 | the config is wrong, the data does not exist, or the span is too short |
//! | 4 | data exists but could not be trusted |

use std::path::PathBuf;

use crucible_core::prelude::*;
use crucible_data::calendar::Calendar;
use crucible_data::ingest::window::{CivilDate, civil_from_days, date_of, days_from_civil};
use crucible_engine::{BacktestParams, FreeFills, SpreadCrossFills, Summary};
use crucible_funnel::walkforward::{FoldPlan, FoldSpec, RunIdentity, WalkForwardReport, run_grid};

use crate::combo::{annualization, collect_events, print_header};
use crate::config::{self, LoadedConfig};
use crate::pull::EXIT_USAGE;

/// Combos whose per-fold detail is printed in full.
///
/// The cut is by grid index, not by rank: printing "the best five" would make
/// the report a selection step, and selecting on out-of-sample results is the
/// thing the whole funnel exists to measure rather than to do casually.
const FOLD_DETAIL_COMBOS: usize = 8;

/// Arguments to `crucible walk-forward`.
#[derive(Debug, clap::Args)]
pub struct WalkForwardArgs {
    /// Path to the combo config. Must carry a `[walk_forward]` section.
    #[arg(long)]
    pub config: PathBuf,
    /// Print only the determinism hash of the whole report. This is the
    /// walk-forward CI gate.
    #[arg(long)]
    pub hash_only: bool,
}

/// Runs the command, returning the process exit code.
pub fn run_cmd(args: &WalkForwardArgs) -> i32 {
    let loaded = match config::load(&args.config) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    let Some(wf) = loaded.file.walk_forward.as_ref() else {
        eprintln!(
            "error: {} declares no [walk_forward] section, so there are no folds to cut.\n\
             \x20      Add one — scheme = \"rolling\" | \"anchored\", train_days, test_days, and\n\
             \x20      optionally step_days (it defaults to test_days). The units are TRADING\n\
             \x20      DAYS, not months: a month holds a number of sessions the exchange's\n\
             \x20      holiday schedule chooses, not the researcher.",
            loaded.path.display()
        );
        return EXIT_USAGE;
    };
    let fold_spec = match wf.to_fold_spec() {
        Ok(spec) => spec,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    let events = match collect_events(&loaded) {
        Ok(events) => events,
        Err((code, message)) => {
            eprintln!("error: {message}");
            return code;
        }
    };
    if events.is_empty() {
        eprintln!("error: the data source produced no bars; there is nothing to replay");
        return EXIT_USAGE;
    }

    let days = match trading_days(&loaded, &events) {
        Ok(days) => days,
        Err(message) => {
            eprintln!("error: {message}");
            return EXIT_USAGE;
        }
    };
    let plan = match FoldPlan::build(&days.keys, loaded.grid.max_warmup_bars(), fold_spec) {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("error: walk_forward: {e}");
            return EXIT_USAGE;
        }
    };

    let bars_per_year = annualization(&loaded, &events);
    let params = BacktestParams {
        initial_cash_nano_usd: loaded.initial_cash_nano_usd,
        bars_per_year,
    };
    let identity = RunIdentity {
        config_hash: loaded.config_hash,
        root_seed: loaded.file.run.seed,
    };
    let expect = "INVARIANT: a collected bar series is already availability-ordered, and the \
                  plan was built from it";
    let report = if loaded.file.execution.fill_model == "free_fills" {
        run_grid(
            &events,
            &loaded.grid,
            &plan,
            &loaded.spec,
            &params,
            &identity,
            &FreeFills,
        )
    } else {
        run_grid(
            &events,
            &loaded.grid,
            &plan,
            &loaded.spec,
            &params,
            &identity,
            &SpreadCrossFills {
                half_spread_ticks: loaded.file.execution.half_spread_ticks,
                fee_per_contract_nano_usd: loaded.fee_per_contract_nano_usd,
            },
        )
    }
    .expect(expect);

    if args.hash_only {
        println!("{:016x}", report_hash(&plan, &report));
        return 0;
    }

    print_header(&loaded, "walk-forward");
    print_plan(&plan, &days, events.len(), bars_per_year);
    print_oos_table(&report);
    print_fold_detail(&report, &plan);
    println!("  determinism hash {:016x}", report_hash(&plan, &report));
    print_footer(&loaded, &plan, &report);
    0
}

/// The trading-day key of every bar, and where the answer came from.
struct TradingDays {
    /// One nondecreasing key per bar.
    keys: Vec<i64>,
    /// Which calendar answered, or `None` for the UTC fallback.
    calendar_id: Option<String>,
    /// A caveat worth printing, if any.
    caveat: Option<String>,
}

impl TradingDays {
    /// The civil date of a key, for report labels.
    fn label(key: i64) -> CivilDate {
        civil_from_days(key)
    }
}

/// Assigns each bar to a trading day.
///
/// Keyed on **`avail_ts`, never `ts_open`** (CLAUDE.md §2.1): a fold boundary
/// decides which bars are ordered into which window, and ordering decisions
/// use availability time. For a 1m bar the two land on the same session
/// anyway; for coarser bars they need not, and the availability answer is the
/// one that cannot see the future.
fn trading_days(loaded: &LoadedConfig, events: &[MarketEvent]) -> Result<TradingDays, String> {
    let calendar = Calendar::for_instrument(&loaded.spec.instrument)
        .map_err(|e| format!("bundled calendar tables are broken: {e}"))?;

    match calendar {
        Some(cal) => {
            let keys: Vec<i64> = events
                .iter()
                .map(|ev| days_from_civil(cal.trading_day(ev.avail_ts())))
                .collect();
            // A session template describes one era of an exchange. Cutting
            // 2013 folds on 2015-onward session boundaries attributes bars to
            // the wrong trade date (D-0039).
            let caveat = events.first().and_then(|ev| {
                let first = date_of(ev.avail_ts());
                (days_from_civil(first) < days_from_civil(cal.valid_from())).then(|| {
                    format!(
                        "these bars start {first}, before calendar {} describes the exchange \
                         ({}). Session hours changed, so some bars are attributed to the \
                         wrong trading day and the fold boundaries below sit on the wrong \
                         bars",
                        cal.id(),
                        cal.valid_from()
                    )
                })
            });
            Ok(TradingDays {
                keys,
                calendar_id: Some(cal.id().to_owned()),
                caveat,
            })
        }
        None => Ok(TradingDays {
            keys: events
                .iter()
                .map(|ev| days_from_civil(date_of(ev.avail_ts())))
                .collect(),
            calendar_id: None,
            caveat: Some(format!(
                "no bundled calendar claims {}, so folds are cut on UTC civil days. For a \
                 synthetic feed that is exactly right — it has no exchange. For a real \
                 instrument it means a session that spans midnight UTC is split across two \
                 folds' worth of days",
                loaded.spec.instrument
            )),
        }),
    }
}

fn print_plan(plan: &FoldPlan, days: &TradingDays, n_bars: usize, bars_per_year: f64) {
    let spec: &FoldSpec = plan.spec();
    println!(
        "  folds          {} — train {} / test {} / step {} trading days (D-0062)",
        spec.scheme, spec.train_days, spec.test_days, spec.step_days
    );
    match &days.calendar_id {
        Some(id) => println!("  trading days   {id}"),
        None => println!(
            "  trading days   UTC civil days — no bundled calendar governs this instrument"
        ),
    }
    println!(
        "  replay         {n_bars} bars, one series shared by every combo; {} warmup, {} in a\n\
         \x20                partial first session, {} evaluable trading day(s)",
        plan.warmup_bars(),
        plan.partial_day_bars(),
        plan.days().len()
    );
    println!(
        "  out of sample  {} bars across {} fold(s), covering every session from {} to {}",
        plan.oos_bars(),
        plan.folds().len(),
        first_test_date(plan),
        last_test_date(plan)
    );
    if plan.unused_tail_days() > 0 {
        println!(
            "  unused tail    {} trading day(s) after the last fold — step_days does not divide\n\
             \x20                the remainder, so no fold reaches them",
            plan.unused_tail_days()
        );
    }
    println!("  annualization  {bars_per_year:.0} bars/yr");
    if let Some(caveat) = &days.caveat {
        println!("\n  NOTE: {caveat}.");
    }
    println!();

    println!(
        "  {:>4}  {:>26} {:>8}  {:>26} {:>8}",
        "fold", "train window", "bars", "test window (OOS)", "bars"
    );
    for fold in plan.folds() {
        println!(
            "  {:>4}  {:>26} {:>8}  {:>26} {:>8}",
            fold.index,
            span(plan, fold.train.days.start, fold.train.days.end),
            fold.train.n_bars(),
            span(plan, fold.test.days.start, fold.test.days.end),
            fold.test.n_bars(),
        );
    }
    println!();
}

/// `2024-01-03..2024-03-28` — a half-open day range, rendered closed, because
/// a reader comparing against a calendar wants the last session that is in
/// the window rather than the first that is not.
fn span(plan: &FoldPlan, from: usize, to: usize) -> String {
    let first = TradingDays::label(plan.days()[from]);
    let last = TradingDays::label(plan.days()[to - 1]);
    format!("{first}..{last}")
}

fn first_test_date(plan: &FoldPlan) -> CivilDate {
    let d = plan.folds().first().map_or(0, |f| f.test.days.start);
    TradingDays::label(plan.days()[d])
}

fn last_test_date(plan: &FoldPlan) -> CivilDate {
    let d = plan.folds().last().map_or(1, |f| f.test.days.end);
    TradingDays::label(plan.days()[d - 1])
}

/// The headline: out-of-sample only, one row per combo, in grid-index order.
fn print_oos_table(report: &WalkForwardReport) {
    println!("  OUT OF SAMPLE — every test window pooled, training windows excluded");
    println!(
        "  {:>5}  {:<34} {:>10} {:>9} {:>7} {:>10} {:>7} {:>11}",
        "combo", "parameters", "OOS return", "max DD", "trades", "Sharpe", "win%", "whole-run"
    );
    for c in &report.combos {
        let s = &c.oos_pooled;
        println!(
            "  {:>5}  {:<34} {:>9.2}% {:>8.2}% {:>7} {} {} {:>10.2}%",
            c.id.combo_index,
            c.label,
            s.total_return_pct,
            s.max_drawdown_pct,
            s.round_trips,
            sharpe(s),
            win(s),
            c.whole_run.total_return_pct,
        );
    }
    println!(
        "\n  `whole-run` is the same combo measured the way `crucible combo` measures it —\n\
         \x20 the whole series, warmup prefix included, training windows included. The gap\n\
         \x20 between the two columns is what a fold table is for."
    );
    println!();
}

fn print_fold_detail(report: &WalkForwardReport, plan: &FoldPlan) {
    let shown = report.combos.len().min(FOLD_DETAIL_COMBOS);
    println!("  PER FOLD — in-sample (IS) beside out-of-sample (OOS)");
    for c in report.combos.iter().take(shown) {
        println!("\n  combo {} — {}", c.id.combo_index, c.label);
        println!(
            "  {:>4}  {:>22} {:>9} {:>10}  {:>22} {:>10} {:>10} {:>7}  {:>16}",
            "fold",
            "IS window",
            "IS return",
            "IS Sharpe",
            "OOS window",
            "OOS return",
            "OOS Sharpe",
            "trades",
            "seed"
        );
        for (fold, r) in plan.folds().iter().zip(&c.folds) {
            println!(
                "  {:>4}  {:>22} {:>8.2}% {} {:>22} {:>9.2}% {} {:>7}  {:016x}",
                r.fold_index,
                span(plan, fold.train.days.start, fold.train.days.end),
                r.is.total_return_pct,
                sharpe(&r.is),
                span(plan, fold.test.days.start, fold.test.days.end),
                r.oos.total_return_pct,
                sharpe(&r.oos),
                r.oos.round_trips,
                r.seed,
            );
        }
    }
    if report.combos.len() > shown {
        println!(
            "\n  {} further combo(s) replayed and pooled into the table above, but their fold\n\
             \x20 detail is not printed. The cut is by grid index, not by out-of-sample rank:\n\
             \x20 a report that showed you its best folds would be doing the selection this\n\
             \x20 command exists to keep honest.",
            report.combos.len() - shown
        );
    }
    println!();
}

fn sharpe(s: &Summary) -> String {
    s.sharpe_naive
        .map_or_else(|| "       n/a".to_owned(), |x| format!("{x:10.2}"))
}

fn win(s: &Summary) -> String {
    s.win_rate
        .map_or_else(|| "    n/a".to_owned(), |w| format!("{:6.1}%", w * 100.0))
}

/// FNV-1a over the fold layout and every combo's out-of-sample numbers.
///
/// The layout is hashed too, and first: a change that moves a fold boundary
/// while leaving each window's arithmetic intact is exactly the kind of
/// regression a determinism gate that only hashed equity would sleep through.
/// Floats are hashed by their bit pattern, because §2.2's claim is
/// bit-identity, not approximate agreement.
fn report_hash(plan: &FoldPlan, report: &WalkForwardReport) -> u64 {
    let mut h = crate::Fnv64::new();
    let i64_of = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    h.write_i64(i64_of(plan.warmup_bars()));
    h.write_i64(i64_of(plan.partial_day_bars()));
    for fold in plan.folds() {
        h.write_i64(i64_of(fold.index));
        h.write_i64(i64_of(fold.train.bars.start));
        h.write_i64(i64_of(fold.train.bars.end));
        h.write_i64(i64_of(fold.test.bars.start));
        h.write_i64(i64_of(fold.test.bars.end));
    }
    for c in &report.combos {
        h.write_i64(i64_of(c.id.combo_index));
        for r in &c.folds {
            h.write_i64(i64_of(r.fold_index));
            #[expect(
                clippy::cast_possible_wrap,
                reason = "hashing the seed's bits, not its value"
            )]
            h.write_i64(r.seed as i64);
            hash_summary(&mut h, &r.is);
            hash_summary(&mut h, &r.oos);
        }
        hash_summary(&mut h, &c.oos_pooled);
        hash_summary(&mut h, &c.is_pooled);
        hash_summary(&mut h, &c.whole_run);
    }
    h.finish()
}

fn hash_summary(h: &mut crate::Fnv64, s: &Summary) {
    h.write_i64(s.initial_equity_nano_usd);
    h.write_i64(s.final_equity_nano_usd);
    h.write_i64(s.fees_nano_usd);
    h.write_i64(i64::try_from(s.round_trips).unwrap_or(i64::MAX));
    #[expect(
        clippy::cast_possible_wrap,
        reason = "hashing float bit patterns, not their values — §2.2 claims bit-identity"
    )]
    for f in [
        Some(s.total_return_pct),
        Some(s.max_drawdown_pct),
        s.win_rate,
        s.sharpe_naive,
    ] {
        // `None` is a distinct outcome (no trades, or no variance), not a
        // zero, and must not hash like one.
        h.write_i64(f.map_or(i64::MIN, |x| x.to_bits() as i64));
    }
}

fn print_footer(loaded: &LoadedConfig, plan: &FoldPlan, report: &WalkForwardReport) {
    let suppressed: usize = report.combos.iter().map(|c| c.suppressed_intents).sum();
    let conflicts: usize = report.combos.iter().map(|c| c.conflicting_signals).sum();
    let cancelled: usize = report.combos.iter().map(|c| c.cancelled_at_eof).sum();

    println!(
        "  {suppressed} order(s) across the grid were dropped because a combo was warm before\n\
         \x20 the grid was ({} bars, §2.6). Every combo's folds are cut at the same bars.",
        plan.warmup_bars()
    );
    if conflicts > 0 {
        println!(
            "\n  WARNING: enter_long and enter_short were both true on {conflicts} bar(s) with a\n\
             \x20          flat position. No position was taken on any of them. Two entry rules\n\
             \x20          that can fire together is a bug in the config, not a signal."
        );
    }
    if cancelled > 0 {
        println!(
            "\n  {cancelled} order(s) were still pending when the series ended, and were cancelled."
        );
    }
    println!(
        "\n  fill model     {} — every number above states it (§2.4)",
        loaded.file.execution.fill_model
    );
    println!("\n  not consumed by `walk-forward`:");
    let unconsumed = loaded.unconsumed_sections(true);
    if unconsumed.is_empty() {
        println!("    nothing — every section this config declares was read");
    }
    for section in unconsumed {
        println!("    {section}");
    }
    println!(
        "    ([run].seed IS read: every (combo, fold) above carries a seed derived from it\n\
         \x20    (D-0064), though nothing in this build draws a random number yet)"
    );
    println!(
        "\n  Each number above was computed on the window it names: the grid's warmup and\n\
         \x20 every training window are excluded from every OOS figure, so D-0061's\n\
         \x20 sqrt(n_eval/n_total) factor does not apply here. `crucible combo` still\n\
         \x20 carries it, because it still reports one window and that window contains the\n\
         \x20 warmup."
    );
    println!(
        "\n  This is EVIDENCE, NOT A VERDICT. There is no cost-sensitivity sweep, no trial\n\
         \x20 count, no deflated Sharpe, no PBO and no permutation null here — an\n\
         \x20 out-of-sample Sharpe read off a fold table without them is the most\n\
         \x20 confidently wrong number this project can produce. Those arrive with the\n\
         \x20 funnel (M3), which consumes exactly what this command computes."
    );
}
