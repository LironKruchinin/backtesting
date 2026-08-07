---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: brent-intraday-buildup
topic: intraday-seasonality
grade: B
hypothesis_family: energy-intraday-activity-modes
status: draft
blocked_on: a macro/inventory release calendar — the paper's modes are tied to named scheduled events, which is an M4 static CSV that does not exist
created: 2026-08-06
doi: 10.2139/ssrn.6415578
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Event-window conditioning on intraday crude activity

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

Erik Haugom, Christian Oliver Ewald, Xianwen Chen, Erik Smith-Meyer. *Intraday Stylized Facts and the Shape of Volatility Build-Up in ICE Brent Crude Oil Futures*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6415578`. <https://doi.org/10.2139/ssrn.6415578>
Retrieved from the crossref API on 2026-08-06.

A descriptive paper on ICE Brent, built from tick records covering roughly two decades and reaching down the curve past the front month. Its reported findings: intraday activity is multi-modal rather than one smooth shape, with peaks the authors line up against European market opens, US data releases, inventory reports and settlement; one-step return autocorrelation is negative while variance clusters on a daily cycle; and within-day variance accumulates faster for contracts nearer their own expiry.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6415578':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The strong version has a payer and the weak version does not, and the difference between them is a file we do not own. If the activity peaks are genuinely tied to scheduled releases, then whoever is holding through a release he did not want exposure to is paying, and that is a real and repeating population — hedgers who cannot flatten, funds whose mandates keep them positioned. But identifying those windows requires knowing which release and when, and without a calendar the modes collapse into fixed clock times on a nearly 23-hour contract. At that point the only nameable loser is the participant whose local session boundary we happen to be trading against, which is the same thin story every other time-of-day rule tells. Note also that the inventory report is weekly and shifts on holiday weeks, and the grammar has no day-of-week predicate at all, so the sharpest of the paper's modes is the one least reachable.

## Signal in Crucible terms

- Instrument `CLM2024`, timeframe `15m`, aggregated on read from curated 1-minute bars against the energy calendar's own sessions.
- The construction WOULD be: entries gated on `minutes_since_open` windows placed around each named release, with a continuation or fade rule inside each window and a flat position outside them.
- Where it breaks, first: the windows are expressible but the events are not. A release that moves — and they all move on holiday weeks — is mistimed by a fixed clock offset, and the misses are silent.
- Where it breaks, second: the weekly inventory report needs a day-of-week predicate, and the grammar has no calendar predicates of any kind. That mode is simply unreachable, not merely approximate.
- Where it breaks, third: the maturity effect needs several contracts compared inside one run, and `combo` refuses a config declaring two instruments rather than answering half the question.
- Where it breaks, fourth: within-day variance accumulation is a session-scoped aggregate, and there are none in the grammar — no VWAP, no session high or low, no running sum.

## Data

- Owned: curated 1-minute CL bars from 2010-06-06 to 2026-07-28, plus `ohlcv-1s` in raw for the same span, which is the closest thing the archive has to the paper's tick record.
- Owned: an energy calendar (D-0089), with its documented caveat that CL carries a 16:15 CT close before 2015-09-21 and that a handful of pre-holiday early closes are knowingly unmodelled.
- Not owned: ICE Brent. The paper's contract is a different benchmark on a different exchange, and no acquisition plan includes ICE.
- Not owned: any macro or inventory release calendar. That is the named gap, and it is the input the paper's mode identification actually rests on.
- Not owned: a measured spread for CL, ever (D-0120) — which matters more here than in most files, since an event-window rule trades exactly when spreads are widest.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.02` at 5-, 15- and 30-minute horizons inside each registered window, with the bootstrap interval excluding zero at the same horizon — basis: D-0085, and an event window with no directional information should stop before an equity curve exists.
- The mode-specificity gate, which can kill this even if it makes money: the effect inside a registered window must exceed the effect in adjacent windows of the same width by a registered margin. If it does not, what was found is a general intraday pattern and not the event modes claimed, and this hypothesis is dead even though something was found — basis: the mechanism is what is on trial.
- `require_plateau = true` over the window offset — basis: a result at one 15-minute offset with nothing on either side is a spike, and given that our offsets are fixed-clock approximations of moving events, a real effect should be smeared across neighbouring offsets rather than concentrated in one.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: a few event windows per session, so 250 sessions is the minimum for a countable sample; one CL contract reaches neither and will be killed for sample adequacy.
- `kill_if_dead_at_ticks = 1.0` and `min_oos_sharpe_after_costs = 0.5` — basis: the backlog's constant floors; the cost floor is decisive here because the strategy deliberately trades into information events.
- `max_permutation_p = 0.05` — basis: an event rule trades on a small subset of bars, which is where an ordinary draw most easily looks extreme against a naive benchmark.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The contract is wrong. ICE Brent and CME WTI are related but distinct benchmarks with different delivery, different participants and a spread between them that has been a trade in its own right. Anything run here is a transplant.
- The paper is an SSRN working paper with no peer review, dated 2026, and it is descriptive: it reports no strategy and makes no trading claim, so any tradeable reading is entirely ours.
- Their record reaches back roughly four years further than ours does — the archive opens 2010-06-06 — so the 2008 dislocation and its aftermath sit entirely outside anything we can replay.
- The span contains 2008, 2014-16 and 2020. Those are regime breaks large enough that an intraday activity shape measured across all of them is an average over several different markets.
- Cost realism is load-bearing here more than almost anywhere in this batch. `half_spread_ticks = 1` is a permanent assumption for CL, and a rule that deliberately trades around scheduled releases is trading in the minutes when the real spread is furthest from that assumption.
- The paper reports its own descriptive figures; none are restated here, and none of them describes anything this engine would produce.

## Triage grade

**B.** B, with the honest note that half of it is closer to C. The fixed-clock approximation of the European-open and settlement modes needs only code. The release-tied modes need a calendar with event names and timestamps and an availability rule, which is a static file somebody has to build and verify rather than a library to write, and the weekly inventory mode additionally needs a day-of-week predicate the grammar does not have. Building the approximation and calling it the hypothesis would be the substitution this backlog exists to refuse.
