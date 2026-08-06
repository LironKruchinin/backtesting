---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: inventories-and-oil-basis
topic: term-structure-roll-yield
grade: C
hypothesis_family: cl-inventory-basis-relation
status: draft
blocked_on: physical inventory data (EIA or equivalent) with a stated availability rule, plus a two-maturity basis feature
created: 2026-08-06
doi: 10.1016/j.resourpol.2022.102657
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Inventories, the basis, and where storage theory stops working

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

Jennifer I. Considine, Philipp Galkin, Abdullah Aldayel. *Inventories and the term structure of oil prices: A complex relationship*.
Resources Policy, 2022.
DOI `10.1016/j.resourpol.2022.102657`. <https://openalex.org/W4220746520>
Retrieved from the openalex API on 2026-08-06.

The authors re-examine whether stock levels explain the gap between near and deferred oil prices, using daily data across several international storage locations over roughly three years. They report that the textbook relation behaves as expected at daily and weekly frequency and degrades as the data is coarsened, and they propose an alternative built on the value of spread options with a locational term added.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.resourpol.2022.102657':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Storage theory says the shape of the curve is rent for holding physical barrels, and rent is paid by whoever needs the barrels somewhere they are not. The losing side is nameable and physical: the refiner who must run and the utility that must deliver, both of whom pay up when tanks are drawn down because a tank is a constraint rather than an opinion; and symmetrically the producer who must place barrels when tanks are full. They keep paying because pipelines, tanks and shipping schedules do not negotiate. What makes this hard rather than merely unbuilt is availability. Inventory is a scheduled statistical release describing a week that has already ended, and it is revised afterwards. Section 2.1 requires the availability rule to be fixed before integration, and for a weekly release the only honest stamp is the publication instant, not the reference week — which is exactly the mistake that makes an inventory study look prescient.

## Signal in Crucible terms

- Instruments: CL contracts of two maturities read together, which one config cannot declare.
- Timeframe: `1d`, aggregated on read. The paper finds the relation strongest at daily and weekly frequency, so the grain is available even though the feature is not.
- Features: a near-versus-deferred price difference, and a stock level. The first needs arithmetic between operands and two instruments; the second needs a data source the archive does not hold.
- Rule as it would be written: take the front contract long when stocks are low relative to a trailing norm and the curve is in backwardation, flat otherwise. Every term of that condition is currently inexpressible.
- Before any of it: the release calendar and revision history for the stock series must be recorded with the data, because a rule that reads a reference-week number on the reference week is a lookahead bug that produces exactly the result the hypothesis predicts.

## Data

- Not owned: any physical inventory series, from any agency or commercial provider. This is the blocking acquisition and it is not a market-data purchase, so the existing vendor relationship does not cover it.
- Not owned: locational spot or interdealer quotes, which is the specific dimension the paper's proposed alternative is built on.
- Owned: every CL contract, `ohlcv-1m`, 2010-06-06 to 2026-07-28, plus expiries from the definition records. The futures side is complete.
- The paper's window is about three years; ours is sixteen. That looks like an advantage and is not, because our sixteen years contain no inventory data at all.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Gate zero, before any replay: the inventory series must arrive with a publication timestamp per observation and a revision history. A series carrying only a reference week is refused outright, because the availability rule cannot be stated for it and §2.1 makes that a hard stop rather than a caveat.
- `min_oos_sessions = 750` — basis: three years of out-of-sample sessions to obtain roughly 150 weekly releases, which is the least that supports a claim about a weekly conditioning variable.
- `min_oos_trades = 100` — basis: at most one signal change per weekly release, so this asks for a couple of years of releases after the warmup.
- `min_oos_sharpe_after_costs = 0.50` — basis: a two-leg basis position pays the spread twice per side, so the floor must sit clear of the doubling.
- `kill_if_dead_at_ticks = 1.0` — basis: four half-spread charges per round-trip on the assumed one tick; not surviving that is decisive.
- `max_permutation_p = 0.05` with declared and swept block length — basis: an inventory series is itself strongly autocorrelated, so a conditioning variable drawn from it will look informative against any null that does not preserve dependence.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper is a measurement of an economic relation, not a strategy. There is no rule to replicate, so anything we run is our invention with their observation attached, and the record must say so.
- Their sample is three years of daily data across storage hubs; ours would be sixteen years of one exchange's futures with no stock data. These are not comparable studies and a disagreement between them would be uninformative.
- The venue publishes across a wide range of resource economics and has had variable quality control. Nothing here has been read.
- Inventory statistics are revised. A backtest that reads final revised values is lookahead by construction, and it is the specific lookahead that would make this hypothesis appear to work.
- `half_spread_ticks = 1` is an assumption, not a measurement, charged four times per round-trip on a two-leg position, and CL has no L1 data in this archive to settle it with (D-0120).

## Triage grade

**C.** Two blockers, and the expensive one is data rather than code: a physical stock series with per-observation publication timestamps and a revision history, which is a new provider, a new availability rule and a new manifest shape. On top of that sits the same multi-maturity feature that blocks the rest of this topic. The futures half is free; the half that carries the hypothesis is not.
