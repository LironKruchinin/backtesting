---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: high-frequency-lead-lag
topic: cross-asset-lead-lag
grade: C
hypothesis_family: cme-cross-asset-lead-lag
status: draft
blocked_on: multi-instrument configs — a lead-lag statistic is defined on a PAIR, and `combo` refuses a config declaring two instruments
created: 2026-08-06
doi: 10.1080/07350015.2019.1697699
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Separating true lead-lag from simultaneous co-movement

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

Giuseppe Buccheri, Fulvio Corsi, Stefano Peluso. *High-Frequency Lead-Lag Effects and Cross-Asset Linkages: A Multi-Asset Lagged Adjustment Model*.
Journal of Business &amp; Economic Statistics, 2020.
DOI `10.1080/07350015.2019.1697699`. <https://doi.org/10.1080/07350015.2019.1697699>
Retrieved from the crossref API on 2026-08-06.

The authors extend single-asset models of delayed price adjustment to several assets at once, so that estimated leading and lagging relationships can be told apart from moves that are genuinely simultaneous, and so that a covariance estimate survives the usual high-frequency nuisances. They validate the estimator in simulation and apply it to a set of US listed equities.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1080/07350015.2019.1697699':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Lead-lag exists because information does not reach every instrument at the same instant: it is priced first where the depth is, and the thinner instrument catches up. The losing side is whoever is quoting the slower instrument with a stale view, and they are paying a real cost. The trouble is that they stop paying the moment anybody tells them, which is why measured lead-lags in modern electronic markets are quoted in microseconds and belong to participants with colocated hardware and direct feeds. At one-minute bars there is nothing left. At one-second bars a bar is still not a quote, and the fill model here crosses an assumed spread with no latency term at all. So the losing side is nameable in principle and is emphatically not us: at the frequency this claim lives at, with bar data and an assumed half-spread, we are the slow party being led.

## Signal in Crucible terms

- Instruments: pairs — ES against NQ, or ES against ZN. A lead-lag statistic is defined on two series and `combo` refuses a config declaring two instruments, so the object cannot be constructed.
- Timeframe: `1s` is the finest grain the archive stores and the only one at which the question is even interesting; `1m` is far too coarse for the effect the paper measures.
- Feature: an estimated lag structure between two price processes. Even if two instruments were declarable, the grammar has no arithmetic between operands, so a cross-series statistic cannot be formed.
- Rule as it would be written: trade the lagging instrument on the leader's move, exit within seconds. Nothing about that sentence is expressible.
- The paper's estimator is an econometric contribution rather than a trading rule. What would be built here is a rule the paper never proposed, tested on instruments it never studied.

## Data

- Owned: `ohlcv-1s` for all seven roots, 2010-06-06 to 2026-07-28. That is a large and genuinely fine-grained sample, and it is bars rather than quotes.
- Owned but narrow: `trades` and `tbbo` for ES only, 2025-07-28 to 2026-07-28, and one month of `mbo` for ES. Any honest treatment of a sub-second effect needs quote data, and we have twelve months of it for one root.
- Not owned: individual equities, which is the asset class the paper actually applies its estimator to. Nothing about the application transfers.
- Not owned: any latency model. The engine has no notion of the time between a decision and its arrival at the exchange, which for this hypothesis is the dominant term rather than a refinement.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `kill_if_dead_at_ticks = 0.5` — basis: deliberately the tightest gate in this batch. An effect measured in seconds cannot pay a full tick of spread, so if it does not survive half of the assumed half-spread there is nothing here. This is the gate expected to end the hypothesis.
- `min_oos_trades = 2000` — basis: a seconds-horizon rule produces very many round-trips, so a low trade count would mean the rule is not doing what it claims and the sample would be uninformative either way.
- `min_oos_sessions = 250` — basis: one year of sessions, which is what the ES quote data actually covers, so this floor is set by the data rather than by taste.
- `min_oos_sharpe_after_costs = 1.0` — basis: higher than anywhere else in this batch on purpose. A high-frequency claim with thousands of round-trips and no latency modelling must clear a bar that a slow strategy would not, or the result is an artefact of frictionless assumptions.
- `max_permutation_p = 0.01` with a declared and swept block length — basis: at one-second grain, bar-to-bar dependence is dominated by microstructure rather than by information, and a null that does not preserve it would be a straw man.
- `require_controls_beaten = true` — basis: a very-high-turnover rule can accumulate an apparent edge from any small systematic bias in the fill model, and the matched random-entry control at identical turnover is what exposes it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper's application is to individual listed equities, not futures. The estimator is general, but the empirical claim we would be testing is not the paper's.
- It is a methodology contribution, not a strategy. No trading result appears in the metadata we have, so a null on our side would not contradict it in the slightest.
- The entire verdict rests on the cost assumption. `half_spread_ticks = 1` is a convention wearing a measurement's field name, and its error direction is deliberately unasserted (D-0120). For six of the seven roots the L1 entitlement lapsed and cannot be reacquired, so this will never be settled.
- There is no latency in the engine. A strategy whose holding period is seconds is being simulated as though its orders arrive instantly, which is the single largest source of optimism available in this codebase.
- The effect this paper measures has been the object of the most expensive arms race in modern market structure. Anything a bar-based backtest finds at one-second grain over sixteen years should be read as an engine-bug alarm first and a discovery second, exactly as §7 instructs.

## Triage grade

**C.** Blocked on multi-instrument configs, since a lead-lag statistic does not exist for a single series, and on arithmetic between operands, since a cross-series relation must be computed rather than compared. Both are funnel-level builds. But the deeper cost is that even with them the hypothesis needs a latency model and measured spreads, and this archive holds twelve months of quote data for one root and can never acquire more.
