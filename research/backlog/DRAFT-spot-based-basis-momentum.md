---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: spot-based-basis-momentum
topic: futures-basis-cash-arbitrage
grade: C
hypothesis_family: commodity-basis-momentum
status: draft
blocked_on: a spot price series per commodity, and a two-maturity curve reader
created: 2026-08-07
doi: 10.2139/ssrn.6546878
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Basis and basis momentum measured against a real spot price

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

Zudong Luo, Shan Xue. *Spot-Based Basis and Basis Momentum in Commodity Futures Markets*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6546878`. <https://doi.org/10.2139/ssrn.6546878>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors argue that the usual practice of standing
in the nearest futures contract as a proxy for spot limits what the basis and basis
momentum signals can capture, construct alternatives from observed spot prices
alongside futures, test them on a large set of Chinese commodity futures, and report
that the spot-based versions retain predictive content after controlling for the
conventional ones.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6546878':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Basis is the price of storage and basis momentum is the change in that price, and
both are supposed to work because they reveal how tight the physical market is. The
payer is the party that needs the physical good sooner than the curve implies, and
it keeps paying because inventory is genuinely scarce. The paper's contribution is
a measurement point: if you use the front future as your spot, then your basis is a
calendar spread and it inherits the front contract's roll behaviour rather than the
physical market's tightness. That is a sharp and testable criticism, and it is
directly relevant to how any future Crucible basis feature should be defined — a
warning delivered before the feature is built, which is the best time to receive
one.

## Signal in Crucible terms

- Not expressible. Two maturities in one config, plus a spot leg we do not own.
- The paper's own point makes the cheaper substitute suspect in advance: the
  front-versus-second calendar spread is what a Crucible curve reader would most
  naturally compute, and this paper says that object is the weaker one. Registering
  the substitute would be registering the version the citation criticises.
- Basis momentum additionally needs a change over time in a quantity we cannot
  compute at one point in time.

## Data

- Owned: every maturity of CL and GC — 247 and 221 curated contracts. The futures
  legs are complete for both.
- Not owned: spot crude or spot gold from any source in the plan.
- Not built: a reader that holds two maturities of one root in a single run.
- The split is worth stating because it separates the two candidates in this topic:
  a curve reader alone gives the *conventional* basis, which is testable and which
  this paper says is the inferior measure; the spot leg is what gives the version the
  paper actually advocates.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now: when a curve reader lands, the basis definition is fixed in
  writing before any run — which two contracts, how the roll window is handled, and
  what happens when the second contract is illiquid. All three are choices that can
  be made after seeing results, and the paper's own argument shows how much the
  answer depends on them.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their market is Chinese commodity futures with a domestic spot market attached;
  ours is CME. The exchange, the participants and the spot-market institutions all
  differ, and Chinese commodity markets have position limits, price bands and a
  retail share with no CME counterpart — the same caveat wave 1's
  `commodity-night-vs-day-returns` records about a different Chinese-market paper.
- A 2026 working paper, not refereed.
- The paper's own reported results are theirs and are not restated here.
- Basis and basis momentum are cross-sectional factors in most of this literature,
  and this build has no cross-sectional portfolio accounting (§11, post-M4). The
  time-series version is a weaker relative of what the paper tests.

## Triage grade

**C.** C, and the missing pieces are **a spot price series per commodity** (an
acquisition) and **a two-maturity curve reader** (a build over owned data). It
belongs with the term-structure cluster, and it adds a design constraint to that
cluster rather than only a demand: whoever builds the curve reader should record
that the front-contract proxy is contested.
