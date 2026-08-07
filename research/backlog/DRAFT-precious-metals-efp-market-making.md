---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: precious-metals-efp-market-making
topic: liquidity-provision-market-making
grade: C
hypothesis_family: metals-efp-spread-dynamics
status: draft
blocked_on: a spot metals series and quote-level data; the archive holds neither for GC
created: 2026-08-07
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The futures-to-spot spread in precious metals as a mean-reverting object

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

Alexander Barzykin, Philippe Bergault, Olivier Guéant. *Market Making in Spot Precious Metals*.
arXiv q-fin, 2024.
**no DOI** (preprint). <http://arxiv.org/abs/2404.15478v5>
Retrieved from the arxiv API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors model the exchange-for-physical spread
between precious-metals futures and spot with a nested mean-reverting process,
motivated by the observation that liquidity in the sector sits mainly in the
futures, and use it to solve a market-maker's inventory problem across both legs.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["title"].startswith('Market Making in Spot Precious Metals'):
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The interesting claim for this project is not the control problem, it is the
premise: the spread between the futures and the spot metal relaxes at several
speeds at once, because participants with different horizons close it at different
rates. If true, the spread is a genuinely mean-reverting series with a structural
reason to revert — arbitrage between two claims on the same metal — rather than a
statistical one. Who pays: whoever needs immediacy in the less liquid leg. That is
about as solid a mechanism as this harvest contains, and it is entirely unavailable
to us, because we hold one of the two legs.

## Signal in Crucible terms

- Not expressible. The object is a spread between two series and we have one of
  them. There is no operand for the other and no arithmetic to difference them
  with even if there were.
- The market-making application needs quote-level data and a resting-order model;
  the engine's fill models are `free_fills` and `spread_cross`, both of which cross
  the spread rather than post inside it. Modelling a liquidity *provider* would need
  a queue simulator, which §4 names as `queue_sim` and places in M4.
- Two blocks of different kinds, and the milestone one is real: even with both
  price legs, this build cannot represent the position the paper is optimising.

## Data

- Owned: GC `ohlcv-1m` and `ohlcv-1s`, 221 contracts.
- Not owned: spot gold or silver, at any frequency, from any source in the plan.
- Not owned: quotes for GC, and unobtainable (D-0120).
- Not owned: silver, platinum and palladium futures — the paper's sector is wider
  than our one metal.
- Not built: `queue_sim`. It is a named fill model with a milestone, not a gap
  discovered here.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable. The strategy class — posting rather than crossing — is outside
  what this build can score, so a threshold on it would be a number about a
  simulation that does not exist.
- Registrable now: when `queue_sim` lands, a passive strategy must be judged
  against the **same** cost sweep as an aggressive one, and its fill assumptions
  named as loudly (§2.4). A market-making result reported without a queue model is
  the anonymous-default-execution failure §2.4 exists to prevent.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Spot precious metals against futures, with quote-level modelling; we hold futures
  bars only. The overlap is one leg at a coarser grain.
- The paper reports no backtested performance to restate; it is a stochastic control
  paper.
- Its authors are established in this area and the modelling is serious. The grade
  is about our distance from it, not its quality.
- The nested mean-reversion premise is the part most worth remembering, because it
  suggests the EFP spread would be a good candidate *if* the spot leg ever arrived —
  and a bad one to approximate with anything else.

## Triage grade

**C.** C, and the missing pieces are **a spot metals series**, **quote-level data for GC**
(unobtainable, D-0120) and the **`queue_sim` fill model** (M4). Three blockers, one
of which is a milestone rather than a gap — which is why it is graded C rather than
B despite the futures leg being fully owned.
