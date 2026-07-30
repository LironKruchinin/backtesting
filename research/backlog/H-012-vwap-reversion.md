---
id: H-012
slug: vwap-reversion
topic: volume-structure
grade: B
hypothesis_family: es-vwap-reversion
status: backlog
created: 2026-07-30
---

# H-012 — VWAP reversion

## Citation

**There is no peer-reviewed citation for the predictive claim.** That is the
most important line in this file and it is not an oversight — it is the finding.

A systematic sweep of arXiv q-fin for `VWAP` returns roughly thirty papers, and
**every one of them treats VWAP as an execution benchmark or a pricing object,
not as a predictor of returns.** Representative:

- Enzo Busseti, Stephen Boyd, *"Volume Weighted Average Price Optimal
  Execution"*, arXiv:1509.08503 — minimizing tracking error to VWAP.
- Takashi Kato, *"VWAP Execution as an Optimal Strategy"*, arXiv:1408.6118 —
  conditions under which VWAP execution is optimal for a risk-neutral trader.
- Olivier Guéant, Guillaume Royer, *"VWAP execution and guaranteed VWAP"*,
  arXiv:1306.2832 — pricing guaranteed-VWAP contracts.
- Alexander Barzykin, Fabrizio Lillo, *"Optimal VWAP execution under transient
  price impact"*, arXiv:1901.02327.
- Jin Hyuk Choi, Kasper Larsen, Duane J. Seppi, *"Equilibrium Effects of
  Intraday Order-Splitting Benchmarks"*, arXiv:1803.08336 — and this one cuts
  *against* the folk story: it argues TWAP and VWAP benchmarking **reduce
  market liquidity and increase volatility** relative to terminal-only targets.

The "price reverts to VWAP" claim, by contrast, appears in trading-education
sites, broker blogs, and indicator-vendor marketing. Those sources make specific
numerical claims about hit rates and edge. **Those numbers are not restated
anywhere in this file**, because they are unverifiable, unattributable to any
method, and repeating them would launder marketing copy into a research
document.

## Mechanism

The folk mechanism is a real observation with an unjustified conclusion bolted
on. The real part: institutional execution algorithms are benchmarked against
VWAP, so a desk working a large buy order is rewarded for filling below VWAP and
penalized for filling above it. That gives a genuinely large population of
traders a *preference* to buy under VWAP and sell over it, and the Choi–Larsen–
Seppi paper above confirms these benchmarks change equilibrium behaviour.

The unjustified part is the leap to "therefore price is pulled back to VWAP".
A benchmark-following algorithm is **schedule-driven, not price-driven**: a VWAP
algo buys a fixed share of each interval's volume whether price is above or
below the line, because that is how it tracks the benchmark. It does not step in
to defend the level. And the desks trading *against* it are not obliged to lose.
So while there is a named population whose behaviour is shaped by VWAP, there is
**no argument that they are systematically paying** — which is the question this
backlog requires an answer to. The honest statement is: mechanism identified,
loser not identified.

There is also a mechanical trap. VWAP is a **cumulative** average anchored at
the session open. Early in the session it sits almost on top of price and is
extremely noisy; late in the session it is heavily weighted by hours of old
trading and barely moves. "Distance from VWAP" therefore means something
completely different at 09:35 than at 15:55, and any unconditional test pools
two different variables.

## Signal in Crucible terms

- **Basket:** ES first; CL second, since its session shape differs enough to
  matter.
- **Timeframe:** `1m` bars, which carry `volume: u64` — enough to build a
  session-anchored VWAP.
- **Feature 1 — session VWAP:** running Σ(typical price × volume) ÷ Σ(volume),
  reset at each session open. Using the 1-minute typical price (H+L+C)/3 is an
  approximation of true volume-at-price, but VWAP is a first moment and is far
  less sensitive to intrabar distribution than a histogram is (contrast H-013,
  where the same approximation is fatal).
- **Feature 2 — deviation bands:** a rolling standard deviation of price around
  session VWAP.
- **Rule:** fade a close beyond the band, exit at VWAP.
- **Mandatory conditioner:** session progress. The test must be run in buckets
  by time-since-open, because of the mechanical trap above. An unconditional
  version is not an acceptable primary result.

## Data

**Owned and sufficient:** `ohlcv-1m` for all seven parents, 2010-06-06 →
2026-07-28, including per-bar volume.

Better data exists for one instrument: `trades` and `tbbo` for **ES only,
2025-07-28 → 2026-07-28**. That one year permits a check of how far the
1-minute-approximated VWAP drifts from a true trade-weighted VWAP — worth doing
once, as a validation of the approximation, before sixteen years of results are
built on it.

**Missing — all code:**
1. A **session-anchored cumulative indicator**, which is a new shape: our three
   indicators are all fixed-window rolling, and this one resets on a session
   boundary.
2. **Volume as a rule operand.**
3. **Session anchors** (shared with H-001, H-002, H-004).
4. **Session-progress bucketing** for the conditioning requirement.

## Pre-registered kill criteria

Given the evidentiary situation, the bar is set high and the predictor gate is
where this should live or die.

