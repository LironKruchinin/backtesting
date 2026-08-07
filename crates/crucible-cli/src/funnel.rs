//! `crucible funnel` — the command that produces a verdict.
//!
//! Same config, same shared bar series and same fold layout as `crucible
//! walk-forward`. The difference is what comes out: `walk-forward` prints
//! evidence and says in its own footer that evidence is not a conclusion; this
//! command adds the parts that turn one into the other — the registry claim
//! before every run, the cost-free screen, the mandatory 0 / 0.5 / 1 / 2 tick
//! sweep, the two controls, and the pre-registered kill criteria — and writes
//! a scorecard.
//!
//! It runs unattended: no prompts, no interactive choices, one exit code.
//!
//! ## Three refusals, all before any bar is replayed
//!
//! - **No `[funnel]` section.** There is nothing pre-registered to judge
//!   against, and inventing criteria at judging time is the one thing
//!   `PROJECT_PLAN.md` §7.2 forbids by name.
//! - **`fill_model = "free_fills"`.** The funnel runs the free-fill screen
//!   itself at S1 and then asks S2 whether the edge survives honest costs. A
//!   config declaring `free_fills` would make those the same run and the cost
//!   sweep a table of one number repeated (D-0006).
//! - **No git sha.** §2.5: every persisted result carries one, and a number
//!   that cannot be reproduced is a rumor. Resolved from `git rev-parse HEAD`
//!   in the working directory, or from `CRUCIBLE_GIT_SHA` when the process is
//!   not running inside the checkout.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the grid ran, every combo has a verdict, and the scorecard was written |
//! | 2 | the config is wrong, the data does not exist, or provenance is missing |
//! | 4 | data exists but could not be trusted |
//! | 5 | the run completed and **every combo was killed** |
//!
//! Exit 5 is not a failure — most ideas must die, and cheaply. It exists
//! because a scheduled job that reads "everything was killed" as success
//! learns nothing, exactly as `qa` exits 4 on findings.

use std::fmt::Write as _;
use std::path::PathBuf;

use crucible_core::prelude::*;
use crucible_engine::INTRABAR_CONVENTION;
use crucible_funnel::registry::Registry;
use crucible_funnel::scorecard::{self, Provenance};
use crucible_funnel::stages::{Criteria, Verdict};
use crucible_funnel::walkforward::{FoldPlan, FoldSpec, RunIdentity};
use crucible_funnel::{ComboOutcome, Costs, FunnelInputs, FunnelReport, run_funnel};

use crate::combo::{annualization, attach_sessions, collect_events, print_header, usd};
use crate::config::{self, Consumer, DataSource, LoadedConfig};
use crate::pull::EXIT_USAGE;
use crate::walkforward::trading_days;

/// Every combo was killed. See the module docs.
pub const EXIT_ALL_KILLED: i32 = 5;

/// Arguments to `crucible funnel`.
#[derive(Debug, clap::Args)]
pub struct FunnelArgs {
    /// Path to the combo config. Must carry `[walk_forward]` and `[funnel]`.
    #[arg(long)]
    pub config: PathBuf,
    /// Where the registry and the scorecards go. Created if absent.
    #[arg(long, default_value = "results")]
    pub out: PathBuf,
    /// Print only the determinism hash of the verdicts. The funnel CI gate.
    #[arg(long)]
    pub hash_only: bool,
    /// Judge the config and stop: every refusal this command makes before it
    /// reads a bar, and none of the work after. Touches no archive, charges no
    /// trial, writes nothing.
    ///
    /// It exists so a *registration* can be checked without being run —
    /// `tests/backlog_registration.rs` points it at every embedded config
    /// block in `research/backlog/`, which is how a hypothesis file that
    /// declares a stage without the section that stage needs is caught the day
    /// it is written rather than the day someone tries to run it.
    #[arg(long)]
    pub check_config: bool,
}

/// Everything this command can decide about a config **before** a bar exists.
///
/// One function, because two callers must get the identical answer: the run
/// path below, and `--check-config`. A lint that re-listed these requirements
/// would be a second copy of them, and the requirement the copy dropped would
/// be the one nobody noticed — which is exactly the failure that let two
/// backlog registrations sit unrunnable (D-0101).
///
/// The git sha is deliberately **not** checked here. It is provenance, not
/// configuration: a registration is well-formed or not regardless of whether
/// the process can see a checkout, and folding the two together would make the
/// lint fail for a reason that has nothing to do with the file it is linting.
struct Preflight<'a> {
    /// The pre-registered criteria, judged coherent.
    criteria: Criteria,
    /// The fold layout `[walk_forward]` describes.
    fold_spec: FoldSpec,
    /// The section itself, for the report header.
    walk_forward: &'a config::WalkForward,
}

/// Judges a loaded config against everything knowable without data.
///
/// Returns the message to print on refusal — already a whole explanation, so
/// both callers print it the same way.
fn preflight(loaded: &LoadedConfig) -> Result<Preflight<'_>, String> {
    let Some(cfg) = loaded.file.funnel.as_ref() else {
        return Err(format!(
            "{} declares no [funnel] section, so there are no pre-registered criteria to\n\
             \x20      judge against. Add one — stages, cost_sensitivity_ticks, min_oos_trades,\n\
             \x20      min_oos_sessions, min_oos_return_pct_free_fills,\n\
             \x20      min_oos_sharpe_after_costs, kill_if_dead_at_ticks,\n\
             \x20      require_controls_beaten, max_pbo, require_plateau.\n\
             \x20      Criteria written after seeing results are not criteria.",
            loaded.path.display()
        ));
    };
    // `Criteria::new` owns the stage rules, including the `s0` <-> `[s0]`
    // biconditional (D-0085). Asking it is what makes this function report the
    // loader's requirements rather than a remembered list of them.
    let criteria = cfg
        .to_criteria(loaded.file.s0.as_ref().map(|c| c.min_abs_ic))
        .map_err(|e| e.to_string())?;
    if loaded.file.execution.fill_model == "free_fills" {
        return Err(
            "execution.fill_model = \"free_fills\", which `funnel` refuses.\n\
             \x20      The funnel runs the free-fill screen itself at S1 and then asks S2 whether\n\
             \x20      the edge survives honest costs; with free_fills declared those are the\n\
             \x20      same run and the mandatory cost sweep is one number repeated four times.\n\
             \x20      FreeFills is a screening tool, never a result (D-0006)."
                .to_owned(),
        );
    }
    let Some(wf) = loaded.file.walk_forward.as_ref() else {
        return Err(format!(
            "{} declares no [walk_forward] section. The funnel's S2 gate is a\n\
             \x20      walk-forward under costs, so there is nothing to gate without folds.",
            loaded.path.display()
        ));
    };
    let fold_spec = wf.to_fold_spec().map_err(|e| e.to_string())?;
    // The `[s0]` block's `score` names an indicator slot, and a slot that does
    // not resolve is an unevaluable criterion — the same class of config bug as
    // a missing section, and one that would otherwise surface after the grid
    // had already been replayed. Combo 0 answers it: every combo of a grid
    // carries the same slots, differing only in their parameters.
    if criteria.runs(crucible_funnel::stages::Stage::S0)
        && let Some(s0) = loaded.file.s0.as_ref()
        && let Some(first) = loaded.grid.iter().next()
    {
        crucible_strategies::combo::ComboScorer::build(loaded.grid.spec(), &first, &s0.score)
            .map_err(|e| format!("s0.score: {e}"))?;
    }
    crucible_funnel::grid::check_size(loaded.grid.len(), &loaded.file.meta.hypothesis_family)
        .map_err(|e| e.to_string())?;
    Ok(Preflight {
        criteria,
        fold_spec,
        walk_forward: wf,
    })
}

