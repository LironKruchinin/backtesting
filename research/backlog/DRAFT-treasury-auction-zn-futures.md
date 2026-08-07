---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: treasury-auction-zn-futures
topic: treasury-auction-cycle
grade: C
hypothesis_family: zn-auction-window-response
status: draft
blocked_on: a Treasury auction calendar (dates, times, and the bid-to-cover result)
created: 2026-08-07
doi: 10.1111/acfi.12635
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — What a Treasury auction does to the ten-year note future

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

Lee A. Smales. *The effect of treasury auctions on 10‐year Treasury note futures*.
Accounting &amp; Finance, 2020.
DOI `10.1111/acfi.12635`. <https://doi.org/10.1111/acfi.12635>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: working at a fifteen-minute grain over 2000–2017, the
author finds a measurable effect of US Treasury auctions on the ten-year note
futures market — higher prices, more volatility and more volume in the interval
straight after an auction — with stronger auction demand associated with positive
returns, and reads the pattern as dealers covering short futures hedges once the
issue is placed.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1111/acfi.12635':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This is the best-identified mechanism in wave 2 and it is on our own instrument.
A primary dealer must bid at auction and must hedge the inventory it is about to
take, and the cheapest hedge for a ten-year note is a short in the ten-year note
future. When the auction clears, the hedge is unwound — a buy — and the size and
timing of that buy are both known in advance to anyone with the auction schedule.
Who pays is unusually concrete: it is the dealer, and it is paying not because it
is wrong but because it is obliged. Obligation-driven flow is the one class of
counterparty that does not learn and go away, which is what separates this from
most of the harvest. The claim that the effect scales with auction demand is the
part that would need the result itself and not merely the date.

## Signal in Crucible terms

- Not expressible. An auction is an event at a wall-clock instant on an irregular
  set of dates, and no operand names a date. The session clock (D-0078) can say
  "how many minutes into the session", which would isolate the 13:00 ET result
  minute — but on every day, not on auction days.
- That partial construction is worth naming precisely because it is tempting and
  wrong: ten-year auctions happen roughly monthly, so a rule fired at the auction
  minute on every session would trade twenty non-events for every event, and would
  report the average of the two as if it were the effect.
- The bid-to-cover conditioner needs the auction *result*, which is a second data
  object beyond the calendar, published at the same instant.
- The direction is pre-specified by the paper — a buy after the clearing — so this
  is one of the few candidates where the eventual config would have a sign fixed in
  advance rather than a symmetric long/short pair.

## Data

- Owned: ZN `ohlcv-1m` and `ohlcv-1s`, 68 curated contracts, 2010-06-06 →
  2026-07-28. The paper worked at fifteen minutes; we hold one minute and one
  second, so our grain is finer than the evidence.
- Owned: a CME rates session calendar with eras (D-0089), including the measured
  fact that ZN's 16:00 CT prints are settlement prints rather than session minutes.
  An event-window study at a fixed clock position needs that distinction.
- Not owned: the auction calendar. Treasury publishes its schedule in advance and
  its results at a fixed instant, so this is a static CSV in the same class as the
  macro calendar — not a purchase, and not something any milestone has built.
- Not owned: bid-to-cover ratios, tail, and the when-issued yield the auction is
  compared against.
- Sample: their window is 2000–2017 and ours is 2010–2026, so the overlap is seven
  years and the recent decade — including the post-2020 supply expansion — is ours
  alone. That makes this one of the few candidates where our archive extends the
  paper's evidence rather than merely repeating part of it.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today, because the event dates are the hypothesis.
- Registrable now, and it matters that it is written before anyone looks: the
  window is declared in minutes around the result instant, once, for both arms; the
  auction *tenor* is fixed to the ten-year note rather than pooled across the curve;
  and the bid-to-cover split — if that data ever arrives — is declared as a median
  split, not as the threshold that separates the returns best.
- `min_oos_trades = 60` — basis: ten-year auctions are roughly monthly, so sixty
  events is five years. Below that the sample is a handful of days and no cost
  sweep can rescue it.
- `kill_if_dead_at_ticks = 1.0` — basis: ZN's tick is a thirty-second of a point and
  its quoted market is usually one tick wide, so a rule that cannot survive a full
  tick is not a rule. The assumed half spread (D-0120) is likely to be closest to
  right on ZN of any root here and still is not a measurement.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their instrument is ours, which is rare in this harvest and is the main reason
  this candidate is worth the calendar it needs.
- The paper's own reported effects are theirs and are not restated here, and none
  was produced under a fill model.
- The dealer-hedging explanation is inferred from the pattern rather than observed
  in positions; the paper does not see dealer inventory directly and neither would
  we.
- The post-2020 period is the obvious out-of-sample test and is also the period in
  which dealer balance-sheet capacity, the mechanism's premise, changed most. An
  effect that weakens there is evidence about the mechanism, not merely about the
  effect.
- `min_oos_sessions` is the wrong sample measure for this candidate and using it
  would flatter the result — the binding count is auctions, not sessions, and that
  is why the trade floor is registered instead.

## Triage grade

**C.** C, and the missing piece is a **Treasury auction calendar** — dates, tenors and the
result instant — with the bid-to-cover series as a second, separable acquisition.
Both are public and free. It joins the seven wave-1 candidates blocked on an event
calendar, and it is the strongest argument in either wave for building that CSV: the
instrument is owned, the grain is finer than the paper's, and the schedule is
published years ahead.
