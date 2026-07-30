//! TOML → [`ComboSpec`]: the config seam, and the only place `serde` meets a
//! strategy.
//!
//! D-0060 put this here rather than in `crucible-strategies`, which must
//! compile with no I/O and no dependencies beyond `crucible-core` forever
//! (CLAUDE.md §3). The structs below are a **1:1 transcription** of the file
//! and do no judging of their own: they hand the transcription to
//! [`ComboSpec::new`], which owns every domain rule, so a refusal reads the
//! same whether the config came from disk or from a unit test. The funnel
//! inherits this module in M3.
//!
//! Two policies are enforced here rather than downstream:
//!
//! - **`deny_unknown_fields` on every struct** (§5.5). A typo'd key must be a
//!   hard error at load. `half_spread_tick` silently becoming a no-op is how
//!   a research pipeline reports a costless backtest as a costed one.
//! - **Money is a decimal string, never a TOML float** (D-0027).
//!   `fee_per_contract_usd = "1.25"` parses straight to nanodollars;
//!   `= 1.25` is refused, because `1.25` is exact but `0.1` is not, and a
//!   money field that is sometimes exact is worse than one that never is.
//!
//! Sections a given command does not consume (`[funnel]` in every build so
//! far; `[walk_forward]` under `crucible combo`, which reports one window and
//! has no folds to cut) still parse and are still checked for typos — and
//! every report **names them in its output** rather than reading past them,
//! because a declared kill criterion that nothing evaluates is the quietest
//! way for a config to lie. [`LoadedConfig::unconsumed_sections`] takes which
//! command is asking, so the list is true per command rather than true on
//! average.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crucible_core::prelude::*;
use crucible_data::ingest::money::parse_usd_to_nano;
use crucible_engine::SpreadCrossFills;
use crucible_funnel::stages::{Criteria, CriteriaError, CriteriaSource};
use crucible_funnel::walkforward::{FoldError, FoldSpec};
use crucible_strategies::combo::{
    ComboError, ComboSpec, ConfigHash, FloatAxis, Grid, IndicatorSpec, IntAxis, RuleSource,
};
use crucible_strategies::indicators::RollingSource;
use serde::Deserialize;

/// The only schema version this build understands.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// A config file, transcribed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    /// Must be [`SUPPORTED_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Who this idea is and what it claims.
    pub meta: Meta,
    /// What it trades.
    pub universe: Universe,
    /// Where the bars come from.
    pub data: DataSource,
    /// The contract's tick and point value — named, never implied (§2.4).
    pub contract: Contract,
    /// Indicator slots, keyed by the name rules reference them with.
    pub indicators: BTreeMap<String, IndicatorCfg>,
    /// The four rules.
    pub rules: RulesCfg,
    /// The execution assumption.
    pub execution: Execution,
    /// The fold layout. Consumed by `crucible walk-forward`; named as
    /// unconsumed by every other command that prints a report.
    pub walk_forward: Option<WalkForward>,
    /// What S0 measures, required when — and only when — `[funnel].stages`
    /// declares `s0` (D-0085).
    pub s0: Option<S0Cfg>,
    /// Declared; consumed by `crucible funnel` (M3).
    pub funnel: Option<FunnelCfg>,
    /// Run-level settings.
    pub run: RunCfg,
}

/// `[meta]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    /// Human name for the idea.
    pub name: String,
    /// The trial-registry key every run under this idea is charged to.
    pub hypothesis_family: String,
    /// One sentence. If it cannot be written, the idea is not ready to test.
    pub economic_rationale: String,
}

/// `[universe]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Universe {
    /// Instruments to run against.
    pub instruments: Vec<String>,
    /// Bar intervals to run against.
    pub timeframes: Vec<String>,
}

