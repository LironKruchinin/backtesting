---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: commodity-shock-efficiency
topic: short-horizon-reversal
grade: A
hypothesis_family: commodity-one-day-shock-reaction
status: draft
created: 2026-08-06
doi: 10.22004/ag.econ.169763
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Reaction to large one-day shocks in commodity futures

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

Khelifa Mazouz, Jian Wang, Mazouz, Khelifa, Wang, Jian. *Are commodity futures markets short-term efficient? An empirical investigation*.
AgEcon Search (University of Minnesota, USA), 2014.
DOI `10.22004/ag.econ.169763`. <https://openalex.org/W2108852573>
Retrieved from the openalex API on 2026-08-06.

The paper tests how eighteen commodity futures behave after unusually large single-day moves. A naive mean-adjusted abnormal-return specification flags apparent over- or under-reaction in a minority of the contracts. Once systematic risk and time-varying variance are accounted for, the paper reports that essentially all of them react efficiently. In other words: the headline finding is a null, and the interesting part is that the effect visible in the simple specification does not survive a better one.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.22004/ag.econ.169763':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This is the rare registration where the source paper's own conclusion is that nobody is paying. That deserves to be taken at face value rather than argued around. The candidate payer, had the effect been real, would have been the liquidity provider forced to warehouse inventory through a shock and needing compensation to do it — which predicts continuation over minutes to hours, not a multi-day drift, and would not show up at daily grain anyway. The paper's own result suggests the apparent effect in the naive model was volatility clustering: a large move is followed by large moves in both directions, and a specification that assumes constant variance misreads dispersion as drift. So the honest framing is that this is a falsification exercise, not an idea. Running it is worth the compute only because a confirmed null on our own archive is a real research output — and because a positive result here would be a red flag about our machinery long before it was a discovery.

## Signal in Crucible terms

- Instrument: `CLM2024` and `GCM2024`, separate configs, four-digit keys.
- Timeframe: `1d` for the faithful version — trading-day bars opening 17:00 CT the previous evening, resampled on read (D-0077).
- `[indicators.shock] kind = "zscore"`, `period = [10, 20, 40]`, `source = "return"` — a standardised one-day move, which is the paper's 'shock' definition as closely as this grammar allows. Note `source = "return"` costs one extra warmup bar and it is declared, not absorbed (D-0080).
- `enter_long = "shock crosses_below -2.0"`, `exit_long = "shock >= 0.0"`, `enter_short = "shock crosses_above 2.0"`, `exit_short = "shock <= 0.0"` — the reversal reading; the continuation reading is the same construction with the signs swapped and is a separate registration.
- Threshold axis `[1.5, 2.0, 3.0]` as an explicit list (D-0060).
- Arithmetic that would be needed and is not available: the abnormal-return model's own risk adjustment. We can standardise a return by its trailing dispersion and nothing more, which means we are running the naive specification the paper says produces a false positive — a fact that must be printed on the scorecard, not buried.

## Data

- CL and GC hold curated 1-minute bars 2010-06-06 → 2026-07-28, resampled to `1d` on read.
- The arithmetic is brutal and must be stated up front: one contract's active life is roughly sixty sessions, so a `1d` config sees roughly sixty bars, of which a 20-period z-score consumes twenty in warmup. A two-sigma daily event on forty remaining bars occurs one or two times. There is no verdict to be had.
- Running the same construction at `1h` would give a workable bar count, but an hourly shock is not the paper's object and would be a different hypothesis wearing its name.
- Their eighteen commodities include agricultural and softs markets we hold nothing for. We have exactly two of their asset groups.
- `half_spread_ticks = 1` is an assumption for both roots and always will be (D-0120).

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: raised above the batch default because the signal is a tail event. At a two-sigma threshold, 500 sessions produce on the order of twenty-five events, which is the bare minimum for the trade floor below to mean anything.
- `min_oos_trades = 100` — basis: follows directly from the session floor and the event rate; a lower number would let a handful of 2020 crude days determine the whole result.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills.
- `kill_if_dead_at_ticks = 2.0` — basis: deliberately loose. A daily-grain strategy trades rarely, so cost is not the binding constraint here; setting this tight would kill the idea for the wrong reason and obscure the sample-size verdict that should kill it.
- `max_permutation_p = 0.05` — basis: the paper's own null is that shocks are followed by nothing. A permutation null is the same statement, and a large p-value here confirms the paper rather than embarrassing us.
- `require_controls_beaten = true` — basis: gold rose across most windows in the archive, so the buy-and-hold control is a live threat to any long-biased GC rule, and the matched random-entry control is the honest comparator for a tail-event trigger.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- AgEcon Search is a working-paper repository at a university library, not a refereed venue. There is no peer review behind this record.
- The paper's own bottom line is efficiency. Registering it means registering an idea whose authors concluded there is nothing there, and the expected outcome is `Kill`. That is the correct result and should not be presented as a disappointment.
- Their universe is eighteen commodity futures over a long daily sample; we hold two commodity roots and sixteen years. Even a positive result would not be a replication.
- We cannot reproduce the risk and heteroskedasticity adjustments that turned their result from something into nothing — the grammar has no arithmetic between operands. So we can run only the specification they identified as misleading, which is a limitation worth stating loudly rather than a detail.
- The daily grain plus a single contract's life makes the effective sample smaller than any other candidate in this batch. The sample gate will fire first and hardest, and it will be right.

## Triage grade

**A.** Expressible: a trailing `zscore` on `source = "return"` over resampled trading-day bars, with threshold crossings for entry. The grammar covers it. But runnable is not answerable, and here the gap is worse than usual — one contract's roughly sixty sessions become roughly sixty daily bars, of which warmup eats a third and a two-sigma trigger fires once or twice. The sample gate kills it correctly, and only registry pooling across contracts changes that.
