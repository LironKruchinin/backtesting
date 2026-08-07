# Candidate index — triage list

**91 candidates across two harvests.** Wave 1 drafted 61 on 2026-08-06; wave 2
added 30 on 2026-08-07, deliberately in seams wave 1 did not touch and weighted
away from equity index. Both were built from `research/intake/corpus/` over the
four official APIs (see `research/intake/README.md`). **Nothing here is a
registration** and nothing here has been run.

**Combined grade tally: 19 A · 24 B · 48 C.**
Wave 1: 15 A · 22 B · 24 C. Wave 2: **4 A · 2 B · 24 C.**

Wave 2's grade distribution is much worse than wave 1's and that is the finding,
not a shortfall. Wave 1 swept intraday, momentum, volatility and calendar seams —
literatures stated in terms of a single series, which is what the combo grammar
can read. Wave 2 swept storage, curve shape, carry, auctions, positioning,
order flow and execution, and those literatures are stated in terms of objects
this build has no way to form: a second maturity, a second venue, a cash leg, a
position report, a release timestamp. **Twenty-four of thirty wave-2 candidates
are C, and only one of the twenty-four is C because the idea is expensive; the
rest are C because the sentence the paper wrote cannot be typed into a config.**

## What wave 2 was for

Liron owns sixteen years of CL (247 curated contracts), GC (221), 6E (149) and
ZN (68), and almost nothing in `research/backlog/` touches them. Fifty
variations of intraday momentum are one idea tested fifty times, and the trial
count and the overfitting battery treat them that way — so mechanism diversity
across genuinely different markets is what makes any eventual survivor
believable. **Twenty-nine of wave 2's thirty candidates are non-equity-index**;
the one that is not is included because its conclusion bounds what the others
can claim.

## Read the A column correctly before you spend anything on it

A grade states **cost to test, nothing else** (`research/backlog/README.md` §2).
A grade-A idea can be worthless and a grade-C idea can be the best thing on the
list.

And **grade A means *runnable today*, not *answerable today***. `combo` and
`walk-forward` replay **one raw contract** — continuous aliases are refused for
the grid commands by design (§2.2) — which is roughly 60 sessions for ES.
`research/backlog/README.md` §6.2 states the consequence plainly: **the A column
produces no verdicts until registry pooling lands**, because no sample-adequacy
criterion worth registering is satisfiable at that length, so today's A-grade
runs are guaranteed to be killed for sample size — correctly, by the machine.
That is the pre-registration working. It is not a result.

Wave 2's four A candidates make that sharper rather than softer. Two of them
(`early-close-session-effect`, `london-fix-overnight-gold`) count **events**
rather than sessions — early closes happen about nine times a year — so one
contract's life holds one or two observations, and their registered sample floors
are deliberately set above the usual ones to say so.

So the A column is a list of things that can be *stated and executed* now. The B
and C columns each name the one piece that is missing, which is the more useful
column if the question is what to build.

## What the B and C columns add up to, across both waves

The grade says what a candidate costs. This says what the COSTS have in common —
the more useful question if you are deciding what to build rather than what to
run. Each candidate is counted once, under the first bucket its `missing piece`
matches. Wave 1's assignments are carried through unchanged; wave 2's are
assigned by the first blocker its Triage grade names.

| blocked | wave 1 | wave 2 | the one piece they are waiting on |
|---|---|---|---|
| **16** | 7 | 9 | data the archive does not hold |
| **13** | 7 | 6 | a macro / event calendar |
| **9** | 8 | 1 | multi-instrument configs (two roots in one run) |
| **6** | 5 | 1 | calendar predicates (day-of-week, day-of-month) |
| **5** | 5 | — | a fitted-model indicator (regime-switching, EGARCH) |
| **4** | 4 | — | continuous position sizing |
| **4** | 4 | — | a rolling extremum (opening range, Donchian) |
| **3** | — | 3 | **a multi-maturity curve reader** (two contracts of one root, one config) |
| **3** | — | 3 | **a root the archive does not hold** (agriculturals, refined products, a second rates point) |
| **2** | — | 2 | **quote / message-level data — unobtainable, not merely unbought** (D-0120) |
| **1** | — | 1 | **a cost-input estimator over owned bars** |
| **1** | 1 | — | a stitched series for the GRID commands |
| **1** | 1 | — | arithmetic between operands |
| **1** | 1 | — | an anchored reference price |
| **1** | 1 | — | an open-interest series |
| **1** | 1 | — | a contract-age operand |
| **1** | 1 | — | a causal time-of-day normalizer |
| **72** | 46 | 26 | total blocked (91 candidates − 19 grade A) |

