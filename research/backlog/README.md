# Research backlog — pre-registered hypothesis files

This directory is the queue of **candidate strategy ideas harvested from public
research**, restated in Crucible's terms, with kill criteria chosen *before*
anything is run. Nothing here has been tested. Nothing here is a
recommendation. A file in this directory is a *proposal to spend compute*, and
Liron triages it — nothing enters the funnel without his selection.

The point of writing an idea down this way is that the expensive part of
research is not running the backtest, it is deciding what would have counted as
a failure. A file whose kill criteria are written after the first equity curve
is a rationalization (`docs/PROJECT_PLAN.md` §7.2). So the criteria are written
here, first, by someone who does not yet know the answer.

---

## 1. The binding rule for these files

**No file in this directory contains a predicted return, Sharpe, win rate, or
any other performance number for Crucible.** These documents extract and
restate other people's ideas; they do not forecast our results. Where a source
paper's own reported figure is quoted, it is attributed to that paper in the
same sentence and lives only in the *Honesty note* section — never in the
mechanism, never in the kill criteria, and never phrased as something we expect
to reproduce.

The working prior, from the project's external review, is that **most published
strategy papers are garbage**: they are mined from the same few decades of US
equity data, published because they worked, and rarely survive costs or a
second sample. A grade C with honest reasons outranks an inflated grade A. If
the honest finding is "the academic support for this is absent and the claims
come from vendor blogs", that is what the file says (see `H-012`, `H-013`).

---

## 2. Triage grades

The grade answers exactly one question: **what does it cost to test this?**
It is not a quality judgement — a grade A idea can be worthless and a grade C
idea can be the best thing here.

| Grade | Meaning |
|---|---|
| **A** | Expressible in combo TOML this week. No new Rust. Runs with `crucible combo --run` / `crucible walk-forward` against curated bars we already have. |
| **B** | Needs new indicator or engine code, but **only data we already own**. The gap is code, and the code is in scope for M2/M3. |
| **C** | Needs data we do not have, or machinery no milestone has built yet (resampler, options loader, cross-sectional portfolio). The gap is an acquisition or a milestone. |

### 2.1 What grade A actually means, precisely

Grade A is a narrow envelope, and it is narrow on purpose. As of 2026-07-30 a
combo config can express **only** this:

- **Indicators:** `sma` (period), `ema` (period), `bollinger` (period, k →
  fields `.mid`, `.upper`, `.lower`). That is the complete list
  (`crucible-strategies::combo::spec`).
- **Operands:** a numeric constant, a price field (`open`, `high`, `low`,
  `close`) of the completed bar, or an indicator slot.
- **Comparisons:** `<`, `<=`, `>`, `>=`, `crosses_above`, `crosses_below`,
  combined with `and` / `or` / `not` / parentheses.
- **Rules:** `enter_long`, `exit_long`, `enter_short`, `exit_short`. All four
  are evaluated on every bar and never short-circuit (CLAUDE.md §9).
- **Universe:** exactly one instrument and exactly one timeframe per config.
  `combo` refuses a config declaring two of either. The timeframe may be any of
  `1s 1m 5m 15m 1h 1d` — the last four are resampled from 1-minute bars on read
  (D-0077, §2.2).
- **Execution:** `free_fills` (S0–S1 only) or `spread_cross` with integer
  `half_spread_ticks` and a decimal-string fee.

And it **cannot** express any of the following. Each of these is the single
most common reason a paper's signal is grade B rather than A:

| Not expressible | Consequence |
|---|---|
| **Time of day / session position** | Every "first half-hour", "last hour", "opening range", "RTH vs Globex" idea is grade B. There is no clock operand in the grammar. |
| **Volume** | `Bar` carries `volume: u64`, but the rule grammar has no `volume` operand. Volume ideas are grade B — the data is there, the grammar is not. |
| **Arithmetic between operands** | Only *comparisons*. You cannot write `(bb.upper - bb.lower) > x`, so no width, ratio, spread, or normalized-deviation term. |
| **Calendar predicates** | Day-of-week, day-of-month, turn-of-month, holiday proximity. Grade B (the calendar exists in `crucible-data`; the grammar cannot reach it). |
| **Stops / targets** | The engine has brackets (D-0069) but the combo grammar cannot declare one. Both grid commands print a zero path-sensitive count and say why. |
| **Multi-timeframe / multi-instrument** | One of each per config, by design. |

### 2.2 The two structural blockers behind most grade B/C calls

**~~There is no bar resampler.~~ Resolved 2026-07-30 (D-0077).**
`crucible-data::transcode` still maps vendor schemas to timeframes one-to-one,
and we still bought **only `ohlcv-1m` and `ohlcv-1s`** (`docs/DATA_PLAN.md` —
the hourly and daily aggregates are $190/GB and were deliberately not bought).
What changed is that `crucible-data::curated::resample` now aggregates curated
1-minute bars into `5m` / `15m` / `1h` / `1d` **when they are read**, on the
exchange's own sessions, so `--timeframe 5m` and `timeframes = ["1d"]` work
everywhere without a second copy of the archive. The `M5`/`M15` `TimeFrame`
variants remain *deliberately unmapped* in `transcode` — this is the only path
that produces them.

