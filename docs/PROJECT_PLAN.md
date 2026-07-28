# Crucible — Project Plan

**Status:** working document · owner: Liron Kruch · last revised 2026-07-28
**Precedence:** this file is the strategic map. `docs/MILESTONES.md` remains the
executable checklist and the source of truth for *current scope* (CLAUDE.md §8);
on any conflict, MILESTONES wins and this file gets corrected at the next
milestone boundary. Update this file only at milestone boundaries — never
mid-sprint — so it cannot drift into a second source of truth.

---

## 1. What is being built, in one paragraph

Crucible is an event-driven backtesting engine for CME futures whose product is
**verdicts, not equity curves**: a strategy idea goes in, and a
kill/iterate/graduate scorecard comes out — produced under honest execution
assumptions, with overfitting statistics attached, from bit-reproducible runs.
It is simultaneously (a) the research tool for evaluating real strategy ideas
(indicator combos, the Volume Profile setups, the hybrid regime system), and
(b) a quant-research portfolio project whose engineering discipline —
availability-time replay, integer accounting, trial counting, permutation
harnesses — *is* the credential.

**Non-goals (permanent):** live order routing / automated execution of real
money; GPU acceleration; news/earnings scraping infrastructure (macro events
enter as static release-calendar CSVs); being a general-purpose OSS framework.

---

## 2. Requirements

### 2.1 Functional
- **Archive:** immutable local market-data lake (raw DBN + curated Parquet)
  with a manifest that answers "what do we own?" and "which bytes fed run X?";
  acquisition that can never double-purchase and never certifies bytes it
  didn't verify. *(built)*
- **Replay engine:** deterministic, single-threaded, availability-time-ordered
  event loop; named fill models; integer money. *(built; first ran on real
  archived ES bars 2026-07-28)*
- **Strategy layer:** streaming indicators; hand-written strategies;
  config-assembled indicator combos; score-emitting signals (predictor track).
- **Research funnel:** S0 signal triage → S1 free-fill coarse grid → S2
  walk-forward with costs → S3 statistical battery; pre-registered kill
  criteria; verdict scorecards.
- **Predictor workbench:** forecast evaluation *without trading* — score →
  forward-return joins, hit rate / IC / quantile / calibration tables. (Added
  2026-07-28 after external review; see §7.3.)
- **Research memory:** run registry with config-hash dedup, automatic trial
  counting per hypothesis family, strategy graveyard.
- **Later:** L1-calibrated spread models, MBO queue-sim fills, paper trading on
  the live feed, the hybrid LLM regime system as a tested strategy family.

### 2.2 Non-functional (ordered — earlier outranks later)
1. **Correctness:** no lookahead by construction; an engine that lies has
   negative value. Enforced by invariants (CLAUDE.md §2), golden tests,
   and (M3) permutation + truncation-invariance harnesses.
2. **Statistical honesty:** every number ships with its denominators —
   fill model, trial count, benchmark, sample size.
3. **Reproducibility:** same config + data ⇒ bit-identical results on any
   machine. *Empirically proven:* demo hash `b55747513df596ed` identical on
   Windows/MSVC/Rust 1.97 and Linux/Rust 1.95 (two independent runs).
4. **Throughput:** grid search saturates 8 cores (5800X3D) via run-level
   parallelism; single runs stay single-threaded.
5. **Cost discipline:** data spend is quoted before purchase, gated by
   integer-USD caps, and journaled; target all-in data cost through M4:
   **≈ $200–450** (one or two $199 Standard months + ~$65/mo metered cron +
   change).

### 2.3 Constraints
- One developer (econ+math undergrad) + Claude Code sessions, governed by
  CLAUDE.md / DECISIONS.md discipline; this chat thread acts as architect and
  independent reviewer of money-path and engine changes.
- Hardware: Ryzen 5800X3D (8c/16t, 96 MB V-cache), 32 GB DDR4, RTX 2070S
  (unused by design), NVMe archive at `G:/Crucible`. Bars live in RAM;
  tick/book data streams from disk.
- Data vendors: Databento (usage-based account now; exactly one Standard month
  planned for the archive blitz), ThetaData (already owned) for options,
  free CBOE/BLS/Fed sources for volatility indices and macro calendars.