/// Runs the command, returning the process exit code.
#[expect(
    clippy::too_many_lines,
    reason = "this is the wiring the command exists to be: load, refuse, collect, plan, run, \
              render. Splitting it would spread one linear story across six functions that are \
              each called once"
)]
pub fn run_cmd(args: &FunnelArgs) -> i32 {
    let mut loaded = match config::load(&args.config) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    let (criteria, fold_spec, wf) = match preflight(&loaded) {
        Ok(p) => (p.criteria, p.fold_spec, p.walk_forward),
        Err(message) => {
            eprintln!("error: {message}");
            return EXIT_USAGE;
        }
    };
    if args.check_config {
        println!(
            "{} is a runnable funnel registration.\n  stages         {}\n  folds          \
             {} train {}d / test {}d / step {}d\n  grid           {} combo(s)\n\nNothing was run: \
             --check-config judges the config and stops.",
            loaded.path.display(),
            criteria
                .stages
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            wf.scheme,
            wf.train_days,
            wf.test_days,
            wf.step_days.unwrap_or(wf.test_days),
            loaded.grid.len(),
        );
        return 0;
    }
    let git_sha = match resolve_git_sha() {
        Ok(sha) => sha,
        Err(message) => {
            eprintln!("error: {message}");
            return EXIT_USAGE;
        }
    };

    // ---- The pooled path (C6b-iii) ----------------------------------------
    //
    // Taken before `collect_events`, which reads `universe.instruments[0]` and
    // would replay one contract under a pooled config's header — the exact
    // half-wired answer D-0117 refused rather than allow.
    if loaded.file.pooling.is_some() {
        return run_pooled(&loaded, args, &git_sha, fold_spec, &criteria);
    }

    let series = match collect_events(&loaded) {
        Ok(series) => series,
        Err((code, message)) => {
            eprintln!("error: {message}");
            return code;
        }
    };
    if series.events.is_empty() {
        eprintln!("error: the data source produced no bars; there is nothing to replay");
        return EXIT_USAGE;
    }
    if let Err(message) = attach_sessions(&mut loaded, &series.events) {
        eprintln!("error: {message}");
        return EXIT_USAGE;
    }
    let days = match trading_days(&loaded, &series.events) {
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

    // A determinism gate is a question about the CODE, not a piece of research:
    // it must charge no trial and leave no row behind (D-0083). The ephemeral
    // registry honours insert-before-run in memory and never opens the file, so
    // `--hash-only` cannot contaminate the research memory it is meant to be
    // cheap to run against.
    let registry_path = args.out.join("registry.jsonl");
    let mut registry = if args.hash_only {
        Registry::ephemeral(&registry_path)
    } else {
        match Registry::open(&registry_path) {
            Ok(registry) => registry,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_USAGE;
            }
        }
    };

    let bars_per_year = annualization(&loaded, &series.events);
    let params = crucible_engine::BacktestParams {
        initial_cash_nano_usd: loaded.initial_cash_nano_usd,
        bars_per_year,
    };
    let s0_spec = loaded
        .file
        .s0
        .as_ref()
        .map(|cfg_s0| crucible_funnel::s0::S0Spec {
            score_slot: cfg_s0.score.clone(),
            horizons_ns: cfg_s0
                .horizons_minutes
                .iter()
                .map(|m| i64::from(*m) * 60_000_000_000)
                .collect(),
            buckets: cfg_s0.buckets,
            bootstrap_draws: cfg_s0.bootstrap_draws,
            min_abs_ic: cfg_s0.min_abs_ic,
        });
    let s0_data_source = if criteria.runs(crucible_funnel::stages::Stage::S0) {
        match s0_data_source_identity(&loaded) {
            Ok(data_source) => Some(data_source),
            Err(message) => {
                eprintln!("error: {message}");
                return EXIT_USAGE;
            }
        }
    } else {
        None
    };
    let registration_hash = if criteria.runs(crucible_funnel::stages::Stage::S0) {
        let Some(spec) = s0_spec.as_ref() else {
            eprintln!("error: `stages` declares s0 but there is no `[s0]` block");
            return EXIT_USAGE;
        };
        let data_source = s0_data_source
            .as_ref()
            .expect("S0 data identity was constructed above");
        match crucible_funnel::s0::s0_run_registration_hash(
            loaded.grid.spec(),
            spec,
            loaded.spec.tick,
            data_source,
            &series.events,
            &days.keys,
            &series.data_manifest_ids,
        ) {
            Ok(hash) => hash,
            Err(error) => {
                eprintln!("error: {error}");
                return EXIT_USAGE;
            }
        }
    } else {
        loaded.config_hash
    };
    let identity = RunIdentity {
        config_hash: registration_hash,
        root_seed: loaded.file.run.seed,
        // No account is evaluated yet: the breach probability, the day-block
        // bootstrap and P(pass) are `ACCOUNT_EVAL_SPEC.md` §4 and land with the
        // block that consumes the day series this run already captures. The
        // field is threaded through the run identity and the seed lineage now
        // (D-0067) so that the first account to arrive cannot be charged
        // silently.
        account_id: None,
    };
    let now = timestamp();
    // ---- S0, ahead of everything, because a score that predicts nothing is
    // dead before any equity curve exists (D-0081/D-0085). It takes no
    // position, so it has no fill model and no replay: one streaming pass per
    // combo over the same bars.
    let mut s0_report = None;
    if criteria.runs(crucible_funnel::stages::Stage::S0) {
        let Some(spec) = s0_spec.as_ref() else {
            eprintln!("error: `stages` declares s0 but there is no `[s0]` block");
            return EXIT_USAGE;
        };
        let s0_inputs = crucible_funnel::s0::S0Inputs {
            data_source: s0_data_source
                .as_ref()
                .expect("S0 data identity was constructed above"),
            events: &series.events,
            day_keys: &days.keys,
            grid: &loaded.grid,
            s0: spec,
            tick_size: loaded.spec.tick,
            identity: &identity,
            criteria: &criteria,
            hypothesis_family: &loaded.file.meta.hypothesis_family,
            git_sha: &git_sha,
            data_manifest_ids: &series.data_manifest_ids,
            now: &now,
        };
        match crucible_funnel::s0::run_s0(&s0_inputs, &mut registry) {
            Ok(r) => s0_report = Some(r),
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_USAGE;
            }
        }
    }

    let inputs = FunnelInputs {
        events: &series.events,
        day_keys: &days.keys,
        grid: &loaded.grid,
        plan: &plan,
        spec: &loaded.spec,
        params: &params,
        identity: &identity,
        criteria: &criteria,
        s0_spec: s0_spec.as_ref(),
        s0_data_source: s0_data_source.as_ref(),
        costs: Costs {
            half_spread_ticks: loaded.file.execution.half_spread_ticks,
            fee_per_contract_nano_usd: loaded.fee_per_contract_nano_usd,
        },
        qty: Qty(loaded.file.run.qty_contracts),
        hypothesis_family: &loaded.file.meta.hypothesis_family,
        git_sha: &git_sha,
        data_manifest_ids: &series.data_manifest_ids,
        now: &now,
    };

    let report = match run_funnel(&inputs, &mut registry, s0_report) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    if args.hash_only {
        println!("{:016x}", verdict_hash(&report));
        return 0;
    }

    if let Some(s0) = report.s0.as_ref() {
        print!("{}", format_s0(s0));
    }

    let provenance = Provenance {
        idea_name: loaded.file.meta.name.clone(),
        hypothesis_family: loaded.file.meta.hypothesis_family.clone(),
        economic_rationale: loaded.file.meta.economic_rationale.clone(),
        config_hash: loaded.config_hash.to_string(),
        registration_hash: identity.config_hash.to_string(),
        git_sha: git_sha.clone(),
        data_manifest_ids: series.data_manifest_ids.clone(),
        data_source: series.description.clone(),
        universe: format!(
            "{} {}",
            loaded.file.universe.instruments[0], loaded.timeframe
        ),
        fill_model: format!(
            "spread_cross — {} tick half-spread, {}/contract/side",
            loaded.file.execution.half_spread_ticks,
            usd(loaded.fee_per_contract_nano_usd)
        ),
        intrabar_convention: INTRABAR_CONVENTION.to_owned(),
        capital: format!(
            "{} initial, {} contract(s)",
            usd(loaded.initial_cash_nano_usd),
            loaded.file.run.qty_contracts
        ),
        rendered_at: now.clone(),
    };
    let html = match scorecard::render(&report, &criteria, &provenance) {
        Ok(html) => html,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };
    let card = args
        .out
        .join(format!("scorecard-{}.html", identity.config_hash.short()));
    if let Err(e) = std::fs::write(&card, html) {
        eprintln!("error: cannot write {}: {e}", card.display());
        return EXIT_USAGE;
    }

    print_header(&loaded, "funnel");
    println!(
        "  registration   {}  (registry/run identity, D-0106)",
        identity.config_hash
    );
    println!("  bars           {}", series.description);
    crate::grain::print_caveats(&series.caveats);
    print_report(&loaded, &report, &criteria, &days, series.events.len());
    println!("  registry       {}", registry.path().display());
    println!("  scorecard      {}", card.display());
    println!("  determinism    {:016x}", verdict_hash(&report));
    print_footer(&loaded, &report);

    if report
        .combos
        .iter()
        .all(|c| c.assessment.verdict == Verdict::Kill)
    {
        return EXIT_ALL_KILLED;
    }
    0
}

