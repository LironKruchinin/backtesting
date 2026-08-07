---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: hedging-pressure-risk-premium
topic: open-interest-positioning
grade: C
hypothesis_family: commodity-hedging-pressure-premium
status: draft
blocked_on: trader-position data (Commitments of Traders), and multi-instrument configs for the equity link
created: 2026-08-07
doi: 10.1002/fut.22122
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Hedging pressure and the equity link as determinants of the commodity risk premium

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

Mohammad Isleimeyyeh. *The role of financial investors in determining the commodity futures risk premium*.
Journal of Futures Markets, 2020.
DOI `10.1002/fut.22122`. <https://doi.org/10.1002/fut.22122>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author derives and tests a model in which the
futures risk premium depends on hedging pressure, on stock-market returns and on the
correlation between the commodity and equities, and reports that the equity channel
became more important for energy after 2008 while hedging pressure remains a strong
explanatory variable across specifications.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.22122':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Hedging pressure is the oldest explanation of a commodity risk premium and the one
with the clearest payer: a producer who must sell forward is buying insurance and
the speculator who takes the other side is selling it. If the balance of hedgers is
short, the speculator is long and is paid for it; if hedgers are net long, the sign
flips. That is a mechanism with a named counterparty and a reason the counterparty
persists — the producer's motive is operational, not directional. The financialization
half is a claim that a second, newer payer appeared after 2008, which is more
fragile: it says the premium now partly compensates equity-market risk, which makes
it a beta rather than a premium.

## Signal in Crucible terms

- Not expressible. Hedging pressure is measured from position reports and no
  operand names one. Open interest is not in the curated schema either — the raw
  `statistics` schema is archived for all seven roots and nothing transcodes it,
  which is the same gap wave 1's `open-interest-volatility` records.
- The equity-correlation channel needs two instruments.
- Note that the two gaps are of different sizes: the open-interest series is
  **already in the raw archive** and needs a transcode path and an operand, whereas
  position reports are an outside acquisition. A file that lumped them together
  would hide a cheap unlock behind an expensive one.

## Data

- Owned: CL and GC `ohlcv-1m` across 247 and 221 contracts; ES for the equity leg.
- Owned but unused: the `statistics` schema for all seven roots, 2010-06-06 →
  2026-07-29, sitting in `raw/` with no curated reader. Open interest lives there.
- Not owned: Commitments of Traders or any equivalent position breakdown. Weekly,
  free, published with a lag — and the lag is the availability rule (§2.1): a
  Tuesday position reported on Friday afternoon is not knowable on Tuesday, and a
  study that treats it as such has lookahead in it.
- Not owned: multi-instrument configs for the correlation channel.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now, and it is the most important line in this file: **the position
  report's availability rule is decided before the data is used, not after.** The
  reports are published with a multi-day lag and are revised; a hedging-pressure
  signal stamped with the survey date rather than the publication date is §2.1
  lookahead of exactly the kind this project exists to prevent, and it is the
  standard way this literature is implemented.
- The pre-2008 and post-2008 split is declared as two windows, not searched for.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their instruments include energy and their data includes position reports;
  ours is price and volume only. The paper's main regressor is unavailable to us.
- The paper's own reported estimates are theirs and are not restated here.
- The financialization literature is large, contested and mostly identified off a
  single structural break, which is a weak identification strategy — one break in
  one decade, shared with a financial crisis that changed everything else too.
- Hedging pressure as a *time-series* signal on one root is far weaker than the
  cross-sectional version the literature usually runs, and it is the only version
  this build could ever express.

## Triage grade

**C.** C, and the missing pieces are **trader-position data with a stated availability
rule** and **multi-instrument configs**. It also names a cheaper, separate unlock
worth tracking on its own: **open interest is already in the raw archive** and needs
only a transcode path and an operand — the same piece wave 1's
`open-interest-volatility` is waiting on, which makes two candidates behind one small
build.
