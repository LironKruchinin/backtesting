---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: currency-basis-skewness
topic: futures-basis-cash-arbitrage
grade: C
hypothesis_family: fx-basis-distribution-predictor
status: draft
blocked_on: a spot FX series alongside the futures leg, and a distributional feature over it
created: 2026-08-07
doi: 10.1002/fut.21991
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The shape of the currency basis distribution as a predictor

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

Xue Jiang, Liyan Han, Libo Yin. *Can skewness of the futures‐spot basis predict currency spot returns?*.
Journal of Futures Markets, 2019.
DOI `10.1002/fut.21991`. <https://doi.org/10.1002/fut.21991>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors relate the skewness of the futures-minus-
spot basis to subsequent currency spot returns, report a negative association with
in-sample and out-of-sample forecasting content, find the relationship stable over
time and free of structural breaks, and argue it adds to what the basis level alone
provides.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.21991':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The basis in a currency is the interest differential, so the *level* of it is the
carry — and this paper's claim is that the level is not all the information in it.
An asymmetric distribution of the basis says the pressure to hold one side is
episodic rather than steady, which is a statement about who is being squeezed and
how often. Who pays: the hedger who needs the forward leg at the moment the basis
is dislocated, which in currencies is a well-documented seasonal and quarter-end
phenomenon. The claim of no structural breaks over the sample is the part to
distrust most, because the currency basis did break — the post-2008 covered
interest parity deviations are one of the most-documented dislocations in modern
finance.

## Signal in Crucible terms

- Not expressible. The basis needs a spot leg; the archive has the futures leg only.
- Skewness is not in the indicator set. `zscore` and `stdev` are the two
  distributional statistics available (D-0080) and both are second moment or below;
  a third-moment statistic is a new `IndicatorKind` with its own warmup and its own
  hand-computed fixture test.
- Note that the skewness gap is small and self-contained — one trailing statistic
  over a declared source — while the spot gap is an acquisition. Two blockers of very
  different sizes, and only the small one is a build.
- As with the carry candidate in this wave, the FX futures curve embeds the
  differential across maturities, so a two-maturity reader would give a
  *futures-implied* basis without any spot data. That would not be this paper's
  object, and the difference should be recorded rather than elided.

## Data

- Owned: 6E `ohlcv-1m`, 149 curated contracts, 2010-06-06 → 2026-07-28.
- Not owned: spot EUR/USD, or forward points, from any source in the plan.
- Not owned: any currency other than the euro, and the paper works on a panel.
- Not built: a third-moment trailing indicator, and no milestone names one.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now: if a skewness indicator is ever added, it is a trailing window
  like everything else in `indicators::rolling` and must have no full-sample variant
  (D-0080). A skewness computed over the whole series and then used as a signal is
  the same lookahead as a full-sample z-score, and it is the form this literature
  usually takes.
- The structural-break claim is registered as something to test rather than to
  accept: the sample is split at 2008 and the two halves are compared, declared in
  advance.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their data is spot and futures currency prices across a panel; ours is one euro
  futures contract at a time.
- The paper's own reported forecasting results are theirs and are not restated here.
- Out-of-sample forecasting claims in currency prediction have a poor replication
  record, and beating a random walk is the standard bar precisely because it is
  rarely cleared.
- The stability claim sits awkwardly beside the documented post-2008 covered
  interest parity dislocations, and a reader should check which sample window the
  paper's stability test covers before taking it at face value.

## Triage grade

**C.** C, with two blockers of very different cost: **a spot FX series** (an acquisition
nobody has planned) and **a third-moment trailing indicator** (a small, well-defined
build). Recording them separately matters because the small one is reusable — a
skewness slot would serve the carry-crash candidate's diagnostic too.
