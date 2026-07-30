# Account evaluation — specification

**Status: SPEC. Nothing here is implemented.** The evaluation engine lands with
the funnel in **M3**; this document exists so that M3 implements it without
re-deciding anything. The account rules themselves are already data, in
`configs/accounts/*.toml`, verified against each firm's own pages on
**2026-07-30**.

**Scope.** In: the methodology, the engine capture contract, and the metric
battery. Out: the implementation, and anything to do with live trading — order
routing is never in scope for this project (`docs/MILESTONES.md`, Post-M4).
Deflated Sharpe, PBO/CSCV and permutation nulls are M3 items specified in
`crucible-funnel::stats`; this document *depends* on them (§9) and does not
respecify them.

**The one thing this document exists to prevent.** An intraday trailing
threshold evaluated on daily closing marks will systematically **understate**
breach probability. Not "approximately" — the bias has a sign. The maximum
decline from a running high-water over a path is never smaller than the decline
measured between two endpoints of that path, so a close-only estimate of
P(breach) is a lower bound on the truth, always, by construction. Call it the
**endpoint fallacy**. It is the cheapest available way to publish a number that
is optimistic and looks careful, and every requirement below that seems
expensive is there because the cheap version is wrong in the flattering
direction.

---

## 1. What is being asked

Given a strategy that survived the funnel, and an account whose rules are
pre-declared as data: *what happens to this strategy inside this account?* The
answers are probabilities over paths, not a single equity curve:

- P(breach before target) — the account dies before it passes
- the distribution of days-to-target, median reported *with* the distribution
- P(pass evaluation), which is not `1 − P(breach)` because targets have
  side conditions (minimum trading days, consistency, a target that must be
  standing at a close)
- expected payout cadence under the firm's consistency rules
- for `personal_*`: risk of ruin, because a cash account cannot be breached

An account rule is a **path functional**. That is the whole difficulty. Total
return, Sharpe and max drawdown are functionals the existing `Summary` already
computes; "did cumulative PnL ever come within $2,000 of its own running
high-water, measured on unrealized equity, before it first closed a day $3,000
up" is not, and no amount of post-processing a per-bar equity vector into
statistics recovers it once the path resolution is gone.

---

## 2. The normalized account model

### 2.1 Everything in cumulative-PnL space

Define, for run time `t`,

```
cum(t) = equity_nano_usd(t) − equity_nano_usd(anchor)
```

taken from the engine's existing mark-to-market equity series
(`crucible-engine::replay`, step 2), with `anchor` the bar before the evaluation
window opens — the same anchor convention
`crucible-funnel::walkforward::window` already uses, and for the same reason:
the move *into* the first bar is the window's first return.

Cumulative PnL, not balance, because the firms' own arithmetic is only
consistent there. A Topstep Trading Combine starts at $50,000 with its limit at
$48,000; a Topstep Express Funded Account starts at **$0** with its limit at
**−$2,000** and locks at $0. Those are the same account in cum-PnL space
(floor at −2,000, lock at 0) and two different accounts in balance space. One
model, and the firm's label — buying power or starting cash — stays a display
detail.

All of this is integer nano-USD (§2.3). No `f64` enters the breach arithmetic;
floats appear only where a *statistic* is reported.

### 2.2 The threshold, in one formula

Let `D` be `drawdown.amount_usd`, `L` be `drawdown.ratchet_lock_at_cum_pnl_usd`
(absent ⇒ `+∞`), and let `B(s)` be the **ratchet input**:

| `ratchet_basis` | `B(s)` is | accounts |
|---|---|---|
| `peak_equity_including_unrealized` | `cum(s)` at every mark, unrealized included | Apex (all phases), TPT PRO |
| `highest_daily_closing_equity` | `cum(s)` only at the firm's daily close | Topstep Combine/XFA, TPT Test |

Then

```
peak(t)      = max( 0, max_{s ≤ t} B(s) )
threshold(t) = min( peak(t) − D , L )
breach at t  ⟺ cum_including_unrealized(t) ≤ threshold(t)
```

Three properties worth stating because each one is a bug someone will otherwise
introduce:

1. `peak` starts at 0, so `threshold` starts at `−D`. A day-one loss of `D`
   breaches; there is no grace.
2. The breach test uses **unrealized-inclusive** equity for *every* account in
   `configs/accounts/`, including the ones whose ratchet is end-of-day. Every
   firm that documents it says so in those words. "EOD account" describes the
   ratchet only.
3. `≤`, not `<`. Touching the threshold is a breach — Apex, Topstep and TPT
   all say "touches or falls below", and all three add that a liquidation
   filling *back above* the threshold does not undo it.

**This formula reproduces every worked example on every firm's page**, which is
why it is the formula. Those examples become the golden fixtures (§5).

| account | `D` | ratchet | `L` | firm's own example |
|---|---|---|---|---|
| `apex_50k` | 2,000 | peak incl. unrealized | +3,000 | peak 50,900 ⇒ threshold 48,900; locks at 53,000 when peak hits 55,000 |
| `apex_50k` PA (not encoded) | 2,000 | peak incl. unrealized | +100 | locks at 50,100 when peak hits 52,100 |
| `topstep_50k` | 2,000 | highest daily close | 0 | +500 day ⇒ 48,500; −500 next day ⇒ still 48,500 |
| `tpt_25k` Test | 1,500 | highest daily close | 0 | +1,000 day ⇒ 24,500; stops at 25,000 |
| `tpt_25k` PRO (not encoded) | 1,500 | peak incl. unrealized | 0 | unrealized +1,000 ⇒ 24,500 in real time |

Take Profit Trader is the useful case: the *same* firm, the *same* `D`, the
ratchet changing between Test and PRO. Any difference the model reports between
those two is the ratchet and nothing else.

### 2.3 What a daily loss limit is, and is not

