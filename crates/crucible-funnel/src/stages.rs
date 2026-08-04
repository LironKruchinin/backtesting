//! Funnel stages, pre-registered criteria, and verdicts.
//!
//! ## Stage contracts
//!
//! **S0 — signal triage** (seconds; no trading, no fills). Score the bars,
//! join each score to the return that followed it, bucket by score quantile,
//! and measure the information coefficient. No relationship ⇒ `Kill` — nothing
//! downstream can rescue a signal that predicts nothing.
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
//! S0, S1 and S2. A config that declares `s3` is **refused at load**, with a
//! message naming what it needs, because the alternative is a run that prints a
//! fold table under a heading the config asked a different question of.
//! `crucible-funnel::stats` still owes S3's battery.
//!
//! **S0 landed 2026-07-31** (D-0085): [`crate::s0`] is the measurement — the
//! forward-return join, the information coefficient, the quantile buckets and
//! the session block bootstrap (D-0082) — and `crucible-cli::funnel` is the
//! caller that scores a combo, charges its trial and records the row. Its
//! criterion needs **both halves at one horizon**: `|IC| >= min_abs_ic` *and* a
//! mean forward return whose bootstrap interval excludes zero. Size without
//! significance is what a large enough sample of noise gives for free — the
//! null harness reads `|IC| = 0.0378` on 20,000 bars of seeded random walk —
//! and significance without size is an effect too small to trade.
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

/// Where a verdict was decided.
///
/// Four of these are the funnel's declarable **gates** (S0..S3, §4). The fifth,
/// [`Stage::Admission`], is not a gate and cannot be declared: it is the
/// pre-trial check that asks whether there is enough evidence to judge at all.
/// It was called `S0` until 2026-07-30, which was a label squat from before the
/// predictor stage existed — a scorecard could read "killed at S0" for a combo
/// with too few sessions while a config declaring `s0` was refused at load
/// (D-0084).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// **Not a gate.** Sample adequacy, checked before any stage runs: too few
    /// pooled out-of-sample round-trips or sessions to judge anything.
    ///
    /// Ordered first so `decided_at` still sorts in evaluation order. Never
    /// parsed from a config — see [`Stage::from_str`].
    Admission,
    /// Signal triage: does the score predict the return after it (D-0085).
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
        matches!(self, Stage::Admission | Stage::S0 | Stage::S1 | Stage::S2)
    }

    /// What the stage still needs, for the refusal message.
    const fn missing_because(self) -> &'static str {
        match self {
            Stage::S3 => {
                "S3 is deflated Sharpe, PBO/CSCV, the permutation nulls and the cross-instrument \
                 rhyme check — `crucible-funnel::stats`, which is still a module-doc spec"
            }
            Stage::Admission | Stage::S0 | Stage::S1 | Stage::S2 => "",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stage::Admission => "admission",
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
            // Deliberately absent: "admission". It is a pre-trial check, not a
            // gate, so a config cannot ask for it or skip it (D-0084).
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
    /// **Admission**: fewer pooled out-of-sample round-trips than this and
    /// there is nothing to measure, whatever the return says. Checked before
    /// any stage runs, and named `admission` rather than `s0` since D-0084 —
    /// sample adequacy is a pre-trial check, not the predictor gate.
    pub min_oos_trades: usize,
    /// **Admission**: pooled out-of-sample trading days. See
    /// [`Criteria::min_oos_trades`] for why this is not `s0`.
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
    /// S0: the smallest `|IC|` at any declared horizon that counts as a
    /// relationship. `None` when the config did not declare `s0`.
    pub s0_min_abs_ic: Option<f64>,
    /// **S3, and the first piece of the battery that is real**: the largest
    /// permutation-null p-value that still counts as an edge. `None` when the
    /// caller supplies no permutation result.
    ///
    /// Evaluated whenever an `Evidence` carries a p-value, rather than gated on
    /// `stages` containing `s3` — because `s3` as a *declarable stage* still
    /// needs the rest of the battery (deflated Sharpe, PBO, truncation) and is
    /// still refused at load (D-0075). This is the seam that lets the harness
    /// be exercised by its acceptance test before the stage it belongs to
    /// exists.
    pub max_permutation_p: Option<f64>,
    /// S3, echoed and **not evaluated** by this build.
    pub max_pbo: f64,
    /// S3, echoed and not evaluated by this build.
    pub require_plateau: bool,
}

