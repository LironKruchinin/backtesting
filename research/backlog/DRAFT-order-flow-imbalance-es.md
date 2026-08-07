---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: order-flow-imbalance-es
topic: volume-price
grade: C
hypothesis_family: es-order-flow-imbalance
status: draft
blocked_on: a signed order-flow feature and the loader under it: `tbbo`/`trades` exist for ES ONLY and for one year of sixteen (D-0120), and no curated path reads them
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Signed order flow and price change in ES at one-second resolution

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

Makoto Takahashi. *Returns and Order Flow Imbalances: Intraday Dynamics and Macroeconomic News Effects*.
arXiv q-fin, 2025.
**no DOI** (preprint). <http://arxiv.org/abs/2508.06788v4>
Retrieved from the arxiv API on 2026-08-06.

An econometric study of ES in which signed order flow and price change are modelled jointly at one-second resolution, separately within each 15-minute slice of the day, with identification coming from variance shifts rather than from timing assumptions. Scheduled announcements change the balance — a given quantity of flow moves price more while flow itself becomes less variable — and the impulse responses die out almost immediately.

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

This is the one candidate whose loser is unambiguous and textbook: the trader who must transact now pays impact to the trader who is quoting, and the payment is the spread plus the price move his own order causes. That arrangement is permanent, because immediacy has value and somebody must be compensated for holding the resulting inventory. The problem is not the mechanism, it is the clock. The paper's own result is that the shock decays essentially inside one second, so the money changes hands on a horizon our curated data does not reach — curated bars are one minute, and by then the effect being measured is long gone. Reading a strategy out of this paper is our inference and not the author's; the paper describes price formation and claims no edge. Anyone building on it is asserting they can act inside the window where payment occurs, which is a latency claim rather than a signal claim.

## Signal in Crucible terms

- Instrument ES only, since ES is the only root with any trade-level data at all. The faithful grain is `1s` or finer; curated data is 1-minute only.
- The construction WOULD be: signed trade volume aggregated per interval into an imbalance operand, with a directional rule conditioned on imbalance and on the interval's position in the day.
- Where it breaks, first: there is no signed order-flow feature and no loader under one. `tbbo` and `trades` exist for ES from 2025-07-28 to 2026-07-28 — one year out of sixteen — and no curated path reads either.
- Where it breaks, second: signing trades needs a rule (quote side from `tbbo`, or a Lee-Ready style inference from `trades`), and that rule needs its own availability answer before it can enter any join.
- Where it breaks, third: the announcement conditioning needs a macro release calendar with timestamps. Nothing in the acquisition plan buys one and no milestone consumes one.
- Where it breaks, fourth: the effect's own half-life is inside a second, so even a perfect implementation is betting on execution speed this project has never claimed to have.

## Data

- Owned: ES `ohlcv-1s` raw for 2010-06-06 to 2026-07-28 — bars at one-second grain, which is not signed flow but is the closest thing in the archive.
- Owned: ES `tbbo` and `trades` for 2025-07-28 to 2026-07-28, and `mbo` for a single month. That window cannot grow — the L1 entitlement lapsed (D-0120).
- Not owned: any macro or announcement calendar, so the half of the paper about news effects has no input at all.
- Not owned: signed flow for any other root, now or ever. Six of the seven roots have no trade-level data.
- Curated data is 1-minute only, so nothing below a minute is replayable today regardless of what raw holds.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- No run is authorized under this key until a signed-flow curated path exists. These criteria are written now so they are ready and not invented after the first result.
- The horizon gate, which is the one that decides it: the effect must still be measurable at a horizon at least one second AFTER a plausible fill, not at the instant of the flow. An edge that lives inside our own reaction time is not an edge, and failing this is Kill — basis: the paper's own finding that shocks dissipate within a second.
- `kill_if_dead_at_ticks = 0.5` — basis: at this horizon the entire move is a fraction of a tick, so half a tick is not a sensitivity check, it is the whole question; the usual 1.0 floor would be meaningless here.
- `min_abs_ic = 0.02` at 1-, 5- and 30-second horizons with the bootstrap interval excluding zero at the same horizon — basis: D-0085, and a predictor claim should be settled before any equity curve exists.
- `min_oos_sessions = 250` — basis: the backlog constant, and worth registering here precisely because one year of `tbbo` supplies roughly that and no more, so there is no room for a second sample.
- `max_permutation_p = 0.05` — basis: at one-second grain the bar count is enormous and almost anything reaches conventional significance on sample size alone; the block-permutation null is what corrects for that.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper makes no trading claim. It is a structural description of price formation, and any strategy read out of it is entirely our construction — the author is not on the hook for it.
- It is an arXiv preprint, currently at version four, with no peer review.
- The instrument matches, which is rare in this batch: ES is a market we hold and trade. That is the strongest single point in its favour.
- One year of `tbbo` out of sixteen is not a sample that supports a second, independent test. Whatever is found on it cannot be checked anywhere else, now or later, because the entitlement lapsed.
- Everywhere else in this backlog `half_spread_ticks = 1` is an assumption we tolerate; here it is the entire subject matter, and ES `tbbo` is the one place in the archive where the assumption could actually be replaced by a measurement.
- Identification through heteroskedasticity is a statistically respectable device that nonetheless rests on assumptions about which variances shift and when. Those assumptions are not testable from the data alone.

## Triage grade

**C.** The gap is not one indicator, it is a chain: a `trades`/`tbbo` transcode path, a trade-signing rule with an explicit availability answer, a curated schema and column for signed flow, an engine operand, and a macro calendar for the announcement half. Underneath all of that sits a window of one year in sixteen that will never grow, and an effect whose own half-life is shorter than any horizon this build can act on. It belongs in the backlog as a record, not a queue position.
