---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: oil-overnight-predicts-equity-vol
topic: overnight-intraday
grade: C
hypothesis_family: cl-es-cross-session-volatility
status: draft
blocked_on: multi-instrument configs — the claim relates CL's overnight session to ES's realized volatility, and one config names one instrument
created: 2026-08-06
doi: 10.1002/for.2903
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Crude's overnight variation as an input to equity-index volatility forecasts

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

Feng Ma, M. I. M. Wahab, Julien Chevallier, Ziyang Li. *A tug of war of forecasting the US stock market volatility: Oil futures overnight versus intraday information*.
Journal of Forecasting, 2022.
DOI `10.1002/for.2903`. <https://doi.org/10.1002/for.2903>
Retrieved from the crossref API on 2026-08-06.

The authors split high-frequency crude futures variation into an overnight part and a within-day part and use both to forecast US stock market volatility. High overnight variation in oil is associated with higher subsequent equity variance, and downward overnight oil moves carry more of the forecasting weight than upward ones. Splitting the components by the sign of within-day moves is reported to help out of sample, including in turbulent stretches.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/for.2903':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The story is a lead-lag in risk rather than in price: crude trades through hours when US equities are quiet, so an energy shock lands in the oil tape first and equity variance catches up the next day, with downward moves counting for more because leverage and margin respond asymmetrically to losses. That is plausible, and it is also not a strategy. No losing side can be named here, and the paper does not attempt to name one — it forecasts a variance statistic, and the natural counterparty for a variance view is an options seller, a market this project holds no data for and does not trade. A directional futures rule conditioned on a variance forecast is a different claim the paper never makes and never tests. Until someone specifies who is paying and why they keep doing it, the honest reading is a forecasting result with no identified payer, which is the category this backlog is most sceptical of.

## Signal in Crucible terms

- Faithful construction: CLZ2024's overnight variation as the conditioner, ESZ2024 as the traded leg. That is two instruments in one config, which `combo` refuses by design rather than silently running the first.
- Second blocker: 'overnight realized variation' is a session-scoped aggregate — a sum over the bars of one session block — and the grammar has no session accumulator. `stdev(period, return)` is a trailing window that walks across the boundary rather than resetting at it.
- Third blocker: the sign decomposition needs upside and downside pieces separated, which needs arithmetic between operands the grammar does not have.
- Single-instrument approximation (CL conditioning CL) is expressible and is a different hypothesis; if anyone wants it, it gets its own family key rather than borrowing this one.
- There is also no registered path that scores a forecast. The funnel judges position rules, so even a fully built version would have to be restated as a gated position before any criterion here applies.

## Data

- Owned: both legs at 1-minute grain, ES and CL, 2010-06-06 → 2026-07-28. This is one of the few candidates where the archive genuinely covers the instruments the paper studied.
- Owned: session calendars for both roots, so the overnight/day split is definable — though the CL RTH boundary is an inherited convention rather than a measurement.
- Not owned: options or VIX data, so the implied-volatility comparison and any natural way to monetize a variance view are both out of reach.
- Not owned: the economic-policy-uncertainty series their conditioning uses, and no macro calendar to reconstruct anything like it.
- Not built: multi-instrument configs and cross-instrument accounting — the first is a deliberate refusal today, the second is post-M4.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: a cross-asset lead-lag claim measured on fewer than two years of sessions is a coincidence with a p-value attached.
- The traded descendant must clear `min_oos_sharpe_after_costs = 0.3` under `spread_cross`; a variance forecast that cannot be turned into a position that clears a modest bar is not a result this project can use.
- The discriminator that kills it: an ES-only arm, conditioning equity on equity's own overnight variation, must NOT do as well as the CL→ES arm. If the domestic conditioner matches the cross-asset one, the oil part of the claim is dead and the whole thing is Killed.
- `kill_if_dead_at_ticks = 1.0` — basis: a daily-frequency variance gate trades rarely, so if one tick of half-spread kills it the edge was smaller than the book.
- `max_permutation_p = 0.05` — basis: block permutation over sessions is the only control this build has against a lead-lag that is really shared exposure to the same shocks.
- `max_pbo = 0.5` — basis: the sign decomposition multiplies the parameter count, and PBO is what charges for that.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Journal of Forecasting, 2022, and the instruments are ours — WTI futures and the US equity index. That is a better match than most of this batch and should be said before the criticism.
- The claim is about a forecasting statistic, not about money. Forecast-encompassing comparisons are notoriously sensitive to which benchmark model is chosen, and the abstract's framing as the first study of its kind is a publication-incentive marker rather than a strength.
- The paper reports its own in-sample and out-of-sample results; they are not restated here, and none of them passed through a fill model or a cost sweep.
- Crude and equities are both risk assets that respond to the same macro shocks. A cross-asset variance link is close to guaranteed to exist; that it is directional and exploitable is the untested claim, and the ES-only control arm above is registered specifically to attack it.
- Any traded descendant's costs rest on `half_spread_ticks = 1` (D-0120), an assumption that will never be measured for CL in this archive.

## Triage grade

**C.** C stands. The registered claim names two instruments and one config names one, which is a deliberate refusal rather than an oversight. Beyond that it needs a session-scoped variance accumulator and a signed decomposition — two constructs the grammar lacks — and a scoring path for a forecast rather than a position. The cost is a multi-instrument funnel path plus cross-instrument accounting, which is post-M4.