- **Gate 0 — predictor before system, bucketed by session progress.** Split the
  session into four equal buckets. Conditional on a close beyond the band,
  measure the mean forward return to VWAP over the next 5, 15 and 30 minutes,
  **within each bucket**.
  - The reversion must be present with a block-bootstrap 95 % CI excluding zero
    in **at least 3 of the 4 buckets**. Present in only one — most likely the
    first, where VWAP hugs price and the "reversion" is arithmetic rather than
    economic — is a **Kill**.
- **Gate 0b — the spread test.** Mean reversion per round trip must exceed
  **one full tick** (0.25 ES points, $12.50). Below that it is real and
  untradeable, and the file records "inside the spread". Same rule as H-008 and
  for the same reason.
- **Gate 0c — the anchor control, which is the decisive one.** Run the identical
  strategy against a **deliberately wrong anchor**: a VWAP reset at a random
  time of day, and a plain rolling VWAP with no session reset at all. If the
  session-anchored version does **not** significantly outperform both, then
  nothing about *VWAP specifically* is doing the work — we have rediscovered
  generic mean reversion (H-008) under a more complicated name, and this
  hypothesis is **Killed** while H-008 keeps the finding. This control is
  mandatory and non-negotiable: it is the only thing separating this file from
  a relabelling of H-008.
- **Gate 1 — S1 `free_fills`:** profitable costless, or **Kill**.
- **Gate 2 — S2:** `min_oos_sharpe_after_costs = 0.5`,
  `kill_if_dead_at_ticks = 1.0`. Sample minimum **200 round trips** and **250
  sessions** pooled across contracts.
- **Gate 3 — S3:** `max_pbo = 0.5`, `require_plateau = true` over the band width
  and lookback.

## Honesty note

- **The grade and the credibility point in opposite directions, deliberately.**
  This is graded **B** because the grade measures *cost to test* and the cost is
  a few pieces of code on data we own. It is not graded B because the idea is
  well-supported. It is poorly supported, and a reader who takes only the grade
  away from this file has misread it.
- **The absence of academic evidence is weak evidence of absence, and should be
  weighted accordingly.** Execution desks with a genuine short-horizon edge do
  not publish it, and the academic literature's silence on VWAP-as-predictor may
  reflect that nobody with the data has an incentive to write it up. But it may
  equally reflect that people have looked and found nothing. We cannot tell
  which, and the honest position is that this idea enters the queue with **no
  external support at all** rather than with support we have chosen not to
  count.
- **The one relevant peer-reviewed result points the other way.** Choi, Larsen
  and Seppi find that VWAP benchmarking *reduces* liquidity and *increases*
  volatility in equilibrium. That is not a refutation of reversion, but it is
  the only refereed finding in the vicinity and it does not help the folk story.
- **The vendor numbers are excluded on purpose.** Several sources encountered
  while researching this file quote specific edges in percentage points and
  claim statistical significance. None states a sample, a cost assumption, or a
  method. Under this directory's binding rule they do not appear here, and they
  must not appear in any config's `economic_rationale` either.
- **Our data can build VWAP but not perfectly.** True VWAP is trade-weighted;
  ours would be 1-minute-bar-weighted for fifteen of sixteen years. The one year
  of ES `trades` is the only period where the approximation can be checked.
- **This is a case where the honest triage outcome may be "do the anchor control
  first and nothing else".** Gate 0c costs almost nothing once the indicator
  exists and can retire the entire hypothesis in one run.

## Triage grade

**B.** New indicator code (session-anchored cumulative VWAP), a volume operand,
and session anchors — on data we already own. Cheap to test, and it should be
tested only *after* H-008, because if generic mean reversion already dies on
costs then VWAP reversion is dead too and Gate 0c has nothing to separate.

---

## Changelog

Append-only. The registration above is never rewritten — a pre-registration
that gets edited after the fact is not one (README §1).

### 2026-07-30 — re-graded against the four grammar unlocks (D-0077…D-0080): **B → B**

**What closed — the mandatory conditioner.** This file registers session
progress as non-optional ("an unconditional version is not an acceptable
primary result"), and that bucketing is now writable (D-0078):

```toml
enter_long = "minutes_since_rth_open > 60 and minutes_since_rth_open <= 120 and <fade condition>"
```

**What still blocks — the feature itself.** Session-anchored cumulative VWAP is
`Σ(typical price × volume) ÷ Σ(volume)`, **reset at each session open**. Three
separate constructs are missing:

1. the typical price `(H+L+C)/3` — arithmetic between operands;
2. two cumulative accumulators that **reset on a session boundary** — every
   indicator in the grammar is a fixed-length trailing window, and none is
   session-anchored;
3. the division of one by the other.

`zscore` and `stdev` (D-0080) do not substitute: a trailing z-score of close is
a deviation from a *rolling* mean, and this hypothesis is deviation from a
*session-anchored volume-weighted* mean. They are different features with
different reset behaviour, and swapping them would be a different hypothesis
under this file's family key.

The volume operand landing (D-0079) does not help either — VWAP needs volume
*accumulated and weighted*, not compared.
