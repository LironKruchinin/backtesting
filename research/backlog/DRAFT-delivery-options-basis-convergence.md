---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: delivery-options-basis-convergence
topic: futures-basis-cash-arbitrage
grade: C
hypothesis_family: delivery-option-convergence
status: draft
blocked_on: a cash market price for the deliverable, and a delivery-option valuation
created: 2026-08-07
doi: 10.1002/fut.10028
source_api: crossref
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Why a futures contract need not converge cleanly, and what the seller's options are worth

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

Jana Hranaiova, William G. Tomek. *Role of delivery options in basis convergence*.
Journal of Futures Markets, 2002.
DOI `10.1002/fut.10028`. <https://doi.org/10.1002/fut.10028>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors estimate the value of the timing and
location choices a short holds in a grain futures contract for each delivery month
over nearly a decade, relate those values to how much the basis varies at the start
of the delivery month, find they help explain that variability but improve forecasts
of convergence only marginally, and note that thin cash-market trading limits the
precision of their estimates.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.10028':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A futures contract is not a promise to deliver a thing; it is a promise to deliver
one of a set of things, somewhere in a set of places, on one of a set of days, and
the short picks. Every one of those choices is an option the short owns and the
long has sold, and the price of the contract reflects it. Who pays: the long, in
the form of a basis that does not converge to zero. This is the most mechanically
certain candidate in wave 2 — the options exist by contract specification, not by
behaviour — and it is also one of the least tradeable, because the paper's own
finding is that knowing the option values barely improves a forecast.

## Signal in Crucible terms

- Not expressible. There is no cash price for any deliverable in this archive, and
  no valuation machinery for the embedded options.
- The relevance to owned roots is real but indirect: **ZN has exactly this structure**
  and it is the sharpest case in the CME complex, because the short delivers a
  cheapest-to-deliver note chosen from a basket and the conversion factors make the
  choice non-trivial. Wave 2's other rates candidates all sit on top of that fact
  without naming it.
- What *is* observable from owned data is the behaviour of a contract's price as it
  approaches expiry, which is wave 1's `samuelson-maturity-effect` — a related but
  distinct hypothesis about volatility rather than about level.

## Data

- Owned: ZN and CL `ohlcv-1m` across every contract, so the futures side of any
  convergence question is complete.
- Not owned: cash Treasury prices, conversion factors, the deliverable basket, or
  repo rates — everything the cheapest-to-deliver calculation needs.
- Not owned: grain futures or cash grain, the paper's own market.
- Not built: any option valuation anywhere in the workspace.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now, and it is a caution rather than a hypothesis: **a futures price
  near expiry is not a clean proxy for the underlying**, and any candidate that
  reasons about convergence, roll cost or basis on ZN is implicitly assuming
  something about a delivery basket it cannot see. That belongs on the record before
  someone builds a roll-cost model on top of it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their market is CBOT grain in the 1990s with a thin cash market — a limitation
  the paper states about itself. Ours is CME rates and energy at one-minute grain,
  three decades later.
- The paper's own reported results are theirs and are not restated here, and its
  forecasting improvement is described in its own abstract as occasional and small.
- The mechanism transfers to ZN cleanly in principle and the data to test it does
  not exist here at all.
- This candidate is included for its structural content rather than as work to
  schedule; that is a judgement, and a reader who disagrees should delete it rather
  than let it sit as a permanently unreachable item.

## Triage grade

**C.** C, and the missing pieces are **a cash price for the deliverable** and **a
delivery-option valuation**, neither owned nor planned. Its practical value is the
note it leaves for the rates candidates: ZN's convergence is governed by a delivery
basket this archive cannot see.
