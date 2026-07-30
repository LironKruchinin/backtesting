---
id: H-013
slug: volume-profile-value-area
topic: volume-structure
grade: C
hypothesis_family: es-value-area-open-location
status: backlog
created: 2026-07-30
---

# H-013 — Volume profile / value-area open location

## Citation

**No peer-reviewed citation was found for the predictive claims.** This section
records what was searched and what came back, because for this idea the state of
the evidence *is* the research finding.

**Origin, which is real and documented:** J. Peter Steidlmayer developed Market
Profile at the Chicago Board of Trade, released publicly in the mid-1980s,
organizing a session's trading into a distribution of Time Price Opportunities
(TPOs) and deriving a "value area" and a "point of control" from it. Volume
Profile is the later variant that weights by traded volume rather than by time.
The construct is genuine market history and is used by real desks.

**The predictive claims are another matter.** A search across arXiv q-fin,
Semantic Scholar and the general web for empirical evaluation of value-area or
point-of-control signals returned **trading-education sites, broker blogs, and
indicator vendors — and nothing refereed**.

Several of those pages attribute specific findings to authoritative-sounding
sources: a hit rate for "imbalanced profiles preceding trend continuation"
credited to the *Journal of Futures Markets* (2020), a statistic on time spent
within the value area credited to CME Group, and a claim about price velocity
through low-volume nodes credited to a 2019 Steidlmayer & Associates analysis.
**None of these was verifiable.** No article, volume, page number, or author was
given for the journal claim; the underlying documents were not located. Those
figures are therefore **not reproduced here and must not be cited from this
file**. A number without a retrievable source is a rumour (CLAUDE.md §2.5), and
it does not become one less so by having a journal's name next to it.

This is not a claim that no such literature exists. It is a record that a
reasonable search did not find it, which is the honest thing to write down.

## Mechanism

The underlying idea is coherent and worth stating properly, because it is
better than its evidence base. A session's traded volume, plotted against price
rather than against time, is a picture of where transactions actually happened —
where buyers and sellers repeatedly agreed. Prices with heavy volume are levels
the market has tested and accepted; prices with light volume were traversed
quickly because nobody wanted to transact there. The claim is that this
distribution has memory: a market re-entering a high-volume region finds
willing counterparties and stalls, while one entering a low-volume region moves
quickly through it because there is nothing to slow it down.

The "open location" variant — where today's open sits relative to yesterday's
value area — is the one this project already has in view (`docs/PROJECT_PLAN.md`
M2.5 exit). Its story is that opening outside the prior value area means
something re-priced overnight, and the session then either accepts the new level
or rotates back.

Who is on the losing side? The best available answer is the **participant who
must transact at a price the market has not recently accepted** — forced to
trade in a low-volume region and paying a wide spread for it. That is a real
cost borne by real people. But it is a story about *transaction costs at thin
prices*, and it does not obviously imply a *directional* edge, which is what a
value-area entry rule would need. The mechanism supports "price moves faster
through thin regions" much more comfortably than it supports "buy here, sell
there".

## Signal in Crucible terms

- **Basket:** ES first; the concept originates on CME futures and this is its
  home ground.
- **Timeframe:** session-level profile built from finer bars.
- **Feature 1 — the session volume-at-price histogram:** volume accumulated per
  price bucket over a session.
- **Feature 2 — value area:** the contiguous price region containing a
  pre-registered share of the session's volume, centred on the point of control
  (the highest-volume price). **The share must be fixed before any run** — the
  conventional 70 % is the pre-registration; treating it as a tunable parameter
  turns one hypothesis into a grid and must charge trials accordingly.
- **Feature 3 — open location:** where today's RTH open sits relative to
  yesterday's value area (inside, above, below), available at the open and never
  revised.
- **Use:** predictor-first. Does open location forecast the session's
  subsequent range, direction, or rotation back into the prior value area?

## Data

**This is where it becomes grade C.** A volume profile is a statement about
**volume at price**, and a 1-minute OHLC bar does not record volume at price —
it records one volume figure and four prices. Distributing a bar's volume across
its range is an assumption, and for a histogram it is a load-bearing one: the
point of control is by definition the *argmax* of that distribution, so the
approximation determines the answer. This is precisely the case CLAUDE.md §7's
"two things disagree" rule is about, and it is a sharper problem than the one in
H-012, where VWAP is a first moment and the same approximation is mild.

