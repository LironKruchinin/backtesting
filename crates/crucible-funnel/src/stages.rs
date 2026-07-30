//! Funnel stages, pre-registered criteria, and verdicts.
//!
//! ## Stage contracts
//!
//! **S0 — signal triage** (seconds; no trading, no fills). Compute the
//! signal on bars, bucket forward returns by signal quantile, check for a
//! monotonic relationship and a nonzero information coefficient. No
//! relationship ⇒ `Kill` — nothing downstream can rescue a signal that
//! predicts nothing. **Not implemented** (see below).
//!
//! **S1 — coarse grid, free fills** (seconds–minutes). Wide-step grid under
//! `FreeFills`. This is the ONLY sanctioned use of `FreeFills` (CLAUDE.md
//! §2.4): a strategy that can't make money with free fills is dead, cheaply.
//!
//! **S2 — walk-forward with honest costs** (minutes). Survivors rerun under
//! `spread_cross` with rolling train/test folds, plus the **cost-sensitivity
//! sweep** (0 / 0.5 / 1 / 2 ticks): an edge that dies at 1 tick is not an
//! edge. The folds themselves are [`crate::walkforward`]'s (D-0062, D-0063);
//! what S2 adds is the sweep, the controls, and the kill criteria.
//!
//! **S3 — the battery**: parameter perturbation (plateau, not spike), regime
//! slices, deflated Sharpe, PBO/CSCV, permutation nulls, cross-instrument
//! rhyme. **Not implemented** (see below).
//!
//! ## What this build actually runs, and what it refuses
//!
//! S1 and S2. A config that declares `s0` or `s3` is **refused at load**, with
//! a message naming what each needs, because the alternative is a run that
//! prints a fold table under a heading the config asked a different question
//! of. `crucible-funnel::stats` still owes S3's battery; S0 needs a
//! signal-extraction seam the combo grammar does not have — its rules produce
//! *positions*, not a continuous score to bucket forward returns by.
//!
//! **The consequence: this build cannot award [`Verdict::Graduate`].** The
//! glossary defines Graduate as "survived the full battery", and the battery
//! is what is missing, so the best verdict available is [`Verdict::Iterate`]
//! — and every report says so, rather than letting a reader infer that nothing
//! graduated because nothing was good enough.
//!
//! ## Rule zero
//!
//! Kill criteria and thresholds come from the config, written before the run,
//! and are stored on the registry row that was inserted before the run
//! ([`crate::registry`]). Post-hoc threshold adjustment is rationalization and
//! does not ship.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The outcome of a funnel evaluation. `Kill` is the common, healthy case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Falsified at some stage; record the stage and reason in the scorecard.
    Kill,
    /// Survived with caveats worth another, sharper hypothesis iteration.
    Iterate,
    /// Survived the full battery; eligible for paper trading (M4).
    Graduate,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Verdict::Kill => "KILL",
            Verdict::Iterate => "ITERATE",
            Verdict::Graduate => "GRADUATE",
        })
    }
}

/// A funnel gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Signal triage. Not implemented.
    S0,
    /// Free-fill coarse screen.
    S1,
    /// Walk-forward under costs, with the sweep and the controls.
    S2,
    /// The overfitting battery. Not implemented.
    S3,
}

impl Stage {
    /// Whether this build can run the stage.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(self, Stage::S1 | Stage::S2)
    }

    /// What the stage still needs, for the refusal message.
    const fn missing_because(self) -> &'static str {
        match self {
            Stage::S0 => {
                "S0 buckets forward returns by signal quantile and needs a continuous score to \
                 bucket by; the combo rule grammar produces positions, not scores, so there is \
                 nothing to compute an information coefficient over yet"
            }
            Stage::S3 => {
                "S3 is deflated Sharpe, PBO/CSCV, the permutation nulls and the cross-instrument \
                 rhyme check — `crucible-funnel::stats`, which is still a module-doc spec"
            }
            Stage::S1 | Stage::S2 => "",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stage::S0 => "s0",
            Stage::S1 => "s1",
            Stage::S2 => "s2",
            Stage::S3 => "s3",
        })
    }
}

