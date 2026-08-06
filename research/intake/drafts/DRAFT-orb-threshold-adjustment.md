---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: orb-threshold-adjustment
topic: breakout-range-expansion
grade: B
hypothesis_family: orb-threshold-and-exit-design
status: draft
blocked_on: a session-anchored rolling high/low (opening-range) indicator, plus a declarable exit bracket — the engine has brackets (D-0069) but the combo grammar cannot name one
created: 2026-08-06
doi: 10.1109/cifer.2019.8759112
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Opening-range breakout with an adaptive trigger distance

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

Jia-Hao Syu, Mu‐En Wu, Shin-Huah Lee, Jan-Ming Ho. *Modified ORB Strategies with Threshold Adjusting on Taiwan Futures Market*.
venue unrecorded, 2019.
DOI `10.1109/cifer.2019.8759112`. <https://openalex.org/W2961397055>
Retrieved from the openalex API on 2026-08-06.

The authors take the ordinary opening-range breakout, observe that it has stopped paying on most contracts, and propose letting the trigger distance move with how persistent recent price action has been instead of sitting at a fixed offset from the opening range. Tested on Taiwanese index futures across 2008 through 2012, they report the adjusted version dominating the plain one, with the widest margin in the falling market.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1109/cifer.2019.8759112':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Overnight, information accumulates while the market is thin or shut, and the opening does not fully clear it; the first minutes of a session therefore carry a backlog of orders whose direction can persist past the range that forms while they trade. That is the standard story, and the honest thing to notice is that this paper's own premise is that the plain version stopped paying — which means whoever was on the losing side either stopped showing up or started being compensated properly. An adaptive trigger distance does not restore a payer; it changes how far price must travel before the trade is taken, which is a volatility-scaling argument wearing a trend-continuation costume. If a counterparty can be named at all it is the liquidity provider quoting through the open into a genuine information arrival, and that participant has spent fifteen years learning to widen instead of standing still. Treat the loser here as unidentified.

## Signal in Crucible terms

- Instrument `ESH2024` or `NQH2024`, timeframe `5m`; one raw contract per config, since the grid commands refuse continuous aliases.
- The construction WOULD be: an opening-range slot holding the high and low of the first N minutes of the session, then `enter_long: close crosses_above orb_high_plus_threshold`, with the threshold widening when a persistence measure is elevated.
- Where it breaks, first: there is no session-anchored rolling max or min. `bollinger(period, k).upper` is a mean plus k standard deviations, which moves when dispersion moves rather than when an extreme prints, and it does not reset at the session boundary.
- Where it breaks, second: `orb_high + threshold` is arithmetic between operands, and the grammar compares operands but never combines them.
- Where it breaks, third: the paper's exits are protective levels. The engine has brackets (D-0069) but no config can declare one, so the exit would have to be faked with a comparison rule and would not be the paper's exit.
- The session-time half IS expressible today — `minutes_since_rth_open < 30` gates the formation window — which is why the missing piece is the level, not the clock.

## Data

- Owned: curated 1-minute bars for ES and NQ from 2010-06-06 to 2026-07-28, with `5m` and `15m` aggregated on read from the exchange's own sessions (D-0077).
- Owned: the equity-index calendar with session eras (D-0086), so an opening range anchored on the real open rather than on a UTC clock is available.
- Not owned: Taiwan Futures Exchange data of any kind. The paper's market is not one we can touch, so this is an out-of-sample transplant to a different exchange, not a replication.
- Not owned: any measured spread for NQ. `half_spread_ticks = 1` is an assumption (D-0120) and will stay one — the L1 entitlement lapsed and only ES has `tbbo` at all.
- RTY would be the natural third index but its archive begins in 2017, because the contract did not list on CME before then.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: one opening-range trade per session at most, so 250 sessions is the smallest window in which a per-session rule has a countable sample; a single contract cannot reach it and will be killed for sample adequacy, correctly.
- `min_oos_sharpe_after_costs = 0.5` — basis: the same floor every other file in this backlog registers, chosen once so results are comparable across the queue rather than tuned per idea.
- `kill_if_dead_at_ticks = 1.0` — basis: the opening window carries the widest spread of the day, so a breakout entry pays the worst price of any entry in the session; an edge that does not survive one tick there is not an edge.
- The adaptive arm must beat the fixed-threshold arm on `min_oos_sharpe_after_costs` by a registered margin, else Kill — basis: the paper's entire claim is the delta between the two, so a run where both do equally well has refuted the hypothesis even if one of them made money.
- `require_controls_beaten = true` and `max_permutation_p = 0.05` — basis: breakout rules on an index that rose over the sample are exactly the family where a random-entry draw and a block-permuted null earn their keep.
- `require_plateau = true` over the formation-window length and the threshold multiplier — basis: a result at one window length with nothing on either side of it is a spike, and the whole grid exists so a plateau has room to appear.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The market is Taiwan, not CME. Different tick size, different participants, different session structure, different retail share. Anything we run is a transplant and should be reported as one.
- The sample is 2008 through 2012 — five years straddling the financial crisis, which is the single most favourable window in modern history for any strategy that gets long volatility.
- The venue is an IEEE conference and the index returned no journal name for it. The paper reports its own performance figures; they are not restated here, except to note that the headline is a ratio against its own unmodified baseline rather than against any benchmark.
- The paper closes by proposing neural networks as the next step. That is a tell: a rule whose author is already reaching for more capacity is a rule whose author knows the current form is thin.
- Cost realism is where this dies if it dies. Every number we produce rests on `half_spread_ticks = 1`, and an opening-window strategy trades precisely when that assumption is least defensible.

## Triage grade

**B.** The gap is code and the data is owned. What is missing is a session-anchored rolling high/low that resets at the open, plus a way to name an exit bracket the engine already implements. The indicator is a bounded job — a monotonic deque with O(1) update so grid throughput survives — but the bracket half touches the config schema, the canonical hash form and grid expansion, which makes it a decision-log change rather than an afternoon.
