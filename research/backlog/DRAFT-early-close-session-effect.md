---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: early-close-session-effect
topic: holiday-weekend-effects
grade: A
hypothesis_family: holiday-adjacent-session-effect
status: draft
created: 2026-08-07
doi: 10.5750/jpm.v15i3.1964
source_api: crossref
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The holiday-adjacent session, identified by the exchange closing early

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

William Ziemba, Constantine Dzhabarov. *Holiday Effects in the US Equity Futures Markets*.
The Journal of Prediction Markets, 2021.
DOI `10.5750/jpm.v15i3.1964`. <https://doi.org/10.5750/jpm.v15i3.1964>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors examine equity index futures over several
sub-periods and report gains on each of the three sessions preceding a holiday, with
the small-cap contract stronger than the large-cap one, note gains on the two
sessions after the holiday for large caps, observe that the effect weakened over the
decades so that only the third session before remains statistically distinguishable,
and argue the futures move anticipates the cash market's move.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.5750/jpm.v15i3.1964':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The pre-holiday effect is one of the oldest calendar anomalies and its usual
explanation is inventory rather than sentiment: participants who do not want risk
over a closure reduce it beforehand and rebuild afterwards, and whoever is willing
to hold across the gap is paid to. Who pays is therefore whoever must be flat over
the holiday — a desk with a risk limit, a fund with a mandate. That is
obligation-driven flow again, which is the class that does not learn. Against that
sits the paper's own finding that the effect has decayed, which is what a
well-publicised anomaly does. The reason to test it on **energy, metals, FX and
rates** is that the paper looked at equity index only, and the holiday calendars of
those four products are genuinely different — CLAUDE.md §9 records that on one
January holiday the last traded minute was 12:00 CT for ES and ZN, 13:30 for CL and
GC, and 15:58 for 6E, which traded a full session. Five products, one date, four
answers. If the effect is about closures, it should follow those differences.

## Signal in Crucible terms

- One instrument, one timeframe, raw contract, `spread_cross`. Registered on
  `CLZ2024`, `GCZ2024`, a single `6E` contract and a single `ZN` contract as four
  configs under one family.
- **The holiday-adjacent session is expressible today, and this is the construction
  the candidate exists to record.** `minutes_to_close` honours an early close while
  `minutes_to_rth_close` counts to the scheduled one (D-0078), and the grammar
  compares two operands as readily as an operand and a constant. So
  `minutes_to_close < minutes_to_rth_close` is true exactly on a session the
  exchange is shutting early and false on an ordinary one — a calendar predicate
  reached without a calendar operand.
- `enter_long = "minutes_to_close < minutes_to_rth_close"`, with
  `exit_long = "minutes_to_close <= 5"` to flatten into the early close. Long only,
  because the paper's direction is pre-specified; a symmetric pair would be a
  different, weaker experiment.
- Second arm, same family: hold through the closure instead of flattening —
  `exit_long = "minutes_to_close >= minutes_to_rth_close"`, which becomes true again
  on the next ordinary session. The difference between the two arms is the whole
  test of whether the gain is in the shortened session or in the gap.
- **What this does NOT express, stated plainly:** the paper's effect is on the
  sessions *before* a holiday, and CME's early close usually falls *on* the
  holiday-adjacent session rather than the one before it. So the predicate names a
  neighbouring session, not the paper's day −3. It is a proxy and the file grades
  itself on the proxy.
- It also silently misses the 26 dates D-0089 records as **unmodelled** 15:15 CT
  pre-holiday closes for 6E and ZN and 6 for CL and GC. On those dates the predicate
  is false although the exchange closed early, so the 6E and ZN arms are testing a
  subset of the real closures and the report should say how many.

## Data

- Owned: CL, GC, 6E and ZN `ohlcv-1m` across 2010-06-06 → 2026-07-28, and four
  bundled commodity session calendars with eras (D-0089) that model the early
  closes this predicate reads.
- Owned and load-bearing: those calendars' `rth_close_local` values are a **cited
  convention rather than a measurement** on all four commodity tables (CLAUDE.md
  §9) — open outcry ended and CME publishes no RTH window for these products. The
  predicate compares against that convention, so the whole construction rests on a
  number nobody measured. That is the single largest caveat on this candidate and it
  is stated here rather than in a footnote.
- Known-wrong dates: the 26 + 6 unmodelled early closes above, and the pre-2013
  holiday close differences the same decision records.
- `half_spread_ticks = 1` is an assumption for all four roots (D-0120), and a rule
  that trades a handful of sessions a year is unusually exposed to it because each
  round trip is a large fraction of the sample.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: deliberately double the usual floor. CME has
  roughly nine early closes a year, so 250 sessions holds about nine events and the
  binding sample is events, not sessions. This is the candidate where the standard
  floor would most flatter the result.
- `min_oos_trades = 30` — basis: thirty events is a little over three years. Below
  that a single unusual holiday dominates.
- `min_oos_sharpe_after_costs = 0.4` — basis: the rule is out of the market almost
  always, so its few trades must be good ones; a low bar on a sparse rule is a bar
  on noise.
- `kill_if_dead_at_ticks = 1.0` — basis: one round trip per event, and early-close
  sessions are thin, so the true spread is likely worse than the assumed one exactly
  when the rule trades.
- **The cross-product discriminator is the point of registering four arms.** CME's
  holiday hours differ by product, so if the effect is about closures it should track
  those differences; if all four arms behave identically on dates where the exchange
  behaved differently, the result is not about holidays.
- `require_controls_beaten = true` and `max_permutation_p = 0.05` — basis: a rule
  with thirty trades will beat a random-entry median by luck often enough to matter,
  and a calendar anomaly is the archetype of a finding that survives in-sample and
  nowhere else.
- The decay is registered as a prediction: the effect must not be **larger** in the
  second half of the archive than the first. The paper says it has been decaying for
  thirty years; a run that finds it strengthening in 2010–2026 is more likely to have
  a calendar bug than a discovery.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their instruments are S&P 500 and Russell 2000 futures over 1993–2020 — equity
  index, which is the asset class this wave is deliberately weighted away from. This
  candidate applies the idea to four non-equity roots and is therefore an
  **extension, not a replication**, and its result says nothing about whether the
  paper is right about equities.
- The paper's own reported figures are theirs and are not restated here.
- The journal is a small specialist one and the authors are long-standing
  proponents of calendar effects, which is a reason for extra caution rather than
  less: an anomaly's most enthusiastic reporters are not its most sceptical testers.
- The paper's own headline is that the effect has decayed to one surviving day. A
  candidate whose source says the effect is mostly gone is being registered mainly
  to be killed, and that is a legitimate use of a cheap grade-A slot — but it should
  be spent knowing that.
- The proxy mismatch (early-close session versus day −3) and the RTH convention are
  the two ways this could produce a wrong answer while looking correct, and both are
  in the Signal and Data sections rather than here so that they are read before the
  config is written.

## Triage grade

**A.** A. Two session-clock operands compared against each other, plus a flatten rule, on
one raw contract at one timeframe — legal TOML today with no new Rust and no new
data. It is worth flagging that **the backlog README lists calendar predicates as
inexpressible**, and this construction shows that one specific calendar predicate —
"the exchange is closing early today" — falls out of two existing operands. That does
not retire the row: day-of-week, day-of-month and turn-of-month remain
inexpressible, and five wave-1 candidates plus two wave-2 ones are still blocked on
them. Runnable is not answerable here either, and less so than usual: one contract's
life contains one or two early closes.
