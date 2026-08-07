---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: commodities-long-run-carry
topic: term-structure-roll-yield
grade: C
hypothesis_family: commodity-carry-and-spot-decomposition
status: draft
blocked_on: carry as a feature (needs two maturities) and cross-sectional portfolio accounting (post-M4)
created: 2026-08-06
doi: 10.2469/faj.v74.n2.4
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Splitting long-run commodity returns into carry and price level

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

Ari Daniel Levine, Yao Hua Ooi, Matthew Richardson, Caroline Sasseville. *Commodities for the Long Run*.
Financial Analysts Journal, 2018.
DOI `10.2469/faj.v74.n2.4`. <https://openalex.org/W2886757093>
Retrieved from the openalex API on 2026-08-06.

The authors assemble a very long daily history of futures prices, far longer than the usual samples, and separate index-level commodity returns into a piece attributable to the shape of the curve and a piece attributable to movement in the price level itself. They report that variation across inflation and business-cycle states comes mostly from the price-level piece, and read the whole as support for holding commodities alongside equities and bonds.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2469/faj.v74.n2.4':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The carry story has a named loser and it is one of the oldest in the literature: the producer who sells forward to lock in a price, accepting a discount because their financing and their business depend on the price rather than on the forecast. Whoever takes the other side is paid for standing between the hedger and the eventual buyer. That much is solid. What is not solid is the leap from there to this paper's conclusion, and the paper's own decomposition is what undermines it: if the state-dependence lives mostly in the price level rather than in the carry, then the component that is a genuine risk premium is not the component doing the work, and the recommendation to hold commodities is a bet on the price level with a premium story attached. That is a materially different claim from the one the mechanism supports.

## Signal in Crucible terms

- Instruments: the archive holds two commodity roots — CL and GC. A cross-sectional claim tested on a cross-section of two is not a test of the claim, and this draft treats that as disqualifying rather than as a limitation.
- Timeframe: `1d`, aggregated on read.
- Feature: carry, which is a relation between two maturities of the same root. Needs multi-instrument configs and arithmetic between operands, neither of which exists.
- The portfolio side needs cross-sectional accounting — ranking several roots and holding a spread of positions — which is explicitly post-M4 and not a near-term build.
- Rule as it would be written: rank the available commodity roots by carry, hold the top and short the bottom, rebalance monthly. With two roots that rule is one pair trade wearing a portfolio's vocabulary.

## Data

- Owned: CL and GC `ohlcv-1m` 2010-06-06 to 2026-07-28, with expiries. That is two commodities out of the many any index-level claim rests on.
- Not owned: agricultural, livestock, industrial metals, or any of the breadth that makes a commodity index a commodity index.
- Not owned: inflation or business-cycle state variables. The paper's central finding is about conditioning on economic states, and we hold no state variable to condition on.
- The paper's history reaches back well over a century; our archive covers sixteen years of it. Their long sample is also, by necessity, spliced from sources of varying construction, which is a limitation of the evidence rather than of our ability to match it.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Instrument breadth is a pre-registered kill: a cross-sectional hypothesis run on two roots must be killed on admissibility, before any replay. Reporting a two-name result in the shape of an index-level finding is exactly the partial answer dressed as a whole one that this project refuses.
- `min_oos_sessions = 1000` — basis: four years of out-of-sample sessions, because a monthly-rebalanced carry rule produces only about a dozen independent decisions a year and a claim about economic states needs to span more than one of them.
- `min_oos_trades = 100` — basis: roughly eight years of monthly rebalances, which is the scale at which a carry premium's presence or absence becomes distinguishable from a run of commodity beta.
- `min_oos_sharpe_after_costs = 0.40` — basis: a spread position pays the assumed half-spread on each leg each way, and a carry premium is a slow, modest thing by the paper's own framing, so the floor is set where those two facts meet.
- `require_controls_beaten = true` — basis: the buy-and-hold control is the whole argument here. A paper recommending commodities as a holding must be judged against holding them, and a carry rule that cannot beat the passive position has not demonstrated the premium it names.
- `max_pbo = 0.30` — basis: ranking rules have many free choices — the lookback, the rebalance frequency, the number of names — and with two names the effective search space is larger than the evidence.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is a practitioner journal and the authors are affiliated with an asset manager that sells the exposure the paper concludes in favour of. That is not an accusation of bad faith; it is a structural selection concern that belongs on the record, because a study concluding against the product would have had a harder road to publication.
- The paper reports its own long-run return decomposition figures; they are not restated here and none of them is a claim about anything this archive would produce.
- A history reaching back to the nineteenth century is a construction, not an observation. Contract specifications, delivery terms, exchanges and liquidity all changed repeatedly over that span, and the earliest data is nothing anyone could actually have traded.
- Our archive covers a small and unusual slice of their sample: sixteen years containing one commodity supercycle unwind, one pandemic dislocation, and a negative crude settlement. Any state-conditional claim tested on it is a claim about that slice.
- `half_spread_ticks = 1` is an assumption and not a measurement for both roots, and a two-leg rebalanced position charges it repeatedly (D-0120).

## Triage grade

**C.** C, and the blocker named above understates it. Carry needs two maturities and arithmetic between operands; the cross-sectional claim needs portfolio accounting that is explicitly post-M4; and even with both, this archive holds two commodity roots, which cannot support an index-level finding at all. The buildable part would answer a much smaller question than the paper asks, and the draft should not pretend the gap is only machinery.
