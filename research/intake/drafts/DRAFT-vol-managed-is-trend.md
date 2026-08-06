---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: vol-managed-is-trend
topic: vol-managed-exposure
grade: B
hypothesis_family: futures-vol-managed-trend-attribution
status: draft
blocked_on: continuous position sizing, needed to build the volatility-managed arm the comparison is against; the trend arm alone is already expressible
created: 2026-08-06
doi: 10.3905/jpm.2025.1.764
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Is volatility-managed performance just trend following?

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

Benjamin T. Hood, Cameron Raughtigan. *Volatility Targeting Is Trendy: How Trend Following Explains Alpha in Volatility-Managed Strategies*.
The Journal of Portfolio Management, 2025.
DOI `10.3905/jpm.2025.1.764`. <https://doi.org/10.3905/jpm.2025.1.764>
Retrieved from the crossref API on 2026-08-06.

From the title alone — the index returned no abstract for this record — the argument appears to be that the excess performance credited to volatility-managed strategies is largely explained by an implicit trend-following exposure rather than by anything new. Everything in this restatement is inferred from a title and could be wrong in its particulars.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.3905/jpm.2025.1.764':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Scaling by trailing volatility produces a position whose profile over time resembles a slow trend rule: smaller after volatile declines, larger through calm advances. If that overlap is most of the story, then the performance credited to volatility management is an exposure already named, already sold and already crowded, and counting it twice is the error being pointed at. This candidate names no new payer, and that is its content rather than a defect — it is a decomposition claim, and its value here is deflationary. The trend arm's own payer is at least arguable: participants who cut risk after losses for institutional rather than price reasons, and who keep doing it because the trigger is a drawdown limit. But that is the same payer the trend arm already claims, so nothing additional is being collected. Note again that no abstract was available, so even this reading rests on a title.

## Signal in Crucible terms

- The trend arm is fully expressible today: `ESM2024` at `1d`, `[indicators.trend] kind = "sma", period = { start = 20, end = 200, step = 20 }`, `enter_long = "close crosses_above trend"`, `exit_long = "close crosses_below trend"`, mirrored short.
- The volatility-managed arm is not expressible for the same reason as the previous candidate: continuous sizing, which the grammar has no way to name.
- The test is the comparison, so having one arm and not the other is having nothing. That is why the trend arm's expressibility does not promote this file.
- What it would cost: the same sizing seam, plus a way to compare two strategies' return series directly — the funnel scores combos independently and does not correlate one against another today.
- Their attribution is a factor regression. Even with both arms, reproducing that specific method needs factor return series this project does not hold, so the expressible version is a correlation between the two arms' own series, which is weaker.

## Data

- Owned: all seven roots at 1-minute grain, resampled to 1d — enough for the trend arm on every root the paper's class list would cover.
- Not owned: factor return series of any kind (momentum, value, time-series-momentum benchmarks). Their attribution regressions have no data path here.
- Not built: continuous sizing, so the arm being explained cannot be constructed.
- Not available: the paper's abstract. The index returned nothing, so the claim above is a title reading and the grade should be treated as provisional on that.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The decomposition IS the criterion: if the managed arm's per-fold return series correlates above 0.7 with the trend arm's over identical windows, the paper's claim stands here and the managed arm gets no separate trial budget under any future family.
- Symmetrically, if the correlation is below 0.3 and the managed arm still clears `min_oos_sharpe_after_costs = 0.3`, the claim has failed on our data and that is a result worth having.
- `min_oos_sessions = 500` pooled — basis: a correlation between two strategy return series is only stable over a long window, and two folds of agreement mean nothing.
- `kill_if_dead_at_ticks = 1.0` on the trend arm — basis: if the trend arm itself does not survive one tick, there is no exposure to attribute anything to and the comparison is vacuous.
- `max_permutation_p = 0.05` on the trend arm — basis: the comparison presumes the trend arm has something in it, and the permutation null is what establishes that before anything is attributed.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The index returned NO ABSTRACT for this record. Everything written above is inferred from the title. Nobody has read the paper, and this is the one file in the batch where even the claim restatement is a guess.
- The Journal of Portfolio Management, 2025 — a practitioner venue, and a very recent paper with no replication history at all.
- There are no reported figures available to restate, which at least removes the temptation.
- The underlying observation is close to mechanical and has been made informally for years; a decomposition result is worth more as a caution against double-counting exposures than as an idea to trade.
- Even fully built, our version substitutes a correlation between two of our own arms for the paper's factor attribution. That is a weaker instrument and the difference should not be glossed over in any later report.
- Costs on the trend arm rest on `half_spread_ticks = 1` (D-0120).

## Triage grade

**B.** B stands. The missing piece is continuous position sizing: without it the volatility-managed arm cannot be built, and a comparison with one arm is not a comparison. The cost is the same sizing seam as the asset-class-asymmetry candidate, plus a funnel path that correlates two combos' return series rather than scoring each alone. Also note the record carried no abstract, so the grade rests on a title.
