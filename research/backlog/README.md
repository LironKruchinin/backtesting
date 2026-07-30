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
  fields `.mid`, `.upper`, `.lower`), `zscore` (period, source) and `stdev`
  (period, source) where source is `close` / `volume` / `return` (D-0080). That
  is the complete list (`crucible-strategies::combo::spec`), and every one of
  them is a trailing-window statistic — there is no full-sample variant, and no
  config can name one.
- **Operands:** a numeric constant, a price field (`open`, `high`, `low`,
  `close`) of the completed bar, its `volume` in contracts (D-0079), an
  indicator slot, or a **session clock reading** — `minutes_since_open`, `minutes_to_close`,
  `minutes_since_rth_open`, `minutes_to_rth_close`, `is_rth`, `is_overnight`,
  `is_post_rth` (D-0078). Every reading is taken at the bar's `avail_ts`;
  `minutes_to_close` honours an early close, `minutes_to_rth_close` counts
  toward the scheduled one.
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
| ~~Time of day / session position~~ | **Expressible since 2026-07-30** (D-0078). `minutes_since_rth_open > 0 and minutes_since_rth_open <= 30` is the first half-hour of RTH; `minutes_to_close <= 30` is the last half-hour of the session, early closes included. Needs a bundled calendar, so a synthetic feed is refused. An *opening range* still is not: it needs a rolling high/low over a window, not a clock. |
| ~~Volume~~ | **Expressible since 2026-07-30** (D-0079). `volume` is the completed bar's traded size in contracts — an absolute figure, so a threshold has to be chosen for the grain. A *relative* one ("twice the 20-day average") needs the rolling normalizer, not the operand. |
| **Arithmetic between operands** | Only *comparisons*. You cannot write `(bb.upper - bb.lower) > x`, so no width, ratio or spread term. **A normalized deviation no longer needs one** — `zscore` (D-0080) is that term as a slot — but a general expression still cannot be built. |
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
ceiling on grade A, and it is now a deliberate exclusion rather than a missing
part.** The continuous replay path **exists**: D-0073/D-0076 wired
`ContinuousFeed` into replay, roll tables are built (`curated/rolls/` holds ES,
NQ, RTY, CL), and **`backtest` replays a stitched `ES.v.0` series today**.

The grid commands are excluded on purpose. `crucible-cli::combo` refuses a
continuous alias because *a grid expands rules it has not seen*, and a rule
comparing a price to an absolute constant is not safe on a back-adjusted series
— the level a bar sits at is the sum of every roll gap after it. `backtest`
gets the stitched series because it runs one strategy an operator named and
read; a grid does not have that guarantee. Do not read this as a gap waiting to
be closed.

The consequence still constrains every grade A here: **the longest sample a
grade-A config can replay is one contract's active life** — roughly a quarter
for ES, ~60 sessions. That is a triage sample, not a verdict sample. Turning
one into the other needs **registry pooling**: running the same config across
many contracts (`ESH2024`, `ESM2024`, `ESU2024`, …) and pooling honestly
through the M3 registry, which charges every contract as a trial. That is the
**fifth unlock** (§6), and it is the one that converts this whole directory's
grade-A column from "runnable" into "answerable".

**Contract keys carry a four-digit year** (D-0072): `ESH2024`, never the
vendor's `ESH4`. A CME year code has one digit and repeats every ten years, and
our windows are sixteen years long. The grid commands do **not** resolve the
shorthand — only `backtest` prints `ESH4 -> ESH2024`, and only when
unambiguous. **Every config in this directory must spell the four-digit key.**

**This build cannot award `Graduate`** (D-0075). S3's battery is still owed, so
the ceiling is `Iterate` and every report says so. No file here may register a
criterion whose passing implies `Graduate` — the honest top outcome of anything
in this directory today is `Iterate`.

### 2.3 The `[funnel]` schema is owned elsewhere — this directory carries values, not schema

`[funnel]` is parsed by `crucible-cli::config::FunnelCfg`, and **the funnel
workstream owns it**. `configs/example-combo.toml` is the canonical, shipped,
parser-checked instance. This directory does **not** get a second copy of that
schema.