A DLL is a **lockout**: positions flatten, the day ends, the account lives.
Modelling it as a breach overstates failure; ignoring it lets a simulated day
keep losing past the point the firm would have stopped it. Its effect on
P(pass) is **not sign-definite** — a lockout truncates bad days *and* good ones
— so it is a declared choice, not a conservative one, and §4.1 requires the
headline reported both ways.

---

## 3. Part A — the engine capture contract

### 3.1 What the engine already produces

Facts, from `crates/crucible-engine/src/`:

- `replay.rs` step 2 marks the portfolio to the bar close and pushes one
  `(Ts, NanoUsd)` equity point **per bar**. This is the mark-to-market loop.
  There is exactly one, and it is the only place in the workspace where
  unrealized PnL is evaluated against a price.
- `portfolio.rs` tracks `episode_net` across the currently-open round-trip and
  pushes a `ClosedTrade { closed_ts, net_nano_usd }` when the position returns
  to flat. `FeeEvent { ts, fee_nano_usd }` per costed fill.
- `Portfolio::unrealized_nano_usd()` is defined between `mark` calls and is the
  quantity every series below is a function of.
- `walkforward::window::RunTrace` already indexes trades and fees against bar
  positions in the equity curve — a merge, not a search.

So the capture is an **extension of an existing loop**, not a new pass. Four
series are required. Each names its sampling point, its type, and its memory.

### 3.2 Series 1 — per-trading-day PnL, calendar-sliced

```rust
pub struct DayRecord {
    /// The firm's trading day, from `Calendar::trading_day`.
    pub trading_day: CivilDate,
    /// Bar index range of this day within the run's equity series.
    pub bars: Range<usize>,
    /// cum(day close) − cum(previous day close). Fees included.
    pub close_pnl_nano_usd: NanoUsd,
    /// Max of cum(t) − cum(day open) over the day.
    pub peak_from_open_nano_usd: NanoUsd,
    /// Min of cum(t) − cum(day open) over the day.
    pub trough_from_open_nano_usd: NanoUsd,
    /// Largest decline from the running high-water WITHIN the day. §3.3.
    pub max_drop_from_running_peak_nano_usd: NanoUsd,
}
```

**Sampling point:** replay step 2, after `portfolio.mark(bar.close)` and after
`equity.push(...)`. One `DayRecord` closes when `Calendar::trading_day(ts)`
changes.

**Trading days per `crucible-data::calendar`, never wall-clock days.** CLAUDE.md
§4 pins fold windows in trading days for exactly this reason and the reason is
sharper here: `cme_globex_equity_index` rolls the trade date at 17:00 CT, so a
loss taken at 18:30 CT Monday belongs to **Tuesday**. A wall-clock slicer books
it on Monday, which mis-dates the daily PnL, mis-orders the ratchet on every
EOD account, and mis-attributes the best day that the consistency rules key on.
Three wrong numbers from one wrong boundary.

The firm's day is not always the exchange's day. Topstep flattens at 15:10 CT
while the exchange runs to 16:00 CT; `[session].flat_by_local_time` carries it.
The roll point is identical, so `Calendar::trading_day` is reusable as-is; the
15:10–16:00 CT window is exchange-open and firm-closed, and a strategy holding
through it is running something the account does not permit. Report it, do not
silently clip it.

**Memory:** 72 bytes per session (`CivilDate` 24 + `Range<usize>` 16 + four
`i64` 32). Sixteen years is 252 × 16 = **4,032 sessions ⇒ 290 KB**, independent
of the replay timeframe. Retain in full, always.

### 3.3 Series 2 — the intraday unrealized-equity high-water series

The running high-water of `cum(t)` including unrealized PnL, and the running
decline from it:

```rust
/// Updated once per mark. Both fields are monotone within their scope.
pub struct HighWaterState {
    pub peak_cum_nano_usd: NanoUsd,          // max over the run so far
    pub max_drop_from_peak_nano_usd: NanoUsd, // max over the run so far
}
```

**Sampling point: the existing mark-to-market loop, `replay.rs` step 2,
immediately after `portfolio.mark(bar.close)`.** Not a second pass. Not a
post-processing step.

**Required for all sixteen accounts, including every `highest_daily_closing_equity`
one.** The ratchet basis decides what advances the threshold; it decides nothing
about what is tested against it, and every firm here tests intraday on
unrealized equity (§7.0). There is no account in `configs/accounts/` for which
this series can be skipped, and "it's an EOD account" is not a reason — it is the
reason eight of the sixteen would otherwise be evaluated wrongly.

**HARD REQUIREMENT: this series is never reconstructed from bars after the
fact.** Reconstruction — walking the OHLC series afterwards and inferring what
the equity path must have been — re-derives the path from high and low prints,
and the order of a bar's high and low is not in the data. The engine already
resolves that ambiguity, once, in its fill model: `spread_cross` and the M2
stops/targets work adopt a **worst-case intrabar ordering** convention, and
every fill, every position and therefore every mark downstream is a consequence
of that choice. A reconstruction re-opens the question with its own, different
convention, and the drawdown it computes is then the drawdown of a path the
account never took. Two paths that produce identical bars can differ by more
than `D`; the whole point of a first-passage metric is that it depends on the
part of the path the bars do not record. Capture it where it is known, or do
not report it.

**Memory.** This is where the honest answer is "no".

Bars per year come from `Calendar::bars_per_year`: a 23-hour session over 252
sessions is 347,760 `1m` bars and 20,865,600 `1s` bars.

| replay grain | bars, 16 y | per-bar equity `Vec<(Ts, NanoUsd)>` @16 B | a second per-bar series @16 B |
|---|---|---|---|
| `1d` | 4.0 k | 65 KB | 65 KB |
| `1m` | 5.56 M | 89 MB | 89 MB |
| `1s` | **334 M** | **4.98 GiB** | **+4.98 GiB** |

**Verdict for a 16-year 1-second series: it cannot be retained, and neither can
the per-bar equity vector the engine builds today.** `replay.rs` allocates
`equity: Vec<(Ts, NanoUsd)>` unconditionally, so a 16-year 1s replay already
costs ~5 GiB before any account series is added; retaining a second per-bar
series doubles it, on a machine where the bar data itself is also resident
(`ParquetBarFeed` loads into RAM). M3 requirements, therefore:

