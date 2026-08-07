---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: high-low-spread-estimator
topic: execution-cost-slippage
grade: B
hypothesis_family: cost-input-half-spread-measurement
status: draft
blocked_on: a low-frequency spread estimator over curated bars, and a place to put its output
created: 2026-08-07
doi: 10.46503/vutl1758
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Estimating the bid–ask spread from daily high, low and close prices

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

J. Saleemi. *An estimation of cost-based market liquidity from daily high, low and close prices*.
Finance, Markets and Valuation, 2020.
DOI `10.46503/vutl1758`. <https://doi.org/10.46503/vutl1758>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author proposes a spread proxy built from daily
high, low and close prices, compares it with the Roll, Corwin–Schultz and CHL
low-frequency proxies over a large dataset, and reports that unlike some of them it
returns a positive, defined estimate on the whole sample.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.46503/vutl1758':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This candidate is not a trading rule and must not be read as one. It is a
**measurement** aimed at the single assumption every other candidate in this
directory rests on. CLAUDE.md §9 records that the half spread behind every cost
number is a convention wearing a measurement's field name: the L1 entitlement
windows were allowed to lapse, so the archive holds `tbbo` for `ES.FUT` and one
year of it, and NQ, RTY, CL, GC, ZN and 6E have no quote data at all and cannot
acquire any (D-0120). Every config in this repository therefore declares
`half_spread_ticks = 1` because someone had to write something. A low-frequency
estimator changes that: it reads high, low and close — three fields the archive has
for 863 curated contracts over sixteen years — and returns a number with a
derivation instead of a number with a shrug. Nobody is on the losing side, because
nobody is being traded against; the thing being improved is the denominator under
every other verdict.

## Signal in Crucible terms

- Not a signal. No entry rule, no exit rule, no position. It produces one number
  per contract per window, and that number is an input to the cost sweep rather than
  an output of a run.
- The estimator itself is arithmetic over `high`, `low` and `close` — which is
  precisely what the combo grammar cannot do, because it compares operands and never
  combines them. So it cannot live in a config even in principle; it belongs in
  `crucible-data` beside `qa`, which is already the module that reads the archive
  back and reports on it.
- Its output would be reported per root and per era, not as one constant: a spread
  estimate that does not vary across sixteen years is an estimate nobody should
  believe.

## Data

- Owned, completely, and that is the whole argument for this candidate: `high`,
  `low` and `close` for every curated contract of all seven roots, 2010-06-06 →
  2026-07-28. The estimator's entire input is already on disk and already verified.
- Owned for validation, narrowly: `tbbo` and `trades` for `ES.FUT`, 2025-07-28 →
  2026-07-28. One root, one year of sixteen — enough to check the estimator against
  a measured spread on the one root where a measured spread exists, and not enough
  to calibrate it.
- Not owned and not acquirable: L1 for CL, GC, ZN, 6E, NQ, RTY. D-0120 states this
  plainly. That is the reason the estimator is worth building rather than a
  convenience.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- This candidate does not enter the funnel and has no verdict, so its criteria are
  a different kind of thing and are marked as such rather than dressed up as kill
  criteria for a strategy.
- The acceptance test is a **two-sided control**, in the shape CLAUDE.md §7 asks
  for. Positive: on `ES.FUT` over the year where `tbbo` exists, the estimate must be
  compared against the measured effective spread, and the comparison must be
  reported whichever way it comes out. Negative: run it on a synthetic series with a
  known spread planted in, and watch the estimator recover it — an estimator nobody
  has seen recover a known answer is decoration.
- Third case, because two disagreeing numbers only say that they disagree: if the
  estimate and the measurement differ on ES, the run that makes them agree — same
  bars, same window, estimator applied to the quote midpoint rather than the trade
  price — is what names the cause.
- **The direction of the error must not be assumed.** D-0120 deliberately refuses to
  assert whether `half_spread_ticks = 1` is optimistic or pessimistic, and an
  estimator that arrives with a prior about its own sign has reintroduced the
  convention it was built to remove.
- Nothing about this changes a single existing result. Re-costing past runs with a
  new spread would rewrite numbers that were reported under a stated assumption; the
  estimate is reported beside the assumption, and which one a config uses is a
  separate decision with its own log entry.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper's sample is equities and its comparisons are against other equity
  spread proxies. Futures spreads are a different object: a one-tick market in ES is
  not a penny-wide market in a share, and the estimator's behaviour on a lattice
  this coarse is unknown.
- That coarseness is not a small caveat. D-0099 records what happened to the last
  statistic in this repository built from an order statistic of a
  lattice-valued variable: the spike sigma can only ever return an integer number of
  ticks, and there are forty-three distinct values in the entire archive. Any
  high–low estimator inherits the same lattice and the same failure mode, and
  checking for it is part of the acceptance test rather than a follow-up.
- The paper's own reported comparisons are theirs and are not restated here.
- The venue is a small journal and the proxy is one of many in a crowded family. The
  family matters more than the paper: Roll, Corwin–Schultz and this one are all
  candidates, and the right deliverable is probably a comparison rather than an
  adoption.

## Triage grade

**B.** B, and the missing piece is **a low-frequency spread estimator over curated bars,
plus somewhere for its output to live** — a `qa`-style report, not a config field.
The data is entirely owned, so the gap is code. It is listed here even though it is
not a strategy because it is the cheapest thing in either wave that improves *every*
other candidate's verdict: the cost sweep is mandatory (§2.4) and its centre is
currently an assumption on six of seven roots.