The rule, and the reason for it: `deny_unknown_fields` means a `[funnel]` block
that has drifted from the parser is a **hard load error**, and a *missing*
required field is equally fatal. A backlog file that inlines a stale schema
turns a pre-registration into a config that will not load — which is the
strictness working, but at the cost of a confusing failure at the worst moment.

So: hypothesis files pre-register **threshold values and the reasoning behind
them**, and any config block they carry is explicitly marked as a copy whose
canonical source is `configs/example-combo.toml`. **If the two disagree, the
shipped config wins and the backlog file is stale.** Before running a config
from this directory, diff its `[funnel]` block against the shipped one.

This has already bitten once. The first draft of H-007 and H-008 carried
`stages = ["s0", "s1", "s2", "s3"]` and only four criterion fields. Since then
the funnel landed and the schema requires `min_oos_trades`,
`min_oos_sessions`, `min_oos_return_pct_free_fills` and
`require_controls_beaten`, and **`s0`/`s3` are refused at load rather than
skipped**. Both configs would have failed to load. They are corrected; the
lesson is the rule above.

### 2.4 Registered gate order is binding, and S0 is the next build

Several files here pre-register a **predictor-first** gate — a no-trading
measurement of forward returns conditional on the signal — ahead of any equity
curve, per `docs/PROJECT_PLAN.md` §7.3. That gate is the funnel's **S0**, and
S0 is **not implemented**: a config declaring it is refused at load, because
*the combo grammar's rules produce positions, not a continuous score to bucket
forward returns by* (`crucible-funnel::stages`).

Closing that gap now has a name and a place in the plan: **the S0 predictor
seam** (D-0081) — a score-emitting evaluation path with forward-return joins,
which is the M2.5 predictor workbench arriving *as* the funnel's S0 rather than
beside it, so its report carries a trial count and a registry row instead of
standing outside the thing that counts. It is **M3's first block**
(`docs/MILESTONES.md`). Until it lands the refusal stands: `s0` stops being
refused at load in the commit where S0 can run, never earlier.

**A file whose Gate 0 is a predictor measurement may not skip to Gate 1 because
Gate 0 is inconvenient.** The order is part of the pre-registration: running the
equity-curve gate first and the predictor gate afterwards means the predictor
result is read with the backtest already known, which is the failure
pre-registration exists to prevent. Such a file is marked `blocked_on:` in its
front-matter and stays blocked until the seam lands. Grade A means *expressible
today*; it does not override a registered order.

**Six files carry a predictor-first gate**: H-001, H-008, H-011, H-012, H-013,
H-014. Only H-008 is marked `blocked` today, because the other five are already
blocked behind larger gaps (a resampler, a volume operand, an options loader)
and would not be runnable even with the seam. But all six ultimately want the
same thing — *bucket forward returns by a signal, without trading* — so the seam
is one piece of work serving six hypotheses, and it does not change a single
grade. That is worth saying plainly: it is the highest-leverage item here that
the grading scheme is structurally blind to, because the grades measure cost to
express and this measures whether we are allowed to look yet.

That is also the reason it is scheduled first rather than merely noticed
(D-0081): S0-refused stops the **front** of the funnel, so for these six the
block is not a delay that a faster machine shortens — no route to a verdict
exists that does not pass through it. Every other open M3 item improves an
answer this build already produces.

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

### Curated (replayable) — **1m only, now broadly complete**

Bulk transcode has advanced a long way since the first draft of this directory:
**371 contract partitions**, all carrying four-digit year keys (D-0072), with ES
fully transcoded across **69 contracts** and 6E, GC, RTY, CL present. Each
carries the tiled window set (`2010-06-06--2024-01-01`, `2024-01`,
`2024-02-01--2026-07-28`).

**Roll tables are built** — `curated/rolls/` holds ES, NQ, RTY and CL — so
`ES.v.0` is loadable and `backtest` replays it. The grid commands still do not
(§2.2, and deliberately).

