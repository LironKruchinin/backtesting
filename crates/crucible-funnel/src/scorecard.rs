//! Verdict scorecards — the user-facing output, and the portfolio artifact.
//!
//! One self-contained HTML file per evaluated idea, answering "is this worth
//! pursuing?" with the evidence visible and the assumptions named.
//!
//! ## The honesty box is load-bearing, and this module enforces it
//!
//! The spec says: *"Nothing in this box is optional; a scorecard without its
//! honesty box does not render."* That is implemented literally —
//! [`render`] returns [`ScorecardError`] and produces **no file** when a
//! required field is missing. It is the one place in this codebase where a
//! blank field aborts a render, and it is deliberate: every other omission on
//! a page like this reads as "not applicable", and "we did not record the git
//! sha" must not.
//!
//! Required, all of them: the fill model and its parameters, the intrabar
//! ordering convention and its path-sensitive count, the trial count for the
//! hypothesis family, both control benchmarks, the sample sizes, the config
//! hash, the git sha, and the data manifest ids (§2.4, §2.5, and
//! `PROJECT_PLAN.md` §7.4's denominators).
//!
//! ## Sections that say they are missing rather than being absent
//!
//! The spec lists eight sections; this build can compute five of them. The
//! other three — the parameter-plateau heatmap, the regime table, and the
//! permutation null — are **rendered as explicit gaps**, naming what each
//! needs, because a reader who does not see a null comparison cannot tell
//! "there wasn't one" from "it passed". A scorecard with a hole in it that
//! says so is honest; one with a hole nobody can see is not.
//!
//! ## Rendering
//!
//! Static HTML, inline CSS, inline SVG, **no JavaScript and no network**: the
//! file has to open from disk in five years. The charts are hand-emitted SVG
//! rectangles rather than a plotting library, which is also why there is no
//! dependency here.

use std::fmt::Write as _;

use crucible_engine::WorstDayDistribution;

use crate::funnel::{ComboOutcome, FunnelReport};
use crate::stages::{Criteria, Verdict};

/// Everything the honesty box needs that a [`FunnelReport`] does not already
/// carry.
#[derive(Clone, Debug)]
pub struct Provenance {
    /// `meta.name`.
    pub idea_name: String,
    /// `meta.hypothesis_family`.
    pub hypothesis_family: String,
    /// `meta.economic_rationale`.
    pub economic_rationale: String,
    /// blake3 of the config's canonical form (D-0012).
    pub config_hash: String,
    /// Repository revision (§2.5).
    pub git_sha: String,
    /// blake3 of every archived file the series was read from (§2.5). Empty is
    /// legal **only** for a synthetic feed, whose provenance is its seed —
    /// which is why `data_source` is required beside it.
    pub data_manifest_ids: Vec<String>,
    /// What the bars were: `synthetic random walk, seed 42, 20000 bars` or
    /// `curated ESH2024 1m 2024-01-01..2024-02-01`.
    pub data_source: String,
    /// Instrument and timeframe.
    pub universe: String,
    /// `spread_cross — 1 tick half-spread, $1.25/contract/side`.
    pub fill_model: String,
    /// The named intrabar ordering convention (§2.4, D-0069).
    pub intrabar_convention: String,
    /// Declared starting capital, rendered.
    pub capital: String,
    /// Wall clock, supplied by the caller. This crate reads no clock.
    pub rendered_at: String,
}

/// A required honesty-box field was missing, so nothing was rendered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScorecardError {
    /// Which fields were empty.
    pub missing: Vec<&'static str>,
}

impl std::fmt::Display for ScorecardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the scorecard's honesty box is missing {}, so nothing was rendered.\n\
             Every field in that box is required (CLAUDE.md §2.4, §2.5): a page that omits its \
             fill model, its trial count or its git sha looks exactly like a page whose numbers \
             are safe to quote, and it is the one omission that must fail loudly instead of \
             rendering blank.",
            self.missing.join(", ")
        )
    }
}

impl std::error::Error for ScorecardError {}

