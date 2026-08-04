---
id: H-007
slug: trend-following-span
topic: momentum-horizon
grade: A
hypothesis_family: es-trend-span-cost-optimal
status: run
created: 2026-07-30
---

> **RUN 2026-07-31 — verdict `Kill`, decided at `admission`.** 100 combos, 100
> trials charged, determinism hash `4821d8831fd3c39a` reproduced across two
> runs. Killed for **sample adequacy**: 58 pooled out-of-sample sessions against
> the 250 this file registered. That is the designed triage outcome, not a
> disappointment — no profitability verdict is attached and none may be quoted.
> Full result at the end of this file, including what Tests 1 and 2 could and
> could not be evaluated on.

# H-007 — The cost-optimal trend-following span, and the structure around it

## Citation

Artur Sepp, Vladimir Lucic, **"The Science and Practice of Trend-Following
Systems"**, arXiv:2607.19497 (submitted 2026-07-21).

- <https://arxiv.org/abs/2607.19497>

Their stated claims, as given in the abstract: trend-following systems generate
profits when **long-term autocorrelation is positive, even under short-term
mean reversion**; alpha appears as **excess spectral mass at low frequencies**;
longer lookback windows capture both trend persistence and the squared drift of
the return process; the authors derive **closed-form expressions for the net
Sharpe ratio and the cost-optimal span under trading costs**; and
trend-following returns exhibit **positive skewness at every horizon**.

## Mechanism

A moving-average crossover is a low-pass filter, and the paper's contribution is
to say precisely what has to be true of the price process for filtering to pay:
the return series must carry more power at low frequencies than a random walk
does. That is an assumption about who is trading, and the standard story names
two losers. The first is the **risk transferrer** — hedgers and rebalancers who
must reduce exposure as a move goes against them, mechanically supplying the
trend with the flow that extends it; they keep doing it because the alternative
is carrying a risk their mandate forbids. The second is the **slow-updating
holder**, who sells a long position gradually over weeks rather than at once,
producing exactly the low-frequency drift the filter is tuned to detect. Against
that sits the cost term, which the paper makes explicit: a faster span reacts to
more of the signal but pays the spread more often, so net Sharpe is hump-shaped
in the span and the peak moves *longer* as costs rise. That last relationship is
the part of the paper that is testable without the strategy being profitable at
all, and it is what this hypothesis is built around.

## Signal in Crucible terms

An EMA crossover grid — which is `SmaCross` generalized, i.e. the layer Crucible
already has — evaluated not on "did it make money" but on **three structural
predictions**.

- **Basket:** ES first (`ESH2024` for the proven-transcoded window), then GC and
  RTY, which have the full 2010→2026 transcode and are genuinely different
  regimes.
- **Timeframe:** `1m` (the only grain we can replay; see Honesty note).
- **Features:** `ema(fast)`, `ema(slow)`, crossover in both directions.

Runnable today, as `configs/` material:

```toml
# EXTRACTED: configs/hypotheses/H-007-trend-following-span.toml
# This file and the ```toml block in research/backlog/H-007-trend-following-span.md
# are asserted BYTE-IDENTICAL by crucible-cli/tests/backlog_registration.rs.
# Edit one, edit both — the registration and the config that runs are one
# artifact precisely so the registered grid and the run grid cannot differ.

schema_version = 1

[meta]
name = "es-trend-span-cost-optimal"
hypothesis_family = "es-trend-span-cost-optimal"
economic_rationale = "Trend-following pays when low-frequency autocorrelation is positive; net Sharpe is hump-shaped in filter span and the optimum lengthens as costs rise (Sepp & Lucic 2026)."

[universe]
# FOUR-digit year (D-0072). The vendor spelling `ESH4` has a one-digit year that
# repeats every ten years, and our windows are sixteen years long; the grid
# commands do not resolve the shorthand — only `backtest` does.
instruments = ["ESH2024"]
timeframes = ["1m"]

[data]
source = "curated"
# ESH2024's front-month life: it takes over when ESZ2023 expires and runs to its
# own March expiry. ~60 sessions — a triage sample, not a verdict sample.
start = "2023-12-15"
end = "2024-03-15"

[contract]
tick_points = "0.25"
point_value_usd = 50

[indicators.fast]
kind = "ema"
period = { start = 10, end = 100, step = 10 }

[indicators.slow]
kind = "ema"
period = { start = 120, end = 480, step = 40 }

