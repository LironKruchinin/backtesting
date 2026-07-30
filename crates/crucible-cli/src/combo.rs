//! `crucible combo` — a strategy defined in TOML instead of in Rust.
//!
//! Expansion (the default) reads a config, validates it, expands its
//! parameter axes and prints the grid. It opens no archive, spends nothing,
//! and is the cheapest possible check that a config says what its author
//! meant. `--run` then replays every combo on the config's declared data
//! source.
//!
//! What this command deliberately is **not**: the funnel. There are no
//! stages, no folds, no trial counting, no verdict and no scorecard — those
//! consume grids at scale and are M3. What it does do is the part §2.6
//! demands and no funnel can retrofit: every combo runs on **one** bar
//! series, collected once, and every combo's orders are suppressed until the
//! grid's shared warmup has passed, so a short-warmup combo cannot win by
//! having traded a longer sample.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the config expanded (and, with `--run`, replayed) |
//! | 2 | the config is wrong, or the data asked for does not exist |
//! | 4 | data exists but could not be trusted |

use std::path::PathBuf;

use crucible_core::prelude::*;
use crucible_data::SyntheticFeed;
use crucible_data::calendar::Calendar;
use crucible_data::continuous::ContinuousAlias;
use crucible_data::curated::{CuratedError, ParquetBarFeed};
use crucible_data::ingest::range_from_dates;
use crucible_data::ingest::window::parse_civil_date;
use crucible_engine::{BacktestParams, BacktestResult, FreeFills, INTRABAR_CONVENTION, run};
use crucible_strategies::Aligned;
use crucible_strategies::combo::ComboStrategy;

use crate::config::{self, ConfigError, DataSource, LoadedConfig};
use crate::pull::{EXIT_FAILED, EXIT_USAGE, data_dir};

/// Nanoseconds in a mean Gregorian year (365.2425 days).
pub(crate) const NANOS_PER_YEAR: f64 = 365.2425 * 86_400.0 * 1e9;

/// Combos above which a grid is more likely a mistyped `step` than a plan.
///
/// Re-exported from `crucible-funnel::grid` rather than restated: `combo`
/// warns at this count and `funnel` refuses at a higher one, and two crates
/// holding their own copy of either number is how they drift apart.
pub(crate) use crucible_funnel::grid::LOUD_COMBO_COUNT;

/// Combos listed in full before the listing is elided.
const LIST_LIMIT: usize = 40;

/// Arguments to `crucible combo`.
#[derive(Debug, clap::Args)]
pub struct ComboArgs {
    /// Path to the combo config.
    #[arg(long)]
    pub config: PathBuf,
    /// Replay every combo on the config's declared data source.
    #[arg(long)]
    pub run: bool,
    /// Print only the determinism hash of every combo's equity curve. Implies
    /// `--run`; this is the grid's CI gate.
    #[arg(long)]
    pub hash_only: bool,
}

/// Runs the command, returning the process exit code.
pub fn run_cmd(args: &ComboArgs) -> i32 {
    let loaded = match config::load(&args.config) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("error: {e}");
            if let ConfigError::Parse { .. } = e {
                eprintln!(
                    "       Unknown fields are refused on purpose (§5.5): a typo'd key that \
                     was\n       silently ignored would turn a costed backtest into a costless \
                     one."
                );
            }
            return EXIT_USAGE;
        }
    };

    let hash_only = args.hash_only;
    if !hash_only {
        print_header(&loaded, "combo");
    }
    if !args.run && !hash_only {
        print_grid_listing(&loaded);
        print_footer(&loaded);
        return 0;
    }

    let series = match collect_events(&loaded) {
        Ok(series) => series,
        Err((code, message)) => {
            eprintln!("error: {message}");
            return code;
        }
    };
    let events = series.events;
    if events.is_empty() {
        eprintln!("error: the data source produced no bars; there is nothing to replay");
        return EXIT_USAGE;
    }

    let bars_per_year = annualization(&loaded, &events);
    let results: Vec<Replay> = (0..loaded.grid.len())
        .map(|index| replay(&loaded, &events, bars_per_year, index))
        .collect();

    if hash_only {
        println!("{:016x}", grid_hash(&results));
        return 0;
    }

    print_run_context(&loaded, &events, bars_per_year);
    print_results(&loaded, &results);
    println!("  determinism hash {:016x}", grid_hash(&results));
    print_path_sensitivity(
        results.iter().map(|r| r.result.n_protective_exits).sum(),
        results.iter().map(|r| r.result.path_sensitive_bars).sum(),
    );
    print_footer(&loaded);
    0
}

