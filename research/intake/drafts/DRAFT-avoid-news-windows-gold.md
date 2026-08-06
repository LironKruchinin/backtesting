---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: avoid-news-windows-gold
topic: macro-announcements
grade: C
hypothesis_family: gc-announcement-window-abstention
status: draft
blocked_on: a macro announcement calendar — the ENTIRE intervention is 'do not hold a position in this window', which is the cheapest possible use of one and still needs one
created: 2026-08-06
doi: 10.58837/chula.the.2023.689
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Standing aside through scheduled releases, tested on gold

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

, Patipan Piyakulmala. *How avoiding positions during macroeconomic news announcement periods enhances gold trading strategies*.
venue unrecorded.
DOI `10.58837/chula.the.2023.689`. <https://doi.org/10.58837/chula.the.2023.689>
Retrieved from the crossref API on 2026-08-06.

A postgraduate thesis asks whether three familiar trend and momentum rules on gold do better when they refuse to hold a position through scheduled macroeconomic release windows, across eight release families spanning the US, China, Germany and the euro area, with controls for cycle phase, sentiment, uncertainty and the rules' own recent accuracy. Its reported conclusion runs against the intervention: for the highest-impact releases, standing aside is associated with worse outcomes, with a few releases pointing the other way.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.58837/chula.the.2023.689':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The abstention story is a cost story, not a forecasting one, and its payer is nameable: the participant who leaves a resting order or a stop in the book through a release and is filled by whoever is quoting a wide, defensive market for those seconds. That participant pays a spread several multiples of the usual one, and on a stop pays it at a price the market visited only briefly. So the money is real and someone is collecting it. The problem is that this build cannot represent it. `spread_cross` charges a constant `half_spread_ticks` on every fill regardless of the clock, so abstaining from a release window removes exposure and removes exactly zero cost. Whatever the machine reported would be a statement about the return distribution inside those minutes, which is not the claim being made. The thesis's own finding, that abstention costs more than it saves, points the same way.

## Signal in Crucible terms

- Instrument: `GCZ2024` and the rest of the GC chain. Gold is owned at one-minute grain from 2010-06-06, so the asset matches the paper exactly, which is rare here.
- Timeframe: `1m` for the window logic or `15m` for the base rules; both available, the second aggregated on read from stored one-minute bars.
- Base rules: an EMA crossover is directly expressible — `enter_long: ema_fast crosses_above ema_slow`. MACD is not: it is a difference of two averages and the grammar has no arithmetic between operands. A momentum rule stated as a price difference is not expressible either. Two of the thesis's three rules therefore cannot be written as stated.
- The abstention predicate needs a release-calendar operand. The grammar's clock readings are session-relative and cannot name a publication.
- The calendar is larger than the US-only one the other candidates in this batch need: two Chinese series, a German survey and a euro-area rate decision, each with its own publication convention and time zone.

## Data

- Owned: GC `ohlcv-1m`, 2010-06-06 → 2026-07-28, and a bundled metals session calendar with eras (D-0086) — needed here, because an abstention window has to be tested against real session boundaries rather than clock arithmetic.
- Not owned: the release calendar, in the four-jurisdiction form this thesis uses.
- Not owned: sentiment and uncertainty indices, or any business-cycle labelling — the thesis's control variables. A version without them is a different regression, not a smaller one.
- No L1 for GC, ever (D-0120). That is the measurement which would decide this whole question, and it is the one the archive cannot obtain.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- This is a refutation registration and should be marked as one: the source's own conclusion is that the intervention hurts, so the criterion is stated in that direction and a result agreeing with the thesis is a successful run rather than a failed one.
- `min_oos_trades = 300` on the base rule. Basis: abstention only bites on the subset of trades that would have spanned a window, so the base rule needs several times the events to leave a testable subset behind.
- `min_oos_sessions = 500`. Basis: eight release families across four jurisdictions gives on the order of a hundred windows a year, and two years of sessions is the floor at which the affected subset stops being anecdote.
- `kill_if_dead_at_ticks = 1.0` applied to the base rule before any abstention is layered on. Basis: an intervention that improves a rule which does not survive one tick is an improvement to nothing.
- `require_controls_beaten = true` on the base rule, for the same reason: the matched random-entry control is what decides whether the base rule is a rule at all.
- The abstaining and non-abstaining variants must be declared as two combos in the same family so both are charged as trials and neither can be reported without the other. Basis: reporting only the favourable half is the pre-registration failure D-0101 exists to prevent.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Venue: none recorded. Our metadata carries no journal, no year and one blank author field, and the identifier resolves to an institutional thesis repository. Under this project's working prior a thesis with no journal version is the weakest venue class in this batch, and this file says so before anyone spends a fold on it.
- The reported conclusion runs against the intervention, which is unusually useful. A registration whose source found nothing is not competing with publication bias in the usual direction, and that is the main reason to keep the file at all.
- The base rules are only partly expressible here, so any run would test a modified version — and a modified version is a different experiment, which must be stated in the registration rather than discovered in the report.
- The instrument matches, but the release set spans four jurisdictions, and the two Chinese series and the German survey publish at hours that intersect the gold session very differently from a US release.
- The whole intervention is a cost claim, and every cost number in this project rests on an assumed `half_spread_ticks = 1` (D-0120). For six of the seven roots, gold included, no L1 exists and none is acquirable, so the widening this hypothesis is about is structurally invisible to the build.
- The thesis reports its own regression coefficients and their signs; those are its results on its sample, and none of them describes anything this build would output.

## Triage grade

**C.** `missing` names a release calendar and this is the largest one in the batch — four jurisdictions rather than one. Worse, the intervention is a cost claim while the build charges a constant half-spread regardless of the clock (D-0120), so even with the calendar the machine would measure the exposure change and not the cost change. Closing that means intraday spread data for gold, which the archive cannot acquire. Two of the three base rules are inexpressible as well.
