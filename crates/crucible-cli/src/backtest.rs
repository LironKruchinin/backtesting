//! `crucible backtest` — the reference strategy on real archived bars.
//!
//! This is the M1 exit artifact, not a research tool. It runs exactly one
//! strategy (`SmaCross`) under exactly one fill model (`spread_cross`) over
//! one instrument, and prints what happened. Grids, folds, walk-forward,
//! trial counting, and scorecards are the funnel's job (M2/M3) and are
//! deliberately absent here — a thin command that tells the truth is worth
//! more at this milestone than a thick one that invites p-hacking.
//!
//! ## Two stores, one command (D-0073)
//!
//! `--instrument` names either a **curated contract** (`ESH2024`, or the
//! `ESH4` shorthand while it is unambiguous — D-0072) or a **continuous
//! alias** (`ES.v.0`, `ES.c.0` — §4). The two are told apart by shape, not by
//! trying one and falling back: a name with three dot-separated parts is an
//! attempt at an alias and gets an alias-shaped refusal if it is a bad one.
//!
//! A continuous replay adds one assumption to the header — the **adjustment**
//! — and one number to the result: the **roll count**. Both are there because
//! §2.4 does not allow a silent one. A stitched series carries two price views
//! on every bar: the tradeable prices of the then-front contract, which fills,
//! marks and PnL use, and the back-adjusted level beside them, which the
//! strategy's indicators use. Nothing here converts between them; the type
//! system will not allow it (D-0042).
//!
//! What a continuous run does **not** model is the cost of the roll itself: a
//! position carried across one pays no spread and no fee in this build. The
//! roll count is printed so a reader can size that omission rather than
//! discover it.
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

use std::path::{Path, PathBuf};

use crucible_core::prelude::*;
use crucible_data::calendar::Calendar;
use crucible_data::catalog::TsRange;
use crucible_data::continuous::{
    AdjustmentKind, ContinuousAlias, ContinuousError, ContinuousFeed, ContinuousSegment, RollTable,
    roll_table_for_alias,
};
use crucible_data::curated::{
    CURATED_SCHEMA_VERSION, CuratedError, CuratedSource, ParquetBarFeed, Resolution,
    TRANSCODER_VERSION,
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
    /// Instrument to replay: a curated contract (`ESH2024`, or the `ESH4`
    /// shorthand while it names exactly one) or a continuous alias (`ES.v.0`
    /// volume-roll, `ES.c.0` calendar-roll). A contract must already be
    /// transcoded; an alias also needs its roll table (`crucible rolls`).
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
    /// How a continuous series is adjusted for the strategy's indicators:
    /// `back_adjust` (the default) or `none`. Ignored for a single contract,
    /// which has nothing to adjust.
    ///
    /// It is a named assumption and not a convenience (§2.4). `back_adjust`
    /// shifts every older segment onto the newest so an indicator sees no step
    /// at a roll; `none` leaves each segment where it traded, so it does.
    /// Neither ever touches PnL, which is computed from the tradeable prices
    /// in both cases.
    #[arg(long, default_value = "back_adjust")]
    pub adjustment: String,
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
    /// How a stitched series is adjusted for signals. Unused by a single
    /// contract, which has no rolls.
    adjustment: AdjustmentKind,
    /// Position size in contracts, validated positive. Carried here so the
    /// roll-gap bound can be scaled by it without re-reading the arguments.
    qty: i32,
}

/// The two things this command can replay, behind one [`Feed`].
///
/// A wrapper rather than `Box<dyn Feed>` because the accessors below are not
/// on the `Feed` trait and should not be: the engine has no business asking a
/// feed how many bars it holds. This is CLI wiring, which is what §3 says the
/// CLI is for.
enum Replay {
    /// One curated contract.
    Outright(ParquetBarFeed),
    /// A stitched chain of them. Boxed because it is much the larger variant.
    Continuous(Box<ContinuousFeed>),
}

impl Replay {
    fn len(&self) -> usize {
        match self {
            Replay::Outright(f) => f.len(),
            Replay::Continuous(f) => f.len(),
        }
    }

    fn first_ts_open(&self) -> Option<Ts> {
        match self {
            Replay::Outright(f) => f.first_ts_open(),
            Replay::Continuous(f) => f.first_ts_open(),
        }
    }

    fn last_ts_open(&self) -> Option<Ts> {
        match self {
            Replay::Outright(f) => f.last_ts_open(),
            Replay::Continuous(f) => f.last_ts_open(),
        }
    }

    fn sources(&self) -> &[CuratedSource] {
        match self {
            Replay::Outright(f) => f.sources(),
            Replay::Continuous(f) => f.sources(),
        }
    }
}

