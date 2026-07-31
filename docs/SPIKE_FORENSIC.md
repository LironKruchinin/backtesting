# Spike forensic — the unclassified QA finding, classified

**Date:** 2026-07-31. **Method:** read-only `crucible qa --timeframe 1m` over the
curated archive; no data purchased, nothing written outside the scratchpad.
**Revised 2026-07-31** after a full 863-contract sigma sweep: the *verdict* below
stands unchanged and the *mechanism* originally given for it was wrong. The
superseded text is not softened, it is struck — see "What the count is actually
measuring".

The archive-wide QA sweep produced one warning class this project could not
classify, and CLAUDE.md's standing rule is that an unclassified warning class is
a finding rather than a footnote. This document classifies it.

## The finding

`qa` flags a bar as a **spike** when its bar-to-bar move exceeds `--spike-sigmas`
(default 8) times a **robust sigma computed over the contract's whole span**.
Counts ranged from 0 to 6,974 per contract, median ~249. ESH2020 was the worst
and is not a thin contract (119,468 bars), so "thin data makes sigma tiny" did
not explain it.

### Those counts are drawn from an undeclared subsample

**91 of 863 contracts produce no sigma at all**, and when that happens `qa`
prints **no `spikes` line whatsoever** — not "0 spikes", nothing. An automated
extraction records the missing field as zero, so a contract whose spike check
never ran is indistinguishable from one that ran and found nothing. That is how
this escaped every prior sweep, including the one that produced the range and
median above.

Two different `return`s produce it, and only one is substantive:

| cause | contracts | what they are |
|---|---|---|
| `mad <= 0.0` — over half the bars did not move | **47** | 44 ZN (67k–102k bars each) + CLZ2029/2030/2031 |
| `moves.len() < 3` — too few adjacent pairs | 16 | ≤3 bars, deep-deferred 2027–2036 |
| either, indistinguishable from outside | 28 | 4–999 bars, all deep-deferred |

**44 of 68 ZN contracts — 65 % of the root — have no spike check**, because a
1/64 tick and a quiet 1-minute bar mean the *median* absolute move is exactly
zero. In total **4,002,334 of 70,641,676 curated bars (5.7 %)** sit under a
detector that never ran and never said so. An absent detector rendering as a
clean result is the failure §9 exists to name, and it is the most actionable
thing in this document.

## Verdict: REAL VOLATILITY, in every cluster examined

Six contracts, the worst offender plus the next five. For each, the months the
top-20 spikes fall in, and what was happening.

| contract | spikes | top-20 concentration | the event |
|---|---|---|---|
| **ESH2020** | 6,974 | **all 20** in 2020-02/03 | COVID crash |
| **CLZ2017** | 6,466 | **14 of 20** on 2012-09 | 2012-09-17 crude flash break |
| **CLZ2018** | 4,072 | scattered 2011–2015 | — (see below) |
| **CLN2016** | 3,817 | **13 of 20** in 2015-02 | Feb-2015 oil rebound |
| **CLN2014** | 3,594 | **13 of 20** on 2013-06 | 2013-06-20, taper-tantrum aftermath |
| **CLM2020** | 842 | **14 of 20** in 2020-04 | 2020-04-20/21, negative WTI |

**Four corroborations that settle it**, beyond the clustering:

1. **`ESH2020 2020-03-03T15:00Z +67.75 pt`** lands on the *minute* of the Fed's
   emergency 50 bp inter-meeting cut (15:00 UTC = 10:00 ET, 2020-03-03). A data
   defect does not schedule itself for an FOMC announcement.
2. **`ESH2020 2020-03-15T22:01Z` and `2020-03-16T22:01Z`** are both **one minute
   after the 17:00 CT session open**. 2020-03-15 was a Sunday; ES gapped and hit
   limit-down within a minute of the reopen. The timestamps are the reopen, not
   a random instant.
3. **The circuit-breaker days are all present** — 2020-03-09, 03-12, 03-16,
   03-18, the four level-1 halts — and so are their reopen whipsaws
   (`03-17T19:59 +45.25` then `20:00 −36.50`, consecutive minutes).
4. **`CLM2020` clusters on 2020-04-21**, the day after WTI settled at −$37.63 —
   an event this repository *already documents* in CLAUDE.md §9 (D-0070), where
   it is the reason the price-validity test is `!= UNDEF_PRICE` and never `> 0`.

**The isolated-print test the forensic was asked for.** A bad print is a lone
bar in an otherwise ordinary stretch that fully reverses. What these are instead
is **14 of one contract's 20 largest moves landing on a single date** — that is a
regime, not an error. The consecutive-minute reversals (−2.26 then +2.21 on
`CLZ2017 2012-09-17T17:59/18:00`) look like the isolated-reversal signature in
miniature, but they sit inside a cluster of thirteen others on the same day and
at a halt boundary, which is reopen behaviour.

**CLZ2018 is the honest exception** and is reported as such: its top-20 is
scattered across 2011–2015 with no dominant cluster. It is a *deferred* contract
— a December-2018 contract trading in 2011 — so its book is thin and a handful
of lots moves it. Those prints are almost certainly real too, but "almost
certainly" is not the standard used for the other five, so it is recorded as
**consistent with thin-book trading, not independently corroborated**.

## What the count is actually measuring

**The mechanism this document originally gave was wrong.** Three claims are
struck outright rather than softened, because each was load-bearing and each is
refuted by measurement:

