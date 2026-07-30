# Spike forensic — the unclassified QA finding, classified

**Date:** 2026-07-31. **Method:** read-only `crucible qa --timeframe 1m` over the
curated archive; no data purchased, nothing written outside the scratchpad.

The archive-wide QA sweep produced one warning class this project could not
classify, and CLAUDE.md's standing rule is that an unclassified warning class is
a finding rather than a footnote. This document classifies it.

## The finding

`qa` flags a bar as a **spike** when its bar-to-bar move exceeds `--spike-sigmas`
(default 8) times a **robust sigma computed over the contract's whole span**.
Counts ranged from 0 to 6,974 per contract, median ~249. ESH2020 was the worst
and is not a thin contract (119,468 bars), so "thin data makes sigma tiny" did
not explain it.

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

## What the count is actually measuring — and the §9-flagged proposal

The spike count is high not because the archive is bad but because **the
statistic answers a different question than `qa` asks.**

`qa`'s spike check exists to find **bad prints** — a price the market did not
trade. A bad print is implausible *relative to its neighbours*: a data error
does not know what the volatility regime is. The current statistic asks instead:
*is this bar's move large relative to the average of this contract's entire
life?* For a contract whose span contains both a crisis and years of calm, those
are very different questions:

- `ESH2020`'s sigma is **0.3707 pt**, computed over Jan-2019 → Mar-2020 and
  therefore dominated by calm 2019. Applying it to March 2020 flags the whole
  crisis.
- Every CL contract shares a sigma of **0.0148 pt** (≈1.5 ticks). An 8σ move is
  ~12 ticks. On 2020-04-21 crude moved dollars a minute.

### The proposal

Replace the full-span robust sigma with a **rolling (or era-aware) volatility
estimate**, so the null becomes *"given how this market has been moving lately,
is this move implausible?"* — which is the null the check was written for.

**The justification is not a smaller count, and this is the load-bearing part.**
A rolling sigma changes which bars are flagged **in both directions**:

- It *removes* the crisis-wide flags above, which are real volatility.
- It *adds* flags a full-span sigma currently **hides**: a bad print during a
  calm stretch of a contract whose sigma has been inflated by a volatile stretch
  elsewhere in its life. Today, ESH2020's 0.3707 pt sigma means a genuinely
  impossible 2-point jump in quiet July 2019 sits at 5.4σ and never appears.

That second effect is the reason to make the change. A change whose only
argument was "the number goes down" would be refused, per this project's own
rule — the argument here is that the current statistic is **blind in exactly the
place a data defect is most likely to hide.**

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
   a contract whose full-span sigma is inflated, and show the rolling estimate
   catches it while the full-span one does not. That is the claim this proposal
   rests on, and §7 gives it no quality exemption.
3. The window length declared and swept — the same demand D-0087 makes of the
   permutation null's block length, and for the same reason.
