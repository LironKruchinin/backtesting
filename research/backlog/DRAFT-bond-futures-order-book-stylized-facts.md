---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: bond-futures-order-book-stylized-facts
topic: limit-order-book-dynamics
grade: C
hypothesis_family: rates-order-book-state
status: draft
blocked_on: limit-order-book data for rates futures, which the archive does not hold and cannot acquire
created: 2026-08-07
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Order-book structure in bond futures, and what a bar series cannot see of it

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

Hamza Bodor, Laurent Carlier. *Stylized Facts and Market Microstructure: An In-Depth Exploration of German Bond Futures Market*.
arXiv q-fin, 2024.
**no DOI** (preprint). <http://arxiv.org/abs/2401.10722v1>
Retrieved from the arxiv API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors work through tick-by-tick book data for
four German government bond futures and document order sizes, flow patterns and
inter-arrival times, noting where the four contracts behave alike and where they
differ, and proposing the resulting measurements as realism benchmarks for market
simulators.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["title"].startswith('Stylized Facts and Market Microstructure'):
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

There is no trading claim here at all, and that is why it is worth registering.
The paper is a description of what the book of a rates future actually looks like,
and its usefulness to this project is as a **specification of what a bar series
throws away**. Every candidate in either wave that reasons about liquidity —
the reversal rules, the cost sweep, the half-spread assumption — is reasoning from
open, high, low, close and volume about a quantity that lives in the book. Knowing
the shape of the thing we cannot see is how a reader judges whether an OHLCV proxy
for it is defensible. Nobody is on the losing side of a description.

## Signal in Crucible terms

- Not a signal and not expressible. Every quantity in the paper — order size
  distributions, arrival times, book depth — is a message-level object, and the
  grammar's operands are four prices, a volume and seven clock readings.
- The nearest owned quantity is bar volume (D-0079), which is the sum of trade
  sizes over a minute and says nothing about resting depth, queue position or
  cancellation.
- Registering an OHLCV proxy for a book statistic and calling it a test of this
  paper would be the substitution this directory exists to refuse.

## Data

- Owned: ZN `ohlcv-1m` and `ohlcv-1s` over sixteen years, and one month of `mbo`
  for `ES.FUT` alone.
- Not owned: any book data for ZN, or for any root other than ES. D-0120 states the
  entitlement position; the practical consequence is that the archive's only
  message-level month is on the wrong instrument for this paper and too short for
  any of its statistics.
- Not owned: German government bond futures, which trade on Eurex and are outside
  this archive's vendor scope entirely.
- Note the double gap: the wrong exchange *and* the wrong data level.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable as a hypothesis. There is nothing to kill.
- What is worth recording is a **standing caution**: when a wave-1 or wave-2
  candidate justifies a cost assumption or a liquidity gate from bar data, this
  file is the reminder that the justified quantity is a book quantity and the
  justification is an analogy. The one-month ES `mbo` sample is the only place in
  this archive where such an analogy could ever be checked, and it can be checked
  for ES only.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Eurex bond futures, tick-by-tick book data, a recent sample. CME rates futures at
  one-minute bars over sixteen years is a different exchange, a different data
  level and a different contract.
- The paper reports no strategy and no performance, so there is nothing to restate.
- Its own stated purpose is benchmarking simulators, which is a use this project
  does not have. It is listed because the *negative* content — the enumeration of
  what a book contains — is directly relevant to how much this build can honestly
  claim about execution.
- Grading it C is almost a category error: it is not a strategy that costs data to
  test, it is a description of the data we do not have. Recording it under the same
  scheme keeps it visible to a future reader, which is worth the slight
  mis-fit.

## Triage grade

**C.** C, and the missing piece is **limit-order-book data for a rates future**, which for
this archive is unobtainable rather than unbought (D-0120) and would in any case be
a different exchange from the paper's. It is included as a named hole rather than as
a candidate to run: the scorecard renders missing pieces as named holes for the same
reason (CLAUDE.md §9).
