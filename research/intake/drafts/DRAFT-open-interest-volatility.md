---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: open-interest-volatility
topic: volume-price
grade: B
hypothesis_family: commodity-open-interest-volatility
status: draft
blocked_on: an open-interest series in curated data and an operand for it — the raw `statistics` schema is archived for all seven roots and nothing transcodes it
created: 2026-08-06
doi: 10.1016/j.kjss.2016.01.004
source_api: crossref
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Flow versus the outstanding position base as variance predictors

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

Tanachote Boonvorachote, Kritika Lakmas. *Price volatility, trading volume, and market depth in Asian commodity futures exchanges*.
Kasetsart Journal of Social Sciences, 2016.
DOI `10.1016/j.kjss.2016.01.004`. <https://doi.org/10.1016/j.kjss.2016.01.004>
Retrieved from the crossref API on 2026-08-06.

On Asian commodity futures exchanges, the authors split flow and the outstanding position base into an anticipated part and a surprise part, then regress three separate definitions of variance on both with a GARCH specification. Surprise flow raises variance in every definition; the outstanding position base works the other way, which they read as depth absorbing orders rather than amplifying them.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.kjss.2016.01.004':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Nobody is on the losing side here, and saying so plainly is the finding. This is a variance model: it relates flow and the stock of outstanding positions to how much prices move, never to which way they move. The intuition is standard and probably right — flow proxies for information arriving, while a large base of open positions means there is inventory capacity to absorb that flow, so the same order does less damage into a deeper market. Both halves are statements about magnitude. Any tradeable version has to be a conditioning layer bolted onto some other directional bet, or a volatility-timing overlay, and this build owns neither options nor continuous position sizing, so there is no instrument through which anyone could pay us. Until a directional hypothesis is attached, the honest statement is that the counterparty cannot be named, because no bet has been placed.

## Signal in Crucible terms

- Instruments `CLM2024` and `GCZ2024`, timeframe `1h` or `1d`; the paper's variance definitions are close-to-close, close-to-open and open-to-close, all of which are daily objects.
- The construction WOULD be: an `open_interest` operand from the archived `statistics` schema, normalised as `zscore(period, open_interest)`, used as a regime gate on some directional rule — for example `enter_long: oi_z < 0 and <directional condition>`.
- Where it breaks, first: open interest is not in curated data at all. The raw `statistics` schema is archived and verified for all seven roots, and nothing transcodes it, so there is no column and no operand.
- Where it breaks, second: the anticipated-versus-surprise split is a regression residual. That is arithmetic between operands, which the grammar cannot do, and a full-sample fit unless the expectation model is estimated causally.
- Where it breaks, third: there is no directional rule to gate. The paper supplies a variance result, so this file cannot be run at all until somebody registers the bet the gate would apply to.
- The volume half IS expressible today via `zscore(period, volume)`, so a partial version exists — and running the partial version under this file's name would be testing half a hypothesis and reporting it as the whole.

## Data

- Owned but not curated: the `statistics` schema, archived for all seven roots across 2010-06-06 to 2026-07-28. The bytes are bought and `verify`-clean; the gap is a transcode path, not an acquisition.
- Owned: curated 1-minute CL and GC bars with daily aggregation on read, so the variance definitions the paper uses are all constructible from what we have.
- Not owned: any Asian exchange data. TOCOM, the Thai exchange and the Chinese exchanges are entirely absent, so the paper's markets cannot be touched.
- Not owned: market depth. The archive holds OHLCV and, for ES only and for one year, `tbbo`; the paper's depth variable has no counterpart here.
- An availability question must be answered before anything is integrated: open interest is published with a lag, preliminary then revised, and Sec 2.1 requires an explicit `avail_ts` rule before it enters any join.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Nothing runs until a directional hypothesis is registered separately for the gate to apply to — basis: a conditioning variable with no bet under it cannot be scored, and this file must not be allowed to launder a variance result into a return claim.
- S0 gate: the open-interest-conditioned forward return must clear `min_abs_ic = 0.02` with its bootstrap interval excluding zero at the same horizon — basis: D-0085, and if a variance variable has no directional information the correct outcome is to stop before any equity curve is built.
- The gated arm must beat the ungated arm on `min_oos_sharpe_after_costs`, else Kill — basis: a gate that does not improve the bet it gates has not earned the transcode path, the operand and the availability rule it cost.
- `min_oos_sessions = 250` and `kill_if_dead_at_ticks = 1.0` — basis: the session floor is the backlog constant; the cost floor applies because a regime gate changes when you trade, and changing when you trade changes what you pay.
- `max_pbo = 0.5` and `require_controls_beaten = true` — basis: adding a second conditioning axis multiplies the grid, and the random-entry control is what detects a gate that merely reduced trade count.
- A registered refusal: if the open-interest series turns out to be published too late to be used at the bar it describes, the hypothesis is Killed on availability rather than reformulated to use a lagged value — basis: reformulating after seeing the timestamp is exactly the post-hoc move criteria are written to prevent.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The markets are Asian commodity exchanges with day sessions, local participants and different tick structures. We trade CME on a nearly 23-hour schedule. The transplant is severe and there is no overlap in venue.
- Kasetsart Journal of Social Sciences is a regional outlet and the paper follows a 1993 methodology closely; that is a replication in a new market rather than new evidence about how futures work.
- The result is a variance regression. It reports no trading result and makes no trading claim, so every directional reading of it is our inference and the burden is entirely ours.
- Open interest is a stock and volume is a flow, and their measurement conventions differ across exchanges. A finding about one exchange's open-interest reporting may not transfer even to a market that looks similar.
- The cost assumption is not load-bearing here because there is no trade yet — but it becomes load-bearing the moment a directional rule is attached, and CL and GC will never have a measured spread (D-0120).

## Triage grade

**B.** B, and only just. The bytes are owned — `statistics` is archived for all seven roots — so the gap is code rather than acquisition. But the code is more than a column: a transcode path, a curated schema, a grammar operand, and an explicit availability rule for a series that is published preliminary and then revised, which is a Sec 2.1 design question that must be settled before the first join. Add that the paper supplies no directional bet, and this cannot be scheduled until something else is.
