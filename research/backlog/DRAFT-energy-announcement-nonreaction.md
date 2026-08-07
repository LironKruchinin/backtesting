---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: energy-announcement-nonreaction
topic: announcement-drift-commodities
grade: C
hypothesis_family: energy-scheduled-release-nonreaction
status: draft
blocked_on: a macro announcement calendar with release timestamps and surprise values
created: 2026-08-07
doi: 10.1002/fut.21796
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — A published null: scheduled macro releases do not drive energy jumps

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

Kam Fong Chan, Philip Gray. *Do Scheduled Macroeconomic Announcements Influence Energy Price Jumps?*.
Journal of Futures Markets, 2016.
DOI `10.1002/fut.21796`. <https://doi.org/10.1002/fut.21796>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: across six energy futures markets the authors ask
whether large discrete price moves cluster at scheduled macroeconomic releases,
motivated by earlier work finding little announcement effect in average energy
returns, and report that they find no convincing increase in the rate at which
jumps arrive on release dates and no clear link between the size or the direction
of the surprise and the size of the jump.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.21796':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This candidate is registered for its **null**, which is the most useful thing a
paper can give a backlog. The prior it corrects is a strong one and it is
everywhere in the practitioner literature: that scheduled macro data moves oil, and
that a rule which stands aside — or leans in — around releases should therefore pay.
The paper says the mean effect was already known to be small and adds that the tail
effect is small too. If that holds, then whoever is on the losing side of an
energy release-window strategy is the strategy itself, paying spread for a
non-event twelve times a month. Recording that here is cheaper than discovering it
at S2, and it directly qualifies wave 1's `avoid-news-windows-gold`, which proposes
exactly the abstention intervention this paper's evidence argues would do nothing
in energy.

## Signal in Crucible terms

- Not expressible. Release timestamps are the entire conditioning variable and no
  operand names a date or an event.
- The jump-identification half *is* partly expressible — a trailing standardization
  of the bar return isolates outsized moves, which is what
  `extreme-move-reversal-cost-barrier` in this batch already registers. What cannot
  be written is the join to the release calendar, which is the paper's whole test.
- So the two halves separate cleanly: we can find the jumps and we cannot ask when
  they happened relative to anything scheduled.

## Data

- Owned: CL `ohlcv-1m` and `ohlcv-1s`, 247 curated contracts, 2010-06-06 →
  2026-07-28. The jump side is fully available and at a finer grain than the paper's.
- Not owned: the announcement calendar, the release times, or the consensus
  forecasts the surprises are measured against. All three are needed; a calendar
  alone tests arrival rates but not the surprise-magnitude claim.
- Not owned: natural gas, heating oil, gasoline — most of the paper's six markets.
  Ours is crude alone.
- The availability rule for a consensus forecast is its own problem: a survey median
  is published before the release, revised, and sometimes restated afterwards.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now, and this is the point of the file: **the null is the prior.** Any
  future energy release-window candidate must state that it is arguing against a
  refereed null on six markets, and a run that finds a large announcement effect in
  CL should be checked for a calendar-alignment bug — a timezone error that lands
  the window on the wrong minute produces exactly the shape a discovery would.
- If the calendar lands, the abstention arm and the participation arm are declared
  together, and the registered expectation is that neither beats the ungated rule.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Six energy futures markets over a sample ending before ours begins in earnest;
  we hold one of them. The instrument overlap is partial and the era overlap is
  small.
- The paper's own reported results are theirs and are not restated here.
- A null is easier to publish in a good journal than to overturn, and this one is in
  a good journal — but nulls also fail to replicate, and the paper's own framing
  says it is testing whether an already-weak mean effect hides in the tails. That is
  a low-powered question by construction.
- The honest asymmetry: this file makes it *harder* to justify a future energy
  event-window candidate, which is the direction a backlog entry should push when
  the evidence points that way.

## Triage grade

**C.** C, and the missing piece is the **macro announcement calendar with release
timestamps and surprise values** — the same M4 static CSV seven wave-1 candidates
wait on, plus the consensus series, which is a separate acquisition. It is listed
despite being unrunnable because its content is a prior that changes how three other
candidates should be read.
