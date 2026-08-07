---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: wti-term-structure-forecast
topic: term-structure-roll-yield
grade: C
hypothesis_family: cl-term-structure-return-forecast
status: draft
blocked_on: multi-contract curve construction in one config — the DATA is fully owned (every CL contract, sixteen years), so this is a machinery gap and not an acquisition
created: 2026-08-06
doi: 10.1016/j.eneco.2021.105350
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Curve-shape factors as a predictor of crude futures returns

> **This is a DRAFT, not a registration.** Nothing here has been run and
> nothing here is a recommendation. It was built from index metadata — title,
> venue, year, and the abstract the API returned — by `research/intake`;
> **the paper itself has not been read**. Promote it into `research/backlog/`
> by hand, after reading, or delete it.
>
> **The kill criteria below are PROPOSALS**, marked `criteria_status:
> proposed` in the front matter. A proposal is not a pre-registration: it
> becomes one when Liron approves it, by name, and the file is promoted. The
> marking is what lets a later reader tell a criterion someone committed to
> from a number a drafter suggested.

## Citation

Don Bredın, Conall O’Sullivan, Simon E. F. Spencer. *Forecasting WTI crude oil futures returns: Does the term structure help?*.
Energy Economics, 2021.
DOI `10.1016/j.eneco.2021.105350`. <https://openalex.org/W3169100372>
Retrieved from the openalex API on 2026-08-06.

The authors summarise the shape of the crude futures curve with a small number of fitted factors and report that those factors carry information about subsequent holding-period returns within their sample. They extend this out of sample by combining the curve factors with macroeconomic and oil-specific predictors under a shrinkage estimator, and compare the result against a no-change benchmark across several horizons and maturities.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.eneco.2021.105350':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The curve's shape is the market's own statement about the cost and scarcity of storage, and it is the closest thing commodities have to a valuation signal. Backwardation says the physical barrel is scarce enough that its holder is being paid to part with it. The losing side is nameable and well documented: the producer hedging forward, whose business risk is the price and whose financing frequently requires the hedge, and the index investor who rolls a long position on a published schedule regardless of what the curve looks like on the day. Both keep paying because neither is trying to win the roll — one is buying certainty, the other is buying exposure to an index specification. The problem is that this is the most heavily published commodity factor in existence, so the interesting question is not whether the payer exists but whether anything is left after everyone who reads the same journals has queued in front of you.

## Signal in Crucible terms

- Instruments: several CL contracts of different maturities read together — for example `CLZ2024` against `CLM2025`. `combo` refuses a config declaring two instruments, by design, so nothing here is expressible.
- Timeframe: `1d`, aggregated on read; the paper works at horizons of weeks to months.
- Feature: a fitted summary of the curve's level, slope and curvature. Even a two-point slope needs a difference between two operands, and the grammar has no arithmetic between operands at all.
- Rule as it would be written: hold the front contract long when the fitted slope factor is in one tail and flat or short in the other, with the factor re-fitted on a trailing window only — a full-sample fit is the lookahead §2.1 forbids.
- The cheapest honest first step is not the paper's model but its simplest cousin: a two-maturity slope, which needs multi-instrument configs and one arithmetic operator, and which would settle whether the elaborate factor structure is earning its keep.

## Data

- Owned in full: every CL contract, `ohlcv-1m`, 2010-06-06 to 2026-07-28, curated. This is the rare hypothesis where nothing needs buying — the block is entirely machinery.
- Owned: expiries from the archived definition records, so the maturity axis a curve needs is constructible without new acquisition (D-0090).
- Not owned: the macroeconomic leading indicators and oil-specific predictors that the paper's out-of-sample models depend on. The arm we could reproduce is the pure-curve one, which is the weaker arm in the paper's own account.
- Not owned: any inventory series, so the storage-cost interpretation of the curve cannot be checked against the physical quantity that is supposed to drive it.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 750` — basis: three years of out-of-sample sessions, because the paper's horizons run to months and a shorter window contains too few independent holding periods to say anything.
- `min_oos_trades = 60` — basis: a slope-conditioned position turns over on the order of monthly, so this asks for roughly five years of signal changes.
- `min_oos_sharpe_after_costs = 0.50` — basis: a two-leg construction pays the spread twice on entry and twice on exit, so the after-cost floor has to be meaningfully above zero to survive the doubling.
- `kill_if_dead_at_ticks = 1.0` — basis: with two legs the assumed one-tick half-spread is charged four times per round-trip, and an edge that cannot carry that is not an edge in a market this liquid.
- `max_pbo = 0.35` and `max_permutation_p = 0.05` — basis: the factor structure has several free choices — how many factors, what decay, what horizon — and a curve factor is precisely the sort of construction that reproduces itself on permuted returns because it re-fits.
- `require_controls_beaten = true` — basis: a long-biased crude position over 2010 to 2026 spans several enormous directional moves, and buy-and-hold must be beaten explicitly or the result is a statement about the oil price.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper reports its own performance comparisons against passive and historical-mean benchmarks; they are not restated here, and none of them is a claim about what this archive would produce.
- The out-of-sample arm — the one that carries the paper's strongest claim on this metadata — leans on macro predictors we do not hold. The reproducible arm is the pure-curve one, so a null here would not contradict the paper.
- This factor is the single most published idea in commodity futures. Between index-fund flows after 2004, the shale supply response, and a negative front-month settlement in 2020, the curve's own generating process changed at least twice inside our sample.
- Sample overlap is heavy; the study is recent and its window sits largely inside ours.
- The half-spread assumption is more damaging here than anywhere else in this batch. A two-leg position pays it four times per round-trip, `half_spread_ticks = 1` is a convention and not a measurement, and CL has no L1 data in this archive and cannot acquire any (D-0120). The verdict on a spread trade should not rest on a number we chose.

## Triage grade

**C.** C, and it is the most frustrating C here because nothing needs buying. The block is that a config declares one instrument and one timeframe, and a curve is by definition several maturities read together — plus the grammar has no arithmetic, so even a two-point slope is inexpressible. Closing it means funnel-level multi-instrument orchestration and per-leg accounting, which is a milestone, not a task.