fn s0_data_source_identity(
    loaded: &LoadedConfig,
) -> Result<crucible_funnel::s0::S0DataSourceIdentity, String> {
    let instrument = loaded.spec.instrument.clone();
    let timeframe = loaded.timeframe;
    match &loaded.file.data {
        DataSource::Curated { start, end } => {
            Ok(crucible_funnel::s0::S0DataSourceIdentity::Curated {
                instrument,
                timeframe,
                start: start.clone(),
                end: end.clone(),
            })
        }
        DataSource::Synthetic {
            seed,
            bars,
            start_price_points,
            vol_ticks,
        } => {
            let start_price_nanopoints = Price::from_points_str(start_price_points)
                .map_err(|error| format!("data.start_price_points: {error}"))?
                .as_nanos();
            Ok(crucible_funnel::s0::S0DataSourceIdentity::Synthetic {
                instrument,
                timeframe,
                seed: *seed,
                bars: *bars,
                start_price_nanopoints,
                vol_ticks: *vol_ticks,
            })
        }
    }
}

/// The repository revision (§2.5).
///
/// `git rev-parse HEAD` in the working directory, falling back to
/// `CRUCIBLE_GIT_SHA`. Refusing when neither answers is deliberate: the
/// alternative is a stored result whose provenance reads `unknown`, which is
/// the same as no provenance and looks like provenance.
fn resolve_git_sha() -> Result<String, String> {
    if let Ok(sha) = std::env::var("CRUCIBLE_GIT_SHA")
        && !sha.trim().is_empty()
    {
        return Ok(sha.trim().to_owned());
    }
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if sha.is_empty() {
                Err(git_sha_help())
            } else {
                Ok(sha)
            }
        }
        _ => Err(git_sha_help()),
    }
}

