---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: es-intraday-liquidity-decay
topic: intraday-seasonality
grade: A
hypothesis_family: es-intraday-range-decay
status: draft
created: 2026-08-06
doi: 10.2139/ssrn.6847024
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Session-time conditioning on ES from the open decay pattern

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

Max Brown. *Intraday Microstructure Dynamics of E-mini S&amp;amp;P 500 Futures: Volatility Regimes, Liquidity Decay, and Long Memory in Realized Volatility*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6847024`. <https://doi.org/10.2139/ssrn.6847024>
Retrieved from the crossref API on 2026-08-06.

A descriptive study of one-minute ES bars over roughly two years and about five hundred sessions. Activity and bar range are heaviest right after the open and fall steadily as the day wears on; a small number of separate volatility regimes are identified, with a 2025 tariff episode as the extreme one; daily realized variance is strongly persistent out to about a month; and average bar range tracks session realized variance closely. It ends by arguing that designs should look at the opening window's range and at the prevailing volatility state before taking a side.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6847024':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A shape in volatility is not a trade, so the descriptive part of this paper has no counterparty by construction. The tradeable reading is the conditioning suggestion it closes on: if participation and price movement are concentrated in the first part of the session, then a directional rule evaluated in that window is operating where information is actually arriving, while the same rule later in the day is operating on drift. The candidate payer is the liquidity provider quoting into the opening backlog and being run over by orders that queued overnight. That participant is real, but he also knows this — the opening spread is the widest of the day precisely because he prices the risk. So the mechanism predicts that the session-time conditioning survives and the direction does not, and the gates below are built to separate exactly those two outcomes rather than to reward either one.

## Signal in Crucible terms

- Instrument `ESM2025`, timeframe `5m`, aggregated on read from curated 1-minute bars. The paper's window (April 2024 to May 2026) sits inside our span, which is unusual in this batch.
- Opening-window arm: `enter_long: minutes_since_rth_open < 30 and close crosses_above bollinger_12.upper`, `exit_long: minutes_since_rth_open >= 60 or close crosses_below bollinger_12.mid`.
- Symmetric short: `enter_short: minutes_since_rth_open < 30 and close crosses_below bollinger_12.lower`, `exit_short: minutes_since_rth_open >= 60 or close crosses_above bollinger_12.mid`.
- Falsifier arm, a second config under the same family with identical parameters: replace the entry gate with `minutes_since_rth_open > 210`. If the late-session arm does as well, time of day is not the mechanism and the finding is something else.
- Fidelity caveat that does NOT block a grade A run: the paper's headline variable is mean bar range, which is `high - low` and therefore arithmetic the grammar cannot write. The runnable version conditions on session time, not on range, and the report must say which of the two claims it actually tested.
- `minutes_to_close` shortens on an early close while `minutes_to_rth_close` counts to the scheduled one (D-0078); the rules above use the RTH readings deliberately, since the paper's shape is about the regular session.

## Data

- Owned: curated 1-minute ES bars from 2010-06-06 to 2026-07-28, with the `5m` grain aggregated on read against the equity-index calendar's own sessions.
- Owned: the equity-index calendar with session eras (D-0086), so `minutes_since_rth_open` is anchored on the real open and honours the era change rather than a fixed UTC offset.
- Owned, partially: ES `tbbo` for 2025-07-28 to 2026-07-28 — one year, overlapping the tail of the paper's window. It is the only place in the archive where the intraday spread shape could be measured rather than assumed.
- Not owned: a measured spread for the other fifteen years. `half_spread_ticks = 1` is a flat assumption (D-0120), which for an opening-window strategy is the least defensible place to apply a flat number.
- Sample ceiling: one raw contract per config, roughly sixty sessions for a quarterly ES contract, against a paper that used more than five hundred.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- S0 first: `min_abs_ic = 0.02` at 5-, 15- and 30-minute horizons for bars inside the opening window, with the bootstrap interval excluding zero at the same horizon — basis: D-0085, since magnitude alone reads well above 0.02 on noise at this bar count.
- The discrimination gate, which can kill this even if it is profitable: the opening-window arm must beat the late-session arm by a registered margin. If the rule works equally well at 14:30, time of day is not doing the work and the hypothesis on trial has been refuted — basis: the mechanism is what is being tested, not the equity curve.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: at most a couple of opening-window trades per session, so 250 sessions is the smallest window with a countable sample; one ES contract reaches neither and will be killed for sample adequacy, correctly.
- `min_oos_sharpe_after_costs = 0.5` — basis: the backlog's constant floor, held fixed across the queue.
- `kill_if_dead_at_ticks = 1.0` — basis: this rule trades in the widest-spread half hour of the day by design, so the cost sweep is not a robustness check here, it is the decisive test.
- `require_controls_beaten = true` and `max_permutation_p = 0.05` — basis: a rule that trades a fixed slice of every session is easy to beat with random entries in the same slice, and the matched control is what shows whether the timing or the direction earned anything.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper is an SSRN working paper with no peer review and a single author. Its contribution is descriptive statistics on a window we already hold, which means we could rederive every one of its findings ourselves rather than taking them on trust.
- Its sample is roughly 529 sessions, April 2024 through May 2026 — about two years, containing one very large shock. A regime count from two years is a count of episodes, not of regimes.
- The paper reports its own descriptive figures for bar range and persistence; those are its measurements on its window and nothing here forecasts anything about what this engine would produce.
- The finding that opening activity is highest is among the most replicated facts in market microstructure. That it replicates again is not evidence that trading on it pays.
- Cost realism dominates. Every number we produce rests on `half_spread_ticks = 1`, and an opening-window rule pays the widest spread of the session — so the assumption is doing more work in this file than in any other grade-A entry in this batch.
- The tradeable reading is ours, not the paper's. It suggests conditioning; it does not claim a strategy.

## Triage grade

**A.** Every operand exists — `minutes_since_rth_open`, `is_rth`, `bollinger`, price fields of the completed bar — so it runs this week with no new Rust. But runnable is not answerable. One raw ES contract is roughly sixty sessions, and no sample-adequacy floor worth registering is satisfiable at that length, so today's run is guaranteed to be killed for sample size, correctly, by the machine, until registry pooling across contracts lands. What comes back is triage, not a verdict on the idea.
