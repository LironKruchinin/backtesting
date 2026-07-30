---
id: H-009
slug: volatility-managed-exposure
topic: vol-regime
grade: B
hypothesis_family: futures-vol-managed-exposure
status: backlog
created: 2026-07-30
---

# H-009 — Volatility-managed exposure: take less risk when volatility is high

## Citation

Alan Moreira, Tyler Muir, **"Volatility-Managed Portfolios"**, *The Journal of
Finance* 72(4), August 2017, 1611–1644.

- Publisher: <https://onlinelibrary.wiley.com/doi/abs/10.1111/jofi.12513>
- Author copy: <https://amoreira2.github.io/alan-moreira.github.io/VolPortfolios_published.pdf>
- NBER working paper: <https://www.nber.org/papers/w22208>
- SSRN: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2659431>

Their abstract, verbatim: *"Managed portfolios that take less risk when
volatility is high produce large alphas, increase Sharpe ratios, and produce
large utility gains for mean-variance investors. We document this for the
market, value, momentum, profitability, return on equity, investment, and
betting-against-beta factors, as well as the currency carry trade. Volatility
timing increases Sharpe ratios because changes in volatility are not offset by
proportional changes in expected returns."*

**Read the last sentence carefully — it is the entire claim.** The strategy is
not a forecast of returns. It is an assertion about the *ratio*: volatility is
highly forecastable at short horizons, expected returns are not, so scaling
exposure by inverse forecast variance raises the Sharpe ratio mechanically,
without predicting direction at all.

## Mechanism

Volatility is persistent — today's realized variance forecasts tomorrow's with
real accuracy, which is one of the most robust facts in empirical finance. If
the expected return does *not* rise proportionally when volatility rises, then
the reward-per-unit-risk falls in high-volatility periods, and an investor who
holds constant notional exposure is systematically overpaying for risk exactly
when risk is expensive. Scaling down fixes the mismatch.

Who is on the losing side? This is where the mechanism is weaker than the
statistics, and the file should say so plainly. There is **no captive
counterparty here**. Nobody is forced to buy risk at bad prices; the claim is
that the marginal investor fails to adjust exposure as conditions change. If
that were a persistent free lunch, an enormous and unconstrained set of
investors could take it, which is the standard reason to distrust a result of
this shape. The most defensible reading is that the effect is compensation for
something — a strategy that de-risks in crises gives up the rebound, so it is
partly selling a lottery ticket rather than harvesting an inefficiency. H-010
is the published rebuttal, and it shares this file's `hypothesis_family` on
purpose.

## Signal in Crucible terms

- **Basket:** all seven parents. The paper's claim is about a broad class of
  risky assets, and the equity legs alone would not test it.
- **Timeframe:** exposure decisions daily; the variance estimate built from
  intraday returns.
- **Feature — realized variance:** the sum of squared 1-minute returns over a
  trailing window, which is a **better** variance estimator than the daily-return
  version the paper uses, and one our data supports directly.
- **Rule:** target position size ∝ 1 / forecast variance, applied to an
  underlying strategy — the market itself (long-only), or any other hypothesis
  in this backlog.
- **Point-in-time is mandatory and is the failure mode to watch.** The variance
  forecast may use only data available at the decision instant. A full-sample
  variance normalization would make this strategy look spectacular and would be
  pure lookahead (CLAUDE.md §2.1); it is the single easiest way to fake this
  result and the reason M2.5 pins all feature standardization to rolling
  statistics.

## Data

**Owned and unusually well-suited:** `ohlcv-1m` for all seven parents,
2010-06-06 → 2026-07-28. Realized variance from 1-minute returns is exactly what
this needs, and we have sixteen years of it across four asset classes. We even
own `ohlcv-1s`, which allows a sensitivity check on the sampling frequency of
the variance estimator — a known methodological choice in the realized-variance
literature.

**Missing — all code, no data:**
1. **Continuous position sizing.** `qty_contracts` is a fixed integer in the
   config and there is no exposure-scaling layer anywhere in the engine. This is
   the binding gap, and it is a real design question rather than a small one:
   futures trade in whole contracts, so "scale exposure by 0.4" on a
   one-contract position means holding zero or one, and the rounding *is* the
   strategy at small account sizes. The honest implementation needs either
   enough contracts for rounding to be second-order, or an explicit accounting
   of what integer rounding does to the result.
