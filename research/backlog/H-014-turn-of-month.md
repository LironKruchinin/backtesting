---
id: H-014
slug: turn-of-month
topic: calendar
grade: B
hypothesis_family: equity-index-turn-of-month
status: backlog
created: 2026-07-30
---

# H-014 — The turn-of-the-month effect in stock index futures

## Citation

Oscar Carchano, Ángel Pardo Tornero, **"Calendar Anomalies in Stock Index
Futures"**.

- SSRN: <https://dx.doi.org/10.2139/ssrn.1958587>

**Retrieval caveat, recorded because it matters:** the SSRN page returned
HTTP 403 when fetched during this sweep, so the abstract below is assembled from
secondary descriptions rather than read from the source. The described study
examines **188 possible cyclical anomalies** — day-of-week, month-of-year,
weekday-of-month, week-of-month, semi-month, turn-of-month, end-of-year,
holidays, Friday-the-13th, Halloween, quarterly expiration and others — across
**S&P 500, DAX and Nikkei index futures from 1991 to 2008**, using percentile-t
bootstrap and Monte Carlo methods, and reports that the **turn-of-the-month
effect in S&P 500 futures is the only calendar effect that is statistically and
economically significant and persistent over time**.

**Anyone taking this ticket must read the paper directly and correct this file
before running anything.** The specifics above — the count, the sample window,
the method — are exactly the details that get garbled in secondary retelling,
and they are also exactly the details this hypothesis's multiple-testing
accounting depends on.

Background on the underlying equity effect: Wei Xu, John J. McConnell,
*"Equity Returns at the Turn of the Month"* —
<https://www.chesler.us/resources/academia/turn_of_the_month_stock_returns.pdf>

## Mechanism

Money arrives on a schedule. Salaries are paid at month end, retirement
contributions settle then, and funds that must remain fully invested put that
cash to work within days of receiving it. Index funds rebalance to month-end
weights, and a large share of institutional performance measurement is
month-end-dated, which gives managers a reason to be positioned before the
window closes rather than after. None of this flow is a forecast — it is
calendar-driven, price-insensitive, and it recurs every month whether the market
is cheap or expensive. The losing side is whoever supplies liquidity into that
predictable buying, and they are compensated for it; the payer is the
**mandated, schedule-driven allocator**, who keeps paying because their
obligation is to be invested, not to be clever about the day.

This is one of the better mechanism stories available: the payer is named, the
obligation is documented, and the flow is not discretionary. It is also, for
exactly that reason, one of the most widely known, and anything a hundred
thousand traders can put in a calendar reminder is a candidate for having been
arbitraged.

## Signal in Crucible terms

- **Basket:** ES primary. NQ and RTY as the rhyme check — and here they are a
  genuine test, because the described study found the effect in S&P futures
  specifically, so a US-equity-wide flow story predicts it should appear in all
  three.
- **Timeframe:** daily bars.
- **Feature:** trading-day-of-month index, counted from the **end** of the month
  (the conventional definition places the window around the last trading day and
  the first few of the next month). The count must be over **CME trading days**,
  not calendar days — a rule stated in calendar days lands on a Saturday four
  times a year, and this project has already ruled that fold windows are
  measured in trading days for the same reason (D-0062).
- **Rule:** long ES from the close of the last trading day of the month through
  the close of trading day *k* of the new month; flat otherwise. `k` is a
  pre-registered small integer, not a swept parameter (see kill criteria).

The signal itself is the simplest in this backlog: a position determined
entirely by the calendar, with no price input at all. That is a virtue — it has
almost no capacity to overfit *within* a run — and it concentrates the entire
risk into the choice of window, which is why the window is frozen up front.

## Data

**Owned, sufficient, and well-matched:** `ohlcv-1m` for ES, NQ, RTY,
2010-06-06 → 2026-07-28. Roughly **193 month-turns** per instrument. The
`crucible-data::calendar` module already models CME sessions, holidays and early
closes with sources cited (D-0039/D-0040), which is exactly the machinery a
trading-day-of-month count needs — and it already knows that most CME
"holidays" are early closes rather than closures, which a naive implementation
would get wrong.

**Missing — all code:**
1. **A daily grain.** No resampler (see `research/backlog/README.md` §2.2).
   Alternatively the rule can be expressed on 1-minute bars with a session-close
   predicate, which needs the same session anchors as H-001.
2. **A calendar predicate in the rule layer.** The combo grammar has no
   day-of-month operand, and `crucible-engine` may not depend on
   `crucible-data`. The D-0071 pattern is the precedent and the answer: the CLI
   computes trading-day keys **once** and hands the same `&[i64]` slice to every
   consumer, so two components cannot disagree about which day it is.