/// One combo's replay, plus the two numbers that only the alignment wrapper
/// and the strategy itself can report.
struct Replay {
    index: usize,
    label: String,
    own_warmup_bars: usize,
    suppressed_intents: usize,
    conflicting_signals: usize,
    result: BacktestResult,
}

fn replay(
    loaded: &LoadedConfig,
    events: &[MarketEvent],
    bars_per_year: f64,
    index: usize,
) -> Replay {
    let combo = loaded.grid.combo(index);
    let mut strategy = loaded.grid.aligned_strategy(index);
    let mut feed = SliceFeed { events, at: 0 };
    let params = BacktestParams {
        initial_cash_nano_usd: loaded.initial_cash_nano_usd,
        bars_per_year,
    };
    let result = run_with_fill_model(loaded, &mut feed, &mut strategy, &params);
    Replay {
        index,
        label: combo.label(),
        own_warmup_bars: combo.own_warmup_bars(),
        suppressed_intents: strategy.suppressed_intents(),
        conflicting_signals: strategy.inner().conflicting_signals(),
        result,
    }
}

/// The fill model is a config value, so the branch is here rather than in a
/// generic parameter threaded through everything.
fn run_with_fill_model(
    loaded: &LoadedConfig,
    feed: &mut SliceFeed<'_>,
    strategy: &mut Aligned<ComboStrategy>,
    params: &BacktestParams,
) -> BacktestResult {
    let expect = "INVARIANT: a collected bar series is already availability-ordered";
    if loaded.file.execution.fill_model == "free_fills" {
        run(feed, strategy, &mut FreeFills, &loaded.spec, params).expect(expect)
    } else {
        let mut fills = loaded.spread_cross_fills();
        run(feed, strategy, &mut fills, &loaded.spec, params).expect(expect)
    }
}

/// Replays a pre-collected bar series.
///
/// Collecting once and replaying the same slice per combo is what makes "the
/// same bars for every combo" (§2.6) literal rather than aspirational: there
/// is one series in memory, and a re-`open` of a Parquet file cannot drift
/// from it because there is no re-`open`.
struct SliceFeed<'a> {
    events: &'a [MarketEvent],
    at: usize,
}

impl Feed for SliceFeed<'_> {
    fn next_event(&mut self) -> Option<MarketEvent> {
        let event = self.events.get(self.at)?.clone();
        self.at += 1;
        Some(event)
    }
}

/// One bar series, with the provenance §2.5 makes a stored result carry.
pub(crate) struct Series {
    /// The bars, in availability order.
    pub events: Vec<MarketEvent>,
    /// blake3 of every archived raw file the curated partitions came from —
    /// the "data manifest ids" D-0013 requires and D-0014 defines. Empty for a
    /// synthetic feed, which is generated rather than archived, and whose
    /// whole provenance is the seed named in [`Series::description`].
    pub data_manifest_ids: Vec<String>,
    /// What the bars were, in one line, for a report header.
    pub description: String,
}

