---
id: H-008
slug: short-horizon-overreaction
topic: momentum-horizon
grade: A
hypothesis_family: es-short-horizon-reversal
status: run
created: 2026-07-30
---

> **RUN 2026-07-31 — verdict `Kill`, decided at `admission`.** 24 combos, 24
> trials charged, determinism hash `a803247c25de44c7` reproduced across two runs.
> **S0 independently killed all 24 as well**, and the two facts are worth keeping
> apart: `|IC|` cleared the registered 0.02 bar at every horizon and the mean
> forward return's interval contained zero at every horizon, so the score has
> measurable *size* and no *significance* — exactly the failure mode D-0085's
> two-part criterion exists to catch. The IC sign is **negative at all four
> horizons**, which is the reversal direction this file predicted. **Gate 0b —
> the gate this hypothesis was built around — remains UNEVALUATED.** Full result
> at the end of this file.

> **UNBLOCKED 2026-07-31.** The S0 predictor seam landed (D-0085), so Gate 0
> and Gate 0b — no-trading measurements of forward returns conditional on the
> signal — are runnable in the registered order. `stages = ["s0", ...]` is
> accepted, and an `[s0]` block declares the score slot, the horizons, and the
> `|IC|` below which the score predicts nothing.
>
> **Two caveats before anyone runs this.** The seam measures forward returns as
> **fractions**, and Gate 0b registers its bar in **ticks** (one ES tick =
> 0.25 points = $12.50). Converting a mean fractional return to ticks at a
> given price level is arithmetic this build does not do for you — do it
> explicitly and write it down, because the whole point of Gate 0b is the
> comparison against the spread. And the sample ceiling is unchanged: one
> contract's life is ~60 sessions, which no adequacy criterion worth
> registering is satisfiable at, so the first run is a **triage run** until
> registry pooling lands (README §6.2, unlock 5).

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
# EXTRACTED: configs/hypotheses/H-008-short-horizon-overreaction.toml
# This file and the ```toml block in
# research/backlog/H-008-short-horizon-overreaction.md are asserted
# BYTE-IDENTICAL by crucible-cli/tests/backlog_registration.rs. Edit one, edit
# both — the registration and the config that runs are one artifact precisely so
# the registered grid and the run grid cannot differ.

schema_version = 1

[meta]
name = "es-short-horizon-reversal"
hypothesis_family = "es-short-horizon-reversal"
economic_rationale = "Impatient liquidity takers push price beyond fair value; inventory-bearing liquidity providers push it back as they unwind, so extreme short-horizon deviations partially revert."

[universe]
# EIGHT consecutive ES contracts, pooled. FOUR-digit years (D-0072): the vendor
# spelling `ESH4` has a one-digit year that repeats every ten years and our
# windows are sixteen years long.
#
# Eight rather than one because the registered 250-session floor is not
# satisfiable by one contract and never was — a single ES front window is ~64
# sessions (D-0119). Pooling is the sanctioned way to meet it, and it meets it
# by supplying sessions honestly rather than by lowering the bar (D-0114).
instruments = [
  "ESM2022",
  "ESU2022",
  "ESZ2022",
  "ESH2023",
  "ESM2023",
  "ESU2023",
  "ESZ2023",
  "ESH2024",
]
timeframes = ["1m"]

[pooling]
# Declared, never inferred from the symbols: a typo'd contract would otherwise
# silently define its own root, and a pool is a claim that these contracts are
# ONE instrument across time rather than a cross-instrument breadth claim.
root = "ES"

[data]
source = "curated"
# The span the pooled front windows live in. Each contract is evaluated on its
# OWN front-month window, which the `.v` volume roll table decides (D-0119);
# this range only has to contain them.
start = "2022-03-01"
end = "2024-03-20"

[contract]
tick_points = "0.25"
point_value_usd = 50

[indicators.bb]
kind = "bollinger"
period = { start = 5, end = 30, step = 5 }
k = [1.5, 2.0, 2.5, 3.0]

