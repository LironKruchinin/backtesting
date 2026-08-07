---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: commodity-regime-switching-variance
topic: vol-regime-clustering
grade: B
hypothesis_family: commodity-regime-switching-variance
status: draft
blocked_on: a Markov regime-switching conditional-variance indicator; every indicator in this build is a trailing window with no latent state
created: 2026-08-06
doi: 10.1002/jae.590
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Two switching variance states in commodity index futures

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

Wai Mun Fong, Kim Hock See. *Modelling the conditional volatility of commodity index futures as a regime switching process*.
Journal of Applied Econometrics, 2001.
DOI `10.1002/jae.590`. <https://doi.org/10.1002/jae.590>
Retrieved from the crossref API on 2026-08-06.

The authors model the conditional variance of a commodity index futures return series with a specification that permits abrupt shifts between states, with the odds of shifting depending on observable fundamentals including the basis, alongside GARCH dynamics, seasonal terms and fat tails. On mid-1990s daily data they report clear evidence of two states, that GARCH effects largely disappear once switching is allowed, that a negative basis makes the turbulent state more likely, and that the switching specification forecasts daily variance better than plain GARCH.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/jae.590':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A two-state description says variance does not drift smoothly but jumps between a calm regime and a turbulent one, and that the odds of jumping depend on the basis — tight inventories invert the curve and make prices jumpy, which is the theory of storage restated as a transition probability. The tradeable descendant would be a state gate: stand aside, or trade differently, while the turbulent state is inferred. Who pays? The paper never says, and it makes no trading claim at all. If such a gate worked, the plausible payer is whoever must hold commodity exposure through the turbulent state regardless — index-tracking commodity funds rolling on a published schedule, and hedgers whose size is set by physical output rather than by market conditions. That payer is being supplied by us, not by the paper, and a mechanism the source does not assert should carry correspondingly less weight in any decision to spend compute on it.

## Signal in Crucible terms

- Faithful construction: a filtered probability of the turbulent state, updated bar by bar and estimated only from data available at decision time, used as an operand.
- Blocker one: every indicator in this build is a trailing window. There is no latent state, no filter, no `IndicatorKind` that could name one, and a full-sample maximum-likelihood fit would be exactly the §2.1 lookahead the grammar is built to refuse.
- Blocker two: the basis is the difference between two contracts' prices. That needs a second instrument in one config AND arithmetic between operands — two separate things the grammar does not have.
- The nearest expressible substitute is a threshold on `stdev(period, return)`: a hard classifier with no persistence and no transition probability. It is a different object, and if it is what gets run the file must say so and take its own family key.
- `CLZ2024` / `GCZ2024` at `1d` would be the vehicle, with the state indicator's parameters entering the grid and its warmup declared into `max_warmup_bars`.

## Data

- Owned: CL and GC at 1-minute grain 2010-06-06 → 2026-07-28, resampled to 1d on the exchange's own sessions.
- Owned but unreachable from a config: every outright contract, including the deferred months a basis term would need. The data exists; the grammar names one instrument.
- Not owned: the GSCI itself, or any way to construct a commodity index basket from two roots.
- Not owned: physical inventory or warehouse-stock data, so the storage story behind the transition probabilities can only be proxied, never checked.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: a two-state model needs many visits to the rarer state, and a sample containing two turbulent episodes has estimated a transition probability from two observations.
- The discriminator that can kill it: the latent-state gate must beat a plain fixed-threshold `stdev` gate on the same windows. If a hard threshold does as well, the switching machinery bought nothing and this is Killed no matter how the curve looks.
- `min_oos_sharpe_after_costs = 0.3` and `kill_if_dead_at_ticks = 1.0` — basis: a state gate on daily bars trades rarely, so if one tick of half-spread kills it the state was never worth inferring.
- `max_pbo = 0.5` — basis: a switching model has more free parameters than anything else in this batch, and PBO is the charge for that.
- `max_permutation_p = 0.05` and `require_plateau = true` over the state estimator's lookback — basis: regime assignments that shift wholesale with a small parameter change are fitted labels, not states.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Daily GSCI futures over 1992–1997 — five years, thirty years ago, on an index rather than on a contract anyone replays. There is no sample overlap with our archive and the instrument is not one we hold.
- The Journal of Applied Econometrics is a strong venue, but this is a modelling exercise with no trading claim and no costs anywhere in it.
- Regime-switching models are well known to fit in sample and to assign states unstably out of sample. That is the specific worry the plateau and PBO criteria above are registered against.
- The most durable part of the paper is the basis link, and that is the part our grammar cannot compute at all — so a descendant built here would drop the paper's best idea and keep its weakest.
- The paper reports its own forecast comparisons; they are not restated here.
- Costs rest on `half_spread_ticks = 1` (D-0120), permanently, for both CL and GC.

## Triage grade

**B.** B: the data is owned but the missing piece is a latent-state indicator, and this build has none — every indicator is a trailing window. The cost is a filtered-probability estimator in `crucible-strategies` with an expanding-window fit (a full-sample fit would be lookahead), a warmup contribution, and a grid spec for its parameters. The basis term is a second, separate gap and would stay unbuilt.
