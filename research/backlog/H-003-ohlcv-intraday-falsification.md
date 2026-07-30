---
id: H-003
slug: ohlcv-intraday-falsification
topic: intraday-session
grade: B
hypothesis_family: nq-ohlcv-intraday-falsification
status: backlog
created: 2026-07-30
---

# H-003 — Replicating a negative result: OHLCV intraday signals do not clear costs

## Citation

Mathias Mesfin, **"Structural Limits of OHLCV-Based Intraday Signals in MNQ
Futures: A Systematic Falsification Study"**, arXiv:2605.04004 (submitted
2026-05-05, revised 2026-07-13).

- <https://arxiv.org/abs/2605.04004>

Their stated claim, quoted from the abstract: fourteen signal families were
tested on 947 trading days of five-minute Micro E-mini Nasdaq 100 (MNQ) data
from 2021–2025, each evaluated on out-of-sample walk-forward validation, a
minimum t-statistic of 2.0, at least 30 trades, positive net return after a
fixed two-point round-trip friction cost, and consistent performance across
years. **None of the tested strategies satisfied all of these requirements.**
The authors report gross returns before costs ranging from roughly 0.07 to 1.50
points per trade against the assumed two-point friction, one family
(gap-continuation short) that cleared the t-statistic but had only 22 trades,
and two separately validated signals presented explicitly as **positive
controls** confirming the methodology can detect edge when it exists. The
authors state the primary contribution is methodological rather than
predictive.

## Mechanism

There is no profit mechanism here, and that is the point of the file. The
mechanism being tested is the **null**: that the information in five-minute
OHLCV bars about the next few bars' direction is smaller than the cost of
acting on it, because everything that can be read off a public bar has already
been read off it by participants whose costs are lower than ours. Nobody is on
the losing side, which is exactly why nobody is on the winning side either. The
economic content of the claim is about the *size of the friction relative to
the size of the signal*, and that ratio is the single most common reason a
backtest that looked good in gross terms is worthless in net terms.

This file exists because Crucible's stated purpose is verdicts, and a
correctness project needs a published negative result it can try to reproduce.
It is a **control on the engine as much as a test of the signals**.

## Signal in Crucible terms

- **Basket:** `NQ` (we own the full-size E-mini parent, not the micro — see
  the honesty note). `ES` as a second instrument.
- **Timeframe:** 5-minute, which we cannot currently produce (see Data).
- **Signals:** the fourteen families would have to be enumerated from the paper
  itself and each pre-registered *by name* before any run. This file does not
  restate them as though they were faithfully transcribed — reading the paper
  and pinning the fourteen definitions is the first task of anyone who takes
  this ticket, and it must land in the config before the first replay.
- **Execution:** the paper's two-point round-trip friction, translated for our
  contract. On NQ, one point is $20 and the tick is 0.25 points; two index
  points round-trip is $40. Under `spread_cross` this is expressed as
  `half_spread_ticks` plus `fee_per_contract_usd`, and the cost sweep
  (0/0.5/1/2 ticks, mandatory per CLAUDE.md §2.4) covers the rest.

**The interesting outcomes are asymmetric, and both are pre-registered:**

1. **Crucible also finds nothing.** Then we have an independent reproduction of
   a published negative result under a stricter engine, and a citable statement
   that our cost model is not flattering us.
2. **Crucible finds an edge where they found none.** Per CLAUDE.md §7, this is
   an **engine-bug alarm first and a discovery second**. The response is to run
   the truncation-invariance and permutation harnesses (M3) against it before
   anyone writes down a number. A strategy that beats a published falsification
   study is far more likely to be lookahead than alpha.

## Data

**Owned:** `ohlcv-1m` for NQ and ES, 2010-06-06 → 2026-07-28 — a span more than
three times the paper's 2021–2025 window, at a finer grain.

**Missing:**
1. **A 5-minute resampler.** This is the binding gap and it is structural: we
   bought `ohlcv-1m` and `ohlcv-1s` only, and `transcode` maps vendor schemas
   to timeframes one-to-one. `TimeFrame::M5` exists but is deliberately
   unmapped rather than silently aliased (`crucible-data::transcode`). Building
   a 1m → 5m aggregator is small, well-defined, and unlocks a large fraction of
   this backlog.
2. **Time-of-day predicates**, for any family defined against the RTH or London
   session (the paper's positive controls are both session-scoped).
3. **Volume as a rule operand**, for the families that use it.

No purchase required.

## Pre-registered kill criteria

This hypothesis inverts the usual polarity: the *expected* outcome is that
everything dies, so the criteria are written to make a **survival** hard and to
make the reproduction itself the deliverable.

- **Adopt the paper's own bar, unchanged**, so the comparison is meaningful:
  t-statistic ≥ **2.0**, at least **30 trades**, positive net return after the
  two-point round-trip friction, and consistent sign across years.
- **Add Crucible's bar on top:** `min_oos_sharpe_after_costs = 0.5`,
  `kill_if_dead_at_ticks = 1.0`, `max_pbo = 0.5`, `require_plateau = true`.
- **Trial counting is the whole point of the family key.** Fourteen signal
  families across a parameter grid is a large number of trials, and every one
  of them charges `nq-ohlcv-intraday-falsification`. The deflated Sharpe must
  read the count from the registry, never from memory (CLAUDE.md §4). Any
  family that survives the raw bar but not the deflation is a **Kill**, and the
  file records "died to the trial count" — which is the honest description of
  most published intraday results.
- **Any survivor is quarantined, not celebrated:** it does not graduate until
  the M3 permutation and truncation-invariance harnesses have been run against
  it and both have passed. Until then its verdict is `Iterate`, never
  `Graduate`.

## Honesty note

- **We do not own MNQ; we own NQ.** They are different contracts on the same
  index: MNQ is one-tenth the notional ($2/point vs $20/point) with the same
  0.25 tick. A friction stated in *index points* transfers between them, which
  is why the comparison is legitimate. But the order books are not the same
  depth, and the paper's two-point friction assumption was calibrated to the
  micro. Any claim about "the same result on NQ" needs that difference stated
  every time it is made.
- **Sample overlap is real but small.** Their 2021–2025 sits inside our
  2010–2026. If we run the full span, roughly a quarter of our sample is their
  sample. The pre-registered primary test should be run on the **non-overlapping
  portion** (2010–2020 and 2026) and the overlap reported separately, or the
  reproduction is partly circular.
- **This is a single-author arXiv preprint, not peer-reviewed.** Its value here
  is that it publishes a null with an explicit, checkable methodology and
  declares its positive controls — which is more methodological honesty than
  most refereed strategy papers manage. But the fourteen families are that
  author's choices, and "fourteen families found nothing" is not "OHLCV
  contains nothing".
- **The positive controls are not reproducible from the abstract**, which
  reports them without their definitions. If the paper's body does not define
  them precisely enough to re-implement, our version has no positive control,
  and a falsification study without one is decoration (CLAUDE.md §7). In that
  case the honest move is to build our own planted-signal control instead, and
  say that is what we did.
- **A negative result costs the same compute as a positive one** and is, for
  this project, worth more: a rigorous kill is a good result
  (`docs/PROJECT_PLAN.md` §M4 exit).

## Triage grade

**B.** The data is owned and the span exceeds the paper's. The gaps are a 1m→5m
resampler, a time-of-day predicate, and a volume operand — three pieces of
code, all in M2/M3 scope, none of them requiring a purchase.