/// Materializes the config's data source into one bar series.
pub(crate) fn collect_events(loaded: &LoadedConfig) -> Result<Series, (i32, String)> {
    let instrument = &loaded.file.universe.instruments[0];
    match &loaded.file.data {
        DataSource::Synthetic {
            seed,
            bars,
            start_price_points,
            vol_ticks,
        } => {
            if instrument != "SYN:RW" {
                return Err((
                    EXIT_USAGE,
                    format!(
                        "universe.instruments is [{instrument:?}] but the synthetic random walk \
                         emits SYN:RW. A contract spec labelled with an instrument the feed \
                         never yields is a header that lies"
                    ),
                ));
            }
            let start = Price::from_points_str(start_price_points)
                .map_err(|e| (EXIT_USAGE, format!("data.start_price_points: {e}")))?;
            if start.as_nanos() <= 0 || start.as_nanos() % loaded.spec.tick.as_nanos() != 0 {
                return Err((
                    EXIT_USAGE,
                    "data.start_price_points must be positive and a whole number of ticks"
                        .to_owned(),
                ));
            }
            let mut feed = SyntheticFeed::random_walk(
                *seed,
                *bars,
                loaded.timeframe,
                start,
                loaded.spec.tick,
                *vol_ticks,
            );
            Ok(Series {
                events: std::iter::from_fn(|| feed.next_event()).collect(),
                // A generated series has no archived bytes to name. Its
                // provenance is the seed, which is in the description below —
                // an empty manifest list here is a fact, not an omission, and
                // the scorecard says which one it is.
                data_manifest_ids: Vec::new(),
                description: format!(
                    "synthetic random walk — seed {seed}, {bars} bars, from \
                     {start_price_points} pt, ±{vol_ticks} ticks"
                ),
            })
        }
        DataSource::Curated { start, end } => {
            // Still refused, for a *different* reason than before D-0076.
            //
            // The old reason — nothing could say which of the two price series
            // it wanted — is gone: a `Bar` now carries both, indicators read
            // the signal view and PnL the tradeable one, and `crucible
            // backtest --instrument ES.v.0` replays a stitched series.
            //
            // What has not gone away is that the rule grammar can write
            // `close > 4500`, and on a back-adjusted series a *level* is not a
            // level: where a 2010 bar sits depends on the sum of every roll
            // gap after it, which had not happened yet (§2.1). A shift-
            // invariant rule — an SMA crossover — is unaffected, and the
            // grammar cannot tell the two apart, so the grid would silently
            // mix a sound combo with a leaking one and rank them together.
            // `backtest` runs one named strategy an operator chose; a grid
            // runs whatever the config expands to.
            if ContinuousAlias::looks_like(instrument) {
                return Err((
                    EXIT_USAGE,
                    format!(
                        "{instrument} is a continuous alias, and `combo` / `walk-forward` run \
                         raw contracts (ESH2024) only (D-0076). `backtest` replays a stitched \
                         series, because it runs one strategy an operator named; a grid \
                         expands rules it has not seen, and a rule comparing a price to an \
                         absolute constant is not safe on a back-adjusted series — the level \
                         a bar sits at is the sum of every roll gap after it, which had not \
                         happened at that bar (§2.1)"
                    ),
                ));
            }
            let s = parse_civil_date(start).ok_or_else(|| {
                (
                    EXIT_USAGE,
                    format!("data.start {start:?} is not a YYYY-MM-DD calendar date"),
                )
            })?;
            let e = parse_civil_date(end).ok_or_else(|| {
                (
                    EXIT_USAGE,
                    format!("data.end {end:?} is not a YYYY-MM-DD calendar date"),
                )
            })?;
            let range = range_from_dates((s.year, s.month, s.day), (e.year, e.month, e.day))
                .map_err(|err| (EXIT_USAGE, err.to_string()))?;
            let dir = data_dir().map_err(|msg| (EXIT_USAGE, msg))?;
            let id = InstrumentId::new(instrument);
            let mut feed =
                ParquetBarFeed::open(&dir, &id, loaded.timeframe, Some(range)).map_err(|e| {
                    let code = match e {
                        CuratedError::NoCuratedData { .. } | CuratedError::EmptyRange { .. } => {
                            EXIT_USAGE
                        }
                        _ => EXIT_FAILED,
                    };
                    (code, e.to_string())
                })?;
            // The manifest ids §2.5 requires, taken from the curated files'
            // own metadata: each names exactly one raw file's blake3 (D-0036),
            // which IS its manifest id (D-0014). Deduplicated and sorted,
            // because a result's provenance is a set and a set printed in file
            // order would change with the filesystem.
            let mut data_manifest_ids: Vec<String> = feed
                .sources()
                .iter()
                .map(|s| s.source_file_blake3.clone())
                .collect();
            data_manifest_ids.sort_unstable();
            data_manifest_ids.dedup();
            Ok(Series {
                description: format!(
                    "curated {instrument} {} {start}..{end} — {} source file(s)",
                    loaded.timeframe,
                    data_manifest_ids.len()
                ),
                data_manifest_ids,
                events: std::iter::from_fn(|| feed.next_event()).collect(),
            })
        }
    }
}

