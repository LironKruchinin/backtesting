---
id: H-006
slug: time-series-momentum
topic: momentum-horizon
grade: C
hypothesis_family: futures-tsmom-horizon
status: backlog
created: 2026-07-30
---

# H-006 — Time-series momentum at 1–12 month horizons, reversing beyond

## Citation

Tobias J. Moskowitz, Yao Hua Ooi, Lasse Heje Pedersen, **"Time Series
Momentum"**, *Journal of Financial Economics* 104(2), 2012, 228–250.

- Author copy: <https://w4.stern.nyu.edu/facdir/lpederse/papers/TimeSeriesMomentum.pdf>
- Working paper: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2089463>
- Practitioner summary: <https://www.aqr.com/Insights/Research/Journal-Article/Time-Series-Momentum>

Their stated claim: significant time-series momentum in equity index, currency,
commodity and bond futures for **each of the 58 liquid instruments** considered;
persistence in returns for **one to 12 months that partially reverses over
longer horizons**, which they read as consistent with initial under-reaction
followed by delayed over-reaction; and a diversified portfolio of time-series
momentum strategies across asset classes delivering abnormal returns with
little exposure to standard factors, performing best in extreme markets.

## Mechanism

Two populations trade against the trend-follower and both are structurally
committed. The first is the **hedger**: a producer selling forward, an airline
buying fuel, a pension fund with a mandated currency hedge. Their trades are
determined by an operating need, not by a price forecast, and they will accept
a worse price to transfer risk — the trend-follower is the counterparty being
paid to bear it, and that payment is a risk premium rather than an
inefficiency. The second is the **slow-updating investor** whose beliefs lag
new information: the initial under-reaction leaves a drift for a few months,
and the eventual over-shoot creates the longer-horizon reversal the paper also
documents. The first channel has a captive loser and should survive; the second
does not and is the part most likely to have decayed since 2012. Crucially, the
paper's headline is a **diversified portfolio across asset classes** — the
claim is that the premium is small and noisy per instrument and only becomes
economically interesting when many weakly-correlated instruments are combined.

## Signal in Crucible terms

- **Basket:** the natural universe is our whole seven-parent basket — ES, NQ,
  RTY (equity), CL, GC (commodity), ZN (rates), 6E (FX). That spread across
  asset classes is precisely what the strategy needs, and it is why
  `docs/DATA_PLAN.md` bought four non-equity legs.
- **Timeframe:** daily bars, with lookbacks of 1–12 months and holding periods
  of one month.
- **Signal:** the sign of the trailing `k`-month excess return, sized inversely
  to a trailing volatility estimate, summed across instruments into one
  portfolio.
- **Continuous series required:** a 12-month lookback spans contract expiries,
  so this needs back-adjusted continuous series (`ES.v.0` etc.). Signals run on
  the back-adjusted series; PnL uses the tradeable price of the then-front
  contract, and `AdjustedPrice` exists as a distinct type specifically so a
  back-adjusted level cannot reach `pnl_nano_usd` (D-0042).

## Data

**Owned:** `ohlcv-1m` for all seven parents, 2010-06-06 → 2026-07-28, plus
`definition` (expiries — the roll table's input) and `statistics` (settlements
and open interest — the volume-roll signal). This is the right basket and a
sixteen-year span.

**Missing:**
1. **Daily bars.** We bought `ohlcv-1m` and `ohlcv-1s` only; `ohlcv-1d` is
   $190/GB and was deliberately not bought (`docs/DATA_PLAN.md`). `transcode`
   maps schemas to timeframes one-to-one, so **there is no way to produce a
   daily bar today** without a resampler. For a 12-month lookback strategy,
   running on 1-minute bars is not a workaround — it is roughly 5.6 million
   bars per instrument to express a monthly signal.
2. **The roll table artifact.** The continuous-contract code landed (D-0041…
   D-0046) but `curated/rolls/` is empty, so no continuous series can be
   loaded and any config naming `ES.v.0` is refused (D-0045). This is a command
   to run, not code to write — the cheapest item on this list.
