---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: announcement-rates-covariance
topic: macro-announcements
grade: C
hypothesis_family: rates-announcement-covariance
status: draft
blocked_on: a macro announcement calendar, plus multi-instrument configs for the covariance half
created: 2026-08-06
doi: 10.1002/fut.20336
source_api: crossref
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Scheduled releases and the intraday covariance of rate futures

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

Dimitrios D. Thomakos, Tao Wang, Jingtao Wu, Russell P. Chuderewicz. *Macroeconomic announcements, intraday covariance structure and asymmetry in the interest rate futures returns*.
Journal of Futures Markets, 2008.
DOI `10.1002/fut.20336`. <https://doi.org/10.1002/fut.20336>
Retrieved from the crossref API on 2026-08-06.

The study examines how scheduled macroeconomic releases affect intraday volatility, covariance and correlation between two US interest-rate futures contracts, and reports that most of the abrupt intraday changes in those quantities line up with the release schedule, with the timing of a release mattering as well as its content. It also reports that releases sharpen the asymmetry in the volatility response and that correlation tends to rise alongside volatility. It is a second-moment paper: nothing in it is a directional claim.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.20336':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

There is no payer here because there is no directional claim to be paid on. Knowing that variance clusters at a scheduled minute tells you when risk arrives, not who is wrong about it. There are three ways such knowledge becomes money and this build can express none of them: sell the volatility, which needs options the archive does not hold; size positions continuously against forecast variance, which the grammar cannot state because position size is a fixed contract count; or stand aside through the window, which is the next candidate in this batch and whose saving this build cannot even represent, since the fill model charges a constant half-spread regardless of the clock. So the honest statement is that this paper is infrastructure rather than a strategy — useful for deciding when a result should be distrusted, not for deciding what to trade. Promoted at all, it should be promoted as a regime-labelling task and graded as one.

## Signal in Crucible terms

- The paper's two instruments are a short-rate contract and a long-bond contract. The archive holds ZN and nothing else in rates, so the covariance half has exactly one leg.
- A covariance needs both series inside one config, and the grammar admits one instrument per config. That is the multi-instrument gap `missing` names.
- The single-instrument residue is `stdev(period, return)` on `ZNH2024` — a trailing window with no way to key it to a release time. It measures volatility, not volatility conditional on a release, and the difference is the paper's entire subject.
- Nothing here becomes an `enter_long` or `enter_short` rule without an additional claim the paper does not make, which is the reason this candidate has no signal rather than a blocked one.

## Data

- Owned: ZN `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one minute.
- Not owned: the short-rate contract the paper pairs it with. That family was wound down after the transition away from LIBOR, so it is not merely unowned — it largely no longer trades in the form studied, and its successor is not among the seven roots either.
- Not owned: any release calendar with timestamps.
- Not owned: any options or implied-volatility series, which is where a second-moment result would normally be monetised.
- No L1 for ZN and none acquirable (D-0120), so the release-window spread widening this paper's result implies is invisible to the build by construction.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Before any funnel gate applies, the registration needs a directional claim: a second-moment finding supplies no `enter_long`, and a config with no entry produces a run with no trades, which the machine correctly reports as nothing. That is the first criterion and it is a structural one.
- If promoted instead as an abstention rule, the criteria of the following candidate in this batch apply and this file should be folded into it rather than run beside it — two registrations of one experiment double the trial count for no information.
- If a directional claim is derived — for instance that the release-window volatility is followed by a signed move — then `min_abs_ic = 0.05` at the declared horizon. Basis: about 0.04 is the random-walk floor at this sample length (D-0085).
- `max_permutation_p = 0.01`, block length declared and swept (D-0087). Basis: the events are sparse and dated, so the strict bar is the honest one at this count.
- `min_oos_trades = 400` and `kill_if_dead_at_ticks = 1.0`. Basis: the trade would sit in the widest spread of the day, and the sample has to be long enough that the release schedule rather than one quarter is what produced the result.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- A real derivatives journal and a competent second-moment study, but it is not a strategy paper and does not present itself as one. Reading a variance result as an edge is our error to avoid, not theirs to answer for.
- One of the two instruments studied has effectively been retired by the transition away from LIBOR. A covariance result between two contracts cannot be reproduced when one of them no longer trades in the form measured.
- Their sample predates this archive entirely; ours opens 2010-06-06, after the financial crisis and across the whole zero-rate era they could not have observed. Rate-futures covariance in a period when the front of the curve is pinned is a different object from the one they measured.
- No cost treatment and no execution assumption appears in anything we hold about this paper.
- Every cost number here rests on `half_spread_ticks = 1` (D-0120), and ZN has no L1 in the archive and no path to any, so the widening the result implies cannot be seen — which is awkward, because that widening is most of what the finding would be good for.
- The paper reports its own decomposition of intraday jumps across the announcement set; those figures are its own, describe neither a Crucible run nor a tradeable quantity, and are not restated here.

## Triage grade

**C.** C, and it should probably be declined rather than queued. `missing` names a release calendar and multi-instrument configs, both real, but the deeper obstruction is that the paper makes no directional claim, so there is nothing for the funnel to gate — a registration needs an entry rule and a second-moment result does not supply one. One of its two instruments has also been retired by the LIBOR transition. Promote it, if at all, folded into the abstention hypothesis.