fn git_sha_help() -> String {
    "cannot resolve a git sha, and CLAUDE.md §2.5 requires every persisted result to carry one \
     — a number that cannot be reproduced is a rumor and does not get stored.\n       Run this \
     from inside the checkout, or set CRUCIBLE_GIT_SHA to the revision the\n       binary was \
     built from."
        .to_owned()
}

/// Wall clock, ISO-8601 UTC to the second.
///
/// Read through [`SystemClock`], which is the workspace's one OS-clock
/// implementation (D-0032), and handed to the funnel and the registry as a
/// **string**: neither reads a clock, for the same reason `Catalog::append`
/// takes an `acquired_ts` (D-0015). It is metadata and never reaches a result
/// — `--hash-only` deliberately does not hash it, so two runs a minute apart
/// still produce the same gate value.
fn timestamp() -> String {
    let nanos = crate::pull::SystemClock.now_ts().0.max(0);
    let secs = nanos / 1_000_000_000;
    let date = crucible_data::ingest::window::civil_from_days(secs / 86_400);
    let rem = secs % 86_400;
    format!(
        "{date}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn print_report(
    loaded: &LoadedConfig,
    report: &FunnelReport,
    criteria: &crucible_funnel::Criteria,
    days: &crate::walkforward::TradingDays,
    n_bars: usize,
) {
    println!(
        "  stages         {} — declared S0 signal triage runs before trading evidence; S3's\n\
         \x20                remaining battery is refused, not silently skipped",
        criteria
            .stages
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  criteria       {} OOS trade(s), {} OOS session(s), {:+.2}% free-fill return,\n\
         \x20                {:.2} costed Sharpe, still alive at {} tick(s), controls {}",
        criteria.min_oos_trades,
        criteria.min_oos_sessions,
        criteria.min_oos_return_pct_free_fills,
        criteria.min_oos_sharpe_after_costs,
        criteria.kill_level_ticks(),
        if criteria.require_controls_beaten {
            "must be beaten"
        } else {
            "reported but not required"
        }
    );
    match &days.calendar_id {
        Some(id) => println!("  trading days   {id}"),
        None => println!("  trading days   UTC civil days — no bundled calendar governs this"),
    }
    if let Some(caveat) = &days.caveat {
        println!("\n  NOTE: {caveat}.\n");
    }
    println!(
        "  replay         {n_bars} bars, one series shared by every combo; {} evaluable\n\
         \x20                trading day(s) across {} fold(s)",
        report.combos.first().map_or(0, |c| c.oos_sessions),
        report.combos.first().map_or(0, |c| c.costed.folds.len())
    );
    println!(
        "  trials         {} charged to {} (was {} before this run)",
        report.trials_after, loaded.file.meta.hypothesis_family, report.trials_before
    );
    println!(
        "  registry       {} run(s) claimed, {} already finished, {} re-run after an\n\
         \x20                unfinished claim",
        report.runs_claimed, report.runs_already_done, report.runs_retried
    );
    println!();

    println!(
        "  {:>5}  {:<34} {:>9} {:>10} {:>9} {:>7}  {:>9} {:>9}",
        "combo", "parameters", "verdict", "decided at", "OOS ret", "trades", "vs random", "vs B&H"
    );
    for c in &report.combos {
        println!(
            "  {:>5}  {:<34} {:>9} {:>10} {:>8.2}% {:>7}  {:>9} {:>9}",
            c.id.combo_index,
            c.label,
            c.assessment.verdict,
            c.assessment.decided_at,
            c.costed.oos_stitched.total_return_pct,
            c.costed.oos_stitched.round_trips,
            control_gap(c, 0),
            control_gap(c, 1),
        );
    }
    println!();

    for c in &report.combos {
        println!("  combo {} — {}", c.id.combo_index, c.label);
        for reason in c.assessment.rendered_reasons() {
            println!("    {reason}");
        }
        print!("    cost sweep:");
        for level in &c.sweep {
            print!(
                "  {}t {:+.2}%",
                level.ticks(),
                level.oos_stitched.total_return_pct
            );
        }
        println!("  |  free_fills {:+.2}%", c.free_fill_oos.total_return_pct);
        for control in &c.controls {
            match &control.oos_stitched {
                Some((s, _)) => println!(
                    "    {:<22} {:+.2}%{}  — this combo beat {} of {} draw(s)",
                    control.name,
                    s.total_return_pct,
                    if control.draws > 1 {
                        format!(" (median of {})", control.draws)
                    } else {
                        String::new()
                    },
                    control.draws_beaten,
                    control.draws,
                ),
                None => println!(
                    "    {:<22} ABSENT — {}",
                    control.name,
                    control
                        .absent_because
                        .as_deref()
                        .unwrap_or("no reason recorded")
                ),
            }
        }
        if let Some(worst) = c.costed.oos_worst_days.worst_close_nano_usd() {
            println!(
                "    worst OOS day: {} closing, {} at its intraday trough over {} session(s) —\n\
                 \x20   the gap is the part of a bad day a daily-close model never sees",
                usd(worst),
                usd(c.costed.oos_worst_days.worst_trough_nano_usd().unwrap_or(0)),
                c.costed.oos_worst_days.n_days()
            );
        }
        println!();
    }
}

/// `+1.23%` — the combo's pooled OOS return minus a control's, or `ABSENT`.
fn control_gap(c: &ComboOutcome, which: usize) -> String {
    c.controls[which].return_pct().map_or_else(
        || "ABSENT".to_owned(),
        |control| format!("{:+.2}%", c.costed.oos_stitched.total_return_pct - control),
    )
}

/// FNV-1a over every combo's verdict and the numbers behind it.
///
/// The wall clock, the registry paths and the scorecard's HTML are all
/// deliberately outside it: they change every run and none of them is a
/// result. What is inside is the verdict, the stage that decided it, and every
/// pooled statistic it was decided on — so a change that moves a verdict fails
/// the gate and a change that only moves a timestamp does not.
fn verdict_hash(report: &FunnelReport) -> u64 {
    let mut h = crate::Fnv64::new();
    let i64_of = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    // S0 first, because it runs first. Absent when the config declared no
    // `s0`, in which case nothing is written and the hash is unmoved — which
    // is what keeps every pre-S0 config's pinned gate valid (D-0085).
    if let Some(s0) = report.s0.as_ref() {
        let encoded = s0.determinism_bytes();
        h.write_i64(i64_of(encoded.len()));
        for byte in encoded {
            h.write_i64(i64::from(byte));
        }
    }
    for c in &report.combos {
        h.write_i64(i64_of(c.id.combo_index));
        h.write_i64(match c.assessment.verdict {
            Verdict::Kill => 0,
            Verdict::Iterate => 1,
            Verdict::Graduate => 2,
        });
        for byte in c.assessment.decided_at.to_string().bytes() {
            h.write_i64(i64::from(byte));
        }
        h.write_i64(i64_of(c.oos_sessions));
        hash_f64(&mut h, Some(c.costed.oos_stitched.total_return_pct));
        hash_f64(&mut h, c.costed.oos_stitched.sharpe_naive);
        hash_f64(&mut h, Some(c.free_fill_oos.total_return_pct));
        for level in &c.sweep {
            h.write_i64(level.half_ticks);
            hash_f64(&mut h, Some(level.oos_stitched.total_return_pct));
            hash_f64(&mut h, level.oos_stitched.sharpe_naive);
        }
        for control in &c.controls {
            hash_f64(&mut h, control.return_pct());
        }
        for day in &c.costed.account.days {
            h.write_i64(day.trading_day_key);
            h.write_i64(day.close_pnl_nano_usd);
            h.write_i64(day.trough_from_open_nano_usd);
        }
    }
    h.finish()
}

fn hash_f64(h: &mut crate::Fnv64, v: Option<f64>) {
    // `None` is a distinct outcome (no trades, no variance, an absent
    // control), not a zero, and must not hash like one.
    #[expect(
        clippy::cast_possible_wrap,
        reason = "hashing float bit patterns, not their values — §2.2 claims bit-identity"
    )]
    h.write_i64(v.map_or(i64::MIN, |x| x.to_bits() as i64));
}

