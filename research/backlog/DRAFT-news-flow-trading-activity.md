---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: news-flow-trading-activity
topic: volume-price
grade: C
hypothesis_family: commodity-news-arrival-volatility
status: draft
blocked_on: a news dataset with arrival timestamps and an availability rule; nothing in `docs/DATA_PLAN.md` acquires one
created: 2026-08-06
doi: 10.1002/fut.21724
source_api: crossref
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — News arrival and tone against realized variance in gold and crude

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

Adam E. Clements, Neda Todorova. *Information Flow, Trading Activity and Commodity Futures Volatility*.
Journal of Futures Markets, 2015.
DOI `10.1002/fut.21724`. <https://doi.org/10.1002/fut.21724>
Retrieved from the crossref API on 2026-08-06.

Using a commercial news feed tagged to gold and crude, the authors relate the rate at which stories arrive and their tone to realized variance, controlling for flow, depth and trader positioning. Arrival surprises and negative tone carry the most weight, while positioning adds little once news is accounted for — a conclusion that runs against an earlier strand of the literature they name.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.21724':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

No payer is identified, and none is claimed. The paper models variance: news arriving and news tone move how much gold and crude bounce around, and positioning adds little once arrival is controlled for. Nothing in that says which direction, and a magnitude result is not a trade. If a directional version existed it would have to be an underreaction story — prices absorb a news shock too slowly, so continuation pays — but that is a different hypothesis with a different literature and it is not what was tested. The honest reading is that the counterparty cannot be named from this result. There is also an uncomfortable second-order problem: the sentiment score is itself a fitted model shipped by a vendor, so any edge attributed to tone is jointly an edge attributed to whatever that vendor's classifier happened to learn from its own training corpus, which nobody outside the vendor can audit.

## Signal in Crucible terms

- Instruments `GCZ2024` and `CLM2024`, timeframe `15m` or `1h` — a grain fine enough for a news arrival to be locatable but coarse enough that a story maps to a bar.
- The construction WOULD be: a news-arrival-rate operand and a tone operand, each stamped with an explicit availability time, used to gate a directional rule.
- Where it breaks, first: there is no news corpus. Nothing in the acquisition plan buys one, and no milestone consumes one.
- Where it breaks, second, and it is the deeper problem: a news timestamp is a publication time, not an availability time, and Sec 2.1 requires the availability rule to be defined BEFORE integration. A feed that backfills or restates would silently leak.
- Where it breaks, third: the paper also uses trader positioning and market depth. We have neither — no COT data, and no book depth outside one year of ES `tbbo`.
- The nearest expressible surrogate today is `stdev(period, source=return)` as a trailing regime gate on GC or CL. That is a different hypothesis about variance clustering and it gets its own file rather than being run under this name.

## Data

- Owned: curated 1-minute GC and CL bars from 2010-06-06 to 2026-07-28, with metals and energy calendars behind them.
- Not owned: a news corpus of any kind, at any grain, with or without timestamps. This is an acquisition nobody has proposed.
- Not owned: trader positioning. There is no COT series in the archive and none is planned.
- Not owned: market depth. The archive holds OHLCV plus one year of ES `tbbo`, and neither GC nor CL has any book data at all.
- The `statistics` schema is archived for both roots and holds settlement-type fields, but it is not depth and it is not curated.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- No run is authorized under this key until a news corpus with a stated availability rule exists. Criteria are recorded now so they are not written after the first plot.
- Every news feature must be evaluated at its availability time, and a run that keys on publication timestamps is void rather than a weaker result — basis: Sec 2.1, and a backfilled feed is the classic silent leak this project exists to prevent.
- A truncation-invariance control must pass before any verdict: the feature's reading at bar t must not change when the series is truncated after t — basis: D-0088, and a news join is exactly where truncation invariance breaks quietly.
- `min_abs_ic = 0.02` at the horizons the corpus timestamps actually support, with the bootstrap interval excluding zero at the same horizon — basis: D-0085; if the news feature carries no directional information, stop before building anything.
- `min_oos_sessions = 250` and `kill_if_dead_at_ticks = 1.0` — basis: the backlog constants; the cost floor bites hard here because a news-triggered entry trades precisely when the spread widens.
- `max_permutation_p = 0.05` and `require_controls_beaten = true` — basis: an event-conditioned rule trades on a small subset of bars, which is where an ordinary draw most easily looks extreme.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The Journal of Futures Markets is a genuine refereed outlet and this is a serious paper — which is worth saying, because most of this batch is not.
- The result is about variance, not direction. Any directional strategy built on it is our extrapolation, and the paper should not be cited as support for one.
- The news feed is licensed, commercial and expensive, and the sentiment scores are the output of a proprietary model. Reproducing this requires buying both the corpus and the vendor's judgement.
- The paper explicitly disagrees with earlier published work on positioning. That is honest of it, and it also means the literature here is unsettled rather than converged.
- The sample is 2000s-into-2010s gold and crude. Our CL and GC begin 2010-06-06, so the overlap is partial at best and the news-flow regime of that era predates the social-media news cycle entirely.
- The cost assumption does not bite until a directional rule exists, but when it does, GC and CL will never have a measured spread (D-0120).

## Triage grade

**C.** The missing piece is a timestamped news corpus with an availability rule, and that is an acquisition plus a design decision, not code. It would cost a licensing negotiation, a per-field answer to the question of when each item could actually have been known, and a sentiment model whose overfitting surface we would then own. Add that the paper offers a variance result with no directional bet under it, and there is nothing here to schedule.