1. `HighWaterState` is a **streaming reducer**: O(1) state, two integer
   comparisons per mark. It is never a `Vec`. Nothing is lost, because the
   breach question is a running max and a running max-drop.
2. The per-bar equity vector needs an opt-out for long 1s runs, and the
   summaries below must be sufficient without it.
3. Per-day summarisation (§3.2) is the retained artifact. 290 KB versus
   4.98 GiB — four orders of magnitude — and §3.3.1 proves it loses nothing
   that a breach test needs.

#### 3.3.1 The per-day summary is exactly sufficient (proof)

The bootstrap (§4.3) shuffles whole days, so it needs to answer "does this
resampled concatenation of days breach?" without the intraday paths. Claim: for
a day carrying in a peak `h` (measured relative to the day's opening `cum`),
whose lock has not engaged and cannot engage during the day, the four numbers in
`DayRecord` decide it exactly.

Write everything relative to the day's open. Breach ⟺
`∃t: cum_rel(t) ≤ max(h, runmax_rel(t)) − D`, and `runmax_rel` is
non-decreasing, so the times at which the carry-in `h` is the binding term form
a prefix. Claim: breach ⟺

```
trough_from_open ≤ h − D    OR    max_drop_from_running_peak ≥ D
```

*Sufficient.* If `trough_from_open ≤ h − D` then at the trough time `t`,
`threshold(t) = max(h, runmax_rel(t)) − D ≥ h − D ≥ cum_rel(t)` — the whole-day
minimum can be used even though `h` binds only on a prefix, because the
threshold only ever rises above `h − D`. If instead the running decline from an
intraday peak reaches `D`, that peak is ≥ the threshold's own reference and the
breach is immediate.

*Necessary.* Suppose neither holds. At any `t`: if `runmax_rel(t) ≤ h` the
threshold is `h − D` and `cum_rel(t) ≥ trough_from_open > h − D`; if
`runmax_rel(t) > h` the threshold is `runmax_rel(t) − D` and the decline from the
running peak is `< D`. No `t` breaches. ∎

`peak_from_open` and `close_pnl_nano_usd` then advance the ratchet and the
running `cum` into the next day, so the recursion closes.

**The one place this is approximate, stated rather than absorbed.** When a
`ratchet_lock` engages *inside* a day, the threshold stops rising mid-day and
the day's whole-day trough is tested against `L` — which can declare a breach
that happened before the lock at a level that was legal at the time. (A day
whose lock was *already* engaged on entry is exact again: the threshold is the
constant `L` all day, so `trough_from_open ≤ L` decides it. Only the crossing
day is approximate, and there is at most one per path.) The bias
is **conservative** (it over-declares breaches, so P(pass) comes out too low),
and the fix is bounded: retain the full intraday path for days on which the
lock could engage, which is at most one day per run, and fall back to the
conservative test only with the affected-resample count printed. A number that
is wrong in the safe direction and says so is acceptable; one that is wrong in
the flattering direction is not.

#### 3.3.2 Mark granularity is a reported assumption, not a detail

The finest resolution available is the replay timeframe's mark grid. The engine
marks at bar closes, so a 1-minute replay tests the threshold 1,380 times a
session and a daily replay tests it once. Both are *lower bounds* on the true
number of tests, hence lower bounds on P(breach). Consequences, all mandatory:

- Every reported breach probability names its mark grain, next to the number,
  like a fill model (§2.4). `P(breach) = 0.41 (marks: 1m closes)`.
- An account whose `ratchet_basis` is `peak_equity_including_unrealized` is
  **refused** on a `1d` replay. The answer would be a bound so loose it is
  not an answer, and refusing costs a config edit (D-0029's calculus).
- Where a 1s archive exists, the intraday-trail headline is computed on 1s and
  the 1m number is printed beside it. If they differ materially, the 1m number
  was optimistic — which is the finding, not a nuisance.
- When M2's worst-case intrabar ordering lands, the mark becomes the worst-case
  intrabar extreme and every breach probability rises. That is a semantics
  change: decision-log entry, re-derived goldens (`testdata/README.md` rule 3).

### 3.4 Series 3 — MAE / MFE per round-trip

Maximum adverse and favourable excursion of the open position's unrealized PnL,
per **round-trip** (CLAUDE.md §4's episode: position leaves flat and returns to
flat).

```rust
pub struct ClosedTrade {           // extends the existing struct
    pub closed_ts: Ts,
    pub net_nano_usd: NanoUsd,
    pub opened_ts: Ts,             // new
    pub mae_nano_usd: NanoUsd,     // new, ≤ 0
    pub mfe_nano_usd: NanoUsd,     // new, ≥ 0
}
```

**Sampling point:** the same mark, step 2, guarded on `position != 0`.
`Portfolio` already carries `episode_net` across an open round-trip; MAE/MFE are
two more running extremes on the same lifecycle, reset when the position flattens
and moved into `ClosedTrade` there. Excursions are measured on episode
unrealized-plus-realized-so-far (a flip through zero closes one episode and
opens another, so the existing `apply_fill` boundary is the right one).

**Attribution follows the close** (D-0063): a round-trip opened in a training
window and closed in a test window is a test-window trade. MAE/MFE inherit that,
unchanged, so a fold's excursion distribution and its trade count agree.

**Memory:** 40 bytes per round-trip. An intraday strategy over 16 years might
close 10 k–50 k round-trips ⇒ **≤ 2 MB**. Retain in full at every grain. This
is the series that answers "how close did this strategy come, trade by trade,
without ever technically breaching" — the near-miss distribution that separates
a strategy that survives an account from one that survived a sample.

Same granularity caveat as §3.3.2: bar-close MAE is a lower bound on true MAE.

### 3.5 Series 4 — the worst-day distribution

**Derived, not captured.** It is the order statistics of
`DayRecord.close_pnl_nano_usd` over the pooled out-of-sample days, plus — because
a DLL and a daily-loss question care about the path, not the close — the order
statistics of `trough_from_open_nano_usd`. Reported as the full empirical
distribution with the 1st, 5th, 10th, 50th percentiles and the single worst day,
never as a mean.

Two distributions, not one, and the pair is the point: the gap between "worst
close" and "worst trough" is the amount of a bad day that a daily-close model
never sees. When they diverge, §3.3.2's caveat is quantified rather than
asserted.

### 3.6 Capture invariants

- **Integer accounting (§2.3).** Every field above is `NanoUsd`. No `f64` in
  the breach path, the ratchet, or the summaries. Percentages appear only in
  reported statistics, as `_pct` f64 on 0–100.
- **Determinism (§2.2).** Capture is a fold over the event stream in
  `avail_ts` order; no `HashMap` iteration reaches a result, no clock is read.
  Bootstrap seeds derive from `(config_hash, account_id, combo_index, fold,
  draw_index)`, extending D-0064's lineage — never from time or thread id.
  `account_id` joins the derivation because two accounts on one strategy are
  two runs, and they must not share a resampling draw.
- **`avail_ts` only (§2.1).** Day boundaries, ratchet advances and breach tests
  key on `avail_ts`. `ts_open` appears in display only.
- **The account's position cap is checked, not clamped.** If the strategy
  config's `qty_contracts` exceeds `[position_limit].max_contracts`, the run is
  **refused**. Silently trading a smaller size answers a question nobody asked;
  `crucible combo` already refuses a two-instrument config for the same reason.

---

## 4. Part B — the metric battery, per account config

### 4.1 Headline metrics

Computed per `(strategy config, account config)` pair, over pooled
**out-of-sample** days only:

| metric | definition |
|---|---|
| `p_breach_before_target_pct` | fraction of resampled paths hitting `threshold(t)` before satisfying the objective |
| `days_to_target` | full empirical distribution over resamples; median, IQR, 10th/90th, and the censored fraction that never reaches it |
| `p_pass_evaluation_pct` | reaches the target **and** satisfies `min_trading_days`, `consistency`, `target_basis`, and `access_period_calendar_days` |
| `payout_cadence_days` | expected days between payouts under the firm's gates (§4.4) |
| `worst_day_nano_usd` | §3.5, both distributions |
| `p_ruin_pct` | `personal_*` only (§4.5) |

**OOS-only headlines.** Anything computed on in-sample data is not a headline.
It may appear, labelled `IS`, in a diagnostics section, never in the banner and
never in a sentence that starts "this strategy would". The pooled OOS day series
comes from `crucible-funnel::walkforward`, which already guarantees the property
that makes pooling legal: `step_days ≥ test_days`, refused otherwise (D-0062),
so no session is counted twice. A bootstrap over a series with duplicated
sessions inflates the sample and flatters every statistic reading it — including
this one.

Every headline is reported **with the DLL enforced and not enforced** (§2.3),
and with its mark grain (§3.3.2). Neither is a footnote.

### 4.2 Breach probability is a first-passage problem

It is not a drawdown statistic. `max_drawdown_pct` asks how far the curve fell;
this asks **whether an absorbing boundary was touched, and when, relative to a
second boundary**. The differences are structural:

- **Absorbing.** After a breach the path stops existing. A drawdown statistic
  averages over what happened next; a breach cannot.
- **Two-sided race.** Target and threshold compete. The answer depends on the
  *order* of good and bad days, so any statistic that is invariant to the order
  of daily PnL cannot compute it — which rules out mean, variance, Sharpe, and
  the whole i.i.d. family.
- **Moving boundary.** The threshold ratchets, so the boundary is a functional
  of the path's own history.
- **Path-resolution-dependent.** For an intraday ratchet the boundary moves
  within the day (§3.3).

Estimator: simulate. Resample day-shaped objects (§4.3), replay the §2.2
recursion over each resampled sequence, record first-passage. Report Monte
Carlo standard error alongside every probability; a probability without its own
error bar invites the reader to over-read the third digit.

### 4.3 Block bootstrap, and the block length

**Resampling unit: a whole trading day, as a `DayRecord`** (§3.2) — never a
scalar daily PnL. The four numbers travel together because the within-day path
must not be invented, and §3.3.1 proves they are sufficient. This is the design
decision that lets a *daily* bootstrap answer an *intraday* question.

**Scheme: circular block bootstrap over consecutive days. Pre-declared block
length `L = 20` trading days**, with a mandatory sweep over
`L ∈ {1, 5, 10, 20, 40, 60}` reported beside the headline.

Justification, three independent arguments landing in the same place:

1. **Rate.** For a series of `n` daily observations the classical block-length
   rate is `O(n^{1/3})`: `n ≈ 1,000` (four years OOS) gives ≈ 10, `n ≈ 4,000`
   (sixteen years) gives ≈ 16. Order 10–20.
2. **The dependence that actually matters here.** Daily futures strategy PnL
   inherits volatility clustering from the underlying: GARCH-type persistence in
   daily equity-index volatility decays over weeks, and a trend or breakout
   strategy adds regime persistence of its own on top. 20 sessions ≈ one
   calendar month, long enough to carry a bad month through intact.
3. **The functional.** First passage is driven by **runs of consecutive losing
   days**, not by the marginal distribution of a day. `L` must exceed the loss
   runs we intend to preserve, so the report prints the empirical longest losing
   streak in the OOS series next to `L`, and `L < streak` is a warning in the
   output. The choice is then auditable rather than asserted.

**Why not i.i.d.** An i.i.d. bootstrap destroys exactly the serial dependence
that drives first passage. It scatters a bad month's losses across the sample,
so the running decline from the high-water never accumulates, so the boundary is
touched less often: **an i.i.d. bootstrap understates breach probability**.
`L = 1` in the sweep *is* the i.i.d. case, and it is retained precisely so the
understatement is visible as a number rather than argued about — see the control
in §5.8.

**Refusals.** `n < 10L` (200 sessions at `L = 20`) is refused: with fewer than
ten blocks the resamples are near-copies of the original and the interval is
theatre. Refusing costs a config edit.

Everything here consumes seeded `rand_chacha` (§6-blessed), seeded per §3.6.
Two runs of the battery are bit-identical or the battery is broken.

### 4.4 Payout cadence, and why consistency is a path constraint

Every consistency rule in `configs/accounts/` constrains the **path**, not the
total:

- Topstep Combine: best single day ≤ 50 % of the *profit target*; exceeding it
  **raises the target** to `best_day / 0.50` rather than failing the account,
  and losses never reset the best day.
- TPT Test: best single day ≤ 50 % of *total net profit*, same escalation.
- Apex PA payouts: no profitable day ≥ 50 % of profit since the last approved
  payout; five qualifying days each over a per-size minimum; a safety net of
  `drawdown + $100` that must hold for the account's life; six payouts, then
  the account closes.
- Topstep payouts: five winning days of $150+ net (Standard) or three trading
  days with largest day ≤ 40 % of net profit (Consistency path).

**So a strategy can reach the target and fail anyway.** One enormous day and a
scatter of small ones satisfies every scalar metric in the funnel and fails the
consistency gate. This is not an edge case — it is the modal failure of a
strategy with fat-tailed daily PnL, which is most trend strategies. It follows
that:

- `p_pass_evaluation_pct` evaluates consistency on the resampled day sequence,
  including the escalating target, per resample.
- The report prints `p_target_reached_pct` and `p_pass_evaluation_pct`
  separately. The gap between them is the consistency tax, and it is a number
  worth having.
- Payout cadence is the expected number of trading days to satisfy the gates
  conditional on survival, reported as a distribution, with the censored
  fraction that never qualifies. A mean payout cadence over surviving paths
  only, unlabelled, is survivorship bias with a dollar sign.

### 4.5 `personal_*`: risk of ruin, not synthetic drawdown

A cash account has no threshold, no firm, and nothing to breach. Inventing a
"synthetic drawdown limit" for it would manufacture a failure mode to make the
comparison table look uniform, and the resulting probability would be a
statement about the invention.

Instead: **risk of ruin** — P(cum PnL reaches `[ruin].threshold_cum_pnl_pct` of
starting capital) over the same resampled paths, plus the full drawdown
distribution and time-to-recovery. The threshold is **our pre-declared research
parameter** (−50 % as shipped), not anybody's rule, and every quoted ruin
probability prints it. Two `personal_*` runs at different thresholds are two
runs, and the second one is a trial.

`[margin]` is empty in all four files, and **filling it is a human task**. CME
performance bonds change by clearing advisory several times a year, so a constant
pasted into a config is a dated observation and never a spec value; and CME's
Data Terms of Use forbid automated retrieval while their site blocks it outright,
so no automated process in this project may fill the field in. Absent means
**the contract-count cap is unmodelled**, the report says so, and no default is
substituted.

**Which number is wanted, and how to break the tie.** The **exchange maintenance
margin** is the baseline, because it is the conservative figure: it is larger
than a broker's day-trading margin, so it caps contracts lower and understates
rather than flatters what the account could have held. Broker day-margins are a
**future refinement**, not the baseline — they are smaller, broker-specific, and
revocable intraday at the broker's discretion, which makes them a worse thing to
pin a research constant to. **Where the two differ, the conservative (larger)
figure is chosen.** Whoever pastes the numbers pastes
`initial_margin_per_contract_usd`, `maintenance_margin_per_contract_usd` and
`as_of_date` together; a figure without its date is not a figure, because the
rate it came from has already moved.

`personal_*` is the control arm and that is its real job: every difference
between `personal_50k` and a prop account of the same size, same strategy, same
window, is attributable to the account rule and nothing else.

### 4.6 The selection trap, and the structure that forbids it

**Choosing the account after seeing the results is a selection step.** Run a
strategy against sixteen account configs, report the one with the best
`p_pass_evaluation_pct`, and the number is a maximum over sixteen draws
presented as an expectation. It is the same error as picking the best grid combo
and quoting its Sharpe — which this project already has machinery against, and
that machinery is the answer:

1. **The account config is pre-declared**, in the config, before the run —
   exactly as the funnel pre-registers kill criteria (`docs/PROJECT_PLAN.md` §7,
   item 2). Not a CLI flag chosen at report time.
2. **`account_id` is part of the run identity**, alongside
   `(config_hash, combo_index, fold)`. The registry insert happens
   before the run (M3), so the set of accounts tried is a record, not a memory.
3. **Every additional account is a trial** charged to the strategy's
   `meta.hypothesis_family`, feeding the deflated Sharpe like any other trial.
   Account shopping is not forbidden; it is *priced*, which is stronger, because
   a rule that costs nothing to break gets broken.
4. **A report never sorts by the metric it is about to quote.** Per-account
   detail prints in declared config order, following `walk-forward`'s existing
   rule about printing the first N combos by grid index and never "the best N".

### 4.7 Automation policy, and why `reference_only` is not an exclusion

Two fields exist because "can this account legally be traded by an automated
strategy?" is a fact about the product that a metric cannot see, and a P(pass)
for an account this project's output may not run on means something different
from a P(pass) for one it may.

**`[account].automation_policy`** — closed set `forbidden | conditional |
unknown`, each with `automation_policy_source_url` and
`automation_policy_accessed_date`, cited to the page it was read on:

| accounts | value | basis |
|---|---|---|
| `topstep_*` | `conditional` | "Can I use automated trading strategies? Yes, with conditions." |
| `tpt_*` | `forbidden` | "We do not allow any automated or bot trading of any kind. All trades must be manually executed by the trader." |
| `apex_*` | `unknown` | no stance located on any cited page; the linked Prohibited Activities page was not retrievable |
| `personal_*` | `unknown` | no firm exists; the governing terms are the chosen broker's, and no broker is named |

**`unknown` is a value, not a placeholder**, and it must never be read as
permission. Closing it by inference — "they didn't say no" — is exactly the move
this project's refusal discipline exists to block, and it stays open (§8, items
8 and 9) until someone reads a page that says so.

**`[account].reference_only`** — `true` on the five `tpt_*` files, meaning the
firm's own rules forbid running this project's output on the funded account this
evaluation leads to. It does **not** mean excluded:

- **A reference-only account is evaluated by the full battery.** Its rule
  *structure* is what measures strategy fragility, which is the research
  question this document is in service of. TPT is the most informative account
  in the directory precisely because one firm supplies both ratchet bases at an
  identical drawdown amount — the cleanest available isolation of §7.0's axis.
- **Which product to target is an M4 decision**, taken against rules re-verified
  at that time. Every figure here carries a 2026-07-30 access date because these
  rules changed during 2026 and will change again; pruning a config today on the
  strength of a rule read today would throw away a measurement to act on a fact
  with a shorter shelf life than the measurement has.
- What `reference_only` does change is the **wording of the report**: a
  reference-only account's numbers are labelled as a fragility measurement and
  never as a route to funding. `automation_policy` prints beside them, so a
  reader cannot mistake one for the other.

---

## 5. Planted-bug controls

CLAUDE.md §7: a detector nobody has watched fire is decoration. Every check
below ships two controls — one that must fire, one that must not — and each pair
is merge-blocking the day the check lands. Numbers are hand-derived with the
derivation in the test comment (§5.4).

| # | detector | must fire | must NOT fire |
|---|---|---|---|
| 5.1 | breach at the boundary | path reaching `threshold + (−1)` nanodollar, i.e. 1 nano below | path reaching `threshold + 1` nanodollar |
| 5.2 | the firms' own examples | each row of §2.2's table reproduced exactly | a path 1 nano above each example's threshold |
| 5.3 | ratchet basis | path with a high intraday peak and flat daily closes: breaches under `peak_equity_including_unrealized` | the same path under `highest_daily_closing_equity` |
| 5.4 | endpoint fallacy | close-only evaluation of a constructed sample yields **strictly lower** `p_breach` than intraday evaluation | on a sample with no intraday excursion beyond its closes, the two agree exactly |
| 5.5 | ratchet lock | path peaking past the lock trigger then bleeding: survives with `ratchet_lock_at_cum_pnl_usd` set | the identical path breaches with the field absent (Tradovate variant) |
| 5.6 | calendar slicing | a loss at 18:30 CT Monday is booked to Tuesday by `Calendar::trading_day`; a wall-clock slicer books Monday, and the test asserts the two **disagree** | the calendar slicer's attribution matches the hand-derived date |
| 5.7 | DLL is a lockout | a day hitting the DLL is truncated and the account **survives** | a DLL hit does not register as a breach, and the account is still eligible next day |
| 5.8 | bootstrap dependence | on a series with planted loss clustering, `L = 1` gives **strictly lower** `p_breach` than `L = 20` | on an i.i.d.-by-construction series the two agree within Monte Carlo error |
| 5.9 | consistency gate | one-huge-day path reaching the target reports `target_reached = true`, `pass = false`; Topstep's $1,200/$2,800 = 43 % example passes; TPT's $2,000/$3,100 = 65 % example fails | the same total spread evenly passes |
| 5.10 | ruin (`personal_*`) | path touching −50.000001 % of capital reports ruin | path touching −49.999999 % does not |
| 5.11 | determinism | two full battery runs on one seed lineage hash identically | changing only `account_id` changes the hash — a shared resampling draw would be a bug |
| 5.12 | reconstruction ban | a test that builds the high-water series from OHLC after the run and asserts it **differs** from the captured series on a fixture with intrabar path ambiguity | on a fixture whose bars are single-tick, the two agree — proving the difference in the first control is the ambiguity, not an unrelated bug |

5.12 is the control for §3.3's hard requirement. Without it, "never reconstruct
from bars" is advice; with it, the divergence is a number in a test log.

---

## 6. What makes these numbers honest, and what would make them a lie

Honest, if all of them hold:

- the breach test runs on the captured intraday high-water series from the
  engine's own mark loop, at a stated mark grain;
- headlines are out-of-sample, on non-overlapping pooled test windows;
- the account config was declared before the run and is part of the run
  identity, and every extra account is a charged trial;
- resampling preserves serial dependence, with the block length and the
  empirical loss-streak printed;
- every probability carries its Monte Carlo error, its mark grain, and its
  DLL setting;
- every reported figure traces to a firm's own page with an access date, and
  every ambiguity is in §8 rather than resolved by preference;
- `automation_policy` prints beside every account's numbers, so a P(pass) for an
  account this project's output may not legally trade is never read as a route
  to funding (§4.7).

A lie, by any one of these — each has been seen in published prop-firm
"backtests":

1. **Evaluating an intraday trail on daily closes.** Understates breach
   probability, one-directionally. §5.4 exists to make the size of the lie
   visible instead of arguable.
2. **Reconstructing the equity path from OHLC.** Measures a path the account
   never took, with a favourable intrabar ordering chosen by accident.
3. **Picking the account size after seeing the results.** A maximum over
   configs reported as an expectation.
4. **i.i.d. resampling of daily PnL.** Deletes the loss clustering that causes
   breaches.
5. **Quoting a median days-to-target without the censored fraction.** The paths
   that died are silently excluded, and the median describes the survivors.
6. **Reporting P(pass) as `1 − P(breach)`.** Ignores minimum days, consistency,
   and a target that must be standing at a close — all of which only subtract.
7. **A "synthetic drawdown" on a cash account.** Manufactures the failure mode
   it then measures.
8. **Quoting in-sample.** Any of the above, minus the excuse.

---

## 7. Verification record: where the working baseline was wrong

Captured separately from the reading of it. The baseline this work started from
is on the left; the firm's own page on 2026-07-30 is on the right; the verified
figure is what `configs/accounts/` encodes.

### 7.0 The framing was wrong, not only the numbers

Ten individual figures were corrected below, and that is the smaller half of
this record. The larger half is that **the working baseline's "intraday-trailing
versus EOD-trailing" dichotomy is not a true dichotomy, and no assignment of
these sixteen accounts to those two labels can be correct.** It is a one-axis
framing of a two-axis object, and the axes are independent:

- **the ratchet** — what advances the threshold upward: a continuously-updated
  peak of equity including unrealized PnL, or the highest end-of-day balance;
- **the breach test** — what is compared against the threshold, and how often.

The dichotomy implicitly ties the two together, so "EOD-trailing" is heard as
*both* an end-of-day ratchet *and* an end-of-day test. The second half is false
for every account in this directory. Topstep's own page: the MLL "updates at the
end of each trading day **but is monitored in real time throughout the session.
Both realized and unrealized P&L count toward it.**" Take Profit Trader's Test:
"If your account drops to the Minimum Account Balance **at any time — through
realized or unrealized losses** — the account is immediately liquidated."

Two consequences, and the second one is the whole cost of getting this wrong:

1. **`ratchet_basis` and `breach_basis` are separate config fields** because the
   product of the two, not either alone, is what an account is. The same firm can
   move one and hold the other — TPT's Test and PRO differ in ratchet at an
   identical drawdown amount — and a schema with one field cannot say so.
2. **The intraday unrealized-equity high-water series (§3.3) is required for all
   sixteen configs, including every "EOD" one.** It is not an Apex-only
   refinement to be skipped for Topstep and TPT. An implementer who reads
   "EOD account" as licence to evaluate Topstep on daily closes reintroduces the
   endpoint fallacy on eight of the sixteen accounts, and gets numbers that are
   optimistic by construction while looking careful — the exact failure this
   document opens by naming. The ratchet is what changes between accounts; the
   capture requirement does not change at all.

A one-axis model of these accounts is therefore not a simplification with a
known error bar. It is a model that cannot represent the object, and its error
has a sign.

**Apex Trader Funding** — Intraday Trailing Drawdown Evaluation
([evaluations](https://support.apextraderfunding.com/hc/en-us/articles/45683414022299-Intraday-Trailing-Drawdown-Evaluations),
[mechanics](https://support.apextraderfunding.com/hc/en-us/articles/45683513113115-Intraday-Trailing-Drawdown-Explained),
[scaling/micros](https://support.apextraderfunding.com/hc/en-us/articles/46729420990235-Scaling-Levels-PA-Explained),
[PA payouts](https://support.apextraderfunding.com/hc/en-us/articles/47206370796827-Intraday-Trailing-Drawdown-Payouts))

| item | baseline | verified |
|---|---|---|
| 150K max drawdown | $4,500 | **$4,000** — corroborated twice: the evaluation table, and the PA safety net of $154,100 = 150,000 + 4,000 + 100 |
| 100K max contracts | 10 | **8** |
| 150K max contracts | 14 | **12** |
| trail lock | "locks at the starting balance once reached" | **three different levels**: evaluation on Rithmic/WealthCharts locks at the profit-target balance; evaluation on Tradovate never locks; the funded PA locks at starting balance + $100 |
| 25/50/100K targets and drawdowns, 25K contracts, no DLL, micro ratio 10:1 | — | **confirmed** |

**Topstep** — Trading Combine
([parameters](https://help.topstep.com/en/articles/8284197-trading-combine-parameters),
[MLL](https://help.topstep.com/en/articles/8284204-what-is-the-maximum-loss-limit),
[DLL](https://help.topstep.com/en/articles/10490293-daily-loss-limit-in-the-trading-combine-and-express-funded-account),
[consistency](https://help.topstep.com/en/articles/8284208-what-is-the-consistency-target),
[payouts](https://help.topstep.com/en/articles/8284233-topstep-payout-policy),
[XFA rules](https://www.topstep.com/express-funded-account-rules))

| item | baseline | verified |
|---|---|---|
| max loss trail | "EOD-trailing" | **the LEVEL is end-of-day; the BREACH is real-time on unrealized P&L.** "monitored in real time throughout the session. Both realized and unrealized P&L count toward it." The most consequential correction in this document |
| minimum 5 winning days | a Combine pass requirement | **not a Combine requirement.** Minimum is 2 trading days. Five winning days of **$150+** (not $200) is an Express Funded *payout* gate |
| consistency | no single day > 30 % of total profit | **Combine: best day ≤ 50 % of the PROFIT TARGET**, and exceeding it raises the target rather than failing the account. The 40 %-of-net-profit rule is the XFA Consistency payout path |
| daily loss limit | a hard rule | **optional** in the Combine and in an XFA on TopstepX; automatic in the Live Funded Account and on other XFA platforms. A lockout, not a breach |
| targets $3,000/$6,000/$9,000; MLL $2,000/$3,000/$4,500; DLL $1,000/$2,000/$3,000; 5/10/15 minis, 50/100/150 micros | — | **confirmed** |
| — | not in the baseline | XFA balances start at **$0**; the size label is buying power. §2.1's cum-PnL normalisation exists because of this |
| — | not in the baseline | flat by **15:10 CT**; the trading day runs 17:00 CT → 15:10 CT; automated strategies permitted "with conditions" |

**Take Profit Trader** — Test account, fetched entirely from the firm's help
centre, nothing from memory
([Rule 1](https://takeprofittraderhelp.zendesk.com/hc/en-us/articles/15169070804125-Rule-1-Hit-Your-Profit-Target),
[Rule 3](https://takeprofittraderhelp.zendesk.com/hc/en-us/articles/15170265979165-Rule-3-Do-Not-Hit-End-Of-Day-EOD-Maximum-Trailing-Drawdown),
[Rule 5](https://takeprofittraderhelp.zendesk.com/hc/en-us/articles/15170316538013-Rule-5-Be-Consistent),
[PRO rules](https://takeprofittraderhelp.zendesk.com/hc/en-us/articles/15171769361053-PRO-Account-Rules))

| size | profit target | max trailing drawdown (EOD) | max contracts |
|---|---|---|---|
| $25,000 | $1,500 | $1,500 | 3 |
| $50,000 | $3,000 | $2,000 | 6 |
| $75,000 | $4,500 | $2,500 | 9 |
| $100,000 | $6,000 | $3,000 | 12 |
| $150,000 | $9,000 | $4,500 | 15 |

The baseline said "25k–150k"; there are **five** sizes including a $75,000 one.
Test = EOD ratchet, locks at the starting balance, breach tested "at any time —
through realized or unrealized losses". PRO = **intraday** ratchet, same
drawdown amount. Minimum 5 trading days. Consistency: no single day > 50 % of
total net P/L.

**PRO forbids automated trading outright** — "We do not allow any automated or
bot trading of any kind. All trades must be manually executed by the trader" —
and forbids open positions within one minute either side of FOMC, NFP and CPI.
A systematic strategy cannot be run in a TPT PRO account, so the five TPT files
carry `automation_policy = "forbidden"` and `reference_only = true`. **They are
kept and they are fully evaluated** (§4.7): the rule structure is what measures
fragility, and the Test/PRO ratchet pair is the sharpest instrument in the
directory for the axis §7.0 is about. Product targeting is an M4 decision made
against rules re-verified then. That is a fact about the product, not a caveat
about the model.

**CME margins**, for `personal_*`: **nothing encoded.** CME's site returns
"This IP address is blocked due to suspected web scraping activity" and its Data
Terms of Use forbid automated retrieval, so no figure was read; and performance
bonds change by clearing advisory several times a year, so even a correctly-read
figure is a dated observation, not a constant. `[margin]` is empty, says why,
and names the pages a human should read.

---

## 8. Open questions — refused rather than guessed

**These stay open.** None of the twelve is to be closed by inference, by
plausibility, or by the absence of a contrary statement — only by reading a page
that answers it, with an access date, or by asking the firm. An item resolved
here without a source is worse than the item, because it stops looking like a
gap. Where an item has a *provisional* encoding (a target basis, an Apex lock
variant), the encoded value is the stricter reading, the alternative is named,
and the report says both.

1. **Does the Apex evaluation target have to be standing at a close?** Inferred
   from "the evaluation will be marked as passed after market close", not
   stated. Encoded as end-of-day; the alternative raises P(pass).
2. **Same question for Topstep** ("Reach and maintain your Profit Target") **and
   for TPT** (silent). Both encoded as end-of-day, the stricter reading. Report
   both ways until asked.
3. **Which Apex evaluation lock applies to us?** Rithmic/WealthCharts lock at
   the profit-target balance; Tradovate never locks. It does not move
   P(breach before target) — the lock is only reachable beyond the level that
   already passed — but it does move any funded-phase number.
4. **TPT's Rule 3 reads two ways.** "the drawdown for test accounts is
   calculated only at the end of the trading day, not during open trades"
   versus "If your account drops to the Minimum Account Balance at any time —
   through realized or unrealized losses — the account is immediately
   liquidated." Read as level-vs-breach they agree, which is what is encoded;
   read literally the first sentence contradicts the second.
5. **TPT's consistency formula disagrees with TPT's consistency rule.** The page
   gives "Updated Profit Goal = Net P/L × 2" ($3,100 → $6,200) and then shows
   $2,000/$4,001 < 50 % as sufficient. `best_day / 0.50` is encoded.
6. **TPT's micro-to-mini ratio** is not on the rule pages.
   `micros_per_contract` is absent rather than assumed to be 10.
7. **TPT's flat-by time.** Their own copy says "5PM CT" for Test and "5PM EST"
   for PRO; 17:00 CT is the Globex *open* and 16:00 CT the close, so both are
   wrong on their face. None encoded.
8. **Does TPT's algo ban extend to the Test phase?** Stated only on the PRO
   page.
9. **Apex's stance on automated trading** was not located on their rule pages.
   Topstep's is verified permissive; TPT's PRO is verified prohibitive; Apex is
   unknown.
10. **Apex Intraday PA tier-based DLL and tier-based position size.** The tier
    tables were read but are not encoded: they make position size
    path-dependent on end-of-day balance, which is a position-sizing rule the
    engine does not yet express and a fixed-`qty_contracts` strategy cannot
    honour. Funded-phase work.
11. **CME margin figures** (§7). A dated human observation, not a config
    constant.
12. **Simulated commissions inside prop platforms** are not necessarily the
    exchange-plus-broker figures a `personal_*` account pays. Costs live in the
    strategy config's fill model, so a truly like-for-like comparison may need
    two fee settings — deliberately not resolved here, because it is a cost
    question and §2.4 keeps cost questions in the fill model.

---

## 9. M3 dependencies

This battery is built on top of, and after:

- `crucible-funnel::stats` — deflated Sharpe, PBO/CSCV, block-permutation
  nulls, empirical p-values. Account metrics are additional per-run outputs, not
  a replacement: a strategy that passes an evaluation on a mined parameter set
  passes an evaluation on a mined parameter set. The trial count from §4.6
  feeds the same deflation.
- The DuckDB registry — `account_id` in the run identity, insert-before-run,
  dedupe on `(config_hash, account_id, combo_index, fold)`.
- The rayon scheduler — bootstrap draws are embarrassingly parallel and merge by
  sorting on draw index, never on completion order (§2.2).
- The scorecard's honesty box — an account section states mark grain, block
  length, DLL setting, Monte Carlo error, the accounts tried, and the access
  date of the rules it was judged against. A scorecard quoting P(pass) without
  those does not render.
- M2's stops/targets with worst-case intrabar ordering (`docs/MILESTONES.md`),
  which is what will let the mark grain in §3.3.2 stop being a lower bound.