fn print_footer(loaded: &LoadedConfig, report: &FunnelReport) {
    let killed = report
        .combos
        .iter()
        .filter(|c| c.assessment.verdict == Verdict::Kill)
        .count();
    println!(
        "\n  {killed} of {} combo(s) killed. Most ideas must die, and cheaply — that is the\n\
         \x20 funnel working, not failing.",
        report.combos.len()
    );
    println!(
        "\n  NO COMBO ABOVE CAN BE `GRADUATE`. Graduate means \"survived the full battery\", and\n\
         \x20 the ceiling stays ITERATE (D-0075). The battery is no longer wholly\n\
         \x20 absent, though, so the gap is worth naming exactly. Deflated Sharpe and\n\
         \x20 PBO/CSCV are computed and judged (D-0109); their reasons print only for\n\
         \x20 a combo that survived as far as S3, so a combo killed earlier shows\n\
         \x20 none. The permutation null and the truncation harness are built and\n\
         \x20 hash-pinned, and both caught the planted leak in their own suites — but\n\
         \x20 neither runs on this path, and `max_permutation_p` cannot be set from a\n\
         \x20 config at all, so no p-value appears anywhere above, ever. The plateau\n\
         \x20 test and the cross-instrument rhyme check do not exist. Every Sharpe\n\
         \x20 printed above is the NAIVE one, an upper bound; the deflated companion\n\
         \x20 appears only in an S3 reason, with the trial count applied."
    );
    println!(
        "\n  A leaky reference strategy exists in this repository on purpose\n\
         \x20 (`crucible-strategies::controls::LeakyZScore`, a full-sample z-score — the exact\n\
         \x20 lookahead §2.1 names). The gates above still do NOT catch it, and that is\n\
         \x20 recorded rather than hidden — but it is no longer the whole story. The\n\
         \x20 block-permutation null caught it on 2026-07-31 at p = 0.2079 against a\n\
         \x20 pre-registered 0.05, and the truncation harness caught it independently,\n\
         \x20 4 divergences against SmaCross's 0. `crucible-funnel/tests/planted_leak.rs`\n\
         \x20 asserts `Kill` for it and asserts that every gate before S3 still passes,\n\
         \x20 so the day either changes is a day somebody notices."
    );
    println!("\n  not consumed by `funnel`:");
    let unconsumed = loaded.unconsumed_sections(Consumer::Funnel);
    if unconsumed.is_empty() {
        println!("    nothing — every section this config declares was read");
    }
    for section in unconsumed {
        println!("    {section}");
    }
}

fn format_s0_buckets(
    buckets: &crucible_funnel::s0::Availability<crucible_funnel::s0::BucketSet>,
) -> String {
    use crucible_funnel::s0::Availability;

    let mut text = String::new();
    match buckets {
        Availability::Available { value } => {
            let _ = writeln!(
                text,
                "      bucket  score bounds                 n  mean forward return (fraction)  mean move (ticks)"
            );
            for (index, bucket) in value.as_slice().iter().enumerate() {
                let _ = writeln!(
                    text,
                    "      {:>6}  [{:+.6}, {:+.6}]  {:>8}  {:+.9}                    {:+.6}",
                    index + 1,
                    bucket.score_lo,
                    bucket.score_hi,
                    bucket.n,
                    bucket.mean_return,
                    bucket.mean_move_ticks
                );
            }
        }
        Availability::Unavailable { reason } => {
            let _ = writeln!(text, "      buckets UNAVAILABLE — {reason}");
        }
    }
    text
}

