---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: conditional-quantiles-by-realized-variation
topic: vol-regime-clustering
grade: A
hypothesis_family: es-cl-trailing-variation-forward-quantiles
status: draft
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Trailing variation as a conditioner on the whole forward return distribution

> **This is a DRAFT, not a registration.** Nothing here has been run and
> nothing here is a recommendation. It was built from index metadata — title,
> venue, year, and the abstract the API returned — by `research/intake`;
> **the paper itself has not been read**. Promote it into `research/backlog/`
> by hand, after reading, or delete it.
>
> **The kill criteria below are PROPOSALS**, marked `criteria_status:
> proposed` in the front matter. A proposal is not a pre-registration: it
> becomes one when Liron approves it, by name, and the file is promoted. The
> marking is what lets a later reader tell a criterion someone committed to
> from a number a drafter suggested.

## Citation

Filip Zikes, Jozef Barunik. *Semiparametric Conditional Quantile Models for Financial Returns and Realized Volatility*.
arXiv q-fin, 2013.
**no DOI** (preprint). <http://arxiv.org/abs/1308.4276v1>
Retrieved from the arxiv API on 2026-08-06.

The authors model the conditional quantiles — not just the conditional mean — of future returns and of realized volatility, using measures of realized variation including separate upside and downside components and a jump term, alongside option-implied volatility. Working on S&P 500 and WTI crude futures, they report that fairly simple quantile specifications track both conditional distributions about as well as established benchmark models, and frame the result as a risk-management tool.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == None:
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Trailing variation forecasts the dispersion of the next period's returns far better than it forecasts their centre — close to the most robust finding in this whole literature, and also the reason to be suspicious of any directional strategy built on it. If only the width of the distribution moves and the middle does not, there is no directional payer at all: the value of the forecast sits in options pricing and in position sizing. Options we do not own; sizing the grammar cannot express. So the honest statement is that nobody is systematically paying you for knowing tomorrow's dispersion in a futures-only account, and this file says that rather than inventing a counterparty. What remains worth testing is the weaker half — whether the centre of the forward distribution shifts at all with the trailing variation state — and the funnel's S0 measures exactly that without placing a single trade. If it does not shift, the idea dies for the right reason and cheaply.

## Signal in Crucible terms

- `ESM2024` and `CLZ2024` as separate configs, `timeframes = ["15m"]`, one instrument each.
- S0 first, and the registered order is binding: `[s0] score = "vol"`, `horizons_minutes = [1, 5, 10, 20]`, `buckets = 5`, `bootstrap_draws = 500`, with `[indicators.vol] kind = "stdev", period = 20, source = "return"`. That bucketing of forward returns by a trailing-variation score IS the paper's conditional-quantile question, asked without trading.
- S1/S2 arm only if S0 passes: `[indicators.z] kind = "zscore", period = [20, 40], source = "close"`; `enter_long = "vol < 0.0015 and z crosses_below -2.0"`; `exit_long = "z crosses_above 0.0"`; mirrored short.
- All float thresholds enumerated (D-0060). Note `source = "return"` adds a bar to the grid's `max_warmup_bars` (D-0080), which is declared rather than absorbed so the whole grid starts together.
- What is NOT expressed, and should not be claimed later: semivariance, jump variation, integrated variance from tick data, and the implied-volatility regressor. `stdev(20, return)` is a trailing window, and it is a much blunter instrument than any of theirs.
- The gate above tests the LOW-variation bucket for reversion. A second config testing the high bucket belongs under the same family and is charged as further trials, not filed separately.

## Data

- Owned: ES and CL `ohlcv-1m` 2010-06-06 → 2026-07-28 — genuinely the two instruments the paper studied, which is the strongest data match in this batch.
- Owned: ES `trades` and `tbbo` for 2025-07-28 → 2026-07-28 only. A single year, ES only, which is nowhere near enough to build the tick-level variation estimators the paper uses and cannot cover CL at all.
- Not owned: options or implied-volatility data of any kind. One of the paper's main regressors is simply absent, so even a faithful build would be missing an input rather than approximating one.
- Constraint: one raw contract per config, roughly 60 sessions for ES.
- `half_spread_ticks = 1` (D-0120) is an assumption, and for CL it will remain one.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.05` at the same horizon at which the mean forward return's bootstrap interval excludes zero — basis: a trailing z-score on 20,000 bars of seeded random walk returns |IC| around 0.038, so any floor at or below that is measuring noise. S0 passes only on both halves together (D-0085).
- A saturated reading — |IC| near 1.0 — is treated as a LEAK SIGNATURE and the run is stopped and inspected, not celebrated. Nothing predicts the next ten minutes perfectly.
- `min_oos_sessions = 250` and `min_oos_trades = 150` — basis: the state variable spends most of its time in the middle, so the tail buckets need a year of sessions to be populated at all. Unreachable on one contract today.
- `min_oos_sharpe_after_costs = 0.3` and `kill_if_dead_at_ticks = 1.0` — basis: a reversion rule gated to LOW-variation windows is by construction trading small moves, which is where one tick of half-spread does the most damage; if it dies there it was arithmetic, not an edge.
- `require_plateau = true` over the `stdev` lookback and the variation threshold — basis: a distributional claim that holds at one lookback and not its neighbours contradicts its own premise.
- `max_permutation_p = 0.05` and `require_controls_beaten = true`.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- An arXiv preprint from 2013; the index records no refereed venue for it here, and nobody has read it. The restatement above comes from the indexed abstract alone.
- Their instruments are ours — the S&P 500 and WTI crude futures — which is rare in this batch and is the honest reason the data section is short on complaints.
- Their inputs are not ours. Integrated variance, upside and downside semivariance, jump variation and option-implied volatility are four separate estimators; we substitute one trailing standard deviation of returns and should not later describe that as a replication.
- The paper's contribution is distributional modelling for risk management. A quantile model that fits well implies no trading edge whatsoever, and the S0-first ordering above exists so that the directional question is asked and answered before any equity curve exists to rationalize.
- The paper reports its own model comparisons; they are not restated here and none passed through a fill model.
- `half_spread_ticks = 1` (D-0120) rests under every cost figure, and the low-variation arm is the construction most sensitive to it.

## Triage grade

**A.** A: the S0 block plus a trailing `stdev(period, return)` gate on a z-score core is legal TOML today, and S0 answers the directional half without trading. Runnable is not answerable — one ES or CL contract is roughly 60 sessions against `min_oos_sessions = 250`, so the machine kills these correctly for sample size until pooling lands. The faithful version, with semivariance, jumps and an implied input, is not grade A and is not what this registers.