# The score S0 reads, verbatim from `configs/s0-smoke.toml` — D-0085's
# registered default, a trailing 20-bar z-score of the close. It is the
# continuous form of exactly what the rules below threshold: a close beyond a
# Bollinger band of period n and width k IS |z| > k at that same period, and the
# combo grammar has no arithmetic between operands, so a band cannot be turned
# into a graded deviation (this file's own honesty note says so). No rule
# references this slot, which is legal and intended — it exists to be measured,
# not to trade. `period = 20` is a fixed axis, so the grid stays 6 x 4 = 24
# combos and the warmup stays 30 bars, set by bb's longest period.
#
# CAVEAT, stated because it is a limitation of this registration and not of the
# result: the score is fixed at period 20 while the grid trades six periods, so
# every combo's S0 reading is the SAME series. S0 here is one measurement
# charged 24 times, not 24 measurements. Making it track bb.period would add a
# third grid axis (144 combos, most of them mismatched pairs) and has no
# conventional source, so it is not done.
[indicators.z]
kind = "zscore"
period = 20
source = "close"

[rules]
enter_long = "close < bb.lower"
exit_long = "close > bb.mid"
enter_short = "close > bb.upper"
exit_short = "close < bb.mid"

[execution]
fill_model = "spread_cross"
half_spread_ticks = 1
fee_per_contract_usd = "1.25"

# ---------------------------------------------------------------------------
# FOLD GEOMETRY — AMENDED 2026-08-07 (A4, D-0134). In TRADING DAYS (D-0062).
#
# **What it replaced, and why that had to go.** This file registered
# `5 / 2 / 2`, taken verbatim from `configs/combo-smoke.toml`. The intent was
# right — a fold layout invented per hypothesis is a free parameter chosen by
# the person who wants the result — but the source was wrong: combo-smoke's
# values are calibrated for a ~14-day SYNTHETIC fixture, so what this file
# inherited was not a convention but another file's fixture. D-0119 names that
# exact mistake as how H-007 and H-008 died at admission for a reason having
# nothing to do with their ideas.
#
# **The RATIO is conventionally sourced.** Pardo, *The Evaluation and
# Optimization of Trading Strategies*, 2nd ed. (Wiley, 2008), ch. 11: the
# walk-forward out-of-sample window is conventionally 10–20 % of the in-sample
# window. `4 / 21` = 19.0 %, inside that band and at its upper end — chosen at
# the upper end deliberately, because more out-of-sample is the conservative
# direction and §9's direction test says to take the choice that does not
# flatter the strategy.
#
# **The SCALE is the conventional trading month.** `train_days = 21` is one
# trading month, the standard count and the unit §4 already pins fold windows
# to. `test_days = 4` is 19 % of it rounded to a whole session, because a fold
# is a whole number of sessions and 4.2 is not one.
#
# **What is NOT sourced, stated rather than hidden.** The scale is *bounded*
# as well as conventional: D-0119 cuts folds INSIDE each contract's front
# window, so `train + test` must fit ~64 sessions whatever convention says.
# One trading month fits with room for ~10 folds; one trading QUARTER (63)
# would not fit at all. So the convention and the constraint agree here — but
# they agree by luck, and a root with shorter front windows would force the
# constraint to win. That is a limitation of the front-window pooling mode
# (D-0119), not a property of this geometry.
#
# `step_days == test_days` so the out-of-sample windows TILE rather than
# overlap: D-0062 refuses `step < test` because pooling overlapping windows
# counts a session twice, which is the double-count D-0114 forbids across
# contracts, applied within one.
# ---------------------------------------------------------------------------
[walk_forward]
scheme = "rolling"
train_days = 21
test_days = 4
step_days = 4