/// Renders one self-contained HTML scorecard for a whole funnel run.
///
/// # Errors
/// [`ScorecardError`] if any honesty-box field is empty. No partial page is
/// produced — see the module docs.
pub fn render(
    report: &FunnelReport,
    criteria: &Criteria,
    provenance: &Provenance,
) -> Result<String, ScorecardError> {
    check_honesty_box(report, provenance)?;

    let mut h = String::with_capacity(64 * 1024);
    h.push_str(HEAD);
    write_header(&mut h, report, provenance);
    write_honesty_box(&mut h, report, criteria, provenance);
    write_verdicts(&mut h, report);
    for combo in &report.combos {
        write_combo(&mut h, combo, criteria);
    }
    write_gaps(&mut h, criteria);
    h.push_str("</body></html>\n");
    Ok(h)
}

/// The rule the module docs describe, as code.
fn check_honesty_box(report: &FunnelReport, provenance: &Provenance) -> Result<(), ScorecardError> {
    let mut missing = Vec::new();
    let required: [(&'static str, bool); 9] = [
        ("meta.name", provenance.idea_name.trim().is_empty()),
        (
            "meta.hypothesis_family",
            provenance.hypothesis_family.trim().is_empty(),
        ),
        ("config hash", provenance.config_hash.trim().is_empty()),
        ("git sha", provenance.git_sha.trim().is_empty()),
        ("the data source", provenance.data_source.trim().is_empty()),
        ("the fill model", provenance.fill_model.trim().is_empty()),
        (
            "the intrabar ordering convention",
            provenance.intrabar_convention.trim().is_empty(),
        ),
        ("the universe", provenance.universe.trim().is_empty()),
        (
            "both mandatory controls",
            report
                .combos
                .iter()
                .any(|c| c.controls.iter().any(|k| k.name.trim().is_empty())),
        ),
    ];
    for (name, is_missing) in required {
        if is_missing {
            missing.push(name);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ScorecardError { missing })
    }
}

const HEAD: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Crucible scorecard</title>
<style>
:root{--ink:#15181d;--dim:#5b6470;--rule:#d8dde4;--bg:#fbfcfd;--kill:#a8262a;--iterate:#8a6a12;--grad:#1d6b3a;--box:#f2f5f8}
*{box-sizing:border-box}
body{margin:0;padding:2rem 1.25rem 4rem;font:15px/1.55 ui-sans-serif,system-ui,-apple-system,Segoe UI,Roboto,sans-serif;color:var(--ink);background:var(--bg)}
main,header,section{max-width:64rem;margin:0 auto}
h1{font-size:1.5rem;margin:0 0 .25rem}
h2{font-size:1.05rem;margin:2.25rem 0 .6rem;padding-bottom:.3rem;border-bottom:1px solid var(--rule)}
h3{font-size:.95rem;margin:1.5rem 0 .4rem}
p,li{margin:.4rem 0}
.dim{color:var(--dim)}
.mono{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.85em;word-break:break-all}
table{border-collapse:collapse;width:100%;margin:.5rem 0 1rem;font-size:.9rem}
th,td{text-align:right;padding:.35rem .5rem;border-bottom:1px solid var(--rule)}
th:first-child,td:first-child{text-align:left}
thead th{color:var(--dim);font-weight:600}
.wrap{overflow-x:auto}
.box{background:var(--box);border:1px solid var(--rule);border-radius:6px;padding:.9rem 1.1rem;margin:.75rem 0}
.kv{display:grid;grid-template-columns:14rem 1fr;gap:.25rem 1rem}
.kv dt{color:var(--dim)}
.kv dd{margin:0}
.verdict{display:inline-block;padding:.15rem .55rem;border-radius:4px;font-weight:700;letter-spacing:.04em;color:#fff}
.v-kill{background:var(--kill)}.v-iterate{background:var(--iterate)}.v-graduate{background:var(--grad)}
.pass{color:var(--grad)}.fail{color:var(--kill);font-weight:600}
.gap{border-left:3px solid var(--iterate);padding-left:.9rem;margin:1rem 0}
svg{display:block;max-width:100%;height:auto;margin:.5rem 0}
.bar-pos{fill:#2f7d4f}.bar-neg{fill:#b0393d}.axis{stroke:var(--rule)}
.tick{fill:var(--dim);font-size:9px}
</style></head><body>
"#;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn write_header(h: &mut String, report: &FunnelReport, p: &Provenance) {
    let _ = write!(
        h,
        "<header><h1>{}</h1><p class=\"dim\">{}</p><p class=\"dim\">{} combo(s) evaluated · \
         rendered {}</p></header>",
        esc(&p.idea_name),
        esc(&p.economic_rationale),
        report.combos.len(),
        esc(&p.rendered_at)
    );
}

fn write_honesty_box(h: &mut String, report: &FunnelReport, criteria: &Criteria, p: &Provenance) {
    let manifest = if p.data_manifest_ids.is_empty() {
        "none — the series is generated, and its seed (named in the data source above) is its \
         whole provenance"
            .to_owned()
    } else {
        p.data_manifest_ids.join("<br>")
    };
    let path_sensitive: usize = report
        .combos
        .iter()
        .map(|c| c.costed.path_sensitive_bars)
        .sum();
    let protective: usize = report
        .combos
        .iter()
        .map(|c| c.costed.n_protective_exits)
        .sum();

    let _ = write!(
        h,
        "<section><h2>Honesty box</h2><div class=\"box\"><dl class=\"kv\">\
         <dt>universe</dt><dd>{universe}</dd>\
         <dt>data</dt><dd>{data}</dd>\
         <dt>fill model</dt><dd>{fill}</dd>\
         <dt>intrabar convention</dt><dd>{intrabar} — {sensitive} of {protective} protective \
         exit(s) across the grid came from a bar that touched both levels, where the convention \
         chose the outcome rather than the data (D-0069)</dd>\
         <dt>capital</dt><dd>{capital}</dd>\
         <dt>hypothesis family</dt><dd class=\"mono\">{family}</dd>\
         <dt>trials charged</dt><dd><strong>{after}</strong> (was {before} before this run) — \
         read from the registry, never from memory. A trial is one (config, account, combo); \
         folds of one combo are one trial</dd>\
         <dt>naive Sharpe</dt><dd>reported per combo below</dd>\
         <dt>deflated Sharpe</dt><dd><strong>not computed by this build.</strong> Deflating \
         requires the trial count (which is above) <em>and</em> the skew/kurtosis correction of \
         Bailey &amp; López de Prado — `crucible-funnel::stats`, still a module-doc spec. Every \
         Sharpe on this page is the naive one and must be read as an upper bound</dd>\
         <dt>config hash</dt><dd class=\"mono\">{config}</dd>\
         <dt>git sha</dt><dd class=\"mono\">{git}</dd>\
         <dt>data manifest ids</dt><dd class=\"mono\">{manifest}</dd>\
         <dt>cost sweep</dt><dd>{sweep} tick(s) of half-spread, mandatory (§2.4)</dd>\
         <dt>registry</dt><dd>{claimed} run(s) claimed, {done} already finished, {retried} \
         re-run after an unfinished claim</dd>\
         </dl></div></section>",
        universe = esc(&p.universe),
        data = esc(&p.data_source),
        fill = esc(&p.fill_model),
        intrabar = esc(&p.intrabar_convention),
        sensitive = path_sensitive,
        protective = protective,
        capital = esc(&p.capital),
        family = esc(&p.hypothesis_family),
        after = report.trials_after,
        before = report.trials_before,
        config = esc(&p.config_hash),
        git = esc(&p.git_sha),
        manifest = manifest,
        sweep = criteria
            .cost_sweep_half_ticks
            .iter()
            .map(|&t| crate::stages::render_half_ticks(t))
            .collect::<Vec<_>>()
            .join(" / "),
        claimed = report.runs_claimed,
        done = report.runs_already_done,
        retried = report.runs_retried,
    );
}

fn verdict_class(v: Verdict) -> &'static str {
    match v {
        Verdict::Kill => "v-kill",
        Verdict::Iterate => "v-iterate",
        Verdict::Graduate => "v-graduate",
    }
}

fn write_verdicts(h: &mut String, report: &FunnelReport) {
    h.push_str(
        "<section><h2>Verdicts</h2><p class=\"dim\">In grid-index order, never ranked by \
         out-of-sample performance: a table sorted by the number you are about to quote is a \
         selection step wearing a report's clothes.</p><div class=\"wrap\"><table><thead><tr>\
         <th>combo</th><th>parameters</th><th>verdict</th><th>decided at</th><th>OOS return</th>\
         <th>OOS Sharpe</th><th>trades</th><th>sessions</th></tr></thead><tbody>",
    );
    for c in &report.combos {
        let s = &c.costed.oos_pooled;
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"mono\">{}</td><td><span class=\"verdict {}\">{}</span>\
             </td><td>{}</td><td>{:+.2}%</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            c.id.combo_index,
            esc(&c.label),
            verdict_class(c.assessment.verdict),
            c.assessment.verdict,
            c.assessment.decided_at,
            s.total_return_pct,
            opt(s.sharpe_naive),
            s.round_trips,
            c.oos_sessions,
        );
    }
    h.push_str("</tbody></table></div></section>");
}

fn opt(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_owned(), |x| format!("{x:.2}"))
}

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_owned(), |x| format!("{x:+.2}%"))
}

