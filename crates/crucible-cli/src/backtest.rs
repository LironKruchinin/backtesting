//! `crucible backtest` — the reference strategy on real archived bars.
//!
//! This is the M1 exit artifact, not a research tool. It runs exactly one
//! strategy (`SmaCross`) under exactly one fill model (`spread_cross`) over
//! one instrument, and prints what happened. Grids, folds, walk-forward,
//! trial counting, and scorecards are the funnel's job (M2/M3) and are
//! deliberately absent here — a thin command that tells the truth is worth
//! more at this milestone than a thick one that invites p-hacking.
//!
//! Everything the result depends on is printed with it. The cost flags exist
//! for that reason rather than for configurability: CLAUDE.md §2.4 says
//! execution assumptions are named, never implied, so the half-spread and the
//! fee are arguments with today's hand-set values as defaults, echoed back in
//! the header. Their calibration from archived L1 is M4's job.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the backtest ran |
//! | 2 | usage error, or the curated data asked for does not exist |
//! | 4 | curated data exists but could not be trusted |

use crucible_core::prelude::*;
use crucible_data::calendar::Calendar;
use crucible_data::catalog::TsRange;
use crucible_data::curated::{
    CURATED_SCHEMA_VERSION, CuratedError, ParquetBarFeed, TRANSCODER_VERSION,
};
use crucible_data::ingest::money::parse_usd_to_nano;
use crucible_data::ingest::range_from_dates;
use crucible_data::ingest::window::{date_of, days_from_civil, parse_civil_date, start_of};
use crucible_engine::{BacktestParams, BacktestResult, SpreadCrossFills, run as run_backtest};
use crucible_strategies::SmaCross;

use crate::pull::{EXIT_FAILED, EXIT_USAGE, data_dir};

/// Nanoseconds in a mean Gregorian year (365.2425 days).
const NANOS_PER_YEAR: f64 = 365.2425 * 86_400.0 * 1e9;

/// Arguments to `crucible backtest`.
#[derive(Debug, clap::Args)]
pub struct BacktestArgs {
    /// Instrument to replay, e.g. ESH4. Must already be transcoded.
    #[arg(long)]
    pub instrument: String,
    /// Bar interval: 1s 1m 5m 15m 1h 1d.
    #[arg(long, default_value = "1m")]
    pub timeframe: String,
    /// Inclusive UTC start date, YYYY-MM-DD. Omit both dates to replay
    /// everything held for this instrument.
    #[arg(long)]
    pub start: Option<String>,
    /// Exclusive UTC end date, YYYY-MM-DD.
    #[arg(long)]
    pub end: Option<String>,
    /// Fast SMA period, in bars.
    #[arg(long, default_value_t = 20)]
    pub fast: usize,
    /// Slow SMA period, in bars.
    #[arg(long, default_value_t = 50)]
    pub slow: usize,
    /// Position size, in contracts.
    #[arg(long, default_value_t = 1)]
    pub qty: i32,
    /// Minimum price increment, in points.
    #[arg(long, default_value = "0.25")]
    pub tick_points: String,
    /// Dollars per point per contract.
    #[arg(long, default_value_t = 50)]
    pub point_value_usd: i64,
    /// Starting cash, in dollars.
    #[arg(long, default_value = "100000")]
    pub initial_cash_usd: String,
    /// Half-spread crossed by a market order, in ticks.
    #[arg(long, default_value_t = 1)]
    pub half_spread_ticks: i64,
    /// Commission per contract per side, in dollars.
    #[arg(long, default_value = "1.25")]
    pub fee_per_contract_usd: String,
    /// Override the annualization factor outright. Beats both the calendar
    /// and the sample measurement.
    #[arg(long)]
    pub bars_per_year: Option<f64>,
    /// Which session calendar supplies the annualization factor: `auto` to
    /// match the instrument's root symbol, `none` to measure the sample
    /// instead, or an explicit calendar id.
    #[arg(long, default_value = "auto")]
    pub calendar: String,
}

/// Where the annualization factor came from. Printed with the result, because
/// an annualization factor is an assumption and CLAUDE.md §2.4 does not allow
/// silent ones.
enum Annualization {
    /// The operator said so.
    Given(f64),
    /// A session calendar counted the intervals in a year of sessions.
    Calendar {
        /// Which calendar answered.
        id: String,
        /// Its answer.
        value: f64,
        /// What measuring the loaded sample would have said, for comparison.
        sample: f64,
    },
    /// Measured from the loaded bars (D-0038), because no calendar applies.
    Sample {
        /// The measurement.
        value: f64,
        /// Why the calendar did not answer.
        why: String,
    },
}

