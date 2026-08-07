---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: trading-time-seasonality-energy
topic: calendar-effects
grade: B
hypothesis_family: energy-trading-time-seasonality
status: draft
blocked_on: calendar predicates — the effect is indexed by the futures TRADING date, which no operand names
created: 2026-08-06
doi: 10.1016/j.eneco.2022.106324
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Seasonality indexed by the trading date, not the delivery month

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

Christian-Oliver Ewald, Erik Haugom, Gudbrand Lien, Ståle Størdal, Yuexiang Wu. *Trading time seasonality in commodity futures: An opportunity for arbitrage in the natural gas and crude oil markets?*.
Energy Economics, 2022.
DOI `10.1016/j.eneco.2022.106324`. <https://doi.org/10.1016/j.eneco.2022.106324>
Retrieved from the crossref API on 2026-08-06.

The authors distinguish a calendar pattern attached to when a contract is traded from the far better known pattern attached to when it delivers, argue the two are not the same thing, and report evidence of the former in gas and crude using rank-based tests and an asset-pricing framing. They also argue the obvious pricing-kernel explanation does not fully account for it.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.eneco.2022.106324':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This is the one calendar hypothesis in the batch with a payer worth naming. Physical energy participants hedge on a schedule that has nothing to do with their price view: producers after budget and reserve-based lending cycles, utilities and refiners ahead of the seasons they must serve. That demand arrives whether or not the price is attractive, and whoever takes the other side of a large one-directional order flow charges for the inventory risk. The loser is the calendar-bound hedger, and they keep losing because the hedge is a financing covenant or a regulated procurement obligation rather than a trade. What makes this a hypothesis rather than folklore is the separation the paper insists on: the delivery-month seasonal is already in every textbook and already in the curve, so a pattern in the trading date is a claim about the premium rather than about the commodity.

## Signal in Crucible terms

- Instrument: one CME WTI contract per config, four-digit key. Natural gas, which carries half the paper's evidence, is not in the archive.
- Timeframe: `1d`, aggregated on read. The effect is monthly, so the bar grain is not the constraint; the sample length is.
- Feature: the calendar month of the trading date, held separate from the contract's delivery month. No operand names either, and the second is the harder one — a contract's delivery month is part of its identity, not something the grammar can read.
- Rule as it would be written: hold long during one pre-registered trading month and stay flat otherwise, on a contract whose delivery month is held fixed so the two seasonals do not blur together.
- The separation the paper's whole argument turns on is the part hardest to express here. A single-instrument config cannot hold delivery month constant while varying trading month, because it only ever sees one contract.

## Data

- Owned: CL `ohlcv-1m` 2010-06-06 to 2026-07-28, every contract, curated, with expiries resolvable from the archived definition records (D-0090).
- Not owned: natural gas. Given the paper's own emphasis on gas, running crude alone answers a narrower question than the one asked.
- Not owned: any positioning or trader-category series. The hedging-pressure channel the paper points at is exactly what a positioning report would show, and we cannot observe it — only its shadow in price.
- Sixteen years gives roughly sixteen observations per calendar month, and no finer grain produces more Januaries.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- One trading month is pre-registered before the run, with the contract's delivery month fixed. Every other pairing is a declared trial in this family, and there are many, which is what `max_pbo` below is reading.
- `min_oos_sessions = 500` — basis: roughly two years of sessions, which delivers about two instances of the registered month. Stating the floor makes the sample inadequacy a machine-checked kill rather than a caveat.
- `min_oos_trades = 30` — basis: at most one round-trip per year per registered month, so this is a floor that only pooling across contracts can ever satisfy.
- `min_oos_sharpe_after_costs = 0.40` — basis: turnover is a handful of round-trips a year, so costs are near-irrelevant and the floor is doing statistical work rather than economic work.
- `kill_if_dead_at_ticks = 1.0` — basis: with turnover this low, an edge that cannot clear the assumed half-spread is an artefact.
- `max_permutation_p = 0.05` with a declared, swept block length, and `max_pbo = 0.35` — basis: twelve trading months crossed with twelve delivery months is a large candidate space, and the overfit probability is the statistic that actually reads it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is a strong field journal and the framing is careful, which raises the prior relative to the rest of this batch. That is a comment on the paper, not a prediction about our data.
- The paper is a measurement, not a strategy. Its tests are distributional comparisons and an asset-pricing check; any rule we build from it is our construction and must not be described as a replication.
- Half the evidence rests on a market we do not hold. Crude alone reproduces neither the cross-commodity comparison nor the seasonal-commodity extension.
- The window almost certainly overlaps ours substantially, so this would be a re-test rather than an out-of-sample test.
- The hedging-pressure explanation is offered informally by the paper's own description and cannot be verified here: without positioning data we can observe a price pattern and attribute it to a channel we never see.
- `half_spread_ticks = 1` is an assumption and not a measurement (D-0120); at this turnover the verdict does not turn on it, which is one of the few reassuring things in this file.

## Triage grade

**B.** Two calendar operands are missing, not one: the trading date's month, and the contract's own delivery month held fixed beside it. The first is the same build that unblocks the rest of this batch. The second is harder — a single-instrument config has no way to vary one while holding the other, so this hypothesis needs the funnel-level cross-product as well, and that is pooling work rather than grammar work.
