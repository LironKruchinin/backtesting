---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: egarch-gated-breakout
topic: breakout-range-expansion
grade: B
hypothesis_family: equity-energy-volatility-gated-breakout
status: draft
blocked_on: a conditional-variance (EGARCH) indicator; every statistic in `crucible-strategies::indicators` is a trailing window, and no `IndicatorKind` names a fitted model
created: 2026-08-06
doi: null
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Breakout entries gated on a conditional-variance state

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

Woosub Shin. *Volatility Gated Momentum: An EGARCH Filtered Intraday Trading Framework for Equity Index and Energy Futures*.
Research at the University of Copenhagen (University of Copenhagen), 2026.
**no DOI** (preprint). <https://openalex.org/W7165437951>
Retrieved from the openalex API on 2026-08-06.

A master's thesis proposing that intraday breakout signals only carry direction in particular volatility conditions. It fits an EGARCH model to obtain a conditional-variance state, permits long-only breakout entries only while that state qualifies, and evaluates it on 5-minute bars for the Nasdaq-100, for the S&P 500, and for WTI crude, spanning 2019 through 2025. The headline is a comparison against the identical framework with the gating layer switched off.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == None:
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The claim is conditional rather than directional: a breakout is supposed to be noise in quiet conditions and information in violent ones, so estimating a variance state and trading only in the qualifying state should raise the share of signals that mean anything. The trouble is that a filter creates no counterparty. It selects when to take a bet somebody else already had to be losing. Whoever pays a breakout trader is presumably the liquidity provider standing in front of a real information arrival, and the gate's only job is to stop paying that provider on the days nothing arrives. The thesis identifies no payer, offers no reason a payer would persist, and rests its headline on a comparison between two nested specifications on one window. That is the weakest evidence available: it shows that the richer specification fit better, which richer specifications generally do.

## Signal in Crucible terms

- Instruments `NQZ2024`, `ESZ2024`, `CLM2024`; timeframe `5m`, aggregated on read from curated 1-minute bars. One instrument per config, so this family is three registrations, not one.
- The construction WOULD be: a conditional-variance slot from an EGARCH(1,1) fit, a state test such as `egarch_state > threshold`, and a breakout entry conditioned on it.
- Where it breaks, first: no `IndicatorKind` names a fitted model. Everything in the indicator set is a trailing window behind a one-bar `update`, and no constructor takes a series.
- Where it breaks, second, and this is the harder half: a model fitted once over the whole span is a full-sample statistic and is Sec 2.1 lookahead by definition. A legitimate version must refit inside each fold's training window only, which drags the fold seam into the indicator layer.
- Where it breaks, third: the breakout leg itself needs a rolling max/min, which is the same gap the Donchian entry in this batch has.
- The nearest thing expressible today is `stdev(period, source=return)` as a trailing gate — a different object, a different hypothesis, and it deserves its own file rather than being smuggled in under this one.

## Data

- Owned: curated 1-minute ES, NQ and CL bars covering 2010-06-06 to 2026-07-28, so the thesis's 2019 to 2025 window sits entirely inside ours with eleven extra years to spare.
- Owned: the `5m` grain by aggregation on read (D-0077), on the exchange's own sessions rather than a UTC grid.
- Not owned: any measured spread for NQ or CL — `half_spread_ticks = 1` is an assumption for six of the seven roots and always will be (D-0120).
- Not owned: overnight or auction-level detail beyond OHLCV bars, so a variance state estimated here is estimated from bar data, not from the tick record the thesis had.
- No purchase is required for this idea; the gap is entirely in code.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The gated arm must beat the ungated arm on `min_oos_sharpe_after_costs` by a registered margin, else Kill — basis: the thesis's own headline is the comparison, so a run where the gate adds nothing has refuted the hypothesis regardless of whether either arm made money.
- Every EGARCH refit must be confined to the fold's training window, and a run fitted across the whole span is void rather than a result — basis: Sec 2.1, and D-0088's truncation-invariance harness is the tool that proves it.
- `max_permutation_p = 0.05` — basis: a gate is a selector over subsamples, which is precisely the setting where an ordinary draw looks extreme; the block-permutation null is what tells the two apart.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: a gated strategy trades a fraction of the bars, so the session floor buys the trade floor; one contract reaches neither and will be killed for sample adequacy.
- `kill_if_dead_at_ticks = 1.0` and `require_controls_beaten = true` — basis: a long-only rule on equity index over a rising window beats nothing by default, so buy-and-hold is the control that matters here.
- `max_pbo = 0.5` — basis: the gate adds a threshold and the model adds parameters, so the grid grows fast and the backtest-overfitting probability is the statistic that reads that growth.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- This is an unpublished master's thesis by a single author with no DOI and no peer review. That is the weakest provenance in this batch by a wide margin, and the grade should not be read as an endorsement of the finding.
- The sample runs 2019 to 2025 and contains the COVID crash and the 2022 rate shock. Those are perhaps three genuinely independent volatility episodes, so a volatility-state model has very few effective observations however many bars it saw.
- The thesis attributes a circular block bootstrap p-value of 0.004 to its gated-versus-ungated comparison; that is its number, on its data, and nothing here forecasts anything about ours.
- Long-only equity index over 2019 to 2025 is a period the underlying rose substantially. Any long-only result on that window has to clear buy-and-hold before it means anything, and the abstract does not say it was compared against one.
- A gate chosen after seeing which gate worked is one comparison drawn from a large family of possible gates, and nothing in the abstract indicates the family size was accounted for.

## Triage grade

**B.** Data is owned and the window is inside ours. The missing piece is a conditional-variance indicator, and it is a bigger job than it sounds: every existing statistic is a trailing window updated one bar at a time, whereas an EGARCH state is a fitted object that must be refit per fold to stay legal under Sec 2.1. That means an indicator that knows about folds, plus a truncation-invariance control before any result is quoted. The breakout leg needs the rolling max/min separately.