- **Owned, but wrong shape:** `ohlcv-1m` and `ohlcv-1s` for seven parents,
  2010-06-06 → 2026-07-28. The 1-second archive helps considerably — a 1-second
  bar's range is usually one or two ticks, so its volume is nearly
  unambiguously located — and this is the strongest argument that a decent
  profile is reachable without trade data.
- **Owned, right shape, far too short:** `trades` for **ES only,
  2025-07-28 → 2026-07-28**. Exact volume-at-price, for one instrument, for one
  year. Enough to *validate* a 1-second-derived profile against ground truth;
  not enough to run a sixteen-year study on.
- **Missing:**
  1. **Trade data for the other fifteen years** — a purchase, and a large one:
     `trades`/`tbbo` price at ≈ $28/GB and the twelve months we own for ES alone
     is 1.93 GB (`docs/DATA_PLAN.md`).
  2. **A histogram indicator**, session-anchored, plus point-of-control and
     value-area extraction. Substantially more machinery than any indicator we
     have.
  3. **Session anchors and volume operands** (shared with H-001, H-004, H-012).

## Pre-registered kill criteria

- **Gate −1 — validate the construction before believing any signal.** Build the
  profile from 1-second bars and, over the twelve months of ES `trades` we own,
  compare its point of control and value-area boundaries against the same
  quantities computed from actual trades. Pre-registered tolerance: the
  1-second-derived point of control must fall within **2 ticks** of the
  trade-derived one on at least **90 %** of sessions. If it does not, the
  approximation is not fit for purpose and **every downstream result is void** —
  not weakened, void. No signal test runs until this passes.
- **Gate 0 — predictor before system**, per `docs/PROJECT_PLAN.md` §7.3 and the
  M2.5 exit criterion:
  - Condition on open location (inside / above / below prior value area) and
    measure the distribution of the session's forward return and range.
  - The three buckets' forward-return distributions must be distinguishable at
    the **5 %** level under a block bootstrap (block = 20 sessions), with at
    least **250 sessions in each bucket**. Indistinguishable → **Kill**.
- **Gate 0b — the shuffle control, mandatory.** Recompute the value area from a
  **randomly permuted** assignment of volume to prices within each session and
  re-run Gate 0. If the permuted profile predicts as well as the real one, the
  signal is coming from the session's price *range*, not from its volume
  *distribution*, and the hypothesis is **Killed** — the range is a much simpler
  thing we could have measured directly.
- **Gate 1 — costs:** any tradeable version must clear
  `kill_if_dead_at_ticks = 1.0` and `min_oos_sharpe_after_costs = 0.5`.
- **Gate 2 — S3:** `max_pbo = 0.5`; `require_plateau = true` over the value-area
  share, if that is ever varied — and varying it charges trials.
- **The 70 % value-area share is pre-registered and frozen.** Any run at a
  different share is a new trial under this family, declared before it is run.

## Honesty note

- **This idea is a named target of the project plan** — M2.5's exit criterion is
  "one signal family (Volume Profile open-location) evaluated predictor-first,
  with a report worth sending to an external reviewer". The honest finding from
  this sweep is that **it enters that milestone with no refereed empirical
  support whatsoever**, and with its most-quoted supporting statistics
  untraceable to any locatable source. That does not make it a bad choice for
  M2.5 — an unexamined, widely-traded construct is a *better* subject for a
  falsification exercise than a well-studied one, and a rigorous negative result
  on it would be a genuinely novel contribution. But the report must open by
  saying the prior literature is absent, not by implying it is supportive.
- **The strongest reason for scepticism is that the mechanism does not predict
  direction.** Thin regions move fast; that is close to a definition. Getting
  from there to a profitable entry rule requires a step the folk version never
  takes.
- **The construction risk dominates the signal risk.** Gate −1 exists because
  almost every practitioner implementation builds profiles from minute bars
  without checking them against trades, and the point of control is an argmax —
  the least stable statistic one could pick.
- **We own the ground truth for exactly one year of one instrument.** That is a
  fortunate accident of the M4 calibration buy, and it is the single most
  valuable thing in this file: it converts "we assumed the approximation is
  fine" into a measurable claim.
- **No sample-overlap concern**, because there is no prior sample to overlap
  with.

## Triage grade

**C.** The faithful construction needs volume-at-price, and we own trade data
for one instrument for one year out of sixteen. The 1-second archive makes a
credible approximation *plausible* — but plausible is not established, which is
why Gate −1 comes before everything and why this is graded on the data we would
need rather than the data we might get away with. The 1-minute-approximated
version is a **B-grade descendant** and a different, weaker object; if anyone
runs it, it gets its own file rather than borrowing this one's grade.