# Gate 0 and Gate 0b, pre-registered. Every value verbatim from
# `configs/s0-smoke.toml` (D-0085's registered defaults) except the horizons,
# which come from this file's own Gate 0 text — "the next 1, 5, 10 and 20
# minutes" — and which the two sources agree on anyway. Horizons are minutes
# rather than bars because `ohlcv` has no bar for an interval that did not trade
# (D-0082).
#
# Gate 0b is NOT evaluated by this build. S0 measures forward returns as
# FRACTIONS; Gate 0b registers its bar in ticks (one ES tick = 0.25 pt =
# $12.50), and converting between them at a price level is arithmetic this build
# does not do. The conversion must be done by hand and written down, or the
# comparison against the spread — the gate this whole hypothesis was built
# around — has not actually been made.
[s0]
score = "z"
horizons_minutes = [1, 5, 10, 20]
buckets = 5
bootstrap_draws = 500
min_abs_ic = 0.02

# ---------------------------------------------------------------------------
# COPY. Canonical source: `configs/example-combo.toml` (README §2.3).
# `s0` is declared and runs: the predictor seam landed 2026-07-31 (D-0085), so
# this file's Gate 0 is evaluated in the registered order — before any equity
# curve — which is what it was blocked on. `s3` is still refused at load rather
# than skipped, so Gates 3 and 4 remain unevaluated and the ceiling is `Iterate`
# (D-0075).
# ---------------------------------------------------------------------------
[funnel]
stages = ["s0", "s1", "s2"]
cost_sensitivity_ticks = [0.0, 0.5, 1.0, 2.0]
min_oos_trades = 200
min_oos_sessions = 250
min_oos_return_pct_free_fills = 0.0
min_oos_sharpe_after_costs = 0.5
kill_if_dead_at_ticks = 1.0
require_controls_beaten = true
max_pbo = 0.5                         # declared; EVALUATED since D-0109
require_plateau = true                # declared; NOT evaluated (S3)

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
  contracts** before any verdict, encoded as `min_oos_trades` /
  `min_oos_sessions`. One ES contract (~60 sessions) does not reach this and
  will be killed for sample adequacy — a **triage run with no profitability
  verdict**. The floors come down only when registry pooling supplies the
  sessions honestly (README §6.2, unlock 5), never to make a short run pass.
- **Ceiling is `Iterate`** (D-0075): S3's battery is unbuilt, so `Graduate` is
  not awardable by this build.

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

**A, and no longer `blocked`.** The strategy is expressible today with no new
Rust, and — unusually for this directory — the grain we can replay is the grain
the effect is claimed at. What used to block it was not expressibility but
**order**: its first two gates are predictor measurements, and the S0 seam that
performs them did not exist. It landed on 2026-07-31 (D-0085), and the `[s0]`
block above is what this file now registers against it.

**Gate 0b is still not evaluated by this build**, and that is the one caveat
worth carrying forward rather than the blocked status: S0 reports forward
returns as fractions, Gate 0b's bar is one tick, and nothing converts between
them. The gate this hypothesis was built around is the gate a reader must still
compute by hand.

That was a good problem rather than a bad one. Its most valuable property is
Gate 0b, which is designed to produce the answer *"the effect is real and
smaller than our costs"* — the most common true answer in short-horizon research
and the one a backtest-first approach is worst at reaching. Reaching it requires
measuring forward returns **without** trading, so building the seam is not
overhead for this hypothesis; it is the hypothesis. As the S0 predictor seam's
first consumer (D-0081), this file specified half of what that seam had to do:
bucket forward returns at 1/5/10/20 minutes conditional on a signal, with a
block bootstrap over sessions, and report the result in ticks so it can be
compared against the spread. The other half is the quantile/IC contract in
`crucible-funnel::stages`' module doc. The seam shipped the first half and the
bootstrap; the **report-in-ticks** half is the piece still owed, which is why
Gate 0b remains a hand calculation.

## Result — 2026-07-31

Run: `crucible funnel --config
configs/hypotheses/H-008-short-horizon-overreaction.toml --out results`, twice.
Exit 5 both times. Determinism hash `a803247c25de44c7` and all 24 verdict rows
identical across the two runs; the scorecards differ only in the render
timestamp, the trials-before figure, and the registry claim/dedupe counts.

