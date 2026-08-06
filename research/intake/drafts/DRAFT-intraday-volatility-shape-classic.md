---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: intraday-volatility-shape-classic
topic: overnight-intraday
grade: A
hypothesis_family: equity-index-intraday-volatility-shape
status: draft
created: 2026-08-06
doi: 10.1111/j.1540-6261.1990.tb03705.x
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The U-shaped session variance profile as a clock gate

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

Larry J. Lockwood, Scott C. Linn. *An Examination of Stock Market Return Volatility During Overnight and Intraday Periods, 1964–1989*.
The Journal of Finance, 1990.
DOI `10.1111/j.1540-6261.1990.tb03705.x`. <https://openalex.org/W2108554238>
Retrieved from the openalex API on 2026-08-06.

Measuring hourly US stock market return variance across a quarter century, the authors find dispersion is highest in the first hour, declines through the middle of the day, and picks up again into the close, and that the trading day carries more variance than the hours the market is shut. They also date several shifts in the overall level of market variance to identifiable structural changes in the 1970s and 1980s.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1111/j.1540-6261.1990.tb03705.x':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The U-shape is one of the oldest stylized facts in this literature and it is not, by itself, a claim that anyone pays you. It says dispersion is high at the ends of the session and low in the middle. A rule built on it is really betting that per-trade edge scales with dispersion faster than the spread does, so the same signal is worth taking at 09:35 and worth skipping at 12:30. There is a nameable payer at the ends of the day: index funds and any flow benchmarked to the opening or closing print must transact then whatever the price, and news that arrived overnight is repriced in the first minutes by participants who could not act sooner. They keep paying because the mandate names the time, not the price. The middle of the session has no such payer — which is exactly why a version that works there and not at the edges should be treated as a fluke.

## Signal in Crucible terms

- `ESM2024` (and siblings), `timeframes = ["15m"]`. The U-shape is a conditioner, so it is paired with a plain reversion core rather than traded on its own.
- `[indicators.z] kind = "zscore", period = [20, 40, 60], source = "close"`.
- Opening arm — `enter_long = "minutes_since_rth_open <= 60 and z crosses_below -2.0"`; `exit_long = "minutes_since_rth_open > 60 or z crosses_above 0.0"`; mirrored for the short side.
- Closing arm — the same rules with `minutes_to_rth_close <= 60`. Note `minutes_to_close` shortens on an early close and `minutes_to_rth_close` does not (D-0078); the scheduled-day question is the right one here, so `minutes_to_rth_close` is the operand.
- Midday control arm — `minutes_since_rth_open > 90 and minutes_to_rth_close > 90`. This arm is expected to fail, and it is registered so that a result appearing everywhere can be recognized as not being about the clock at all.
- Thresholds enumerated (`[1.5, 2.0, 2.5]`); window lengths on an integer axis may use `{ start, end, step }`. The last regular-hours bar reads `is_rth` even though its interval ends at the bell (D-0078), so a `minutes_to_rth_close <= 15` gate does include it.

## Data

- Owned: ES `ohlcv-1m` from 2010-06-06, curated, resampled to 15m on read; the equity-index calendar supplies a real, measured RTH window for this root, unlike the commodity tables.
- Not owned: the instrument they measured. Their series is the US cash stock market 1964–1989; ours is an E-mini futures contract in a 23-hour market. The overnight half of their comparison has no counterpart here — our overnight is continuous trading, not a closure.
- Not owned: hourly cash-index data, NYSE floor-era microstructure, or anything that would let their structural-break findings be checked.
- Constraint: one raw contract per config, roughly 60 sessions for ES.
- `half_spread_ticks = 1` (D-0120) is uniform across the session, which is precisely the assumption a U-shape hypothesis stresses — the real spread is widest at the same hours the strategy wants to trade.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a time-of-day effect must be measured across a year of days, not across a handful. Unreachable today, and the run will be killed for it.
- `min_oos_trades = 200` — basis: each clock arm can fire at most a few times per session, so anything smaller is a sample of days wearing the clothes of a sample of trades.
- The discriminator that can kill it: the opening or closing arm must clear `min_oos_sharpe_after_costs = 0.3` while the midday control arm does not. If the midday arm does just as well, the effect is not about the session clock and this is Killed even with a profitable curve on the page.
- `require_plateau = true` over the gate width — basis: an effect that exists at exactly 60 minutes and vanishes at 45 and 75 is a spike, and a genuine session profile is smooth by construction.
- `kill_if_dead_at_ticks = 1.0` — basis: the hours this trades are the hours the book is widest, so an edge that needs a tight spread has been measured in the wrong place.
- `max_permutation_p = 0.05` and `require_controls_beaten = true` — basis: block permutation is what distinguishes a clock effect from an ordinary draw of a trending contract.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- 1964–1989 US cash equities, hourly, measured through the fixed-commission and floor-trading eras. The market is not the one we trade and the era is not ours; almost nothing about the execution environment survived.
- The Journal of Finance is a strong venue, but the claim is descriptive. A well-established fact about variance is not evidence of a return effect, and this batch's working prior is that the step from one to the other is where papers go wrong.
- The paper's own contribution includes several dated shifts in market variance — which is itself a statement that the profile is not stable. No figures of theirs are restated here.
- The U-shape is so well known that any edge conditioned on it is, by assumption, competed against by everyone. That is a reason to doubt it survives costs, not a reason to test it more cheaply.
- The midday control arm exists because without it a positive result is uninterpretable; registering it now, before any run, is the only way it stays a control rather than a footnote.
- `half_spread_ticks = 1` is an assumption (D-0120) and it is biased in this idea's favour at exactly the hours the idea trades.

## Triage grade

**A.** A: `minutes_since_rth_open`, `minutes_to_rth_close` and a trailing z-score express all three arms in TOML today, with no new Rust. But runnable is not answerable — one ES contract is roughly 60 sessions against a registered `min_oos_sessions = 250`, so the machine kills this correctly for sample size until registry pooling across contracts arrives. The midday control is what makes the eventual pass mean anything.
