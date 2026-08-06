---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: orb-index-futures
topic: breakout-range-expansion
grade: B
hypothesis_family: equity-index-opening-range-breakout
status: draft
blocked_on: a session-anchored rolling high/low (opening-range) indicator — README Sec 2.1 names this explicitly as the one thing the session clock does NOT give us
created: 2026-08-06
doi: 10.1109/access.2019.2899177
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Opening-range breakout timed to the cash market's hours

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

Yi-Cheng Tsai, Mu-En Wu, Jia-Hao Syu, Chin-Laung Lei, Chung-Shu Wu, Jan-Ming Ho et al.. *Assessing the Profitability of Timely Opening Range Breakout on Index Futures Markets*.
IEEE Access, 2019.
DOI `10.1109/access.2019.2899177`. <https://doi.org/10.1109/access.2019.2899177>
Retrieved from the crossref API on 2026-08-06.

The paper runs a one-minute opening-range breakout rule on five index futures markets over roughly a decade, with the range window lined up against the hours when the underlying cash index is actually trading rather than against the futures session. It reports positive outcomes in all five markets with small p-values, a best range-length that is shorter in the US than in Asia, and — using one exchange's transaction records — that the breakout direction tends to agree with institutional order flow.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1109/access.2019.2899177':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The standard opening-range story has a nameable payer and it is a good one: the edges of the early range are where resting stop orders accumulate, and a break sweeps them. That flow is price-insensitive and mechanical, so whoever anticipates it collects from whoever placed the stops. The paper's own supporting observation — that breakout direction lines up with institutional flow — points at a second payer, the participant on the other side of a large order being worked through the morning. Both are plausible. What should be weighed against them is that this is the single most mined day-trading rule in existence, with at least three free parameters (range length, entry trigger, exit time) and decades of retail and academic search over them. If the mechanism is stop-sweeping, the people best placed to exploit it are the ones who can see the resting orders, and we cannot. A rule this famous surviving on a US sample from 2003 to 2013 is weak evidence that it survives now.

## Signal in Crucible terms

- What it would be: `ESM2024` at `1m`; latch the high and the low of the first N minutes after `minutes_since_rth_open == 0` for N in {15, 30, 60}; `enter_long` on a close above the latched high, `enter_short` on a close below the latched low; flat by `minutes_to_rth_close <= 5`.
- Where it breaks: there is no rolling max/min and no session-scoped aggregate in the grammar. The session clock gives the timing half — `minutes_since_rth_open` is negative before the bell, so `> 0 and <= 30` names the first half hour exactly — and gives nothing at all about the price level reached during it.
- A `bollinger(period, k).upper` breakout is NOT this hypothesis and must not be substituted. A trailing band is a dispersion statistic over the last N bars wherever they fall; an opening range is a fixed window anchored to a session boundary. Swapping them tests something else and reports it under this paper's name.
- The exit half is fully expressible: `exit_long = "minutes_to_rth_close <= 5"` is exactly the flatten-on-the-bell rule, and `minutes_to_rth_close` rather than `minutes_to_close` is the right operand because the paper's window is the cash market's, not the futures session's (D-0078).
- The paper's cash-hours alignment is the one part this build handles well: `minutes_since_rth_open` and `is_rth` come from the equity-index calendar's RTH fields, which are measured rather than assumed for that table.

## Data

- ES and NQ hold curated 1-minute bars 2010-06-06 → 2026-07-28 at exactly the paper's grain. RTY from 2017 only.
- The equity-index calendar carries session eras (D-0086) with real RTH open and close times; the 15:15–15:30 CT halt exists in era 3a and was removed effective 2021-06-28, which any session-anchored study spanning that date must account for.
- Missing, and this is the graded gap: a session-anchored rolling high/low. The README names it as the one thing the session clock does not give us.
- Missing: three of the paper's five markets. We hold no HSI, no TAIEX, and no cash index series for any of them.
- `half_spread_ticks = 1` is an assumption (D-0120), and ES is the one root with any `tbbo` at all — 2025-07-28 → 2026-07-28 — so this is the single candidate in the batch where the assumption could eventually be checked against a measurement, over one year.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: one entry opportunity per session, so sessions are the sample unit; one pooled trading year is the floor and a single contract cannot reach it.
- `min_oos_trades = 200` — basis: at roughly one to two entries per session this is reachable within the session floor, and below it the range-length grid cannot be distinguished across its three points.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills.
- `kill_if_dead_at_ticks = 1.0` — basis: this is the gate that decides the idea. A breakout entry crosses the spread at the exact moment the book is thinnest and the move is fastest, so a fixed one-tick half-spread is a generous assumption; an edge that cannot carry it did not exist.
- `max_pbo = 0.5` — evaluated since D-0109. Basis: the paper's own headline includes a per-market best range length chosen with the sample in view, which is precisely the selection step PBO exists to price.
- `require_controls_beaten = true` — basis: an index breakout rule that is net long more often than short over a bull sample must be checked against buy-and-hold, and the matched random-entry control against a rule that always enters at the same time of day.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- IEEE Access is a high-volume, author-pays journal with light review, and it is not a finance venue. A strategy-profitability paper published there deserves a low prior before anything else is considered.
- Their sample is 2003 to 2013 and ours begins 2010-06-06, so there are roughly three overlapping years and the rest of their evidence is from a market structure we hold no data for.
- Three of the five markets — including the one carrying their strongest result and the one where the order-flow analysis was done — are markets we do not trade and hold no data for.
- The reported p-values attach to a strategy whose range length was selected per market. A p-value computed after a per-market parameter search is not the p-value it looks like, and this is exactly what `max_pbo` and the permutation null exist to price. The paper reports its own performance figures; they are not restated here.
- `half_spread_ticks = 1` is an assumption (D-0120) and this idea is more exposed to it than most: a stop-triggered breakout entry is the least likely order in the batch to receive anything near the mid.

## Triage grade

**B.** The missing piece is a session-anchored rolling high/low. The session clock gives timing and no price memory, and a Bollinger band is a different object that must not be substituted. Closing it costs a new indicator class whose window resets on a session boundary — which means the reset has to arrive through the same seam the session-clock operands already use, since `crucible-strategies` may not depend on `crucible-data` — plus per-session rather than per-series warmup accounting under §2.6.