**Verdict: `Kill`, decided at `admission`, 24 of 24 combos.** Trials charged: 24.
Config hash `43e43ca1748d…`. The recorded deciding gate is `admission` because
admission precedes every stage (D-0084) — 58 pooled out-of-sample sessions
against the registered 250. Round-trips were never binding: 551 to 10,569 against
a floor of 200.

### Gate 0 — predictor before system. Evaluated. KILL.

Every combo, at all four registered horizons:

| horizon | pairs | IC | mean fwd return, 95 % CI |
|---|---|---|---|
| 1m | 86,082 | **-0.0372** | +0.00009 % [-0.00005 %, +0.00021 %] |
| 5m | 86,244 | **-0.0335** | +0.00043 % [-0.00016 %, +0.00101 %] |
| 10m | 86,241 | **-0.0291** | +0.00086 % [-0.00018 %, +0.00201 %] |
| 20m | 86,235 | **-0.0261** | +0.00168 % [-0.00063 %, +0.00379 %] |

Two things, and they point opposite ways:

- **The sign is right.** A negative information coefficient means a high z-score
  is followed by a negative return — reversal, which is what this file
  pre-registered. It is not a large effect and it is consistent across all four
  horizons, decaying as the horizon lengthens, which is the shape the mechanism
  predicts.
- **The significance is absent.** `|IC|` clears the registered `min_abs_ic =
  0.02` at every horizon, but the mean forward return's bootstrap interval
  contains zero at every horizon, so no horizon clears **both** halves of
  D-0085's criterion → `KILL at s0`. That is the criterion working as designed:
  on 86,000 observations, an `|IC|` of 0.037 is what a large enough sample of
  noise gives away for free.

**Every combo's S0 reading is identical**, as this file's own config caveat
predicted: the score slot is fixed at `zscore(20)` while the grid varies only the
Bollinger parameters, so S0 here is *one* measurement charged 24 times. Only the
bootstrap intervals differ, and only because each combo draws its own seed.

### Gate 0b — the spread test. UNEVALUATED, and this is the run's biggest gap.

Gate 0b asks whether the measured reversion exceeds one full tick (0.25 pt =
$12.50) per round trip at the 10-minute horizon. **It could not be evaluated**,
for two compounding reasons — neither of which is a property of the market:

1. **The printed mean forward return is unconditional.** It is the same for all
   24 combos and at +0.00086 % per 10 minutes it is the window's drift, not the
   reversion conditional on a close beyond the band. (ES returned +17.60 % over
   this window; the two are consistent.) Gate 0b needs the *conditional*
   magnitude.
2. **The quantile buckets that would supply it are computed and never shown.**
   `[s0].buckets = 5` is declared and pre-registered, `crucible-funnel::s0`
   computes `report.buckets` with a per-bucket `mean_return`, and
   `crucible-cli::funnel::print_s0` prints none of it. Nothing in the registry
   or the scorecard carries it either.

Converting a fraction to ticks by hand was the known caveat (recorded in the
banner above and in the config). **The bucket omission is new**, and it means
the hand conversion has no input. Flagged rather than fixed here — see the
session report; it is a reporting defect of the same family as D-0100, not a
result.

### Gates 1 and 2 — reached, and not the cause of death.

Under `free_fills` several combos were profitable (up to +8.28 %), and all of
them died as soon as the spread was charged: combo 21 goes +2.73 % at 0 ticks to
-52.68 % at 1 tick to -108.09 % at 2. **That is the mechanism this file
predicted** — the compensation being collected is the spread, and we are paying
it to enter. It is the expected cause of death, and the sample gate means it is
recorded as an observation rather than as a verdict. Every combo beat the matched
random-entry control 16 of 16 draws; every combo lost to buy-and-hold (0 of 1).

### Gates 3 and 4 — not run.

`max_pbo` and `require_plateau` echoed, not evaluated (S3 is owed). CL was not
run: Gate 4 is a second registration, not an extension of this one.
