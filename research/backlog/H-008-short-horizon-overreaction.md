---
id: H-008
slug: short-horizon-overreaction
topic: momentum-horizon
grade: A
hypothesis_family: es-short-horizon-reversal
status: backlog
created: 2026-07-30
---

# H-008 — Short-horizon overreaction and reversal

## Citation

Primary:

Szymon Lis, Robert Ślepaczuk, Paweł Sakowski, **"Overreaction as an indicator
for momentum in algorithmic trading: A Case of AAPL stocks"**,
arXiv:2602.18912 (published 2026-02-21).

- <https://arxiv.org/abs/2602.18912>

Their stated claim: machine-learning methods detect emotion-driven intraday
overreactions, with behavioural momentum effects **peaking around a 10-minute
horizon**.

Supporting, and closer to our instrument:

Dmitrii Vlasiuk, Mikhail Smirnov, **"Push-response anomalies in high-frequency
S&P 500 price series"**, arXiv:2511.06177 (published 2025-11-09) — large
historical price "pushes" correlate with delayed responses, with asymmetric
liquidity following sell-shocks.

## Mechanism

A large, impatient order moves price further than the information it carries
justifies, because it must cross the book to get done. Two things then happen.
Liquidity providers who absorbed the order hold inventory they did not want and
are compensated for taking it — they push price back toward fair value as they
unwind, which is the reversal. And traders who read the move as information
follow it, extending the overshoot before it retraces, which is why the effect
looks like brief momentum before it looks like reversion. The losing side is
named and structural: the **impatient liquidity taker**, who pays the spread
plus impact for immediacy, and who keeps doing it because their need to be
done — a redemption, a hedge, a risk limit — is not negotiable. The asymmetry
in the second paper is the interesting refinement: liquidity behaves differently
after sell-shocks than after buy-shocks, so the reversal is not symmetric, and a
symmetric rule averages two different phenomena.

The reason to be sceptical up front: the compensation paid to the liquidity
provider **is the spread**. Any strategy trying to collect this from outside the
book, by crossing the spread to enter, is trying to earn the market maker's
income while paying the market maker's fee.

## Signal in Crucible terms

Bollinger mean-reversion — structurally the config that ships as
`configs/example-combo.toml`, retargeted at a short horizon and at a raw
contract that can actually be replayed.

- **Basket:** ES first; then CL, whose different session and volatility regime
  makes it a real test rather than a third equity index.
- **Timeframe:** `1m`, which for once is the *right* grain — the primary paper's
  effect peaks near 10 minutes, so a 1-minute bar resolves it directly and no
  resampler is needed.
- **Features:** `bollinger(period, k)` on close; the band is a rolling standard
  deviation, so the deviation measure is point-in-time by construction and
  carries no full-sample statistics (CLAUDE.md §2.1).
- **Rules:** fade a close outside the band, exit at the mid.

Runnable today:

```toml
schema_version = 1

[meta]
name = "es-short-horizon-reversal"
hypothesis_family = "es-short-horizon-reversal"
economic_rationale = "Impatient liquidity takers push price beyond fair value; inventory-bearing liquidity providers push it back as they unwind, so extreme short-horizon deviations partially revert."

[universe]
instruments = ["ESH4"]
timeframes = ["1m"]

[data]
source = "curated"
start = "2024-01-01"
end = "2024-02-01"

[contract]
tick_points = "0.25"
point_value_usd = 50

[indicators.bb]
kind = "bollinger"
period = { start = 5, end = 30, step = 5 }
k = [1.5, 2.0, 2.5, 3.0]

[rules]
enter_long = "close < bb.lower"
exit_long = "close > bb.mid"
enter_short = "close > bb.upper"
exit_short = "close < bb.mid"

[execution]
fill_model = "spread_cross"
half_spread_ticks = 1
fee_per_contract_usd = "1.25"

[funnel]
stages = ["s0", "s1", "s2", "s3"]
cost_sensitivity_ticks = [0.0, 0.5, 1.0, 2.0]
min_oos_sharpe_after_costs = 0.5
max_pbo = 0.5
require_plateau = true
kill_if_dead_at_ticks = 1.0

[run]
seed = 42
initial_cash_usd = "100000"
qty_contracts = 1
```

The **asymmetry** the second paper reports is deliberately *not* in this config,
because testing it means running long-only and short-only variants and comparing
— which doubles the trial count and must be a pre-registered second stage, not a
thing discovered by staring at the first result.

