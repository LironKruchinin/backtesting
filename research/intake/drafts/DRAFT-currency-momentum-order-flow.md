---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: currency-momentum-order-flow
topic: order-flow-microstructure-commodities
grade: C
hypothesis_family: fx-flow-conditioned-momentum
status: draft
blocked_on: signed order flow by counterparty and instrument, and a currency cross-section
created: 2026-08-07
doi: 10.2139/ssrn.6618279
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Currency momentum conditioned on where the buying pressure came from

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

Ryuta Sakemoto, Shintaro Suda. *Cross-sectional currency momentum and order flow **.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6618279`. <https://doi.org/10.2139/ssrn.6618279>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors sort currency portfolios jointly on
past returns and on order flow drawn separately from spot, forward and swap
instruments at several maturities, report that the effect of flow differs by
instrument and maturity, that pressure in short-dated swaps goes with stronger
momentum, that bank flow carries the effect, and that the double sort does best when
demand for dollar hedging is high.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6618279':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Momentum in currencies is a well-worn result and the contribution here is the
conditioner: not how much price has moved, but who moved it and in which instrument.
Flow through short-dated swaps is funding flow rather than view-taking, and the
claim that momentum is stronger when it is present says the trend is being pushed by
balance-sheet needs rather than by opinion. Who pays: the institution that must
roll a dollar hedge and cannot wait for a better price. That is the same
obligation-driven counterparty the Treasury auction candidates identify, which is
worth noticing — across three entirely separate literatures, the payer that keeps
paying is the one under a mandate.

## Signal in Crucible terms

- Not expressible, on two independent counts. Signed order flow segmented by
  counterparty type and instrument is a dataset this archive does not have at any
  level, and the sort is cross-sectional across many currencies while we hold one.
- The momentum half alone is expressible — a moving-average or z-score rule on 6E —
  but registering that here would be registering plain FX momentum under a citation
  about flow, which is the paraphrase creep §4 of the backlog README warns about.
  Wave 1's `trend-horizon` candidates already carry the momentum half honestly.
- Even the ES `tbbo`/`trades` window (D-0120) would not help: it is the wrong root,
  and it carries trade signs rather than counterparty identity.

## Data

- Owned: 6E `ohlcv-1m`, 149 contracts, 2010-06-06 → 2026-07-28.
- Not owned: order flow of any kind for 6E, and unobtainable (D-0120).
- Not owned: counterparty-segmented flow, which even a full L3 feed would not
  provide — it comes from dealer-reported data, not from the exchange.
- Not owned: any currency other than the euro.
- This is the deepest data gap in wave 2: three separate acquisitions, one of which
  is not sold by any exchange.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable, and unusually far from it.
- Registrable now: the observation that a momentum rule conditioned on *anything
  unobservable* becomes, in practice, a momentum rule with a free parameter. Any
  future FX candidate that claims a flow conditioner it cannot observe should be
  graded on the unconditioned rule, because that is what would actually run.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- A 2026 working paper, not refereed. Its data is proprietary dealer flow.
- The paper's own reported portfolio results are theirs and are not restated here.
- Cross-sectional currency momentum is a crowded literature with a long record of
  in-sample results, and a double sort adds a second selection step on top of the
  first.
- The candidate is included mainly to make the flow gap visible from a third
  direction. Wave 1 recorded it for ES order-flow imbalance; the auction candidate
  records it for ZN; this records it for FX. The same wall, three times, from three
  literatures — which is a more useful thing for a reader to see than one entry.

## Triage grade

**C.** C, and the missing pieces are **signed order flow segmented by counterparty**,
which no exchange sells, and **a currency cross-section**, which is a purchase. It
is the least reachable candidate in wave 2 and is listed to complete the picture of
the order-flow wall rather than as work anyone should plan.