impl Feed for Replay {
    fn next_event(&mut self) -> Option<MarketEvent> {
        match self {
            Replay::Outright(f) => f.next_event(),
            Replay::Continuous(f) => f.next_event(),
        }
    }
}

/// What a continuous replay rests on, gathered for the header.
///
/// Every field here is an assumption the numbers depend on, which is why they
/// are carried to the printer instead of being applied and forgotten (§2.4).
struct Continuous {
    alias: ContinuousAlias,
    table_path: PathBuf,
    table: RollTable,
    adjustment: AdjustmentKind,
    /// The window asked for, and the window actually replayed. They differ
    /// when the request reached outside the table's span, and the difference
    /// is printed rather than absorbed.
    requested: Option<TsRange>,
    effective: Option<TsRange>,
    /// One entry per contract that contributed bars, with its offset.
    segments: Vec<ContinuousSegment>,
}

impl Continuous {
    /// Rolls *inside the replayed window* — one fewer than the contracts that
    /// actually contributed bars, not the whole table's row count.
    fn rolls(&self) -> usize {
        self.segments.len().saturating_sub(1)
    }

    /// The largest amount of PnL above that could be an artifact of a roll
    /// rather than a price move: `Σ |gap| × point_value × qty` over the rolls
    /// this window spans.
    ///
    /// A roll is a **position** event and this build models it as a price
    /// event only (M2). The feed hands the engine the then-front contract's
    /// tradeable prices under one alias, so a position carried through a roll
    /// sees the price step by the gap and books it, where a real roll would
    /// have closed the old contract and opened the new one at the two prices
    /// that made the gap — no PnL, and two more spreads and fees to pay.
    ///
    /// The bound is worst case in both directions: it assumes a full position
    /// was open across *every* roll and that every gap fell the same way.
    /// Actual contamination is that number times the fraction of rolls the
    /// strategy was positioned through, signed. Printing the bound rather than
    /// the actual figure is deliberate — the engine does not retain a position
    /// series (D-0071 makes the same argument about equity), and a bound that
    /// is certainly not exceeded is worth more than an estimate that looks
    /// exact.
    fn roll_gap_bound_nano_usd(&self, spec: &ContractSpec, qty: i32) -> NanoUsd {
        let mut total: NanoUsd = 0;
        for pair in self.segments.windows(2) {
            let row = self.table.rows.iter().find(|r| {
                r.from_contract == pair[0].contract.as_str()
                    && r.to_contract == pair[1].contract.as_str()
            });
            if let Some(row) = row {
                let magnitude = Price::from_nanos(row.adjustment.as_nanos().abs());
                total += spec.pnl_nano_usd(magnitude, i64::from(qty.abs()));
            }
        }
        total
    }
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

    // Which store the name points at is decided by its shape, before either
    // is opened. A continuous alias never names a curated contract directory
    // and vice versa, so there is nothing to try-and-fall-back on.
    let (mut feed, instrument, continuous) = if ContinuousAlias::looks_like(&args.instrument) {
        match open_continuous(&dir, &args.instrument, &settings) {
            Ok((feed, instrument, ctx)) => (feed, instrument, Some(ctx)),
            Err(code) => return code,
        }
    } else {
        match open_outright(&dir, &args.instrument, &settings) {
            Ok((feed, instrument)) => (feed, instrument, None),
            Err(code) => return code,
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

    print_header(args, &settings, &feed, &annualization, continuous.as_ref());

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
            print_result(&result, &settings, continuous.as_ref());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            EXIT_FAILED
        }
    }
}

/// Opens one curated contract, resolving a shorthand on the way.
fn open_outright(
    dir: &Path,
    requested: &str,
    settings: &Settings,
) -> Result<(Replay, InstrumentId), i32> {
    let instrument = resolve_instrument(dir, settings.tf, requested)?;
    match ParquetBarFeed::open(dir, &instrument, settings.tf, settings.range) {
        Ok(feed) => Ok((Replay::Outright(feed), instrument)),
        Err(e) => {
            eprintln!("error: {e}");
            if let CuratedError::NoCuratedData { .. } = &e {
                suggest_instruments(dir, settings.tf);
            }
            Err(match e {
                CuratedError::NoCuratedData { .. } | CuratedError::EmptyRange { .. } => EXIT_USAGE,
                _ => EXIT_FAILED,
            })
        }
    }
}