/// `[data]`, tagged by `source`.
#[derive(Debug, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum DataSource {
    /// Archived Parquet bars.
    Curated {
        /// Inclusive UTC start date, `YYYY-MM-DD`.
        start: String,
        /// Exclusive UTC end date, `YYYY-MM-DD`.
        end: String,
    },
    /// A seeded random walk — the null harness (D-0011).
    Synthetic {
        /// Seeds the bar generator.
        seed: u64,
        /// How many bars to generate.
        bars: usize,
        /// Opening price in points, as decimal text.
        start_price_points: String,
        /// Random-walk step size, in ticks.
        vol_ticks: u64,
    },
}

/// `[contract]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    /// Minimum price increment in points, as decimal text (D-0038).
    pub tick_points: String,
    /// Dollars per point per contract.
    pub point_value_usd: i64,
}

/// `[indicators.<name>]`, tagged by `kind`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IndicatorCfg {
    /// Simple moving average.
    Sma {
        /// Window length in bars.
        period: IntAxisCfg,
    },
    /// Exponential moving average.
    Ema {
        /// Smoothing length in bars.
        period: IntAxisCfg,
    },
    /// Bollinger bands.
    Bollinger {
        /// Window length in bars.
        period: IntAxisCfg,
        /// Band width in population standard deviations.
        k: FloatAxisCfg,
    },
    /// Trailing-window z-score of a declared source (D-0080).
    Zscore {
        /// Window length in bars.
        period: IntAxisCfg,
        /// `close`, `volume` or `return`. **Required**: "the z-score of what"
        /// is not a detail worth defaulting, and two slots differing only in it
        /// are two different features.
        source: String,
    },
    /// Trailing-window population standard deviation of a declared source
    /// (D-0080).
    Stdev {
        /// Window length in bars.
        period: IntAxisCfg,
        /// `close`, `volume` or `return`.
        source: String,
    },
}

/// An integer axis: `20`, `[10, 20]`, or `{ start, end, step }`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IntAxisCfg {
    /// A scalar — an axis of one.
    Fixed(usize),
    /// Values written out.
    List(Vec<usize>),
    /// Inclusive range.
    Range {
        /// First value.
        start: usize,
        /// Inclusive upper bound.
        end: usize,
        /// Increment.
        step: usize,
    },
}

/// A float axis. The `Range` form parses and is then **refused** with an
/// explanation (see [`ComboError::SteppedFloatAxis`]) — a config that writes
/// it has a reason to expect it to work and deserves better than
/// `unknown field`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FloatAxisCfg {
    /// A scalar.
    Fixed(f64),
    /// Values written out.
    List(Vec<f64>),
    /// Inclusive range — parsed, then refused.
    Range {
        /// First value.
        start: f64,
        /// Inclusive upper bound.
        end: f64,
        /// Increment.
        step: f64,
    },
}

/// `[rules]`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesCfg {
    /// Opens a long when true and flat.
    pub enter_long: Option<String>,
    /// Closes a long.
    pub exit_long: Option<String>,
    /// Opens a short when true and flat.
    pub enter_short: Option<String>,
    /// Closes a short.
    pub exit_short: Option<String>,
}

/// `[execution]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    /// `free_fills` or `spread_cross`.
    pub fill_model: String,
    /// Half-spread a market order crosses, in ticks.
    pub half_spread_ticks: i64,
    /// Commission per contract per side, as decimal dollars in a string.
    pub fee_per_contract_usd: String,
}

/// `[walk_forward]` — consumed by `crucible walk-forward`.
///
/// Counted in **trading days**, not months (D-0062): a "6-month" window holds
/// between 122 and 130 CME sessions depending on where it lands, so a fold
/// layout written in months has a sample size chosen by the exchange's
/// holiday schedule rather than by the researcher.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkForward {
    /// `rolling` (fixed-length training window that slides) or `anchored`
    /// (training window that grows from a fixed start).
    pub scheme: String,
    /// In-sample trading days per fold.
    pub train_days: usize,
    /// Out-of-sample trading days per fold.
    pub test_days: usize,
    /// Trading days each fold advances. Defaults to `test_days`, which tiles
    /// the sample exactly; a smaller value is refused, because the headline
    /// pools the test windows and overlapping windows count a session twice.
    pub step_days: Option<usize>,
}