impl FromStr for Stage {
    type Err = CriteriaError;

    fn from_str(s: &str) -> Result<Stage, CriteriaError> {
        match s {
            "s0" => Ok(Stage::S0),
            "s1" => Ok(Stage::S1),
            "s2" => Ok(Stage::S2),
            "s3" => Ok(Stage::S3),
            other => Err(CriteriaError::UnknownStage {
                found: other.to_owned(),
            }),
        }
    }
}

/// The pre-registered criteria, as declared in a config's `[funnel]` section
/// **before** the run.
///
/// Serializable because the registry stores it verbatim on the run row: a
/// criterion that lives only in a config file at judging time is remembered,
/// not pre-registered, and the difference is the whole of `PROJECT_PLAN.md`
/// §7.2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Criteria {
    /// Stages to run, deduplicated and in order.
    pub stages: Vec<Stage>,
    /// The mandatory cost sweep (§2.4), in **half-ticks**: `0 / 1 / 2 / 4` is
    /// the 0 / 0.5 / 1 / 2 ticks the rule names. Integers because a price grid
    /// has half-ticks and not tenths (D-0073).
    pub cost_sweep_half_ticks: Vec<i64>,
    /// S0-adequacy: fewer pooled out-of-sample round-trips than this and there
    /// is nothing to measure, whatever the return says.
    pub min_oos_trades: usize,
    /// S0-adequacy: pooled out-of-sample trading days.
    pub min_oos_sessions: usize,
    /// S1: pooled out-of-sample return under `free_fills`.
    pub min_oos_return_pct_free_fills: f64,
    /// S2: pooled out-of-sample Sharpe under the config's costed fill model.
    pub min_oos_sharpe_after_costs: f64,
    /// S2: the sweep level, in half-ticks, at which the Sharpe criterion must
    /// still hold.
    pub kill_if_dead_half_ticks: i64,
    /// S2: the combo must beat both mandatory controls on pooled OOS return.
    pub require_controls_beaten: bool,
    /// S3, echoed and **not evaluated** by this build.
    pub max_pbo: f64,
    /// S3, echoed and not evaluated by this build.
    pub require_plateau: bool,
}

/// Why a `[funnel]` section does not describe runnable criteria.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CriteriaError {
    /// A stage name this build has never heard of.
    UnknownStage {
        /// What was written.
        found: String,
    },
    /// A stage this build cannot run. Refused, never skipped.
    UnimplementedStage {
        /// Which one.
        stage: Stage,
    },
    /// No stages at all.
    NoStages,
    /// A sweep level that is not a whole number of half-ticks.
    UnrepresentableSweepLevel {
        /// The level as written.
        ticks: String,
    },
    /// §2.4 names four sweep levels and they are not optional.
    SweepMissingLevel {
        /// The level that is missing, in ticks.
        ticks: String,
    },
    /// The cost-death criterion names a level the sweep does not run.
    KillLevelNotSwept {
        /// The level that is missing, in ticks.
        ticks: String,
    },
}

impl fmt::Display for CriteriaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CriteriaError::UnknownStage { found } => write!(
                f,
                "unknown funnel stage {found:?}; this build names s0, s1, s2 and s3"
            ),
            CriteriaError::UnimplementedStage { stage } => write!(
                f,
                "stage {stage} is declared but not implemented in this build, and it is refused \
                 rather than skipped: a config that asks for {stage} and silently receives the \
                 stages below it has been answered with a different question than the one it \
                 asked.\n       {}",
                stage.missing_because()
            ),
            CriteriaError::NoStages => write!(
                f,
                "funnel.stages is empty; a funnel run with no gates is a backtest with a verdict \
                 stapled to it"
            ),
            CriteriaError::UnrepresentableSweepLevel { ticks } => write!(
                f,
                "cost_sensitivity_ticks contains {ticks}, which is not a whole number of \
                 half-ticks. A half-spread is a distance on a price grid, and that grid has \
                 half-ticks, not tenths (D-0073) — write 0, 0.5, 1 or 2"
            ),
            CriteriaError::SweepMissingLevel { ticks } => write!(
                f,
                "cost_sensitivity_ticks is missing {ticks}. CLAUDE.md §2.4 makes the 0 / 0.5 / \
                 1 / 2 tick sweep mandatory on every scorecard, so it is not a list to trim: \
                 dropping the level an edge dies at is how a cost sensitivity stops being one"
            ),
            CriteriaError::KillLevelNotSwept { ticks } => write!(
                f,
                "kill_if_dead_at_ticks = {ticks} names a level cost_sensitivity_ticks does not \
                 run, so the criterion could never be evaluated"
            ),
        }
    }
}

