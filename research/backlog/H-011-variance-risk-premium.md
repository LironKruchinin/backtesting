---
id: H-011
slug: variance-risk-premium
topic: vol-regime
grade: C
hypothesis_family: es-variance-risk-premium
status: backlog
created: 2026-07-30
---

# H-011 — The variance risk premium as a return predictor

## Citation

Tim Bollerslev, George Tauchen, Hao Zhou, **"Expected Stock Returns and
Variance Risk Premia"**, *The Review of Financial Studies* 22(11), November
2009, 4463–4492.

- Publisher: <https://academic.oup.com/rfs/article-abstract/22/11/4463/1565787>
- SSRN: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=948309>
- RePEc: <https://ideas.repec.org/a/oup/rfinst/v22y2009i11p4463-4492.html>

Their stated claim: motivated by a stylized general-equilibrium model with
time-varying economic uncertainty, the **variance risk premium** — the
difference between implied and realized variation — explains a nontrivial
fraction of the time-series variation in post-1990 aggregate stock market
returns, with **high premia predicting high future returns and low premia
predicting low future returns**.

International and inferential follow-up: Bollerslev, Marrone, Xu, Zhou, *"Stock
Return Predictability and Variance Risk Premia: Statistical Inference and
International Evidence"*, *Journal of Financial and Quantitative Analysis* —
<https://www.cambridge.org/core/journals/journal-of-financial-and-quantitative-analysis/article/abs/stock-return-predictability-and-variance-risk-premia-statistical-inference-and-international-evidence/0BE5DE1D942A0342DDBA24D7BFBEA5C8>

## Mechanism

Options are, in aggregate, insurance. The people who buy index puts and
variance exposure are hedging portfolios they already own, and they are willing
to pay more than the actuarially fair price to do it — which is why implied
variance sits persistently above subsequently realized variance. The size of
that gap is not constant: it widens when investors are more anxious about
uncertainty, and it is precisely when they are most anxious that the
compensation for bearing equity risk is highest. So the premium is not a
forecast of the market in the way a signal is; it is a **measurement of the
price of risk**, read off a market whose participants have already paid to
express it. The losing side is explicit and structural: the **hedger buying
insurance**, who accepts a negative expected return on the option leg in
exchange for protection, and who keeps doing it because the alternative is
carrying tail risk against a mandate or a nervous client. That is a genuine risk
premium with a named payer — the strongest mechanism story in this backlog.

The corresponding caution: earning a risk premium means *bearing the risk*. This
is not an inefficiency, and a strategy harvesting it should expect to lose money
in exactly the states where the insurance pays out.

## Signal in Crucible terms

- **Basket:** ES, as the futures expression of the S&P 500 the premium is
  measured on.
- **Timeframe:** daily decisions; the premium is a monthly-to-quarterly
  forecasting variable in the source literature, not an intraday one.
- **Feature 1 — realized variance:** the sum of squared 1-minute returns over
  the trailing month. **We can compute this today, well**, from data we own.
- **Feature 2 — implied variance:** VIX², or a model-free implied variance
  computed from the SPX option cross-section. **We cannot compute this at all.**
- **Feature 3 — the premium:** feature 2 minus feature 1, as a scalar per day.
- **Rule:** long ES when the premium is in its upper region, flat or reduced
  when in its lower region, with thresholds set from a trailing window only.

## Data

**Owned:** the realized half. `ohlcv-1m` and `ohlcv-1s` for ES,
2010-06-06 → 2026-07-28, which supports a high-quality realized-variance
estimator and a sensitivity check on sampling frequency.

**Missing — and this is a data gap, not a code gap:**

1. **The VIX complex is not downloaded.** `external/cboe/` does not exist on the
   archive. The files are **free** and the plan for them is already written
   (`docs/DATA_PLAN.md`): `VIX_History.csv`, `VIX9D`, `VIX3M`, `VIX1D`, plus VIX
   futures settlements, downloaded by hand from Cboe, each with a filled-in
   `README` recording the exact URL, download date, row count and last date.
   There is deliberately **no scraper** (D-0010). So the acquisition is an
   afternoon of manual work, not a purchase — but it has not been done.
2. **The loader does not exist**, deliberately. It is scheduled with the
   post-M4 regime work, at which point it applies the availability rule and is
   tested against a hand-checked fixture.
3. **Model-free implied variance** — the more faithful version of feature 2 —
   needs the SPX option cross-section. ThetaData EOD/greeks/open-interest for
   SPX is being acquired, but **ThetaData integration is explicitly post-M4**
   (`docs/MILESTONES.md`) and no loader joins options to futures bars.