impl WalkForward {
    /// The fold layout this section describes.
    ///
    /// # Errors
    /// [`ConfigError::Folds`] if `scheme` is not a scheme this build knows.
    /// Everything else about the layout is judged by [`FoldPlan::build`],
    /// which is where the rules live — a refusal reads the same whether the
    /// layout came from a config or from a unit test.
    ///
    /// [`FoldPlan::build`]: crucible_funnel::FoldPlan::build
    pub fn to_fold_spec(&self) -> Result<FoldSpec, ConfigError> {
        Ok(FoldSpec {
            scheme: self.scheme.parse().map_err(ConfigError::Folds)?,
            train_days: self.train_days,
            test_days: self.test_days,
            step_days: self.step_days.unwrap_or(self.test_days),
        })
    }
}

/// `[funnel]` — the pre-registered criteria, consumed by `crucible funnel`.
///
/// Every field here is written **before** the run and judged by a machine
/// afterwards. That ordering is the whole point (`docs/PROJECT_PLAN.md` §7.2):
/// a threshold chosen after seeing the result is not a criterion, it is a
/// rationalization with a number in it. The funnel prints these back at the
/// top of every scorecard, next to the verdict they produced, so a reader can
/// check that the bar was set before the jump.
/// What the S0 predictor stage measures, declared **before** the run
/// (D-0085).
///
/// This block is required exactly when `[funnel].stages` contains `s0`, and
/// refused when it does not. A declared-but-unrun block is a config that thinks
/// it asked for something; a stage with no block is a stage with no criteria,
/// which is the thing pre-registration exists to prevent.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S0Cfg {
    /// The indicator slot the continuous score is read from — `"z"` for a
    /// scalar indicator, `"bb.upper"` for one with bands. The slot must be one
    /// of `[indicators]`.
    pub score: String,
    /// Forward horizons, in **minutes**. Written in minutes rather than bars
    /// because `ohlcv` has no bar for an interval that did not trade, so a bar
    /// count is a different question on every grain (D-0082).
    pub horizons_minutes: Vec<u32>,
    /// Equal-count quantile buckets of the score.
    pub buckets: usize,
    /// Block-bootstrap resamples, over whole sessions.
    pub bootstrap_draws: usize,
    /// **Pre-registered kill criterion**: the smallest `|IC|` at any declared
    /// horizon that counts as a relationship. Below it at every horizon, the
    /// score predicts nothing and nothing downstream can rescue it.
    pub min_abs_ic: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelCfg {
    /// Which stages to run — `s0` / `s1` / `s2` / `s3`. A stage this build
    /// does not implement is **refused**, not skipped: a config that asks for
    /// the permutation battery and silently gets a fold table has been
    /// answered with a different question than the one it asked.
    pub stages: Vec<String>,
    /// The mandatory cost-sensitivity sweep (§2.4), in ticks. Every entry must
    /// be a whole number of half-ticks — `0.0`, `0.5`, `1.0`, `2.0` — because
    /// that is what a price grid can express (D-0073).
    pub cost_sensitivity_ticks: Vec<f64>,
    /// **Admission** criterion: a combo with fewer out-of-sample round-trips
    /// than this is killed for having nothing to measure, before any
    /// performance number is looked at. Not `s0` — sample adequacy is a
    /// pre-trial check rather than the predictor gate (D-0084).
    pub min_oos_trades: usize,
    /// **Admission** criterion, the denominator §7.4 of the plan calls
    /// "N independent days": out-of-sample trading days pooled across folds.
    pub min_oos_sessions: usize,
    /// S1 criterion, applied under `free_fills`: a strategy that cannot make
    /// money cost-free is dead cheaply (D-0006).
    pub min_oos_return_pct_free_fills: f64,
    /// S2 criterion, applied under the config's own costed fill model.
    pub min_oos_sharpe_after_costs: f64,
    /// S2 criterion: the combo must still clear
    /// `min_oos_sharpe_after_costs` at this many ticks of half-spread, or the
    /// edge is a rounding error on the cost assumption.
    pub kill_if_dead_at_ticks: f64,
    /// S2 criterion: the combo must beat both mandatory controls
    /// (random-entry and buy-and-hold) on pooled out-of-sample return, or its
    /// number is the market's and not the strategy's.
    pub require_controls_beaten: bool,
    /// S3 criterion. Parsed and echoed; **not evaluated** by this build, and
    /// the report says so on every run — PBO needs the CSCV recombination
    /// machinery that lands with the rest of `crucible-funnel::stats`.
    pub max_pbo: f64,
    /// S3 criterion. Parsed and echoed; not evaluated by this build, for the
    /// same reason.
    pub require_plateau: bool,
}

