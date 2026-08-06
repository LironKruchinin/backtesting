---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: metals-session-efficiency
topic: intraday-seasonality
grade: A
hypothesis_family: gc-session-window-conditioning
status: draft
created: 2026-08-06
doi: 10.1016/j.jcomm.2018.05.001
source_api: crossref
harvested_from: crossref, openalex, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Session-window conditioning on gold, Asian hours against New York

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

Kentaro Iwatsubo, Clinton Watkins, Tao Xu. *Intraday seasonality in efficiency, liquidity, volatility and volume: Platinum and gold futures in Tokyo and New York*.
Journal of Commodity Markets, 2018.
DOI `10.1016/j.jcomm.2018.05.001`. <https://doi.org/10.1016/j.jcomm.2018.05.001>
Retrieved from the crossref API on 2026-08-06.

Comparing gold and platinum across Tokyo, London and New York trading hours, the authors infer that flow in the Tokyo venue is largely uninformed for both metals, while New York shows evidence of both kinds of participant with the information-motivated sort dominating its day session. They conclude that the same commodity behaves differently depending on which venue and which hours you look at.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.jcomm.2018.05.001':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The losing side here is nameable and unusually durable: the participant whose trading hours are set by his time zone rather than by the market's. Japanese retail accounts and Japanese corporate hedgers transact during their own working day, and they will keep doing it, because a time zone is not a decision anybody gets to revisit. If information-motivated flow prefers the deeper hours, then moves in the Asian window are disproportionately inventory and scheduled hedging and should be more likely to give back, while moves in the New York window are disproportionately information and should persist. That is a clean story with a payer who cannot leave. The caveat is severe and belongs beside it: this archive sees gold on Globex, so we observe the Asian hours but not the Asian venue, and the local exchange flow the paper actually measured reaches us only through arbitrage, diluted by whatever the arbitrageurs kept for themselves.

## Signal in Crucible terms

- Instrument `GCZ2024`, timeframe `15m`, aggregated on read from curated 1-minute bars against the metals calendar's own sessions.
- Asian-hours fade arm: `enter_short: minutes_since_open >= 120 and minutes_since_open < 420 and close crosses_above bollinger_20.upper`, `exit_short: close crosses_below bollinger_20.mid or minutes_since_open >= 420`, with the symmetric long side off `bollinger_20.lower`.
- New York continuation arm, a second config under the same family: `enter_long: is_rth and close crosses_above bollinger_20.upper`, `exit_long: close crosses_below bollinger_20.mid or not is_rth`.
- The comparison between the two arms IS the hypothesis. Fade in Asia, follow in New York; if both pay or neither does, session membership is not the mechanism.
- Fidelity caveat that does not block a grade A run: `minutes_since_open` is anchored on the CME metals session, which opens 17:00 CT. Tokyo and New York observe daylight saving on different dates, so a fixed minute offset drifts by an hour twice a year and the Asian window is approximate at those boundaries. The report must state this rather than let it be discovered mid-run.
- There is no way to measure informed versus uninformed flow here — no trade signing, no depth. What is testable is the price consequence the paper's inference predicts, not the inference itself.

## Data

- Owned: curated 1-minute GC bars from 2010-06-06 to 2026-07-28, with `15m` aggregated on read.
- Owned: a metals calendar (D-0089), including its documented caveat that a 45-minute close discrepancy exists before 2015-09-21 and that some pre-holiday early closes are knowingly unmodelled.
- Not owned: platinum, and not owned: any TOCOM data. The paper's cross-venue comparison cannot be reproduced at all; only the hours-of-day half is reachable.
- Not owned: any measured spread for GC, ever — no `tbbo` for six of seven roots (D-0120). One GC tick is $10, and the Asian window is the thinnest part of the metals day.
- Sample ceiling: one raw contract per config, so a single GC config sees a fraction of the sixteen years the archive holds.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- S0 first: `min_abs_ic = 0.02` at 15-, 30- and 60-minute horizons computed separately inside the Asian and New York windows, with the bootstrap interval excluding zero at the same horizon — basis: D-0085, both halves required, magnitude alone is free on a large sample.
- The discrimination gate, which can kill this even if it makes money: the Asian-window fade must beat the New York-window fade by a registered margin, and the sign of the effect must differ between windows. Same behaviour in both windows means session membership is not the mechanism and this hypothesis is dead regardless of the equity curve.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: a few window-conditioned trades per session, so 250 sessions is the minimum for a countable sample; one GC contract reaches neither and will be killed for sample adequacy.
- `min_oos_sharpe_after_costs = 0.5` — basis: the backlog's fixed floor, applied unchanged so this file is not scored on an easier scale.
- `kill_if_dead_at_ticks = 1.0` — basis: the whole strategy trades in the thinnest hours of a $10-tick contract, which is the worst possible place for a flat one-tick spread assumption; if anything in this batch dies on the cost sweep it is this.
- `require_controls_beaten = true` and `max_permutation_p = 0.05` — basis: a rule confined to a fixed block of hours is easy to match with random entries in the same block, which is exactly what the control draws.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The Journal of Commodity Markets is a real refereed outlet and this is a careful microstructure paper. It is also not a strategy paper, so every tradeable reading of it is our construction.
- The paper's core comparison is across venues — Tokyo versus New York exchanges. We hold neither Tokyo venue, so the reachable test is about hours of day on Globex, which is a weaker claim than the paper's.
- Platinum is absent from the archive entirely, and platinum is the metal where the paper's Tokyo venue actually matters. We can only run the leg the paper uses as its contrast case.
- The daylight-saving mismatch is real and unmodelled: Tokyo does not observe it, the US does, and Europe changes on different dates, so any fixed minute offset misidentifies the window twice a year for several weeks.
- The cost assumption is load-bearing. `half_spread_ticks = 1` on GC in Asian hours is an assumption that will never be replaced by a measurement (D-0120), and the fade leg is precisely the trade that pays it most often.
- The paper reports its own statistical figures; none are restated here and none forecasts anything about this engine's output.

## Triage grade

**A.** `minutes_since_open`, `is_rth`, `bollinger` and the completed bar's price fields are all in the grammar, so this runs against curated GC with no new Rust. But runnable is not answerable: one raw contract is a short window, no sample-adequacy floor worth registering is reachable at that length, and today's run is guaranteed to be killed for sample size, correctly, by the machine, until registry pooling across contracts lands. Treat the output as triage plus a daylight-saving fidelity check, not as a verdict.