/// Where the Sharpe annualization factor comes from.
///
/// A synthetic feed has a bar for every interval by construction, so the
/// answer is exact arithmetic rather than a measurement. Archived `ohlcv` data
/// does not, so it takes the same precedence `backtest` uses (D-0039):
/// calendar if one governs the instrument, otherwise the sample.
pub(crate) fn annualization(loaded: &LoadedConfig, events: &[MarketEvent]) -> f64 {
    let tf = loaded.timeframe;
    #[expect(clippy::cast_precision_loss, reason = "statistics space (§2.3)")]
    let interval = tf.duration_ns() as f64;
    match &loaded.file.data {
        DataSource::Synthetic { .. } => NANOS_PER_YEAR / interval,
        DataSource::Curated { .. } => match Calendar::for_instrument(&loaded.spec.instrument) {
            Ok(Some(cal)) => cal.bars_per_year(tf),
            _ => sample_bars_per_year(events, tf),
        },
    }
}

fn sample_bars_per_year(events: &[MarketEvent], tf: TimeFrame) -> f64 {
    let (Some(first), Some(last)) = (events.first(), events.last()) else {
        return 0.0;
    };
    let MarketEvent::Bar(first) = first;
    let MarketEvent::Bar(last) = last;
    let span_ns = (last.ts_open.0 - first.ts_open.0) + tf.duration_ns();
    if span_ns <= 0 {
        return 0.0;
    }
    #[expect(clippy::cast_precision_loss, reason = "statistics space (§2.3)")]
    let (bars, span) = (events.len() as f64, span_ns as f64);
    bars * NANOS_PER_YEAR / span
}

/// FNV-1a over every combo's equity curve, in index order.
///
/// Index order, never completion order: §2.2 requires parallel results to
/// merge by run identity, and a hash that depends on scheduling is not a
/// determinism gate.
fn grid_hash(results: &[Replay]) -> u64 {
    let mut h = crate::Fnv64::new();
    for replay in results {
        h.write_i64(i64::try_from(replay.index).unwrap_or(i64::MAX));
        for &(ts, equity) in &replay.result.equity {
            h.write_i64(ts.0);
            h.write_i64(equity);
        }
    }
    h.finish()
}

pub(crate) fn usd(n: NanoUsd) -> String {
    format!("${:.2}", nano_usd_to_f64(n))
}

