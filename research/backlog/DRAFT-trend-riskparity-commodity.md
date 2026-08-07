---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: trend-riskparity-commodity
topic: trend-horizon
grade: B
hypothesis_family: commodity-trend-risk-parity-sizing
status: draft
blocked_on: continuous position sizing — the rule grammar emits boolean entries only, so an inverse-volatility weight cannot be expressed
created: 2026-08-06
doi: 10.1016/j.irfa.2013.10.001
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Trend rule plus inverse-volatility sizing in commodity futures

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

Andrew Clare, James Seaton, Peter N. Smith, Stephen Thomas. *Trend following, risk parity and momentum in commodity futures*.
International Review of Financial Analysis, 2014.
DOI `10.1016/j.irfa.2013.10.001`. <https://doi.org/10.1016/j.irfa.2013.10.001>
Retrieved from the crossref API on 2026-08-06.

No abstract was indexed for this record — neither Crossref nor the other sources returned one — so everything below is inferred from the title and venue alone and nothing else has been read. The title states the ingredients: a trend rule, risk-parity (inverse-volatility) weighting, and momentum, applied to commodity futures. The presumed claim is that combining the trend rule with volatility-scaled sizing improves portfolio outcomes over either alone. That inference could be wrong in detail and should be verified before anything is registered.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.irfa.2013.10.001':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The two halves of this idea have very different standing, and conflating them is how papers like this get overrated. The trend half has the usual candidate payers in commodities: hedgers — producers and consumers — laying off price risk and paying a premium to whoever carries it, plus slow diffusion of supply-and-demand news through a chain of physical participants. Those are nameable and at least plausible, though we hold no position data to confirm anyone is actually paying. The risk-parity half has no payer at all, and that is the point worth being blunt about: inverse-volatility sizing is a variance-management overlay. It reallocates the same edge across time and across markets; it does not extract anything from a counterparty. So a paper reporting that trend-plus-sizing beats trend alone is reporting a change in the shape of the outcome, not the discovery of a new source of it. Any registration built on this should test the trend rule and treat the sizing as a separate, weaker question.

## Signal in Crucible terms

- What it would be: `CLM2024` and `GCM2024`, `1h` or `1d`, a moving-average crossover for direction, with contract count scaled by the inverse of `[indicators.rv] kind = "stdev"`, `period = 60`, `source = "return"`.
- Where it breaks: `[rules]` emits boolean `enter_long` / `enter_short` intents and position size is a fixed contract count. There is no operand, no field and no rule shape that carries a weight, so the inverse-volatility term has nowhere to go.
- The closest expressible degradation is a boolean volatility filter — `enter_long = "fast crosses_above slow and rv < 0.008"` — which trades or does not trade rather than trading more or less. That is a different hypothesis and must be registered as one, not passed off as this paper's.
- The trend half alone is fully expressible today and is worth registering on its own terms, with the sizing claim explicitly excluded from what the result speaks to.
- Also not expressible: the cross-sectional momentum ranking the title's third word implies, which needs multi-instrument configs `combo` refuses.

## Data

- CL and GC hold curated 1-minute bars from 2010-06-06 → 2026-07-28, resampled on read to `1h` and `1d`.
- Both roots carry commodity session calendars with documented era caveats (D-0089): CL and GC take a 16:15 CT close before 2015-09-21, and six pre-holiday early closes are knowingly unmodelled, which surface as missing bars rather than as a calendar error.
- Missing: the abstract itself, so we cannot check what sample, era or contract set the paper used, nor whether it reported anything after costs.
- Missing: any sizing seam. This is a code gap, not a data gap, and it is the graded one.
- `half_spread_ticks = 1` is an assumption for both CL and GC and always will be — no `tbbo` exists outside ES (D-0120).

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a commodity trend claim over a multi-decade sample cannot be answered on one contract quarter; one pooled trading year is the floor.
- `min_oos_trades = 150` — basis: a crossover on `1h` commodity bars trades often enough to reach this, and below it the cost sweep cannot distinguish the four tick levels from each other.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills; the sizing half that would flatter this number is not being tested.
- `kill_if_dead_at_ticks = 1.0` — basis: CL and GC ticks are large in dollar terms but the strategy is directional and crosses the spread on every turn; if one tick kills it there is nothing to size.
- `max_permutation_p = 0.05` — basis: commodity trend over 2010–2026 includes 2014–2016 oil and 2020, both of which produce enormous directional runs that a permutation null will price correctly and a raw statistic will not.
- `require_controls_beaten = true` — basis: gold rose substantially over most windows in our archive, so buy-and-hold is a genuine and often winning competitor for a long-biased rule on GC.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The index carried no abstract at all for this record. Nobody has read the paper, and unlike the rest of this batch we do not even have the authors' own summary — the claim above is reverse-engineered from six words of title. That is thin, and the draft says so rather than pretending otherwise.
- International Review of Financial Analysis, 2014. A mid-tier finance journal in the peak years of commodity-factor publication, which is precisely the corpus that later failed to replicate.
- We cannot check era, sample, contract universe, or whether costs were modelled, because we have no abstract. Any of those could invalidate the idea outright.
- Their commodity universe is presumably broad; we hold exactly two commodity roots, CL and GC. A result built on breadth does not transfer to two markets.
- The sizing half, which is half the title, is the half we cannot test at all. A registration that tested only the trend rule and reported a verdict would be answering a different question than the paper asked — and that is the honest reason this sits at B rather than A.

## Triage grade

**B.** The missing piece is continuous position sizing: `[rules]` emits boolean entries and the engine takes a fixed contract count, so an inverse-volatility weight has nowhere to live. Closing it costs a sizing expression in the rule grammar, a target-position path that accepts a computed quantity, and a decision about how a fractional weight rounds against whole-contract accounting — plus integer-accounting review, since §2.3 forbids the obvious float route.
