---
id: H-004
slug: gap-volume-regime-classifier
topic: intraday-session
grade: B
hypothesis_family: es-gap-volume-regime
status: backlog
created: 2026-07-30
---

# H-004 — Overnight gap × opening volume as a regime *conditioner*

## Citation

Mathias Mesfin, **"A Validated Volatility-Volume-Gap Classifier for Regime
Identification in MNQ Intraday Data"**, arXiv:2605.11423v2 (published
2026-05-12).

- <https://arxiv.org/abs/2605.11423>

Their stated claim: a framework using the **overnight gap, the first 30-minute
return, and volume** identifies distinct intraday regimes in Micro E-mini
Nasdaq futures, but — in the authors' own words — the classification **"does
not translate into a robust standalone trading signal"**.

## Mechanism

The claim being made is narrower than a strategy and should be read that way: a
trading session is not drawn from one distribution. A session that opens with a
large overnight gap on heavy volume is a different object from one that opens
flat and quiet — the first is a session where positioning changed while the
market was closed and participants are re-pricing against unfamiliar levels;
the second is one where yesterday's balance still holds. Whoever is on the
losing side differs by regime, which is precisely why no single rule extracts
money from both. On gap-and-heavy-volume days the loser is the participant
forced to re-hedge into a level nobody has traded recently, and liquidity is
thin because market makers widen when they cannot infer fair value; on quiet
days the loser is whoever pays the spread for impatience, and there is far less
of that to collect. A classifier does not earn anything by itself. It earns by
telling a *different* strategy when its own mechanism is present.

This is why the file exists. H-001 reports its effect is stronger on volatile
days, high-volume days and macro-news days — a conditioner is exactly the
machinery that claim needs, and building it once serves the whole backlog.

## Signal in Crucible terms

Not an entry rule. A **feature that other hypotheses consume**, plus a
reporting slice.

- **Basket:** ES primary; NQ and RTY for the rhyme check; CL and 6E to test
  whether the regime structure is equity-specific.
- **Timeframe:** 1m bars (owned), with session-relative windows.
- **Feature 1 — overnight gap:** today's RTH open ÷ previous RTH close.
  Normalized by a trailing realized-volatility estimate so "large" means large
  *for this regime*, computed only from data available at the open (CLAUDE.md
  §2.1 — a full-sample gap quantile is lookahead and is the single easiest way
  to fake this result).
- **Feature 2 — opening volume:** volume in the first 30 RTH minutes relative
  to a trailing median of the same window.
- **Feature 3 — first 30-minute return** (shared with H-001).
- **Output:** a discrete regime label per session, available at RTH open + 30
  minutes and **never revised**.
- **Use:** a per-regime slice on every scorecard, and a pre-registered
  conditioning axis for H-001, H-008 and H-013.

## Data

**Owned, sufficient:** `ohlcv-1m` for all seven parents, 2010-06-06 →
2026-07-28. Bars carry `volume: u64`, so feature 2 needs no new data.

**Missing — all code:**
1. **Volume as a rule operand.** `Bar.volume` exists and reaches the engine;
   the combo grammar simply has no `volume` operand
   (`research/backlog/README.md` §2.1).
2. **Time-of-day predicates and session anchors** (shared with H-001, H-002).
3. **A rolling normalizer.** Both features are ratios against a trailing
   statistic, and CLAUDE.md §2.1 is explicit that a full-sample z-score or
   quantile is lookahead. Rolling-only standardization is also the M2.5
   point-in-time rule (`docs/PROJECT_PLAN.md`), so this belongs to that layer.
4. **Per-regime reporting slices.**

No purchase required.

## Pre-registered kill criteria

The paper already reports the standalone-signal version fails. Re-testing it as
a standalone strategy would be re-running someone else's null, so this file
pre-registers the **conditioner** claim instead — which is the part they did
not test.

- **Sample minimum:** **1,500 sessions**, and **at least 200 sessions in every
  regime bucket**. A regime that is rare is a regime we cannot evaluate; if any
  bucket is under 200, the labelling is **rejected and re-specified** with
  fewer buckets, before any strategy is conditioned on it. Re-specifying after
  seeing strategy results is forbidden under this family key.
- **Stability, judged first:** regime labels must be assigned by a rule fit on
  a rolling trailing window only. A labelling that cannot be computed at RTH
  open + 30 min from prior data is **Kill**, no appeal — that is lookahead, not
  a modelling choice.
