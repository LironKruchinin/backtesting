---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: pre-announcement-drift
topic: macro-announcements
grade: C
hypothesis_family: es-zn-pre-announcement-drift
status: draft
blocked_on: a macro announcement calendar with release timestamps and an explicit availability rule (Sec 2.1) — an M4 static CSV that does not exist yet
created: 2026-08-06
doi: 10.1017/s0022109018000625
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Prices moving the right way before scheduled US releases

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

Alexander Kurov, Alessio Sancetta, Georg Strasser, Marketa Halova Wolfe. *Price Drift Before U.S. Macroeconomic News: Private Information about Public Announcements?*.
Journal of Financial and Quantitative Analysis, 2018.
DOI `10.1017/s0022109018000625`. <https://openalex.org/W3123373742>
Retrieved from the openalex API on 2026-08-06.

Across a large set of scheduled US macroeconomic releases, the study examines index and treasury futures in the window immediately preceding publication. For a subset of the releases that actually move markets, it reports that a large fraction of the whole repricing has already happened before the number is public, and in the direction the number will later justify. The authors offer leakage and privately assembled forecasting as candidate explanations for that.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1017/s0022109018000625':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Here the payer is nameable and the mechanism is not mysterious. If some participants know the number before publication, everyone quoting into that window is adversely selected: market makers who cannot withdraw from every release without leaving the business, and scheduled hedgers who must transact whether or not a print is due. They keep paying because the leakage is not observable in advance and the alternative is not trading at all. What we could collect, though, is a much weaker thing than what the informed participant collects. They trade on the number; we would only see the drift after it had begun, which makes our version momentum into a release rather than a forecast of one. That distinction decides the whole idea. A trade riding a move already underway competes with everyone else who can also see it, and pays the spread in the widest minutes of the day to join.

## Signal in Crucible terms

- Instruments: `ESH2024` and `ZNH2024` and their chains. Both of the paper's instrument families are roots we hold at one-minute grain, which is unusual in this batch.
- Timeframe `1m`. The window the claim lives in is tens of minutes, so one minute resolves its start; `5m` would blur exactly the boundary being tested.
- The rule would be: inside a fixed window before a scheduled release, take the sign of the move so far and hold it into the print. Naming that window needs an operand the grammar does not have — its clock readings are session-relative and know nothing about a publication schedule.
- The precedent for supplying it is D-0071: the CLI computes the keys once and hands the same slice to every consumer, so no two components can disagree about which window a bar falls in.
- The calendar needs an explicit availability rule before integration (§2.1). Release times are published far in advance, which makes this the easy case — but the schedule as known on the day and the schedule as reconstructed in 2026 differ whenever a release was moved, and only the first is legal.

## Data

- Owned: ES and ZN `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one minute. Both instrument families, at a grain finer than the effect.
- Not owned: any release calendar carrying timestamps. That is the `missing` piece and it is an M4 static file that does not exist yet.
- Not owned: consensus forecasts. Not required for the drift-riding version above; required for anything that conditions on the size or sign of the surprise.
- No L1 for ES before 2025-07-28 and none for ZN ever (D-0120). That matters more here than almost anywhere else in the backlog, because the trade is deliberately placed where the spread is widest.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_trades = 400`. Basis: roughly ten market-moving US releases a month puts 400 events at a little over three years, which no single contract supplies — so this gate kills every run until registry pooling lands, and it is registered knowing that, because that is the correct answer today.
- `min_abs_ic = 0.05` for the pre-window move as a predictor of the release-window move. Basis: about 0.04 is what a random walk gives away at this sample length (D-0085), so anything under it is a floor rather than a finding.
- `max_permutation_p = 0.01`, block length declared before the run and swept (D-0087). Basis: the events are dated and sparse, and a conventional bar on that count is met by one favourable quarter.
- `kill_if_dead_at_ticks = 1.0` and `min_oos_sharpe_after_costs = 0.5`. Basis: the position opens in the least liquid minutes of the release cycle, so one tick is the optimistic reading of the cost; if it dies there it never existed.
- The placebo is a registered criterion, not a diagnostic: run the identical rule at clock-matched windows on days with no scheduled release. If the placebo looks comparable the verdict is Kill, and the record says session-clock effect rather than release effect.
- `max_pbo = 0.5`. Basis: window length and horizon are both parameters, and a rule chosen across them is what CSCV exists to price.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is the strongest in this batch — a top-tier finance journal with a serious refereeing process. That lifts the prior; it does not remove it, because a top journal publishes what worked and there is no drawer for what did not.
- The sample window is not in the metadata we hold and must be read from the paper before promotion. The overlap against 2010-06-06 → 2026-07-28 is what decides whether our run is out-of-sample or a partial rerun.
- The mechanism is a compliance question as much as a market one, and the arrangements that make early access possible have been revised more than once by regulators and data vendors over the years this archive covers. Whether the regime they document still exists is not something our bars can settle, and a promoter should treat it as the first question rather than the last.
- Our version cannot condition on the surprise, so it tests a strictly weaker claim than the paper's. A negative result here would not refute the paper, and the file should not later be read as if it had.
- The paper reports that the drift ahead of publication accounts on average for roughly two fifths of the whole repricing; that is its measurement on its sample, and it is not a statement about anything this build would produce.
- Every cost figure rests on `half_spread_ticks = 1`, an assumption and not a measurement (D-0120). For a strategy that deliberately trades the widest minutes of the day, that assumption is doing more work here than anywhere else in this backlog.

## Triage grade

**C.** C, and the strongest C in this batch. Both instrument families are owned at a finer grain than the effect, so the only gap is the schedule `missing` names: a dated table with a source, an access date and an availability rule, plus a caller-side operand in the D-0071 pattern because the grammar's clock readings are session-relative. That is one static file and one seam, not a purchase. Nothing else about the idea is blocked, which is rare here.