/// Formats the S0 evidence: what the score predicted, at what horizon, and
/// whether it cleared the bar declared before the run.
fn format_s0(report: &crucible_funnel::s0::S0Report) -> String {
    use crucible_funnel::s0::Availability;

    let mut text = String::new();
    if let Err(reason) = report.validate() {
        let _ = writeln!(text, "S0 — UNAVAILABLE: invalid typed evidence ({reason})");
        return text;
    }
    let Some(first) = report.combos.first() else {
        let _ = writeln!(text, "S0 — UNAVAILABLE: no grid combos were measured");
        return text;
    };
    let min_abs_ic = first.combo.spec.min_abs_ic;
    let _ = writeln!(text);
    let _ = writeln!(
        text,
        "S0 — signal triage. Score = `{}`. No orders, no fills, no equity curve:
            this stage asks only whether the score predicts the return that follows it.
            Pre-registered: |IC| >= {min_abs_ic:.4} at some declared horizon.",
        first.combo.spec.score_slot
    );
    let _ = writeln!(text, "  registration/run hash {}", report.registration_hash);
    for metrics in &report.combos {
        let c = &metrics.combo;
        let _ = writeln!(text);
        let _ = writeln!(
            text,
            "  combo {:>3}  {:<34}  {} scores, warmup {}",
            c.combo_index, c.label, c.scores, c.warmup_bars
        );
        let _ = writeln!(
            text,
            "    {:>9}  {:>8}  {:>9}  {:>10}  {:>24}",
            "horizon", "pairs", "dropped", "IC", "UNCONDITIONAL mean return 95% CI"
        );
        for hz in &c.horizons {
            let minutes = hz.horizon_ns / 60_000_000_000;
            let ic = hz
                .ic
                .value()
                .map_or_else(|| "     n/a".to_owned(), |v| format!("{v:+8.4}"));
            let ci = match &hz.unconditional_mean_interval {
                Availability::Unavailable { reason } => format!("ABSENT — {reason}"),
                Availability::Available { value: iv } => {
                    format!(
                        "{:+.5}% [{:+.5}%, {:+.5}%]{}",
                        iv.point * 100.0,
                        iv.lo * 100.0,
                        iv.hi * 100.0,
                        if iv.excludes_zero() { " *" } else { "" }
                    )
                }
            };
            let _ = writeln!(
                text,
                "    {:>7}m  {:>8}  {:>9}  {:>10}  {}",
                minutes,
                hz.n_pairs,
                hz.dropped_no_partner + hz.dropped_invalid_price,
                ic,
                ci
            );
            text.push_str(&format_s0_buckets(&hz.buckets));
        }
        match c.criterion() {
            crucible_funnel::s0::S0CriterionOutcome::Passed { horizon_ns: h } => {
                let b = c.best_abs_ic().expect("a passed criterion has an IC");
                let _ = writeln!(
                    text,
                    "    best |IC| {b:.4}; cleared at {}m (|IC| >= {min_abs_ic:.4} AND its                  UNCONDITIONAL mean interval excludes zero) — PASS",
                    h / 60_000_000_000
                );
            }
            crucible_funnel::s0::S0CriterionOutcome::Failed => {
                let b = c
                    .best_abs_ic()
                    .expect("a measured criterion failure has an IC");
                let _ = writeln!(
                    text,
                    "    best |IC| {b:.4}, but no horizon clears BOTH |IC| and its UNCONDITIONAL mean interval — KILL at s0."
                );
                let _ = writeln!(
                    text,
                    "      Size without significance is what a large enough sample of noise                      gives for free."
                );
            }
            crucible_funnel::s0::S0CriterionOutcome::Unavailable { reason } => {
                let _ = writeln!(
                    text,
                    "    required S0 evidence UNAVAILABLE ({reason}) — criterion UNEVALUATED; the funnel cannot clear an absent bar"
                );
            }
        }
    }
    let _ = writeln!(text);
    let _ = writeln!(
        text,
        "  {} of {} combo(s) cleared S0. A forward return is measurement-space only: it is
           joined to a score at the score's avail_ts and never reaches anything signal-side.",
        report.survivors(),
        report.combos.len()
    );
    match report.evidence_scope() {
        Some(crucible_funnel::s0::S0EvidenceScope::EqualCountScoreBuckets) => {
            let _ = writeln!(
                text,
                "  Capability limitation (not an H-008 run result): original H-008 Gate 0b remains UNEVALUATED — equal-count score buckets are not the registered population of closes beyond their Bollinger band."
            );
        }
        None => {}
    }
    text
}

/// The pooled funnel command (C6b-iii).
///
/// Separate from [`run_cmd`]'s single-contract body rather than folded into it,
/// because a pooled run answers a different question with a different
/// denominator: its sessions are a union rather than a count (D-0114), its
/// trials are N rather than one (D-0124), and its cost table cannot print a
/// drawdown (D-0129). Threading all of that through one function with `if
/// pooled` at each of those points is how the two answers start borrowing each
/// other's numbers.
fn run_pooled(
    loaded: &crate::config::LoadedConfig,
    args: &FunnelArgs,
    git_sha: &str,
    fold_spec: crucible_funnel::FoldSpec,
    criteria: &crucible_funnel::Criteria,
) -> i32 {
    let pooling = loaded
        .file
        .pooling
        .as_ref()
        .expect("INVARIANT: this path is taken only when `[pooling]` is declared");

    // **A pooled config declaring `s0` is REFUSED, not run with S0 skipped.**
    //
    // This is D-0075's rule and it was violated here until 2026-08-07. The
    // pooled path never ran S0, and `assess` reading absent S0 evidence
    // reports `KILL` *decided at s0* — a verdict that looks exactly like a
    // real predictor rejection and is nothing of the kind. Found by pointing
    // H-008 at the pooled path (A4): every combo came back killed at s0 with
    // no S0 measurement having been taken.
    //
    // `run_funnel` has always guarded this — `validate_s0_report` refuses when
    // criteria declare S0 and its report is absent — and the pooled path
    // bypassed that guard rather than repeating it. The refusal is the fix
    // because pooled S0 is genuinely unimplemented: S0 measures a forward
    // return join over one series, and which series a pooled run means is a
    // question nobody has answered yet.
    if criteria.runs(crucible_funnel::Stage::S0) {
        eprintln!(
            "error: this config declares `stages = [\"s0\", ...]` and a `[pooling]` block, and 
                    pooled S0 is not implemented. It is refused rather than run with S0
                    skipped: `assess` reading absent S0 evidence reports KILL *decided at
                    s0*, which is indistinguishable from a real predictor rejection
                    (D-0075). Drop `s0` from `stages` to pool the trading gates, or run
                    the config against a single contract where S0 does run."
        );
        return EXIT_USAGE;
    }

    let table = match crate::pooled::load_volume_roll_table(loaded, &pooling.root) {
        Ok(table) => table,
        Err((code, message)) => {
            eprintln!("error: {message}");
            return code;
        }
    };
    let plans = match crate::pooled::plan_pool(
        loaded,
        &table,
        &loaded.file.universe.instruments,
        fold_spec,
    ) {
        Ok(plans) => plans,
        Err((code, message)) => {
            eprintln!("error: {message}");
            return code;
        }
    };

    let registry_path = args.out.join("registry.jsonl");
    let mut registry = if args.hash_only {
        Registry::ephemeral(&registry_path)
    } else {
        match Registry::open(&registry_path) {
            Ok(registry) => registry,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_USAGE;
            }
        }
    };

    let identity = crucible_funnel::walkforward::RunIdentity {
        config_hash: loaded.config_hash,
        root_seed: loaded.file.run.seed,
        account_id: None,
    };
    let shared = crucible_funnel::funnel::SharedInputs {
        grid: &loaded.grid,
        spec: &loaded.spec,
        criteria,
        identity: &identity,
        costs: Costs {
            half_spread_ticks: loaded.file.execution.half_spread_ticks,
            fee_per_contract_nano_usd: loaded.fee_per_contract_nano_usd,
        },
        qty: Qty(loaded.file.run.qty_contracts),
    };
    let now = timestamp();
    let report = match crate::pooled::run_pooled_funnel(
        loaded,
        shared,
        &plans,
        &mut registry,
        crate::pooled::PooledRunMeta {
            hypothesis_family: &loaded.file.meta.hypothesis_family,
            git_sha,
            data_manifest_ids: &[],
            now: &now,
        },
    ) {
        Ok(report) => report,
        Err((code, message)) => {
            eprintln!("error: {message}");
            return code;
        }
    };

    if args.hash_only {
        println!("{:016x}", pooled_verdict_hash(&report));
        return 0;
    }
    print_pooled_report(loaded, &report, criteria);

    // Exit 5 when every combo was killed: not a failure and not success, the
    // same contract `funnel` already keeps (D-0075).
    if report
        .combos
        .iter()
        .all(|c| c.assessment.verdict == Verdict::Kill)
    {
        5
    } else {
        0
    }
}

