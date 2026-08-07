---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: eurusd-intraday-reversion
topic: short-horizon-reversal
grade: A
hypothesis_family: 6e-intraday-mean-reversion
status: draft
created: 2026-08-06
doi: 10.1515/foli-2015-0014
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Fading a one-minute stretch in EUR/USD futures

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

Marta Wiśniewska. *Eurusd Intraday Price Reversal*.
Folia Oeconomica Stetinensia, 2014.
DOI `10.1515/foli-2015-0014`. <https://doi.org/10.1515/foli-2015-0014>
Retrieved from the crossref API on 2026-08-06.

The paper reports that minute-by-minute EUR/USD behaves as a mean-reverting series: a unit-root test is rejected, deviations from a moving average average out to roughly nothing, and long and short positions opened at the same moment reach similar best-case excursions over a day. The author reads this as evidence against weak-form efficiency.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1515/foli-2015-0014':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

At one-minute scale in a major currency pair, apparent reversion is mostly a property of how transaction prices are recorded, not of what the market believes. Prices alternate between bid and offer as impatient buyers and sellers arrive, and that alternation looks exactly like reversion to anyone measuring last-trade prices. If that is the whole story, the payer is the impatient trader and the receiver is the market maker sitting on both sides, and we are on the wrong side of it: a `spread_cross` fill charges the spread on entry and again on exit, which is precisely the quantity generating the appearance. If instead there is genuine short-horizon overreaction, the payer is whoever pushed price too far and must be filled back: real, but small. Nothing in the paper's stated evidence separates those two, and nothing computable on OHLCV separates them either. That symmetry is why a Kill is the sensible prior — the mechanism that would pay us and the one that would bill us produce the same statistic.

## Signal in Crucible terms

- Instrument: `6EM2024` — CME EUR/USD futures, four-digit key, one raw contract per config.
- Timeframe: `1m`, stored grain, no resampling.
- `[indicators.stretch] kind = "zscore"`, `period = [30, 60, 120]`, `source = "close"`.
- `enter_long = "stretch crosses_below -2.0"`, `exit_long = "stretch >= 0.0"`, `enter_short = "stretch crosses_above 2.0"`, `exit_short = "stretch <= 0.0"`.
- Threshold axis `[1.5, 2.0, 2.5]` written as an explicit list — a float axis cannot be a `{ start, end, step }` range (D-0060).
- Not expressible: a fixed holding period. The exit is 'the stretch came back', not 'N bars later', and those are different strategies; the report must not describe one as the other.

## Data

- 6E holds curated 1-minute bars 2010-06-06 → 2026-07-28, which is the exact grain the paper studied — the closest grain match in this batch.
- The paper studies spot EUR/USD; we hold the CME future. Related, but a different venue, different hours, different tick, and a different participant mix.
- The FX session calendar (D-0089) puts 6E on its own template; on MLK 2022 6E traded a full session while ES and ZN closed at midday, which is why the four commodity calendars are separate tables.
- Missing, and decisively: quotes. `tbbo` exists for ES only, 2025-07-28 → 2026-07-28. For 6E there is no measured spread and never will be — the L1 entitlement lapsed (D-0120). The one number this idea turns on is the one we cannot measure.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `kill_if_dead_at_ticks = 0.5` — basis: this is the gate that decides the idea, and it is deliberately the tightest in the batch. If the edge does not survive half a tick, it was the bid-ask bounce and never anything else.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills; a minute-grain fade generates many observations, so there is no excuse for a marginal number here.
- `min_oos_trades = 500` — basis: raised well above the batch default because a two-sigma fade on minute bars fires often; anything below 500 round-trips means the threshold grid drifted somewhere it should not have.
- `min_oos_sessions = 250` — basis: one pooled trading year. A single 6E contract quarter cannot reach it, and that is stated rather than worked around.
- `max_permutation_p = 0.05` — basis: the block-permutation null is the only gate here that can tell a genuine reversion from the mechanical alternation of a recorded price series across blocks.
- `require_controls_beaten = true` — basis: the matched random-entry control is the relevant one for a symmetric fade; buy-and-hold on a currency future over a quarter is near enough to noise that beating it proves little.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Folia Oeconomica Stetinensia is a small regional journal. The prior on a market-inefficiency claim published there, in 2014, on the most liquid instrument in the world, should be very low.
- The stated evidence does not support the stated conclusion. A rejected unit root means the series is stationary, not that it is profitable. And 'the average deviation from a trailing moving average is close to zero' is close to a definitional property of a trailing mean, not a finding.
- Their instrument is spot EUR/USD on the interbank/retail market; ours is the CME 6E future. We hold no spot or interdealer series at all, so this is a transfer across venues, not a replication.
- By 2014 minute-scale EUR/USD was already an automated market. The paper says as much in framing HFT as background; a claim of exploitable minute-scale reversion in that environment is a claim to have found something a very large number of well-capitalised firms are looking for.
- `half_spread_ticks = 1` is an assumption and not a measurement for 6E, permanently (D-0120). This idea lives or dies on the spread, so our cost model is a guess about the one number that decides it — and the guess is why `kill_if_dead_at_ticks` is set at 0.5 rather than 1.0.

## Triage grade

**A.** Fully expressible: one `zscore` on close, four comparisons, one raw contract, one timeframe. Runnable today — and today's run is guaranteed to be killed for sample size, correctly, by the machine. A grade-A config replays one contract's active life, roughly sixty sessions, and no sample floor worth pre-registering survives that. Registry pooling across contracts is what turns this from runnable into answerable.