### The cheapest unlocks, named

1. **A macro / event calendar — a static CSV, free, and it blocks 13 candidates.**
   It is the largest single unlock in the index and it is not a purchase. Wave 2
   widens what it has to hold: FOMC and macro releases (wave 1), **plus a
   petroleum inventory schedule, plus a Treasury auction schedule**. Two wave-2
   candidates need **times only** and no contents
   (`fx-news-arrival-activity-burst`, and the abstention arms elsewhere), which
   makes a times-only v1 a genuinely useful first step rather than a stub.

2. **A multi-maturity curve reader — the data is entirely owned.** Three wave-2
   candidates name it as their first blocker and three more name it as a
   component: wave 1's `wti-term-structure-forecast`, `equilibrium-forward-curves`
   and `commodities-long-run-carry`, plus wave 2's
   `contango-backwardation-comovement`, `curve-state-spot-futures-linkage`,
   `carry-crash-risk-currency` and `spot-based-basis-momentum`. **Six candidates,
   zero acquisitions**: CL has 247 curated contracts, GC 221, 6E 149. The
   surprise wave 2 produced is that **FX carry is the same object** — the interest
   differential is mechanically the 6E calendar spread — so one reader serves the
   commodity and the currency literatures at once.
   One design constraint arrives with it, free: `spot-based-basis-momentum`'s
   source argues that using the front contract as a spot proxy is the *inferior*
   measure, which is worth knowing before the definition is fixed rather than
   after.

3. **Calendar predicates (day-of-week, day-of-month) — one operand, 6 candidates.**
   Wave 2 also found that **one** calendar predicate is already reachable without
   it: `minutes_to_close < minutes_to_rth_close` is true exactly on a session the
   exchange is closing early, because the grammar compares two operands as
   readily as an operand and a constant, and the two clock readings disagree only
   on an early close (D-0078). That is what `early-close-session-effect` runs on.
   It does **not** retire the row — day-of-week, day-of-month and turn-of-month
   remain unreachable — and it is recorded because a reader looking at
   `research/backlog/README.md` §2.1's "Calendar predicates: not expressible" row
   would otherwise not know that the holiday case falls out of two operands that
   already exist.

4. **An open-interest transcode path and operand — the data is already in `raw/`.**
   The `statistics` schema is archived for all seven roots, 2010-06-06 →
   2026-07-29, and nothing reads it. Two candidates
   (`open-interest-volatility`, `hedging-pressure-risk-premium`) name it, and one
   of them names it as a *separable* sub-unlock behind a much more expensive
   blocker — so building it delivers value without the position data.

5. **A low-frequency spread estimator — one candidate, and it improves every
   other verdict.** The cost sweep is mandatory (§2.4) and its centre,
   `half_spread_ticks = 1`, is a convention wearing a measurement's field name on
   six of seven roots, permanently (D-0120). `high-low-spread-estimator` reads
   only high, low and close — owned for 863 curated contracts over sixteen years —
   and can be validated against the one `tbbo` year that exists for ES. It is the
   only entry in this index whose payoff is to the denominator under everything
   else.

### Two walls, stated once

**The order-flow wall.** Three candidates from three unrelated literatures —
wave 1's `order-flow-imbalance-es`, wave 2's `auction-day-price-pressure-reversal`
and `currency-momentum-order-flow` — stop at the same place. D-0120: the L1/L3
entitlement windows were allowed to lapse, so the archive holds one `tbbo` and one
`trades` record (ES only, one year of sixteen) and one month of `mbo`. This is
**unobtainable, not unbought** — the vendor sells the past only through those
windows. Any candidate whose mechanism is signed flow is permanently out of reach
here, and it is better to say that once than to keep grading it C as though a
budget would fix it.

**The forecast wall.** Two candidates — wave 1's `crude-regime-switching-garch`
and wave 2's `price-volatility-cojump-forecasting` — produce a *volatility
forecast* rather than a position, and the funnel scores position rules. There is
no criterion that reads forecast accuracy, so these cannot be judged even with
perfect data. That is a limitation of the machine rather than of the archive, and
it is the only one in this index that no acquisition touches.

## Candidates — wave 2 (2026-08-07)