/// The pooled determinism hash (C7 pins this).
///
/// Reads the POOLED quantities — the verdict, the stage that decided it, the
/// session union, the pooled returns and Sharpes — rather than any contract's
/// own. A hash over per-contract numbers would be green while the pooling was
/// wrong, which is the blindness D-0128 measured in the single-contract gate.
fn pooled_verdict_hash(report: &crate::pooled::PooledFunnelReport) -> u64 {
    let mut h = crate::Fnv64::new();
    let i64_of = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    h.write_i64(i64_of(report.evaluation.oos_sessions));
    h.write_i64(i64_of(report.evaluation.oos_trades));
    h.write_i64(i64_of(report.evaluation.contracts_evaluated));
    h.write_i64(i64_of(report.trials_after));
    for c in &report.combos {
        h.write_i64(i64_of(c.combo_index));
        h.write_i64(match c.assessment.verdict {
            Verdict::Kill => 0,
            Verdict::Iterate => 1,
            Verdict::Graduate => 2,
        });
        for byte in c.assessment.decided_at.to_string().bytes() {
            h.write_i64(i64::from(byte));
        }
        h.write_i64(i64_of(c.evidence.oos_trades));
        h.write_i64(i64_of(c.evidence.oos_sessions));
        hash_f64(&mut h, Some(c.evidence.costed_return_pct));
        hash_f64(&mut h, c.evidence.costed_sharpe);
        hash_f64(&mut h, Some(c.evidence.free_fill_return_pct));
        hash_f64(&mut h, c.evidence.sharpe_at_kill_level);
        hash_f64(&mut h, c.evidence.random_entry_return_pct);
        hash_f64(&mut h, c.evidence.buy_and_hold_return_pct);
        for level in &c.sweep {
            h.write_i64(level.half_ticks.unwrap_or(i64::MIN));
            hash_f64(&mut h, Some(level.total_return_pct));
            hash_f64(&mut h, level.sharpe_naive);
            h.write_i64(level.fees_nano_usd);
        }
    }
    h.finish()
}

fn print_pooled_report(
    loaded: &crate::config::LoadedConfig,
    report: &crate::pooled::PooledFunnelReport,
    criteria: &crucible_funnel::Criteria,
) {
    let e = &report.evaluation;
    println!("\ncrucible funnel — POOLED run");
    println!(
        "  root           {}",
        loaded
            .file
            .pooling
            .as_ref()
            .map_or("?", |p| p.root.as_str())
    );
    println!(
        "  sessions       {} distinct out-of-sample trading day(s) across {} contract(s)",
        e.oos_sessions, e.contracts_evaluated
    );
    println!(
        "  trades         {} pooled out-of-sample round-trip(s)",
        e.oos_trades
    );
    println!(
        "  trials         {} before, {} after — every pooled contract is a trial (D-0124),\n\
         \x20                so the deflated Sharpe falls as the pool grows",
        report.trials_before, report.trials_after
    );
    println!(
        "  annualization  {:.1} bars/year, the MEAN over contributing contracts — an\n\
         \x20                assumption, not a measurement: each contract measures its own from\n\
         \x20                its own sample (D-0038/D-0039) and the pooled series concatenates them",
        report.bars_per_year
    );
    for (instrument, reason) in &e.skipped {
        println!("  SKIPPED        {instrument}: {reason}");
    }
    if e.skipped.is_empty() {
        println!("  skipped        0 contract(s) — every declared contract contributed");
    }
    for gap in &e.gaps {
        println!(
            "  GAP            {} trading day(s) between two contracts",
            gap.days()
        );
    }
    println!(
        "\n  max drawdown is NOT pooled: it is a path statistic and the pooled series is not a\n\
         \x20 path any account walked (D-0119, D-0129).\n"
    );
    println!("  combo  verdict  decided at   OOS ret   Sharpe   trades  parameters");
    for c in &report.combos {
        println!(
            "  {:>5}  {:<7}  {:<11}  {:+7.2}%  {:>7}  {:>6}  {}",
            c.combo_index,
            c.assessment.verdict.to_string().to_uppercase(),
            c.assessment.decided_at.to_string(),
            c.evidence.costed_return_pct,
            c.evidence
                .costed_sharpe
                .map_or_else(|| "—".to_owned(), |s| format!("{s:.2}")),
            c.evidence.oos_trades,
            c.label,
        );
    }
    println!(
        "\n  ceiling is ITERATE: S3's battery is incomplete, so `Graduate` is unreachable\n\
         \x20 by construction (D-0075). Criteria: {} trade(s), {} session(s) required.",
        criteria.min_oos_trades, criteria.min_oos_sessions
    );
}

