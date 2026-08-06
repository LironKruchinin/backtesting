---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: gold-oil-abnormal-return-day
topic: short-horizon-reversal
grade: A
hypothesis_family: metals-energy-abnormal-return-followthrough
status: draft
created: 2026-08-06
doi: 10.1007/s11408-021-00380-w
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Behaviour on and after outsized days in gold and crude

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

Guglielmo Maria Caporale, Alex Plastun. *Gold and oil prices: abnormal returns, momentum and contrarian effects*.
Financial markets and portfolio management, 2021.
DOI `10.1007/s11408-021-00380-w`. <https://openalex.org/W3149462930>
Retrieved from the openalex API on 2026-08-06.

Using roughly eleven years of daily gold and oil prices, the paper asks four connected questions: whether an unusually large day can be identified before it closes, whether the following day shows a directional tendency, whether that tendency has a describable timing, and whether the timing can be traded. It reports continuation within the outsized day itself, then a split on the day after — continuation in oil and reversal in gold — with both effects short-lived, and claims a trading simulation exploits them.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1007/s11408-021-00380-w':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The plausible payer on an outsized day is forced flow: resting stop orders being swept, and margin-driven liquidation from leveraged positions that were on the wrong side. That flow is price-insensitive by construction, which is what makes it payable; the recipient is whoever takes the other side while it lasts. That story is real and well documented. What undermines this particular paper is the shape of its first result: an outsized day is defined by its own full-day move, so a claim that prices continue in that direction until the day ends is close to circular — you use part of the day to predict the rest of the same day. Strip that out and what remains is the next-day effect, which comes back with opposite signs in the two markets from a single sample. Two markets, four hypotheses, one eleven-year window: opposite signs is exactly what noise plus an invisible multiple-comparison budget produces, and no mechanism is offered for why the metal and the energy should differ.

## Signal in Crucible terms

- Instrument: `GCM2024` and `CLM2024`, separate configs and separate registrations, since the paper claims opposite signs for them.
- Timeframe: `1h`, resampled on read. Chosen over `1d` deliberately: at `1d` a single contract yields roughly sixty bars and the idea cannot be evaluated at all.
- `[indicators.shock] kind = "zscore"`, `period = [20, 40]`, `source = "return"`.
- Crude, continuation reading: `enter_long = "shock crosses_above 2.0"`, `exit_long = "shock <= 0.0"`, mirrored for shorts.
- Gold, contrarian reading: `enter_long = "shock crosses_below -2.0"`, `exit_long = "shock >= 0.0"`, mirrored. Same grid, opposite sign, and the fact that the paper needs both is itself the thing under test.
- Not expressible: 'the day after'. There is no calendar predicate, no day-of-week operand and no anchored prior-session close. The closest available approximation is a session-clock gate — adding `and minutes_since_open < 120` to the entry — which restricts entries to early in a session but does not reference the previous one. That is a weaker hypothesis and must be reported as such.

## Data

- GC and CL hold curated 1-minute bars 2010-06-06 → 2026-07-28, resampled on read to `1h` and `1d`.
- Both roots carry commodity calendars with documented era caveats (D-0089): a 16:15 CT close before 2015-09-21, and six pre-holiday early closes knowingly unmodelled, which surface as missing bars rather than as calendar errors.
- The archive contains the crisis days this kind of study concentrates on — `qa` flags large counts of price spikes on CL contracts clustering on 2020-04-21, the day after WTI settled negative, and those spikes are real (`docs/SPIKE_FORENSIC.md`).
- Missing: whether the paper used spot, nearby futures or a dealer series. We hold CME futures contracts only, so the instrument may not match.
- Missing: any way to define 'the day after' inside a config. This is the sharpest grammar limitation for this candidate and it is not what earned the grade — the idea is still expressible in a degraded form.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: one pooled trading year, and unreachable on a single contract by construction.
- `min_oos_trades = 150` — basis: a two-sigma hourly trigger fires often enough to clear this; below it the two opposite-signed registrations cannot be compared against each other, which is the whole point of running both.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills.
- `kill_if_dead_at_ticks = 1.0` — basis: an outsized-move trigger enters when the market is moving fastest and the book is thinnest, which is when a one-tick half-spread assumption is most generous. If the edge cannot carry one tick, the real cost would have removed it several times over.
- `require_controls_beaten = true` — basis: this is the gate most likely to kill the gold registration and it should. GC rose substantially over most windows in our archive, so a long-biased rule beats zero without beating buy-and-hold, and the funnel will say so.
- `max_permutation_p = 0.05` — basis: opposite signs in two markets from one sample is the signature of a searched result; the block null is what distinguishes it from a real asymmetry.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their window is 2009-01 to 2020-03: a gold bull market bracketed by the aftermath of one crisis and the onset of another. Ours starts 2010-06-06, so roughly two-thirds overlaps — but our archive continues to 2026, which is out-of-sample for them and is the more interesting half.
- Four hypotheses tested on two markets in one sample, with no visible correction for the number of comparisons, producing opposite conclusions for the two markets. That configuration should lower the prior substantially.
- The first hypothesis — that price continues in the direction of an outsized move until that day ends — is nearly definitional, since the day is identified by its own move. It should not be counted as evidence of anything.
- Financial Markets and Portfolio Management is a respectable but small journal. The abstract's trading-simulation claim is the least reliable part of any paper of this kind, because it is where costs are most often omitted; the paper reports its own profitability figures and they are not restated here.
- `half_spread_ticks = 1` is an assumption and not a measurement for both roots, permanently (D-0120). This idea trades precisely when spreads widen, so the assumption is optimistic in exactly the wrong direction.

## Triage grade

**A.** Expressible today as a `zscore` on `source = "return"` with threshold crossings, run separately per root and per sign. The 'day after' framing degrades to a session-clock gate, which weakens the test without breaking it. Runnable is not answerable: one contract's active life is roughly sixty sessions, so the pre-registered sample floors cannot be met and the machine will kill it for that — correctly — until registry pooling across contracts lands.