3. **Multi-instrument portfolio accounting** — **explicitly post-M4**
   (`docs/MILESTONES.md`). This is the binding blocker. Crucible today replays
   one instrument per config and `combo` refuses a config declaring two.
4. **Volatility-inverse position sizing.** `qty_contracts` is a fixed integer;
   there is no exposure-scaling layer. See H-009, which is the same gap.

## Pre-registered kill criteria

- **Sample minimum:** at least **150 monthly observations** per instrument and
  at least **5 instruments** in the portfolio. A "diversified portfolio" of two
  correlated equity indices is not the strategy the paper describes and gets no
  verdict.
- **The portfolio is the unit of judgement, not the instrument.** Per-instrument
  results are reported but never used for the verdict, because selecting the
  instruments that worked is the exact overfit this strategy invites. A
  pre-registered equal-risk-weighted combination of **all seven** parents is
  the headline. Dropping any leg after seeing results is forbidden under this
  key.
- **Horizon plateau is the primary structural test.** `require_plateau = true`
  across lookbacks of 1, 3, 6, 9 and 12 months. The paper claims persistence
  across the whole 1–12 month band; a result present at exactly one lookback
  and absent at its neighbours contradicts the mechanism and is a **Kill**, not
  an optimum.
- **The reversal must also be there.** At lookbacks beyond 24 months the sign
  should weaken or invert. If long-horizon momentum is *as strong* as
  medium-horizon momentum, the under-reaction/over-reaction story is not what
  we have found, and the verdict is **Iterate** with the mechanism marked
  unconfirmed — not `Graduate`.
- **Costs:** `min_oos_sharpe_after_costs = 0.5`, `kill_if_dead_at_ticks = 1.0`.
  Monthly rebalancing on liquid futures is the friendliest possible cost
  profile; failing at one tick here would be damning.
- **`max_pbo = 0.5`**, and the trial count charges every (lookback, holding
  period, instrument set) combination to this family.

## Honesty note

- **This is the most-replicated strategy in this backlog and also the one with
  the strongest publication-decay story.** It is the academic core of a
  multi-billion-dollar managed-futures industry, it has been public since 2012,
  and its post-publication performance has been widely debated. Several
  subsequent papers dispute whether the effect survives once the volatility
  scaling is separated from the momentum signal itself. Anyone taking this
  ticket should read those before running anything, and the file should be
  updated with them.
- **Sample overlap is severe.** Their sample runs 1965–2009. Ours starts
  mid-2010 — so our test is almost entirely out-of-sample relative to theirs,
  which is the good news, but it also means **our entire sample is the
  post-publication period**, which is the bad news. We cannot reproduce their
  result; we can only test whether it still exists.
- **Sixteen years is not many independent observations for a monthly
  strategy.** At 12-month lookbacks with monthly holding, the effective sample
  is on the order of a couple of hundred overlapping monthly returns per
  instrument, across seven instruments that are not independent (ES/NQ/RTY are
  nearly one bet, and ZN/6E/GC co-move with rates and the dollar). The honest
  effective-N here is small enough that the M2.5 sample-size gate should be
  treated as binding, not advisory.
- **Their 58 instruments versus our 7.** The paper's headline Sharpe comes from
  diversification across 58 weakly-correlated markets. Seven markets, three of
  which are the same trade, will produce a much noisier portfolio, and any
  comparison to their reported figures is invalid. Ours is a *smaller* version
  of the strategy and must be reported as such.
- **The vol-scaling confound is real and pre-registered against:** the paper
  scales positions by inverse trailing volatility. Some of what looks like
  momentum alpha is volatility timing (see H-009 and its rebuttal H-010). A run
  of this strategy *without* vol scaling must be reported beside the scaled
  one, or the two effects cannot be told apart.

## Triage grade

**C.** The data is owned and the basket is right, but three things are missing
and one of them is a milestone rather than a task: a daily-bar resampler (code),
the roll-table artifact (a command), and **multi-instrument portfolio
accounting, which is explicitly post-M4**. Running a single-instrument version
on ES would be cheap and is *not* this hypothesis — the paper's claim is about
the portfolio, and a one-instrument test would answer a question nobody asked
while charging trials to this family.
