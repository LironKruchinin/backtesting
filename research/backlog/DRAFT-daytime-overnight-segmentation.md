---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: daytime-overnight-segmentation
topic: overnight-intraday
grade: A
hypothesis_family: equity-index-session-segmentation
status: draft
created: 2026-08-06
doi: 10.5430/afr.v1n2p13
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Regular hours and the overnight book as two markets, not one

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

Sandip Dutta, Subhash C Sharma. *Daytime vs. Overnight Trading in Equity Index Futures Markets*.
Accounting and Finance Research, 2012.
DOI `10.5430/afr.v1n2p13`. <https://doi.org/10.5430/afr.v1n2p13>
Retrieved from the crossref API on 2026-08-06.

The authors look at how price information moves between the E-mini's regular-hours block and the hours around it, a window they say prior work had left alone. Their reading is that a nominally continuous 24-hour contract is not one market: the regular-hours block behaves as a market in its own right, while the surrounding hours act as a conduit carrying information from one day's regular session into the next and reducing mispricing along the way.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.5430/afr.v1n2p13':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Segmentation means the two blocks are not one continuous auction. The overnight book is thin, and the people in it are hedging a foreign exposure or reacting to news at an hour when the US day crowd is absent. If information arriving overnight is only partly absorbed by the time regular hours begin, the remainder is worked out in the day session, and a rule trading one block on the other's information is claiming that correction. The losing side is nameable and plausible: mandate-bound desks that trade only US hours and therefore transact at whatever the reopening price is, plus anyone pushed into a thin overnight book by a margin call or a foreign-book hedge rather than by a view. What weakens it is that both groups know this, ES overnight volume has grown for fifteen years, and a segmentation finding is a claim about information flow rather than about money.

## Signal in Crucible terms

- `ESM2024` (and siblings), `timeframes = ["15m"]`, one raw contract per config as the grammar requires.
- Overnight arm — `[indicators.z] kind = "zscore", period = [20, 40, 60], source = "close"`; `enter_long = "is_overnight and z crosses_below -2.0"`; `exit_long = "is_rth or z crosses_above 0.0"`.
- Mirrored: `enter_short = "is_overnight and z crosses_above 2.0"`; `exit_short = "is_rth or z crosses_below 0.0"`.
- Regular-hours arm, a second config under the same `hypothesis_family`: the identical rules with `is_rth` and `is_overnight` swapped. The comparison between the two arms is the test; either arm alone is just a reversion backtest.
- Float thresholds are enumerated (`[1.5, 2.0, 2.5]`), never `{ start, end, step }` — a stepped float axis has a floating-point-dependent length (D-0060), which would make the trial count itself unstable.
- Both arms are charged to `equity-index-session-segmentation`, so the second arm costs trials against the same family rather than hiding in a new one.

## Data

- Owned: ES `ohlcv-1m` 2010-06-06 → 2026-07-28, curated, resampled to 15m on read on the exchange's own sessions.
- Owned: the CME equity-index calendar with eras (D-0086), which is what makes `is_rth` / `is_overnight` / `is_post_rth` mean anything. The 15:15–15:30 CT halt exists in one era and not the next, so a segmentation test straddling 2021-06-28 is comparing two different exchanges.
- Constraint, not a gap: `combo` and `walk-forward` replay raw contracts only, so the longest sample this config can see is one contract's active life — roughly 60 sessions for ES.
- Not owned: cash-index intraday data, order-flow data outside ES's single year of `trades`/`tbbo`, and any measure of who is actually in the overnight book. The segmentation mechanism names participants we cannot observe.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: one calendar year of sessions is the minimum on which a claim about a recurring daily structure means anything. This is not satisfiable on one contract today and the run will be Killed for it, correctly.
- `min_oos_trades = 150` — basis: a session-gated reversion rule fires a few times a session at most, and below this the sample is the noise.
- `min_oos_sharpe_after_costs = 0.3` — basis: set below the shipped 0.5 because the object here is a session asymmetry, not a finished strategy.
- `kill_if_dead_at_ticks = 1.0` — basis: overnight ES is genuinely wider than the day session, so if the overnight arm dies at one tick of half-spread it was never real; the 2-tick column of the mandatory sweep is the honest one to read for that arm.
- The discriminator that can kill it: the overnight arm must clear `min_oos_sharpe_after_costs` while the regular-hours arm does not. If both arms clear it, the finding is a generic reversion effect and the segmentation claim is dead; if neither does, likewise.
- `max_permutation_p = 0.05` and `require_controls_beaten = true` — basis: block permutation is what separates a session effect from an ordinary draw, and the matched random-entry control is the only benchmark this build gives an empirical p-value against.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Accounting and Finance Research, 2012 — an open-access venue with light refereeing, and a paper indexed by only two of our sources. Treat the finding as a hypothesis, not as established.
- Their sample predates almost everything that matters to the overnight book: the growth of overnight ES volume, the 2021 removal of the afternoon halt, and the migration of European and Asian risk transfer into CME hours.
- The paper studies information transmission, not trading returns. It reports no strategy and no costs, so there is nothing of theirs to restate and nothing that would survive a cost sweep by construction.
- The archive replays E-mini futures, which is genuinely the instrument they studied — a rare clean match in this batch, and the main reason the grade is A rather than lower.
- Every cost figure rests on `half_spread_ticks = 1` (D-0120), applied uniformly across sessions. That is most wrong exactly where this hypothesis lives: the overnight book is wider than the day book, and a uniform assumption flatters the overnight arm.
- Splitting the sample by era to respect D-0086 shrinks an already short sample, and picking the era where the result appears would be the selection this directory exists to prevent.

## Triage grade

**A.** A: the construction above is legal TOML today — session predicates, a trailing z-score, and two configs under one family. Grade A means runnable, not answerable. One raw ES contract is roughly 60 sessions, so `min_oos_sessions = 250` is unreachable and the machine will kill this run for sample size, correctly, until registry pooling across contracts lands. Do not lower the sessions floor to make it pass.
