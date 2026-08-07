---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: gold-futures-seasonal-anomalies
topic: calendar-effects
grade: B
hypothesis_family: gc-calendar-seasonality
status: draft
blocked_on: calendar predicates (day-of-week, month-of-year) — no operand names a date
created: 2026-08-06
doi: 10.20409/berj.2023.422
source_api: semanticscholar
harvested_from: semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Weekday and month seasonality in gold futures

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

Yasemin Karataş Elçiçek. *Examination of the Existence of Month of the Year, Day Effect of the Week, and Seasonal Anomalies in Gold Futures Contracts: The Case of Turkey*.
Business and economics research journal, 2023.
DOI `10.20409/berj.2023.422`. <https://www.semanticscholar.org/paper/fee11dc658f2ce5a26b11183392e96d948c0bb89>
Retrieved from the semanticscholar API on 2026-08-06.

The author fits a mean-and-variance time-series specification to returns on gold futures listed on a derivatives exchange in Turkey over roughly nine years, and reports coefficients that differ from zero on particular weekdays, particular months, and particular quarters of the year.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.20409/berj.2023.422':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

There is no payer here and the draft should say so at the top rather than bury it. Gold has no payroll cycle, no month-end index reweighting that a bullion contract absorbs, no scheduled hedging obligation tied to a weekday. The equity calendar effects in this batch at least have a flow story with a named institution behind it; this one has a set of regression coefficients and no proposed economic channel visible in the metadata. Two things could still produce a real seasonal in a metals future — the physical demand cycle around festival and jewellery buying, and a currency effect if the contract is quoted in a currency with its own calendar — and the second is the more likely explanation for a result found on a lira-denominated contract. Neither is claimed by the paper as far as we can tell. Unnamed losing side, and no plausible candidate that survives inspection.

## Signal in Crucible terms

- Instrument: one CME gold contract per config, four-digit key (`GCM2024`). This is dollar-denominated bullion on a different exchange from the one studied.
- Timeframe: `1d`, aggregated on read from 1-minute bars.
- Features: weekday of the trading day, and calendar month. Neither operand exists; the grammar's clock readings are intra-session only.
- Rule as it would be written: `enter_long: month == january`, exit at month end; and separately a weekday variant. Each is a distinct pre-registered trial, and the paper's own result is a scan across three seasonal axes at once.
- There is no price-based substitute for a calendar term, so nothing partial can be run today.

## Data

- Owned: GC `ohlcv-1m` 2010-06-06 to 2026-07-28, curated, with a modelled metals session table (D-0086, D-0089).
- Not owned: the contract the paper actually studies, its exchange, or its quote currency. We hold no Turkish lira series and the FX root in this archive is EUR/USD.
- Sixteen years yields about sixteen observations of any given calendar month, which is a small sample by any standard and cannot be enlarged by using a finer grain.
- The paper's window sits inside ours, so there is no clean out-of-sample period available for a like-for-like test.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Exactly one month and exactly one weekday are pre-registered before the run; every other choice is a declared trial in this family. The paper searched three axes simultaneously and our accounting must start from its search, not from ours.
- `max_permutation_p = 0.01` — basis: deliberately stricter than the 0.05 used elsewhere in this batch, because the hypothesis arrives from a multi-axis scan with no visible correction and the prior should be adjusted at the gate rather than in the prose.
- `min_oos_sessions = 500` — basis: about two years of sessions, which yields roughly two instances of any registered month. That is nowhere near enough, and stating it as a floor makes the inadequacy machine-visible instead of a footnote.
- `min_oos_trades = 40` — basis: a month-scoped rule trades twelve times a year at most, so this asks for several years before anything is said.
- `kill_if_dead_at_ticks = 1.0` — basis: turnover is low so costs should barely register; an edge that cannot survive the assumed spread at twelve round-trips a year was never there.
- `max_pbo = 0.30` — basis: tighter than the batch default because the candidate parameter space is a calendar with twelve months and five weekdays, and that is a search whose overfit probability is the whole question.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The instrument, the exchange, the currency and the regulatory regime all differ from ours. A seasonal found in a lira-quoted contract may be a statement about the lira, and this archive holds nothing that could separate those two readings.
- Three seasonal axes were examined in one study with no correction visible in the metadata. Testing weekdays, months and quarters at conventional significance produces findings by arithmetic alone.
- The venue is a regional business and economics journal. That is not automatically disqualifying, but combined with a multi-axis calendar scan it lowers the prior considerably.
- The study window overlaps ours almost entirely, so we cannot offer a genuinely fresh sample — only a different contract in a different currency over roughly the same years.
- Cost figures rest on `half_spread_ticks = 1`, an assumption with no L1 measurement available for GC and none acquirable (D-0120).

## Triage grade

**B.** Graded on cost to test, not on merit, and the cost is the same missing calendar operand that blocks three other files in this batch — one build unlocks all of them. The merit is another matter: a different exchange, a different currency, a multi-axis scan and a fully overlapping window make this the weakest candidate here, and the criteria above are deliberately set so it dies at the permutation gate rather than at a return floor.
