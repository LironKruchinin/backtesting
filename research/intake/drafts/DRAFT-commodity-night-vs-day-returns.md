---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: commodity-night-vs-day-returns
topic: overnight-intraday
grade: A
hypothesis_family: commodity-session-return-asymmetry
status: draft
created: 2026-08-06
doi: 10.1108/cfri-10-2017-0213
source_api: semanticscholar
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Night-session returns leading the day session in energy and metals

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

Juan Du. *Empirical differences between the overnight and day trading hour returns: Evidence from the Chinese commodity futures*.
China Finance Review International, 2018.
DOI `10.1108/cfri-10-2017-0213`. <https://www.semanticscholar.org/paper/ae3fdd19cd875492621dcd5f2124f264e9150d72>
Retrieved from the semanticscholar API on 2026-08-06.

Working on Chinese commodity futures, the author builds separate day and night market-return series by principal components and fits vector autoregressions to them. A specification allowing positive and negative moves to act differently traces a multi-day lead from night returns into day returns; a symmetric one does better on squared returns at night. The stated practical reading is that carrying a day position through the night is risky.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1108/cfri-10-2017-0213':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Overnight and day blocks draw different participants, and if the night crowd sets prices the day crowd only partly absorbs, the day session inherits a drift. In China's commodity markets the payer is credible and specific: the night session was opened by administrative decision, the participant base is heavily retail, and domestic hedgers with physical exposure often cannot or will not trade it — so they take the reopening price rather than making it, and position limits and daily price bands funnel that flow into predictable windows. The problem is that this payer is a property of that exchange. CME's overnight book in crude and gold is populated by global professionals, not by a cohort locked out of half the clock, so the mechanism's counterparty may simply not exist in the market we replay. That is the strongest single reason to expect no transfer, and it is a thing to test rather than assume.

## Signal in Crucible terms

- `CLZ2024` and `GCZ2024` as separate configs, `timeframes = ["1h"]`, resampled on read from 1-minute bars.
- Night arm — `[indicators.trend] kind = "sma", period = [12, 24, 48]`; `enter_long = "is_overnight and close crosses_above trend"`; `exit_long = "is_rth"`. The exit on `is_rth` is the point: it flattens into the day session and directly tests the carry claim.
- Mirrored: `enter_short = "is_overnight and close crosses_below trend"`; `exit_short = "is_rth"`.
- Carry arm, a second config under the same family: identical entries with `exit_long = "close crosses_below trend"`, so the position is held through the reopen. The difference between the two arms is the whole test.
- Nothing here needs arithmetic between operands, an anchored price or a rolling extreme, which is why this is grade A while most of the batch is not.
- The paper's own construction — a principal-component index across many commodities — is not expressible and is not what this registers; two single-contract arms are a much weaker relative.

## Data

- Owned: CL and GC `ohlcv-1m` 2010-06-06 → 2026-07-28, curated with four-digit keys, resampled on read.
- Owned but load-bearing and weak: `is_rth` / `is_overnight` on CL and GC rest on `rth_open_local` / `rth_close_local`, which on the four commodity tables are a cited convention rather than a measurement — open outcry ended in 2016 and CME publishes no RTH window for these products. This hypothesis lives entirely on that split, so it inherits that assumption whole.
- Not owned: any Chinese futures data, any position or inventory data, and any way to observe the participant mix the mechanism depends on.
- Constraint: one instrument per config, and `combo` refuses two. The paper's cross-commodity index has no expressible analogue here.
- Cost figures rest on `half_spread_ticks = 1` (D-0120); neither CL nor GC has `tbbo` in the archive, so no measured alternative will ever exist.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a claim about a daily session boundary needs a year of boundaries. Not reachable on one contract today.
- `min_oos_trades = 100` — basis: an hourly trend gate confined to one session block fires rarely; below this there is nothing to measure.
- `min_oos_sharpe_after_costs = 0.3` — basis: below the shipped 0.5 deliberately, because a session asymmetry is a conditioner and not a finished idea.
- `kill_if_dead_at_ticks = 1.0` — basis: the flat-into-the-day version pays a full round trip every session, so it is the most cost-exposed construction in this batch.
- The discriminator: the night arm must clear the bar while the carry arm does not, at the same parameters. If holding through the reopen is no worse, the paper's central practical claim has failed here and this is Killed regardless of how either arm looks on its own.
- `max_permutation_p = 0.05` and `require_controls_beaten = true` — basis: a session-gated rule on a trending contract can beat nothing while looking fine, and the matched random-entry median over 16 draws is the control that catches it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Chinese commodity futures, with night trading introduced only in the mid-2010s, hard position limits, daily price bands, and a retail share that has no CME counterpart. The market studied is not one we trade, and the sample does not overlap ours.
- China Finance Review International, 2018. The abstract's own findings are mixed — one specification wins for returns and the other for squared returns — which is a horse race reported as a result.
- The paper reports its own model comparisons; no performance figures are restated here, and none of them were produced under transaction costs or a fill model.
- The CL and GC RTH window is a convention we inherited, not a measurement (CLAUDE.md §9). Any night/day split on these two roots is therefore partly an artifact of a boundary nobody has verified against the tape.
- `half_spread_ticks = 1` is an assumption (D-0120), and the carry-versus-flatten comparison turns directly on the cost of one extra round trip per session, so this is a case where the assumption may decide the verdict.

## Triage grade

**A.** A: session predicates plus a moving average is legal TOML today, and the flatten-versus-carry comparison is two configs under one family. Runnable is not answerable. One CL or GC contract's active life is far short of `min_oos_sessions = 250`, so the machine kills this for sample size until pooling across contracts lands. The RTH convention caveat is a second reason to distrust an early pass.
