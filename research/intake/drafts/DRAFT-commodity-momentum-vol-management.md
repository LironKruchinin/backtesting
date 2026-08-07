---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: commodity-momentum-vol-management
topic: vol-managed-exposure
grade: C
hypothesis_family: commodity-cross-sectional-momentum-vol
status: draft
blocked_on: cross-sectional portfolio accounting (post-M4) AND continuous position sizing; commodity momentum here is a cross-sectional sort, not a time-series rule
created: 2026-08-06
doi: 10.1002/fut.22195
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Volatility management applied to cross-sectional commodity momentum

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

Qi Xu, Ying Wang. *Managing volatility in commodity momentum*.
Journal of Futures Markets, 2021.
DOI `10.1002/fut.22195`. <https://doi.org/10.1002/fut.22195>
Retrieved from the crossref API on 2026-08-06.

Using Chinese commodity futures, the authors report that scaling a cross-sectional momentum portfolio by its own trailing volatility improves it, and that adding the scaled version to an allocation helps. They then work through a list of candidate explanations — costs, leverage limits, sentiment, business-cycle risk, snooping — and conclude none accounts for the result, tying it instead to predictability in momentum's own risk rather than in market risk.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.22195':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Cross-sectional commodity momentum has one of the better-identified payers in this literature: producers hedge forward output and pay speculators to carry the price risk, and inventory and roll-yield information spreads slowly across a fragmented set of physical markets. That is a reason a premium persists, rather than a pattern someone found in a finite sample. Volatility management stacked on top is a separate claim — that momentum's own crash risk is forecastable from the strategy's trailing volatility, so cutting the book before an unwind avoids the worst of it — and its payer is whoever holds the momentum portfolio through the unwind without adjusting. Neither claim is testable here, and the reason is not subtle: a cross-section of seven correlated futures, three of which are effectively the same equity-index trade and only two of which are commodities, has no cross-sectional power at all. The mechanism is credible; the machinery is post-M4.

## Signal in Crucible terms

- Faithful construction: rank a broad set of commodity roots on trailing return, hold the winners long and the losers short simultaneously, then scale the whole book by its own trailing volatility.
- Blocker one: that is a ranked sort across many instruments with simultaneous long and short legs. A config names one instrument, and there is no cross-sectional accounting until post-M4.
- Blocker two: the volatility scaling is continuous sizing, which the grammar cannot name — the same gap as the two candidates above.
- The time-series analogue on CL alone (a trend rule scaled by trailing volatility) is a different phenomenon with a different payer, and it would get its own family key rather than borrowing this one.
- Even with both blockers cleared, the archive offers two commodity roots against the dozens a cross-sectional sort needs. Building the machinery would not make this answerable.

## Data

- Owned: CL and GC at 1-minute grain 2010-06-06 → 2026-07-28. Those are the only two commodities in the archive; 6E and ZN are financial futures and do not belong in a commodity sort.
- Not owned: any Chinese futures data, and no plan to acquire it.
- Not owned: inventory, warehouse-stock or trader-position data, so the hedging-pressure story the base effect rests on cannot be examined at all here.
- Not built: cross-sectional portfolio accounting (post-M4) and continuous sizing.
- Cost inputs rest on `half_spread_ticks = 1` (D-0120), which for a long-short book is charged on both legs and every rebalance.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- No run is authorized under this key until a commodity cross-section exists. Registered now so the thresholds are not chosen after the fact.
- Sample minimum: at least 15 commodity roots with continuous coverage and 1,000 sessions. Below either, no verdict is issued at all — this is a floor, not a gate.
- The scaled book must beat the unscaled book on the same windows; if it does not, the paper's specific contribution has failed and this is Killed even if plain momentum looks fine.
- The kill that matters most: if the improvement disappears when the volatility estimate is lagged one extra session, it was a timing artifact rather than a forecast, and the result is discarded rather than debugged.
- `min_oos_sharpe_after_costs = 0.3`, `kill_if_dead_at_ticks = 1.0` — basis: a long-short book rebalancing on a schedule pays costs on both legs, so it is among the most cost-fragile constructions in this backlog.
- `max_pbo = 0.5` and `max_permutation_p = 0.05` — basis: ranking windows and scaling lookbacks together are a wide search over a small cross-section.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Chinese commodity futures: retail-heavy, with exchange position limits, daily price bands and a listing history that starts in the 2010s. The market is not one we trade and never will be here.
- Journal of Futures Markets, 2021 — a respectable venue for this material. The abstract's own claim that data-snooping bias does not account for the profitability is worth noting as a marker rather than as evidence: essentially every published paper reports having ruled out snooping.
- The paper reports its own performance figures; they are not restated here.
- The base effect — cross-sectional commodity momentum — has broad independent support and a nameable payer, which puts it well ahead of most of this batch. The scaled variant is the newer and much weaker part of the claim.
- A cross-section of two commodities is not a cross-section. Grading this A or B by substituting a single-instrument analogue would be exactly the inflation this directory's rules forbid.
- Costs rest on `half_spread_ticks = 1` (D-0120) and are charged twice per rebalance on a long-short book.

## Triage grade

**C.** C stands, and it is doubly blocked. Cross-sectional portfolio accounting is post-M4, and continuous position sizing needs the same engine seam the other volatility-management files want. Building both would still leave the deeper problem: the archive holds two commodity roots against the dozens a ranked sort requires, so this is an acquisition question as much as a milestone one.
