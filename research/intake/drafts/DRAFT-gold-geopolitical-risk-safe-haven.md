---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: gold-geopolitical-risk-safe-haven
topic: gold-safe-haven
grade: C
hypothesis_family: gc-safe-haven-conditional-correlation
status: draft
blocked_on: a geopolitical-risk index series, and multi-instrument configs
created: 2026-08-07
doi: 10.1016/j.resourpol.2020.101872
source_api: semanticscholar
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Gold's correlation with equities conditioned on a geopolitical-risk index

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

Mohamed Bilel Triki, Abderrazek Ben Maatoug. *The GOLD market as a safe haven against the stock market uncertainty: Evidence from geopolitical risk*.
Resources Policy, 2021.
DOI `10.1016/j.resourpol.2020.101872`. <https://doi.org/10.1016/j.resourpol.2020.101872>
Retrieved from the semanticscholar API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors take monthly gold prices, a US equity
index and a published geopolitical-risk index over three and a half decades, fit a
multivariate GARCH and a dynamic copula, and report that gold's co-movement with
equities is lower in calm periods and higher when political tension is extreme,
reading that as support for a hedging role.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.resourpol.2020.101872':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The safe-haven claim is a conditional-correlation claim, not a return claim, and
that distinction is the whole difficulty. Nobody argues gold earns a premium; the
argument is that its correlation with risk assets falls, or inverts, exactly when
that is worth most. Who pays: everyone who wants insurance at the same moment,
which is why the payoff and the crowding arrive together. But a conditional
correlation is not a position, and a config emits positions. Turning this into a
strategy needs a second leg to be hedged, a weight to hedge it with, and a state
variable to switch on — three objects, none of which exists in this build. What
this file registers is therefore the *measurement*, and the honest observation is
that the measurement is the paper's whole contribution too.

## Signal in Crucible terms

- Not expressible, on three counts at once. The claim is about a pair of markets,
  so it needs two instruments; the conditioner is an external index, so it needs a
  data source; and the output is a correlation, so it needs something other than a
  boolean entry rule.
- The frequency is wrong as well: monthly observations against an archive of
  one-minute bars. Resampling to `1d` is supported (D-0077) and monthly is not, and
  a monthly claim tested on daily bars is a different claim.
- `is_rth` and the rest of the session clock have nothing to offer here — the
  conditioning variable is a macro state, not a position in the session.

## Data

- Owned: GC `ohlcv-1m`, 221 curated contracts, 2010-06-06 → 2026-07-28; ES over the
  same span for the equity leg.
- Not owned: the geopolitical-risk index. It is published monthly by its authors
  and is free, so acquiring it is not expensive — but §2.1 makes the first question
  "as known when?", and a monthly index revised after publication has a non-trivial
  answer that would have to be settled before it touched a run.
- Not owned: any way to hold two instruments in one config, or to express a hedge
  ratio.
- Sample mismatch: their span starts in 1985 and ours in 2010, so the events that
  drive their result — the tension episodes of the 1980s and 1990s — are outside
  our archive entirely.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable as a strategy today. A correlation is not a verdict the funnel
  can render, and dressing one up as an entry rule would register something the
  paper did not claim.
- Registrable now is the discipline the eventual test needs: the tension threshold
  is fixed before the run rather than chosen as the quantile that separates the
  data best; the correlation window is declared in sessions; and the comparison is
  two-sided — the calm-period correlation must be reported beside the tense-period
  one, because a number that is only quoted in one state cannot be distinguished
  from a number that is always that size.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Monthly data, an equity index rather than a futures contract, and a span that
  is mostly before this archive begins. Nothing about the sample overlaps ours.
- The paper's own reported estimates are theirs and are not restated here.
- Conditional-correlation studies of gold are an enormous and repetitive
  literature, and the harvest that produced this candidate returned dozens of
  near-identical papers differing mainly in the country and the crisis studied.
  That is a warning about the family, not about this paper: a result that has been
  found by everyone, everywhere, in every window, is either a deep fact or a
  specification that cannot fail.
- Both halves of the safe-haven claim are about *not losing*, and the funnel
  scores strategies on what they make. A candidate whose success looks like a
  smaller drawdown on a leg we do not hold is a poor fit for this machine, and
  saying so is more useful than grading it optimistically.

## Triage grade

**C.** C, and the missing pieces are a **geopolitical-risk index with a stated
availability rule** and **multi-instrument configs**. It sits in the same bucket as
wave 1's `oil-overnight-predicts-equity-vol` and `high-frequency-lead-lag`: the
statistic is defined on a pair, and one config names one instrument. The external
index is cheap; the pair is a design rule.
