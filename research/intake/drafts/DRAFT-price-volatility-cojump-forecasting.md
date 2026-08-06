---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: price-volatility-cojump-forecasting
topic: jump-detection-discontinuities
grade: C
hypothesis_family: jump-conditioned-volatility-forecast
status: draft
blocked_on: an options-implied volatility series, a jump estimator, and a criterion that scores a forecast
created: 2026-08-07
doi: 10.1002/fut.70091
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Jumps in price and in volatility at the same instant, and what they forecast

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

Kefu Liao. *The Role of Price‐Volatility Cojumps in Volatility Forecasting*.
Journal of Futures Markets, 2026.
DOI `10.1002/fut.70091`. <https://doi.org/10.1002/fut.70091>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author identifies instants at which an equity
index and its implied-volatility index both jump, separates them by direction,
embeds the resulting measures in a heterogeneous autoregressive volatility model,
reports that downward and upward simultaneous jumps push future volatility in
opposite directions and that including them improves out-of-sample forecasts, and
notes that a recent price jump matters more for forecasting when a volatility jump
accompanied it.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.70091':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A price jump on its own is ambiguous: it can be information arriving or liquidity
withdrawing. A price jump accompanied by a jump in the implied volatility index is
the second, because the option market has simultaneously repriced uncertainty. That
distinction is genuinely useful and it is a distinction about *state*, not about
direction — which is the same conclusion the gold volatility candidate in this batch
registers, reached from a different direction. Nobody is paying anyone here; it is a
forecasting result. Its relevance to this project is as a bound: with bars alone we
can see the price jump and never the volatility jump, so we can never make the
distinction the paper says is the informative one.

## Signal in Crucible terms

- Not expressible. The second series is an options-implied index and
  `external/cboe/` does not exist; the loader is deliberately unbuilt until post-M4.
- The jump identification is also not expressible in the paper's sense — it uses a
  high-frequency estimator, and the grammar's nearest object is a trailing z-score of
  returns, which is a threshold rule rather than an estimator.
- And the output is a **volatility forecast**, which the funnel does not score. The
  funnel judges position rules and their equity curves; a criterion that reads
  forecast accuracy does not exist. Wave 1 recorded this same gap for
  `crude-regime-switching-garch`, and it is worth counting: two candidates now wait
  on a way to score a forecast rather than a strategy.

## Data

- Owned: ES `ohlcv-1s` and `ohlcv-1m` over sixteen years, which is the price leg at
  a finer grain than the paper's.
- Not owned: any implied-volatility index. The CSVs are free and manual per
  `docs/DATA_PLAN.md`, and the availability rule is already written down — a daily
  index value is knowable at that session's close — so this is one of the more
  tractable missing series in the index.
- Not built: a jump estimator; not built: a forecast-scoring criterion.
- Note the asset class: this candidate is equity index, which wave 2 is deliberately
  weighted away from. It is included because its *conclusion* — that the informative
  object is the joint jump, which we cannot see — is a bound on every jump candidate
  here regardless of root.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today, and doubly so: neither the input nor the scoring exists.
- Registrable now: a forecast criterion, when one is built, must be pre-registered
  in the same way a return criterion is — the loss function, the benchmark model and
  the out-of-sample window declared before the fit. A volatility model compared
  against a benchmark chosen afterwards is the same selection problem as a grid
  quoted at its best combo.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Equity index and its implied-volatility index; a recent sample. We hold the price
  leg only, and equity index is the asset class this wave is weighted away from.
- The paper's own reported forecast improvements are theirs and are not restated
  here.
- HAR-family volatility forecasting is a crowded field in which almost every added
  regressor improves out-of-sample fit somewhere, and the number of published
  variants is itself a reason for caution about any one of them.
- The honest summary is that this paper tells us what we cannot see rather than what
  to trade, and it is graded accordingly.

## Triage grade

**C.** C, and three missing pieces: an **options-implied volatility series** (free, manual,
availability rule already written), a **jump estimator** (a build) and a **criterion
that scores a forecast rather than a position** (a funnel gap now shared with one
wave-1 candidate). The third is the interesting one, because it is a limitation of
the machine rather than of the archive.