1. ~~"Every CL contract shares a sigma of 0.0148 pt."~~ **Refuted.** CL takes at
   least eight values across 221 contracts; only 42 read 0.0148. CLM2020 reads
   0.0297.
2. ~~"ESH2020's sigma is 0.3707 pt, computed over Jan-2019 → Mar-2020 and
   therefore dominated by calm 2019."~~ **Refuted.** 0.3707 is *identical* on
   ESH2011, ESM2013, ESU2016, ESZ2018, ESH2020, ESM2021 and ESU2023. It carries
   no information about 2019 or about any other year. Nothing is "dominated by"
   anything.
3. ~~The framing that the defect is **staleness** — one sigma over a whole life
   conflating regimes.~~ **Refuted.** The defect is **RESOLUTION.**

### The measured mechanism

`sigma = 1.4826 × median(|Δclose|)` over adjacent same-interval bars. Every price
is an integer multiple of the tick, so every `Δclose` is too — and the median of
a set of integer multiples is itself an **exact integer multiple of the tick**.
The estimator cannot return anything else. Measured over **all 863 curated
contracts**: **43 distinct sigma values in the entire archive**, every one an
integer number of ticks.

| root | MAD in ticks (distinct values observed) | tick |
|---|---|---|
| **ES** | **1** (×44), 2 (×13), 3 (×9), 7 (×1) | 0.25 |
| NQ | 1 … 14, 17, 23, 43 | 0.25 |
| CL | 1 (×42), **2** (×128), 3 (×34), 4, 5, 6, 10, 11 | 0.01 |
| GC | 1 (×24), **2** (×68), 3 (×50), … up to 20 | 0.10 |
| RTY | 2 (×14), 3 (×12), 4, 5, 6, 12 — never 1 | 0.10 |
| ZN | 1 (×23); the other 44 have **no sigma at all** | 1/64 |
| 6E | 1, 2, 3, 4 (seen through 4-decimal print rounding) | 0.00005 |

So the statistic **is** a volatility estimate — GC spans 1→20 ticks, NQ 1→43 —
but one quantised to integer ticks and floored at one. It has **no resolution
between adjacent integers**: a 40 % change in volatility is invisible, and below
one tick it cannot go at all.

For **ES the floor binds on 44 of 67 contracts, spanning 2011 → 2023 and
including ESH2020 across COVID** — for every one of those the 8σ gate is a
constant **2.9656 points**, whatever the market was doing. The other 23 ES
contracts read 2, 3 or 7 ticks, so "constant across the whole archive" would
overstate it; "constant across two thirds of it, including the crisis contract
this document is about" is what was measured.

The arithmetic that made ESH2020 look regime-conflated still holds — a 2-point
jump in quiet July 2019 sits at 5.4σ and never appears — but the **reason** is
that 8σ is pinned to the tick grid, not that a calm year diluted a crisis.

### Why this changes the proposal

The originally proposed repair — "replace the full-span sigma with a rolling
one" — **would not fix ES**, because a rolling median of ES 1-minute moves is
one tick in essentially every window too. A rolling *median* saturates harder
than a full-span one (fewer samples, more ties at the floor) and reaches the
`mad <= 0.0` case far more often — a case that already fires at full span on 47
contracts. Any estimator built from an **order statistic** of a lattice-valued
variable inherits the lattice. The repair has to escape it, not re-enter it at a
shorter window.

That design is being planned separately and deliberately is **not** specified
here, because this document's job is to record what was measured.

### Status: PROPOSED, NOT IMPLEMENTED — and the proposal is incomplete

**The new count has not been measured.** The rule requires old and new counts
reported together, and only the old ones exist:

| contract | old count (full-span sigma) | new count (rolling) |
|---|---|---|
| ESH2020 | 6,974 | **not measured** |
| CLZ2017 | 6,466 | **not measured** |
| CLZ2018 | 4,072 | **not measured** |
| CLN2016 | 3,817 | **not measured** |
| CLN2014 | 3,594 | **not measured** |
| CLM2020 | 842 | **not measured** |

Until both columns are filled the proposal is not actionable, and it is recorded
here in that state deliberately rather than implemented on the strength of a
plausible argument. Whoever implements it owes:

1. Both counts, per contract, on the six above.
2. A **planted control**: inject a known-impossible print into a calm stretch of
   a contract whose sigma is pinned to the tick floor, and show the new estimate
   catches it while the current one does not. That is the claim this proposal
   rests on, and §7 gives it no quality exemption.
3. Every parameter declared and swept — the same demand D-0087 makes of the
   permutation null's block length, and for the same reason.
4. **A converse control written first**: real crisis volatility must survive.
   An estimator that flags nothing passes item 2 perfectly.
5. **The zero-scale case answered explicitly**, because it is not hypothetical:
   47 contracts, 44 of them ZN, already produce no sigma at full span. Whatever
   replaces the median must either return a positive scale for them or say —
   out loud, with a count — that it could not. Silence is what is being
   repaired.

### The separate, smaller repair that does not wait for any of this

`qa` should print the spike line **even when it cannot compute a sigma**, saying
so and counting the bars it skipped. Today the line is simply absent, which is
why 5.7 % of the archive has been reported as clean without being examined.
That is a reporting fix, not a statistical one, and it is independent of every
open question above.