impl FunnelCfg {
    /// The pre-registered criteria this section describes.
    ///
    /// # Errors
    /// [`ConfigError::Criteria`] if a stage is unknown or unimplemented, or if
    /// the mandatory cost sweep is incomplete or unrepresentable. Everything
    /// judgeable before a bar is replayed is judged here, because a criterion
    /// that turns out to be unevaluable after an hour of grid replay has
    /// already cost the hour.
    pub fn to_criteria(&self, s0_min_abs_ic: Option<f64>) -> Result<Criteria, ConfigError> {
        Criteria::new(&CriteriaSource {
            stages: &self.stages,
            cost_sensitivity_ticks: &self.cost_sensitivity_ticks,
            min_oos_trades: self.min_oos_trades,
            min_oos_sessions: self.min_oos_sessions,
            min_oos_return_pct_free_fills: self.min_oos_return_pct_free_fills,
            min_oos_sharpe_after_costs: self.min_oos_sharpe_after_costs,
            kill_if_dead_at_ticks: self.kill_if_dead_at_ticks,
            require_controls_beaten: self.require_controls_beaten,
            s0_min_abs_ic,
            max_pbo: self.max_pbo,
            require_plateau: self.require_plateau,
        })
        .map_err(ConfigError::Criteria)
    }
}

/// `[run]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunCfg {
    /// Roots every derived per-run seed: `crucible walk-forward` mixes it
    /// with the config hash, the combo index and the fold to get the seed
    /// §2.2 requires. Distinct from the synthetic feed's own seed, and read
    /// even though no strategy is randomized yet — see
    /// `crucible_funnel::walkforward::seed`.
    pub seed: u64,
    /// Starting capital, as decimal dollars in a string.
    pub initial_cash_usd: String,
    /// Position size in contracts.
    pub qty_contracts: i32,
}

/// Just enough of a file to check its version before anything else is
/// interpreted.
///
/// Two passes, deliberately: with one pass, a v2 file carrying a v2 field
/// fails on `unknown field` and never mentions the version, which sends the
/// reader hunting for a typo that is not there.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

impl VersionProbe {
    const fn version(&self) -> u32 {
        self.schema_version
    }
}

/// A config, validated, expanded, and identified.
pub struct LoadedConfig {
    /// Where it was read from.
    pub path: PathBuf,
    /// The transcription.
    pub file: ConfigFile,
    /// The expanded grid.
    pub grid: Grid,
    /// `blake3(canonical_form)` — D-0012.
    pub config_hash: ConfigHash,
    /// The contract spec named by `[contract]` and `[universe]`.
    pub spec: ContractSpec,
    /// Bar interval, parsed.
    pub timeframe: TimeFrame,
    /// Starting capital.
    pub initial_cash_nano_usd: NanoUsd,
    /// Commission per contract per side.
    pub fee_per_contract_nano_usd: NanoUsd,
}