[rules]
enter_long = "fast crosses_above slow"
exit_long = "fast crosses_below slow"
enter_short = "fast crosses_below slow"
exit_short = "fast crosses_above slow"

[execution]
fill_model = "spread_cross"
half_spread_ticks = 1
fee_per_contract_usd = "1.25"

# Fold geometry, in TRADING DAYS (D-0062). Every value below is taken verbatim
# from `configs/combo-smoke.toml`, the canonical fold layout that pins the
# walk-forward determinism hash 711e1cb34a2ee2b4 — this file registers no fold
# parameter of its own invention, because a fold layout chosen per hypothesis is
# a free parameter, and a free parameter chosen by the person who wants the
# result is the thing pre-registration exists to remove.
#
# NOTE, because it is a real limitation and not a detail: those values were
# calibrated for a ~14-day synthetic fixture. Against this file's ~60-session
# contract they cut roughly 27 folds of a 2-session test window each, pooling to
# ~54 out-of-sample sessions. That is far below the 250 registered below and the
# run will be killed for sample adequacy — which is this file's stated design
# (see "The sample floors are deliberately unsatisfiable"), not a surprise.
[walk_forward]
scheme = "rolling"
train_days = 5
test_days = 2
step_days = 2

# ---------------------------------------------------------------------------
# COPY. Canonical source: `configs/example-combo.toml`, owned by the funnel
# workstream (README §2.3). `deny_unknown_fields` makes a stale block a hard
# load error — diff against the shipped config before running.
# `s3` is REFUSED at load, not skipped: its battery is still owed. `s0` was
# refused too until the predictor seam landed (D-0085); this file does not
# declare it, because its tests are structural rather than predictive and a
# stage with no criteria is a stage with no pre-registration.
# ---------------------------------------------------------------------------
[funnel]
stages = ["s1", "s2"]
cost_sensitivity_ticks = [0.0, 0.5, 1.0, 2.0]
min_oos_trades = 200                  # this file's pre-registered sample floor
min_oos_sessions = 250                # ...which one contract CANNOT satisfy
min_oos_return_pct_free_fills = 0.0   # S1: dead cheaply if it loses cost-free
min_oos_sharpe_after_costs = 0.5
kill_if_dead_at_ticks = 1.0
require_controls_beaten = true        # vs random-entry AND buy-and-hold
max_pbo = 0.5                         # declared; EVALUATED since D-0109
require_plateau = true                # declared; NOT evaluated (S3)

