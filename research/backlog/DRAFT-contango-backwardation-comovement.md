---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: contango-backwardation-comovement
topic: energy-roll-yield-timing
grade: C
hypothesis_family: commodity-curve-slope-comovement
status: draft
blocked_on: a curve-slope feature (two maturities of one root) and multi-root configs
created: 2026-08-07
doi: 10.1002/fut.70092
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Curve slopes move together across commodities, and gold moves against them

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

Angelo Luisi, Francesco Roccazzella, Athanasios Triantafyllou. *On the Comovement of Contango and Backwardation Across Futures Commodity Markets*.
Journal of Futures Markets, 2026.
DOI `10.1002/fut.70092`. <https://doi.org/10.1002/fut.70092>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors model the slope of the futures curve
across agricultural, metals and energy markets jointly and report that the slopes
co-move, that the co-movement strengthens in periods of stress, and that gold's
slope tends to move opposite to the rest.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.70092':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The slope of a futures curve is a price of storage, and storage is priced by the
same balance sheets across commodities: when funding is dear, everybody's carry
gets dear together. That is a credible reason for slopes to co-move and it names
the payer — whoever needs to roll a position while the cost of carrying inventory
is rising is paying the constraint of somebody else's balance sheet. The gold
result is the part worth taking seriously and the part hardest to trust: gold's
above-ground stock is enormous relative to flow, so its "storage" is nothing like
crude's, and a slope that behaves in the opposite direction is what a monetary
asset masquerading as a commodity would look like. Whether that survives out of
sample is exactly the question, and it cannot be asked here yet.

## Signal in Crucible terms

- Not expressible. The unit of analysis is a *slope*, which is a relation between
  two maturities of one root, and a config replays one contract.
- Even with a slope, the claim is about the co-movement of several roots, which
  needs several instruments in one run, which `combo` refuses by design.
- No arithmetic between operands, so a difference of two prices cannot be written
  even if both were visible.
- The gold-versus-the-rest asymmetry is the most interesting testable piece and is
  the furthest out of reach: it is a statement about two curves at once.

## Data

- Owned, and this is the point worth recording: **every maturity of every root is
  in the archive.** CL has 247 curated contracts, GC 221, 6E 149, ZN 68. The data
  a curve needs is bought, transcoded and on disk.
- Not owned: any object that reads several of them together. `curated/rolls/`
  holds roll tables, which pick one contract at a time; there is no forward-curve
  reader and no config shape that would declare one.
- Not owned: agricultural futures. The paper's cross-section includes them and
  ours cannot.
- Constraint: `combo` and `walk-forward` replay raw contracts only, so even a
  stitched series would not help here.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today, for the same reason as any curve hypothesis: the feature
  does not exist, so a threshold on it would be a number about nothing.
- Registrable now is the shape: the two maturities are named before the run (front
  and second-nearby, not "whichever pair worked"); the stress periods are declared
  by a rule on trailing realized volatility rather than picked by eye; and the gold
  asymmetry is stated as a directional prediction before the sign is looked at, or
  it is not a test of the paper.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their sample spans agricultural, metals and energy markets in a joint model; ours
  is four commodity roots and no agriculturals, so a co-movement statistic computed
  here would be a different statistic.
- The paper's own reported estimates are theirs and are not restated here.
- The stress-period result is the classic shape of a finding that is real and
  useless: correlations rise in crises, which is when a position cannot be exited
  cheaply, so the effect and the cost of exploiting it arrive together.
- Nothing in this archive would let a run distinguish a slope that moved because
  storage repriced from a slope that moved because the front contract was rolling.

## Triage grade

**C.** C, and the missing piece is a **curve-slope feature — two maturities of one root
read in one config** — followed by **multi-root configs** for the co-movement half.
The first is machinery over data we fully own, which puts it in the same bucket as
wave 1's `wti-term-structure-forecast` and `equilibrium-forward-curves`; that bucket
is now the largest single unlock the commodity seams are waiting on.
