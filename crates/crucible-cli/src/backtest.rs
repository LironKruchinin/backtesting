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
//! `--stop-ticks` / `--target-ticks` bracket every position the strategy
//! opens, and bring a *second* named assumption with them: an OHLC bar does
//! not say whether its high or its low printed first, so the intrabar ordering
//! convention (`stop_first_intrabar`, `crucible-engine::bracket`) is printed in
//! the header and the bars where it decided the outcome are counted in the
//! result. A run whose PnL turns on many of those is a run to distrust, and it
//! says so.
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
    CURATED_SCHEMA_VERSION, CuratedError, ParquetBarFeed, Resolution, TRANSCODER_VERSION,
};
use crucible_data::ingest::money::parse_usd_to_nano;
use crucible_data::ingest::range_from_dates;
use crucible_data::ingest::window::{date_of, days_from_civil, parse_civil_date, start_of};
use crucible_engine::{
    BacktestParams, BacktestResult, INTRABAR_CONVENTION, SpreadCrossFills, run as run_backtest,
};
use crucible_strategies::{Bracketed, SmaCross};

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
    /// Protective stop, in ticks below (long) / above (short) the price each
    /// entry actually fills at. Omit for a naked position.
    ///
    /// `allow_negative_numbers` so that `--stop-ticks -8` reaches the check
    /// below and gets told *why* a signed distance is wrong, instead of clap's
    /// "unexpected argument '-8'" — the direction is taken from the position,
    /// and writing it out is a plausible mistake.
    #[arg(long, allow_negative_numbers = true)]
    pub stop_ticks: Option<i64>,
    /// Profit target, in ticks the other way. Omit for a naked position.
    #[arg(long, allow_negative_numbers = true)]
    pub target_ticks: Option<i64>,
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
    /// Protective levels every entry carries, if any. `None` is a naked
    /// position, which is what this command did before M2's stops landed and
    /// still does by default.
    bracket: Option<Bracket>,
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

    let instrument = match resolve_instrument(&dir, settings.tf, &args.instrument) {
        Ok(id) => id,
        Err(code) => return code,
    };
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
    let mut fills = settings.fills;

    print_header(args, &settings, &feed, &annualization);

    // Two branches because a bracketed strategy is a different type, not a
    // different flag: nothing downstream can forget to apply the wrapper.
    let outcome = match settings.bracket {
        Some(bracket) => {
            let mut strategy =
                Bracketed::new(SmaCross::new(args.fast, args.slow, Qty(args.qty)), bracket);
            run_backtest(
                &mut feed,
                &mut strategy,
                &mut fills,
                &settings.spec,
                &params,
            )
        }
        None => {
            let mut strategy = SmaCross::new(args.fast, args.slow, Qty(args.qty));
            run_backtest(
                &mut feed,
                &mut strategy,
                &mut fills,
                &settings.spec,
                &params,
            )
        }
    };

    match outcome {
        Ok(result) => {
            print_result(&result, &settings);
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
    for (name, ticks) in [
        ("--stop-ticks", args.stop_ticks),
        ("--target-ticks", args.target_ticks),
    ] {
        if ticks.is_some_and(|t| t <= 0) {
            return Err(format!(
                "{name} must be a positive tick distance from the fill price; zero would put \
                 the level on the entry and a negative one on the wrong side of it"
            ));
        }
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
        bracket: (args.stop_ticks.is_some() || args.target_ticks.is_some())
            .then(|| Bracket::new(args.stop_ticks, args.target_ticks)),
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
    match settings.bracket {
        Some(bracket) => {
            println!(
                "  brackets       stop {}, target {} — from the price each entry FILLS at",
                bracket
                    .stop_ticks()
                    .map_or_else(|| "none".to_owned(), |t| format!("{t} tick(s)")),
                bracket
                    .target_ticks()
                    .map_or_else(|| "none".to_owned(), |t| format!("{t} tick(s)"))
            );
            println!(
                "  intrabar       {INTRABAR_CONVENTION} — a bar touching both levels is read as \
                 the STOP\n\
                 \x20                filling first, and a bar opening beyond a level fills at that \
                 open,\n\
                 \x20                never at the level (D-0069). Bars where that choice decided \
                 the\n\
                 \x20                outcome are counted below"
            );
        }
        None => println!("  brackets       none — positions are naked; no exit is path-dependent"),
    }
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

fn print_result(result: &BacktestResult, settings: &Settings) {
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
    if settings.bracket.is_some() {
        println!(
            "  stop/target exits{:>14}   (of {} fills)",
            result.n_protective_exits, result.n_fills
        );
        println!(
            "  path-sensitive   {:>14}   (bars where {INTRABAR_CONVENTION} chose the outcome)",
            result.path_sensitive_bars
        );
        println!(
            "{}",
            path_sensitivity_note(result.path_sensitive_bars, result.n_protective_exits)
        );
    }
    println!(
        "\n  One instrument, one fill model, one parameter pair, no benchmark and no\n\
         \x20 trial count: this is a control run, not a verdict. Comparisons against\n\
         \x20 buy-and-hold and a matched random-entry baseline arrive with the\n\
         \x20 predictor workbench; deflated Sharpe and PBO arrive with the funnel."
    );
}

/// How much of the number above rests on a convention rather than on the data.
///
/// An OHLC bar does not say whether its high or its low printed first, so an
/// exit from a bar that touched both levels is a *choice*. Stating the count is
/// the whole point of the M2 line that asked for it: a run where most exits
/// came from ambiguous bars would have a materially different PnL under a
/// different-but-equally-defensible rule, and a reader cannot judge that from a
/// return figure alone.
fn path_sensitivity_note(path_sensitive_bars: usize, n_protective_exits: usize) -> String {
    if path_sensitive_bars == 0 {
        return "\n  Every stop/target exit above was decided by the bars themselves: no bar \
                touched\n\
                \x20 both levels, so nothing here depends on the intrabar convention."
            .to_owned();
    }
    let share = if n_protective_exits == 0 {
        0.0
    } else {
        #[expect(clippy::cast_precision_loss, reason = "small counts, display only")]
        let (num, den) = (path_sensitive_bars as f64, n_protective_exits as f64);
        num / den * 100.0
    };
    format!(
        "\n  PATH-SENSITIVE: {path_sensitive_bars} of {n_protective_exits} stop/target exits came \
         from a bar that touched BOTH\n\
         \x20 levels ({share:.0}%). An OHLC bar does not record which printed first, so each of\n\
         \x20 those exits is the worst-case convention's answer, not the data's: under a\n\
         \x20 target-first rule they would each have paid the target instead. Treat the\n\
         \x20 return above as one end of a range, and prefer a wider bracket or a finer\n\
         \x20 timeframe (1s bars resolve most of these) before quoting it."
    )
}

/// Turns `--instrument` into the curated contract it names, or an exit code.
///
/// A curated contract is keyed by its canonical four-digit spelling
/// (`ESH2024`), but `ESH4` is what the vendor writes and what this project's
/// own docs used, so the shorthand keeps working wherever it names exactly one
/// contract — and **refuses where it names two** (D-0072). `GCZ4` is December
/// 2014 gold and December 2024 gold; picking one would be the very bug the
/// four-digit key exists to prevent, moved from the archive into the CLI.
fn resolve_instrument(
    dir: &std::path::Path,
    tf: TimeFrame,
    requested: &str,
) -> Result<InstrumentId, i32> {
    match crucible_data::curated::resolve_instrument(dir, tf, requested) {
        Ok(Resolution::Exact(name)) => {
            if name != requested {
                println!("  instrument      {requested} -> {name}");
            }
            Ok(InstrumentId::new(&name))
        }
        Ok(Resolution::Ambiguous(names)) => {
            eprintln!(
                "error: --instrument {requested} names {} curated contracts: {}.\n\
                 \x20      A one-digit CME year code repeats every ten years and this \
                 archive spans sixteen, so it is genuinely two contracts, not one \
                 spelled loosely (D-0072). Name the one you mean.",
                names.len(),
                names.join(", ")
            );
            Err(EXIT_USAGE)
        }
        // Nothing matched: leave the refusal to `ParquetBarFeed::open`, which
        // already says "not found" and lists what is there.
        Ok(Resolution::Missing(_)) => Ok(InstrumentId::new(requested)),
        Err(e) => {
            eprintln!("error: {e}");
            Err(EXIT_FAILED)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::path_sensitivity_note;

    /// The zero case has to say *why* it is zero. "No number printed" and "no
    /// ambiguous bars" read identically to someone scanning a report, and only
    /// one of them means the result is safe to quote.
    #[test]
    fn no_ambiguous_bars_still_says_so() {
        let note = path_sensitivity_note(0, 12);
        assert!(note.contains("decided by the bars themselves"), "{note}");
        assert!(!note.contains("PATH-SENSITIVE"), "{note}");
    }

    /// And the nonzero case has to be impossible to skim past: the counts, the
    /// share, and what a different convention would have paid.
    #[test]
    fn ambiguous_bars_are_flagged_loudly_with_their_share() {
        let note = path_sensitivity_note(3, 12);
        assert!(note.contains("PATH-SENSITIVE"), "{note}");
        assert!(note.contains("3 of 12"), "{note}");
        assert!(note.contains("(25%)"), "{note}");
        assert!(note.contains("target-first"), "{note}");
    }
}
