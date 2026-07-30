---
id: H-002
slug: overnight-intraday-tug-of-war
topic: intraday-session
grade: B
hypothesis_family: es-overnight-intraday-split
status: backlog
created: 2026-07-30
---

# H-002 — The overnight/intraday split as a conditioning variable

## Citation

Dong Lou, Christopher Polk, Spyros Skouras, **"A tug of war: Overnight versus
intraday expected returns"**, *Journal of Financial Economics* 134(1), 2019,
192–213.

- Publisher: <https://www.sciencedirect.com/science/article/abs/pii/S0304405X19300650>
- Author copy: <https://personal.lse.ac.uk/polk/research/TugOfWar.pdf>
- Internet appendix: <https://personal.lse.ac.uk/loud/ATugofWar_appendix.pdf>
- RePEc: <https://econpapers.repec.org/RePEc:eee:jfinec:v:134:y:2019:i:1:p:192-213>

Their stated claim: they link investor heterogeneity to the persistence of the
overnight and intraday components of returns, documenting firm-level return
continuation *within* each component along with an offsetting *cross-period*
reversal, and report that the smoothed spread between a strategy's overnight
and intraday return components forecasts time variation in that strategy's
close-to-close performance.

Supporting, and useful mainly as a warning: Bruce Knuteson, **"Strikingly
Suspicious Overnight and Intraday Returns"**, arXiv:2010.01727 (2020-10-05),
documents that decades of large positive overnight and negative intraday
returns are hard to explain innocuously. Treat it as evidence the split is
*real and large*, not as an endorsement of its stated cause.

## Mechanism

Different investors trade at different times of day, habitually and for
structural reasons. Retail and news-driven flow concentrates around the open;
institutional execution algorithms work through the session; index and
benchmark-tracking flow concentrates into the close. Because each clientele's
demand is persistent, the price impact each one leaves is persistent too — and
because they are pushing at different hours, the impact of one is partly
unwound during the hours the other dominates. That is the "tug of war": the
overnight component and the intraday component carry *different* clienteles'
footprints, and a return decomposed into the two is more informative than the
close-to-close return that averages them. The losing side is whichever
clientele is habitually demanding liquidity at its preferred hour and paying
the other for it — most plausibly the open-concentrated retail and
overnight-gap-chasing flow, which trades at the session's widest spreads
because that is when it is paying attention. They keep doing it because
attention, not execution quality, is what determines when a discretionary
trader trades.

## Signal in Crucible terms

This idea is **not a standalone entry rule** and should not be triaged as one.
Its value to Crucible is as a *conditioning variable* and as a diagnostic.

- **Basket:** ES primary; NQ, RTY, and — as a genuinely different session
  shape — CL and 6E.
- **Timeframe:** daily decomposition built from 1m bars.
- **Feature 1 — overnight return:** previous RTH close → today's RTH open.
- **Feature 2 — intraday return:** today's RTH open → today's RTH close.
- **Feature 3 — the smoothed spread:** a moving average of (feature 1 −
  feature 2) over a lookback in sessions. This is the paper's forecasting
  variable, restated for one instrument.
- **Use:** condition any other hypothesis in this backlog on feature 3, and
  report the two components separately in every scorecard for a strategy that
  holds overnight. A strategy whose entire PnL is the overnight component is a
  different (and far less capacity-constrained) claim than one that earns it
  intraday, and today nothing in Crucible would tell them apart.

## Data

**Owned, sufficient:** `ohlcv-1m`, seven parents, 2010-06-06 → 2026-07-28.
The decomposition needs nothing but bars and a session calendar.

**Missing — all code:**
1. Session-boundary anchors (`crucible-data::calendar` knows them; the engine
   may not depend on it — supply caller-side per D-0071).
2. A daily aggregation layer built from 1m bars. Note this is **not** the
   generic resampler: RTH-open-to-RTH-close is a session-relative window, not a
   fixed-width bar, so a `1d` resampler would not produce it.
3. Reporting: splitting a scorecard's PnL into overnight and intraday buckets.

## Pre-registered kill criteria

Because this is a conditioner rather than a strategy, the criteria are
predictor-shaped and there is no S1/S2 equity gate.

- **Sample minimum:** **1,500 sessions** with both components defined.
- **Existence check:** the overnight and intraday mean returns must be
  *statistically distinguishable from each other* — a two-sided block-bootstrap
  test (block = 20 sessions) on the difference of means at the **5 %** level.
  If ES's overnight and intraday components are indistinguishable, the
  decomposition carries no information for this instrument and the idea is
  **Killed** as a conditioner.
- **Forecasting check:** the smoothed spread (feature 3) must produce a
  monotone relationship between its quintile and the *next* session's
  close-to-close return, measured as a rank correlation with a
  block-bootstrap 95 % CI excluding zero. Non-monotone or CI-spanning-zero →
  **Kill** as a forecaster; it may still survive as a *reporting* requirement.
- **Reporting survives regardless.** Even on a double Kill, the recommendation
  to split scorecard PnL into overnight and intraday buckets stands, because
  that is a description of what happened, not a claim about what will.
- **Lookback plateau:** `require_plateau = true` over the smoothing lookback.
  A result that exists at 21 sessions and vanishes at 15 and 30 is **Kill**.

## Honesty note

- **The biggest weakening is that their result is cross-sectional and ours
  would not be.** They sort *firms* on overnight-vs-intraday components and
  document continuation and reversal across that cross-section. We have one
  instrument. A single-instrument time-series test of "does the smoothed spread
  forecast next-day returns" is a **different and much weaker claim** than the
  one the paper establishes, and a failure here is *not* evidence against the
  paper. This file must never be cited as a replication.
- **Cross-sectional accounting is post-M4** (`docs/MILESTONES.md`), so the
  faithful version of this test is not reachable in this project's current
  scope at all. That is a milestone gap, not a data gap.
- **Their data is US equities; ours is futures.** The clientele story is
  specifically about equity investors with attention constraints. ES overnight
  flow is substantially macro and non-US-hours institutional, which is a
  different population than the one the mechanism describes. The mechanism may
  simply not transfer, and it would be honest to say so before the test rather
  than after.
- **The Knuteson papers are provocative, not settled.** They are unrefereed
  arXiv preprints making a strong claim about market participants. Cited here
  as documentation that the overnight/intraday asymmetry is large enough to
  matter; not as support for any explanation of it.
- **Sample overlap:** their sample ends before publication in 2019; ours runs
  to 2026. Overlap is roughly half our sample.

## Triage grade

**B** *for the conditioner and the reporting split* — data owned, code missing,
and the missing code overlaps almost entirely with H-001's.

The faithful cross-sectional version is **C** and post-M4. The file is graded on
what we can actually do, and what we can actually do is the weaker version, so
the weaker version is what the kill criteria judge.