Three properties are worth knowing before stating an idea on coarse bars:

- **A daily bar is a TRADING-day bar**, opening when the session opened. It is
  not a UTC day: a UTC-day bar for 2024-01-03 would hold that day's 00:00–22:00Z
  bars *and* the 23:00Z bars that opened the fourth's session.
- **No resampled bar spans a session boundary**, and an early close simply makes
  the last bucket short — a 12:15 CT close makes bucket 19 of an hourly resample
  fifteen minutes long, and says so in its volume.
- **A stitched series is still out of reach at coarse grains.** A bucket
  spanning a roll would mix two `signal_offset`s (D-0076), so `ES.v.0` at `5m`
  is refused. The single-contract ceiling in this section still applies.

**`combo` and `walk-forward` replay raw contracts only — this is the hard
ceiling on grade A.** A continuous alias (`ES.v.0`) is *refused*, not
unsupported-by-accident: replaying a back-adjusted series needs a consumer that
says which of the two price series it wants — signals read the adjusted series,
PnL reads the tradeable one — and neither grid command has anywhere to put that
choice (D-0042, and the refusal is quoted in `crucible-cli::combo`). Separately,
`curated/rolls/` is empty, so no roll table has been generated yet either.

The consequence is concrete and it constrains every grade A in this directory:
**the longest sample a grade-A config can replay today is one contract's active
life** — roughly a quarter for ES. That is an S0 triage sample, not a verdict
sample. A verdict needs one of:

1. the D-0042 consumer, so a stitched multi-year series can be replayed; or
2. running the same config across many individual contracts (`ESH4`, `ESM4`,
   `ESU4`, `ESZ4`, …) and **pooling** the results — which needs the M3 registry
   to pool honestly, and which charges every contract as a trial.

So "grade A" means *runnable this week*, and nothing stronger. No file in this
directory may issue a `Graduate` verdict off a single-contract sample.

---

## 3. What we actually own (2026-07-30)

Stated here once so no hypothesis file has to guess. **Raw** is the durable
asset; **curated** is derived, disposable, and currently mid-build.

### Raw archive (`manifest.jsonl`, 41 acquisition records, `verify` clean)

| Schema | Symbols | Span |
|---|---|---|
| `ohlcv-1m` | ES, NQ, RTY, CL, 6E, ZN, GC (parent keys) | 2010-06-06 → 2026-07-28 |
| `ohlcv-1s` | same seven | 2010-06-06 → 2026-07-28 |
| `definition` | same seven | 2010-06-06 → 2026-07-28 |
| `statistics` | same seven | 2010-06-06 → 2026-07-29 |
| `trades` | **ES only** | 2025-07-28 → 2026-07-28 |
| `tbbo` | **ES only** | 2025-07-28 → 2026-07-28 |
| `mbo` | **ES only** | 2026-06-28 → 2026-07-28 |

### Curated (replayable) — **1m only, and incomplete**

Bulk transcode is in progress. At the time of writing: GC (120 contracts), RTY
(38) and CL (1) have the full 2010→2026 window; **ES has only January 2024 and
2024-02-01 → 2026-07-28** — the 2010→2024 ES window is not transcoded yet.
Curated data is derived and rebuildable, so this is a scheduling fact, not a
constraint on what is *ownable*. Any file that says "grade A" means A *once the
relevant contract is transcoded*, which costs a command, not a purchase.

### Not owned / not built

