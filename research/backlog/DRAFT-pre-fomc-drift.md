---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: pre-fomc-drift
topic: macro-announcements
grade: C
hypothesis_family: equity-index-pre-fomc-drift
status: draft
blocked_on: an FOMC meeting calendar with announcement timestamps — the same M4 static CSV, and the effect is defined entirely by that date
created: 2026-08-06
doi: 10.1111/jofi.12196
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Equity index drift ahead of scheduled policy announcements

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

DAVID O. LUCCA, EMANUEL MOENCH. *The Pre‐FOMC Announcement Drift*.
The Journal of Finance, 2015.
DOI `10.1111/jofi.12196`. <https://doi.org/10.1111/jofi.12196>
Retrieved from the crossref API on 2026-08-06.

The study documents that US equities have historically accumulated a large share of their whole realised gain in the hours ahead of scheduled Federal Open Market Committee decisions, that the pattern strengthened across the decades examined, and that it also appears in other major equity indices. It reports no comparable effect in treasuries or short-rate futures, and no comparable effect ahead of other scheduled macro releases. The authors say standard asset-pricing accounts do not explain it comfortably.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1111/jofi.12196':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This is the awkward case: the effect is large, famous, and nobody has convincingly named who pays. Both candidate stories have holes. If it is compensation for carrying policy uncertainty, the payer is whoever de-risks into the meeting and buys back after — a real population, but one whose behaviour would show in the options market this project does not hold rather than in the futures tape it does. If it is a leakage story, it ought to appear in rates as well, and the paper reports that it does not, which is evidence against. What remains is a large regularity with no counterparty, and under this project's prior that is a warning rather than an opportunity: an unexplained pattern over a few hundred dated events is exactly the shape a survivor of many calendar searches takes. Record the payer as unnamed and let the grade follow from it.

## Signal in Crucible terms

- Instrument: `ESH2024` and the rest of the ES chain. Timeframe `1m` stored, or `1h` aggregated on read (D-0077); the window is hours, so either resolves it.
- The rule is pure calendar: hold long from a fixed offset before a scheduled announcement until the announcement itself, flat otherwise. There is no price input at all.
- The grammar has no calendar predicate and no anchored reference time, so it is not one term of the rule that is inexpressible — it is the entire rule. This is the same shape as H-014 in the backlog and fails for the same reason.
- The artefact is small: eight scheduled meetings a year, so a short dated table. It still needs a source citation, an access date and an availability rule under §4 and §2.1, like every other number in this project that was not measured here.
- The falsifier the paper hands us is worth writing as a rule of its own: the identical construction on `ZNH2024`, where the paper reports the effect is absent.

## Data

- Owned: ES, NQ and RTY `ohlcv-1m` and ZN for the falsifier, 2010-06-06 → 2026-07-28, curated at one minute. RTY's archive begins 2017 because the contract did not list on CME before then, which is a listing fact and not a hole.
- Not owned: the meeting calendar. Cheap to write and still absent, which is what keeps this at C.
- Not owned: any options or implied-volatility series, so the risk-premium account of the mechanism cannot be examined here at all — only its shadow in the futures tape.
- The binding constraint is arithmetic, not acquisition: eight scheduled meetings a year across 2010-06-06 → 2026-07-28 is on the order of 130 events, and no amount of data buys more. That number is the sample, permanently.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_trades = 100`. Basis: the archive can never supply more than about 130 of these events, so this gate sits within a handful of the physical ceiling. It is registered at a level that leaves almost no slack deliberately — a calendar effect measured on fewer than that has not been measured.
- `max_permutation_p = 0.01`, block length declared and swept (D-0087). Basis: at this event count the conventional bar is cleared by chance across the space of calendar rules the profession has already tried.
- The placebo calendar is a registered criterion: run the identical hold across every comparable non-meeting window in the sample and require the real one to sit in the tail of that distribution rather than merely be positive. A comparable placebo is a Kill.
- The cross-instrument falsifier is a registered criterion, and it runs the unusual way round: the paper reports the effect is absent in rates, so finding it in ZN is a Kill rather than a confirmation. A pattern that shows up wherever we point is a measurement artefact.
- `kill_if_dead_at_ticks = 1.0`. Basis: eight round trips a year is the lowest turnover imaginable, so costs should be nearly irrelevant — and an effect that dies at one tick with that turnover was never economic in the first place.
- The trial count charged to this family opens above one. Basis: this is a survivor of a wide search over calendar windows conducted by the profession, and the deflation has to pay for the search that produced the hypothesis, not only for the runs we perform.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is as strong as this field offers, which under our prior means the result is real in their sample and says nothing about ours.
- Their sample covers decades ending before publication; ours opens 2010-06-06, overlapping only their tail and otherwise sitting entirely after the paper appeared. That makes our run close to a clean out-of-sample test, and simultaneously means a positive result would be surprising.
- This is one of the most widely publicised anomalies in the literature. It has been in the public domain for over a decade and is implementable by anyone who can set a calendar reminder, which is the first thing to ask about if it survived our gates.
- About 130 events is a small sample that cannot be enlarged, so any interval around any statistic will be wide. The sample gate should be treated as binding rather than advisory.
- Our window is dominated by an era of unusually active and unusually well-telegraphed policy communication, which is a mechanism-consistent tailwind. A positive result may be a statement about 2010–2026 central-bank practice rather than about scheduled announcements in general.
- The paper reports its own magnitudes for the pre-announcement window and its own comparison against other markets; those are its figures on its sample and are not restated here as anything this build would produce.

## Triage grade

**C.** C, with the cheapest artefact in the batch by a distance — about 130 dated announcement times, writable with a citation and an availability rule in an afternoon. What holds it at C is that the grammar has no calendar predicate, so the file buys nothing alone: the rule is entirely calendar, entirely inexpressible, and needs the D-0071 caller-side slice before one bar is replayed. The permanent sample ceiling is the second problem, and no build fixes that one.
