---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: real-time-cross-market-discovery
topic: cross-asset-lead-lag
grade: C
hypothesis_family: cross-market-announcement-response
status: draft
blocked_on: a macro announcement calendar (M4 static CSV) AND multi-instrument configs; the claim is a joint statement about three markets and a release time
created: 2026-08-06
doi: 10.3386/w11312
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — How stock, bond and currency futures reprice on scheduled US releases

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

Torben M. Andersen, Tim Bollerslev, Francis X. Diebold, Clara Vega. *Real-Time Price Discovery in Stock, Bond and Foreign Exchange Markets*.
National Bureau of Economic Research, 2005.
DOI `10.3386/w11312`. <https://openalex.org/W2898289834>
Retrieved from the openalex API on 2026-08-06.

Using high-frequency futures across three countries, the study measures how equity, fixed-income and currency prices move in the moments around scheduled US macroeconomic releases. It reports that fixed income responds most consistently on average, that the equity response varies with the state of the economy to the point of changing sign, and that the markets co-move beyond what the releases alone account for. It describes how prices respond; it does not propose a position to take.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.3386/w11312':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A jump at a scheduled instant is not on its own evidence that anyone is paying anyone. If the price moves at the release and stays moved, the market did its job, and the money that changed hands went between whoever forecast the number and whoever had to be positioned regardless. So this paper names no payer, and neither can we from what it documents. The one payer that does exist here is a liquidity story rather than a forecasting one: whoever must transact through the release window — a hedger on a fixed schedule, a fund forced to re-establish exposure at a level — pays a wider spread and worse fills to whoever is willing to quote into the uncertainty. That participant is real and recurs on a schedule. But collecting from them means supplying immediacy, and every fill model in this build takes it: `spread_cross` pays the half-spread on both legs.

## Signal in Crucible terms

- Would need `ESH2024`, `ZNH2024` and `6EM2024` replayed against one clock. A config admits exactly one instrument and one timeframe, so the joint claim cannot even be stated.
- Would need an operand reading time-to-a-scheduled-release. The grammar's clock readings are session-relative only — `minutes_since_open`, `minutes_to_close`, `minutes_to_rth_close`, `is_rth` — and know nothing about a publication schedule.
- Would need the surprise: the released number against the consensus forecast. `missing` names the calendar; the consensus series is a second artefact and nobody has costed it.
- The state-conditioning half needs a business-cycle classification, a third external series with its own availability problem — a recession date published months after the quarter it describes is textbook lookahead if joined on the date it describes (§2.1).
- What is buildable today that tests this claim: nothing. The reduced single-instrument version tests a different, weaker statement and should be registered under its own family if anyone wants it.

## Data

- Owned: ES, NQ, RTY, ZN, CL, GC and 6E `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one minute. The US half of the paper's instrument set is well covered and at a finer grain than the effect.
- Not owned: German and British futures. Different exchanges entirely, and no acquisition path is planned for either.
- Not owned: any release calendar with timestamps, any consensus forecast series, any business-cycle dating.
- No L1 for six of the seven roots and none acquirable (D-0120) — which matters here because the paper's own subject is the moments when the spread is at its widest.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.05` for the surprise-signed predictor at a declared post-release horizon. Basis: about 0.04 is what noise supplies at 20,000 bars (D-0085), so anything under it is a floor rather than a finding.
- `max_permutation_p = 0.01`, block length declared and swept (D-0087). Basis: the events are a sparse dated set — roughly ten market-moving US releases a month — and a conventional bar at that event count is one lucky quarter.
- `min_oos_trades = 300` and `min_oos_sessions = 200`. Basis: one round trip per release puts 300 events at about two and a half years, which no single contract supplies; this gate kills every run until pooling lands, and is registered because that is the honest answer today.
- `kill_if_dead_at_ticks = 1.0`. Basis: the position is opened in the widest minutes of the day, so a one-tick assumption is already the optimistic reading, and failing at it settles the question without argument.
- `require_controls_beaten = true`. Basis: an event-timed entry with no information still collects the day's volatility, and the matched random-entry control is what separates the two.
- `max_pbo = 0.5`. Basis: horizon and window length are both free parameters, and a rule selected across them is precisely what CSCV prices.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- An NBER working paper. Our metadata records no journal version, so nothing we hold has been through refereeing we can see — the authors' standing in the field is not a substitute for that, and under this project's prior it does not become one.
- Their sample is the window when high-frequency futures data first became available, well before 2010-06-06. Zero overlap with our archive, which is the good news; it also means everything about who trades releases and how fast has changed between their sample and ours.
- Two of the three countries in the study are markets this project does not hold and has no path to.
- The state-dependence result is the paper's most interesting claim and the least testable here. It needs a cycle classification we do not own, and any classification published after the fact carries a lookahead trap that would have to be settled before it could be joined at all.
- The paper reports its own response magnitudes and rankings across markets; those are its numbers on its sample and describe nothing this build would output.

## Triage grade

**C.** C, and a three-artefact C rather than a one-artefact one. `missing` names a release calendar and multi-instrument configs. The claim as stated also needs a consensus forecast series to define a surprise and a business-cycle dating to condition on — neither owned, and the second carrying a publication-lag problem that has to be resolved before it can legally be joined at all. The calendar alone would not make this runnable, which is the thing to notice before costing it.