impl std::error::Error for CriteriaError {}

/// The sweep §2.4 mandates, in half-ticks.
pub const MANDATORY_SWEEP_HALF_TICKS: [i64; 4] = [0, 1, 2, 4];

/// The `[funnel]` section, transcribed. A plain-data argument bundle so
/// [`Criteria::new`] validates one value rather than ten positional ones.
#[derive(Clone, Debug)]
pub struct CriteriaSource<'a> {
    /// `stages`.
    pub stages: &'a [String],
    /// `cost_sensitivity_ticks`.
    pub cost_sensitivity_ticks: &'a [f64],
    /// `min_oos_trades`.
    pub min_oos_trades: usize,
    /// `min_oos_sessions`.
    pub min_oos_sessions: usize,
    /// `min_oos_return_pct_free_fills`.
    pub min_oos_return_pct_free_fills: f64,
    /// `min_oos_sharpe_after_costs`.
    pub min_oos_sharpe_after_costs: f64,
    /// `kill_if_dead_at_ticks`.
    pub kill_if_dead_at_ticks: f64,
    /// `require_controls_beaten`.
    pub require_controls_beaten: bool,
    /// `max_pbo`.
    pub max_pbo: f64,
    /// `require_plateau`.
    pub require_plateau: bool,
}

impl Criteria {
    /// Validates a `[funnel]` section into runnable criteria.
    ///
    /// Everything that can be judged before a single bar is replayed is judged
    /// here, on purpose: a criterion that turns out to be unevaluable after an
    /// hour of grid replay has already cost the hour.
    ///
    /// # Errors
    /// [`CriteriaError`] naming exactly what to fix.
    pub fn new(source: &CriteriaSource<'_>) -> Result<Criteria, CriteriaError> {
        let mut stages: Vec<Stage> = Vec::new();
        for name in source.stages {
            let stage: Stage = name.parse()?;
            if !stage.is_implemented() {
                return Err(CriteriaError::UnimplementedStage { stage });
            }
            if !stages.contains(&stage) {
                stages.push(stage);
            }
        }
        stages.sort_unstable();
        if stages.is_empty() {
            return Err(CriteriaError::NoStages);
        }

        let mut sweep: Vec<i64> = Vec::new();
        for &ticks in source.cost_sensitivity_ticks {
            let half =
                half_ticks(ticks).ok_or_else(|| CriteriaError::UnrepresentableSweepLevel {
                    ticks: format!("{ticks}"),
                })?;
            if !sweep.contains(&half) {
                sweep.push(half);
            }
        }
        sweep.sort_unstable();
        for level in MANDATORY_SWEEP_HALF_TICKS {
            if !sweep.contains(&level) {
                return Err(CriteriaError::SweepMissingLevel {
                    ticks: render_half_ticks(level),
                });
            }
        }

        let kill_if_dead_half_ticks =
            half_ticks(source.kill_if_dead_at_ticks).ok_or_else(|| {
                CriteriaError::UnrepresentableSweepLevel {
                    ticks: format!("{}", source.kill_if_dead_at_ticks),
                }
            })?;
        if !sweep.contains(&kill_if_dead_half_ticks) {
            return Err(CriteriaError::KillLevelNotSwept {
                ticks: render_half_ticks(kill_if_dead_half_ticks),
            });
        }

        Ok(Criteria {
            stages,
            cost_sweep_half_ticks: sweep,
            min_oos_trades: source.min_oos_trades,
            min_oos_sessions: source.min_oos_sessions,
            min_oos_return_pct_free_fills: source.min_oos_return_pct_free_fills,
            min_oos_sharpe_after_costs: source.min_oos_sharpe_after_costs,
            kill_if_dead_half_ticks,
            require_controls_beaten: source.require_controls_beaten,
            max_pbo: source.max_pbo,
            require_plateau: source.require_plateau,
        })
    }