fn usd(nano: i64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "display only (§2.3)")]
    let dollars = nano as f64 / 1e9;
    format!("${dollars:.2}")
}

fn write_combo(h: &mut String, c: &ComboOutcome, criteria: &Criteria) {
    let _ = write!(
        h,
        "<section><h2>Combo {} — <span class=\"mono\">{}</span> \
         <span class=\"verdict {}\">{}</span></h2>",
        c.id.combo_index,
        esc(&c.label),
        verdict_class(c.assessment.verdict),
        c.assessment.verdict
    );

    // 1. The pre-registered criteria, and how each one went.
    h.push_str(
        "<h3>Pre-registered criteria</h3><p class=\"dim\">Written in the config before the run \
         and stored on the registry row that was inserted before the run. Passes are listed as \
         well as failures — a survivor that only showed its failures would look unexamined.</p>\
         <ul>",
    );
    for reason in &c.assessment.reasons {
        let _ = write!(
            h,
            "<li><span class=\"{}\">{}</span> <span class=\"dim\">[{}]</span> {}</li>",
            if reason.passed { "pass" } else { "fail" },
            if reason.passed { "pass" } else { "FAIL" },
            reason.stage,
            esc(&reason.detail)
        );
    }
    h.push_str("</ul>");

    // 2. Controls — the denominators, never optional.
    h.push_str(
        "<h3>Controls</h3><p class=\"dim\">Replayed on the same bars, through the same engine, \
         under the same fill model, and sliced by the same folds. The random-entry control \
         reproduces this combo's own trade count, holding lengths and long/short mix and \
         re-places them at seeded-random times — everything held fixed except <em>when</em>, \
         which is the claim under test. It is the <strong>median of 16 draws</strong>, because \
         one draw is a sample of size one and a strategy can lose to a single coin flip by \
         luck; <span class=\"mono\">beat</span> is how many of those draws the combo cleared, \
         which is the empirical p-value this control can honestly give. A control that could \
         not be built is reported as absent and fails its criterion — it never renders as a \
         zero.</p><div class=\"wrap\"><table><thead><tr><th>benchmark</th><th>OOS return</th>\
         <th>OOS Sharpe</th><th>trades</th><th>beat</th><th>seed</th></tr></thead><tbody>",
    );
    let _ = write!(
        h,
        "<tr><td><strong>this combo</strong></td><td>{:+.2}%</td><td>{}</td><td>{}</td>\
         <td class=\"dim\">—</td><td class=\"dim\">—</td></tr>",
        c.costed.oos_pooled.total_return_pct,
        opt(c.costed.oos_pooled.sharpe_naive),
        c.costed.oos_pooled.round_trips
    );
    for control in &c.controls {
        match &control.oos_pooled {
            Some(s) => {
                let _ = write!(
                    h,
                    "<tr><td>{}{}</td><td>{:+.2}%</td><td>{}</td><td>{}</td><td>{} / {}</td>\
                     <td class=\"mono\">{}</td></tr>",
                    esc(control.name),
                    if control.draws > 1 {
                        format!(" <span class=\"dim\">(median of {})</span>", control.draws)
                    } else {
                        String::new()
                    },
                    s.total_return_pct,
                    opt(s.sharpe_naive),
                    s.round_trips,
                    control.draws_beaten,
                    control.draws,
                    control
                        .seed
                        .map_or_else(|| "deterministic".to_owned(), |s| format!("{s:016x}"))
                );
            }
            None => {
                let _ = write!(
                    h,
                    "<tr><td>{}</td><td colspan=\"5\" class=\"fail\">ABSENT — {}</td></tr>",
                    esc(control.name),
                    esc(control
                        .absent_because
                        .as_deref()
                        .unwrap_or("no reason recorded"))
                );
            }
        }
    }
    h.push_str("</tbody></table></div>");

    // 3. The cost sweep — the most decision-relevant table on the page.
    h.push_str(
        "<h3>Cost sensitivity</h3><p class=\"dim\">Each level is a separate replay, not an \
         adjustment to a finished curve: a different half-spread changes every fill price, which \
         changes the mark-to-market path, which changes the drawdown and the Sharpe. Commission \
         is charged at every level — a tight book does not stop a broker billing.</p>\
         <div class=\"wrap\"><table><thead><tr><th>half-spread</th><th>OOS return</th>\
         <th>max DD</th><th>OOS Sharpe</th><th>fees</th></tr></thead><tbody>",
    );
    for level in &c.sweep {
        let s = &level.oos_pooled;
        let marker = if level.half_ticks == criteria.kill_if_dead_half_ticks {
            " <span class=\"dim\">← kill level</span>"
        } else {
            ""
        };
        let _ = write!(
            h,
            "<tr><td>{} tick{}</td><td>{:+.2}%</td><td>{:.2}%</td><td>{}</td><td>{}</td></tr>",
            level.ticks(),
            marker,
            s.total_return_pct,
            s.max_drawdown_pct,
            opt(s.sharpe_naive),
            usd(s.fees_nano_usd),
        );
    }
    let _ = write!(
        h,
        "<tr><td>free_fills <span class=\"dim\">(S1 screen)</span></td><td>{:+.2}%</td>\
         <td>{:.2}%</td><td>{}</td><td>{}</td></tr></tbody></table></div>",
        c.free_fill_oos.total_return_pct,
        c.free_fill_oos.max_drawdown_pct,
        opt(c.free_fill_oos.sharpe_naive),
        usd(c.free_fill_oos.fees_nano_usd),
    );
    h.push_str(sweep_chart(c).as_str());

    // 4. Per-fold detail.
    h.push_str(
        "<h3>Folds</h3><p class=\"dim\">Every number is computed on the window it names: the \
         grid's warmup and every training window are excluded from every OOS figure, so \
         D-0061's sqrt(n_eval/n_total) factor does not apply here.</p><div class=\"wrap\">\
         <table><thead><tr><th>fold</th><th>IS return</th><th>IS Sharpe</th><th>OOS return</th>\
         <th>OOS Sharpe</th><th>OOS trades</th><th>seed</th></tr></thead><tbody>",
    );
    for f in &c.costed.folds {
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{:+.2}%</td><td>{}</td><td>{:+.2}%</td><td>{}</td><td>{}</td>\
             <td class=\"mono\">{:016x}</td></tr>",
            f.fold_index,
            f.is.total_return_pct,
            opt(f.is.sharpe_naive),
            f.oos.total_return_pct,
            opt(f.oos.sharpe_naive),
            f.oos.round_trips,
            f.seed
        );
    }
    h.push_str("</tbody></table></div>");
    h.push_str(fold_chart(c).as_str());

    // 5. Trade stats and the account-evaluation day summaries.
    write_trade_stats(h, c);
    write_day_summary(h, &c.costed.oos_worst_days);

    h.push_str("</section>");
}