- Timeline: part-time, semester-scale. Every milestone ends demoable so a
  hard stop at any point still leaves a complete artifact.

---

## 3. Architecture (summary — normative detail lives in CLAUDE.md §2–§4)

```
                        ┌─────────────────────────────────────────────────┐
   Databento batch API  │  crucible-data                                  │
  ──── pull (journal,   │  raw/  *.dbn.zst   immutable, checksummed       │
        reconcile,      │  manifest.jsonl    what we own, what fed run X  │
        $-gates) ─────► │  jobs.jsonl        every intent/submit/append   │
                        │  transcode ──► curated/ Parquet  (built)        │
                        └───────────────┬─────────────────────────────────┘
                                        │ ParquetBarFeed (built)
                                        ▼
                        ┌─────────────────────────────────────────────────┐
                        │  crucible-engine   (deterministic, sync)        │
                        │  fills → mark → decide, per event               │
                        │  avail_ts ordering · integer nano-USD           │
                        │  fill models: free_fills / spread_cross /       │
                        │               queue_sim (M4)                    │
                        └───────────────┬─────────────────────────────────┘
              strategies / indicators / │ signals (crucible-strategies)
                                        ▼
                        ┌─────────────────────────────────────────────────┐
                        │  crucible-funnel   (the only threaded crate)    │
                        │  S0 triage → S1 coarse → S2 walk-forward →      │
                        │  S3 battery (DSR·PBO·permutation·plateaus)      │
                        │  forecast workbench · registry · scorecards     │
                        └─────────────────────────────────────────────────┘
```

Load-bearing design facts: bars are stamped at interval *open* and become
knowable at `open + timeframe`; orders never fill against the event that
triggered them; money never touches `f64`; the engine core contains no clocks,
threads, or I/O; tokio exists only behind the `databento` feature's sync seam;
parallelism exists only at run granularity in the funnel. Decision log:
D-0001…D-0038 (`docs/DECISIONS.md`).

---

## 4. Current state — exact inventory (2026-07-28)

### Done and verified
| Area | State |
|---|---|
| M0 skeleton | Workspace, core types, engine loop, indicators (SMA/EMA/Bollinger), SmaCross, synthetic feeds, golden + determinism tests, CI with determinism gate |
| M1 archive catalog | Manifest records, half-open coverage algebra (+ proptest), checksum gatekeeping, `verify` audit; independently reviewed; 45 catalog tests |
| Cost/planning layer | Coverage-subtracted month/whole-gap planning, free-endpoint quoting, integer `--max-cost-usd` gate |
| Pull execute path | Job journal (event-sourced, deterministic intent ids), always-reconcile before submit, single-instance lock, staged download → size+SHA-256 verify → symbols union from DBN metadata → append; clap CLI (`pull`, `verify`, `demo`); exit-code contract; feature-gated `databento` with `--all-features` CI |
| Live proof | $0.14 validation pull submitted once across four invocations (one crashed mid-poll on a stale pooled connection — D-0035 retry policy born from it); no-double-purchase proven **in production, not just tests**. Closed 2026-07-27: `verify` clean, re-run buys nothing |
| Curated store + feed | `transcode` (DBN → Parquet, one file per instrument × window, integer columns end to end) and `ParquetBarFeed`; provenance chains every bar to a manifest id; `backtest` runs the reference strategy on it (D-0036…D-0038) |
| Tests | 206 green in a default build, 216 with `--all-features`; determinism hash unchanged across 3 platform/toolchain combos |
| Docs | CLAUDE.md, DECISIONS (D-0001…D-0038), MILESTONES, DATA_PLAN, RUNBOOK_BLITZ |

### Not built yet
The working-backtester checkpoint is **passed** (2026-07-28): SMA-cross ran on
real archived ESH4 January-2024 1-minute bars. Still missing in M1: session
calendar v1, continuous contracts v1, and the data-QA report. Everything
research-facing (walk-forward, combos, funnel, forecast workbench, registry,
scorecards) sits behind M2.

---

## 5. Critical path to "a working backtester"