    /// A permissive set for tests and for the registry's own fixtures.
    #[doc(hidden)]
    #[must_use]
    pub fn for_tests() -> Criteria {
        Criteria {
            stages: vec![Stage::S1, Stage::S2],
            cost_sweep_half_ticks: MANDATORY_SWEEP_HALF_TICKS.to_vec(),
            min_oos_trades: 1,
            min_oos_sessions: 1,
            min_oos_return_pct_free_fills: 0.0,
            min_oos_sharpe_after_costs: 0.0,
            kill_if_dead_half_ticks: 2,
            require_controls_beaten: true,
            max_pbo: 0.5,
            require_plateau: true,
        }
    }

    /// Whether the criteria ask for a stage.
    #[must_use]
    pub fn runs(&self, stage: Stage) -> bool {
        self.stages.contains(&stage)
    }

    /// `1.5` for a `kill_if_dead_half_ticks` of 3 — the level named in ticks,
    /// for reports that quote the config back.
    #[must_use]
    pub fn kill_level_ticks(&self) -> String {
        render_half_ticks(self.kill_if_dead_half_ticks)
    }
}

/// `1.5` ⇒ `Some(3)`; `0.3` ⇒ `None`. Exact, and without trusting a float to
/// be exactly what it was typed as: the only accepted values are those within
/// a billionth of a half-tick multiple.
fn half_ticks(ticks: f64) -> Option<i64> {
    if !ticks.is_finite() || ticks < 0.0 {
        return None;
    }
    let doubled = ticks * 2.0;
    let rounded = doubled.round();
    if (doubled - rounded).abs() >= 1e-9 || rounded > 1_000.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "bounded above by 1000 and integral by the check above"
    )]
    Some(rounded as i64)
}

/// `3` ⇒ `"1.5"`, for messages that name a level the config wrote in ticks.
#[must_use]
pub fn render_half_ticks(half: i64) -> String {
    if half % 2 == 0 {
        format!("{}", half / 2)
    } else {
        format!("{}.5", half / 2)
    }
}

/// The evidence a verdict is computed from — all of it, named, so that reading
/// the struct tells you what the funnel is allowed to judge on.
#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    /// Pooled out-of-sample round-trips under the costed model.
    pub oos_trades: usize,
    /// Pooled out-of-sample trading days.
    pub oos_sessions: usize,
    /// Pooled out-of-sample return under `free_fills` — the S1 screen.
    pub free_fill_return_pct: f64,
    /// Pooled out-of-sample return under the config's costed model.
    pub costed_return_pct: f64,
    /// Pooled out-of-sample Sharpe under the config's costed model.
    pub costed_sharpe: Option<f64>,
    /// Pooled out-of-sample Sharpe at the `kill_if_dead` sweep level.
    pub sharpe_at_kill_level: Option<f64>,
    /// Pooled out-of-sample return of the matched random-entry control, or
    /// `None` if it could not be built. `None` is **not** a pass: a criterion
    /// that requires beating a control cannot be met by a control that does
    /// not exist.
    pub random_entry_return_pct: Option<f64>,
    /// Pooled out-of-sample return of the buy-and-hold control.
    pub buy_and_hold_return_pct: Option<f64>,
}

/// One criterion, evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reason {
    /// Which gate it belongs to.
    pub stage: Stage,
    /// Whether the combo cleared it.
    pub passed: bool,
    /// The sentence a scorecard prints: what was required, and what was found.
    pub detail: String,
}

/// A verdict, and every criterion that produced it.
#[derive(Clone, Debug)]
pub struct Assessment {
    /// Kill / Iterate / Graduate.
    pub verdict: Verdict,
    /// The stage that decided it — the first one that failed, or the last one
    /// that ran.
    pub decided_at: Stage,
    /// Every criterion evaluated, in evaluation order. Passes are kept as well
    /// as failures: a scorecard that only listed failures would make a
    /// survivor look unexamined.
    pub reasons: Vec<Reason>,
}