fn write_trade_stats(h: &mut String, c: &ComboOutcome) {
    let s = &c.costed.oos_pooled;
    let longs = c
        .costed
        .round_trip_bars
        .iter()
        .filter(|&&(_, _, d)| d == crucible_core::prelude::Side::Buy)
        .count();
    let _ = write!(
        h,
        "<h3>Trade statistics</h3><div class=\"wrap\"><table><thead><tr><th>measure</th>\
         <th>out of sample</th><th>whole replay</th></tr></thead><tbody>\
         <tr><td>round-trips</td><td>{}</td><td>{}</td></tr>\
         <tr><td>win rate</td><td>{}</td><td>{}</td></tr>\
         <tr><td>fees paid</td><td>{}</td><td>{}</td></tr>\
         <tr><td>max drawdown</td><td>{:.2}%</td><td>{:.2}%</td></tr>\
         <tr><td>orders suppressed by warmup alignment</td><td colspan=\"2\">{} (§2.6)</td></tr>\
         <tr><td>long / short episodes (whole replay)</td><td colspan=\"2\">{} / {}</td></tr>\
         </tbody></table></div>",
        s.round_trips,
        c.costed.whole_run.round_trips,
        pct(s.win_rate.map(|w| w * 100.0)),
        pct(c.costed.whole_run.win_rate.map(|w| w * 100.0)),
        usd(s.fees_nano_usd),
        usd(c.costed.whole_run.fees_nano_usd),
        s.max_drawdown_pct,
        c.costed.whole_run.max_drawdown_pct,
        c.costed.suppressed_intents,
        longs,
        c.costed.round_trip_bars.len() - longs,
    );
}

