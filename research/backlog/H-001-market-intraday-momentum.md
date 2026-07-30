---
id: H-001
slug: market-intraday-momentum
topic: intraday-session
grade: B
hypothesis_family: es-intraday-momentum-open-close
status: backlog
created: 2026-07-30
---

# H-001 — Market intraday momentum: the first half-hour predicts the last half-hour

## Citation

Lei Gao, Yufeng Han, Sophia Zhengzi Li, Guofu Zhou, **"Market intraday
momentum"**, *Journal of Financial Economics* 129(2), August 2018, 394–414.

- Publisher: <https://www.sciencedirect.com/science/article/abs/pii/S0304405X18301351>
- Working paper: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2440866>
- RePEc: <https://econpapers.repec.org/RePEc:eee:jfinec:v:129:y:2018:i:2:p:394-414>

Their stated claim: using high-frequency S&P 500 ETF data from 1993–2013, the
first half-hour return on the market — measured from the *previous* day's close
— predicts the last half-hour return. They report the predictability as
stronger on more volatile days, higher-volume days, recession days, and major
macroeconomic news release days, and report it present in ten other actively
traded domestic and international ETFs.

## Mechanism

The economic story is about *forced trading concentrated at the close*.
A large class of market participants must be flat, or must be at a mandated
exposure, by the closing bell: leveraged and inverse ETFs have to rebalance in
the direction the market has already moved (up days force them to buy more),
intraday-only traders must unwind, and dealers who absorbed inventory in the
morning must lay it off before they carry overnight risk. None of these trades
is informed — they are mechanically determined by the day's realized move. If
the morning's move sets the size and sign of that closing demand, then the
morning return is a forecast of the closing pressure, and the last half-hour
prints a continuation of it. The losing side is explicit and named: **the
forced end-of-day rebalancer**, who pays for immediacy at a moment everyone can
predict, and the intraday dealer whose inventory must be flattened on a
schedule rather than on a price. Both keep doing it because their mandates are
not discretionary — a leveraged ETF that declined to rebalance would fail to
track its index, which is a worse outcome for it than paying the spread. The
second, weaker channel is under-reaction: slow investors who see the morning
move but only act at the close. That channel has no captive loser and should be
treated as the disposable half of the story.

## Signal in Crucible terms

- **Basket:** `ES` primary; `NQ` and `RTY` as the cross-instrument rhyme check
  (M3). All three are owned for the full span.
- **Timeframe:** the paper's native grain is 30 minutes. We own 1m and 1s.
- **Feature 1 — morning return:** the return from the previous RTH session's
  close to the price 30 minutes after this session's RTH open.
- **Feature 2 — the trading window:** a position taken at T−30min and forced
  flat at the RTH close.
- **Rule:** go long if feature 1 > 0, short if feature 1 < 0, hold through the
  final window, flatten at the close. Sign-following only — no threshold
  tuning at the proposal stage.
- **Pre-registered definitional choice** (made now, because it is a research
  degree of freedom, not an implementation detail): ES trades nearly 23 hours,
  so "the previous close" is ambiguous in a way it is not for SPY. This
  hypothesis is defined against the **RTH close (16:00 ET / 15:00 CT)** and the
  **RTH open (09:30 ET / 08:30 CT)**, so the "overnight" leg spans the entire
  Globex session. The alternative — anchoring to the Globex daily boundary — is
  a *different hypothesis* and must get its own file and its own family key
  rather than being tried as a variant of this one.

## Data

**Owned, sufficient:** `ohlcv-1m` for ES/NQ/RTY, 2010-06-06 → 2026-07-28. At
roughly 4,000 CME sessions this is a larger session count than the paper's
1993–2013 window, and the 1-minute grain is finer than the 30-minute grain the
signal is stated on.

**Missing — all code, no data:**
1. A **time-of-day predicate**. The combo grammar has no clock operand
   (`research/backlog/README.md` §2.1). This is the binding gap.
2. A **session-relative anchor**: "previous RTH close" and "RTH open" require
   `crucible-data::calendar`, which exists and knows the session table, but the
   engine may not depend on `crucible-data` (CLAUDE.md §3). The D-0071 pattern
   is the precedent and the fix: the CLI computes session keys **once** and
   hands both consumers the same slice, so two independent attributions of
   "which session" cannot disagree.
3. A **forced-flatten at a wall-clock time** — the `RiskLimits` trading-window
   item already scoped in M2.
4. A **return-over-a-window** feature, which is not an indicator we have.

Nothing here needs a purchase.

## Pre-registered kill criteria

Judged by machines, in this order. Any *Kill* line ends the idea.

**Gate 0 — predictor before system** (`docs/PROJECT_PLAN.md` §7.3). Evaluated
with no trading and no equity curve:
- Sign-match rate between the morning return and the last-half-hour return,
  pooled across all sessions, must exceed **52.0 %**, with the **lower bound of
  a 95 % block-bootstrap CI (block = 20 sessions) strictly above 50.0 %**.
  Otherwise **Kill**.