impl Assessment {
    /// The reasons, rendered one per line, for the registry and the scorecard.
    #[must_use]
    pub fn rendered_reasons(&self) -> Vec<String> {
        self.reasons
            .iter()
            .map(|r| {
                format!(
                    "[{}] {} — {}",
                    r.stage,
                    if r.passed { "pass" } else { "FAIL" },
                    r.detail
                )
            })
            .collect()
    }
}

/// Judges one combo's evidence against the criteria it was pre-registered
/// under.
///
/// Evaluation is **ordered and short-circuiting between stages, exhaustive
/// within one**: the first stage with a failure decides the verdict, but every
/// criterion of that stage is still evaluated, so a scorecard can say "it
/// failed the trade count *and* the session count" rather than making a reader
/// re-run to discover the second one.
#[must_use]
pub fn assess(criteria: &Criteria, evidence: &Evidence) -> Assessment {
    let mut reasons: Vec<Reason> = Vec::new();
    let mut decided_at = Stage::S0;

    // Sample adequacy first, under its own heading: "not enough evidence" and
    // "the evidence is bad" are different verdicts about an idea, and
    // reporting the second when the first is true is the commoner mistake.
    let adequacy = vec![
        check(
            Stage::S0,
            evidence.oos_trades >= criteria.min_oos_trades,
            format!(
                "{} pooled out-of-sample round-trip(s), {} required",
                evidence.oos_trades, criteria.min_oos_trades
            ),
        ),
        check(
            Stage::S0,
            evidence.oos_sessions >= criteria.min_oos_sessions,
            format!(
                "{} pooled out-of-sample session(s), {} required",
                evidence.oos_sessions, criteria.min_oos_sessions
            ),
        ),
    ];
    let mut failed = adequacy.iter().any(|r| !r.passed);
    reasons.extend(adequacy);

    if !failed && criteria.runs(Stage::S1) {
        decided_at = Stage::S1;
        let s1 = vec![check(
            Stage::S1,
            evidence.free_fill_return_pct >= criteria.min_oos_return_pct_free_fills,
            format!(
                "{:+.2}% pooled out-of-sample return under free_fills, {:+.2}% required — a \
                 strategy that cannot make money cost-free is dead cheaply (D-0006)",
                evidence.free_fill_return_pct, criteria.min_oos_return_pct_free_fills
            ),
        )];
        failed = s1.iter().any(|r| !r.passed);
        reasons.extend(s1);
    }

    if !failed && criteria.runs(Stage::S2) {
        decided_at = Stage::S2;
        let mut s2 = vec![
            check(
                Stage::S2,
                at_least(evidence.costed_sharpe, criteria.min_oos_sharpe_after_costs),
                format!(
                    "pooled out-of-sample Sharpe after costs {}, {:.2} required",
                    show(evidence.costed_sharpe),
                    criteria.min_oos_sharpe_after_costs
                ),
            ),
            check(
                Stage::S2,
                at_least(
                    evidence.sharpe_at_kill_level,
                    criteria.min_oos_sharpe_after_costs,
                ),
                format!(
                    "pooled out-of-sample Sharpe at {} tick(s) of half-spread {}, {:.2} required \
                     — an edge that dies at the sweep's kill level is a rounding error on the \
                     cost assumption",
                    criteria.kill_level_ticks(),
                    show(evidence.sharpe_at_kill_level),
                    criteria.min_oos_sharpe_after_costs
                ),
            ),
        ];
        if criteria.require_controls_beaten {
            s2.push(control_check(
                "matched random-entry",
                evidence.costed_return_pct,
                evidence.random_entry_return_pct,
            ));
            s2.push(control_check(
                "buy-and-hold",
                evidence.costed_return_pct,
                evidence.buy_and_hold_return_pct,
            ));
        }
        failed = s2.iter().any(|r| !r.passed);
        reasons.extend(s2);
    }

    // Graduate is unreachable, and deliberately: the glossary defines it as
    // "survived the full battery", and S3 — the battery — is not in this
    // build. Awarding it on S1+S2 would just rename Iterate.
    Assessment {
        verdict: if failed {
            Verdict::Kill
        } else {
            Verdict::Iterate
        },
        decided_at,
        reasons,
    }
}

