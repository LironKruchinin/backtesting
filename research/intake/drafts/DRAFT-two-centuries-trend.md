---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: two-centuries-trend
topic: trend-horizon
grade: C
hypothesis_family: futures-multi-month-trend-persistence
status: draft
blocked_on: a continuous-series consumer for the GRID commands — excluded by design (README Sec 2.2); one contract's life cannot hold a multi-month lookback
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Multi-month trend premium on a two-century sample

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

Y. Lempérière, C. Deremble, P. Seager, M. Potters, J. P. Bouchaud. *Two centuries of trend following*.
arXiv q-fin, 2014.
**no DOI** (preprint). <http://arxiv.org/abs/1404.3274v1>
Retrieved from the arxiv API on 2026-08-06.

The paper reports that a slow trend rule earned excess performance in commodities, currencies, equity indices and bonds over an extremely long history, using exchange futures from 1960 and reconstructed spot series stretching back to 1800, with the result surviving adjustment for the general upward drift of those markets. It also reports that the signal saturates for large readings, and — notably — that the slow end has held up while the fast end has faded.

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

Nobody has ever produced a payer for the trend premium that survives scrutiny, and this paper does not either. The two standing candidates are under-reaction (information diffuses slowly, so the late adjuster sells you the move cheaply and buys it back dear) and hedging pressure (commercial producers and consumers pay a premium to lay off price risk, and the speculator collects it). Both are plausible; neither is observable in this archive, because we hold no trader-position data and no fundamental series. The paper's own saturation finding hints at a third party — fundamental traders who ignore weak moves and lean on strong ones — which would make the trend follower's counterparty a value trader who is right eventually and wrong meanwhile. So the honest statement is: the losing side cannot be named from anything we can measure. That is not fatal to running the idea, but it is exactly the condition under which a two-century backtest should be read as a description of history rather than as an entitlement.

## Signal in Crucible terms

- What it would be: a stitched series — `ES.v.0`, `CL.v.0` — at `1d`, with `[indicators.fast] kind = "sma"`, `period = [20, 40, 60]` against `[indicators.slow] kind = "sma"`, `period = [120, 200, 250]`, stop-and-reverse.
- Where it breaks, first: `combo` and `walk-forward` refuse a continuous alias by design (README §2.2, D-0076). A grid expands rules it has not seen, and a rule comparing price to an absolute constant is unsafe on a back-adjusted series. This is a deliberate exclusion, not a gap waiting to be closed.
- Where it breaks, second: even if the alias were allowed, a stitched series at a coarse grain is refused because a bucket spanning a roll would mix two `signal_offset` values.
- Where it breaks, third, and this one is arithmetic: one raw contract's life is roughly sixty sessions. A 250-day slow average never finishes warming up. There is no configuration of the existing grammar that runs this idea on a single contract.
- `backtest` does replay a stitched `ES.v.0` today, so an operator can look at this by hand. That is not a registration and produces no verdict, no trial count and no scorecard.

## Data

- Curated 1-minute bars for seven roots from 2010-06-06, resampled to `1d` on read as trading-day bars opening 17:00 CT the evening before.
- Roll tables exist for ES, NQ, RTY and CL under `curated/rolls/`, so the stitched series the idea needs is buildable — it is the grid commands, not the data, that refuse it.
- Sixteen years of history against the paper's two centuries. We hold nothing before 2010-06-06 and never will; the pre-1960 half of their sample is spot reconstructions with no tradeable instrument behind them at all.
- Missing: any cross-asset breadth beyond seven roots, which is what makes their t-statistics large in the first place.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 1000` — basis: a 250-day lookback consumes a full trading year in warmup, so four years of pooled sessions is the minimum that leaves anything left to evaluate on.
- `min_oos_trades = 100` — basis: deliberately low and paired with a high session floor. A slow trend rule trades a handful of times a year; demanding 200 round-trips would force the grid to its fast end and quietly test a different hypothesis.
- `min_oos_sharpe_after_costs = 0.4` — basis: slightly below the house floor, because a slow rule turns over rarely and its honest claim is durability rather than intensity.
- `kill_if_dead_at_ticks = 2.0` — basis: a rule this slow pays cost a few dozen times a year. If two ticks kills it, there was never anything there, and this gate should be easy for a real slow-trend edge to clear.
- `max_pbo = 0.5` — evaluated since D-0109. Basis: the fast/slow grid here is nine points and the paper's own result is that the answer depends on which end you pick, so overfit probability is the number that matters.
- `require_controls_beaten = true` — basis: a long-biased slow trend rule on an equity index over a bull sample is buy-and-hold with extra steps, and the buy-and-hold control is the one that will say so.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Practitioner authors at a systematic fund, on arXiv, 2014, reporting that the strategy their firm sells has worked for two hundred years. That is the highest-prior-of-motivated-reasoning configuration in this batch and should be treated accordingly.
- Pre-1960 spot reconstructions are not instruments anyone could trade. There is no execution assumption behind them, no spread, no roll, and no way to know what a fill would have been. Half the sample's statistical weight comes from data of that kind.
- The same group's 2026 paper — the first candidate in this batch — reports that the fast end has since died. That is a public partial retraction of one half of this result, and it should raise rather than lower the prior that the surviving half is also era-dependent.
- Their claim is a long-horizon one over centuries; ours would be sixteen years of seven CME roots. Even with the unlock, this is a much weaker test than the paper's, not a replication.
- The paper reports its own statistical significance figures; they are not restated here.
- `half_spread_ticks = 1` is an assumption (D-0120), though it bites least on this idea of anything in the batch, because the turnover is low.

## Triage grade

**C.** The missing piece is a continuous-series consumer for the grid commands, and it is missing on purpose: `combo` refuses a stitched alias because a grid expands rules nobody read, and a level comparison on a back-adjusted series is meaningless. Closing it would mean a level-safety analysis of the rule grammar plus a decision on coarse-grain roll buckets. Independently, a multi-month lookback cannot fit inside one contract's sixty-session life at all, so even lifting the refusal leaves the idea unrunnable until pooling lands.
