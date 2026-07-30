---
id: H-005
slug: intraday-periodicity
topic: intraday-session
grade: C
hypothesis_family: es-intraday-periodicity
status: backlog
created: 2026-07-30
---

# H-005 — Half-hourly return periodicity at exact multiples of a trading day

## Citation

Steven L. Heston, Robert A. Korajczyk, Ronnie Sadka, **"Intraday Patterns in
the Cross-section of Stock Returns"**, *The Journal of Finance* 65(4), 2010.

- Publisher: <https://onlinelibrary.wiley.com/doi/abs/10.1111/j.1540-6261.2010.01573.x>
- Preprint: <https://arxiv.org/abs/1005.3535>
- Working paper: <https://www.bauer.uh.edu/departments/finance/documents/Heston_Korajczyk_Sadka_paper_UH.pdf>
- Internet appendix: <https://afajof.org/wp-content/uploads/files/supplements/Internet_Appendix_to_Intrada.pdf>

Their stated claim: examining intraday predictability in the **cross-section**
of stock returns, they find a striking pattern of return continuation at
half-hour intervals that are **exact multiples of a trading day**, an effect
lasting at least 40 trading days. Volume, order imbalance, volatility and
bid-ask spreads show similar periodicity but do not explain the return pattern.
They separately report that short-term reversal is driven by temporary
liquidity imbalances lasting under an hour and by bid-ask bounce, and that
timing trades can reduce execution costs by roughly the effective spread.

## Mechanism

The proposed cause is *institutional trading habit*, not information. A large
order is worked over days, and the desk or algorithm working it tends to
execute at the same time of day each day — because the trader arrives at the
same hour, because a VWAP schedule is anchored to the session, or because a
rebalancing rule fires at a fixed clock time. That repetition leaves a
footprint with a period of exactly one trading day: whatever pressure existed
in the 10:30 bucket yesterday is partly present in the 10:30 bucket today,
because it is the same parent order still being worked. The losing side is the
institution executing on a predictable schedule — it is paying for the
convenience of a fixed timetable, and it keeps paying because the alternative
(randomizing execution times) costs the desk operational complexity and makes
its own benchmark harder to hit. Note the mechanism's own limit: it predicts
*continuation of order flow*, which decays as the parent order completes, and
it says nothing about the direction being correct.

## Signal in Crucible terms

The faithful test is not available to us, and the file is graded on that fact.
Recorded here so nobody re-derives it later:

- **Faithful version:** sort a broad cross-section of instruments on their
  return in half-hour bucket `k` on day `t−1`, `t−2`, … and measure the
  cross-sectional return spread in bucket `k` on day `t`. This is a
  cross-sectional sort, and its power comes from having hundreds or thousands
  of names.
- **Available weaker analogue** (a *descendant*, not this hypothesis): on one
  instrument, test whether the return in half-hour bucket `k` autocorrelates
  with bucket `k` on prior days. That is a test of **intraday seasonality in a
  single series**, which is a different phenomenon and is substantially
  contaminated by the well-known U-shaped intraday volume and volatility
  profile. If someone wants it, it gets its own file and its own family key.

## Data

**Owned:** `ohlcv-1m` for seven futures parents. That is a cross-section of
**seven**, of which three (ES, NQ, RTY) are highly correlated equity indices —
the `docs/DATA_PLAN.md` basket exists precisely so that "it rhymes across
instruments" cannot mean "it rhymes across three tickers that are 94 % the same
trade". Seven correlated futures is not a cross-section in the sense this paper
requires; the test has essentially no cross-sectional power.

**Missing:**
1. **A cross-section.** This needs equity single names — hundreds of them, at
   intraday grain. We own none, we have deliberately declined an equities
   subscription (`docs/DATA_PLAN.md`, "Do not buy"), and the only equity data
   the project ever wants is a SPY+QQQ sanity micro-pull. Acquiring an intraday
   cross-section of US equities is a purchase nobody has proposed and no
   milestone consumes.
2. **Multi-instrument portfolio accounting** — explicitly post-M4
   (`docs/MILESTONES.md`).
3. **A 30-minute grain** (resampler), and time-of-day bucketing.

## Pre-registered kill criteria

Written for the faithful version, so they are ready if the data situation ever
changes. **No run is authorized under this key until the cross-section exists.**

- **Sample minimum:** at least **200 instruments** with continuous intraday
  coverage, and at least **1,000 trading days**. Below either, no verdict.
- **Existence:** the day-lagged same-bucket cross-sectional return spread must
  be positive with a block-bootstrap 95 % CI excluding zero at lags 1 through 5
  trading days. Failure at lag 1 → **Kill**.
- **The periodicity must be specific, not generic.** The effect at exact
  multiples of one trading day must exceed the effect at neighbouring
  half-hour offsets (± 1 bucket) at the **5 %** level. If bucket `k` predicts
  bucket `k±1` just as well, the finding is intraday autocorrelation, not
  day-multiple periodicity, and this hypothesis is **Killed** even though
  something was found — the mechanism is what is on trial.
- **Costs:** `kill_if_dead_at_ticks = 1.0`. The paper's own framing is that
  the effect is roughly the size of the effective spread; a result that does
  not survive one tick is the null.
- **`max_pbo = 0.5`, `require_plateau = true`** over the lookback in days.

## Honesty note

- **This is the clearest genuine data gap in the sweep.** Everything else here
  is blocked by code we intend to write. This one is blocked by a cross-section
  of equities we have decided not to buy, for good reasons that have not
  changed. Grading it A or B by quietly substituting the single-instrument
  analogue would be exactly the inflation this backlog's rules forbid.
- **Their data is US equities, 2001–2005 in the published sample; ours would be
  futures, 2010–2026.** No overlap at all in instruments and almost none in
  time.
- **The single-instrument analogue is not a weaker version of this result — it
  is a different question**, and one with a strong confound: intraday volume
  and volatility have a pronounced U-shape (documented in the same literature,
  and visible in our own archive), so same-bucket autocorrelation will appear
  in a single series whether or not any order-splitting footprint exists.
- **The mechanism has a natural half-life.** Parent orders complete. Even if
  real, the effect is a statement about a decaying footprint, and the paper's
  "at least 40 trading days" is an empirical duration from one sample, not a
  constant of nature.
- **Age and decay:** published 2010, based on early-2000s data, in the era
  before execution algorithms routinely randomized their schedules. The
  mechanism describes a habit that the intervening fifteen years of execution
  research has been explicitly trying to break.

## Triage grade

**C.** Needs data we do not own and have deliberately declined, plus
cross-sectional machinery that is post-M4. It is in the backlog because the
mechanism is well-identified and the paper is a good one — not because it is
close to runnable. If Crucible ever grows a cross-section, this is the first
thing to test on it.
