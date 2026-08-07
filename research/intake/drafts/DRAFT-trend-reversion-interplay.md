---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: trend-reversion-interplay
topic: trend-horizon
grade: A
hypothesis_family: futures-trend-extremeness-interaction
status: draft
created: 2026-08-06
doi: 10.1016/j.physa.2020.125642
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Gating a crossover on how stretched the trend already is

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

Christof Schmidhuber. *Trends, reversion, and critical phenomena in financial markets*.
Physica A Statistical Mechanics and its Applications, 2020.
DOI `10.1016/j.physa.2020.125642`. <https://openalex.org/W3034373639>
Retrieved from the openalex API on 2026-08-06.

Working from three decades of daily futures prices across equity indices, rates, currencies and commodities, the author fits the next day's average move as a cubic function of current trend strength: a positive linear piece for persistence and a negative cubic piece for reversal, which together imply a critical strength beyond which trends turn. The coefficients are described as tiny and detectable only when many markets and many years are pooled. A statistical-mechanics analogy is offered on top.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.physa.2020.125642':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Two different payers are being asserted, and only one of them is plausible at our grain. Trend persistence pays because information diffuses unevenly and some participants adjust slowly — the loser is whoever is late. Reversal beyond a critical strength pays because the marginal buyer of an already-stretched move is the last chaser in, and value-oriented traders who ignored a weak move step in hard against a strong one; the loser there is the chaser, and the paper's own framing about saturation says as much. What should temper this immediately is the size of the effect the author reports finding: small coefficients that only reach significance once decades and dozens of markets are stacked together. An effect that needs that much pooling to be seen is an effect that no single instrument, over one contract's life, at minute grain, can pay you for. So the honest reading is that the payer exists but is spread so thin across the cross-section that we have no way to reach him from here.

## Signal in Crucible terms

- Instrument: `ESH2024` (and separately `CLM2024`, `GCM2024`) — one raw contract per config, four-digit key.
- Timeframe: `1h`, resampled on read from curated 1-minute bars; trading-day session anchoring, so no bucket spans a session boundary.
- `[indicators.fast] kind = "ema"`, `period = [10, 20]`; `[indicators.slow] kind = "ema"`, `period = [50, 100]`; `[indicators.stretch] kind = "zscore"`, `period = [50, 100]`, `source = "close"`.
- `enter_long = "fast crosses_above slow and stretch < 1.5"` — take the crossover only while the move is not already extended.
- `exit_long = "fast crosses_below slow or stretch > 2.5"` — the second clause is the paper's reversal claim, expressed as an exit rather than as a reversal entry.
- Mirror for the short side. The cubic itself is not expressible — the grammar has no arithmetic between operands — so a two-threshold piecewise gate is the coarsest possible stand-in, and the draft should say so rather than imply the functional form was tested.

## Data

- All seven roots hold curated 1-minute bars from 2010-06-06, resampled to `1h` on read; the paper works on daily prices, so our grain is two orders finer than the one it fitted.
- The paper's sample is thirty years ending around 2020. Ours starts in 2010, so at most a third of its window overlaps ours, and none of the pre-2000 data that carries most of its statistical weight.
- Missing: any cross-sectional pooling machinery. The paper's result is explicitly a pooled one; the funnel today runs one contract per config, which is the wrong shape for it.
- Missing: measured spreads for six roots — `half_spread_ticks = 1` is an assumption (D-0120).

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.03` at the config's declared forward horizon — basis: S0's information coefficient must clear a floor AND its bootstrap interval must exclude zero at the same horizon (D-0085); size without significance is what enough noise gives away free.
- `min_oos_sessions = 250` — basis: the paper's own claim is that the coefficients need heavy pooling to surface, so a sample floor below a trading year is not defensible for an idea whose author says it is faint.
- `min_oos_trades = 200` — basis: a gated crossover trades less than an ungated one, and the gate is the thing under test; too few round-trips and the gate's effect is unmeasurable.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills.
- `max_permutation_p = 0.05` — basis: the gate adds parameters to a crossover that already had two, and the block null is the only thing here that prices that.
- `require_plateau = true` — declared, not evaluated in this build (S3 owes it). Basis: a stretch threshold that works at 2.5 and fails at 2.0 and 3.0 is a spike, and a spike in a threshold whose paper calls it 'critical' is exactly the failure mode.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Physica A is a statistical-physics journal. The criticality and Langevin framing is decoration as far as we are concerned; it neither strengthens nor weakens the empirical claim, and a finance result refereed by physicists deserves the usual discount.
- Their data is daily prices over thirty years, pooled across four asset classes. We would run hourly bars on one CME contract for one quarter. Calling the result a test of the paper would be dishonest.
- The author states the coefficients are small and reach significance only in aggregate, and also reports that persistence has weakened as markets matured. Our sample is the tail of that decline, which is the least favourable slice of their own history.
- Out-of-sample testing and bootstrapping are claimed in the abstract; we have no way to check what was held out or when the functional form was chosen, and a cubic fitted to pooled data has more freedom than its two coefficients suggest.
- `half_spread_ticks = 1` is an assumption (D-0120). A gate that reduces trade count reduces cost sensitivity, so a passing result here should be read as partly a consequence of trading less.

## Triage grade

**A.** Every piece is in the grammar: two moving averages, a trailing `zscore` on close, comparisons, and/or. Nothing is missing. What is missing is sample: a grade-A config replays one contract's active life, roughly sixty sessions for ES, and this paper's own claim is that the effect only surfaces after pooling decades and dozens of markets. It will be killed for sample size, which is the right answer, until registry pooling lands.