[run]
seed = 42
initial_cash_usd = "100000"
qty_contracts = 1
```

Every construct above exists today: `ema` with an integer `{start, end, step}`
axis, `crosses_above`/`crosses_below`, `spread_cross`, and the mandatory cost
sweep. Nothing here needs new Rust.

**The sample floors are deliberately unsatisfiable by this config, and that is
the design.** `min_oos_sessions = 250` against a ~60-session contract means the
funnel will **kill this run for sample adequacy** before reporting a
performance number. That is the pre-registration being enforced by a machine
rather than by my restraint, and it is the correct outcome: it makes the run a
*triage* run whose structural output (Tests 1 and 2 below) is readable while its
profitability verdict is explicitly withheld. Lowering these floors to make a
single contract "pass" would be exactly the post-hoc adjustment CLAUDE.md
forbids. They come down only when **registry pooling** (README §6.2, unlock 5)
supplies the sessions honestly.

## Data

**Owned and sufficient for the structural test:** curated `1m` bars. ESH2024's
January-2024 window is the one proven by the existing `backtest` example; GC and
RTY carry the full 2010→2026 transcode today.

**The sample ceiling is the real constraint, not the data.** `combo` replays raw
contracts only (D-0042 — see `research/backlog/README.md` §2.2), so one config
covers one contract's life. The full-span test requires either the D-0042
consumer or pooling across contracts through the M3 registry.

**Not needed:** no purchase, no new schema, no options, no calendar.

## Pre-registered kill criteria

The point of this file is that **two of the three tests do not require the
strategy to be profitable**, so it produces information even when it loses
money — which, on a 1-minute grain against a one-tick spread, is the expected
outcome.

**Test 1 — the cost-optimal span shifts (primary).** Run the grid at each of
the mandated cost-sensitivity points (0, 0.5, 1, 2 ticks). Record the span that
maximizes net Sharpe at each. The paper predicts the argmax span is
**monotonically non-decreasing in cost**.
- Strictly increasing across at least 3 of the 4 cost points → **prediction
  confirmed**.
- Non-monotone, or flat across all four → **Kill**. A cost model that does not
  move the optimum is either a strategy with no turnover sensitivity or a cost
  model that is not biting, and both are worth knowing.

**Test 2 — positive skewness (secondary).** Per-round-trip PnL skewness must be
**> 0** at the median span, with a block-bootstrap 95 % CI (block = 20
sessions) excluding zero. Negative skew with a CI excluding zero **falsifies the
paper's structural claim on our data** and is recorded as such — a result, not a
failure.

**Test 3 — profitability (tertiary, and expected to fail).**
- `min_oos_sharpe_after_costs = 0.5`, `kill_if_dead_at_ticks = 1.0`,
  `require_controls_beaten = true`. `max_pbo` and `require_plateau` were
  declared and echoed but **not evaluated by the build this was registered
  against** (S3 was owed), and the scorecard rendered both as named holes.
  **Corrected 2026-08-04:** PBO/CSCV landed (D-0109), so `max_pbo` is now an
  enforced gate at the value registered here — the threshold did not move, only
  whether it bites. `require_plateau` is still a named hole. The run recorded
  below predates that and says so in its own words.
- Sample minimum before *any* profitability verdict: **200 round trips** and
  **250 trading sessions pooled across contracts**, encoded as `min_oos_trades`
  and `min_oos_sessions` above. A single-contract run reaches neither and will
  be killed for sample adequacy — a **triage run with no profitability verdict
  attached**, enforced by the funnel rather than by my discipline.
- **The ceiling is `Iterate`, not `Graduate`** (D-0075): S3's battery is what
  `Graduate` means, and it is not built. Nothing in this file can graduate, and
  a criterion implying otherwise would be a criterion this build cannot honour.

**Trial counting.** The grid above is 10 × 10 = 100 combos, times 4 cost points,
times each contract. Every one charges `es-trend-span-cost-optimal`. That count
is large and it is supposed to be — it is the honest denominator for any Sharpe
this produces, and the deflated Sharpe reads it from the registry, never from
memory.

**Negative control, required before believing Test 1.** The identical grid must
be run on `configs/combo-smoke.toml`'s seeded random walk. A random walk has no
low-frequency excess power, so the cost-optimal span shift **must not appear**
there. If it does, what we have measured is an artifact of turnover and the
cost model, not a property of the price process, and Test 1 is void.

## Honesty note

- **The grain is wrong and this is the biggest caveat.** The paper is about
  trend-following systems as practised — daily bars, spans of weeks to months.
  This config replays 1m, so a 480-period EMA is an eight-hour span, not an
  eight-week one. The *structural* predictions (hump-shaped net Sharpe,
  cost-optimal span, positive skew) are stated in a form that does not depend on
  the grain, which is why this is testable today — but **the paper's
  calibration is not being tested**, and no result here may be described as
  confirming or refuting their empirical findings.

  **Corrected 2026-07-31:** this bullet used to read "we can only replay 1s and
  1m (no resampler)", which stopped being true when D-0077 landed. `5m`, `15m`,
  `1h` and `1d` are aggregated on read, on the exchange's own sessions, and a
  daily bar is a trading-day bar. So the grain objection is now a *choice* this
  file makes rather than a limit the build imposes, and the honest thing is to
  say which: a daily-grain variant is registrable today and is **not** this
  file. It would be a second pre-registration with its own trial count against
  the same family — not an edit to this one after seeing its result, and not a
  timeframe swapped into the config above. The sample ceiling is what makes it a
  separate question anyway: ~60 sessions is ~60 daily bars, which cannot carry a
  120–480 period axis at all.
- **Expect it to lose money.** ES at 1-minute with a one-tick half-spread is a
  hostile cost environment for a crossover, and the project's own reference run
  (SMA 20/50 on ESH2024, January 2024) lost 23.51 % under exactly this fill model.
  `SmaCross` is not supposed to be profitable (CLAUDE.md §9) and neither is
  this. The value is Test 1 and Test 2.
- **Sample overlap: none.** The paper was submitted nine days before this file
  was written; our data ends 2026-07-28 and theirs is a theory paper with
  illustrative calibration rather than a claim about a specific sample. There is
  no decay story here, and equally no independent confirmation to lean on.
- **arXiv preprint, not peer-reviewed**, and very recent — it has had no time
  to attract criticism. Sepp is a well-known practitioner-researcher, which is
  a reason to read it, not a reason to believe it.
- **One month of ES is roughly 21 sessions and ~30,000 bars.** That is enough
  bars to fit a hundred combos and nowhere near enough independent sessions to
  distinguish them. The sample-size gate is what stops that from becoming a
  number in a report.

## Triage grade

**A.** Expressible in combo TOML today with no new Rust — the config above uses
only `ema`, integer axes, crossover operators, and the cost sweep that is
mandatory anyway. It is the strongest A in this sweep because its primary test
is structural rather than profit-based, it comes with its own negative control
on the existing random-walk harness, and it exercises the cost-sensitivity
machinery the project already requires on every result.

## Result — 2026-07-31

Run: `crucible funnel --config configs/hypotheses/H-007-trend-following-span.toml
--out results`, twice. Exit 5 both times (every combo killed). Determinism hash
`4821d8831fd3c39a` and all 100 verdict rows identical across the two runs; the
only differences between the two scorecards are the render timestamp, the
trials-before-this-run figure, and the registry's own claim/dedupe counts —
all statements about the *run*, none about the research.

**Verdict: `Kill`, decided at `admission`, 100 of 100 combos.** Trials charged:
100, read from the registry. Config hash `aa840e8b3abc…`.

The deciding gate is **sample adequacy**, exactly as this file predicted:
58 pooled out-of-sample sessions against `min_oos_sessions = 250`. Round-trips
were never the binding constraint — combos produced 220 to 1,396 of them against
a floor of 200. **No profitability verdict is attached to this run** and none may
be quoted from it. The floors come down when registry pooling supplies the
sessions, never to make this contract pass.

### Test 1 — the cost-optimal span. NOT evaluated as registered.

The registered test reads "record the span that maximizes **net Sharpe** at each
cost point". **This build does not produce that number.** The mandatory cost
sweep reports pooled out-of-sample **return** at 0 / 0.5 / 1 / 2 ticks; Sharpe is
computed only under the config's own fill model, once. So the registered
statistic does not exist in the output, and substituting return for it would be
answering a different question in the shape of the registered one.

What the return proxy shows, labelled as a proxy: the argmax is
`fast=90, slow=480` at **all four** cost levels — flat, which is the branch this
file registered as `Kill`.

**But scoring that branch would be wrong, and this is the real finding.** The
argmax sits on the **grid's longest span** in 20 of the 40 (fast × cost-level)
slices, and along `fast=90` the return rises monotonically with the slow span all
the way to the 480 boundary:

| slow | 0t | 0.5t | 1t | 2t |
|---|---|---|---|---|
| 120 | -12.81 | -19.00 | -25.19 | -37.56 |
| 280 | -7.05 | -10.94 | -14.83 | -22.60 |
| 400 | -3.90 | -7.15 | -10.40 | -16.90 |
| 480 | **+0.02** | **-2.79** | **-5.58** | **-11.19** |

The hump's peak is **not bracketed by the grid** — it lies at or beyond 480. A
test that asks "does the argmax move *right* as costs rise" cannot observe
movement when the argmax is already pinned at the right edge at zero cost. So
Test 1 is **structurally uninformative on this grid**, which is a different and
more useful statement than "flat, therefore killed". The grid, not the paper, is
what this run falsified. A re-registration would have to extend the slow axis
well past 480 and confirm the peak is interior before the monotonicity question
can be asked at all.

### Test 2 — positive skewness. Not evaluated.

Per-round-trip PnL skewness with a block-bootstrap CI is not computed by this
build, and no field in the registry or the scorecard carries it. Unevaluated —
not "failed".

### Test 3 — profitability. Withheld by design.

Every combo lost money at every cost level under the config's own fill model.
Both mandatory controls rendered: every combo **beat the matched random-entry
control 16 of 16 draws**, and every combo **lost to buy-and-hold (0 of 1)**,
which returned +17.60 % over the window. Under `require_controls_beaten = true`
that is a failed control regardless of the sample gate. `max_pbo` and
`require_plateau` were echoed and not evaluated (S3 is owed); the scorecard
renders both as named holes. Ceiling was `Iterate` and `Graduate` was
unreachable (D-0075).
