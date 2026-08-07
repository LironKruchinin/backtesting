---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: turn-of-month-rebuttal
topic: calendar-effects
grade: B
hypothesis_family: equity-index-turn-of-month
status: draft
blocked_on: calendar predicates — day-of-month and turn-of-month have no operand; the calendar exists in `crucible-data` and the grammar cannot reach it
created: 2026-08-06
doi: 10.2139/ssrn.244085
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Turn-of-month in index futures — the disappearance arm

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

Edwin D. Maberly, Daniel F. Waggoner. *Closing the Question on the Continuation of Turn-of-The-Month Effects: Evidence from the S&amp;P 500 Index Futures Contract*.
SSRN Electronic Journal, 2000.
DOI `10.2139/ssrn.244085`. <https://doi.org/10.2139/ssrn.244085>
Retrieved from the crossref API on 2026-08-06.

The authors test the month-turn return pattern on a large-cap US index futures contract and report that it stopped being detectable in the 1990s, with the cash index agreeing. They read the pattern as something produced by who was buying at the time rather than as a stable regularity, and warn against projecting in-sample calendar findings forward.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.244085':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The original effect has one of the better payer stories available. Money arrives on a schedule: salaries, retirement contributions, funds obliged to stay invested, and index trackers reweighting to month-end. None of that flow is a forecast; it is price-insensitive and it recurs whether the market is cheap or dear, so whoever supplies liquidity into it is paid for the inventory risk. The mandated allocator is the loser, and keeps losing because being invested is the obligation, not being clever about the date. This registration is the arm that says the loser walked away — that the flow moved into pooled vehicles and stopped landing on those specific days. So the honest framing is inverted: we are registering a null, and the surprise would be finding the effect alive. A confirmation of absence is the expected outcome and must be gradeable as such.

## Signal in Crucible terms

- Instruments: ESH2024 and its siblings, one contract per config; NQ and RTY as the cross-check the flow story demands, since a US-equity-wide cash cycle should not reach large caps only.
- Timeframe: `1d`, aggregated on read from 1-minute bars, giving trading-day bars that open the previous evening (D-0077).
- Feature: an index of the trading day within the month, counted from the month's end. Calendar days would land on a Saturday four times a year; D-0062 already fixed trading days as this project's unit for exactly that reason.
- Rule as it would be written: `enter_long: trading_day_of_month >= -1`, `exit_long: trading_day_of_month > 3`. Neither operand exists — the grammar's clock readings are intra-session (`minutes_since_open`, `is_rth`) and say nothing about where a day sits inside its month.
- The whole rule is calendar. There is no price term to fall back on, which makes this the extreme case: the missing operand is not one clause of the condition, it is the condition.

## Data

- Owned: ES, NQ and RTY `ohlcv-1m`, 2010-06-06 to 2026-07-28, curated. RTY begins in 2017 because the contract did not list on CME before then, so its month-turn count is roughly half of ES's.
- Roughly 190 month-turns per instrument for ES, spread over about sixty contracts — three turns per contract, so pooling is not an optimisation here, it is the only way the sample exists at all.
- Not owned: any cash index series. The paper's spot-market half cannot be reproduced, only its futures half.
- The equity-index session tables carry eras (D-0086), so trading-day boundaries and early closes around month-end holidays are modelled rather than assumed.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 400` — basis: with roughly twenty-one sessions a month, this is a floor of about nineteen month-turns of out-of-sample evidence, and anything thinner cannot distinguish a dead effect from a quiet one.
- `min_oos_trades = 60` — basis: one round-trip per month-turn, so this asks for five years of turns before any statement is made in either direction.
- `min_oos_sharpe_after_costs = 0.50` — basis: twelve round-trips a year is the lowest turnover any hypothesis in this backlog will produce, so costs are near-irrelevant and a weak floor would let noise through.
- `kill_if_dead_at_ticks = 1.0` — basis: an effect that dies at the assumed half-spread on a rule this low-turnover was never economically present, and this is the gate that ends it.
- `max_permutation_p = 0.05` with a declared, swept block length — basis: the null must be run on the month-turn window specifically, because a calendar rule with no price input has nothing else to be wrong about.
- One window is pre-registered and only one: entry at the last trading day's close, exit at the third trading day of the following month. Any other window is a separate declared trial charged to the same hypothesis family, so `max_pbo = 0.40` binds against the search rather than against the run.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- There is zero sample overlap, and that cuts against us. Their evidence ends around 2000; ours begins in 2010. We cannot observe the era in which they say the effect died, so we can neither confirm nor refute their finding — only report what a later, separate era does.
- This is a working-paper record retrieved from a preprint service. Peer review status is unknown from the metadata, and nobody here has read it.
- The effect is the single most widely publicised calendar regularity in equities, listed on every retail-facing site. If it survived our gates the first question would be why, not how much.
- Our sample is dominated by an era of unusually large and persistent passive inflows, which is a mechanism-consistent tailwind. A positive result may be a statement about 2010s fund flows rather than about calendars.
- Cost sensitivity is the least of this hypothesis's problems, but `half_spread_ticks = 1` is still an assumption and not a measurement, and its direction is deliberately not asserted (D-0120).

## Triage grade

**B.** The blocker named above is the entire signal, not a term of it. A day-of-month operand is cheap in isolation — the CLI already computes trading-day keys once and hands the same slice to every consumer under D-0071, which is the pattern to copy — but this hypothesis also needs pooling across roughly sixty contracts, since one ES contract holds three month-turns. Two unlocks, not one, before any verdict is possible.
