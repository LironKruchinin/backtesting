---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: london-newyork-gold-price-discovery
topic: metals-lease-rates-carry
grade: C
hypothesis_family: gc-venue-price-discovery
status: draft
blocked_on: a London spot gold series; the intraday half also needs multi-instrument configs
created: 2026-08-07
doi: 10.1002/fut.21775
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Which venue discovers the gold price, and how that share moves through the day

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

Martin Hauptfleisch, Tālis J. Putniņš, Brian M. Lucey. *Who Sets the Price of Gold? London or New York*.
Journal of Futures Markets, 2016.
DOI `10.1002/fut.21775`. <https://openalex.org/W2343583846>
Retrieved from the openalex API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: using seventeen years of intraday observations the
authors measure how much of gold price discovery happens on the London spot market
against the New York futures market, find the futures venue contributes more on
average despite being far smaller by volume, and report that the share varies with
the hour, with liquidity, and around macroeconomic releases.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.21775':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Price discovery shares are a statement about where information enters first, and
the interesting part of this result is that it does not follow volume. A tenth of
the turnover doing more than half of the discovering says the futures book
concentrates informed order flow — a market-structure claim, not a market claim.
Who pays: the venue that follows, and the participants who trade there on stale
prices during the hours when the lead is strongest. For Crucible the usable
consequence would be a lead-lag rule, and a lead-lag rule needs both legs.

## Signal in Crucible terms

- Not expressible. The London leg does not exist in this archive and cannot be
  synthesized from the futures leg — the whole point of the statistic is that the
  two are different series.
- Even with both legs, a discovery share is a pair statistic, and `combo` refuses a
  config declaring two instruments.
- The one piece that *is* expressible is the weakest and is not what this file
  registers: the hour-of-day variation, as a session-clock gate on GC alone. That
  measures when GC is active, not when GC leads, and wave 1's
  `metals-session-efficiency` already covers session-conditioned gold behaviour —
  so writing it here would duplicate that candidate rather than add to it.

## Data

- Owned: GC `ohlcv-1m` and `ohlcv-1s`, sixteen years, every contract. The New York
  futures leg is complete and at a finer grain than the paper used.
- Not owned: London spot gold, at any frequency. No plan acquires it and the
  archive's vendor does not carry it.
- Not owned: the macro release times the paper conditions on.
- Owned but load-bearing: the metals session calendar's RTH window is a cited
  convention rather than a measurement (CLAUDE.md §9), so any hour-of-day statement
  about gold inherits an assumption about when its "regular" hours are.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today. Without the second venue there is nothing to measure a
  share of.
- Registrable now is the structural point, which is worth keeping: **this archive
  can never answer a venue-comparison question about gold**, because it holds one
  venue. That is a different kind of block from a missing feature, and grouping it
  with the machinery gaps would overstate how reachable it is.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their sample is seventeen years of intraday London spot and COMEX futures;
  ours is COMEX alone. The comparison at the heart of the paper is unavailable by
  construction.
- The paper's own reported shares are theirs and are not restated here.
- This is a well-executed paper in a good journal, and it is graded C for a reason
  that has nothing to do with its quality — which is exactly the distinction
  `research/backlog/README.md` §2 makes about grades.
- The daylight-hours result is the one most likely to be an artifact of when each
  venue is open rather than of where information arrives, and the paper's own
  treatment of the platform upgrade suggests the authors knew that.

## Triage grade

**C.** C, and the missing piece is **a London spot gold series** — an acquisition outside
this archive's vendor. The intraday half additionally needs **multi-instrument
configs**. Unlike the curve candidates, no machinery in any milestone unblocks it.
