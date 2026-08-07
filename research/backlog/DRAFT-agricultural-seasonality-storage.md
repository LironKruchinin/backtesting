---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: agricultural-seasonality-storage
topic: commodity-seasonality-physical
grade: C
hypothesis_family: commodity-physical-seasonality
status: draft
blocked_on: an agricultural or refined-product root, and a calendar predicate for month-of-year
created: 2026-08-07
doi: 10.1002/fut.10017
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Deterministic seasonal components in commodity futures curves

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

Carsten Sørensen. *Modeling seasonality in agricultural commodity futures*.
Journal of Futures Markets, 2002.
DOI `10.1002/fut.10017`. <https://doi.org/10.1002/fut.10017>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author models log commodity prices as a
deterministic seasonal term plus a non-stationary and a stationary state variable,
fits the model to corn, soybean and wheat futures term structures by Kalman filter
over 1972–1997, and reads the fitted convenience yields as evidence for the negative
inventory–convenience-yield relation the theory of storage predicts.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.10017':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Physical seasonality is the one commodity effect with a mechanism nobody has to
argue for: grain is harvested on a schedule, heating oil is burned in winter,
gasoline is burned in summer, and storage costs money in between. The payer is the
producer who must sell into the harvest and the consumer who must buy into the
season, and neither can wait. That makes it the most credible mechanism in this
whole harvest — **and it is the one that most clearly does not apply to what this
archive holds.** Crude is the raw input, not the seasonal product; gold has no
season; a currency and a Treasury note have none either. This candidate exists
mainly to record that finding rather than to propose a run.

## Signal in Crucible terms

- Not expressible on any owned root, and not for a machinery reason. There is no
  agricultural contract and no refined product in the archive.
- Month-of-year is not an operand, so even on a root with a season the annual index
  could not be written — the same calendar-predicate gap that blocks day-of-week.
- The paper's own construction is a state-space model fitted to a term structure,
  which needs a curve reader as well.
- Three independent blockers stack here, and the first one — the instrument — is the
  only one that costs money to remove.

## Data

- Owned: CL, GC, 6E, ZN, ES, NQ, RTY. None of them is seasonal in the physical sense
  the paper means.
- Not owned: corn, soybeans, wheat — the paper's own instruments — and not owned:
  RB and HO, the refined products where crude's demand season actually shows up. CL
  is the crude, and the driving season is a *product* story.
- Not owned: natural gas, the one energy contract with an unambiguous weather season.
- `docs/DATA_PLAN.md` acquires none of these and no milestone plans to.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable, and this one is not close. Registering criteria for an instrument
  the archive does not hold would be a file that can never be judged.
- What is worth registering is the negative finding itself: **the physical-seasonality
  literature is about instruments this archive does not contain**, and a future
  session tempted to look for a driving-season effect in CL should read this line
  first and go and look at RB instead.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their sample is CBOT grains, 1972–1997, weekly. Ours is CME energy, metals, FX,
  rates and equity index, 2010–2026, at one-minute grain. There is no overlap in
  instrument, era or frequency.
- The paper's own reported estimates are theirs and are not restated here.
- A seasonal pattern found in CL by searching for one would be a month-of-year effect
  fitted on sixteen observations per month, which is the sample size at which
  anything can be found.
- The theory-of-storage half of the paper *is* relevant to CL and GC, but it is
  relevant through inventories and the curve, which are the two things the
  term-structure candidates in this batch are already blocked on.

## Triage grade

**C.** C, and the missing piece is **an instrument, not a feature**: an agricultural or
refined-product root. That makes it the only candidate in wave 2 whose blocker is a
purchase rather than code, and it is worth keeping visible for exactly that reason —
the rest of the C column would be unblocked by machinery, and this one would not.