2. **A daily decision grain** (resampler) for the rebalancing schedule.
3. **A realized-variance indicator**, which is not one of the three we have.
4. **Multi-instrument accounting** for the full version — post-M4. A
   single-instrument version (scale ES exposure by ES's own inverse variance) is
   a legitimate reduced test and is what the kill criteria below judge.

## Pre-registered kill criteria

**Gate 0 — the premise, tested before the strategy.** The claim rests entirely
on "changes in volatility are not offset by proportional changes in expected
returns". So test that directly, with no trading:
- Regress forward realized return on trailing realized variance across
  non-overlapping windows. The premise holds if the slope is **not**
  significantly positive at the 5 % level.
- If forward returns *do* rise proportionally with variance in our sample, the
  mechanism is absent here and the strategy is **Killed before it is built** —
  regardless of what an equity curve would have shown.

**Gate 1 — Sharpe improvement, the paper's own metric.**
- The vol-managed version must beat the unmanaged version's Sharpe ratio on the
  same sample, same window, with the difference significant at the 5 % level
  under a block bootstrap (block = 20 sessions). Not significant → **Kill**.
- Sample minimum: **1,500 sessions** and **at least 3 instruments**, so a single
  lucky market cannot carry it.

**Gate 2 — costs, which the paper largely sets aside.**
- `kill_if_dead_at_ticks = 1.0`. Vol-managed strategies rebalance *most* when
  volatility is *highest* — precisely when spreads are widest. A cost model
  applied at a constant one tick is therefore optimistic for this strategy
  specifically, and that must be stated on every result until M4's measured
  time-of-day half-spread replaces it.
- `min_oos_sharpe_after_costs = 0.5`.

**Gate 3 — the integer-rounding control, specific to futures.** Run the
identical strategy at a position scale where rounding to whole contracts is
material and at one where it is not. If the Sharpe improvement survives only at
the unrounded scale, the result does not apply to any account this project could
plausibly model, and the verdict caps at `Iterate` with "not implementable at
our size" recorded.

**Gate 4 — S3:** `max_pbo = 0.5`; `require_plateau = true` over the
variance-estimation lookback. An improvement at a 21-day window that vanishes at
15 and 30 is a **Kill**.

**Gate 5 — the rebuttal is part of the pre-registration.** H-010's
out-of-sample protocol must be run on the same data as a paired test. This is
declared now so that running it is not optional after a favourable result.

## Honesty note

- **There is a well-known published rebuttal, and it is in this backlog as
  H-010, under the same `hypothesis_family` key.** That is deliberate: both are
  trials of one idea, and splitting them into two families to keep each trial
  count low would be exactly the dishonesty the registry exists to prevent.
- **Their data is US equity factors and currency carry, 1926–2015; ours is
  futures, 2010–2026.** No instrument overlap at all — we have no factor
  portfolios and cannot construct them. Our test is of the *principle* on a
  different asset class, and a failure here is not a refutation of their result.
- **The crisis-timing problem.** A strategy that de-risks in high volatility
  necessarily reduces exposure into and through crashes. In our 2010–2026
  sample, every major volatility spike (2011, 2015, 2018, 2020, 2022) was
  followed by a recovery within months. A sample composed almost entirely of
  V-shaped recoveries is *unfavourable* to vol-management on the way down and
  favourable on the way up, and sixteen years contains too few crises for that
  to average out. Effective-N on the question "does de-risking help in a crash"
  is roughly **five**.
- **Costs are understated by construction** — see Gate 2. This is the single
  most likely reason a positive result here would be wrong.
- **The mechanism names no loser.** Recorded again here because it is the most
  important sentence in this file: every other hypothesis in this backlog can
  point at somebody who is structurally obliged to pay. This one cannot, and
  ideas of that shape are the ones that fail out of sample.

## Triage grade

**B.** All the data is owned, and the 1-minute archive is *better* suited to
realized-variance estimation than the daily data the paper used. The gaps are
continuous position sizing, a realized-variance indicator, and a daily decision
grain — all code, all in M2/M3 scope, none requiring a purchase. The
multi-asset version is post-M4; the single-instrument version graded here is
not.

---

## Changelog

Append-only. The registration above is never rewritten — a pre-registration
that gets edited after the fact is not one (README §1).

### 2026-07-30 — re-graded against the four grammar unlocks (D-0077…D-0080): **B → B**

**What closed — two of the three named gaps.**

- **The realized-variance indicator.** `stdev(period, source="return")`
  (D-0080) is a trailing-window population standard deviation of bar-over-bar
  returns, and the `return` source costs one extra warmup bar which is added to
  the declared warmup, so §2.6's grid-wide alignment counts it.
- **The daily decision grain.** `timeframes = ["1d"]` (D-0077) gives
  trading-day bars anchored on the session open — not UTC days.

Worth recording beside them: this file's own warning — that a full-sample
variance normalization "would make this strategy look spectacular and would be
pure lookahead" — is now **enforced by the grammar rather than by discipline**.
D-0080's normalizers are trailing-window only, there is no full-sample variant,
and no config can name one.

**What still blocks.** The rule is `target position ∝ 1 / forecast variance`:
**continuous position sizing**. The combo grammar's four rules are booleans and
position size comes from a fixed `qty_contracts` in `[run]`. There is no sizing
axis, and adding one is an engine-and-config change, not a rule-grammar one.

A binary approximation — flat when `stdev` is above a threshold, on when it is
below — *is* writable today:

```toml
[indicators.rv]
kind = "stdev"
period = { start = 10, end = 60, step = 10 }
source = "return"

[rules]
enter_long = "rv < 0.004"
exit_long  = "rv >= 0.004"
```

That is **a different hypothesis** and must not be run under this file's
`hypothesis_family`. Volatility *timing* (binary) and volatility *scaling*
(continuous) make different claims, have different turnover, and the paper's
result is about the second. If the binary version is worth testing it gets its
own file and its own family key, exactly as H-001's Globex-anchored variant does.