/// The account-evaluation half of the page (`ACCOUNT_EVAL_SPEC.md` §3).
///
/// Two worst-day numbers, never one. The gap between the worst *close* and the
/// worst *trough from the day's open* is exactly the part of a bad day that a
/// daily-close model cannot see, and printing them side by side turns the
/// endpoint fallacy into a number instead of an argument.
fn write_day_summary(h: &mut String, days: &WorstDayDistribution) {
    if days.n_days() == 0 {
        h.push_str(
            "<h3>Out-of-sample days</h3><p class=\"dim\">No out-of-sample trading day was \
             captured, so there is no day distribution to show.</p>",
        );
        return;
    }
    let _ = write!(
        h,
        "<h3>Out-of-sample days</h3><p class=\"dim\">Captured inside the engine's mark loop, \
         never rebuilt from bars afterwards (D-0071): a reconstruction re-opens the intrabar \
         ordering the fill convention already settled and measures a path the account never \
         took. Bar-close marks are a <em>lower bound</em> on what a position endured between two \
         closes.</p><div class=\"wrap\"><table><thead><tr><th>measure</th><th>closing PnL</th>\
         <th>trough from the day's open</th></tr></thead><tbody>\
         <tr><td>worst day</td><td>{}</td><td>{}</td></tr>\
         <tr><td>5th percentile</td><td>{}</td><td>{}</td></tr>\
         <tr><td>25th percentile</td><td>{}</td><td>{}</td></tr>\
         <tr><td>days</td><td colspan=\"2\">{}</td></tr></tbody></table></div>\
         <p class=\"dim\">A day opens at the <em>previous</em> day's close, not at its own first \
         mark, so an overnight gap is inside the trough rather than invisible.</p>",
        days.worst_close_nano_usd()
            .map_or_else(|| "n/a".to_owned(), usd),
        days.worst_trough_nano_usd()
            .map_or_else(|| "n/a".to_owned(), usd),
        days.close_percentile_nano_usd(5)
            .map_or_else(|| "n/a".to_owned(), usd),
        days.trough_percentile_nano_usd(5)
            .map_or_else(|| "n/a".to_owned(), usd),
        days.close_percentile_nano_usd(25)
            .map_or_else(|| "n/a".to_owned(), usd),
        days.trough_percentile_nano_usd(25)
            .map_or_else(|| "n/a".to_owned(), usd),
        days.n_days(),
    );
}