Curated data is derived and rebuildable, so anything missing here costs a
command, not a purchase.

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
status: backlog | blocked
blocked_on: (optional) what must land before the REGISTERED FIRST GATE can run
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
| [H-008](H-008-short-horizon-overreaction.md) | Short-horizon overreaction and reversal | `momentum-horizon` | A · **blocked** | `es-short-horizon-reversal` |
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
| time-of-day / session predicates | §2.1's "Time of day / session position" row | D-0078 |
| volume operand | §2.1's "Volume" row | D-0079 |
| rolling normalizer | the *normalized-deviation* half of §2.1's "Arithmetic between operands" row; volume and volatility become relative rather than absolute | D-0080 |

Grade tally: **2 A · 8 B · 5 C**. That distribution is the honest one, and the
shape of it is the sweep's main structural finding: the combo grammar is three
indicators and six comparison operators, and almost every published intraday
result was stated in terms of a clock the grammar could not read — until
2026-07-30, when four of the five unlocks below landed at once. **That tally is
the pre-unlock triage**, kept as the historical record; the re-grade against the
grammar that now exists is recorded per file, in each file's own changelog.

### 6.2 The five unlocks — four landed, and the fifth is the one that matters

Four pieces of code moved most of the B column's *expressibility* problem, and
§6.1 says which row each retired. The fifth does something different and more
important: it is what makes any grade-A result *mean* anything, and it is still
pending. None of the five costs a purchase.

| # | Unlock | Status | What it frees |
|---|---|---|---|
| 1 | **Time-of-day / session predicates** (+ session anchors, D-0071 pattern) | **landed** (D-0078) | H-001, H-002, H-004, H-012, H-014 |
| 2 | **1m→5m/15m/1h/1d resampler** | **landed** (D-0077) | H-003, H-006, H-009, H-014 |
| 3 | **Volume operand** in the rule grammar | **landed** (D-0079) | H-004, H-012, H-013 |
| 4 | **Rolling normalizer** (point-in-time standardization) | **landed** (D-0080) | H-004, H-009, H-015 |
| 5 | **Registry pooling across contracts** | **pending** | *every grade A* — turns a ~60-session triage sample into a verdict sample (§2.2) |

Unlock 5 is not a convenience. Grade A means "runnable on one contract's life",
and no sample-adequacy criterion worth registering is satisfiable at ~60
sessions — so today's A-grade runs are guaranteed to be killed for sample size,
correctly, by the machine. That is the pre-registration working, but it means
**the A column produces no verdicts until pooling lands**. That has not changed
today and is the thing to keep in view: promoting a file from B to A moves it
from *inexpressible* to *runnable*, **not** to *answerable*. The four unlocks
grew the set of ideas we can state; only the fifth grows the set we can settle.

One prediction in this section was wrong and is left visible rather than
quietly corrected: it read "registry pooling ... likely arrives before any of
unlocks 1–4". Unlocks 1–4 arrived first, on 2026-07-30. Pooling is still the M3
workstream's.

A sixth item is *not* on this list on purpose: the continuous-series consumer
exists already (D-0073/D-0076) and grids are excluded from it deliberately, so
it is not pending work (§2.2).

### 6.3 Blocked entries

**H-008 is `blocked`**, despite being grade A. Its registered Gate 0 is a
predictor-first measurement — the funnel's **S0** — which is refused at load
because the combo grammar produces positions rather than a continuous score
(§2.4). Running its Gate 1 first would break the registered order, so it waits.
It is the **S0 predictor seam's first consumer and its specification** (D-0081):
what that file asks for in Gate 0 and Gate 0b — forward returns bucketed at
1/5/10/20 minutes, a block bootstrap over sessions, and the effect size in
**ticks** so it can be compared against the spread — is half of what the seam
has to do, the other half being the quantile/IC contract in
`crucible-funnel::stages`' module doc. Both halves are owed.

**H-007 is runnable now.** Its primary test reads the cost-sensitivity sweep and
per-round-trip PnL out of S1/S2 and needs no S0 seam.

Two entries carry a warning the grade does not: **H-012** and **H-013** are
graded on cost to test, and both have **no refereed empirical support** for
their predictive claims. H-013 is a named M2.5 target, so that finding matters
more than its grade does.
