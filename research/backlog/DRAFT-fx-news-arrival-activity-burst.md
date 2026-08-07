---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: fx-news-arrival-activity-burst
topic: announcement-drift-commodities
grade: C
hypothesis_family: fx-release-window-activity
status: draft
blocked_on: a macro announcement calendar with release timestamps
created: 2026-08-07
doi: 10.1103/physreve.91.012819
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Trading activity in FX bursts at scheduled news, whether or not the price moves

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

Marcello Rambaldi, Paris Pennesi, Fabrizio Lillo. *Modeling FX market activity around macroeconomic news: a Hawkes process approach*.
arXiv q-fin, 2014.
DOI `10.1103/physreve.91.012819`. <http://arxiv.org/abs/1405.6047v2>
Retrieved from the arxiv API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors fit a self-exciting point process to
high-frequency foreign-exchange data with an added external term for pre-scheduled
macroeconomic releases, find the model captures the rise in activity after a
release both when the release moved volatility and when it did not, and extend it
to allow for the market knowing that a release is due without knowing its content.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1103/physreve.91.012819':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The distinction this paper draws is the one a naive event study misses: activity
and information are not the same thing. A scheduled release produces a burst of
trading even when the number is exactly what was forecast, because participants who
were waiting for the uncertainty to resolve act once it has, regardless of the
answer. Who pays: whoever is queued to trade in that burst and is not the reason for
it. For Crucible the interesting consequence is defensive rather than offensive —
a burst of activity with no information in it is a window in which spreads widen and
fills are worst, and a rule that trades through it is paying for nothing. The
anticipation extension is the sharper claim: the market changes behaviour *before*
the release, on the knowledge that one is coming, which is a genuinely available
signal in a way the content is not.

## Signal in Crucible terms

- Not expressible. The release schedule is the input.
- The pre-release half is the one worth wanting, because it needs only the schedule
  and not the content — no consensus series, no surprise measure. That makes it the
  cheapest calendar-dependent hypothesis in either wave.
- `volume` (D-0079) is the owned proxy for activity, and a trailing z-score of it is
  expressible, so "activity is unusually high right now" can be written. What cannot
  be written is "and a release is due in five minutes", which is the whole claim.
- A `zscore` of volume alone would register a hypothesis about activity bursts
  generally, which is a different and much weaker thing.

## Data

- Owned: 6E `ohlcv-1m` and `ohlcv-1s`, 149 curated contracts, and `volume` per bar.
- Not owned: the release calendar. For this candidate the times alone would suffice,
  which is worth recording because most calendar-blocked candidates need the
  contents too.
- Not owned: order arrivals. The paper's object is a point process over individual
  events and ours is a one-minute aggregate; a bar count is not an arrival rate, and
  the intensity dynamics the model is about are invisible at our grain even with a
  calendar.
- So this candidate has a data block and a *grain* block, and only the first is
  removable.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now: if the schedule arrives, the pre-release window is declared in
  minutes before any return is measured, and the registered claim is about **activity
  and cost**, not about direction. A file that quietly converts an activity result
  into a directional rule has changed hypotheses mid-stream.
- The natural first use is not a strategy at all: measure the realized cost of
  trading inside release windows against outside them, and feed that into the cost
  sweep's assumptions.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their data is high-frequency FX, effectively tick level; ours is one-minute bars
  on a euro futures contract. The model in the paper cannot be fitted to what we
  hold.
- The paper reports no strategy, so there is no performance figure to restate.
- It is a physics-journal paper about point-process modelling, not a trading paper,
  and its value here is conceptual: it separates activity from information, which is
  a distinction several other candidates in this index quietly conflate.
- The anticipation result is the one most likely to be fragile, since it depends on
  identifying behaviour changes before an event from a model fitted around it.

## Triage grade

**C.** C, and the missing piece is the **release calendar (times only)**, plus a grain the
archive does not hold for the arrival-process half. It is the cheapest calendar
dependency in either wave, which is worth noting when the CSV is eventually
scoped — a times-only calendar unlocks this and the abstention arms elsewhere, and
the consensus series is a separable second step.
