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

use std::path::PathBuf;

use crucible_core::prelude::*;
use crucible_engine::INTRABAR_CONVENTION;
use crucible_funnel::registry::Registry;
use crucible_funnel::scorecard::{self, Provenance};
use crucible_funnel::stages::Verdict;
use crucible_funnel::walkforward::{FoldPlan, RunIdentity};
use crucible_funnel::{ComboOutcome, Costs, FunnelInputs, FunnelReport, run_funnel};

use crate::combo::{annualization, collect_events, print_header, usd};
use crate::config::{self, Consumer, LoadedConfig};
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
}

/// Runs the command, returning the process exit code.
#[expect(
    clippy::too_many_lines,
    reason = "this is the wiring the command exists to be: load, refuse, collect, plan, run, \
              render. Splitting it would spread one linear story across six functions that are \
              each called once"
)]
pub fn run_cmd(args: &FunnelArgs) -> i32 {
    let loaded = match config::load(&args.config) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    let Some(cfg) = loaded.file.funnel.as_ref() else {
        eprintln!(
            "error: {} declares no [funnel] section, so there are no pre-registered criteria to\n\
             \x20      judge against. Add one — stages, cost_sensitivity_ticks, min_oos_trades,\n\
             \x20      min_oos_sessions, min_oos_return_pct_free_fills,\n\
             \x20      min_oos_sharpe_after_costs, kill_if_dead_at_ticks,\n\
             \x20      require_controls_beaten, max_pbo, require_plateau.\n\
             \x20      Criteria written after seeing results are not criteria.",
            loaded.path.display()
        );
        return EXIT_USAGE;
    };
    let criteria = match cfg.to_criteria() {
        Ok(criteria) => criteria,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };
    if loaded.file.execution.fill_model == "free_fills" {
        eprintln!(
            "error: execution.fill_model = \"free_fills\", which `funnel` refuses.\n\
             \x20      The funnel runs the free-fill screen itself at S1 and then asks S2 whether\n\
             \x20      the edge survives honest costs; with free_fills declared those are the\n\
             \x20      same run and the mandatory cost sweep is one number repeated four times.\n\
             \x20      FreeFills is a screening tool, never a result (D-0006)."
        );
        return EXIT_USAGE;
    }
    let Some(wf) = loaded.file.walk_forward.as_ref() else {
        eprintln!(
            "error: {} declares no [walk_forward] section. The funnel's S2 gate is a\n\
             \x20      walk-forward under costs, so there is nothing to gate without folds.",
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
    if let Err(e) =
        crucible_funnel::grid::check_size(loaded.grid.len(), &loaded.file.meta.hypothesis_family)
    {
        eprintln!("error: {e}");
        return EXIT_USAGE;
    }
    let git_sha = match resolve_git_sha() {
        Ok(sha) => sha,
        Err(message) => {
            eprintln!("error: {message}");
            return EXIT_USAGE;
        }
    };

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

    let mut registry = match Registry::open(&args.out.join("registry.jsonl")) {
        Ok(registry) => registry,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    let bars_per_year = annualization(&loaded, &series.events);
    let params = crucible_engine::BacktestParams {
        initial_cash_nano_usd: loaded.initial_cash_nano_usd,
        bars_per_year,
    };
    let identity = RunIdentity {
        config_hash: loaded.config_hash,
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
    let inputs = FunnelInputs {
        events: &series.events,
        day_keys: &days.keys,
        grid: &loaded.grid,
        plan: &plan,
        spec: &loaded.spec,
        params: &params,
        identity: &identity,
        criteria: &criteria,
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

    let report = match run_funnel(&inputs, &mut registry) {
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

    let provenance = Provenance {
        idea_name: loaded.file.meta.name.clone(),
        hypothesis_family: loaded.file.meta.hypothesis_family.clone(),
        economic_rationale: loaded.file.meta.economic_rationale.clone(),
        config_hash: loaded.config_hash.to_string(),
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
        .join(format!("scorecard-{}.html", loaded.config_hash.short()));
    if let Err(e) = std::fs::write(&card, html) {
        eprintln!("error: cannot write {}: {e}", card.display());
        return EXIT_USAGE;
    }

    print_header(&loaded, "funnel");
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
        "  stages         {} — S0's signal triage and S3's battery are not in this build and a\n\
         \x20                config declaring either is refused, not silently skipped",
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
            c.costed.oos_pooled.total_return_pct,
            c.costed.oos_pooled.round_trips,
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
                level.oos_pooled.total_return_pct
            );
        }
        println!("  |  free_fills {:+.2}%", c.free_fill_oos.total_return_pct);
        for control in &c.controls {
            match &control.oos_pooled {
                Some(s) => println!(
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
        |control| format!("{:+.2}%", c.costed.oos_pooled.total_return_pct - control),
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
        hash_f64(&mut h, Some(c.costed.oos_pooled.total_return_pct));
        hash_f64(&mut h, c.costed.oos_pooled.sharpe_naive);
        hash_f64(&mut h, Some(c.free_fill_oos.total_return_pct));
        for level in &c.sweep {
            h.write_i64(level.half_ticks);
            hash_f64(&mut h, Some(level.oos_pooled.total_return_pct));
            hash_f64(&mut h, level.oos_pooled.sharpe_naive);
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
         \x20 the battery is S3: deflated Sharpe, PBO/CSCV, the permutation nulls and the\n\
         \x20 truncation-invariance harness. None of them is in this build, so the ceiling is\n\
         \x20 ITERATE and every Sharpe printed here is the NAIVE one — an upper bound, with the\n\
         \x20 trial count beside it but not yet applied to it."
    );
    println!(
        "\n  A leaky reference strategy exists in this repository on purpose\n\
         \x20 (`crucible-strategies::controls::LeakyZScore`, a full-sample z-score — the exact\n\
         \x20 lookahead §2.1 names), and the gates above do NOT catch it. That is recorded, not\n\
         \x20 hidden: it is the honest baseline the permutation and truncation harnesses have to\n\
         \x20 beat, and `crucible-funnel/tests/planted_leak.rs` asserts today's answer so the\n\
         \x20 day it changes is a day somebody notices."
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
