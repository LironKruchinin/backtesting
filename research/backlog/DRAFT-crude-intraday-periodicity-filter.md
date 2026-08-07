---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: crude-intraday-periodicity-filter
topic: intraday-seasonality
grade: C
hypothesis_family: cl-intraday-periodicity-filtered-volatility
status: draft
blocked_on: an intraday-periodicity filter (flexible Fourier form or cubic spline) as an indicator — no milestone builds one, and a full-sample fit would be lookahead besides
created: 2026-08-06
doi: 10.1177/0972652716686207
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Stripping the time-of-day shape out of crude volatility before measuring it

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

B.B. Chakrabarti, Vivek Rajvanshi. *Intraday Periodicity and Volatility Forecasting: Evidence from Indian Crude Oil Futures Market*.
Journal of Emerging Market Finance, 2017.
DOI `10.1177/0972652716686207`. <https://doi.org/10.1177/0972652716686207>
Retrieved from the crossref API on 2026-08-06.

The authors fit the repeating within-session pattern in crude oil futures volatility two ways — a smooth trigonometric form and a spline — using several years of intraday data from an Indian commodity exchange. Once that recurring shape is removed, what remains shows long-memory persistence, and variance forecasts built on the de-shaped series do better. They report the result holds after accounting for scheduled macro releases.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1177/0972652716686207':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A recurring time-of-day shape in volatility is a calendar fact, not an information fact: the open and the settlement window are busy every session whether or not anything new is known. An unfiltered volatility reading therefore fires mostly on the clock, so any rule keyed to it is making the same time-of-day bet each day while pretending to be a state variable. Removing the shape is supposed to leave the part that reflects genuine news arrival, and only that part could plausibly be worth conditioning on. Who is on the losing side? Nobody that can be named, and this file should say so flatly. The paper states no trading rule and identifies no counterparty; it improves a variance forecast, and a variance forecast in a futures-only account is not a position. If a filtered state later gates a directional rule, that rule has to name its own payer, because it inherits none from here.

## Signal in Crucible terms

- Faithful construction: CLZ2024 (and siblings) at 5m or 15m, with a periodicity-filtered volatility slot standing in for `stdev(period, return)`.
- The filter is the missing piece: it needs a fit over time-of-day buckets, updated bar by bar from data available at decision time. Every indicator in this build is a trailing window with no bucketed component.
- A full-sample trigonometric or spline fit is exactly the §2.1 violation the project exists to prevent, so the descendant would need an expanding-window estimator plus a declared warmup contribution to `Grid::max_warmup_bars`.
- Weaker expressible analogue: `[indicators.vol] kind = "stdev", source = "return"` compared against an enumerated constant, gated on `minutes_since_rth_open` to hold time of day roughly fixed. That is a crude control for the shape, not a filter, and it tests a different claim.
- Where it breaks first: the paper's object is a variance series, and the funnel scores position rules, so even with the indicator there is no registered path that scores the filter's own contribution.

## Data

- Owned: CL `ohlcv-1m` 2010-06-06 → 2026-07-28, curated with four-digit contract keys, resampled on read to 5m/15m/1h/1d on the exchange's own sessions.
- Owned: an energy session calendar with eras (D-0089), which is what makes a time-of-day bucket well defined at all — though 26 dates in it are knowingly wrong by 45 minutes.
- Not owned: any Multi Commodity Exchange India data, and none is planned. Their session structure, holiday calendar and participant mix are not ours.
- Not owned: a macro-announcement calendar. Their robustness check against scheduled releases cannot be reproduced here at all — that is an M4 static CSV that does not exist yet.
- Cost inputs rest on `half_spread_ticks = 1`, an assumption rather than a measurement (D-0120), and CL has no `tbbo` so it will stay one.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: an intraday shape is a statement about the average session, and estimating a bucketed profile plus testing it needs at least two years of sessions.
- `min_oos_trades = 200` — basis: a time-of-day conditioner that fires less often than this has not been tested, it has been sampled.
- The filtered state must beat the unfiltered `stdev` state on the same window and the same rule core; if it does not, the filter bought nothing and the idea is Killed even though a gate may still look profitable — the filter is what is on trial.
- `kill_if_dead_at_ticks = 1.0` — basis: intraday volatility gates fire near session edges where the real spread is widest, so an edge that needs a zero-cost book was never there.
- `min_oos_sharpe_after_costs = 0.3` — basis: deliberately below the shipped configs' 0.5, because this is a conditioner rather than a strategy and the question is whether it adds anything measurable.
- `max_permutation_p = 0.05` and `require_plateau = true` over the filter's lookback — basis: a periodicity result that exists at one bucket width and nowhere near it is a spike, not a shape.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their market is Indian MCX crude, roughly five years of it. Ours is CME WTI over sixteen. Different exchange, different session hours, different holiday calendar, different participant mix — there is no sample overlap of any kind.
- Journal of Emerging Market Finance is a modest venue, and an emerging-market microstructure result is the category most likely to describe a market that has since changed.
- The paper reports its own forecasting improvements; they are not restated here, and they are improvements in variance accuracy, not in money. Nobody has read the paper — this restatement comes from the indexed abstract alone.
- The intraday volatility U-shape is one of the least controversial facts in this literature. That the filter works is nearly a foregone conclusion; that it is worth anything to a trader is the untested part.
- Every cost figure any descendant produces rests on `half_spread_ticks = 1`, which is an assumption, and CL will never have a measured spread in this archive because the L1 entitlement lapsed.

## Triage grade

**C.** C stands. The missing piece is a periodicity-filtered volatility indicator — a bucketed time-of-day fit updated point-in-time — and no milestone builds one. Its cost is a new estimator in `crucible-strategies` with an expanding-window fit, a declared warmup contribution, and a grid spec for its parameters. A full-sample fit would be cheaper and would be lookahead. The macro-announcement half cannot be reproduced at all.