impl Annualization {
    fn value(&self) -> f64 {
        match self {
            Annualization::Given(v)
            | Annualization::Calendar { value: v, .. }
            | Annualization::Sample { value: v, .. } => *v,
        }
    }
}

/// Everything parsed out of the arguments, so validation happens once and
/// before any file is opened.
struct Settings {
    tf: TimeFrame,
    range: Option<TsRange>,
    spec: ContractSpec,
    initial_cash_nano_usd: NanoUsd,
    fills: SpreadCrossFills,
}

/// Runs the command, returning the process exit code.
pub fn run(args: &BacktestArgs) -> i32 {
    let settings = match parse(args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    let dir = match data_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };

    let instrument = InstrumentId::new(&args.instrument);
    let mut feed = match ParquetBarFeed::open(&dir, &instrument, settings.tf, settings.range) {
        Ok(feed) => feed,
        Err(e) => {
            eprintln!("error: {e}");
            if let CuratedError::NoCuratedData { .. } = &e {
                suggest_instruments(&dir, settings.tf);
            }
            return match e {
                CuratedError::NoCuratedData { .. } | CuratedError::EmptyRange { .. } => EXIT_USAGE,
                _ => EXIT_FAILED,
            };
        }
    };

    let stale: Vec<u32> = {
        let mut versions: Vec<u32> = feed
            .sources()
            .iter()
            .map(|s| s.transcoder_version)
            .filter(|v| *v != TRANSCODER_VERSION)
            .collect();
        versions.sort_unstable();
        versions.dedup();
        versions
    };
    if !stale.is_empty() {
        eprintln!(
            "warning: curated data was written by transcoder v{}; this build is \
             v{TRANSCODER_VERSION}.\n\
             \x20        The bars below are what v{} produced, not what this build would.\n\
             \x20        Rebuild with: crucible transcode --force",
            stale
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", v"),
            stale[0]
        );
    }

    let annualization = match resolve_annualization(args, &feed, settings.tf, &instrument) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    let params = BacktestParams {
        initial_cash_nano_usd: settings.initial_cash_nano_usd,
        bars_per_year: annualization.value(),
    };
    let mut strategy = SmaCross::new(args.fast, args.slow, Qty(args.qty));
    let mut fills = settings.fills;

    print_header(args, &settings, &feed, &annualization);

    match run_backtest(
        &mut feed,
        &mut strategy,
        &mut fills,
        &settings.spec,
        &params,
    ) {
        Ok(result) => {
            print_result(&result);
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            EXIT_FAILED
        }
    }
}

/// Validates and converts every argument before anything is opened.
fn parse(args: &BacktestArgs) -> Result<Settings, String> {
    if args.fast == 0 || args.slow == 0 {
        return Err("--fast and --slow must be positive bar counts".to_owned());
    }
    if args.fast >= args.slow {
        return Err(format!(
            "--fast {} must be shorter than --slow {}; the crossover has no meaning otherwise",
            args.fast, args.slow
        ));
    }
    if args.qty <= 0 {
        return Err("--qty must be a positive number of contracts".to_owned());
    }
    if args.half_spread_ticks < 0 {
        return Err("--half-spread-ticks must not be negative".to_owned());
    }
    if args.point_value_usd <= 0 {
        return Err("--point-value-usd must be positive".to_owned());
    }

    let tf: TimeFrame = args
        .timeframe
        .parse()
        .map_err(|e: crucible_core::types::ParseTimeFrameError| format!("--timeframe: {e}"))?;

    let range = match (&args.start, &args.end) {
        (None, None) => None,
        (Some(start), Some(end)) => {
            let s = parse_civil_date(start)
                .ok_or_else(|| format!("--start {start:?} is not a YYYY-MM-DD calendar date"))?;
            let e = parse_civil_date(end)
                .ok_or_else(|| format!("--end {end:?} is not a YYYY-MM-DD calendar date"))?;
            Some(
                range_from_dates((s.year, s.month, s.day), (e.year, e.month, e.day))
                    .map_err(|err| err.to_string())?,
            )
        }
        _ => {
            return Err(
                "--start and --end go together; give both to bound the replay, or neither \
                 to use everything held"
                    .to_owned(),
            );
        }
    };

    // Text to integers, never through f64: a tick size one nanopoint off
    // snaps every fill onto the wrong grid (§2.3, D-0027).
    let tick =
        Price::from_points_str(&args.tick_points).map_err(|e| format!("--tick-points: {e}"))?;
    if tick.as_nanos() <= 0 {
        return Err("--tick-points must be a positive price increment".to_owned());
    }
    let initial_cash_nano_usd = parse_usd_to_nano(&args.initial_cash_usd)
        .map_err(|e| format!("--initial-cash-usd: {e}"))?;
    let fee_per_contract_nano_usd = parse_usd_to_nano(&args.fee_per_contract_usd)
        .map_err(|e| format!("--fee-per-contract-usd: {e}"))?;

    Ok(Settings {
        tf,
        range,
        spec: ContractSpec {
            instrument: InstrumentId::new(&args.instrument),
            tick,
            point_value_usd: args.point_value_usd,
        },
        initial_cash_nano_usd,
        fills: SpreadCrossFills {
            half_spread_ticks: args.half_spread_ticks,
            fee_per_contract_nano_usd,
        },
    })
}