The shortest path from today to *SMA-cross running on 16 years of real ES
bars* — the moment the project stops being infrastructure and starts being a
backtester:

1. ~~**Close the acceptance pull.**~~ **Done 2026-07-27.** File at the planned
   path, one manifest row carrying `ES.FUT` plus 41 observed contracts and
   spreads, `verify` clean, and a re-run of the identical command reports
   "nothing to download — the archive already covers this" and exits 0.
2. **Independent review** of `execute.rs`, journal fold, reconciliation
   matcher, retry classification (architect review, already committed to).
3. **Subscribe + blitz** (RUNBOOK_BLITZ): start in the **first 2–3 days** of
   the subscription month; every job must quote $0.00 through our own gate;
   order: `mbo`, `tbbo`, `trades` first (rolling windows decay daily), then
   bars/definitions/statistics. Expect days of wall-clock (FIFO vendor queue);
   `crucible verify` after each tranche; cancel before day 30.
4. ~~**`transcode` + `ParquetBarFeed`.**~~ **Done 2026-07-28.** DBN →
   `curated/bars/{instrument}/{tf}/{window}.parquet`, integer columns end to
   end, a `Feed` reading it in availability order with every failure resolved
   at construction. Two goldens: synthetic bars round-trip through Parquet
   bit-identically, and synthetic bars round-trip through a real DBN encode →
   transcode → Parquet read bit-identically. **← "working backtester"
   checkpoint, passed.**
5. ~~**First honest number.**~~ **Done 2026-07-28.** SMA(20/50) on ESH4,
   January 2024, 30,167 1-minute bars, `spread_cross` at 1 tick + $1.25:
   **−23.51 %** of capital, 665 round trips, 27.1 % win rate, naive Sharpe
   −11.39. Costless upper bound is **−5.21 %**, so the strategy has no edge
   *before* costs and costs roughly quadruple the loss (the half-spread sweep
   is exactly linear at $16,637.50 per tick over 1,331 contracts). The control
   group works.

Estimated wall-clock: 1–2 weeks part-time, dominated by the subscription-month
start date (choose it deliberately; the clock burns from day one).

---

## 6. Roadmap

Effort estimates are part-time. Each milestone's exit criterion is demoable.

### M1 — Data foundation *(≈ 90% done; finish ≈ 1 week + the blitz)*
Remaining: §5 steps 2–3 (review, then the subscription blitz), plus session
calendar v1 (Globex sessions, holidays, `bars_per_year`), continuous contracts
v1 (volume-roll table; signals on back-adjusted, PnL on tradeable prices), and
the data-QA report (gaps, zero-volume runs, spikes, DST boundaries —
`condition.json` feeds it).
**Exit:** a fresh machine with an API key reaches a validated local archive
with one command sequence, and the demo strategy backtests on real ES bars.

