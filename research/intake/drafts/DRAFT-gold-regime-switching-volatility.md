---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: gold-regime-switching-volatility
topic: vol-regime-clustering
grade: B
hypothesis_family: gc-regime-switching-volatility-rule
status: draft
blocked_on: a Markov regime-switching conditional-variance indicator; the paper's trading rule is driven by the fitted regime probability, which no operand can name
created: 2026-08-06
doi: 10.4236/jmf.2012.21014
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Regime-switching variance as a gold trading filter

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

Nop Sopipan, Pairote Sattayatham, Bhusana Premanode. *Forecasting Volatility of Gold Price Using Markov Regime Switching and Trading Strategy*.
Journal of Mathematical Finance, 2012.
DOI `10.4236/jmf.2012.21014`. <https://openalex.org/W1967758914>
Retrieved from the openalex API on 2026-08-06.

The authors fit conditional-variance models whose parameters switch between two unobserved states to a gold price series, and argue those beat a single-state specification on some of the loss functions they check. They then bolt a futures trading rule onto the resulting forecast and compare it against rules built from simpler models.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.4236/jmf.2012.21014':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Volatility clusters, and a two-state latent variable is a blunt but defensible way of saying that a quiet tape and a disorderly tape are different processes. A rule that stands aside when the fitted state says disorder is really a position-sizing rule wearing a forecast's clothes, and sizing can improve a return series without predicting anything at all. The trouble is the counterparty. Nobody pays you for knowing that variance is high. Variance is not a traded quantity in this archive — no gold options, no implied series — so there is no volatility risk premium on offer and nobody systematically short it. What is left is a directional rule conditioned on a variance state, and a variance state carries no sign. The losing side therefore cannot be named, and that is not a blank to be filled in later; it is the single best reason to expect this to die early and cheaply.

## Signal in Crucible terms

- Instrument: one CME gold contract per config, spelled with a four-digit year (`GCZ2024`). A sixteen-year answer needs pooling across contracts, which this build refuses to orchestrate.
- Timeframe: `1d`, aggregated on read from stored 1-minute bars on the exchange's own sessions (D-0077).
- The feature the paper drives its rule with is a filtered probability of occupying the high-variance state. No `IndicatorKind` names a latent state and no operand can reference one, which is the gap.
- Nearest expressible surrogate: `stdev(period, source = 'return')` against a constant, e.g. `enter_long: stdev_ret_20 < 0.0008 and close crosses_above sma_50`. That is a threshold on trailing dispersion, not a fitted regime, and it discards the entire model.
- Where it breaks hardest: any state estimated over the whole series is the lookahead §2.1 names by name. The only legal version re-estimates from data available at each bar, which is a new indicator with its own warmup declaration, not a config change.

## Data

- Owned: GC `ohlcv-1m`, 2010-06-06 to 2026-07-28, curated and replayable. Coarser grains are aggregated on read; there is nothing to build first.
- Not owned: any options or implied-volatility series on gold. The volatility-premium reading of this idea is unreachable rather than merely unbuilt.
- The paper's underlying is a gold price whose construction we cannot see from metadata; a cash quote and a front-month future differ by carry and by the roll, and the difference is not small over a decade.
- GC has a session table with eras (D-0086, D-0089), so trading-day aggregation and out-of-session flags are honest for this root rather than guessed.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: one gold contract yields on the order of sixty replayable sessions, so this floor cannot be met until registry pooling lands. The run being killed for sample adequacy is the correct outcome, not a threshold to soften.
- `min_oos_trades = 100` — basis: a variance filter trades rarely by construction, and below a hundred round-trips the fold statistics are describing a handful of episodes rather than a rule.
- `min_oos_sharpe_after_costs = 0.40` — basis: gold's tick is ten dollars and this rule is low turnover, so a modest floor is still informative; set lower, the machinery is not worth running.
- `kill_if_dead_at_ticks = 1.0` — basis: `half_spread_ticks = 1` is the archive's standing assumption (D-0120), so an edge that evaporates at exactly the assumed spread cannot be told apart from an artefact of the assumption. This is the gate most likely to kill it.
- `max_permutation_p = 0.05`, with the block length declared before the run and swept over (D-0087) — basis: a filter that re-estimates itself is precisely the sort of rule that keeps its apparent edge on shuffled returns.
- `require_controls_beaten = true` — basis: sitting out high-variance stretches mechanically resembles holding a smaller position, and the matched random-entry control is what separates the two readings.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is an open-access mathematics-of-finance title from a publisher with a weak editorial reputation, and the work is from 2012. Nobody here has read the paper; this draft rests on index metadata alone.
- The paper reports its own cumulative performance comparisons between model variants. They are not restated here, and none of them is a statement about anything this archive would produce.
- A better variance forecast is not a strategy. The step from 'the model fit improved' to 'the rule earned money' is exactly where this literature stops being checkable from an abstract, and it is the step we cannot see.
- Sample overlap is unhelpful: their window ends around the start of ours, so a confirmation here would be a different decade tested with a rule chosen in an earlier one, and a contradiction would be uninformative about their era.
- Every cost number rests on `half_spread_ticks = 1`, an assumption wearing the name a measurement would wear. GC holds no L1 data in this archive and cannot acquire any (D-0120), so this will never be settled for gold.

## Triage grade

**B.** The named gap is real and specific: the rule reads a fitted regime probability, and every statistic in the grammar is a trailing window over completed bars with no latent state anywhere. Closing it means a new indicator kind that re-estimates using only data available at each bar, a declared warmup so §2.6 aligns the grid, and a hand-derived fixture test. Most of that cost is spent proving the fit never sees forward data.