/// Decides where the annualization factor comes from.
///
/// Precedence: an explicit `--bars-per-year` beats everything; otherwise a
/// session calendar answers if one governs the instrument; otherwise the
/// sample is measured, as D-0038 did before calendars existed.
///
/// The two disagree on purpose and neither is wrong. The calendar counts the
/// intervals a year of sessions *contains*; the sample counts the intervals
/// that actually *traded*, which is fewer, because `ohlcv` data has no bar for
/// an interval with no trade. Both numbers are printed whenever they differ by
/// more than a rounding error, so a reader can see which side of that the
/// Sharpe below sits on (D-0039).
fn resolve_annualization(
    args: &BacktestArgs,
    feed: &ParquetBarFeed,
    tf: TimeFrame,
    instrument: &InstrumentId,
) -> Result<Annualization, String> {
    let sample = derive_bars_per_year(feed, tf);
    if let Some(given) = args.bars_per_year {
        if !(given.is_finite() && given > 0.0) {
            return Err("--bars-per-year must be a positive number".to_owned());
        }
        return Ok(Annualization::Given(given));
    }

    let calendar = match args.calendar.as_str() {
        "none" => None,
        "auto" => Calendar::for_instrument(instrument)
            .map_err(|e| format!("bundled calendar tables are broken: {e}"))?,
        id => Some(Calendar::by_id(id).map_err(|e| format!("--calendar: {e}"))?),
    };

    match calendar {
        Some(cal) => {
            // A session template describes one era of an exchange. Annualizing
            // a 2013 replay with 2015-onward hours is this era's answer to a
            // different era's question, and silently wrong (D-0039).
            if let Some(first) = feed.first_ts_open() {
                let first_date = date_of(first);
                if days_from_civil(first_date) < days_from_civil(cal.valid_from()) {
                    eprintln!(
                        "warning: these bars start {first_date}, before calendar {} describes
                                  the exchange ({}). Session hours changed, so the
                                  annualization factor below is measured against the wrong
                                  template. Use --calendar none to measure the sample instead.",
                        cal.id(),
                        cal.valid_from()
                    );
                }
            }
            Ok(Annualization::Calendar {
                id: cal.id().to_owned(),
                value: cal.bars_per_year(tf),
                sample,
            })
        }
        None => Ok(Annualization::Sample {
            value: sample,
            why: match args.calendar.as_str() {
                "none" => "--calendar none".to_owned(),
                _ => format!("no bundled calendar claims {instrument}"),
            },
        }),
    }
}

/// Bars per year measured from the sample itself.
///
/// The demo's 347,760 assumes a 23-hour session every one of 252 days. Real
/// `ohlcv` data has no bar for an interval that did not trade, so that
/// constant overstates the bar count, which overstates the annualization
/// factor, which flatters Sharpe. Measuring the sample is the conservative
/// choice wherever no calendar applies.
fn derive_bars_per_year(feed: &ParquetBarFeed, tf: TimeFrame) -> f64 {
    let (Some(first), Some(last)) = (feed.first_ts_open(), feed.last_ts_open()) else {
        return 0.0;
    };
    let span_ns = (last.0 - first.0) + tf.duration_ns();
    if span_ns <= 0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "an annualization factor is statistics space (§2.3), not accounting"
    )]
    let bars = feed.len() as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "an annualization factor is statistics space (§2.3), not accounting"
    )]
    let span = span_ns as f64;
    bars * NANOS_PER_YEAR / span
}