fn check(stage: Stage, passed: bool, detail: String) -> Reason {
    Reason {
        stage,
        passed,
        detail,
    }
}

/// A control that could not be built does not pass. An absent benchmark is a
/// missing denominator, and `PROJECT_PLAN.md` §7.4 bans reporting without one.
fn control_check(name: &str, strategy_pct: f64, control_pct: Option<f64>) -> Reason {
    match control_pct {
        Some(control) => check(
            Stage::S2,
            strategy_pct > control,
            format!(
                "{strategy_pct:+.2}% pooled out-of-sample return vs {control:+.2}% for the \
                 {name} control"
            ),
        ),
        None => check(
            Stage::S2,
            false,
            format!(
                "the {name} control could not be built, so there is nothing to compare \
                 {strategy_pct:+.2}% against — an absent denominator is not a passed criterion"
            ),
        ),
    }
}

/// `None` never clears a floor: a Sharpe that does not exist (no variance, no
/// trades) is not a Sharpe above the bar.
fn at_least(value: Option<f64>, floor: f64) -> bool {
    value.is_some_and(|v| v >= floor)
}

fn show(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.2}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn source<'a>(stages: &'a [String], sweep: &'a [f64], kill: f64) -> CriteriaSource<'a> {
        CriteriaSource {
            stages,
            cost_sensitivity_ticks: sweep,
            min_oos_trades: 30,
            min_oos_sessions: 60,
            min_oos_return_pct_free_fills: 0.0,
            min_oos_sharpe_after_costs: 0.5,
            kill_if_dead_at_ticks: kill,
            require_controls_beaten: true,
            max_pbo: 0.5,
            require_plateau: true,
        }
    }

    fn criteria() -> Criteria {
        let stages = names(&["s1", "s2"]);
        Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.0)).expect("valid")
    }

    fn passing() -> Evidence {
        Evidence {
            oos_trades: 100,
            oos_sessions: 120,
            free_fill_return_pct: 8.0,
            costed_return_pct: 4.0,
            costed_sharpe: Some(1.2),
            sharpe_at_kill_level: Some(0.9),
            random_entry_return_pct: Some(0.5),
            buy_and_hold_return_pct: Some(1.0),
        }
    }

    /// The sweep §2.4 mandates is not a list to trim, and a level that is not
    /// on the price grid is refused rather than rounded.
    #[test]
    fn the_mandatory_cost_sweep_is_validated_before_anything_replays() {
        let stages = names(&["s1", "s2"]);
        assert!(Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.0)).is_ok());
        assert_eq!(
            Criteria::new(&source(&stages, &[0.0, 1.0, 2.0], 1.0)).expect_err("missing 0.5"),
            CriteriaError::SweepMissingLevel {
                ticks: "0.5".to_owned()
            }
        );
        assert_eq!(
            Criteria::new(&source(&stages, &[0.0, 0.3, 0.5, 1.0, 2.0], 1.0))
                .expect_err("0.3 is not a half-tick"),
            CriteriaError::UnrepresentableSweepLevel {
                ticks: "0.3".to_owned()
            }
        );
        assert_eq!(
            Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.5))
                .expect_err("1.5 is not swept"),
            CriteriaError::KillLevelNotSwept {
                ticks: "1.5".to_owned()
            }
        );
    }

    /// A stage this build cannot run is refused at load, not skipped at run
    /// time. The message has to name what is missing, or the refusal is a wall.
    #[test]
    fn an_unimplemented_stage_is_refused_and_says_what_it_needs() {
        for name in ["s0", "s3"] {
            let stages = names(&["s1", "s2", name]);
            let err = Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.0))
                .expect_err("must refuse");
            let text = err.to_string();
            assert!(text.contains("not implemented"), "{text}");
            assert!(text.contains("refused \nrather than skipped") || text.contains("refused"));
            assert!(
                text.contains("information coefficient") || text.contains("PBO"),
                "{text}"
            );
        }
        assert_eq!(
            Stage::from_str("s9").expect_err("unknown"),
            CriteriaError::UnknownStage {
                found: "s9".to_owned()
            }
        );
    }

    /// Sample adequacy decides before performance does — and both adequacy
    /// criteria are reported, not just the first to fail.
    #[test]
    fn too_few_trades_is_killed_at_s0_with_every_adequacy_reason_reported() {
        let mut e = passing();
        e.oos_trades = 3;
        e.oos_sessions = 4;
        let a = assess(&criteria(), &e);
        assert_eq!(a.verdict, Verdict::Kill);
        assert_eq!(a.decided_at, Stage::S0);
        assert_eq!(a.reasons.len(), 2, "both adequacy criteria are evaluated");
        assert!(a.reasons.iter().all(|r| !r.passed));
        // And nothing downstream was judged on a sample that small.
        assert!(a.reasons.iter().all(|r| r.stage == Stage::S0));
    }

    /// The cheapest kill available: not clearly positive even cost-free.
    #[test]
    fn a_strategy_that_loses_with_free_fills_dies_at_s1() {
        let mut e = passing();
        e.free_fill_return_pct = -2.0;
        let a = assess(&criteria(), &e);
        assert_eq!(a.verdict, Verdict::Kill);
        assert_eq!(a.decided_at, Stage::S1);
        assert!(a.reasons.iter().any(|r| r.stage == Stage::S1 && !r.passed));
        assert!(
            !a.reasons.iter().any(|r| r.stage == Stage::S2),
            "S2 is not run once S1 has killed"
        );
    }

    /// An edge that survives at the config's own half-spread but dies at the
    /// sweep's kill level is still dead.
    #[test]
    fn an_edge_that_dies_at_the_kill_level_is_killed_at_s2() {
        let mut e = passing();
        e.sharpe_at_kill_level = Some(0.1);
        let a = assess(&criteria(), &e);
        assert_eq!(a.verdict, Verdict::Kill);
        assert_eq!(a.decided_at, Stage::S2);
    }

    /// Beating the market is not the same as having an edge, and neither is
    /// beating coin flips. Both controls are criteria, not decoration.
    #[test]
    fn failing_either_control_kills() {
        let mut beaten_by_random = passing();
        beaten_by_random.random_entry_return_pct = Some(9.0);
        assert_eq!(
            assess(&criteria(), &beaten_by_random).verdict,
            Verdict::Kill
        );

        let mut beaten_by_market = passing();
        beaten_by_market.buy_and_hold_return_pct = Some(9.0);
        assert_eq!(
            assess(&criteria(), &beaten_by_market).verdict,
            Verdict::Kill
        );
    }

    /// An absent control is not a passed control. This is the one that stops a
    /// scorecard from quietly rendering a verdict with a missing denominator.
    #[test]
    fn a_control_that_could_not_be_built_is_a_failure_not_a_pass() {
        let mut e = passing();
        e.random_entry_return_pct = None;
        let a = assess(&criteria(), &e);
        assert_eq!(a.verdict, Verdict::Kill);
        assert!(
            a.rendered_reasons()
                .iter()
                .any(|r| r.contains("absent denominator")),
            "{:?}",
            a.rendered_reasons()
        );
    }

    /// A Sharpe that does not exist does not clear a floor of 0.5 — or of any
    /// other number.
    #[test]
    fn a_missing_sharpe_never_clears_the_bar() {
        let mut e = passing();
        e.costed_sharpe = None;
        assert_eq!(assess(&criteria(), &e).verdict, Verdict::Kill);
    }

    /// The ceiling this build imposes: everything passes, and the answer is
    /// still Iterate, because Graduate means "survived the full battery" and
    /// the battery is S3.
    #[test]
    fn passing_everything_this_build_runs_is_iterate_and_never_graduate() {
        let a = assess(&criteria(), &passing());
        assert_eq!(a.verdict, Verdict::Iterate);
        assert_eq!(a.decided_at, Stage::S2);
        assert!(a.reasons.iter().all(|r| r.passed));
        assert!(a.reasons.len() >= 6, "{:?}", a.rendered_reasons());
    }
}
