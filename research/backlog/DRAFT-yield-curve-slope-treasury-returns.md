---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: yield-curve-slope-treasury-returns
topic: yield-curve-duration
grade: C
hypothesis_family: rates-curve-slope-predictability
status: draft
blocked_on: a second point on the yield curve — the archive's only rates root is ZN
created: 2026-08-07
doi: 10.2139/ssrn.7168358
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Whether the curve slope predicts bond returns, or the regression predicts itself

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

Andrea Berardi, Stephen M. Schaefer. *Does the slope of the yield curve actually predict Treasury returns?*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.7168358`. <https://doi.org/10.2139/ssrn.7168358>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors argue that small-sample bias affects both
constant-volatility term-structure model estimates and the regression coefficients
they are usually checked against, that the apparent agreement between the two is
partly mechanical, and that fitting a constant-volatility model to yields generated
under stochastic volatility leaves the implied coefficients substantially biased.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.7168358':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The slope of the yield curve predicting bond excess returns is one of the oldest
results in fixed income, and this paper is a methodological attack on the evidence
for it rather than an attack on the idea. That makes it the most Crucible-shaped
paper in wave 2: it is a claim that a widely-reported regularity is partly an
artifact of how it was estimated. There is no counterparty and no trade — the
finding is about inference. It belongs in this directory as a **prior**: any rates
candidate that arrives later claiming curve-slope predictability should be read
with the knowledge that the headline evidence has a documented small-sample
problem, which is the sort of thing a backlog exists to remember.

## Signal in Crucible terms

- Not expressible. A slope needs two maturities, and the archive holds exactly one
  rates root — ZN, the ten-year note. There is no two-year, no five-year, no bond.
- Unlike the commodity curve candidates, the missing maturities are **not** in the
  archive at all: for CL the deferred contracts exist and only the reader is
  missing, whereas here the instruments themselves were never bought. That is a
  materially different block and putting them in the same bucket would overstate
  what a curve reader unlocks.
- No expressible substitute. Trailing ZN returns are not a slope.

## Data

- Owned: ZN `ohlcv-1m`, 68 curated contracts, 2010-06-06 → 2026-07-28.
- Not owned: ZT, ZF, ZB, UB — the rest of the CME Treasury complex — and no cash
  yield series. `docs/DATA_PLAN.md` bought one rates root.
- Not owned: any zero-coupon curve, which is what the paper's estimation actually
  runs on.
- Worth noting for planning: adding one more rates root would make several rates
  hypotheses expressible at once, and it is a purchase rather than a build. That is
  a different lever from anything else in this index.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable. There is no slope to threshold.
- Registrable now is the prior, and it is the point of the file: **treat published
  curve-slope predictability as contested evidence rather than as a settled
  regularity**, and require any future rates candidate built on it to state which
  side of this methodological argument it is standing on.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- A 2026 working paper, not yet refereed, arguing against a large established
  literature. It should be read as one side of an open argument.
- The paper reports no strategy and no performance figures, so there is nothing to
  restate.
- Their object is yields and term-structure models; ours is a futures price series
  for one maturity. Even the dependent variable differs.
- The most useful thing in it for this project is not about bonds at all: a
  regression whose agreement with a model is partly mechanical is exactly the
  failure the permutation and truncation harnesses (D-0087, D-0088) exist to catch
  in our own results.

## Triage grade

**C.** C, and the missing piece is **a second point on the curve** — an instrument, not a
feature. It is the second candidate in wave 2 whose blocker is a purchase, and the
purchase is small: one more CME rates root.