**The availability rule is the trap, and it is already documented.** A Cboe
daily index value is knowable **at that session's close** (15:00 CT), not at its
open and not on the morning of the date it is stamped with. `docs/DATA_PLAN.md`
states plainly that a loader stamping these rows at midnight, or joining them to
a futures bar by calendar date, has invented roughly one session of lookahead —
*"enough on its own to make a volatility-timing strategy look profitable"*. This
hypothesis is a volatility-timing strategy. It is the exact case that warning
was written about.

## Pre-registered kill criteria

**No run is authorized until the Cboe files are archived with their README and
the loader has passed a hand-checked availability fixture.** That fixture is a
gate, not a nicety: it must assert that a VIX value dated day *D* is invisible
to any decision made before day *D*'s 15:00 CT close.

- **Sample minimum:** **150 non-overlapping monthly observations**. Our span
  gives roughly 190, so this is binding and close.
- **Gate 0 — predictor before system.** Regress forward ES returns at 1, 3 and
  6-month horizons on the premium. The 3-month (quarterly) horizon is the
  pre-registered primary, because that is where the source literature reports
  the effect is strongest — chosen now so it cannot be chosen later. The slope
  must be **positive** with a block-bootstrap 95 % CI excluding zero
  (block = 3 months, to respect overlap). Otherwise **Kill**.
- **Gate 1 — the sign must be right for the mechanism.** A *negative* slope
  would mean high insurance prices predict low returns, which contradicts the
  risk-premium story entirely. A significant negative slope is a **Kill**, not
  an inverted-signal opportunity.
- **Gate 2 — S2:** `min_oos_sharpe_after_costs = 0.5`;
  `kill_if_dead_at_ticks = 1.0`. Costs should be almost irrelevant at monthly
  rebalancing; if they are not, something is wrong with the implementation and
  the run is void rather than failed.
- **Gate 3 — S3:** `max_pbo = 0.5`; `require_plateau = true` over the
  realized-variance window and the premium threshold.
- **Gate 4 — the crisis check, pre-registered because the mechanism demands
  it.** Report performance in the worst 5 % of months separately. A version of
  this strategy that shows *no* losses in crisis months has not harvested a risk
  premium — it has a lookahead bug, and Gate 4 failing is an **engine alarm**,
  not a triumph.

## Honesty note

- **Their data is 1990–2007 S&P 500 index options and returns; ours would be
  2010–2026 ES futures with a VIX we have not yet downloaded.** Essentially no
  sample overlap, which is good, but the paper is from 2009 and the variance
  risk premium has since become one of the most heavily traded ideas in the
  market. Short-volatility strategies harvesting it blew up spectacularly in
  February 2018 — inside our sample. That event is in our data and is exactly
  the kind of thing a monthly-frequency backtest with ~190 observations will
  average away.
- **VIX1D exists only from 2023**, and daily SPX 0DTE only from 2022
  (`docs/PROJECT_PLAN.md` §8). Any short-horizon version of this idea has a
  sample of three years, not sixteen. The 30-day VIX is the only leg with the
  full span.
- **The effective sample is much smaller than it looks.** ~190 monthly
  observations, heavily overlapping if computed on rolling windows, spanning
  perhaps four or five distinct volatility regimes. Standard errors computed as
  though monthly returns were independent will be badly understated; the block
  bootstrap in Gate 0 is there for that reason and its block length is
  pre-registered.
- **We would be trading ES against a premium measured on SPX options.** The
  index is the same, the instruments are not, and the basis between them is
  itself a traded quantity. This is a real approximation and should be stated on
  every result.
- **This is the best-motivated idea in the backlog and it is graded C.** That
  tension is the point of grading on cost rather than on quality: the mechanism
  names a captive payer, the effect is a compensated risk premium rather than an
  anomaly, and it still cannot be tested this week because a free CSV has not
  been downloaded and a loader has not been written.

## Triage grade

**C.** The implied half of the central feature does not exist on the archive.
The acquisition is free and manual, the loader is deliberately deferred to
post-M4, and the availability rule that governs it is subtle enough that
`docs/DATA_PLAN.md` singles out this exact strategy class as what gets broken
when it is applied carelessly. **The cheapest useful action on this file is not
to test it — it is to download the Cboe CSVs and write their README**, so the
data is sitting there, correctly provenanced, when the milestone arrives.