3. **A multi-year replay**, which for ES means either the D-0042 consumer or
   pooling across quarterly contracts. With ~193 month-turns spread over ~64
   contracts, pooling is unavoidable here — a single contract contains three.

No purchase required.

## Pre-registered kill criteria

The defining risk of this hypothesis is **multiple testing**, and the criteria
are built around it rather than around returns.

- **The window is frozen now.** The pre-registered rule is: enter at the close
  of the **last trading day of the month**, exit at the close of the **third
  trading day** of the following month. That is one window, chosen before any
  run, matching the conventional definition. Any other window is a **separate
  declared trial** under this family, and the deflated Sharpe reads the count
  from the registry (CLAUDE.md §4). Sweeping the window and reporting the best
  is the exact failure the source study's own methodology was designed to
  expose, and doing it here would be indefensible.
- **The trial count starts at 188, not at 1.** The effect we are testing is the
  survivor of a large search conducted by someone else. Our deflation must
  account for the selection that produced the hypothesis, not merely for the
  runs we perform. This is recorded here so the registry entry is created with
  the right prior and nobody later reads "one trial, significant" off our own
  run count.
- **Sample minimum:** **150 month-turns** per instrument. ES gives ~193, so this
  binds and leaves little slack.
- **Gate 0 — predictor before system:** mean return over the pre-registered
  window must exceed the mean return over all other periods, block-bootstrap
  95 % CI on the difference excluding zero (block = 1 month, since observations
  are monthly and non-overlapping). Otherwise **Kill**.
- **Gate 1 — costs:** `kill_if_dead_at_ticks = 1.0`,
  `min_oos_sharpe_after_costs = 0.5`. Twelve round trips a year on ES is the
  lowest turnover in this backlog, so costs should be nearly irrelevant; if the
  effect dies at one tick it was never economically real.
- **Gate 2 — cross-instrument:** the flow story is US-equity-wide, so the effect
  must appear in **at least 2 of 3** of ES/NQ/RTY. ES alone is a **Kill**: a
  month-end flow that reaches large caps and not small caps contradicts the
  mechanism, and a mechanism-free calendar regularity in one series over 193
  observations is what data mining looks like.
- **Gate 3 — the decay split, pre-registered.** The source sample ends in 2008;
  ours begins in 2010. Split our sample at 2018 and report both halves. If the
  effect lives only in 2010–2017, the verdict is **Kill** with
  "published-anomaly decay" recorded.
- **Gate 4 — the placebo calendar, mandatory.** Run the identical rule on **all
  other trading-day-of-month positions** and plot the distribution of results.
  The pre-registered window must sit in the tail of that distribution, not
  merely be positive. If a randomly chosen day-of-month window looks about as
  good, we have measured noise with a calendar attached. This is the negative
  control, and per CLAUDE.md §7 a detector nobody has seen fire is decoration.

## Honesty note

- **The primary source was not read directly.** SSRN returned 403 during this
  sweep and the description here is secondhand. This is flagged in the citation
  section as well because it is the kind of caveat that quietly evaporates
  between a backlog file and a report.
- **This is a survivor of 188 tests, and that is the single most important fact
  about it.** Testing 188 hypotheses at the 5 % level yields roughly nine
  spurious "significant" results by construction. The described study used
  bootstrap and Monte Carlo methods specifically to handle that, which is to its
  credit — but our *prior* on the surviving effect must still be much weaker
  than for an effect that was the only thing anyone looked at. Gate 4 exists to
  re-run that selection on our own data rather than inheriting theirs.
- **Sample overlap: none.** Their window is 1991–2008; ours is 2010–2026. Our
  entire sample is post-publication, which makes this a clean out-of-sample
  test — the best overlap situation in this backlog — and simultaneously means a
  positive result would be surprising.
- **193 month-turns is a small sample and cannot be enlarged.** Sixteen years is
  sixteen years; there is no finer grain that produces more month-ends. Whatever
  the answer is, its confidence interval will be wide, and the effective-N gate
  should be treated as binding rather than advisory.
- **Regime confound:** our sample is dominated by a period of extraordinary
  central-bank support and passive-fund inflows, which is a mechanism-consistent
  *tailwind* for month-end allocation flow. A positive result here may be a
  statement about 2010–2026 fund flows rather than about calendars.
- **The effect is trivially easy to trade and universally known.** Every
  retail-facing site lists it. If it survived our gates, the first question
  should be why.

## Triage grade

**B.** All data owned, the sample is fully out-of-sample relative to the source,
and the session calendar this needs is already built and already knows the awkward
parts of the CME schedule. The gaps are a daily grain (or a session-close
predicate), a calendar operand supplied caller-side in the D-0071 pattern, and
multi-contract pooling. It is the cheapest B in this sweep and has the most
clearly named payer.
