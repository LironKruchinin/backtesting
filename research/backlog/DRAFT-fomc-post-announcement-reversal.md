---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: fomc-post-announcement-reversal
topic: macro-announcements
grade: C
hypothesis_family: equity-index-fomc-reversal
status: draft
blocked_on: an FOMC calendar; and the reversal predictor is a change in an options-implied index (VIX), which `external/cboe/` does not hold
created: 2026-08-06
doi: 10.2139/ssrn.4131740
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Fading the move after a scheduled policy announcement

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

Oliver Boguth, Adlai J. Fisher, Vincent Gregoire, Charles Martineau. *Noisy FOMC Returns? Information, Price Pressure, and Post-Announcement Reversals*.
SSRN Electronic Journal, 2022.
DOI `10.2139/ssrn.4131740`. <https://doi.org/10.2139/ssrn.4131740>
Retrieved from the crossref API on 2026-08-06.

The paper applies microstructure tools to aggregate equity returns after scheduled policy announcements and reports that a meaningful part of the announcement-window move is given back before the following meeting. Changes in an options-implied volatility index, unusual volume, variability in order imbalance and fund flows are each reported as forecasting the give-back. The reversal is reported as distinct from the policy surprise itself, which the authors describe as entering prices only gradually.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.4131740':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This is the one candidate in the batch whose payer is both named and plausible. The story is price pressure: the announcement triggers a burst of demand for immediacy — funds re-establishing exposure, flow-driven creation and redemption, risk systems rebalancing onto the new level — and that demand pushes the price past where information alone would leave it. Whoever holds the other side through the imbalance is paid when the price comes back, and the payer is whoever had to be positioned by the close. The catch is structural rather than statistical. Earning a liquidity premium means posting, not taking, and every fill model in this build crosses the spread. A strategy whose entire return is compensation for supplying immediacy, evaluated by a model that pays for immediacy on both legs, is being charged the very thing it claims to earn.

## Signal in Crucible terms

- Instrument: `ESH2024` and the rest of the ES chain. Timeframe `1h` or `1d`, both aggregated on read from stored one-minute bars (D-0077) — and a daily bar here is a trading-day bar opening the previous evening, which matters for an event dated to an afternoon announcement.
- The rule would be: after a scheduled announcement, take a position against the announcement-window move and carry it toward the next meeting. The meeting calendar defines both ends of the holding period, so it is not one term of the rule but its frame.
- The conditioning predictor is a change in an options-implied index, and the archive holds no options data and no such series. The conditional version cannot be built at all; the unconditional version — fade every announcement move — is a weaker and different claim.
- The paper's other reported predictors are no cheaper: variability in order imbalance needs book data, and the only book data here is ES for one year (D-0120). Fund flows are a third-party series nobody has costed.
- A holding period running between meetings is roughly six weeks, and the grammar cannot express a duration — the exit would have to be another calendar predicate, which is the same missing operand a second time.

## Data

- Owned: ES `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one minute; coarser grains aggregate on read.
- Owned and relevant for once: `tbbo` and `trades` for ES, 2025-07-28 → 2026-07-28, plus one month of `mbo`. That is a single year and about eight events — enough to inspect the microstructure of a handful of announcements, nowhere near enough to test a reversal.
- Not owned: the meeting calendar, any options-implied volatility index, any fund-flow series.
- The sample ceiling is the same as the pre-announcement candidate: on the order of 130 scheduled events in the whole archive. Because the holding period is weeks, the trade count and the event count are the same number.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_trades = 100`. Basis: about 130 events exist in the archive and the rule is one round trip per event, so this gate sits just under a permanent ceiling and is registered knowing it nearly binds on its own.
- `max_permutation_p = 0.01`, block length declared and swept (D-0087). Basis: a six-week holding period puts the block scale and the strategy horizon at the same order, which is exactly the case D-0087 warns produces a conservative null — the sweep is what makes the number readable rather than decorative.
- `kill_if_dead_at_ticks = 1.0`. Basis: eight round trips a year makes the spread nearly irrelevant, so a failure at one tick would mean the effect is smaller than one tick over six weeks, and that is not an effect.
- `require_controls_beaten = true`. Basis: a six-week long position in equity index futures across 2010–2026 is close to buy-and-hold, and the control is what separates a reversal from the drift of the sample it was measured in.
- The unconditional and conditional versions must be declared as separate combos and both charged as trials. Basis: without the implied-volatility predictor the conditional version cannot run at all, and reporting an unconditional result under a hypothesis that promised conditioning is the pre-registration failure D-0101 exists to prevent.
- `max_pbo = 0.5`. Basis: window and horizon are free parameters, and CSCV is what prices a rule chosen across them.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Venue: a working paper on a preprint server. Our metadata records no journal version, so nothing we hold has been refereed in a way we can see, and the working prior applies at full strength.
- The reversal is documented on aggregate equity returns; ours would be one futures contract chain. Close, but not the same instrument — and the paper's own predictors come from the options market, which this project does not hold at all.
- Their sample precedes 2022 and ours runs to 2026-07-28, so the two overlap substantially. A positive result here is therefore partly a rerun rather than an out-of-sample test; the non-overlapping tail is a few years and a few dozen events, which is not enough to stand alone.
- The strongest objection is not statistical. The return being claimed is compensation for supplying liquidity, and this build only ever demands it. That is a modelling gap `queue_sim` would begin to close in M4, and until then a positive result would be surprising in the wrong direction — it would imply the effect is considerably larger than price pressure alone would explain, which is a reason to look for a bug.
- The paper reports its own reversal magnitudes and the predictive coefficients behind them; those describe its sample and are not restated here as anything this build would produce.

## Triage grade

**C.** C, and it carries two blockers rather than one. `missing` names the meeting calendar and the options-implied predictor, and the second is the expensive half: no options data is owned, none is planned, and without it only a weaker unconditional version exists. Costing that predictor means a new vendor and a new availability rule. The liquidity-supply objection is a third problem again, and one that only M4's `queue_sim` could answer.