/// Anything that can stop a config from becoming a runnable grid.
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read.
    Read {
        /// Which file.
        path: PathBuf,
        /// Why not.
        source: std::io::Error,
    },
    /// The file is not valid TOML, or has an unknown/missing field.
    Parse {
        /// Which file.
        path: PathBuf,
        /// The parser's own message, which names the line.
        message: String,
    },
    /// The file declares a version this build does not implement.
    Version {
        /// What it declared.
        found: u32,
    },
    /// The config parsed but does not describe a runnable grid.
    Spec(ComboError),
    /// `[walk_forward]` does not describe a fold layout.
    Folds(FoldError),
    /// `[funnel]` does not describe runnable pre-registered criteria.
    Criteria(CriteriaError),
    /// A field parsed as text but is not a valid value.
    Field {
        /// Dotted path of the field.
        field: &'static str,
        /// What is wrong.
        message: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            ConfigError::Parse { path, message } => {
                write!(
                    f,
                    "{} is not a valid combo config:\n{message}",
                    path.display()
                )
            }
            ConfigError::Version { found } => write!(
                f,
                "schema_version = {found}; this build implements version \
                 {SUPPORTED_SCHEMA_VERSION}. A config from a different schema is not \
                 something to guess at"
            ),
            ConfigError::Spec(e) => write!(f, "{e}"),
            ConfigError::Folds(e) => write!(f, "walk_forward: {e}"),
            ConfigError::Criteria(e) => write!(f, "funnel: {e}"),
            ConfigError::Field { field, message } => write!(f, "{field}: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Reads, validates, expands and identifies a combo config.
///
/// # Errors
/// [`ConfigError`] describing exactly what to fix. Every failure is a config
/// bug; none is recoverable.
pub fn load(path: &Path) -> Result<LoadedConfig, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;

    let probe: VersionProbe = toml::from_str(&text).map_err(|e| ConfigError::Parse {
        path: path.to_owned(),
        message: e.to_string(),
    })?;
    if probe.version() != SUPPORTED_SCHEMA_VERSION {
        return Err(ConfigError::Version {
            found: probe.version(),
        });
    }

    let file: ConfigFile = toml::from_str(&text).map_err(|e| ConfigError::Parse {
        path: path.to_owned(),
        message: e.to_string(),
    })?;

    // One instrument, one timeframe. The cross-product over a universe is the
    // funnel's job; running the first entry of a list and calling it done is
    // the kind of partial answer that reads as a whole one.
    if file.universe.instruments.len() != 1 {
        return Err(ConfigError::Field {
            field: "universe.instruments",
            message: format!(
                "{} instruments declared. `combo` runs one instrument; the cross-product \
                 over a universe is the funnel's job (M3), and running only the first \
                 would report a partial answer as a whole one",
                file.universe.instruments.len()
            ),
        });
    }
    if file.universe.timeframes.len() != 1 {
        return Err(ConfigError::Field {
            field: "universe.timeframes",
            message: format!(
                "{} timeframes declared; `combo` runs one",
                file.universe.timeframes.len()
            ),
        });
    }
    let timeframe: TimeFrame = file.universe.timeframes[0].parse().map_err(
        |e: crucible_core::types::ParseTimeFrameError| ConfigError::Field {
            field: "universe.timeframes",
            message: e.to_string(),
        },
    )?;

    match file.execution.fill_model.as_str() {
        "free_fills" | "spread_cross" => {}
        other => {
            return Err(ConfigError::Field {
                field: "execution.fill_model",
                message: format!(
                    "unknown fill model {other:?}; this build names `free_fills` and \
                     `spread_cross` (queue_sim arrives with M4). Execution assumptions are \
                     never implied (§2.4)"
                ),
            });
        }
    }
    if file.execution.half_spread_ticks < 0 {
        return Err(ConfigError::Field {
            field: "execution.half_spread_ticks",
            message: "a half-spread is not negative".to_owned(),
        });
    }
    if file.contract.point_value_usd <= 0 {
        return Err(ConfigError::Field {
            field: "contract.point_value_usd",
            message: "must be positive".to_owned(),
        });
    }

    let tick =
        Price::from_points_str(&file.contract.tick_points).map_err(|e| ConfigError::Field {
            field: "contract.tick_points",
            message: e.to_string(),
        })?;
    if tick.as_nanos() <= 0 {
        return Err(ConfigError::Field {
            field: "contract.tick_points",
            message: "must be a positive price increment".to_owned(),
        });
    }
    let initial_cash_nano_usd =
        parse_usd_to_nano(&file.run.initial_cash_usd).map_err(|e| ConfigError::Field {
            field: "run.initial_cash_usd",
            message: e.to_string(),
        })?;
    let fee_per_contract_nano_usd = parse_usd_to_nano(&file.execution.fee_per_contract_usd)
        .map_err(|e| ConfigError::Field {
            field: "execution.fee_per_contract_usd",
            message: e.to_string(),
        })?;

    // `source` is the one indicator field that is a name rather than a number,
    // so it is checked here — where a `ConfigError` can name the slot — before
    // the transcription below, which has no error channel (D-0060, D-0080).
    for (name, cfg) in &file.indicators {
        if let Some(source) = cfg.declared_source()
            && RollingSource::parse(source).is_none()
        {
            return Err(ConfigError::Field {
                field: "indicators.<slot>.source",
                message: format!(
                    "slot {name:?} declares source = {source:?}; a rolling statistic                      normalizes one of: {}. There is no default, because \"the z-score of                      what\" is not a detail — two slots differing only in it are two                      different features",
                    RollingSource::all()
                        .iter()
                        .map(|s| s.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }

    let slots: Vec<(String, IndicatorSpec)> = file
        .indicators
        .iter()
        .map(|(name, cfg)| (name.clone(), cfg.to_spec()))
        .collect();
    let rules = RuleSource {
        enter_long: file.rules.enter_long.clone(),
        exit_long: file.rules.exit_long.clone(),
        enter_short: file.rules.enter_short.clone(),
        exit_short: file.rules.exit_short.clone(),
    };
    let spec =
        ComboSpec::new(slots, &rules, Qty(file.run.qty_contracts)).map_err(ConfigError::Spec)?;

    let config_hash =
        ConfigHash::from_bytes(*blake3::hash(spec.canonical_form().as_bytes()).as_bytes());
    let grid = spec.expand().map_err(ConfigError::Spec)?;

    Ok(LoadedConfig {
        path: path.to_owned(),
        spec: ContractSpec {
            instrument: InstrumentId::new(&file.universe.instruments[0]),
            tick,
            point_value_usd: file.contract.point_value_usd,
        },
        file,
        grid,
        config_hash,
        timeframe,
        initial_cash_nano_usd,
        fee_per_contract_nano_usd,
    })
}

impl IndicatorCfg {
    /// The `source` string this slot declared, for the kinds that take one.
    const fn declared_source(&self) -> Option<&String> {
        match self {
            IndicatorCfg::Sma { .. }
            | IndicatorCfg::Ema { .. }
            | IndicatorCfg::Bollinger { .. } => None,
            IndicatorCfg::Zscore { source, .. } | IndicatorCfg::Stdev { source, .. } => {
                Some(source)
            }
        }
    }

    /// Transcribes into the strategies crate's plain-data spec. No judging
    /// happens here — [`ComboSpec::new`] owns every domain rule, including the
    /// refusal of [`FloatAxisCfg::Range`].
    fn to_spec(&self) -> IndicatorSpec {
        match self {
            IndicatorCfg::Sma { period } => IndicatorSpec::Sma {
                period: period.to_axis(),
            },
            IndicatorCfg::Ema { period } => IndicatorSpec::Ema {
                period: period.to_axis(),
            },
            IndicatorCfg::Bollinger { period, k } => IndicatorSpec::Bollinger {
                period: period.to_axis(),
                k: k.to_axis(),
            },
            // `source` is validated in `load`, before this runs, because a
            // transcription cannot report an error and this module does no
            // judging of its own (D-0060).
            IndicatorCfg::Zscore { period, source } => IndicatorSpec::ZScore {
                period: period.to_axis(),
                source: RollingSource::parse(source).unwrap_or(RollingSource::Close),
            },
            IndicatorCfg::Stdev { period, source } => IndicatorSpec::Stdev {
                period: period.to_axis(),
                source: RollingSource::parse(source).unwrap_or(RollingSource::Close),
            },
        }
    }
}

impl IntAxisCfg {
    fn to_axis(&self) -> IntAxis {
        match self {
            IntAxisCfg::Fixed(v) => IntAxis::Fixed(*v),
            IntAxisCfg::List(v) => IntAxis::List(v.clone()),
            IntAxisCfg::Range { start, end, step } => IntAxis::Range {
                start: *start,
                end: *end,
                step: *step,
            },
        }
    }
}

impl FloatAxisCfg {
    fn to_axis(&self) -> FloatAxis {
        match self {
            FloatAxisCfg::Fixed(v) => FloatAxis::Fixed(*v),
            FloatAxisCfg::List(v) => FloatAxis::List(v.clone()),
            FloatAxisCfg::Range { start, end, step } => FloatAxis::Range {
                start: *start,
                end: *end,
                step: *step,
            },
        }
    }
}

impl LoadedConfig {
    /// The costed fill model this config declares.
    ///
    /// The half-spread reaches [`SpreadCrossFills`] as a **distance** rather
    /// than a tick count (D-0073), so the funnel's mandatory 0 / 0.5 / 1 / 2
    /// tick sweep can express its middle level; a config still writes whole
    /// ticks, which is what `execution.half_spread_ticks` means.
    #[must_use]
    pub fn spread_cross_fills(&self) -> SpreadCrossFills {
        SpreadCrossFills::from_ticks(self.file.execution.half_spread_ticks, self.spec.tick)
            .with_fee(self.fee_per_contract_nano_usd)
    }

    /// Names the sections that parsed but that the running command does not
    /// act on.
    ///
    /// Printed with every report. A declared kill criterion nothing evaluates
    /// is the quietest way for a config to be wrong, so the honest move is to
    /// say so on every run rather than to trust a reader to remember which
    /// milestone consumes what. The `consumer` argument is what makes the list
    /// command-specific: `crucible combo` reports one number for the whole
    /// series and genuinely ignores `[walk_forward]`, `[funnel]` and
    /// `[run].seed`; `crucible funnel` reads all three, so listing them there
    /// would be a different lie from the one this method exists to prevent.
    #[must_use]
    pub fn unconsumed_sections(&self, consumer: Consumer) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.file.walk_forward.is_some() && consumer == Consumer::Combo {
            out.push("[walk_forward] — folds; run `crucible walk-forward` to consume them");
        }
        if self.file.funnel.is_some() && consumer != Consumer::Funnel {
            out.push(
                "[funnel] — stages, the cost sweep and the kill criteria; run `crucible funnel`",
            );
        }
        if consumer == Consumer::Combo {
            out.push(
                "[run].seed — nothing reads it here; `walk-forward` derives per-fold seeds from it",
            );
        }
        out
    }
}

/// Which command is asking [`LoadedConfig::unconsumed_sections`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consumer {
    /// `crucible combo` — one window, no folds, no gates.
    Combo,
    /// `crucible walk-forward` — folds and seeds, no gates.
    WalkForward,
    /// `crucible funnel` — everything.
    Funnel,
}