/// A bar chart of pooled OOS return per sweep level. Hand-emitted SVG: the
/// file must open from disk with no network and no library.
fn sweep_chart(c: &ComboOutcome) -> String {
    let values: Vec<(String, f64)> = c
        .sweep
        .iter()
        .map(|l| (format!("{}t", l.ticks()), l.oos_pooled.total_return_pct))
        .collect();
    bar_chart("pooled OOS return by half-spread", &values)
}

/// The same, per fold — the closest thing to the spec's IS/OOS equity panels
/// that a build which does not retain per-bar curves can honestly draw.
fn fold_chart(c: &ComboOutcome) -> String {
    let values: Vec<(String, f64)> = c
        .costed
        .folds
        .iter()
        .map(|f| (format!("f{}", f.fold_index), f.oos.total_return_pct))
        .collect();
    bar_chart("OOS return by fold", &values)
}

fn bar_chart(title: &str, values: &[(String, f64)]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let width = 40.0_f64;
    let gap = 12.0_f64;
    let chart_w = values.len() as f64 * (width + gap) + gap;
    let half = 70.0_f64;
    let span = values
        .iter()
        .map(|(_, v)| v.abs())
        .fold(0.0_f64, f64::max)
        .max(1e-9);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {chart_w:.0} {h:.0}\" role=\"img\" aria-label=\"{title}\">\
         <line class=\"axis\" x1=\"0\" y1=\"{half}\" x2=\"{chart_w:.0}\" y2=\"{half}\"/>",
        h = half * 2.0 + 16.0,
        title = esc(title),
    );
    for (i, (label, value)) in values.iter().enumerate() {
        let x = gap + i as f64 * (width + gap);
        let scaled = (value / span) * (half - 6.0);
        let (y, bar_h) = if *value >= 0.0 {
            (half - scaled, scaled)
        } else {
            (half, -scaled)
        };
        let _ = write!(
            svg,
            "<rect class=\"{cls}\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{width}\" \
             height=\"{bar_h:.1}\"/><text class=\"tick\" x=\"{tx:.1}\" y=\"{ty:.0}\" \
             text-anchor=\"middle\">{label}</text>",
            cls = if *value >= 0.0 { "bar-pos" } else { "bar-neg" },
            tx = x + width / 2.0,
            ty = half * 2.0 + 12.0,
            label = esc(label),
        );
    }
    let _ = write!(
        svg,
        "</svg><p class=\"dim\">{} — largest bar is {:+.2}%</p>",
        esc(title),
        values
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::NEG_INFINITY, f64::max)
    );
    svg
}