#[cfg(test)]
mod tests {
    use super::{format_s0, format_s0_buckets};
    use crucible_funnel::s0::{
        Availability, Bucket, BucketSet, HorizonEvidence, Interval, S0ComboReport, S0EvidenceScope,
        S0Report, S0RunMetrics, S0Spec, UnavailableReason,
    };

    const MIN: i64 = 60_000_000_000;

    fn five_bucket_set() -> BucketSet {
        let ticks = [2.0, 1.0, 0.0, -1.0, -2.0];
        let returns = [3.0, 1.0, 0.0, -1.0, -3.0];
        BucketSet::from_nonempty(
            ticks
                .iter()
                .zip(returns)
                .enumerate()
                .map(|(index, (&mean_move_ticks, mean_return))| {
                    #[expect(clippy::cast_precision_loss, reason = "five hand-derived buckets")]
                    let score_lo = index as f64 * 2.0 - 5.0;
                    Bucket {
                        score_lo,
                        score_hi: score_lo + 1.0,
                        n: 1,
                        mean_return: mean_return / 1024.0,
                        mean_move_ticks,
                    }
                })
                .collect(),
        )
        .expect("valid five-bucket fixture")
    }

    fn ordered_report() -> S0Report {
        let declared = [10 * MIN, MIN, 5 * MIN];
        let horizons = declared
            .iter()
            .map(|&horizon_ns| HorizonEvidence {
                horizon_ns,
                n_pairs: 5,
                dropped_no_partner: 0,
                dropped_invalid_price: 0,
                ic: Availability::Available { value: -0.8 },
                buckets: Availability::Available {
                    value: five_bucket_set(),
                },
                unconditional_mean_interval: Availability::Available {
                    value: Interval {
                        point: 0.0,
                        lo: -0.01,
                        hi: 0.01,
                        draws: 200,
                    },
                },
            })
            .collect();
        S0Report {
            registration_hash: "ab".repeat(32),
            combos: vec![S0RunMetrics {
                evidence_scope: S0EvidenceScope::EqualCountScoreBuckets,
                combo: S0ComboReport {
                    score_identity: "zscore close period=20".to_owned(),
                    tick_size_nanopoints: 250_000_000,
                    spec: S0Spec {
                        score_slot: "z".to_owned(),
                        horizons_ns: declared.to_vec(),
                        buckets: 5,
                        bootstrap_draws: 200,
                        min_abs_ic: 0.05,
                    },
                    combo_index: 0,
                    label: "z(period=20 source=close)".to_owned(),
                    warmup_bars: 20,
                    scores: 5,
                    horizons,
                },
            }],
        }
    }

    #[test]
    fn stdout_bucket_absence_is_named_not_empty_or_zero() {
        let text = format_s0_buckets(&Availability::Unavailable {
            reason: UnavailableReason::TooFewObservations {
                observed: 3,
                required: 5,
            },
        });
        assert!(text.contains("buckets UNAVAILABLE"));
        assert!(text.contains("observed: 3"));
        assert!(text.contains("required: 5"));
        assert!(!text.contains("mean move (ticks)"));
    }

    #[test]
    fn stdout_buckets_keep_low_score_first_and_tick_polarity() {
        let buckets = BucketSet::from_nonempty(vec![
            Bucket {
                score_lo: -2.0,
                score_hi: -1.0,
                n: 2,
                mean_return: 3.0 / 1024.0,
                mean_move_ticks: 2.0,
            },
            Bucket {
                score_lo: 1.0,
                score_hi: 2.0,
                n: 2,
                mean_return: -3.0 / 1024.0,
                mean_move_ticks: -2.0,
            },
        ])
        .expect("nonempty buckets");
        let text = format_s0_buckets(&Availability::Available { value: buckets });
        let low = text.find("[-2.000000").expect("low bucket");
        let high = text.find("[+1.000000").expect("high bucket");
        assert!(low < high);
        assert!(text.contains("+0.002929688                    +2.000000"));
        assert!(text.contains("-0.002929688                    -2.000000"));
    }

    #[test]
    fn stdout_renders_every_bucket_for_every_horizon_in_declaration_order() {
        let text = format_s0(&ordered_report());
        let ten = text.find("     10m").expect("10m horizon");
        let one = text.find("      1m").expect("1m horizon");
        let five = text.find("      5m").expect("5m horizon");
        assert!(ten < one && one < five, "{text}");
        assert_eq!(text.matches("bucket  score bounds").count(), 3, "{text}");
        for bounds in [
            "[-5.000000, -4.000000]",
            "[-3.000000, -2.000000]",
            "[-1.000000, +0.000000]",
            "[+1.000000, +2.000000]",
            "[+3.000000, +4.000000]",
        ] {
            assert_eq!(text.matches(bounds).count(), 3, "missing {bounds}: {text}");
        }
        assert!(text.contains("UNCONDITIONAL mean interval"), "{text}");
        assert!(text.contains("not an H-008 run result"), "{text}");
    }

    #[test]
    fn stdout_does_not_present_the_no_separation_converse_as_an_edge() {
        let mut report = ordered_report();
        for horizon in &mut report.combos[0].combo.horizons {
            horizon.ic = Availability::Unavailable {
                reason: UnavailableReason::ConstantForwardReturn,
            };
            horizon.unconditional_mean_interval = Availability::Available {
                value: Interval {
                    point: 1.0 / 256.0,
                    lo: 0.003,
                    hi: 0.005,
                    draws: 200,
                },
            };
            horizon.buckets = Availability::Available {
                value: BucketSet::from_nonempty(
                    (0..5)
                        .map(|index| {
                            let score_lo = f64::from(index) * 2.0 - 5.0;
                            Bucket {
                                score_lo,
                                score_hi: score_lo + 1.0,
                                n: 1,
                                mean_return: 1.0 / 256.0,
                                mean_move_ticks: 3.0,
                            }
                        })
                        .collect(),
                )
                .expect("converse buckets"),
            };
        }
        let text = format_s0(&report);
        assert_eq!(text.matches("+0.003906250").count(), 15, "{text}");
        assert!(text.contains("criterion UNEVALUATED"), "{text}");
        assert!(!text.contains("— PASS"), "{text}");
    }
}
