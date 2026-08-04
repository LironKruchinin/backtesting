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
use crucible_data::calendar::{Calendar, SessionClock, SessionId};
use crucible_data::continuous::ContinuousAlias;
use crucible_data::curated::CuratedError;
use crucible_data::curated::resample::ResampleError;
use crucible_data::ingest::range_from_dates;
use crucible_data::ingest::window::parse_civil_date;
use crucible_engine::{BacktestParams, BacktestResult, FreeFills, INTRABAR_CONVENTION, run};
use crucible_strategies::Aligned;
use crucible_strategies::combo::{
    ComboStrategy, SessionField, SessionPhase, SessionPosition, SessionSeries,
};

use crate::config::{self, ConfigError, DataSource, LoadedConfig};
use crate::grain::{CuratedGrain, grain_caveats, grain_line};
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
    let mut loaded = match config::load(&args.config) {
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
    let events = &series.events;
    if events.is_empty() {
        eprintln!("error: the data source produced no bars; there is nothing to replay");
        return EXIT_USAGE;
    }
    if let Err(message) = attach_sessions(&mut loaded, events) {
        eprintln!("error: {message}");
        return EXIT_USAGE;
    }

    let bars_per_year = annualization(&loaded, events);
    let results: Vec<Replay> = (0..loaded.grid.len())
        .map(|index| replay(&loaded, events, bars_per_year, index))
        .collect();

    if hash_only {
        println!("{:016x}", grid_hash(&results));
        return 0;
    }

    print_run_context(&loaded, &series, bars_per_year);
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
    session_gaps: usize,
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
        session_gaps: strategy.inner().session_gaps(),
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
    /// Anything about the series a reader has to know and would not guess —
    /// today, the two edge facts a resampled series carries (D-0077). Empty is
    /// the normal case and means there is nothing to say, not that nobody
    /// looked.
    pub caveats: Vec<String>,
}

/// Materializes the config's data source into one bar series.
///
/// The single-instrument entry point, and the one every command uses today: it
/// reads `universe.instruments[0]`, which is the *only* instrument a config
/// without `[pooling]` is allowed to declare (`config::validate_pooling`).
pub(crate) fn collect_events(loaded: &LoadedConfig) -> Result<Series, (i32, String)> {
    collect_events_for(loaded, &loaded.file.universe.instruments[0])
}

/// Materializes one *named* instrument from the config's data source.
///
/// Split out of [`collect_events`] for block C: a pooled run replays several
/// contracts of one root through this function, once per contract, rather than
/// teaching the loader to return many series. The instrument is a parameter and
/// everything else — grain, window, contract spec, refusals — still comes from
/// the config, which is what keeps §2.6 honest across a pool: every contract is
/// read the same way, so no contract can gain an edge from being loaded
/// differently.
///
/// **This is a pure refactor and moves no determinism hash.** `collect_events`
/// delegates here with `instruments[0]`, which is exactly what it read before,
/// so the combo, walk-forward and funnel gates must be byte-identical across
/// this commit — and those three are among the five that no test asserts, so
/// they are compared by value against `docs/MILESTONES.md` rather than by
/// "the gate produced output" (D-0118).
pub(crate) fn collect_events_for(
    loaded: &LoadedConfig,
    instrument: &str,
) -> Result<Series, (i32, String)> {
    collect_events_in_window(loaded, instrument, None)
}