## Data

**Owned and sufficient:** curated `1m` bars, and the 1-minute grain matches the
horizon the effect is claimed at. `tbbo` for ES (2025-07-28 → 2026-07-28) would
let M4 replace the hand-set one-tick half-spread with a *measured* one by time
of day — which for this hypothesis is not a refinement but close to the whole
question.

**Sample ceiling:** one contract's life per config (D-0042). Same constraint as
H-007.

**Not needed:** no purchase.

## Pre-registered kill criteria

**Gate 0 — predictor before system.** Before any equity curve: conditional on a
close beyond the band, the mean forward return over the next 1, 5, 10 and 20
minutes must be **opposite in sign to the deviation** at a block-bootstrap 95 %
CI excluding zero for at least the 5- and 10-minute horizons. If the forward
returns do not revert, there is nothing here and the rest is not run → **Kill**.

**Gate 0b — the spread test, which is the one that matters.** The mean reversion
measured in Gate 0 must exceed **one full tick (0.25 ES points = $12.50)** per
round trip at the 10-minute horizon. This is pre-registered because the
mechanism *predicts* the effect is approximately the liquidity provider's
compensation — i.e. approximately the spread. An effect smaller than the spread
is real, publishable, and untradeable from where we sit, and this project's
whole purpose is to say that out loud rather than to discover it after building
a system. Below one tick → **Kill**, recorded as "real but inside the spread".

**Gate 1 — S1, `free_fills`:** must be profitable costless. If not, **Kill**.

**Gate 2 — S2, walk-forward, `spread_cross`:**
- `min_oos_sharpe_after_costs = 0.5`; `kill_if_dead_at_ticks = 1.0`.
- Sample minimum: **200 round trips** and **250 sessions pooled across
  contracts** before any verdict. One month of ESH4 does not reach this, so a
  single-contract run is a **triage run with no verdict**.

**Gate 3 — S3:** `max_pbo = 0.5`; `require_plateau = true` over both `period`
and `k`. A result at `k = 2.5` with nothing at 2.0 or 3.0 is a spike, and the
grid above is 6 × 4 = 24 combos specifically so a plateau has room to show
itself.

**Gate 4 — cross-instrument:** must hold on CL as well as ES, or the verdict
caps at `Iterate`. Microstructure-driven effects should rhyme across liquid
futures; one that appears only on ES is more likely a property of that
contract's tick size than of the mechanism.

## Honesty note

- **The primary paper is about AAPL equity, not index futures**, uses machine
  learning to detect "emotion-driven" overreaction, and is a single-name study.
  The transfer to ES is weak on every axis: different instrument, different
  microstructure, different participant mix, and a mechanism stated in
  behavioural terms that this file has deliberately restated in inventory terms
  because the inventory story names a loser and the behavioural one does not.
  It is cited as a horizon estimate — "around ten minutes" — and for very little
  else.
- **This is the most-mined idea in the whole backlog.** Short-horizon reversal
  is decades old, and the version of it that survives is the one executed by
  participants sitting *inside* the book with negative fees. We would be
  crossing the spread to enter. Gate 0b exists because I expect that to be the
  cause of death, and I would rather pre-register the expectation than discover
  it.
- **ES's tick is large relative to its 1-minute volatility.** A one-tick
  half-spread on a contract whose typical 1-minute range is a few ticks means
  the cost is a large fraction of the signal. This is the specific reason the
  cost sweep is not a formality here.
- **Sample overlap:** the AAPL paper is 2026 and the S&P push-response paper is
  late 2025; both post-date most of our archive. Neither reports a sample we
  would be re-using, so overlap is not the concern — transferability is.
- **`bollinger` gives us the deviation but not a normalized one.** The grammar
  has no arithmetic between operands, so we can ask "is close beyond the band"
  but not "how many band-widths beyond". That coarsens the test and is worth
  remembering when reading a null result: we tested a threshold crossing, not a
  graded response.

## Triage grade

**A.** Runnable today with no new Rust, and — unusually for this directory —
the grain we can replay is the grain the effect is claimed at. Its most valuable
property is Gate 0b: it is designed to produce the answer "the effect is real
and smaller than our costs", which is the most common true answer in
short-horizon research and the one a backtest-first approach is worst at
reaching.