pub(crate) fn print_header(loaded: &LoadedConfig, command: &str) {
    let file = &loaded.file;
    println!("Crucible {command} — {}", file.meta.name);
    println!(
        "  config         {}  (schema v{})",
        loaded.path.display(),
        file.schema_version
    );
    println!(
        "  config hash    {}  (blake3 of the canonical form, D-0012)",
        loaded.config_hash
    );
    println!("  family         {}", file.meta.hypothesis_family);
    println!("  rationale      {}", file.meta.economic_rationale);
    println!();
    println!(
        "  universe       {} {}",
        file.universe.instruments[0], loaded.timeframe
    );
    match &file.data {
        DataSource::Curated { start, end } => {
            println!("  data           curated {start} .. {end}");
        }
        DataSource::Synthetic {
            seed,
            bars,
            start_price_points,
            vol_ticks,
        } => println!(
            "  data           synthetic random walk — seed {seed}, {bars} bars, \
             from {start_price_points} pt, ±{vol_ticks} ticks"
        ),
    }
    println!(
        "  contract       tick {} pt, ${}/pt",
        loaded.spec.tick, loaded.spec.point_value_usd
    );
    if file.execution.fill_model == "free_fills" {
        println!(
            "  fill model     free_fills — SCREENING ONLY (S0-S1, D-0006). Nothing below is a\n\
             \x20                result you may quote"
        );
    } else {
        println!(
            "  fill model     spread_cross — {} tick half-spread, {}/contract/side",
            file.execution.half_spread_ticks,
            usd(loaded.fee_per_contract_nano_usd)
        );
    }
    println!(
        "  capital        {} initial, {} contract(s)",
        usd(loaded.initial_cash_nano_usd),
        file.run.qty_contracts
    );
    println!();

    for (i, slot) in loaded.grid.spec().slots().iter().enumerate() {
        let axes: Vec<String> = slot
            .kind
            .param_names()
            .iter()
            .zip(slot.axis_lens())
            .map(|(name, points)| format!("{name} ({points})"))
            .collect();
        println!(
            "  {:<14} {} {} — {}",
            if i == 0 { "slots" } else { "" },
            slot.name,
            slot.kind.name(),
            axes.join(", ")
        );
    }
    let canonical = loaded.grid.spec().canonical_form();
    for (i, line) in canonical
        .lines()
        .filter(|l| l.starts_with("rule "))
        .enumerate()
    {
        println!(
            "  {:<14} {}",
            if i == 0 { "rules" } else { "" },
            line.trim_start_matches("rule ")
        );
    }
    println!();
    println!(
        "  grid           {} combos, warmup {} bars across the grid (§2.6: every combo's\n\
         \x20                eval window opens at bar {})",
        loaded.grid.len(),
        loaded.grid.max_warmup_bars(),
        loaded.grid.max_warmup_bars()
    );
    if loaded.grid.len() > LOUD_COMBO_COUNT {
        println!(
            "\n  WARNING: {} combos is past the {LOUD_COMBO_COUNT} where a grid stops being a\n\
             \x20          search and starts being a mistyped step. Coarse-to-fine finds the\n\
             \x20          same plateau for a thousandth of the trial count — and every combo\n\
             \x20          here is a trial charged to {}.",
            loaded.grid.len(),
            loaded.file.meta.hypothesis_family
        );
    }
    println!();
}

fn print_grid_listing(loaded: &LoadedConfig) {
    println!(
        "  {:>5}  {:<52} {:>10}",
        "combo", "parameters", "own warmup"
    );
    for combo in loaded.grid.iter().take(LIST_LIMIT) {
        println!(
            "  {:>5}  {:<52} {:>10}",
            combo.index,
            combo.label(),
            combo.own_warmup_bars()
        );
    }
    if loaded.grid.len() > LIST_LIMIT {
        println!("  {:>5}  … {} more", "", loaded.grid.len() - LIST_LIMIT);
    }
    println!();
}

fn print_run_context(loaded: &LoadedConfig, events: &[MarketEvent], bars_per_year: f64) {
    let eval_bars = events.len().saturating_sub(loaded.grid.max_warmup_bars());
    println!(
        "  replay         {} bars, one series shared by every combo; {eval_bars} of them\n\
         \x20                inside the evaluation window",
        events.len()
    );
    println!("  annualization  {bars_per_year:.0} bars/yr");
    println!();
}

