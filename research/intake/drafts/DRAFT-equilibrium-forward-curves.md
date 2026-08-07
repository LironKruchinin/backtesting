---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: equilibrium-forward-curves
topic: term-structure-roll-yield
grade: C
hypothesis_family: commodity-forward-curve-structure
status: draft
blocked_on: a forward-curve object: several maturities of one root read together, which one config cannot declare
created: 2026-08-06
doi: 10.1111/0022-1082.00248
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — An equilibrium account of commodity forward curves

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

Bryan Routledge, Duane J. Seppi, Chester S. Spatt. *Equilibrium Forward Curves for Commodities*.
The Journal of Finance, 2000.
DOI `10.1111/0022-1082.00248`. <https://openalex.org/W2110641059>
Retrieved from the openalex API on 2026-08-06.

The authors build a theoretical model of forward prices for a storable commodity in which stocks cannot go below zero, and show that this single constraint gives the physical good an option-like feature that a forward contract does not share. From that they derive statements about how price variability differs across horizons, including circumstances in which the usual rise-into-expiry pattern should fail, and they fit the model to crude futures.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1111/0022-1082.00248':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

There is no trading claim here and the draft should not manufacture one. The model says that when stocks approach zero the commodity's price behaves differently, because the option to consume later has run out, and that this produces conditional exceptions to the pattern everyone assumes holds unconditionally. That is a conditioning statement, not an edge, and no losing side is named because none is claimed — the model describes an equilibrium in which everyone is behaving optimally, which is the opposite of a mechanism where somebody keeps paying. What the paper is worth to this project is falsifiability: it says our data should show the maturity pattern breaking in identifiable states, which is a prediction that can be wrong. A theory that can be wrong is more useful here than a strategy paper that reports it worked.

## Signal in Crucible terms

- Instruments: several maturities of one root read together — CL for the paper's own calibration market, GC as the contrast, since a precious metal has a very different storage economics and should behave differently under the same model.
- Timeframe: `1d`, aggregated on read.
- Features: dispersion of returns at each point on the curve, compared across maturities. `stdev(period, source = 'return')` gives the statistic; nothing gives the cross-maturity comparison, because a config sees one instrument.
- The model's sharpest prediction is conditional on the stock state, and we cannot observe the stock state. So even with multi-maturity configs, the testable residue is the unconditional pattern, which is the part the model shares with everything else in this topic.
- Rule as it would be written: there is none. This registration is a falsification exercise and would produce a measurement, not a verdict on a strategy.

## Data

- Owned: every CL and GC contract, `ohlcv-1m`, 2010-06-06 to 2026-07-28, plus expiries from the definition records (D-0090). The price side of a forward curve is fully constructible in principle.
- Not owned: any inventory or stock series, which is the conditioning variable the model's distinctive prediction requires. Without it, we can test the model's least distinctive implication only.
- Not owned: options or implied series, so the volatility-term-structure statements can be checked in realised terms only.
- Sixteen years and hundreds of contract lives is a good sample for a structural question, which is unusual in this batch and worth saying.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The registrable arm today is a measurement with no criterion field behind it: no funnel criterion expresses 'variability differs across maturities in the stated direction'. Registering thresholds for a gate that does not exist would be a pre-registration in name only, and this draft declines to do it.
- If a strategy arm is ever attempted — `min_oos_sessions = 750`, basis: three years of out-of-sample sessions, since a curve-state conditioning variable changes slowly and few independent episodes occur per year.
- `min_oos_trades = 60` — basis: a state-conditioned rule turns over on the order of monthly, so this covers roughly five years of state changes.
- `min_oos_sharpe_after_costs = 0.50` — basis: a multi-leg curve position pays the assumed spread on every leg, and the floor must clear that multiple rather than a single charge.
- `kill_if_dead_at_ticks = 1.0` — basis: an edge derived from a structural model of the curve should be robust to the assumed half-spread; if it is not, it is a fitting artefact of the calibration.
- `max_pbo = 0.30` — basis: a calibrated model has many free parameters and the overfit probability is the statistic that reads them, so it is set tighter than the batch default.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- This is a theory paper in a leading finance journal from 2000. Its quality is not in question; its relevance to a backtesting engine is. Nothing here has been read.
- The commodity market it calibrates to has since seen index-fund inflows, a supply revolution, and a negative front-month settlement. A structural model fitted to the 1990s crude market is being asked about a different market.
- The model's distinctive prediction is conditional on a stock state we cannot observe. What we could test is the unconditional part, which is not the paper's contribution.
- There is no reported strategy result to compare against, which removes one publication-bias concern and adds another: a theory paper is under no obligation to have found anything tradeable, so a null on our side would be entirely consistent with the paper being correct.
- `half_spread_ticks = 1` remains an assumption and not a measurement for both CL and GC, neither of which has L1 data in this archive (D-0120).

## Triage grade

**C.** Blocked on a forward-curve object — several maturities of one root read together — which one config cannot declare and which the grammar could not compute across even if it could. That is the same funnel-level multi-instrument build the other term-structure files need. The additional cost here is that the model's sharpest, most falsifiable claim also needs an inventory series, so closing the code blocker leaves the interesting half still shut.
