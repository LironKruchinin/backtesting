---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: intraday-periodic-volatility-curve
topic: intraday-seasonality
grade: B
hypothesis_family: intraday-diurnal-volatility-normalizer
status: draft
blocked_on: a CAUSAL time-of-day volatility normalizer — the paper's estimator is a full-sample average over the whole span, which Sec 2.1 forbids inside a strategy
created: 2026-08-06
doi: 10.1080/01621459.2023.2177546
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — A causal time-of-day volatility normalizer

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

Torben G. Andersen, Tao Su, Viktor Todorov, Zhiyuan Zhang. *Intraday Periodic Volatility Curves*.
Journal of the American Statistical Association, 2023.
DOI `10.1080/01621459.2023.2177546`. <https://openalex.org/W4319789775>
Retrieved from the openalex API on 2026-08-06.

A statistics paper rather than a strategy paper. It constructs an estimator for how average volatility varies across the trading day, indexed by clock time, built from local variance estimates over short overlapping blocks of high-frequency returns and then averaged across many days, and supplies the limit theory and a feasible variance estimator to go with it. Applied to S&P 500 futures, it finds the shape itself is not constant across the sample, with more of the day's movement occurring outside US hours in the later years.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1080/01621459.2023.2177546':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

There is no counterparty because there is no bet. This is preprocessing, and its value is that it stops a fixed threshold from meaning two different things at two times of day: a half-point move at the reopen is ordinary and the same move at midday is not, so an unnormalized z-score or band is loose when the market is fast and tight when it is slow, and it fires accordingly. Dividing by a time-of-day scale makes one rule mean one thing all session. The reason this belongs in the backlog at all is that several other entries here condition on session time, and every one of them is implicitly making a crude version of this correction by hand. Getting it right once, causally, is worth more than getting it approximately right five times. But nobody pays us for a nuisance correction, and treating one as an edge is a category error.

## Signal in Crucible terms

- Instrument `ESM2025` or any curated ES contract, timeframe `5m` or `15m`. The normalizer is instrument-agnostic; ES is where the reference measurements exist.
- The construction WOULD be: a per-time-of-day scale estimated from sessions strictly earlier than the current one, updated forward, with every band and z-score threshold expressed in units of that scale.
- Where it breaks, first: the paper's estimator averages across the whole sample, which is a full-sample statistic and is exactly the lookahead Sec 2.1 forbids inside a strategy. Using it as published would be a leak, not an approximation.
- Where it breaks, second: every indicator in this build is a trailing contiguous window behind a one-bar update. A time-of-day normalizer needs state that is NOT contiguous in time — the 10:05 bucket from the last forty sessions — which is a shape no existing `IndicatorKind` has.
- Where it breaks, third: warmup. This thing needs whole sessions rather than bars, and Sec 2.6 aligns a grid on `max_warmup_bars`, so the warmup declaration has to be expressed in bars derived from sessions or the fair-comparison rule breaks quietly.
- The closest expressible substitute is `stdev(period, source=return)` with a period near one session's bar count — which mixes times of day together rather than separating them, and is therefore not this object at all.

## Data

- Owned: curated 1-minute ES bars from 2010-06-06 to 2026-07-28. Sixteen years is a longer span than the paper's application, which makes this one of the few entries where our archive is the richer input.
- Owned: the equity-index calendar with session eras (D-0086), so buckets can be defined against the session rather than against a UTC clock — and the era boundaries are exactly where a naive UTC bucketing would silently misalign.
- Not owned: any options or VIX series, so there is no external volatility measure to validate the estimated level against; the only check available is internal consistency.
- Not owned: anything below the 1-minute curated grain, so the block sizes the paper's asymptotics assume are coarser here than there.
- No acquisition required. The gap is entirely code plus a control.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- A truncation-invariance run must pass before any result is quoted: the normalizer's reading at bar t must not move when the series is truncated after t. A failure means a leak and the run is void rather than a weaker result — basis: D-0088, and this is the exact defect the estimator invites.
- The full-sample version may exist ONLY inside the test file, as the converse control — basis: D-0080's rule for `LeakyZScore`, and the point of a converse control is to show what the causal version is being compared against without ever making it reachable from TOML.
- The causal normalizer must track the full-sample shape on held-out sessions within a stated tolerance — basis: if it cannot approximate the object it is a causal version of, it is not a normalizer, it is a different statistic.
- For any strategy that then uses it, the normalized arm must beat the unnormalized arm on `min_oos_sharpe_after_costs` by a registered margin, else Kill — basis: infrastructure that does not improve the thing it serves has not earned its complexity, and this is the gate that would end the line of work.
- `min_oos_sessions = 250` — basis: the backlog constant, and doubly binding here since the normalizer itself consumes sessions as warmup before the evaluation window can even open.
- `kill_if_dead_at_ticks = 1.0` — basis: a normalizer changes when thresholds fire, which changes trade count, which changes what is paid; the cost sweep is how that shows up.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The Journal of the American Statistical Association is a top statistics journal and this is a serious methodological paper. Saying so matters, because the working prior applied to the rest of this batch does not fit here — but the paper also makes no trading claim at all, so there is no source performance figure to restate and none is restated.
- The estimator's asymptotics assume shrinking blocks and a very long stretch of high-frequency data. Our per-fold training windows are short by that standard, so the theory that makes the published estimator well-behaved does not straightforwardly cover the causal variant.
- The paper's own finding is that the intraday shape shifts across the sample. That argues against a fixed normalizer and in favour of a rolling one — which means more parameters, a lookback choice, and a fresh overfitting surface.
- The application is S&P 500 futures, which is a market we hold; that is the one clean alignment here, and it means the reference curve could be rederived rather than assumed.
- The cost assumption does not bite directly, since there is no trade. It bites indirectly through every strategy the normalizer would later feed.

## Triage grade

**B.** Data is owned and the market matches. The missing piece is a causal time-of-day normalizer: the first indicator in this codebase whose state is not contiguous in time, since it carries one accumulator per intraday bucket across sessions. That costs a new `IndicatorKind`, a warmup declared in sessions and translated to bars so Sec 2.6 still aligns the grid, and a truncation-invariance control written before the first result — the published estimator is full-sample and would be a leak if lifted directly.
