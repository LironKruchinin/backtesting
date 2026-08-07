---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: cojumps-rates-precious-metals
topic: jump-detection-discontinuities
grade: C
hypothesis_family: rates-metals-cojump-behaviour
status: draft
blocked_on: multi-instrument configs, and silver; the jump-identification half also needs a bipower estimator
created: 2026-08-07
doi: 10.1016/j.irfa.2022.102078
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Simultaneous discontinuities in Treasuries and precious metals

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

Artur Semeyutin, Gareth Downing. *Co-jumps in the U.S. interest rates and precious metals markets and their implications for investors*.
International Review of Financial Analysis, 2022.
DOI `10.1016/j.irfa.2022.102078`. <https://openalex.org/W4213101141>
Retrieved from the openalex API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: working from one-minute data and a wavelet
decomposition, the authors build realized covariance matrices with and without
discontinuities for US Treasury and precious-metals futures, examine what drives
simultaneous jumps across those markets, and consider what the presence of such
jumps does to correlations, hedge ratios and portfolio choice — including the
observation that a simultaneous demand for safety and for quality is less puzzling
once maximum diversification is the objective.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.irfa.2022.102078':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A cojump is the cleanest evidence that two markets are responding to one thing at
one instant, and gold and Treasuries responding together is the flight-to-quality
story stated at a frequency where it can be checked rather than asserted. The
mechanism has no counterparty in the usual sense — nobody is systematically paying —
which is the honest reason this is a risk-management result and not a strategy.
Its value here is as a **caution about diversification claims**: if the two hedges
jump together, then a portfolio holding both is less diversified exactly when it
needs to be more so, and that is invisible in an unconditional correlation.

## Signal in Crucible terms

- Not expressible. A cojump is a joint statement about two series at one instant,
  and `combo` refuses two instruments.
- Jump identification itself is not expressible in the paper's sense either. Bipower
  variation and the wavelet decomposition are estimators over intraday returns, and
  the grammar has trailing means and deviations only. A z-score of returns is a
  proxy, not the estimator, and calling it one would misstate what was tested.
- Silver is not in the archive, so even the metals half is partial.

## Data

- Owned: ZN and GC `ohlcv-1m` and `ohlcv-1s` over the same sixteen years, which
  is exactly the pair and exactly the grain the paper uses. This is the closest
  wave 2 comes to a paper whose *data* we hold and whose *machinery* we lack.
- Not owned: silver, platinum, palladium; and only one point on the Treasury curve,
  where the paper works across it.
- Not built: any object that reads two instruments in one run, and no realized
  covariance anywhere in the codebase.
- Worth stating plainly, because it changes what to build: for this candidate the
  acquisition is nearly complete and the machinery is entirely absent, which is the
  opposite balance from most of the C column.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable as a strategy, and it should not become one. The paper's output is
  a covariance object and a portfolio conclusion; converting it into an entry rule
  would be inventing a hypothesis.
- Registrable now: if multi-instrument runs ever land, the ZN–GC cojump measurement
  is a cheap first use because both series are owned at one-minute grain over an
  identical window, and it produces a number that qualifies every future
  diversification claim rather than a verdict.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their instruments include ours and extend beyond them; their frequency is ours.
  The sample era overlaps substantially. That is unusual in this harvest and is why
  the candidate is worth keeping despite being unrunnable.
- The paper's own reported estimates are theirs and are not restated here.
- Its conclusions are about portfolio construction across two assets, which this
  build cannot represent at all — there is no multi-instrument portfolio accounting
  and §11 places it post-M4.
- A cojump result is sensitive to the jump test used, and the paper uses one this
  project has no implementation of. A replication with a different detector would be
  a different study.

## Triage grade

**C.** C, and the missing pieces are **multi-instrument configs**, a **jump estimator**
(bipower or equivalent) and **silver**. The first two are machinery over data we
already hold at the right grain, which makes this one of the better arguments for
multi-instrument support: the input is bought, verified and aligned.
