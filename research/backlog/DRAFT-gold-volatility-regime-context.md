---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: gold-volatility-regime-context
topic: realized-vs-implied-volatility
grade: A
hypothesis_family: gc-volatility-state-gate
status: draft
created: 2026-08-07
doi: 10.2139/ssrn.6978741
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — A volatility state as context rather than as a directional trigger, in gold

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

Elena Hysa. *Volatility Forecasting in Gold Futures: HAR Models, Options-Implied Volatility, and the Limits of Directional Inference*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6978741`. <https://doi.org/10.2139/ssrn.6978741>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author extends a heterogeneous autoregressive
volatility model for COMEX gold futures with an options-implied gold volatility
index, a jump decomposition, a regularised regression and a geopolitical-risk
proxy over 2021–2026, reports that the implied-index variants forecast better out
of sample though mostly without conventional significance, and states as the
practitioner conclusion that a better volatility forecast identifies
high-uncertainty regimes useful for sizing and risk control but does not by itself
produce a directional edge in gold.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6978741':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The paper's conclusion is the reason to register it, and it points the opposite way
from most of this directory: knowing how much gold will move tells you nothing about
which way. Volatility is forecastable and direction is not, and the two facts get
conflated whenever a volatility filter is bolted onto a directional rule and the
combination is judged by its returns. Who is on the losing side? In the sizing
version, nobody — it is a risk statement, not a trade. In the directional version,
the strategy is, because it has paid extra turnover for a filter that is not
informative about the thing it is filtering. Registering this as a candidate means
registering a **null as the expectation**, which is unusual here and is exactly what
makes it worth a cheap grade-A slot.

## Signal in Crucible terms

- One instrument, one timeframe, raw contract: a single `GC` contract such as
  `GCZ2024`, `timeframes = ["15m"]`, resampled on read from one-minute bars
  (D-0077).
- `[indicators.vol] kind = "stdev", period = [20, 60, 120], source = "return"` — a
  trailing realized-volatility estimate, which is the legal expression of the
  paper's regime variable. It is a trailing window and therefore point-in-time by
  construction (D-0080); the paper's own estimator is fitted, which is not, and the
  difference is stated rather than glossed.
- `[indicators.trend] kind = "ema", period = [20, 50]` for the directional arm.
- Three arms under one family, and the comparison between them is the experiment:
  ungated (`enter_long = "close crosses_above trend"`), gated to the calm state
  (`... and vol < c`), and gated to the turbulent state (`... and vol > c`). The
  threshold `c` is declared per grain before the run and is **not** a grid axis —
  sweeping it would turn a stated regime claim into a search for the split that
  works.
- The registered expectation is that the two gated arms do **not** beat the ungated
  one on risk-adjusted terms once costs are charged. A gate that only reduces
  exposure will reduce dollars, and reducing dollars is not the claim.
- The implied-volatility half of the paper is not expressible and is not registered:
  there is no options data in the engine and `external/cboe/` does not exist.
- No arithmetic, no anchored price, no rolling extremum, no calendar predicate.

## Data

- Owned: GC `ohlcv-1m`, 221 curated contracts, 2010-06-06 → 2026-07-28, resampled
  on read to `15m` on the exchange's own sessions.
- Owned: the CME metals session calendar with eras (D-0089), which the resampler
  anchors its buckets on.
- Not owned: the gold volatility index the paper's best specifications use, and no
  loader for any options-implied series. `docs/DATA_PLAN.md` records the CBOE CSVs
  as free and manual and the loader as deliberately unbuilt until post-M4 regime
  work.
- Not owned: the geopolitical-risk proxy, shared with this wave's
  `gold-geopolitical-risk-safe-haven`.
- `half_spread_ticks = 1` is an assumption for GC and no measured alternative can
  exist (D-0120). It matters less here than for most candidates, because the
  interesting output is a *comparison between three arms* that all pay the same
  assumed spread.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a regime claim needs enough sessions to visit
  both regimes repeatedly, and a year is the minimum that does. Not reachable on one
  contract.
- `min_oos_trades = 100` per arm — basis: the gated arms trade strictly less than
  the ungated one by construction, so the floor must be met by the *gated* arms or
  the comparison has nothing on one side.
- `min_oos_sharpe_after_costs = 0.3` — basis: below the shipped default deliberately,
  because a gate is a conditioner rather than a finished idea and the useful output
  is the difference between arms rather than the level of any one.
- `kill_if_dead_at_ticks = 1.0` — basis: a 15-minute crossover is low turnover, so
  if it cannot survive one tick the problem is the signal and not the cost model.
- **The registered prediction is the null.** If either gated arm beats the ungated
  arm materially, that is a result *against* the paper's stated conclusion, and the
  first thing to check is whether the threshold was effectively chosen after the
  fact — because there is exactly one number in this config that could have been.
- `require_controls_beaten = true` — basis: a rule that is out of the market in one
  volatility state will show a flattering drawdown for reasons that have nothing to
  do with an edge, and the matched random-entry control is what removes that.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- An SSRN working paper, not refereed, on a five-year sample. Ours is sixteen years,
  so a replication would extend it substantially — and the extension includes 2020,
  which the paper's window excludes and which is the largest volatility event in the
  archive.
- The paper's own reported forecast improvements are theirs and are not restated
  here; several are described in the abstract as lacking conventional significance,
  which is a caveat the paper makes about itself.
- The best-performing specifications all use the implied-volatility index, which is
  the one input we do not have. What this candidate tests is therefore the paper's
  *conclusion* — that a volatility state is context and not a trigger — using a
  weaker volatility estimate than the paper's. A null here is consistent with the
  paper and also consistent with our estimator being too crude, and the two cannot
  be separated without the index.
- Volatility-gated trend following is one of the most heavily explored ideas in the
  practitioner literature and wave 1 already carries four candidates in that
  family. This one is kept separate because it registers a null rather than an
  edge, and because its source is specifically about gold; if Liron judges it too
  close to `conditional-volatility-targeting`, they should share a family key.

## Triage grade

**A.** A. A trailing standard deviation of returns, an EMA and four rules on one raw
contract at a resampled grain — legal TOML today, no new Rust and no new data.
Runnable is not answerable: a single GC contract's active life falls short of
`min_oos_sessions = 250`, and the three-arm comparison needs each arm to reach its
own trade floor, so pooling matters more here than for a single-arm candidate.