| grade | asset | topic | mechanism | missing piece | draft |
|---|---|---|---|---|---|
| **A** | metals | `metals-lease-rates-carry` | hold gold from the afternoon London fixing to the next morning's, expressed as two session-clock constants | — | [london-fix-overnight-gold](DRAFT-london-fix-overnight-gold.md) |
| **A** | metals | `realized-vs-implied-volatility` | a trailing volatility state is regime CONTEXT and not a directional trigger — registered expecting the null | — | [gold-volatility-regime-context](DRAFT-gold-volatility-regime-context.md) |
| **A** | rates, FX | `liquidity-provision-market-making` | fade an outsized bar; the registered discriminator is the tick level at which the spread eats the reversal | — | [extreme-move-reversal-cost-barrier](DRAFT-extreme-move-reversal-cost-barrier.md) |
| **A** | energy, metals, FX, rates | `holiday-weekend-effects` | the holiday-adjacent session, identified by `minutes_to_close < minutes_to_rth_close` — an early close, without a calendar operand | — | [early-close-session-effect](DRAFT-early-close-session-effect.md) |
| **B** | energy | `crude-inventory-storage` | the minutes around a scheduled weekly petroleum release behave differently from ordinary minutes | calendar predicates (day-of-week) — the weekly cadence is a public constant and the bars are owned, so only the operand is missing; the holiday-shifted weeks additionally need a real release calendar | [eia-release-window-crude](DRAFT-eia-release-window-crude.md) |
| **B** | all seven roots | `execution-cost-slippage` | estimate the bid-ask spread from high, low and close — the cost input every other candidate assumes | a low-frequency spread estimator over curated bars, and a `qa`-style report to put its output in; NOT a config field, and not a signal | [high-low-spread-estimator](DRAFT-high-low-spread-estimator.md) |
| **C** | cross-asset | `crude-inventory-storage` | oil inventory surprises reprice equities, bonds and the dollar — with a sign that flipped once | a petroleum-inventory release calendar with an availability rule, plus multi-instrument configs | [oil-inventory-news-across-assets](DRAFT-oil-inventory-news-across-assets.md) |
| **C** | energy, metals | `energy-roll-yield-timing` | curve slopes co-move across commodities, and gold's moves against the rest | a curve-slope feature — two maturities of one root in one config — then multi-root configs; the DATA is fully owned | [contango-backwardation-comovement](DRAFT-contango-backwardation-comovement.md) |
| **C** | metals | `energy-roll-yield-timing` | whether cash and deferred legs track each other more tightly in contango than in backwardation (a published null) | a curve-state classifier (two maturities) AND a spot metals series — two blockers, only one of which a curve reader removes | [curve-state-spot-futures-linkage](DRAFT-curve-state-spot-futures-linkage.md) |
| **C** | none owned | `commodity-seasonality-physical` | deterministic physical seasonality in commodity curves — harvest, heating, driving season | an instrument, not a feature: an agricultural or refined-product root. The only wave-2 blocker that is a purchase and nothing else | [agricultural-seasonality-storage](DRAFT-agricultural-seasonality-storage.md) |
| **C** | metals | `gold-safe-haven` | gold's correlation with equities falls in calm periods and rises when political tension is extreme | a geopolitical-risk index with a stated availability rule, and multi-instrument configs | [gold-geopolitical-risk-safe-haven](DRAFT-gold-geopolitical-risk-safe-haven.md) |
| **C** | metals | `metals-lease-rates-carry` | the lease rate is gold's convenience yield, and it falls as warehouse stocks rise | a gold lease-rate / forward-rate series and COMEX warehouse inventory — and the benchmark rate was discontinued mid-archive | [gold-lease-rate-convenience-yield](DRAFT-gold-lease-rate-convenience-yield.md) |
| **C** | metals | `metals-lease-rates-carry` | the New York futures venue does more of gold's price discovery than the far larger London spot market | a London spot gold series — outside this archive's vendor, and no milestone unblocks it | [london-newyork-gold-price-discovery](DRAFT-london-newyork-gold-price-discovery.md) |
| **C** | metals | `liquidity-provision-market-making` | the futures-to-spot spread in precious metals relaxes at several speeds at once | a spot metals series, quote-level GC data (unobtainable, D-0120), and the `queue_sim` fill model (M4) | [precious-metals-efp-market-making](DRAFT-precious-metals-efp-market-making.md) |
| **C** | FX | `fx-carry-interest-differentials` | carry returns are payment for crash risk, and unwind together when funding tightens | an interest-differential feature — reachable as a two-maturity FX curve over data we fully own — plus a currency cross-section we do not own | [carry-crash-risk-currency](DRAFT-carry-crash-risk-currency.md) |
| **C** | FX | `fx-intervention-central-bank` | official intervention works, and mostly in regimes and currencies the euro is not one of | intervention event dates, AND an instrument whose central bank intervenes — the euro is the wrong side of the paper's own boundary condition | [fx-intervention-effectiveness](DRAFT-fx-intervention-effectiveness.md) |
| **C** | FX | `announcement-drift-commodities` | activity in FX bursts at a scheduled release even when the number is exactly as forecast | a release calendar (times only — the cheapest calendar dependency in either wave); the arrival-process half also needs a grain we do not hold | [fx-news-arrival-activity-burst](DRAFT-fx-news-arrival-activity-burst.md) |
| **C** | FX | `order-flow-microstructure-commodities` | currency momentum is stronger when the buying pressure came through short-dated swaps, and from banks | signed order flow segmented by counterparty — which no exchange sells — and a currency cross-section | [currency-momentum-order-flow](DRAFT-currency-momentum-order-flow.md) |
| **C** | FX | `futures-basis-cash-arbitrage` | the skewness of the futures-minus-spot basis predicts subsequent spot returns | a spot FX series (an acquisition) and a third-moment trailing indicator (a small, reusable build) | [currency-basis-skewness](DRAFT-currency-basis-skewness.md) |
| **C** | rates | `treasury-auction-cycle` | dealers cover short futures hedges once an auction is placed, and the ten-year note future moves | a Treasury auction calendar — dates, tenors, result instant — with bid-to-cover as a separable second acquisition | [treasury-auction-zn-futures](DRAFT-treasury-auction-zn-futures.md) |
| **C** | rates | `treasury-auction-cycle` | yields drift up before an auction and retrace after it, scaled by how constrained dealers are | the same auction calendar for the price half; the order-flow half needs ZN L1/L3 data that is unobtainable (D-0120) | [auction-day-price-pressure-reversal](DRAFT-auction-day-price-pressure-reversal.md) |
| **C** | rates | `limit-order-book-dynamics` | what a bond future's order book actually contains — i.e. what an OHLCV bar throws away | limit-order-book data for a rates future, unobtainable here (D-0120) and on a different exchange besides. Listed as a NAMED HOLE, not as work to schedule | [bond-futures-order-book-stylized-facts](DRAFT-bond-futures-order-book-stylized-facts.md) |
| **C** | rates | `yield-curve-duration` | whether curve-slope predictability of bond returns is partly a small-sample artifact of how it is estimated | a second point on the curve — an instrument, not a feature. The archive's only rates root is ZN | [yield-curve-slope-treasury-returns](DRAFT-yield-curve-slope-treasury-returns.md) |
| **C** | rates, metals | `jump-detection-discontinuities` | Treasuries and precious metals jump at the same instant, which unconditional correlation hides | multi-instrument configs, a jump estimator, and silver — the data we DO hold is already the right pair at the right grain | [cojumps-rates-precious-metals](DRAFT-cojumps-rates-precious-metals.md) |
| **C** | energy | `announcement-drift-commodities` | a published NULL: scheduled macro releases do not raise the jump arrival rate in energy futures | a macro announcement calendar with timestamps and surprise values. Registered for its null, which qualifies three other candidates | [energy-announcement-nonreaction](DRAFT-energy-announcement-nonreaction.md) |
| **C** | energy | `volatility-transmission-commodities` | energy and agricultural volatilities are linked only in the turbulent regime | agricultural roots (a purchase), multi-instrument configs (a design rule) and a fitted regime-switching indicator (a build) | [energy-volatility-regime-linkage](DRAFT-energy-volatility-regime-linkage.md) |
| **C** | energy, metals | `open-interest-positioning` | hedging pressure and the equity link determine the commodity futures risk premium | trader-position data with an availability rule, and multi-instrument configs — but it names a cheap separable unlock: open interest is ALREADY in `raw/` | [hedging-pressure-risk-premium](DRAFT-hedging-pressure-risk-premium.md) |
| **C** | energy, metals | `futures-basis-cash-arbitrage` | basis and basis momentum measured against a real spot price rather than against the front contract | a spot price series per commodity, and a two-maturity curve reader — and it constrains how that reader should define the basis | [spot-based-basis-momentum](DRAFT-spot-based-basis-momentum.md) |
| **C** | rates, energy | `futures-basis-cash-arbitrage` | the seller's timing and location choices are worth something, so the basis need not converge cleanly | a cash price for the deliverable and a delivery-option valuation. Its value is the note it leaves: ZN's convergence turns on a basket we cannot see | [delivery-options-basis-convergence](DRAFT-delivery-options-basis-convergence.md) |
| **C** | equity index | `jump-detection-discontinuities` | a price jump matters for forecasting only when a volatility jump came with it | an options-implied volatility series, a jump estimator, and a criterion that scores a FORECAST rather than a position — the third is a funnel gap | [price-volatility-cojump-forecasting](DRAFT-price-volatility-cojump-forecasting.md) |

