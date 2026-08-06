---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: crude-regime-switching-garch
topic: vol-regime-clustering
grade: B
hypothesis_family: cl-volatility-forecast-regime-switching
status: draft
blocked_on: a regime-switching conditional-variance indicator, and a criterion that scores a VOLATILITY FORECAST — the funnel scores position rules, not forecasts
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Two-regime versus one-regime variance models for crude oil

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

Yue-Jun Zhang, Ting Yao, Ling-Yun He. *Forecasting crude oil market volatility: can the Regime Switching GARCH model beat the single-regime GARCH models?*.
arXiv q-fin, 2015.
**no DOI** (preprint). <http://arxiv.org/abs/1512.01676v1>
Retrieved from the arxiv API on 2026-08-06.

The authors run a comparison between several single-state GARCH specifications, linear and asymmetric, and a two-state Markov-switching version, on crude oil volatility across several data frequencies and forecast horizons. Their reading is that the switching version fits better in sample on most of the measures used and forecasts daily variance more accurately, that the advantage fades as the data is coarsened to weekly and monthly, and that the simplest linear specification does best for tail-risk forecasting.

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

There is nothing to trade in this paper, and saying so is the substance of the entry rather than an aside. It is a horse race between variance models: two states against one, across frequencies and horizons, scored on forecast accuracy and on tail-risk measures. No position is ever taken, so no counterparty is ever named, and none can be constructed from the result — the only party who pays for a variance view is an options counterparty, and this project holds no options data and trades no options. What the paper is worth here is a prior, and a deflating one: the switching advantage shows up at daily frequency and fades as the grain coarsens, and different loss functions pick different winners. Anyone about to spend a milestone on a latent-state volatility indicator should read that as the realistic ceiling on what state machinery buys.

## Signal in Crucible terms

- There is no signal to register, because the paper contains no position rule. That is the honest statement and it is why the second half of `missing` exists.
- The funnel scores position rules and nothing else. Registering forecast accuracy as a criterion would need a second scoring path — a loss function over a predicted variance series — that no milestone builds.
- The tradeable descendant is the same state gate as the previous candidate: `CLZ2024` at `1d`, with a filtered turbulent-state probability as an operand, which the grammar cannot name.
- The expressible substitute is again a threshold on `stdev(period, return)`, and the comparison between it and a latent state is the only version of this paper's question this project could ever ask.
- Anything run under this key must be labelled as testing a GATE, not a FORECAST, in the scorecard and the registry row — otherwise the registration and the run are answering different questions.

## Data

- Owned: CL `ohlcv-1m` 2010-06-06 → 2026-07-28, curated, resampled on read to 1h and 1d. The market is genuinely one we trade, unlike most of this batch.
- Owned: an energy session calendar with eras (D-0089), needed for trading-day bars to be trading-day bars.
- Not owned: options or implied-volatility data, so the tail-risk application the paper ends on has no path here at all.
- Not built: a latent-state indicator, and no scoring path for a forecast rather than a position.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Registered for the GATE descendant only; the forecast comparison itself is not registrable here and no run may claim to have made it.
- `min_oos_sessions = 500` — basis: the paper's own finding is frequency-dependent, so a short sample at one grain answers nothing.
- The discriminator that can kill it: the two-state gate must beat a single fixed `stdev` threshold on the same windows. If it does not, the extra state bought nothing and this is Killed — which, given the paper's own fading advantage at coarser grains, is the outcome to plan for.
- `min_oos_sharpe_after_costs = 0.3` and `kill_if_dead_at_ticks = 1.0` — basis: a daily state gate trades rarely, so one tick of half-spread is a low bar and failing it is decisive.
- `require_plateau = true` over the state lookback and `max_permutation_p = 0.05` — basis: a regime label that moves wholesale with a small lookback change is a fitted label.
- `max_pbo = 0.5` — basis: model-selection freedom is the whole content of a horse race, and PBO is what charges for it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- An arXiv preprint from 2015; the index records no refereed venue, and nobody has read it. This restatement comes from the indexed abstract alone.
- The paper's own conclusion is self-limiting — the switching advantage does not survive coarsening the data — and it reports several evaluation criteria with different winners. That honesty is unusual and is the main reason the entry is worth keeping; it also means 'the switching model wins' is itself a selection over criteria.
- The market is crude oil, which we hold sixteen years of. That is a genuine match; the mismatch is that the object measured is a variance series and ours would be a position.
- No trading costs appear anywhere in the paper, because no trades appear. There is therefore nothing of theirs that a cost sweep could be compared against.
- The paper reports its own forecast-accuracy comparisons; they are not restated here.
- Any descendant's costs rest on `half_spread_ticks = 1` (D-0120), which for CL will never be measured in this archive.

## Triage grade

**B.** B stands, and it is blocked twice over. The first gap is the same latent-state indicator the GSCI candidate needs — an expanding-window filtered probability, since a full-sample fit is lookahead. The second is structural and cheaper to state than to fix: the funnel scores position rules, so the paper's actual question, forecast accuracy, cannot be registered as a criterion here at all without a second scoring path.