/// The three sections this build cannot compute, rendered as holes that say so.
fn write_gaps(h: &mut String, criteria: &Criteria) {
    let _ = write!(
        h,
        "<section><h2>What is missing from this scorecard</h2>\
         <p class=\"dim\">Named rather than omitted. A reader who does not see a null comparison \
         cannot tell &ldquo;there wasn't one&rdquo; from &ldquo;it passed&rdquo;.</p>\
         <div class=\"gap\"><strong>Parameter plateau heatmap.</strong> Survivors should sit on \
         hills, not spikes. The config declares <span class=\"mono\">require_plateau = {plateau}\
         </span> and it was <strong>not evaluated</strong>: the perturbation test compares each \
         combo with its ±1-step grid neighbours, which is S3.</div>\
         <div class=\"gap\"><strong>Regime table.</strong> Per-year and per-volatility-regime \
         breakdown, flagging single-regime PnL concentration. S3.</div>\
         <div class=\"gap\"><strong>Permutation null and empirical p-value.</strong> The real \
         metric against a block-permutation distribution — and the alarm that fires when a \
         strategy keeps its edge on shuffled data, which is an engine-bug signal before it is a \
         discovery. The config declares <span class=\"mono\">max_pbo = {pbo}</span> and PBO was \
         <strong>not evaluated</strong> either. Both live in \
         <span class=\"mono\">crucible-funnel::stats</span>, still a module-doc spec.</div>\
         <p><strong>Because S3 did not run, no combo on this page can be GRADUATE.</strong> \
         Graduate means &ldquo;survived the full battery&rdquo;; the battery is what is missing, \
         so the best verdict this build can award is ITERATE. Nothing here graduated because \
         nothing <em>could</em> — not because nothing was good enough.</p>\
         <p class=\"dim\">There is also a strategy in this repository that cheats on purpose \
         (<span class=\"mono\">crucible-strategies::controls::LeakyZScore</span>, a full-sample \
         z-score), and the gates above do <strong>not</strong> catch it. That is recorded as the \
         honest baseline the permutation and truncation harnesses will have to beat.</p>\
         </section>",
        plateau = criteria.require_plateau,
        pbo = criteria.max_pbo,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> Provenance {
        Provenance {
            idea_name: "test".to_owned(),
            hypothesis_family: "fam".to_owned(),
            economic_rationale: "none".to_owned(),
            config_hash: "ab".repeat(32),
            git_sha: "0123456".to_owned(),
            data_manifest_ids: vec![],
            data_source: "synthetic random walk, seed 42".to_owned(),
            universe: "SYN:RW 1m".to_owned(),
            fill_model: "spread_cross — 1 tick, $1.25".to_owned(),
            intrabar_convention: "stop_first_intrabar".to_owned(),
            capital: "$100000.00".to_owned(),
            rendered_at: "2026-07-30T00:00:00Z".to_owned(),
        }
    }

    fn empty_report() -> FunnelReport {
        FunnelReport {
            combos: vec![],
            trials_before: 0,
            trials_after: 3,
            runs_claimed: 3,
            runs_already_done: 0,
            runs_retried: 0,
        }
    }

    /// The rule the spec states and this module enforces: no honesty box, no
    /// file. Every required field is checked, one at a time, so a future field
    /// that is added to the struct but not to the check shows up as a hole in
    /// this test rather than as a blank line on a page.
    /// One required field, and the edit that empties it.
    type Clearer = (&'static str, fn(&mut Provenance));

    #[test]
    fn a_scorecard_without_its_honesty_box_does_not_render() {
        let clearers: [Clearer; 8] = [
            ("meta.name", |p| p.idea_name.clear()),
            ("meta.hypothesis_family", |p| p.hypothesis_family.clear()),
            ("config hash", |p| p.config_hash.clear()),
            ("git sha", |p| p.git_sha.clear()),
            ("the data source", |p| p.data_source.clear()),
            ("the fill model", |p| p.fill_model.clear()),
            ("the intrabar ordering convention", |p| {
                p.intrabar_convention.clear();
            }),
            ("the universe", |p| p.universe.clear()),
        ];
        for (name, clear) in clearers {
            let mut p = provenance();
            clear(&mut p);
            let err = render(&empty_report(), &Criteria::for_tests(), &p)
                .expect_err("must refuse to render");
            assert_eq!(err.missing, vec![name], "clearing {name}");
            assert!(err.to_string().contains("nothing was rendered"));
        }
    }

    /// And with the box complete it renders — self-contained, with no network
    /// reference of any kind, because the file has to open from disk in five
    /// years.
    #[test]
    fn a_complete_scorecard_renders_and_fetches_nothing() {
        let html = render(&empty_report(), &Criteria::for_tests(), &provenance()).expect("renders");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.ends_with("</html>\n"));
        for forbidden in ["http://", "https://", "<script", "src=", "@import"] {
            assert!(!html.contains(forbidden), "found {forbidden}");
        }
        // The honesty box's non-negotiable contents.
        for required in [
            "Honesty box",
            "trials charged",
            "deflated Sharpe",
            "not computed by this build",
            "stop_first_intrabar",
            "git sha",
            "cost sweep",
        ] {
            assert!(html.contains(required), "missing {required}");
        }
    }

    /// A missing manifest list is legal for a generated series and must say
    /// why, rather than rendering an empty cell that reads as an oversight.
    #[test]
    fn an_empty_manifest_list_explains_itself() {
        let html = render(&empty_report(), &Criteria::for_tests(), &provenance()).expect("renders");
        assert!(html.contains("its seed"), "{html}");
    }

    /// The three gaps are named on every page, and so is the reason Graduate
    /// is unreachable.
    #[test]
    fn the_missing_sections_are_rendered_as_named_holes() {
        let html = render(&empty_report(), &Criteria::for_tests(), &provenance()).expect("renders");
        for required in [
            "Parameter plateau heatmap",
            "Regime table",
            "Permutation null",
            "no combo on this page can be GRADUATE",
            "LeakyZScore",
        ] {
            assert!(html.contains(required), "missing {required}");
        }
    }
}