/// Opens the stitched series a continuous alias names.
///
/// Three refusals live upstream of this and are surfaced verbatim: an alias
/// that does not parse, no stored roll table for the rule, and two stored
/// tables answering to one alias letter. The fourth — a replay window the
/// table cannot describe — is [`ContinuousFeed::open`]'s (D-0045), and it is
/// still made: what happens here first is that a *date-granular* request is
/// narrowed to the window the table covers, and the narrowing is printed.
///
/// The distinction matters. `--start 2010-06-06` asks for a calendar day; the
/// archive's first ES bar opens at 22:00 that day, so the request reaches 22
/// hours before the table's span through nothing but the difference between a
/// date and an instant. Refusing that is a refusal about arithmetic, not about
/// missing contracts. A request with **no** overlap still refuses, and so does
/// one the library judges uncovered.
fn open_continuous(
    dir: &Path,
    requested_name: &str,
    settings: &Settings,
) -> Result<(Replay, InstrumentId, Continuous), i32> {
    let refuse_usage = |e: &ContinuousError| -> i32 {
        eprintln!("error: {e}");
        EXIT_USAGE
    };

    let alias = ContinuousAlias::parse(requested_name).map_err(|e| refuse_usage(&e))?;
    let (table, table_path) =
        roll_table_for_alias(dir, &alias, settings.tf).map_err(|e| match e {
            ContinuousError::NoRollTable { .. } | ContinuousError::AmbiguousRollTable { .. } => {
                refuse_usage(&e)
            }
            other => {
                eprintln!("error: {other}");
                EXIT_FAILED
            }
        })?;

    let effective = match settings.range {
        None => None,
        Some(requested) => match table.narrow(requested) {
            Ok(Some(narrowed)) => Some(narrowed),
            Ok(None) => {
                let covered = table.covered_range().map_err(|e| refuse_usage(&e))?;
                eprintln!(
                    "error: {alias} {} covers {} .. {} and the requested window {} .. {} \
                     does not overlap it at all.\n\
                     \x20      The roll table is derived and disposable — if the archive has \
                     grown since it\n\
                     \x20      was built, rebuild it: crucible rolls --root {} --timeframe {} \
                     --write",
                    settings.tf,
                    utc(covered.start_ts()),
                    utc(covered.end_ts()),
                    utc(requested.start_ts()),
                    utc(requested.end_ts()),
                    alias.root(),
                    settings.tf
                );
                return Err(EXIT_USAGE);
            }
            Err(e) => return Err(refuse_usage(&e)),
        },
    };

    let feed = ContinuousFeed::open(dir, &table, settings.adjustment, effective).map_err(|e| {
        eprintln!("error: {e}");
        match e {
            ContinuousError::RangeNotCovered { .. } | ContinuousError::EmptySeries { .. } => {
                EXIT_USAGE
            }
            _ => EXIT_FAILED,
        }
    })?;

    let instrument = feed.alias().clone();
    let context = Continuous {
        alias,
        table_path,
        table,
        adjustment: settings.adjustment,
        requested: settings.range,
        effective,
        segments: feed.segments(),
    };
    Ok((Replay::Continuous(Box::new(feed)), instrument, context))
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

    let adjustment = match args.adjustment.as_str() {
        "back_adjust" => AdjustmentKind::BackAdjust,
        "none" => AdjustmentKind::None,
        // `ratio` is refused by name rather than by omission: it is not a
        // variant at all, for the arithmetic reason in
        // `continuous::adjust` (D-0043), and a reader who typed it deserves
        // to know that instead of "unknown value".
        "ratio" => {
            return Err(
                "--adjustment ratio does not exist: proportional adjustment needs a \
                 rounding scheme that keeps a 2010 bar's value independent of how many \
                 rolls happen after it, and shipping a rounded-f64 version would put \
                 floats in the price path (D-0043). Use back_adjust or none"
                    .to_owned(),
            );
        }
        other => {
            return Err(format!(
                "--adjustment {other:?} is not one of: back_adjust, none"
            ));
        }
    };

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
        adjustment,
        qty: args.qty,
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
    feed: &Replay,
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
fn derive_bars_per_year(feed: &Replay, tf: TimeFrame) -> f64 {
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
    feed: &Replay,
    annualization: &Annualization,
    continuous: Option<&Continuous>,
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
    if let Some(c) = continuous {
        print_continuous_header(c, settings);
    }
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
    print_sources(feed, continuous.is_some());
    println!();
}

/// The provenance chain (D-0013): which raw bytes are behind these bars.
///
/// A single contract lists every curated file it read, which is at most a
/// handful. A sixteen-year stitched series reads eighty, and eighty repeated
/// blake3 lines is a listing nobody checks — so those are folded by **raw
/// file**, which is the manifest id and the thing a reader can actually verify
/// (`crucible verify`). Nothing is dropped: the bar counts still sum to the
/// replay.
fn print_sources(feed: &Replay, fold: bool) {
    if !fold {
        for source in feed.sources() {
            println!(
                "  source         {}  ({} bars)",
                source.source_file_path, source.rows
            );
            println!("                 blake3 {}", source.source_file_blake3);
        }
        return;
    }
    // BTreeMap, not HashMap: this ordering reaches the output (§2.2).
    let mut folded: std::collections::BTreeMap<(&str, &str), (usize, usize)> =
        std::collections::BTreeMap::new();
    for source in feed.sources() {
        let entry = folded
            .entry((
                source.source_file_path.as_str(),
                source.source_file_blake3.as_str(),
            ))
            .or_insert((0, 0));
        entry.0 += source.rows;
        entry.1 += 1;
    }
    for ((path, blake3), (rows, files)) in folded {
        println!("  source         {path}  ({rows} bars in {files} curated file(s))");
        println!("                 blake3 {blake3}");
    }
}

/// The stitch: what it is made of, and every choice that shaped it.
fn print_continuous_header(c: &Continuous, settings: &Settings) {
    println!(
        "  stitch         {} contract(s), {} roll(s) — {} .. {}",
        c.segments.len(),
        c.rolls(),
        c.segments.first().map_or("(none)", |s| s.contract.as_str()),
        c.segments.last().map_or("(none)", |s| s.contract.as_str())
    );
    println!("  roll rule      {}", c.table.rule);
    println!(
        "  roll table     {}  (expiries {}, decade anchor {})",
        c.table_path.display(),
        c.table.expiry_source,
        c.table.decade_anchor
    );
    match c.adjustment {
        AdjustmentKind::BackAdjust => {
            let oldest = c.segments.first().map_or(Price::ZERO, |s| s.offset);
            println!(
                "  adjustment     back_adjust — older segments shifted onto the newest; the \
                 oldest\n\
                 \x20                carries {oldest} points. SIGNALS ONLY: every fill, mark and \
                 PnL\n\
                 \x20                figure below uses the tradeable price of the then-front \
                 contract"
            );
        }
        AdjustmentKind::None => println!(
            "  adjustment     none — every segment left where it traded, so the strategy's\n\
             \x20                indicators see a step at each roll"
        ),
    }
    if let (Some(requested), Some(effective)) = (c.requested, c.effective)
        && requested != effective
    {
        println!(
            "  window         asked {} .. {}\n\
             \x20                table covers ts_open {} .. {}, so {} .. {} was replayed.\n\
             \x20                Outside a roll table's span there is no front-contract answer; \
             if the\n\
             \x20                archive has grown since the table was built, rebuild it:\n\
             \x20                  crucible rolls --root {} --timeframe {} --write",
            utc(requested.start_ts()),
            utc(requested.end_ts()),
            utc(c.table.first_ts_open),
            utc(c.table.last_ts_open),
            utc(effective.start_ts()),
            utc(effective.end_ts()),
            c.alias.root(),
            settings.tf
        );
    }
}

fn print_result(result: &BacktestResult, settings: &Settings, continuous: Option<&Continuous>) {
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
    if let Some(c) = continuous {
        println!(
            "  rolls            {:>14}   (contract handovers inside this window)",
            c.rolls()
        );
        println!(
            "  roll-gap bound   {:>14}   (max of the PnL above that is a roll, not a move)",
            usd(c.roll_gap_bound_nano_usd(&settings.spec, settings.qty))
        );
    }
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
    if let Some(note) = insolvency_note(&result.equity) {
        println!("{note}");
    }
    println!(
        "\n  assumptions    fill model spread_cross ({} tick half-spread, {}/contract/side);\n\
         \x20                intrabar {INTRABAR_CONVENTION}{}",
        settings.fills.half_spread_ticks,
        usd(settings.fills.fee_per_contract_nano_usd),
        match settings.bracket {
            // Not "n/a": no bracket means nothing in this run could touch two
            // levels in one bar, which is *why* the count is absent. §2.4 asks
            // for the assumption to be named either way.
            None => " (no bracket, so no exit was path-dependent)".to_owned(),
            Some(_) => format!(" ({} path-sensitive bar(s))", result.path_sensitive_bars),
        }
    );
    if let Some(c) = continuous {
        println!(
            "\x20                continuous {} on {}, adjustment {} — SIGNALS ONLY.\n\
             \x20                A position carried across one of the {} roll(s) above paid no\n\
             \x20                spread and no fee for it, and booked the price step between the\n\
             \x20                two contracts as PnL: a roll is a position event and this build\n\
             \x20                models it as a price event only (M2). At most {} of the result\n\
             \x20                above is that — assuming a full position through every roll.",
            c.alias,
            c.table.rule,
            c.adjustment,
            c.rolls(),
            usd(c.roll_gap_bound_nano_usd(&settings.spec, settings.qty))
        );
    }
    println!(
        "\n  One instrument, one fill model, one parameter pair, no benchmark and no\n\
         \x20 trial count: this is a control run, not a verdict. Comparisons against\n\
         \x20 buy-and-hold and a matched random-entry baseline arrive with the\n\
         \x20 predictor workbench; deflated Sharpe and PBO arrive with the funnel."
    );
}

/// Says so when the account went to zero, and what that does to the ratios.
///
/// There is no margin model in this build (`docs/MILESTONES.md`, M2), so the
/// replay keeps trading a position no broker would have let it hold, and the
/// equity curve simply goes negative. Every *dollar* figure above is still
/// exact — integer accounting does not care about the sign — but two of the
/// ratios stop meaning what their names say:
///
/// - **Sharpe** is the mean over standard deviation of `equity[i]/equity[i-1] −
///   1`, and `Summary::compute` skips a step whose starting equity is not
///   positive, because a ratio across zero is not a return. So the figure
///   describes the *solvent prefix* of the run and says nothing about what
///   happened after. A positive Sharpe beside a large negative return is that,
///   not a discovery.
/// - **Max drawdown** is measured against the running peak, so once equity is
///   negative it exceeds 100 % and keeps going.
///
/// Printed rather than suppressed: a reader who scrolls to "Sharpe 0.23" and
/// stops is exactly the reader this line exists for.
fn insolvency_note(equity: &[(Ts, NanoUsd)]) -> Option<String> {
    let (index, (ts, value)) = equity
        .iter()
        .enumerate()
        .find(|(_, (_, e))| *e <= 0)
        .map(|(i, &(ts, e))| (i, (ts, e)))?;
    #[expect(clippy::cast_precision_loss, reason = "display only, §2.3")]
    let share = (index as f64) / (equity.len().max(1) as f64) * 100.0;
    Some(format!(
        "\n  INSOLVENT: equity first reached {} at {} — bar {index} of {}, {share:.0}% of the \
         way in.\n\
         \x20 There is no margin model in this build, so the replay went on trading a position\n\
         \x20 no broker would have carried. The dollar figures above are exact; the ratios are\n\
         \x20 not what they look like. The naive Sharpe is computed only over the steps whose\n\
         \x20 starting equity was positive, so it describes the solvent prefix and nothing\n\
         \x20 after it, and max drawdown passes 100% once equity is negative. Read the PnL,\n\
         \x20 the fees and the round-trip count; do not quote the Sharpe.",
        usd(value),
        utc(ts),
        equity.len()
    ))
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
fn resolve_instrument(dir: &Path, tf: TimeFrame, requested: &str) -> Result<InstrumentId, i32> {
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
fn suggest_instruments(dir: &Path, tf: TimeFrame) {
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
    use super::{insolvency_note, path_sensitivity_note};
    use crucible_core::prelude::*;

    fn curve(points: &[i64]) -> Vec<(Ts, NanoUsd)> {
        points
            .iter()
            .enumerate()
            .map(|(i, &e)| {
                #[expect(clippy::cast_possible_wrap, reason = "tiny test indices")]
                let ts = Ts(i as i64);
                (ts, e * 1_000_000_000)
            })
            .collect()
    }

    /// A solvent run says nothing: the note is for the case where two of the
    /// printed ratios stop meaning what they say, and printing it always would
    /// train a reader to skip it.
    #[test]
    fn a_solvent_run_gets_no_insolvency_note() {
        assert!(insolvency_note(&curve(&[100, 90, 1, 50])).is_none());
    }

    /// And an insolvent one has to say *where*, because "how far in" is what
    /// tells a reader whether the surviving ratios describe the run or a
    /// sliver of it.
    #[test]
    fn an_insolvent_run_names_the_bar_it_broke_on() {
        let note = insolvency_note(&curve(&[100, 50, 0, -20])).expect("insolvent");
        assert!(note.contains("INSOLVENT"), "{note}");
        assert!(note.contains("bar 2 of 4"), "{note}");
        assert!(note.contains("do not quote the Sharpe"), "{note}");
        // Zero counts: an account at exactly zero equity is gone, and the
        // return series stops there too (`prev > 0.0` in `Summary::compute`).
        assert!(note.contains("$0.00"), "{note}");
    }

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