- **The conditioner earns its keep only if it separates.** For at least one
  hypothesis in this backlog, the difference in out-of-sample performance
  between the best and worst regime bucket must be significant at the **5 %**
  level under a block bootstrap (block = 20 sessions) **and** must hold in the
  same direction on at least **2 of 3** of ES/NQ/RTY. Otherwise **Kill**: an
  unconditional strategy is simpler and simpler wins ties.
- **The multiple-comparison charge is explicit.** Conditioning an existing
  hypothesis on `k` regime buckets multiplies that hypothesis's trial count by
  `k`, charged to *its* family, not to this one. A conditioned result that
  survives raw but not deflated is a **Kill**.
- **Standalone use is banned under this key.** If someone wants to trade the
  regime label directly, that is a new file and a new family — the authors
  already report it does not work, and re-running it under a conditioner's key
  would launder the trial count.

## Honesty note

- **The source is a single-author, non-peer-reviewed preprint**, and it is the
  same author as H-003. Two files in this sweep resting on one unrefereed
  author is a concentration worth naming. What makes both worth keeping is that
  both *report nulls* — this one explicitly states its classifier does not
  produce a robust standalone signal, which is a claim nobody has an incentive
  to overstate.
- **We do not own MNQ; we own NQ and ES.** Same index for the Nasdaq leg, one
  tenth the notional. See H-003 for why a points-denominated comparison
  transfers and a depth-denominated one does not.
- **"Regime" is the most over-fitted word in this backlog.** Any partition of
  sessions into buckets, chosen after looking at returns, will produce buckets
  that differ in returns. The 200-session minimum, the rolling-only labelling
  rule, and the ban on re-specifying after seeing results are all aimed at that
  single failure mode, and they are still probably not enough. Treat a positive
  result here with more suspicion than a positive result anywhere else in this
  directory.
- **Their sample and ours:** their MNQ study period is short (MNQ launched in
  2019); ours is 2010–2026 on the full-size contracts. Little overlap, which is
  good, but it also means we cannot check our labelling against theirs on
  common data.
- **The gap feature is partly a proxy for scheduled macro events.** Large ES
  overnight gaps cluster on FOMC, CPI and NFP mornings. Until the M4 macro
  calendar exists we cannot separate "gap regime" from "there was a number at
  08:30", and any result here should be read as containing both.

## Triage grade

**B.** Data fully owned, including the volume the classifier needs. The gaps
are a volume operand, session-relative windows, and a rolling normalizer —
the last of which is M2.5's point-in-time standardization layer and is needed
by half this backlog regardless.

---

## Changelog

Append-only. The registration above is never rewritten — a pre-registration
that gets edited after the fact is not one (README §1).

### 2026-07-30 — re-graded against the four grammar unlocks (D-0077…D-0080): **B → B**

**What closed.** All three pieces this file named as its gaps now exist in some
form: the volume operand (D-0079), session-relative windows (D-0078) and a
rolling normalizer (D-0080). The grade still does not move, and the reason is
worth stating precisely, because "the thing I asked for landed" and "my feature
is now writable" turned out to be different claims.

**What still blocks — feature by feature.**

- **Feature 1, the overnight gap** (`today's RTH open ÷ previous RTH close`)
  needs two anchored reference prices *and* a ratio between them. Neither
  exists.
- **Feature 2, opening volume**, is volume **summed over the first 30 RTH
  minutes** compared against a trailing median of that same session-window
  aggregate. The operand that landed is the *completed bar's* volume, and
  `zscore(period, source="volume")` normalizes per-bar volume on a trailing
  window of bars. A session-window aggregate is a different quantity: no
  accumulator in the grammar resets on a session boundary.
- **Feature 3** shares H-001's anchored-price gap.
- **The output** is a discrete regime **label that other hypotheses consume**.
  The grammar's four rules produce *positions*. There is no way for a config to
  emit a label, which is the same shape of gap that keeps S0 refused (D-0081) —
  and worth noticing, because a score-emitting seam is being built for exactly
  that reason.

The rolling normalizer helps this file least of the four unlocks, despite this
file having asked for it: what it normalizes is a per-bar series, and every
feature here is a session-window aggregate or a two-instant return.