## Candidates — wave 1 (2026-08-06)

| grade | asset | topic | mechanism | missing piece | draft |
|---|---|---|---|---|---|
| **A** | FX | `short-horizon-reversal` | one-minute EUR/USD reverts to its trailing mean; fade a trailing z-score extreme | — | [eurusd-intraday-reversion](DRAFT-eurusd-intraday-reversion.md) |
| **A** | all seven roots | `trend-horizon` | short-horizon trend following stopped paying around 2009; test the fast-span end of the crossover grid | — | [short-trend-decay](DRAFT-short-trend-decay.md) |
| **A** | all seven roots | `trend-horizon` | trends revert once they get too strong; gate a crossover on a trailing z-score of price | — | [trend-reversion-interplay](DRAFT-trend-reversion-interplay.md) |
| **A** | all seven roots | `vol-managed-exposure` | adjust exposure ONLY in the volatility extremes rather than continuously | — | [conditional-volatility-targeting](DRAFT-conditional-volatility-targeting.md) |
| **A** | energy | `volume-price` | volume predicts the SIGN of return serial correlation: positive within the hour, negative over the mid-term | — | [volume-serial-correlation-crude](DRAFT-volume-serial-correlation-crude.md) |
| **A** | energy, metals | `overnight-intraday` | night-session and day-session commodity futures returns have different distributions | — | [commodity-night-vs-day-returns](DRAFT-commodity-night-vs-day-returns.md) |
| **A** | energy, metals | `short-horizon-reversal` | reaction to large one-day price shocks in commodity futures: over-, under-, or no reaction | — | [commodity-shock-efficiency](DRAFT-commodity-shock-efficiency.md) |
| **A** | energy, metals | `short-horizon-reversal` | price behaviour on and after abnormal-return days in gold and oil | — | [gold-oil-abnormal-return-day](DRAFT-gold-oil-abnormal-return-day.md) |
| **A** | equity index | `intraday-seasonality` | bar range and activity decay monotonically through the ES session from open to close | — | [es-intraday-liquidity-decay](DRAFT-es-intraday-liquidity-decay.md) |
| **A** | equity index | `overnight-intraday` | volatility falls from the opening hour to early afternoon then rises; intraday exceeds overnight | — | [intraday-volatility-shape-classic](DRAFT-intraday-volatility-shape-classic.md) |
| **A** | equity index | `overnight-intraday` | the daytime and overnight E-mini sessions behave as segmented markets rather than one | — | [daytime-overnight-segmentation](DRAFT-daytime-overnight-segmentation.md) |
| **A** | equity index | `short-horizon-reversal` | five-minute index-futures returns are serially dependent for two intervals, not a random walk | — | [index-futures-return-dependence](DRAFT-index-futures-return-dependence.md) |
| **A** | equity index, energy | `vol-regime-clustering` | the QUANTILES of future returns, not just the mean, shift with trailing realized variation | — | [conditional-quantiles-by-realized-variation](DRAFT-conditional-quantiles-by-realized-variation.md) |
| **A** | metals | `intraday-seasonality` | informational efficiency, volatility and volume in gold differ systematically by trading session | — | [metals-session-efficiency](DRAFT-metals-session-efficiency.md) |
| **A** | rates, FX, equity index | `intraday-seasonality` | intraday volatility on a 23-hour Globex schedule is mostly own-region, not imported | — | [globex-intraday-volatility-shape](DRAFT-globex-intraday-volatility-shape.md) |
| **B** | all seven roots | `vol-managed-exposure` | the alpha of volatility-managed strategies is an embedded trend-following exposure | continuous position sizing, needed to build the volatility-managed arm the comparison is against; the trend arm alone is already expressible | [vol-managed-is-trend](DRAFT-vol-managed-is-trend.md) |
| **B** | all seven roots | `vol-managed-exposure` | volatility targeting helps risk assets and does nothing for bonds, currencies and commodities | continuous position sizing — the grammar has boolean entries and a fixed contract count, so a scaled notional cannot be expressed | [vol-targeting-asset-class-asymmetry](DRAFT-vol-targeting-asset-class-asymmetry.md) |
| **B** | energy | `calendar-effects` | day-of-the-week effects in WTI and refined-product futures | calendar predicates (day-of-week) — no operand names a weekday | [petroleum-day-of-week](DRAFT-petroleum-day-of-week.md) |
| **B** | energy | `calendar-effects` | a seasonality indexed by trading time rather than by maturity or by the underlying | calendar predicates — the effect is indexed by the futures TRADING date, which no operand names | [trading-time-seasonality-energy](DRAFT-trading-time-seasonality-energy.md) |
| **B** | energy | `intraday-seasonality` | intraday activity in crude is multi-modal, with modes at European opens and scheduled releases | a macro/inventory release calendar — the paper's modes are tied to named scheduled events, which is an M4 static CSV that does not exist | [brent-intraday-buildup](DRAFT-brent-intraday-buildup.md) |
| **B** | energy | `short-horizon-reversal` | the first half-hour's return predicts the rest of the crude session, with the sign reversed | an anchored reference price — a price captured at a named past instant (the first half-hour's close) and held for the session | [crude-intraday-reversal](DRAFT-crude-intraday-reversal.md) |
| **B** | energy | `vol-regime-clustering` | whether a two-regime variance model forecasts crude volatility better than a single-regime one | a regime-switching conditional-variance indicator, and a criterion that scores a VOLATILITY FORECAST — the funnel scores position rules, not forecasts | [crude-regime-switching-garch](DRAFT-crude-regime-switching-garch.md) |
| **B** | energy, equity index | `breakout-range-expansion` | opening-range breakout returns differ sharply across volatility states in crude and S&P futures | a session-anchored rolling high/low (opening-range) indicator; the volatility-state half is already expressible via `stdev(period, source="return")` | [orb-volatility-states](DRAFT-orb-volatility-states.md) |
| **B** | energy, metals | `breakout-range-expansion` | the Turtle channel-breakout system, tested over 27 years of one commodity | a rolling max/min (Donchian channel) indicator — `bollinger` is a volatility band around a mean, which is a different object and moves for a different reason | [donchian-turtle-channels](DRAFT-donchian-turtle-channels.md) |
| **B** | energy, metals | `term-structure-roll-yield` | a contract's volatility rises as its own expiry approaches — testable inside ONE contract's life | a time-to-expiry / contract-age operand, OR a criterion that reads the per-fold table: `walk-forward` already slices a contract's life into ordered folds, so the comparison is PRINTED but nothing machine-checks a trend across it | [samuelson-maturity-effect](DRAFT-samuelson-maturity-effect.md) |
| **B** | energy, metals | `trend-horizon` | trend following combined with inverse-volatility sizing in commodity futures | continuous position sizing — the rule grammar emits boolean entries only, so an inverse-volatility weight cannot be expressed | [trend-riskparity-commodity](DRAFT-trend-riskparity-commodity.md) |
| **B** | energy, metals | `vol-regime-clustering` | commodity futures variance is better described by two switching regimes than by one process | a Markov regime-switching conditional-variance indicator; every indicator in this build is a trailing window with no latent state | [commodity-regime-switching-variance](DRAFT-commodity-regime-switching-variance.md) |
| **B** | energy, metals | `volume-price` | trading volume and open interest carry different information about futures volatility | an open-interest series in curated data and an operand for it — the raw `statistics` schema is archived for all seven roots and nothing transcodes it | [open-interest-volatility](DRAFT-open-interest-volatility.md) |
| **B** | equity index | `breakout-range-expansion` | breakout of the opening range, timed to the underlying cash market's active hours | a session-anchored rolling high/low (opening-range) indicator — README Sec 2.1 names this explicitly as the one thing the session clock does NOT give us | [orb-index-futures](DRAFT-orb-index-futures.md) |
| **B** | equity index | `breakout-range-expansion` | opening-range breakout is unprofitable on most commodities in recent years unless the threshold adapts | a session-anchored rolling high/low (opening-range) indicator, plus a declarable exit bracket — the engine has brackets (D-0069) but the combo grammar cannot name one | [orb-threshold-adjustment](DRAFT-orb-threshold-adjustment.md) |
| **B** | equity index | `calendar-effects` | the REBUTTAL arm: whether the turn-of-month effect survived in S&P index futures after it was published | calendar predicates — day-of-month and turn-of-month have no operand; the calendar exists in `crucible-data` and the grammar cannot reach it | [turn-of-month-rebuttal](DRAFT-turn-of-month-rebuttal.md) |
| **B** | equity index | `calendar-effects` | whether the turn-of-month effect is EXPLOITABLE once costs are charged, internationally | calendar predicates (day-of-month / turn-of-month index) — no operand names a date | [turn-of-month-exploitability](DRAFT-turn-of-month-exploitability.md) |
| **B** | equity index | `intraday-seasonality` | the average intraday volatility curve as a function of time-of-day, estimated nonparametrically | a CAUSAL time-of-day volatility normalizer — the paper's estimator is a full-sample average over the whole span, which Sec 2.1 forbids inside a strategy | [intraday-periodic-volatility-curve](DRAFT-intraday-periodic-volatility-curve.md) |
| **B** | equity index | `volume-price` | adjust a moving-average-difference oscillator by volume and intraday range | arithmetic between operands — MACD is a DIFFERENCE of two moving averages, and the grammar compares operands but never combines them | [volume-price-macd](DRAFT-volume-price-macd.md) |
| **B** | equity index, energy | `breakout-range-expansion` | gate intraday breakout signals on a conditional-variance state estimate | a conditional-variance (EGARCH) indicator; every statistic in `crucible-strategies::indicators` is a trailing window, and no `IndicatorKind` names a fitted model | [egarch-gated-breakout](DRAFT-egarch-gated-breakout.md) |
| **B** | metals | `calendar-effects` | month-of-year and day-of-week anomalies in gold futures returns | calendar predicates (day-of-week, month-of-year) — no operand names a date | [gold-futures-seasonal-anomalies](DRAFT-gold-futures-seasonal-anomalies.md) |
| **B** | metals | `vol-regime-clustering` | a gold trading rule driven by the fitted probability of being in the high-variance regime | a Markov regime-switching conditional-variance indicator; the paper's trading rule is driven by the fitted regime probability, which no operand can name | [gold-regime-switching-volatility](DRAFT-gold-regime-switching-volatility.md) |
| **C** | FX | `cross-asset-lead-lag` | whether CME currency futures or the interdealer spot market discovers the exchange rate first | interdealer spot FX data (EBS) — not owned, not acquirable through the vendor this archive uses, and no milestone plans it | [fx-futures-price-discovery](DRAFT-fx-futures-price-discovery.md) |
| **C** | FX | `cross-asset-lead-lag` | lead-lag structure among currency pairs at high frequency | multi-instrument configs, and a currency cross-section the archive does not hold — 6E is the only FX root owned | [fx-lead-lag-structure](DRAFT-fx-lead-lag-structure.md) |
| **C** | all seven roots | `trend-horizon` | multi-month trend premium across four asset classes over two centuries | a continuous-series consumer for the GRID commands — excluded by design (README Sec 2.2); one contract's life cannot hold a multi-month lookback | [two-centuries-trend](DRAFT-two-centuries-trend.md) |
| **C** | cross-asset | `cross-asset-lead-lag` | a lagged-adjustment model that separates true lead-lag from contemporaneous co-movement | multi-instrument configs — a lead-lag statistic is defined on a PAIR, and `combo` refuses a config declaring two instruments | [high-frequency-lead-lag](DRAFT-high-frequency-lead-lag.md) |
| **C** | cross-asset | `cross-asset-lead-lag` | stock, bond and FX futures all jump to real-time macro news, with linkages that differ by market | a macro announcement calendar (M4 static CSV) AND multi-instrument configs; the claim is a joint statement about three markets and a release time | [real-time-cross-market-discovery](DRAFT-real-time-cross-market-discovery.md) |
| **C** | cross-asset | `overnight-intraday` | crude's overnight realized volatility forecasts the US equity index's, asymmetrically in the sign | multi-instrument configs — the claim relates CL's overnight session to ES's realized volatility, and one config names one instrument | [oil-overnight-predicts-equity-vol](DRAFT-oil-overnight-predicts-equity-vol.md) |
| **C** | cross-asset | `trend-horizon` | lead-lag spillover between trending markets used to enhance a univariate trend signal | multi-instrument configs — `combo` refuses a config declaring two instruments, and momentum spillover is a statement about pairs | [network-momentum-leadlag](DRAFT-network-momentum-leadlag.md) |
| **C** | energy | `intraday-seasonality` | filter the recurring intraday shape out of returns before measuring volatility dependence | an intraday-periodicity filter (flexible Fourier form or cubic spline) as an indicator — no milestone builds one, and a full-sample fit would be lookahead besides | [crude-intraday-periodicity-filter](DRAFT-crude-intraday-periodicity-filter.md) |
| **C** | energy | `term-structure-roll-yield` | the spot-futures basis and physical inventory levels, and where storage theory breaks | physical inventory data (EIA or equivalent) with a stated availability rule, plus a two-maturity basis feature | [inventories-and-oil-basis](DRAFT-inventories-and-oil-basis.md) |
| **C** | energy | `term-structure-roll-yield` | curve-shape factors extracted from the WTI futures term structure predict holding-period returns | multi-contract curve construction in one config — the DATA is fully owned (every CL contract, sixteen years), so this is a machinery gap and not an acquisition | [wti-term-structure-forecast](DRAFT-wti-term-structure-forecast.md) |
| **C** | energy, metals | `short-horizon-reversal` | split returns into a speculator-flow component and a residual; they predict next week with opposite signs | trader-position data (CFTC Commitments of Traders or equivalent) — not owned, not in `docs/DATA_PLAN.md`, and not in any milestone | [speculator-flow-momentum-reversal](DRAFT-speculator-flow-momentum-reversal.md) |
| **C** | energy, metals | `term-structure-roll-yield` | long-run commodity futures returns split into a carry component and a spot component | carry as a feature (needs two maturities) and cross-sectional portfolio accounting (post-M4) | [commodities-long-run-carry](DRAFT-commodities-long-run-carry.md) |
| **C** | energy, metals | `term-structure-roll-yield` | an inventory-constrained equilibrium model of the shape and volatility of commodity forward curves | a forward-curve object: several maturities of one root read together, which one config cannot declare | [equilibrium-forward-curves](DRAFT-equilibrium-forward-curves.md) |
| **C** | energy, metals | `vol-managed-exposure` | managing volatility improves a cross-sectional commodity momentum portfolio | cross-sectional portfolio accounting (post-M4) AND continuous position sizing; commodity momentum here is a cross-sectional sort, not a time-series rule | [commodity-momentum-vol-management](DRAFT-commodity-momentum-vol-management.md) |
| **C** | energy, metals | `volume-price` | news arrival rate and sentiment covary with realized volatility in gold and crude beyond trading activity | a news dataset with arrival timestamps and an availability rule; nothing in `docs/DATA_PLAN.md` acquires one | [news-flow-trading-activity](DRAFT-news-flow-trading-activity.md) |
| **C** | equity index | `macro-announcements` | post-FOMC returns look like price pressure and reverse by the end of the announcement cycle | an FOMC calendar; and the reversal predictor is a change in an options-implied index (VIX), which `external/cboe/` does not hold | [fomc-post-announcement-reversal](DRAFT-fomc-post-announcement-reversal.md) |
| **C** | equity index | `macro-announcements` | US equities earn most of their excess return in the day before scheduled FOMC decisions | an FOMC meeting calendar with announcement timestamps — the same M4 static CSV, and the effect is defined entirely by that date | [pre-fomc-drift](DRAFT-pre-fomc-drift.md) |
| **C** | equity index | `overnight-intraday` | the overnight/intraday return split in US equities, attributed to news topics | a timestamped news corpus and its availability rule; also a cash-equity session structure that a 23-hour futures market does not have | [overnight-news-returns](DRAFT-overnight-news-returns.md) |
| **C** | equity index | `volume-price` | order-flow imbalance and returns at one-second frequency, and how announcements reshape the link | a signed order-flow feature and the loader under it: `tbbo`/`trades` exist for ES ONLY and for one year of sixteen (D-0120), and no curated path reads them | [order-flow-imbalance-es](DRAFT-order-flow-imbalance-es.md) |
| **C** | equity index, rates | `macro-announcements` | prices move in the CORRECT direction in the half hour BEFORE scheduled US releases | a macro announcement calendar with release timestamps and an explicit availability rule (Sec 2.1) — an M4 static CSV that does not exist yet | [pre-announcement-drift](DRAFT-pre-announcement-drift.md) |
| **C** | metals | `macro-announcements` | whether a trend rule improves simply by refusing to hold through scheduled releases | a macro announcement calendar — the ENTIRE intervention is 'do not hold a position in this window', which is the cheapest possible use of one and still needs one | [avoid-news-windows-gold](DRAFT-avoid-news-windows-gold.md) |
| **C** | rates | `calendar-effects` | a strong, persistent year-end seasonality in one-month rate derivatives | the instrument itself — this is a short-rate/LIBOR phenomenon and the archive's only rates root is ZN, a 10-year Treasury note future; LIBOR is also discontinued | [year-end-rate-seasonality](DRAFT-year-end-rate-seasonality.md) |
| **C** | rates | `cross-asset-lead-lag` | price discovery and volatility spillover between bond futures and interest-rate swaps | interest-rate swap data and multi-instrument configs; the archive's rates holding is ZN futures alone | [rates-price-discovery-spillover](DRAFT-rates-price-discovery-spillover.md) |
| **C** | rates | `macro-announcements` | scheduled announcements produce most of the intraday jumps in rate-futures volatility and covariance | a macro announcement calendar, plus multi-instrument configs for the covariance half | [announcement-rates-covariance](DRAFT-announcement-rates-covariance.md) |