- Minimum sample: **1,500 sessions** in which both windows exist and the
  session is a full (non-early-close) session. Below that, **no verdict is
  issued at all** — not a pass, not a fail.

**Gate 1 — S1, `free_fills`:** if the sign-following rule is not profitable
before costs, **Kill**. A signal that cannot clear zero at zero cost has
nothing for the cost model to erode.

**Gate 2 — S2, walk-forward, `spread_cross` at 1 tick + $1.25/contract:**
- `min_oos_sharpe_after_costs = 0.5` — below it, **Kill**.
- `kill_if_dead_at_ticks = 1.0` — if the sweep is non-positive at 1 tick,
  **Kill**. ES at one tick is $12.50 on a $50-per-point contract and this
  strategy trades every session; the cost sensitivity is the whole question.
- Folds: `train_days = 504`, `test_days = 126`, `step_days = 126`.

**Gate 3 — S3:**
- `max_pbo = 0.5`.
- `require_plateau = true`, over the two window-length parameters (morning
  window, closing window). A spike at exactly 30/30 minutes and nothing at
  25 or 35 is a fit to the paper's own choice, not a plateau, and is a **Kill**
  rather than an iterate.
- **Cross-instrument:** the sign must hold on at least **2 of 3** of ES/NQ/RTY.
  One-of-three is a **Kill**, not an "ES is special".

**Gate 4 — decay check, pre-registered because the paper is eight years old:**
split the sample at the 2018 publication date. If the post-2018 half is
non-positive after costs while the pre-2018 half carries the result, the
verdict is **Kill**, and the file records "published-anomaly decay" as the
cause of death.

## Honesty note

- **Their data is not our data.** They study SPY, a cash-equity ETF on a
  6.5-hour session, 1993–2013. We would study ES, a futures contract on a
  ~23-hour session, 2010–2026. The instruments track the same index but the
  session structures differ, and the "overnight" leg means something different
  in each. This is a *related* test, not a replication, and the file should
  never be described as one.
- **Sample overlap is small and that is in our favour.** Their window ends in
  2013; ours starts mid-2010. The overlap is about three and a half years out
  of our sixteen — roughly 80 % of our sample post-dates their study. That is
  unusually good for a published anomaly, and it is the main reason this idea
  is worth compute at all.
- **The publication-decay prior is strong and against us.** The paper is
  well-cited and the strategy is simple enough to be traded by anyone who read
  it. Gate 4 exists because I expect this to be where it dies.
- **A known bias in their favour:** the reported conditioning ("stronger on
  volatile days, high-volume days, recession days, macro-news days") is four
  extra slices of the same sample. Conditional results found after the fact
  inflate significance. Our version pre-registers the unconditional test as the
  headline; if the unconditional test fails, a conditional rescue is **not**
  permitted under this family key.
- **Regime overlap:** our sample is dominated by a post-2010 equity bull market
  with two sharp volatility events (2018, 2020) and the 2022 drawdown. A
  close-following strategy that is really a long-volatility trade would look
  good here for reasons that have nothing to do with the mechanism.

## Triage grade

**B.** Every byte of data is owned and the sample is larger than the paper's.
The gap is entirely code, and it is code M2 already intends to write: a
time-of-day predicate, session anchors supplied caller-side as `&[i64]` keys in
the D-0071 pattern, a forced-flatten trading window, and a window-return
feature. It is the highest-value B in this sweep because the same four pieces
unlock H-002, H-003, H-004 and H-014.

---

## Changelog

Append-only. The registration above is never rewritten — a pre-registration
that gets edited after the fact is not one (README §1).

### 2026-07-30 — re-graded against the four grammar unlocks (D-0077…D-0080): **B → B**

**What closed.** The trading window is now expressible. The last half-hour of
RTH and the forced flatten at the close are:

```toml
enter_long = "is_rth > 0 and minutes_to_rth_close <= 30 and <feature 1> > 0"
exit_long  = "minutes_to_rth_close <= 0"
```

`minutes_to_rth_close` counts toward the *scheduled* regular close, and
`is_rth` is 1 only inside regular hours (D-0078).

**What still blocks, and why the grade does not move.** `<feature 1>` above is
not writable. The morning return is a return between two **anchored reference
prices** — the previous RTH session's close, and the price 30 minutes after
this session's RTH open. The grammar's operands are the *bar being decided on*
(`open`/`high`/`low`/`close`), its `volume`, trailing-window indicator slots,
and session clock readings. None of them captures a price **at a named past
instant** and holds it, and there is no arithmetic to form a return from two
such captures. Substituting a trailing-window return (`zscore(period,
source="return")`) would test a different hypothesis with the same name, which
§1 forbids.

The gap is now precisely one construct — an anchored reference price — rather
than the four this file originally listed.
