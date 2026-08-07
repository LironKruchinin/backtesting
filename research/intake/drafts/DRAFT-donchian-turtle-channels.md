---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: donchian-turtle-channels
topic: breakout-range-expansion
grade: B
hypothesis_family: commodity-donchian-channel-breakout
status: draft
blocked_on: a rolling max/min (Donchian channel) indicator — `bollinger` is a volatility band around a mean, which is a different object and moves for a different reason
created: 2026-08-06
doi: 10.58886/jfi.v6i1.2421
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Donchian channel breakout on commodity futures

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

David Rayome, Abhijit Jain. *Do Turtles Have Fat Tails? Donchian Channels and Turtle Trading: The Case of Soybeans*.
Journal of Finance Issues, 2008.
DOI `10.58886/jfi.v6i1.2421`. <https://openalex.org/W4320077369>
Retrieved from the openalex API on 2026-08-06.

A simulation of the published Turtle rules — enter on a new channel extreme, leave on the opposite channel, size and stop by a volatility unit — run on soybean futures from 1980 into 2007. The authors conclude the system has merit, that the capital-preservation stop rules contribute more to the outcome than the entry does, and that the distribution of trade outcomes is heavy-tailed rather than normal.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.58886/jfi.v6i1.2421':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Trend following on commodities has the one genuinely nameable counterparty in this business: the commercial hedger. A producer who is structurally short and a consumer who is structurally long are transferring price risk as a cost of operating, not as a view, and they keep paying because the alternative is carrying an exposure their business does not want. A speculator on the other side collects that transfer, and it arrives as persistence rather than as a fee. The second candidate loser is the discretionary trader whose disposition bias makes him cut winners early and hold losers, which is the exact mirror of a channel rule. Both stories are real and neither is news: managed futures has been running precisely this since the 1980s, and the paper's window closes in 2007 — before the crowding, the fee compression, and the long stretch of flat trend performance that followed it.

## Signal in Crucible terms

- Instrument `CLM2024` and `GCZ2024`, timeframe `1d` (a trading-day bar opening 17:00 CT the evening before, D-0077) or `1h` for a faster variant.
- The construction WOULD be: `enter_long: close > donchian_high(20)` and `exit_long: close < donchian_low(10)`, with the symmetric short, plus a stop placed a volatility unit away from the fill.
- Where it breaks, first: there is no rolling max or min. `bollinger(20, 2).upper` is not a substitute — it is a mean plus a dispersion term, so it rises when the market gets noisy without any extreme having printed, and it falls back toward the mean while price sits at a high.
- Where it breaks, second: the volatility-unit stop is a bracket, and the grammar cannot declare one even though the engine implements them.
- Where it breaks, third: Turtle sizing is continuous — contracts per unit of volatility — and this build trades a fixed contract count. A test without the sizing rule is a test of the entry, and the paper says the entry is the least important part.
- The honest consequence: a stripped version we could run today deliberately removes the ingredient the paper names as the important one.

## Data

- Owned: curated 1-minute CL and GC bars from 2010-06-06 to 2026-07-28, with daily bars aggregated on read against the exchange's own sessions.
- Owned: energy and metals calendars (D-0089), including their documented 45-minute era caveat before 2015-09-21 — which matters for a daily bar because a mis-set close moves the bucket boundary.
- Not owned: soybeans, or any CBOT agricultural contract. The paper's instrument is absent from this archive and no acquisition plan includes it.
- Not owned: open interest in curated form, and no measured spread for either CL or GC — `half_spread_ticks = 1` is an assumption that will never be replaced for these roots (D-0120).
- Contract length is the binding constraint: a config replays one raw contract, and CL lists monthly rather than quarterly, so a single CL config sees a shorter liquid window than the ES case does.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` and `min_oos_trades = 100` — basis: a 20-day channel on daily bars fires rarely, so the trade floor is set lower than the intraday files here while the session floor is not; both are unreachable from one contract and the run will be killed for sample adequacy.
- `min_oos_sharpe_after_costs = 0.5` — basis: the backlog's standard floor, held constant so this idea is not graded on a softer scale than its neighbours.
- `kill_if_dead_at_ticks = 1.0` — basis: a slow channel system trades seldom and holds long, so costs are the one thing it should shrug off; if it dies at one tick the problem is not costs, it is that there was nothing there.
- `require_controls_beaten = true`, and specifically buy-and-hold — basis: a long-biased breakout rule on a commodity in a rising decade will look excellent against nothing at all, and the control is what separates the rule from the drift.
- `max_permutation_p = 0.05` and `require_plateau = true` over both channel lengths — basis: the Turtle parameters are the most published, most re-fitted numbers in retail trading, so a result that appears only at 20/10 and nowhere adjacent is fitted memory, not structure.
- A registered refusal: if the stripped version fails, the file may NOT conclude that the missing stop or sizing rule would have rescued it — basis: that is a post-hoc rescue, and the whole point of writing criteria first is to forbid it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The instrument does not overlap and neither does the era. Soybeans 1980 to 2007 against CL and GC 2010 to 2026 shares no contract and no year.
- The Journal of Finance Issues is a minor regional outlet, and the paper is a simulation of a widely published rule set rather than new evidence about a market.
- The headline equity figure is reported by the paper under what it calls a best-case scenario, which is a selection across scenarios; that framing alone would disqualify the number even if we intended to use it, and we do not.
- The Turtle rules have been public since 1983. An edge that survived forty years of publication either has a structural payer behind it or is being measured on the sample where it was found; the hedging-pressure story is the only version that would explain survival.
- Fill realism in 1980s soybean pits is not comparable to anything this engine models, so the paper's cost treatment cannot be checked against ours even in principle.

## Triage grade

**B.** The missing piece is a rolling max/min indicator; `bollinger` is a band around a mean and answers a different question, so substituting it would test something else under this file's name. The cost is one new `IndicatorKind` with an O(1) update (a monotonic deque, since recomputing the window per bar would kill grid throughput), plus its warmup declaration. The bracket and the volatility sizing are separate, larger gaps, and both belong to the entry the paper calls secondary.
