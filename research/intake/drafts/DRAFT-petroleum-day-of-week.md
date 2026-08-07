---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: petroleum-day-of-week
topic: calendar-effects
grade: B
hypothesis_family: energy-day-of-week
status: draft
blocked_on: calendar predicates (day-of-week) — no operand names a weekday
created: 2026-08-06
doi: 10.1080/23322039.2023.2213876
source_api: semanticscholar
harvested_from: semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Weekday effects in crude and refined-product futures

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

Andrew C. Meek, S. Hoelscher. *Day-of-the-week effect: Petroleum and petroleum products*.
Cogent Economics &amp; Finance, 2023.
DOI `10.1080/23322039.2023.2213876`. <https://www.semanticscholar.org/paper/ff62005c3924e515a30247b149027987627e22a2>
Retrieved from the semanticscholar API on 2026-08-06.

The authors check whether returns on a set of energy futures differ systematically by weekday, working from the liquid front contracts rather than cash quotes. They report that the answer is not the same commodity to commodity, and suggest the pattern might inform when a trade is placed.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1080/23322039.2023.2213876':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Energy has a weekly clock that most markets do not: inventory statistics land on a fixed weekday, rig-count and positioning reports land on another, and physical scheduling runs on business weeks. If a weekday effect is real, it should be a premium paid around a scheduled disclosure — the holder who must carry exposure through an announcement they cannot forecast, or the participant who pays to be flat over a weekend during which pipelines, refineries and geopolitics keep moving while the exchange does not. That is a plausible payer and a familiar one. But we do not own an announcement calendar, so we cannot identify the event, only the weekday it usually falls on, and a weekday with no event attached is a coincidence with a name. The honest position is that the losing side is guessed rather than named, and the draft should not pretend otherwise.

## Signal in Crucible terms

- Instrument: one CME WTI contract per config, four-digit key (`CLZ2024`). Of the five markets the paper covers, this is the only one in the archive.
- Timeframe: `1d`, aggregated on read from stored 1-minute bars on the exchange's own sessions (D-0077); a trading-day bar opens at 17:00 CT the evening before, which matters because a weekday label must attach to the session, not to the UTC date.
- Feature: the weekday of the trading day. No operand names one — the session readings that exist tell you where a bar sits inside its day, never which day it is.
- Rule as it would be written: `enter_long: weekday == wednesday`, `exit_long: weekday == thursday`. Five weekdays is five hypotheses unless exactly one is frozen in advance, which this registration does.
- Fallback available today: none that preserves the idea. There is no price-based proxy for a weekday.

## Data

- Owned: CL `ohlcv-1m`, 2010-06-06 to 2026-07-28, curated and replayable, with a modelled energy session table including the pre-2015 early close (D-0089).
- Not owned: Brent, gasoline, heating oil and natural gas. Four of the paper's five markets are absent, so the cross-commodity comparison that is its actual finding cannot be run.
- Not owned: the inventory and rig-count release calendar. The event that would make a weekday effect mechanical is invisible to us, which is the difference between testing a mechanism and testing a coincidence.
- The paper's series stitches the nearest two contracts; our roll tables are built by volume or by calendar rule (D-0090) and neither is that construction, so a difference in results has a construction explanation available before it has an economic one.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Exactly one weekday is pre-registered before the run. Every other weekday is a separate declared trial charged to this hypothesis family, so the deflation reads five and not one.
- `min_oos_sessions = 500` — basis: a weekday appears on roughly a fifth of sessions, so this floor delivers about a hundred instances of the registered day, which is the least that supports a claim about one day out of five.
- `min_oos_trades = 100` — basis: one round-trip per occurrence of the registered weekday, so the trade count and the instance count are the same number and both must clear.
- `min_oos_sharpe_after_costs = 0.50` — basis: this rule is in and out weekly, so turnover is roughly fifty round-trips a year and costs are material rather than negligible.
- `kill_if_dead_at_ticks = 0.5` — basis: deliberately tighter than elsewhere in this batch. A one-day holding period in crude cannot afford a full tick, so if the edge does not survive half of the assumed spread it is not a strategy. This is the gate expected to fire.
- `max_permutation_p = 0.05`, block length declared and swept — basis: a weekday rule is a partition of the sample, and partitions of a random series produce differences by construction.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Four of the five markets studied are not in this archive, and the paper's finding is precisely that the effect differs across them. What we could run is one commodity from a five-commodity comparison, which is not the same claim.
- The venue is an open-access economics and finance title operating on an author-pays model. That is not disqualifying, but it belongs in the record beside a result that is a scan over weekdays.
- The paper's own framing, as far as the metadata shows, is exploratory — it reports that the pattern varies and suggests it may help timing. There is no trading rule to replicate, so any rule we write is our invention attributed to their observation.
- Sample overlap is substantial: the study is recent and its window almost certainly sits inside ours, so a confirmation would be largely the same data rather than a fresh sample.
- Every cost figure rests on `half_spread_ticks = 1`, an assumption and not a measurement, and CL has no L1 data in this archive and cannot acquire any (D-0120). A weekly-turnover rule is exactly the sort whose verdict that assumption can flip.

## Triage grade

**B.** The missing piece is a weekday operand, and unlike the intraday clock readings that already landed it must be derived from the trading day rather than from the bar's position inside it. The D-0071 pattern applies directly: compute the day key once in the CLI, hand the same slice to every consumer, and never let two components attribute a bar to different days. Cheap, but it also needs pooling to reach a usable sample.
