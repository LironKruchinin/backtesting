---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: eia-release-window-crude
topic: crude-inventory-storage
grade: B
hypothesis_family: cl-scheduled-release-window
status: draft
blocked_on: calendar predicates (day-of-week), and a release calendar for the holiday shifts
created: 2026-08-07
doi: 10.2139/ssrn.6486660
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Trading the crude reaction to a scheduled weekly inventory report

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

William Pape. *Latency Alpha: Real-Time LLM-Based Semantic Extraction of EIA Petroleum Data for Forecasting and Trading Crude Oil Futures*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6486660`. <https://doi.org/10.2139/ssrn.6486660>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author builds an event-driven framework that
turns the text of the US petroleum status report into a signal within a short
latency budget and trades crude futures on it, arguing that the economic value
comes from processing the release faster than the market.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6486660':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A weekly government report lands at a published minute and moves crude. The
paper's version of the edge is *latency* — read the release faster than everyone
else — which is a claim about infrastructure, not about markets, and Crucible
cannot compete on it and should not pretend to. What survives the translation is
the weaker and more interesting claim underneath: that the minutes around a
scheduled release have a different return distribution from ordinary minutes,
and that a rule which knows when the release is due can either stand aside or
lean into it. Who pays, in the latency version, is whoever is slower; in the
weaker version, whoever is forced to trade in the window regardless — hedgers
with delivery obligations and funds with mandates. Only the second payer is
plausible for a replay that reads one-minute bars, and the file registers the
second.

## Signal in Crucible terms

- One instrument, `CLM2024` or another single crude contract, `timeframes =
  ["1m"]`, raw contract only.
- The window would be a conjunction of a weekday and a time of day. The time of
  day is expressible — every reading in D-0078's set is available — and the
  weekday is not. There is no operand that names a day of the week, so
  "Wednesday, in the ten minutes after the release" cannot be written.
- The abstention arm — flatten before the window, re-enter after — is the
  cheapest possible use of a calendar and still needs one. The same is true of
  the participation arm.
- What *is* expressible without the weekday is a rule fired on every day at that
  clock position, which spends four days out of five trading a non-event. That
  is not a weaker version of the hypothesis, it is a different one, and it would
  bury the effect in noise by construction.

## Data

- Owned: CL `ohlcv-1m` and `ohlcv-1s` 2010-06-06 → 2026-07-28, every contract,
  curated. The bars around every release in sixteen years are on disk.
- Owned: a CME energy session calendar with eras (D-0089), so the clock readings
  are real rather than derived from a constant.
- Not owned: the release *schedule*. The weekly cadence is public and stable —
  which is exactly why this is a B rather than a C — but the holiday shifts are
  not derivable from a day-of-week predicate, and the release time itself has
  changed at least once in the archive's span.
- Not owned: the report's contents, the consensus, or the surprise. Anything
  conditioning on the *size* of the surprise is a further acquisition and would
  push this to C.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a weekly event needs a year of weeks before a
  window statistic means anything, and the event count, not the session count, is
  the binding sample here. Not reachable on one contract.
- `min_oos_trades = 40` — basis: one entry per week over 250 sessions is roughly
  fifty events; a floor below the event count would let a rule pass on a handful.
- `min_oos_return_pct_free_fills` must be cleared by the participation arm at
  S1 before the costed arm is run at all, because a release-window rule that
  cannot pay for itself with free fills has nothing left to lose to costs.
- `kill_if_dead_at_ticks = 0.5` — basis: deliberately the tightest kill level of
  any candidate in this batch. The spread around a scheduled release is at its
  widest exactly when the rule wants to trade, so the assumed one-tick half
  spread is most likely to be wrong here and wrong in the flattering direction.
- The discriminator: the abstention arm and the participation arm must disagree.
  If standing aside and leaning in both clear the bar, the window is not doing
  the work and the result is a coincidence of two rules.
- `require_controls_beaten = true` — basis: a rule that trades once a week on a
  trending contract can look fine while beating nothing, and the matched
  random-entry median over sixteen draws is what catches that.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper is an SSRN working paper, not refereed, and its stated edge is
  latency. A latency claim is untestable here in both directions: this build
  replays one-minute bars, so it can neither reproduce the advantage nor rule it
  out, and any result it produces is about a different strategy.
- The paper's own reported figures are theirs and are not restated here.
- The release calendar is the load-bearing input and it is the one thing not
  owned. A hypothesis whose entire signal is a timestamp should be graded on the
  timestamp.
- A day-of-week predicate would get most of the way there and would be silently
  wrong on the shifted weeks, which is worse than being loudly absent: the
  shifted weeks are holiday weeks, and holiday weeks are not ordinary.

## Triage grade

**B.** B, and the missing piece is a **calendar predicate for day-of-week** — the
grammar reads the session clock but not the date — plus, for the shifted weeks,
the same **release calendar** the C-grade candidates need. It is B rather than C
because the weekly cadence is a public constant and the bars are owned; the code
gap is one operand, and it is the operand five wave-1 candidates are also waiting
on.