### M2 — Engine hardening on real data *(≈ 3–4 weeks)*
Warmup alignment (identical eval windows across a grid); walk-forward runner
(anchored + rolling); config-assembled combo strategies (tagged-enum menu →
factory; deny-unknown-fields); stops/targets with worst-case intrabar
ordering, flagged as path-sensitive; risk-policy layer (`RiskLimits`: daily
loss/profit caps on marked equity, trade caps, trading windows, forced
flatten — pre-registered like any parameter); external cross-check (same
strategy, same data through NautilusTrader or hand-audited runs — the "why
trust your engine" answer); indicator numerics to Welford/rebased; criterion
baseline (bars/sec on the 5800X3D).
**Exit:** a TOML-defined combo backtests on real data with externally
cross-checked, reproducible results.

### M2.5 — Predictor workbench *(≈ 1–2 weeks; added after external review)*
Score-emitting signal trait beside `Strategy`; forward-return join at
configurable horizons; hit rate, IC, return-by-score-quantile, calibration
curves; **benchmark module** (buy-and-hold + random-entry with matched
turnover/holding — every report shows % of capital vs both); **sample-size
gates** (pre-registered minimum trade count *and* calendar/regime coverage
before any OOS verdict; effective-N and block-bootstrap CI on expectancy).
Point-in-time rule extended: all feature standardization uses rolling
statistics only.
**Exit:** one signal family (Volume Profile open-location) evaluated
predictor-first, with a report worth sending to an external reviewer.

### M3 — Funnel + statistics *(≈ 3–4 weeks)*
Grid expansion + blake3 config identity; DuckDB registry (runs, hypotheses,
trials, verdicts; insert-before-run; dedup/resume); rayon scheduler (dataset
semaphore, multi-instance pass: one replay feeding K combo instances);
stages S0–S3 with config-declared kill criteria; deflated Sharpe, PBO/CSCV,
block-permutation nulls; truncation-invariance harness in CI (with a planted
leak as the negative control); HTML scorecards with the mandatory honesty box;
cross-instrument rhyme checks (NQ/RTY). Stretch: signal factory (10–50
standardized features) + rolling-trained interpretable combiner
(hand-rolled ridge/logistic), applied strictly out-of-window.
**Exit:** `crucible funnel <config>` runs unattended to scorecards + registry
rows, and a deliberately-leaky test strategy is *caught* by the harnesses.

### M4 — Calibration + validation + write-up *(≈ 3–4 weeks)*
Spread/slippage calibrated from archived L1 (measured half-spread by
time-of-day replaces the hand-set 1 tick); MBO queue-position fill model
prototype; macro-event studies from the static calendar CSV; paper-trading
skeleton (same `Strategy` trait on the live feed — requires a subscription
month; metered live no longer exists); the mini-paper: methodology, one full
idea→verdict case study, limitations.
**Exit:** one strategy taken end-to-end through the funnel to a verdict, and
the write-up tells the story honestly (a rigorous kill is a *good* result).

### Post-M4 backlog (explicitly out of scope until here)
Hybrid LLM regime system backtested through the funnel (its data plan is
already mapped — DATA_PLAN.md; VIX complex from free CBOE data, EM/gamma from
ThetaData, SPY/QQQ micro-pull); SSRN-style cross-sectional strategies +
multi-instrument portfolio; ThetaData options integration; risk-layer ML;
team scaling (the registry's trial counting is what makes multi-person
research honest); FX assets.

---

## 7. Research methodology (how results earn belief)

1. **Funnel with kill gates** — optimistic and cheap early (S0/S1), brutal
   and expensive late (S2/S3). Most ideas must die in seconds. `free_fills`
   is legal only in S0/S1.
2. **Pre-registration** — kill criteria, thresholds, risk limits, and sample
   minimums live in the config *before* the run. Post-hoc threshold tuning is
   rationalization and does not ship.
3. **Predictor before system** — evaluate "can this score predict forward
   returns, at what horizon, with what calibration" *without trading* before
   any equity curve exists. Setups consume validated signals; they are not
   containers for unvalidated ones.
4. **Denominators always** — % of capital, vs buy-and-hold, vs random-entry
   matched baseline, N trades, N independent days, trial count, fill model.
   The dollar figure alone is banned output.
5. **Adversarial self-checks** — permutation (edge on shuffled data = bug
   alarm), truncation invariance (decisions identical when the future is
   deleted), plateaus over peaks, per-regime slices, cross-instrument rhymes.
6. **LLM boundaries** — LLMs interpret structured context and critique
   (regime narratives, contradiction-finding, macro/news later); they never
   compute numbers, never replace validation, and their claimed probabilities
   get calibrated (Brier/ECE) like any forecaster.
7. **Trial counting is automatic** — every run increments its hypothesis
   family in the registry; deflated Sharpe reads the count from there, never
   from memory.

---

## 8. Data & cost plan (quotes are real, from the account, 2026-07-24)

| Purchase | Size | Metered | Under Standard month |
|---|---|---|---|
| Validation: 1 mo ES 1m bars | ~MB | **$0.14 (spent)** | — |
| Bars 1m, 16 y, ES+NQ+RTY | 1.01 GiB | $70.39 | $0 |
| Bars 1s, 16 y, ES+NQ+RTY | 19.67 GiB | $1,377.17 | $0 |
| Definitions + statistics, 16 y | 1.0 GiB | $1.06 | $0 |
| Trades + TBBO, 12 mo, ES | 15.11 GiB | $423.03 | $0 |
| MBO, 1 mo, ES | 16.49 GiB | $29.68 | $0 |
| **Total** | **53.3 GiB** | **$1,901.47** | **$199** |

Blitz basket adds CL/6E/ZN/GC parents at $0 marginal cost inside the flat-rate
windows. Recurring archival cron after the blitz: ~**$65/month** metered
(mbo $30 + tbbo $22 + trades $13) — stays metered; resubscribe only for live
data at M4. Rate structure worth remembering: bars ≈ $70/GB, trades/tbbo ≈
$28/GB, MBO ≈ $1.80/GB — aggregation costs, raw feed is nearly free per byte;
`mbp-10` is never bought (derivable from MBO). Later micro-pull: SPY+QQQ 1m
bars, metered, tens of dollars. Free: CBOE VIX/VIX9D/VIX3M/VIX1D CSVs, VIX
futures settlements, FOMC/CPI/NFP calendars, earnings dates.

Research-honesty data caveats (into any write-up): MBO starts 2017-05-21;
pre-2017 GLBX is MDP2 with `F_BAD_TS_RECV` on every record; timestamps are
millisecond-resolution before 2015-11-20; VIX1D exists only since 2023
(compute a 0–1DTE IV proxy from options before that); daily SPX 0DTE only
since 2022. Options-featured backtests are bounded by ThetaData history
(~2012+), not the 16-year bar archive.

---

## 9. Verification & quality gates (all merge-blocking)

`cargo fmt --check` · `clippy -D warnings` (default and `--all-features`) ·
`cargo test --workspace` (both feature sets) · CI determinism double-run ·
golden tests changed only with hand-rederived arithmetic · new detectors ship
with a planted-failure negative control · independent architect review for
money-path and engine-semantics changes · every session ends with the demo
still behaving (a profitable demo under costs is a red alert, not a win).

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| Overfitting (the #1 research risk) | Funnel, pre-registration, trial registry, DSR/PBO, plateaus, sample-size gates |
| Engine silently lies | Invariants + golden + determinism + permutation + truncation harnesses; external cross-check in M2 |
| Double-purchase / data-cost creep | Journal + reconcile + locks (live-proven); integer $ gates; quotes before every buy; $0.00-gate detects subscription lapse |
| Subscription-month waste | Runbook: start blitz on day 1–3, rolling windows first, FIFO queue expectation set, cancel reminder |
| Scope creep (LLMs, options, FX, team…) | Non-goals list; one active milestone (CLAUDE.md §8); backlog quarantined post-M4 |
| Solo-dev context loss | CLAUDE.md + append-only DECISIONS + module-doc specs + this plan; sessions re-read them by contract |
| Vendor behavior changes | Vendor facts recorded with sources in session plans; reconciliation observes ground truth rather than assuming |
| Long quiet losing stretches mislead (per external review) | Predictor-first metrics decouple "is there signal" from "did the equity curve please us this month" |

---

## 11. Immediate next actions

1. Architect review of the execute path **and the curated/transcode path**
   (committed; blocks the subscription).
2. Pick the subscription start date; run the blitz per RUNBOOK_BLITZ, then
   `crucible transcode` each tranche as it lands.
3. Finish M1: session calendar v1 (which takes `bars_per_year` off `backtest`),
   continuous contracts v1, data-QA report.
4. Start Andrew Ng's ML course in parallel (feeds M2.5/M3 combiner).
5. At M2.5 exit: send the first predictor report to the external reviewer.

---

## 12. Document map

| Document | Role |
|---|---|
| `PROJECT_PLAN.md` | This file — strategic map; updated at milestone boundaries |
| `CLAUDE.md` | Session contract: invariants, architecture rules, style, workflow |
| `docs/DECISIONS.md` | Append-only decision log (D-0001…D-0038 and counting) |
| `docs/MILESTONES.md` | Executable checklist — **source of truth for current scope** |
| `docs/DATA_PLAN.md` | Buy list / do-not-buy list / data caveats |
| `docs/RUNBOOK_BLITZ.md` | Subscription-month operating procedure |
| `README.md` | Public face; falsification pitch; quick start |
