---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: oil-inventory-news-across-assets
topic: crude-inventory-storage
grade: C
hypothesis_family: oil-inventory-news-cross-asset
status: draft
blocked_on: a petroleum-inventory release calendar, and multi-instrument configs
created: 2026-08-07
doi: 10.1002/fut.22096
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Oil inventory surprises priced across equities, bonds and the dollar

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

Ron Alquist, Reinhard Ellwanger, Jianjian Jin. *The effect of oil price shocks on asset markets: Evidence from oil inventory news*.
Journal of Futures Markets, 2020.
DOI `10.1002/fut.22096`. <https://doi.org/10.1002/fut.22096>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors separate oil price moves driven by
inventory information from other oil moves, and trace what each does to equity,
bond-futures and exchange-rate returns, reporting that the equity response
changed sign around the 2007–2008 crisis and attributing that to risk premia.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.22096':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

An inventory release is a scheduled instant at which a physically-grounded
number becomes public, and the interesting claim is not that crude reprices —
it is that the *other* markets reprice, in a direction that flipped once. Who
pays: whoever holds the cross-asset exposure through the release without having
priced the surprise. Before the crisis, higher crude read as a cost shock to
equity earnings; after it, higher crude read as evidence of demand, so the same
number bought the opposite trade. That sign flip is the reason to distrust the
whole family as a rule: a mechanism whose sign is set by which macro regime you
are in is a mechanism with a hidden conditioning variable, and nothing in this
archive observes that variable. The honest reading is that this is a *finding
about oil news*, not a strategy, and the file registers it as such.

## Signal in Crucible terms

- Not expressible today, and the gap is not small. The claim relates one
  market's scheduled news to three other markets' returns; a config names one
  instrument, and `combo` refuses two.
- Even the single-market half is out of reach: the release instants are the
  whole signal, and no operand names a date or a scheduled event. The session
  clock (D-0078) answers "how far into the session are we", not "is a report
  due in ten minutes".
- The nearest expressible relative — which is *not* what this file registers —
  is a within-CL rule gated on a clock band chosen to cover the usual release
  minute, tested on CL alone. That is a different hypothesis with a much weaker
  claim, and stating it here is to say plainly that it is not a substitute.

## Data

- Owned: CL, ES, ZN and 6E `ohlcv-1m` from 2010-06-06, curated with four-digit
  contract keys. Every price series the paper's dependent variables would need
  exists in some form.
- Not owned: the inventory releases themselves — dates, times, published levels
  and the consensus they surprised against. `docs/DATA_PLAN.md` acquires no such
  series, and the macro calendar is an M4 static CSV that does not exist.
- Not owned: an availability rule for any of it. §2.1 demands "as known when?"
  before integration, and for a weekly petroleum report the answer is a specific
  minute that has moved at least once in this archive's span.
- Structural: one instrument per config, so the cross-asset comparison would be
  four separate runs whose joint claim nothing in the funnel evaluates.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today. Writing thresholds for a test that cannot be run would
  produce a file that looks pre-registered and is not, which is the specific
  dishonesty `research/backlog/README.md` §1 exists to prevent.
- What is registrable now is the *shape* of the eventual test, so that it cannot
  be chosen after the fact: the release calendar is fixed before any run; the
  event window is declared in minutes before any return is measured; the pre- and
  post-2008 split is declared as two windows rather than searched for; and both
  windows must clear the same bar.
- `min_oos_sessions = 250` and `min_oos_trades = 100` would apply as floors
  whenever the calendar lands, for the same reason they apply everywhere: a
  weekly event gives roughly fifty observations a year, so a single contract's
  life holds too few to say anything.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their sample is US equity, bond-futures and exchange-rate returns at a daily
  or intraday frequency around inventory news; ours is CME futures bars only.
  The overlap is partial and the dependent variables are not the same objects.
- The paper's own reported effects are theirs and are not restated here. None of
  them was produced under a fill model or transaction costs.
- The sign flip is the finding most likely to fail out of sample, because it is
  a claim that the relationship has a regime and the regime was identified after
  seeing both halves.
- Costs would rest on `half_spread_ticks = 1` for CL, ZN and 6E, which is an
  assumption and not a measurement (D-0120) — and an event-window strategy is
  precisely the case where the true spread is widest and least like the
  assumption.

## Triage grade

**C.** C, and the missing piece is a **petroleum-inventory release calendar with an
explicit availability rule**, plus **multi-instrument configs** for the
cross-asset half. Neither is a purchase: the release schedule is public and the
calendar is the same M4 static CSV that seven wave-1 candidates are waiting on.
The multi-instrument gap is a design rule, not a data gap.