- **No VIX complex.** `external/cboe/` does not exist. The CSVs are free and
  manual (`docs/DATA_PLAN.md`), the availability rule is written down
  (a daily index value is knowable at that session's **close**, 15:00 CT), and
  the loader is deliberately unbuilt until the post-M4 regime work.
- **No options in the engine.** ThetaData EOD / greeks / open-interest for
  SPX, SPY, QQQ, IWM is being acquired (a `theta-pull` was running while this
  was written), but ThetaData integration is explicitly post-M4
  (`docs/MILESTONES.md`) and no loader joins it to futures bars.
- **No macro calendar.** FOMC/CPI/NFP timestamps are an M4 static CSV.
- **No cross-sectional portfolio accounting.** Post-M4.

---

## 4. File format

One file per candidate: `research/backlog/<id>-<slug>.md`, with YAML
front-matter and these sections in this order:

```markdown
---
id: H-0NN
slug: short-kebab-slug
topic: one of the topic keys in §5
grade: A | B | C
hypothesis_family: the pre-registered trial-registry key
status: backlog
created: YYYY-MM-DD
---

# H-0NN — Title

## Citation            link + venue + year, verbatim claim, no paraphrase creep
## Mechanism           one paragraph: why it could work, and WHO IS ON THE LOSING SIDE
## Signal in Crucible terms   instruments, timeframe, features, rules
## Data                 what we own for this; what we lack, flagged explicitly
## Pre-registered kill criteria   numeric, chosen now, judged by machines
## Honesty note         their data vs ours, sample overlap, known biases
## Triage grade         the grade and the specific reason for it
```

**`hypothesis_family`** is the trial-registry key (CLAUDE.md §4). It must cover
**the whole idea, not one parameterization** — every combo, every fold, and
every re-run under that key increments the trial count that feeds the deflated
Sharpe. Choosing a narrow family to keep the count low is the specific dishonest
act the registry exists to prevent. Families are declared here, before the first
run, so the count starts at the right place.

**"Who is on the losing side"** is a required sentence, not decoration. A
strategy is a claim that someone is systematically paying you. If the mechanism
paragraph cannot name who, and why they keep doing it, the idea is a pattern
someone found in a finite sample and the file should say so.

---

## 5. Topics in this sweep

| key | topic | files |
|---|---|---|
| `intraday-session` | intraday index-futures behaviour: session effects, opening range, time-of-day | H-001, H-002, H-003, H-004, H-005 |
| `momentum-horizon` | momentum / mean-reversion horizons on ES, NQ, CL | H-006, H-007, H-008 |
| `vol-regime` | volatility-regime conditioning | H-009, H-010, H-011 |
| `volume-structure` | volume profile, value area, VWAP | H-012, H-013 |
| `calendar` | calendar and seasonality effects | H-014 |
| `options-context` | options-implied context as a *feature* | H-015 |

## 6. Index

| id | title | topic | grade | family |
|---|---|---|---|---|
| [H-001](H-001-market-intraday-momentum.md) | Market intraday momentum: first half-hour predicts last half-hour | `intraday-session` | B | `es-intraday-momentum-open-close` |
| [H-002](H-002-overnight-intraday-tug-of-war.md) | Overnight vs intraday return components | `intraday-session` | B | `es-overnight-intraday-split` |
| [H-003](H-003-ohlcv-intraday-falsification.md) | OHLCV intraday signals fail under costs — a replication of a negative result | `intraday-session` | B | `nq-ohlcv-intraday-falsification` |
| [H-004](H-004-gap-volume-regime-classifier.md) | Overnight-gap × opening-volume regime classifier | `intraday-session` | B | `es-gap-volume-regime` |
| [H-005](H-005-intraday-periodicity.md) | Half-hourly periodicity at multiples of a trading day | `intraday-session` | C | `es-intraday-periodicity` |
| [H-006](H-006-time-series-momentum.md) | Time-series momentum at 1–12 month horizons | `momentum-horizon` | C | `futures-tsmom-horizon` |
| [H-007](H-007-trend-following-span.md) | Cost-optimal trend-following span | `momentum-horizon` | A | `es-trend-span-cost-optimal` |
| [H-008](H-008-short-horizon-overreaction.md) | Short-horizon overreaction and reversal | `momentum-horizon` | A | `es-short-horizon-reversal` |
| [H-009](H-009-volatility-managed-exposure.md) | Volatility-managed exposure | `vol-regime` | B | `futures-vol-managed-exposure` |
| [H-010](H-010-vol-managed-oos-rebuttal.md) | The out-of-sample rebuttal to volatility management | `vol-regime` | B | `futures-vol-managed-exposure` |
| [H-011](H-011-variance-risk-premium.md) | Variance risk premium as a return predictor | `vol-regime` | C | `es-variance-risk-premium` |
| [H-012](H-012-vwap-reversion.md) | VWAP reversion | `volume-structure` | B | `es-vwap-reversion` |
| [H-013](H-013-volume-profile-value-area.md) | Volume profile / value-area open location | `volume-structure` | C | `es-value-area-open-location` |
| [H-014](H-014-turn-of-month.md) | Turn-of-the-month in stock index futures | `calendar` | B | `equity-index-turn-of-month` |
| [H-015](H-015-options-expected-move.md) | Options-implied expected move as a feature | `options-context` | C | `es-implied-move-context` |

### 6.1 What the four unlocks removed

The grades in the table above are **as triaged**, and they are Liron's to
change — this section states only what the code can now do, so a re-grade does
not have to re-derive it. Each entry names the §2.1 row or §2.2 blocker it
retired and the decision that did it.

| unlock | retired | decision |
|---|---|---|
| bar resampler | §2.2's first structural blocker; `5m`/`15m`/`1h`/`1d` are replayable | D-0077 |

Grade tally: **2 A · 8 B · 5 C**. That distribution is the honest one, and the
shape of it is the sweep's main structural finding: the combo grammar is three
indicators and six comparison operators, and almost every published intraday
result is stated in terms of a clock the grammar cannot read. Four pieces of
code — a **time-of-day predicate**, a **1m→5m/daily resampler**, a **volume
operand**, and a **rolling normalizer** — would move most of the B column into
A. They are the highest-leverage work in this directory, and none of them costs
a purchase.

Two entries carry a warning the grade does not: **H-012** and **H-013** are
graded on cost to test, and both have **no refereed empirical support** for
their predictive claims. H-013 is a named M2.5 target, so that finding matters
more than its grade does.
