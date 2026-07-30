---
id: H-007
slug: trend-following-span
topic: momentum-horizon
grade: A
hypothesis_family: es-trend-span-cost-optimal
status: backlog
created: 2026-07-30
---

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

- **Basket:** ES first (`ESH4` for the proven-transcoded window), then GC and
  RTY, which have the full 2010→2026 transcode and are genuinely different
  regimes.
- **Timeframe:** `1m` (the only grain we can replay; see Honesty note).
- **Features:** `ema(fast)`, `ema(slow)`, crossover in both directions.

Runnable today, as `configs/` material:

```toml
schema_version = 1

[meta]
name = "es-trend-span-cost-optimal"
hypothesis_family = "es-trend-span-cost-optimal"
economic_rationale = "Trend-following pays when low-frequency autocorrelation is positive; net Sharpe is hump-shaped in filter span and the optimum lengthens as costs rise (Sepp & Lucic 2026)."

[universe]
instruments = ["ESH4"]
timeframes = ["1m"]

[data]
source = "curated"
start = "2024-01-01"
end = "2024-02-01"

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

[funnel]
stages = ["s0", "s1", "s2", "s3"]
cost_sensitivity_ticks = [0.0, 0.5, 1.0, 2.0]
min_oos_sharpe_after_costs = 0.5
max_pbo = 0.5
require_plateau = true
kill_if_dead_at_ticks = 1.0

[run]
seed = 42
initial_cash_usd = "100000"
qty_contracts = 1
```

Every construct above exists today: `ema` with an integer `{start, end, step}`
axis, `crosses_above`/`crosses_below`, `spread_cross`, and the mandatory cost
sweep. Nothing here needs new Rust.

## Data

**Owned and sufficient for the structural test:** curated `1m` bars. ESH4's
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
  `max_pbo = 0.5`, `require_plateau = true`.
- Sample minimum before *any* profitability verdict: **200 round trips** and
  **at least 250 trading sessions pooled across contracts**. A single-contract
  ES run reaches neither, so a single-contract run is explicitly a **triage
  run with no verdict attached** — `Kill`/`Graduate` may not be issued from it.

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
  We can only replay 1s and 1m (no resampler; `ohlcv-1d` was deliberately not
  bought). A 480-period EMA on 1-minute bars is an eight-hour span, not an
  eight-week one. The *structural* predictions (hump-shaped net Sharpe,
  cost-optimal span, positive skew) are stated in a form that does not depend on
  the grain, which is why this is testable today — but **the paper's
  calibration is not being tested**, and no result here may be described as
  confirming or refuting their empirical findings.
- **Expect it to lose money.** ES at 1-minute with a one-tick half-spread is a
  hostile cost environment for a crossover, and the project's own reference run
  (SMA 20/50 on ESH4, January 2024) lost 23.51 % under exactly this fill model.
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