/// Why a `[funnel]` section does not describe runnable criteria.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CriteriaError {
    /// `stages` declares `s0` but the config has no `[s0]` block.
    S0BlockMissing,
    /// The config has an `[s0]` block but `stages` does not declare `s0`.
    S0BlockUnused,

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
            CriteriaError::S0BlockMissing => write!(
                f,
                "`stages` declares `s0` but there is no `[s0]` block. S0 has to be told what to \
                 measure BEFORE it runs — the score slot, the forward horizons, and the |IC| \
                 below which the score predicts nothing. A stage with no criteria is a stage \
                 with no pre-registration."
            ),
            CriteriaError::S0BlockUnused => write!(
                f,
                "there is an `[s0]` block but `stages` does not declare `s0`, so nothing would \
                 read it. Add \"s0\" to `stages`, or delete the block — a config carrying \
                 criteria nobody evaluates is a config that thinks it asked for something."
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
    /// `[s0].min_abs_ic`, or `None` when the config declares no `[s0]` block.
    pub s0_min_abs_ic: Option<f64>,
    /// The permutation criterion, or `None`.
    pub max_permutation_p: Option<f64>,
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

        // A stage with no criteria is a stage with no pre-registration, and a
        // criteria block for a stage nobody asked for is a config that thinks
        // it asked for something (D-0085). Both are refused at load.
        if stages.contains(&Stage::S0) && source.s0_min_abs_ic.is_none() {
            return Err(CriteriaError::S0BlockMissing);
        }
        if !stages.contains(&Stage::S0) && source.s0_min_abs_ic.is_some() {
            return Err(CriteriaError::S0BlockUnused);
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
            s0_min_abs_ic: source.s0_min_abs_ic,
            max_permutation_p: source.max_permutation_p,
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
            s0_min_abs_ic: None,
            max_permutation_p: None,
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
    /// The permutation null's empirical p-value for this combo's out-of-sample
    /// result, or `None` when the harness did not run
    /// ([`crate::stats::permutation`]). `None` is not a pass — the criterion
    /// below is simply not evaluated, and a config that wanted it says so by
    /// declaring `max_permutation_p`.
    pub permutation_p_value: Option<f64>,
    /// This combo's Sharpe corrected for how many times the family has been
    /// searched, or `None` when the correction has no inputs — no Sharpe, or a
    /// window flat enough to have no higher moments. `None` is not a pass, on
    /// the same terms as an absent control (D-0075).
    pub deflated: Option<crate::stats::deflated::Deflated>,
    /// The trial count the deflation divided by, read from the registry
    /// (D-0083 excludes voided runs by construction). Carried beside the
    /// number because a headline Sharpe without its trial count is not
    /// reported anywhere in this project.
    pub n_trials: usize,
    /// The grid's probability of backtest overfitting, or `None` when CSCV
    /// could not be computed. One value shared by every combo: PBO is a
    /// property of the search, not of a combo.
    pub pbo: Option<f64>,
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

fn s0_detail(s0: Option<&crate::s0::S0ComboReport>, threshold: f64) -> String {
    let Some(result) = s0 else {
        return format!("S0 result unavailable, |IC| threshold {threshold:.4}");
    };
    match result.criterion() {
        crate::s0::S0CriterionOutcome::Passed { horizon_ns } => format!(
            "generic S0 criterion passed at {horizon_ns}ns; |IC| threshold {threshold:.4}"
        ),
        crate::s0::S0CriterionOutcome::Failed => result.best_abs_ic().map_or_else(
            || "S0 criterion says measured failure, but every horizon IC is unavailable".to_owned(),
            |best| format!(
                "best measurable |IC| {best:.4}, but no horizon cleared both |IC| {threshold:.4} and its unconditional mean interval"
            ),
        ),
        crate::s0::S0CriterionOutcome::Unavailable { reason } => format!(
            "required S0 evidence unavailable ({reason}); |IC| threshold {threshold:.4}; the criterion remains unevaluated"
        ),
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
pub fn assess(
    criteria: &Criteria,
    evidence: &Evidence,
    s0: Option<&crate::s0::S0ComboReport>,
) -> Assessment {
    let mut reasons: Vec<Reason> = Vec::new();
    let mut decided_at = Stage::Admission;

    // Sample adequacy first, under its own heading: "not enough evidence" and
    // "the evidence is bad" are different verdicts about an idea, and
    // reporting the second when the first is true is the commoner mistake.
    let adequacy = vec![
        check(
            Stage::Admission,
            evidence.oos_trades >= criteria.min_oos_trades,
            format!(
                "{} pooled out-of-sample round-trip(s), {} required",
                evidence.oos_trades, criteria.min_oos_trades
            ),
        ),
        check(
            Stage::Admission,
            evidence.oos_sessions >= criteria.min_oos_sessions,
            format!(
                "{} pooled out-of-sample session(s), {} required",
                evidence.oos_sessions, criteria.min_oos_sessions
            ),
        ),
    ];
    let mut failed = adequacy.iter().any(|r| !r.passed);
    reasons.extend(adequacy);

    if !failed && criteria.runs(Stage::S0) {
        decided_at = Stage::S0;
        // A signal that predicts nothing is dead here, and nothing downstream
        // can rescue it — that is the entire argument for a predictor-first
        // gate (D-0081).
        let threshold = criteria.s0_min_abs_ic.unwrap_or(0.0);
        let s0 = vec![check(
            Stage::S0,
            s0.is_some_and(crate::s0::S0ComboReport::passed),
            s0_detail(s0, threshold),
        )];
        failed = s0.iter().any(|r| !r.passed);
        reasons.extend(s0);
    }

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

    // The permutation null — the first real piece of S3, and the one gate in
    // this build that asks a question no statistic computed ON the run can
    // answer: would this strategy have produced the same result on a series
    // with the same shape but no long-range ordering? A leak that re-fits on
    // every permutation answers "yes" and dies here (CLAUDE.md §9).
    //
    // Gated on the criterion being SUPPLIED rather than on `stages` naming s3,
    // because s3 as a declarable stage still needs the rest of the battery
    // (deflated Sharpe, PBO, truncation) and is still refused at load (D-0075).
    if !failed && let Some(max_p) = criteria.max_permutation_p {
        decided_at = Stage::S3;
        let s3 = vec![check(
            Stage::S3,
            evidence.permutation_p_value.is_some_and(|p| p <= max_p),
            match evidence.permutation_p_value {
                Some(p) => format!(
                    "permutation-null p = {p:.4}, {max_p:.4} required — the observed edge must \
                     be extreme against the same strategy run on block-shuffled series, or it \
                     is not a property of this data"
                ),
                None => format!(
                    "no permutation null was built, {max_p:.4} required — an absent null is \
                     not a cleared bar (D-0075)"
                ),
            },
        )];
        failed = s3.iter().any(|r| !r.passed);
        reasons.extend(s3);
    }

    // Deflated Sharpe and PBO — the rest of S3 that block B wires in. Both are
    // evaluated whenever the stage is reached at all, rather than on an
    // Option-shaped criterion, because both criteria are always declared: a
    // config cannot opt out of being told how many times it searched.
    if !failed {
        decided_at = Stage::S3;
        let s3 = vec![
            check(
                Stage::S3,
                evidence
                    .deflated
                    .is_some_and(|d| d.beats_selection_benchmark()),
                match evidence.deflated {
                    Some(d) => format!(
                        "deflated Sharpe: observed {:.4} against a selection benchmark of {:.4} \
                         over {} trial(s) — the benchmark is where noise alone would have put \
                         the maximum of a search this wide, so an observed Sharpe below it is \
                         unremarkable however large it looks. P(true Sharpe > 0) = {:.4}",
                        d.observed_sharpe, d.expected_max_sharpe, d.n_trials, d.dsr
                    ),
                    None => format!(
                        "no deflated Sharpe could be computed over {} trial(s) — the combo has \
                         no out-of-sample Sharpe, or its return window is flat enough to have \
                         no higher moments. An absent correction is not a cleared bar (D-0075)",
                        evidence.n_trials
                    ),
                },
            ),
            check(
                Stage::S3,
                evidence.pbo.is_some_and(|p| p <= criteria.max_pbo),
                match evidence.pbo {
                    Some(p) => format!(
                        "PBO = {p:.4}, {:.4} required — the share of in-sample/out-of-sample \
                         recombinations in which this grid's in-sample winner finished below \
                         median out of sample. Near 0.5 means the selection carried no \
                         information",
                        criteria.max_pbo
                    ),
                    None => format!(
                        "no PBO was computed, {:.4} required — CSCV needs at least two combos \
                         and an even number of at least two folds. An absent estimate is not a \
                         cleared bar (D-0075)",
                        criteria.max_pbo
                    ),
                },
            ),
        ];
        failed = s3.iter().any(|r| !r.passed);
        reasons.extend(s3);
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
    use crate::s0::{
        Availability, HorizonEvidence, Interval, S0ComboReport, S0Spec, UnavailableReason,
    };

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
            s0_min_abs_ic: None,
            max_permutation_p: None,
        }
    }

    fn criteria() -> Criteria {
        let stages = names(&["s1", "s2"]);
        Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.0)).expect("valid")
    }

    fn s0_criteria() -> Criteria {
        let stages = names(&["s0", "s1", "s2"]);
        let sweep = [0.0, 0.5, 1.0, 2.0];
        let mut declared = source(&stages, &sweep, 1.0);
        declared.s0_min_abs_ic = Some(0.05);
        Criteria::new(&declared).expect("valid S0 criteria")
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
            permutation_p_value: None,
            // Block B's two S3 gates are evaluated whenever the stage is
            // reached, so a fixture meaning "everything passes" has to clear
            // them too: a Sharpe comfortably above its selection benchmark,
            // and a grid whose in-sample winner usually held up out of sample.
            deflated: Some(crate::stats::deflated::Deflated {
                observed_sharpe: 1.2,
                expected_max_sharpe: 0.4,
                standard_error: 0.1,
                dsr: 0.99,
                n_trials: 6,
            }),
            n_trials: 6,
            pbo: Some(0.1),
        }
    }

    fn measured_failed_s0() -> S0ComboReport {
        S0ComboReport {
            score_identity: "zscore close period=20".to_owned(),
            tick_size_nanopoints: 250_000_000,
            spec: S0Spec {
                score_slot: "z".to_owned(),
                horizons_ns: vec![60_000_000_000],
                buckets: 5,
                bootstrap_draws: 200,
                min_abs_ic: 0.05,
            },
            combo_index: 0,
            label: "z(period=20 source=close)".to_owned(),
            warmup_bars: 20,
            scores: 100,
            horizons: vec![HorizonEvidence {
                horizon_ns: 60_000_000_000,
                n_pairs: 99,
                dropped_no_partner: 1,
                dropped_invalid_price: 0,
                ic: Availability::Available { value: 0.08 },
                buckets: Availability::Unavailable {
                    reason: UnavailableReason::NoBucketsDeclared,
                },
                unconditional_mean_interval: Availability::Available {
                    value: Interval {
                        point: 0.0,
                        lo: -0.01,
                        hi: 0.01,
                        draws: 200,
                    },
                },
            }],
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
        // s3 is what remains unimplemented. `s0` left this list on 2026-07-31
        // when its caller landed (D-0085) — the list IS the record of which
        // stages this build cannot run, so removing s0 from it is the change.
        let stages = names(&["s1", "s2", "s3"]);
        let err =
            Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.0)).expect_err("must refuse");
        let text = err.to_string();
        assert!(text.contains("not implemented"), "{text}");
        assert!(text.contains("refused"), "{text}");
        assert!(text.contains("PBO"), "{text}");
        // And s0 is now accepted rather than refused — but only with its
        // `[s0]` block, which `source` supplies as `None` here.
        let stages = names(&["s0", "s1", "s2"]);
        let err =
            Criteria::new(&source(&stages, &[0.0, 0.5, 1.0, 2.0], 1.0)).expect_err("needs [s0]");
        assert_eq!(err, CriteriaError::S0BlockMissing, "{err}");
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
    fn too_few_trades_is_killed_at_admission_with_every_adequacy_reason_reported() {
        let mut e = passing();
        e.oos_trades = 3;
        e.oos_sessions = 4;
        let a = assess(&criteria(), &e, None);
        assert_eq!(a.verdict, Verdict::Kill);
        assert_eq!(a.decided_at, Stage::Admission);
        assert_eq!(a.reasons.len(), 2, "both adequacy criteria are evaluated");
        assert!(a.reasons.iter().all(|r| !r.passed));
        // And nothing downstream was judged on a sample that small.
        assert!(a.reasons.iter().all(|r| r.stage == Stage::Admission));
        // The label is the point of D-0084: this reads `admission`, and a
        // config declaring a stage by that name is still refused, because
        // admission is a pre-trial check rather than a gate.
        assert_eq!(a.decided_at.to_string(), "admission");
    }

    #[test]
    fn measured_s0_failure_is_not_relabelled_as_absent_ic() {
        let s0 = measured_failed_s0();
        let assessment = assess(&s0_criteria(), &passing(), Some(&s0));
        assert_eq!(assessment.verdict, Verdict::Kill);
        assert_eq!(assessment.decided_at, Stage::S0);
        let reasons = assessment.rendered_reasons().join("\n");
        assert!(reasons.contains("best measurable |IC| 0.0800"), "{reasons}");
        assert!(!reasons.contains("IC unavailable"), "{reasons}");
    }

    #[test]
    fn absent_s0_interval_remains_unevaluated_not_measured_failure() {
        let mut s0 = measured_failed_s0();
        s0.horizons[0].unconditional_mean_interval = Availability::Unavailable {
            reason: UnavailableReason::NoBootstrapDraws,
        };
        let assessment = assess(&s0_criteria(), &passing(), Some(&s0));
        assert_eq!(assessment.verdict, Verdict::Kill);
        assert_eq!(assessment.decided_at, Stage::S0);
        let reasons = assessment.rendered_reasons().join("\n");
        assert!(
            reasons.contains("criterion remains unevaluated"),
            "{reasons}"
        );
        assert!(!reasons.contains("best measurable"), "{reasons}");
    }

    /// `admission` is not a declarable gate, and the refusal names it as an
    /// unknown stage rather than accepting it (D-0084). A config that could
    /// declare the admission check could also *skip* it.
    #[test]
    fn a_config_cannot_declare_admission_as_a_stage() {
        let e = "admission".parse::<Stage>();
        assert!(
            matches!(e, Err(CriteriaError::UnknownStage { .. })),
            "{e:?}"
        );
    }

    /// Every declarable spelling still round-trips, so the new variant did not
    /// disturb the four that configs actually use.
    #[test]
    fn the_four_declarable_stages_still_round_trip() {
        for (text, stage) in [
            ("s0", Stage::S0),
            ("s1", Stage::S1),
            ("s2", Stage::S2),
            ("s3", Stage::S3),
        ] {
            assert_eq!(text.parse::<Stage>().expect("parses"), stage);
            assert_eq!(stage.to_string(), text);
        }
    }

    /// The cheapest kill available: not clearly positive even cost-free.
    #[test]
    fn a_strategy_that_loses_with_free_fills_dies_at_s1() {
        let mut e = passing();
        e.free_fill_return_pct = -2.0;
        let a = assess(&criteria(), &e, None);
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
        let a = assess(&criteria(), &e, None);
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
            assess(&criteria(), &beaten_by_random, None).verdict,
            Verdict::Kill
        );

        let mut beaten_by_market = passing();
        beaten_by_market.buy_and_hold_return_pct = Some(9.0);
        assert_eq!(
            assess(&criteria(), &beaten_by_market, None).verdict,
            Verdict::Kill
        );
    }

    /// An absent control is not a passed control. This is the one that stops a
    /// scorecard from quietly rendering a verdict with a missing denominator.
    #[test]
    fn a_control_that_could_not_be_built_is_a_failure_not_a_pass() {
        let mut e = passing();
        e.random_entry_return_pct = None;
        let a = assess(&criteria(), &e, None);
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
        assert_eq!(assess(&criteria(), &e, None).verdict, Verdict::Kill);
    }

    /// The ceiling this build imposes: everything passes, and the answer is
    /// still Iterate, because Graduate means "survived the full battery" and
    /// the battery is not finished.
    ///
    /// `decided_at` reads **S3** rather than S2 since block B wired the
    /// deflated Sharpe and PBO into the run path: part of the battery now
    /// runs, so part of the battery is now the last stage reached. That is a
    /// real change and is asserted rather than absorbed — but it moves the
    /// *stage*, not the *ceiling*. Graduate stays unreachable while the
    /// permutation null and the truncation harness are still unwired and the
    /// plateau and regime tables are still holes (D-0075), and changing this
    /// assertion to `Graduate` needs the superseding entry D-0075 demands, not
    /// a longer reasons list.
    #[test]
    fn passing_everything_this_build_runs_is_iterate_and_never_graduate() {
        let a = assess(&criteria(), &passing(), None);
        assert_eq!(a.verdict, Verdict::Iterate);
        assert_eq!(a.decided_at, Stage::S3);
        assert!(a.reasons.iter().all(|r| r.passed));
        assert!(a.reasons.len() >= 6, "{:?}", a.rendered_reasons());
        assert!(
            a.reasons
                .iter()
                .any(|r| r.stage == Stage::S3 && r.detail.contains("deflated Sharpe")),
            "the deflated Sharpe must appear as an evaluated criterion: {:?}",
            a.rendered_reasons()
        );
        assert!(
            a.reasons
                .iter()
                .any(|r| r.stage == Stage::S3 && r.detail.contains("PBO")),
            "PBO must appear as an evaluated criterion: {:?}",
            a.rendered_reasons()
        );
    }

    /// The two block-B gates must be able to KILL, not merely to appear.
    ///
    /// Written as the converse of the test above: same fixture, one field
    /// moved at a time. A gate that only ever passes is decoration (§7), and
    /// an absent estimate must fail rather than wave the combo through — the
    /// same rule an absent control follows (D-0075).
    #[test]
    fn each_block_b_gate_kills_on_its_own() {
        let mut deflation_failed = passing();
        deflation_failed.deflated = Some(crate::stats::deflated::Deflated {
            observed_sharpe: 0.3,
            expected_max_sharpe: 0.9,
            standard_error: 0.1,
            dsr: 0.2,
            n_trials: 24,
        });
        let a = assess(&criteria(), &deflation_failed, None);
        assert_eq!(a.verdict, Verdict::Kill, "{:?}", a.rendered_reasons());
        assert_eq!(a.decided_at, Stage::S3);

        let mut deflation_absent = passing();
        deflation_absent.deflated = None;
        assert_eq!(
            assess(&criteria(), &deflation_absent, None).verdict,
            Verdict::Kill,
            "an absent deflation is not a cleared bar"
        );

        let mut pbo_failed = passing();
        pbo_failed.pbo = Some(0.9);
        let a = assess(&criteria(), &pbo_failed, None);
        assert_eq!(a.verdict, Verdict::Kill, "{:?}", a.rendered_reasons());

        let mut pbo_absent = passing();
        pbo_absent.pbo = None;
        assert_eq!(
            assess(&criteria(), &pbo_absent, None).verdict,
            Verdict::Kill,
            "an absent PBO is not a cleared bar"
        );

        // And the boundary is inclusive: a PBO exactly at the declared bar
        // passes, so the criterion reads "at most" rather than "under".
        let mut pbo_at_bar = passing();
        pbo_at_bar.pbo = Some(criteria().max_pbo);
        assert_eq!(
            assess(&criteria(), &pbo_at_bar, None).verdict,
            Verdict::Iterate
        );
    }
}
