---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: energy-volatility-regime-linkage
topic: volatility-transmission-commodities
grade: C
hypothesis_family: energy-crossmarket-volatility-regime
status: draft
blocked_on: multi-instrument configs, agricultural roots, and a fitted regime-switching indicator
created: 2026-08-07
doi: 10.1002/fut.22477
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Volatility linkages that only appear in the turbulent regime

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

Anthony N. Rezitis, Panagiotis Andrikopoulos, Theodoros Daglis. *Assessing the asymmetric volatility linkages of energy and agricultural commodity futures during low and high volatility regimes*.
Journal of Futures Markets, 2023.
DOI `10.1002/fut.22477`. <https://doi.org/10.1002/fut.22477>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: combining regime-switching regressions with two
multivariate GARCH specifications, the authors examine how energy and agricultural
futures volatilities relate under calm and turbulent states, report stronger
cross-correlations in the turbulent state and two-way volatility transmission
between the two groups there, and conclude that each group can hedge the other.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.22477':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Volatility transmission between energy and agriculture has a physical channel that
most spillover papers lack: fuel is an input to farming and grain is an input to
fuel, so a shock to one is a cost shock to the other. That is a real linkage with a
real payer — the processor whose margin is the spread between them. The regime
result is the familiar and less comfortable half: linkages tighten when they are
least useful. What makes this candidate hard for Crucible is that both halves are
statements about pairs, and the regime variable is a fitted latent state rather than
a trailing window.

## Signal in Crucible terms

- Not expressible. Two markets in one run is refused by design, and one of the two
  groups is not in the archive at all.
- The regime half needs a fitted Markov-switching state. Every indicator in
  `crucible-strategies::indicators` is a trailing window with no latent state, and
  no `IndicatorKind` names a fitted model — the same blocker wave 1 records for
  `commodity-regime-switching-variance` and `gold-regime-switching-volatility`.
- A trailing `stdev(source = "return")` threshold is the available substitute for a
  regime and it is genuinely different: it is a level, not a state with persistence
  and transition probabilities. Registering it under this citation would be
  substituting one object for another.

## Data

- Owned: CL `ohlcv-1m`, 247 contracts. That is one of the two groups.
- Not owned: corn, soybeans, wheat — the agricultural side. This is the second
  wave-2 candidate blocked by a missing instrument rather than missing code, after
  the seasonality entry, and it is the same missing instrument.
- Not built: multi-instrument configs; not built: any fitted-model indicator.
- Three blockers, of three different kinds — a purchase, a design rule and a build —
  which is why this sits in the C column without much prospect of moving.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now: if a regime-switching indicator is ever built, its state must be
  estimated **only from data available at the decision instant** (§2.1). Fitting a
  two-state model on the whole series and then labelling history with it is the
  classic version of the leak `controls::LeakyZScore` exists to demonstrate, and it
  is how this literature is usually implemented. That is the single most important
  thing to carry forward from this candidate.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their sample is energy and agricultural futures at a daily frequency; ours is
  crude at one minute and no agriculture. The pair at the heart of the paper cannot
  be formed.
- The paper's own reported estimates are theirs and are not restated here.
- Regime-conditional correlation results are a large literature with a consistent
  finding — correlations rise in stress — that is close to unfalsifiable, because
  the stress regime is usually identified by the same volatility that raises
  measured correlation.
- The hedging conclusion is a portfolio statement and this build has no
  multi-instrument portfolio accounting (§11, post-M4), so even a full dataset would
  not let the funnel judge it.

## Triage grade

**C.** C, and three missing pieces of different kinds: **agricultural roots** (a purchase),
**multi-instrument configs** (a design rule) and **a fitted regime-switching
indicator** (a build). It is grouped with wave 1's two regime-switching candidates
for the third, which now stands at three candidates behind one indicator.