/// One instrument over an *explicitly named* evaluation window (C2).
///
/// `window` is `Some((start, end))` to replace `[data].start`/`[data].end` for
/// this contract alone — inclusive start, exclusive end, like the config's own
/// pair — and `None` to use the config's window unchanged.
///
/// **Why the window is a parameter rather than "the whole curated span".**
/// A pooled run does not replay each contract over its full curated life:
/// ESH2024 carries 239 curated trading days but is *front month* for 64 of
/// them, and the ruling recorded at C3 counts front-month sessions. Deferred
/// bars are largely an echo of the front month's price process — they add `n`
/// without adding information — and `spread_cross`'s half-spread is calibrated
/// for the liquid contract, so replaying them under it assumes a book that was
/// not there (§2.4). C3 supplies each contract's front window from the `.v`
/// roll table; this function is the seam that accepts it.
///
/// Warmup is unaffected, deliberately: a contract's pre-front bars are its own
/// real bars and remain legitimate warmup input, exactly as D-0062 already
/// treats warmup before an eval window. The window governs evaluation, session
/// counting and trade attribution — not what the indicators may warm on.
pub(crate) fn collect_events_in_window(
    loaded: &LoadedConfig,
    instrument: &str,
    window: Option<(&str, &str)>,
) -> Result<Series, (i32, String)> {
    match &loaded.file.data {
        DataSource::Synthetic {
            seed,
            bars,
            start_price_points,
            vol_ticks,
        } => {
            // A generated series is defined by its seed and bar count; there
            // is no calendar window to narrow. Silently ignoring one would let
            // a pooled synthetic config believe it had per-contract windows it
            // never got — a run of a different experiment than the one asked
            // for, which is the D-0075 shape.
            if window.is_some() {
                return Err((
                    EXIT_USAGE,
                    "a synthetic data source has no calendar window to narrow: it is defined by                      its seed and bar count. A per-contract evaluation window is a curated-only                      concept (C2)"
                        .to_owned(),
                ));
            }
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
                // A generated feed emits the requested grain directly, so
                // there is no aggregation to caveat.
                caveats: Vec::new(),
            })
        }
        DataSource::Curated { start, end } => {
            // A named window wins over the config's. Synthetic data has no
            // window to narrow — a generated series is defined by its seed and
            // bar count — so the override is curated-only and the synthetic arm
            // refuses one rather than ignoring it.
            let (start, end) = match window {
                Some((s, e)) => (s, e),
                None => (start.as_str(), end.as_str()),
            };
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
            // The archive holds 1s and 1m; anything coarser is aggregated on
            // the exchange's sessions when it is read (D-0077). Which of the
            // two happened is decided in one place and printed, never assumed.
            let mut feed =
                CuratedGrain::open(&dir, &id, loaded.timeframe, Some(range)).map_err(|e| {
                    let code = match &e {
                        ResampleError::Curated(
                            CuratedError::NoCuratedData { .. } | CuratedError::EmptyRange { .. },
                        )
                        | ResampleError::NoCalendar { .. }
                        | ResampleError::NotCoarser { .. } => EXIT_USAGE,
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
            // The grain belongs in the description rather than beside it: this
            // string is what the registry and the scorecard store as
            // `data_source`, and "5m as delivered" and "5m aggregated here from
            // 1m on cme_globex_equity_index sessions" are different bars (§2.5).
            let grain = grain_line(&feed, loaded.timeframe);
            let caveats = grain_caveats(&feed);
            Ok(Series {
                description: format!(
                    "curated {instrument} {start}..{end} — {grain}, {} source file(s)",
                    data_manifest_ids.len()
                ),
                data_manifest_ids,
                caveats,
                events: std::iter::from_fn(|| feed.next_event()).collect(),
            })
        }
    }
}

/// Computes every bar's session clock **once** and attaches it to the grid.
///
/// The D-0071 device, applied to a second kind of key (D-0078).
/// `crucible-strategies` may not depend on `crucible-data`, so it cannot know
/// what a timezone is; `crucible-engine` may not either. The CLI is the one
/// place that holds both a `Calendar` and a `Grid`, so it does the arithmetic
/// once, in bar order, and every combo in the grid reads the same slice. Two
/// combos scoring the same bar therefore cannot disagree about what time it
/// was — which is the failure the device exists to prevent, in the same shape
/// as a daily-loss-limit breach landing on two different dates.
///
/// Keyed on `avail_ts`, never `ts_open` (§2.1): a rule fires when a bar
/// completes, and the order it emits fills on the next one.
///
/// # Errors
/// A message naming the problem when the config's rules read the session clock
/// and nothing can supply one. That is a refusal rather than a run with silent
/// rules: a `minutes_since_open < 30` that has no opinion on any bar produces a
/// backtest of a *different strategy* than the config describes, and it looks
/// exactly like a strategy that never found a signal.
pub(crate) fn attach_sessions(
    loaded: &mut LoadedConfig,
    events: &[MarketEvent],
) -> Result<(), String> {
    let needed = loaded.grid.spec().rules().uses_session();
    let calendar = Calendar::for_instrument(&loaded.spec.instrument)
        .map_err(|e| format!("bundled calendar tables are broken: {e}"))?;
    let Some(calendar) = calendar else {
        if needed {
            return Err(format!(
                "the rules read the session clock ({}), but no bundled calendar governs {}. \
                 A synthetic feed has no exchange and therefore no session, so every such rule \
                 would be silent on every bar — which is a backtest of a different strategy \
                 than this config describes, and looks exactly like one that found no signal",
                SessionField::all()
                    .iter()
                    .map(|f| f.name())
                    .collect::<Vec<_>>()
                    .join(", "),
                loaded.spec.instrument
            ));
        }
        return Ok(());
    };

    let series: SessionSeries = events
        .iter()
        .map(|ev| session_position(calendar.session_clock(ev.avail_ts())))
        .collect();
    loaded.grid.attach_sessions(series);
    Ok(())
}

/// The six lines where `crucible-data`'s session clock becomes the rule
/// grammar's, and the only place the two crates meet.
///
/// Nanoseconds to minutes is the one conversion, and it happens here because
/// this is the boundary into indicator space where §2.3 puts `f64`;
/// `crucible-data` keeps the exact integers. The phase mapping is one-to-one
/// and total — a new `SessionId` variant must fail to compile here rather than
/// fall into a default, which is why there is no `_ =>` arm.
fn session_position(clock: SessionClock) -> SessionPosition {
    let minutes = |ns: i64| {
        #[expect(clippy::cast_precision_loss, reason = "indicator space (§2.3)")]
        let ns = ns as f64;
        ns / 60e9
    };
    SessionPosition {
        minutes_since_open: minutes(clock.since_open_ns),
        minutes_to_close: minutes(clock.to_close_ns),
        minutes_since_rth_open: minutes(clock.since_rth_open_ns),
        minutes_to_rth_close: minutes(clock.to_rth_close_ns),
        phase: match clock.session {
            SessionId::Overnight => SessionPhase::Overnight,
            SessionId::Regular => SessionPhase::Regular,
            SessionId::PostRegular => SessionPhase::PostRegular,
            SessionId::Closed => SessionPhase::Closed,
        },
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

fn print_run_context(loaded: &LoadedConfig, series: &Series, bars_per_year: f64) {
    let events = &series.events;
    let eval_bars = events.len().saturating_sub(loaded.grid.max_warmup_bars());
    println!("  bars           {}", series.description);
    println!(
        "  replay         {} bars, one series shared by every combo; {eval_bars} of them\n\
         \x20                inside the evaluation window",
        events.len()
    );
    println!("  annualization  {bars_per_year:.0} bars/yr");
    println!();
    crate::grain::print_caveats(&series.caveats);
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
    let gaps: usize = results.iter().map(|r| r.session_gaps).sum();
    if gaps > 0 {
        println!(
            "
  WARNING: a session-relative rule had no clock reading on {gaps} bar(s) and was
                        silent there. `attach_sessions` is supposed to make that impossible, so
                        this is a bug, not a data caveat — the numbers above are a different
                        strategy from the one the config describes (D-0078)."
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Seconds since the epoch, as a `Ts`.
    fn utc(seconds: i64) -> Ts {
        Ts(seconds * 1_000_000_000)
    }

    fn cme() -> Calendar {
        Calendar::for_instrument(&InstrumentId::new("ESH2024"))
            .expect("bundled tables parse")
            .expect("CME governs ES")
    }

    /// The mapping is a unit conversion and a one-to-one relabel, and both are
    /// checked against instants derived by hand.
    ///
    /// Trading day 2024-01-03 opens 2024-01-02 17:00 CST = 23:00Z
    /// (1_704_236_400) and closes 2024-01-03 16:00 CST = 22:00Z
    /// (1_704_319_200) — 1380 minutes. Regular hours are 08:30–15:00 CST, i.e.
    /// 14:30Z (1_704_292_200) to 21:00Z (1_704_315_600).
    ///
    /// The bar under test becomes available at 10:00 CST = 16:00Z
    /// (1_704_297_600): 1020 minutes into the session, 360 from its close, 90
    /// past the regular open and 300 from the regular close, inside RTH.
    #[test]
    fn a_session_clock_becomes_the_grammars_reading() {
        let position = session_position(cme().session_clock(utc(1_704_297_600)));
        assert!((position.minutes_since_open - 1020.0).abs() < 1e-9);
        assert!((position.minutes_to_close - 360.0).abs() < 1e-9);
        assert!((position.minutes_since_rth_open - 90.0).abs() < 1e-9);
        assert!((position.minutes_to_rth_close - 300.0).abs() < 1e-9);
        assert_eq!(position.phase, SessionPhase::Regular);
    }

    /// The same wall-clock reading on the bundled table's real early close, and
    /// the ordinary day beside it.
    ///
    /// CME closes at 12:00 CT on Independence Day itself — 2024-07-04, a
    /// Thursday. That trading day opens 2024-07-03 17:00 CDT = 22:00Z
    /// (1_720_044_000) and closes 2024-07-04 12:00 CDT = 17:00Z
    /// (1_720_112_400), so it is 19 hours = 1140 minutes long instead of 1380.
    ///
    /// A bar available at 11:00 CDT that day (16:00Z, 1_720_108_800) is
    /// therefore **60** minutes from the close. The identical wall-clock bar on
    /// the ordinary Thursday a week earlier — 2024-06-27, 16:00Z,
    /// 1_719_504_000 — is **300**. Both are 1080 minutes into their session,
    /// because an early close moves the close and never the open.
    ///
    /// A rule written against a fixed 16:00 close would have tried to flatten
    /// four hours after the market shut, on exactly the days when being
    /// positioned into an illiquid close costs the most.
    #[test]
    fn the_clock_counts_down_to_the_bundled_tables_early_close() {
        let cal = cme();
        assert!(matches!(
            cal.day_effect(crucible_data::calendar::CivilDate {
                year: 2024,
                month: 7,
                day: 4
            }),
            crucible_data::calendar::DayEffect::EarlyClose { .. }
        ));

        let holiday = session_position(cal.session_clock(utc(1_720_108_800)));
        assert!((holiday.minutes_since_open - 1080.0).abs() < 1e-9);
        assert!((holiday.minutes_to_close - 60.0).abs() < 1e-9);

        let ordinary = session_position(cal.session_clock(utc(1_719_504_000)));
        assert!((ordinary.minutes_since_open - 1080.0).abs() < 1e-9);
        assert!((ordinary.minutes_to_close - 300.0).abs() < 1e-9);
    }

    /// The last bar of a session reads its own session, not `Closed`.
    ///
    /// A bar whose interval ends exactly at 16:00 CST on 2024-01-03 traded
    /// entirely inside that session. Reported as `Closed`, "flatten on the last
    /// bar" would be a rule that never fires; reported as `PostRegular` with
    /// `minutes_to_close == 0`, it fires exactly once.
    #[test]
    fn the_last_bar_of_a_session_is_still_in_it() {
        let position = session_position(cme().session_clock(utc(1_704_319_200)));
        assert!((position.minutes_to_close - 0.0).abs() < 1e-9);
        assert_eq!(position.phase, SessionPhase::PostRegular);
    }
}
