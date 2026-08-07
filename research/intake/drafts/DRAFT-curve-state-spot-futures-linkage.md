---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: curve-state-spot-futures-linkage
topic: energy-roll-yield-timing
grade: C
hypothesis_family: metals-curve-state-linkage
status: draft
blocked_on: a curve-state classifier (two maturities) and a spot metals series
created: 2026-08-07
doi: 10.1002/fut.21736
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Whether the spot–futures link differs between contango and backwardation

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

Viviana Fernandez. *Spot and Futures Markets Linkages: Does Contango Differ from Backwardation?*.
Journal of Futures Markets, 2015.
DOI `10.1002/fut.21736`. <https://doi.org/10.1002/fut.21736>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: using two decades of London Metal Exchange
industrial-metal prices at the cash leg and at three deferred maturities, the
author asks whether the cash and deferred legs track each other more closely under
one curve state than the other, and concludes that the case for a tighter link in
contango does not hold up once the specification is varied.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.21736':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The argument being tested is an arbitrage argument: you can carry metal forward but
you cannot carry it backward, so when the curve is backwardated the trade that
enforces the link is infeasible and the two markets should decouple. That is a real
asymmetry with a named payer — whoever needs the physical metal now and cannot wait
pays the backwardation — and it predicts a regime-dependent correlation. The paper's
answer is essentially "not much", which is why this candidate is worth writing: a
published negative result on a mechanism that sounds compelling is a cheaper thing
to inherit than a positive one, because the failure mode of believing it is smaller.

## Signal in Crucible terms

- Not expressible. Classifying the curve as contango or backwardated requires two
  maturities and a comparison between them; a config sees one contract and cannot
  compare two prices it does not both hold.
- The spot leg does not exist in this archive at all for metals.
- What *is* expressible is a volatility-state or trend-state gate, which is a
  different conditioner entirely. Substituting one for the other would register a
  hypothesis nobody proposed.

## Data

- Owned: GC `ohlcv-1m` for 221 contracts, 2010-06-06 onwards. Every maturity, but no
  way to read two at once.
- Not owned: LME spot or any physical metals spot series, and no plan acquires one.
- Not owned: aluminium, copper, zinc, nickel — the paper's cross-section is
  industrial metals and ours is gold.
- Note the asymmetry with the previous candidate: there the data is fully owned and
  only the machinery is missing; here the spot leg is missing outright, so the two
  are C for different reasons and only one of them is unlocked by a curve reader.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today. The regime classifier is the hypothesis, and it cannot be
  computed.
- Registrable now: the curve-state definition is fixed before any run (which two
  maturities, which threshold separates the states, and what happens on the days
  the two disagree), and the negative result is the prior — a run that finds a
  strong contango effect on gold should be treated as a bug hunt before it is
  treated as a discovery, because it contradicts a refereed null on a larger
  cross-section.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their market is LME industrial metals with a spot leg; ours is COMEX gold futures
  with no spot leg. The instrument, the exchange and the storage economics all
  differ, and gold is the one metal whose storage story is least like the rest.
- The paper's own reported estimates are theirs and are not restated here.
- The paper is a null result after robustness checks, which is the most credible
  kind of published finding and the least likely to be exciting.
- Twenty years of daily LME data against sixteen years of CME minutes is not the
  same sample in any sense, and a replication here would be an analogy, not a
  replication.

## Triage grade

**C.** C, and there are **two** missing pieces rather than one: a **curve-state classifier**
built from two maturities of one root, and a **spot metals price series**, which is
an acquisition nobody has planned. The first is shared with the rest of the
term-structure bucket; the second is unique to this candidate and is why it stays C
even after a curve reader lands.