fn print_results(loaded: &LoadedConfig, results: &[Replay]) {
    println!(
        "  {:>5}  {:<40} {:>14} {:>9} {:>8} {:>7} {:>8} {:>6}",
        "combo", "parameters", "final equity", "return", "max DD", "trades", "Sharpe", "supp."
    );
    for r in results {
        let s = &r.result.summary;
        let sharpe = s
            .sharpe_naive
            .map_or_else(|| "     n/a".to_owned(), |x| format!("{x:8.2}"));
        println!(
            "  {:>5}  {:<40} {:>14} {:>8.2}% {:>7.2}% {:>7} {} {:>6}",
            r.index,
            r.label,
            usd(s.final_equity_nano_usd),
            s.total_return_pct,
            s.max_drawdown_pct,
            s.round_trips,
            sharpe,
            r.suppressed_intents
        );
    }
    println!();

    let suppressed: usize = results.iter().map(|r| r.suppressed_intents).sum();
    let shortest = results
        .iter()
        .map(|r| r.own_warmup_bars)
        .min()
        .unwrap_or_default();
    println!(
        "  `supp.` is orders dropped because the combo was warm before the grid was:\n\
         \x20 {suppressed} across the grid, all from combos whose own warmup is as short as\n\
         \x20 {shortest} bars against the grid's {}. Those are exactly the trades a short\n\
         \x20 combo would have won on a longer sample than its neighbours had (§2.6).",
        loaded.grid.max_warmup_bars()
    );
    let conflicts: usize = results.iter().map(|r| r.conflicting_signals).sum();
    if conflicts > 0 {
        println!(
            "\n  WARNING: enter_long and enter_short were both true on {conflicts} bar(s) with a\n\
             \x20          flat position. No position was taken on any of them. Two entry rules\n\
             \x20          that can fire together is a bug in the config, not a signal."
        );
    }
    let cancelled: usize = results.iter().map(|r| r.result.cancelled_at_eof).sum();
    if cancelled > 0 {
        println!(
            "\n  {cancelled} order(s) were still pending when the series ended, and were cancelled."
        );
    }
}

/// The path-sensitivity flag, summed over a grid.
///
/// Printed even when it is zero, and printed with the *reason* it is zero:
/// "there were no ambiguous bars" and "nothing here can have ambiguous bars"
/// are different facts, and a reader who cannot tell them apart does not know
/// whether the grid's returns depend on an intrabar convention. Shared by
/// `combo` and `walk-forward` so the two cannot drift.
pub(crate) fn print_path_sensitivity(exits: usize, sensitive: usize) {
    if exits == 0 {
        println!(
            "\n  intrabar       no combo declared a stop or target, so no exit above is\n\
             \x20                path-dependent and {INTRABAR_CONVENTION} never had to choose.\n\
             \x20                Brackets reach a replay through `crucible backtest\n\
             \x20                --stop-ticks/--target-ticks` in this build; the combo grammar\n\
             \x20                does not carry them yet (D-0069)."
        );
        return;
    }
    println!(
        "\n  intrabar       {INTRABAR_CONVENTION} — {sensitive} of {exits} stop/target exit(s)\n\
         \x20                across the grid came from a bar that touched BOTH levels, where the\n\
         \x20                convention chose the outcome rather than the data (D-0069)."
    );
}

fn print_footer(loaded: &LoadedConfig) {
    println!("  not consumed by `combo`:");
    for section in loaded.unconsumed_sections(config::Consumer::Combo) {
        println!("    {section}");
    }
    println!(
        "\n  Every number above is measured over the WHOLE series, warmup prefix included:\n\
         \x20 each combo sits flat for the grid's {} warmup bars and its naive Sharpe carries\n\
         \x20 the same sqrt(n_eval/n_total) factor for it (D-0061). That is fair *within* the\n\
         \x20 grid — the factor is identical across it — but it is not a number to quote.\n\
         \x20 `crucible walk-forward` computes each statistic on the window it names.",
        loaded.grid.max_warmup_bars()
    );
    println!(
        "\n  No folds, no stages, no trial count, no verdict: `combo` proves a config\n\
         \x20 expands and replays fairly, which is a different question from whether the\n\
         \x20 idea is real. Deflated Sharpe, PBO and the permutation battery arrive with\n\
         \x20 the funnel (M3)."
    );
}