/// Renders a timestamp as UTC, to the second.
fn utc(ts: Ts) -> String {
    let date = date_of(ts);
    let secs = (ts.0 - start_of(date).0) / 1_000_000_000;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        date.year,
        date.month,
        date.day,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn usd(n: NanoUsd) -> String {
    format!("${:.2}", nano_usd_to_f64(n))
}

/// Every assumption the numbers below rest on, stated before them.
fn print_header(
    args: &BacktestArgs,
    settings: &Settings,
    feed: &ParquetBarFeed,
    annualization: &Annualization,
) {
    println!(
        "Crucible backtest — {} {}, SMA({}/{}) cross, {} contract(s)\n",
        args.instrument, settings.tf, args.fast, args.slow, args.qty
    );
    let span = match (feed.first_ts_open(), feed.last_ts_open()) {
        (Some(first), Some(last)) => format!("{} .. {}", utc(first), utc(last)),
        _ => "(empty)".to_owned(),
    };
    println!("  data           {} bars   {span}", feed.len());
    println!(
        "  contract       tick {} pt, ${}/pt",
        settings.spec.tick, settings.spec.point_value_usd
    );
    println!(
        "  fill model     spread_cross — {} tick half-spread, {}/contract/side",
        settings.fills.half_spread_ticks,
        usd(settings.fills.fee_per_contract_nano_usd)
    );
    println!(
        "  capital        {} initial",
        usd(settings.initial_cash_nano_usd)
    );
    match annualization {
        Annualization::Given(value) => {
            println!("  annualization  {value:.0} bars/yr  (given on the command line)");
        }
        Annualization::Sample { value, why } => {
            println!("  annualization  {value:.0} bars/yr  (measured from this sample — {why})");
        }
        Annualization::Calendar { id, value, sample } => {
            println!("  annualization  {value:.0} bars/yr  (calendar {id})");
            // A large gap means the sample is missing intervals the session
            // contains — a thin contract, or a hole in the data. Either way the
            // reader should see it rather than trust one number.
            if (value - sample).abs() > value * 0.01 {
                println!(
                    "                 {sample:.0} bars/yr would be measured from this sample \
                     ({:+.1}%)",
                    (sample - value) / value * 100.0
                );
            }
        }
    }
    println!("  curated        schema v{CURATED_SCHEMA_VERSION}, transcoder v{TRANSCODER_VERSION}");
    for source in feed.sources() {
        println!(
            "  source         {}  ({} bars)",
            source.source_file_path, source.rows
        );
        println!("                 blake3 {}", source.source_file_blake3);
    }
    println!();
}

fn print_result(result: &BacktestResult) {
    let s = &result.summary;
    println!("  final equity     {:>14}", usd(s.final_equity_nano_usd));
    println!(
        "  return           {:>13.2} %   (of capital)",
        s.total_return_pct
    );
    println!("  max drawdown     {:>13.2} %", s.max_drawdown_pct);
    println!("  round trips      {:>14}", s.round_trips);
    match s.win_rate {
        Some(w) => println!("  win rate         {:>13.1} %", w * 100.0),
        None => println!("  win rate         {:>14}", "n/a (no trades)"),
    }
    match s.sharpe_naive {
        Some(x) => println!("  Sharpe (naive)   {:>14.2}", x),
        None => println!("  Sharpe (naive)   {:>14}", "n/a"),
    }
    println!("  fees             {:>14}", usd(s.fees_nano_usd));
    println!("  fills            {:>14}", result.n_fills);
    if result.cancelled_at_eof > 0 {
        println!(
            "  cancelled at EOF {:>14}   (orders the feed ended before filling)",
            result.cancelled_at_eof
        );
    }
    println!(
        "\n  One instrument, one fill model, one parameter pair, no benchmark and no\n\
         \x20 trial count: this is a control run, not a verdict. Comparisons against\n\
         \x20 buy-and-hold and a matched random-entry baseline arrive with the\n\
         \x20 predictor workbench; deflated Sharpe and PBO arrive with the funnel."
    );
}

/// Names what *is* transcoded, so "not found" is actionable.
fn suggest_instruments(dir: &std::path::Path, tf: TimeFrame) {
    match crucible_data::curated::list_instruments(dir, tf) {
        Ok(found) if !found.is_empty() => {
            eprintln!("       curated {tf} bars exist for: {}", found.join(", "));
        }
        Ok(_) => {}
        Err(e) => eprintln!("       (could not list curated instruments: {e})"),
    }
}
