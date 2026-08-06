---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: volume-serial-correlation-crude
topic: volume-price
grade: A
hypothesis_family: cl-volume-conditioned-return-persistence
status: draft
created: 2026-08-06
doi: 10.1142/s242478632150016x
source_api: crossref
harvested_from: crossref, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Volume as a switch between continuation and reversal in crude

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

Hua Wang, Weige Huang. *Trading volume and serial correlation in crude oil futures returns*.
International Journal of Financial Engineering, 2021.
DOI `10.1142/s242478632150016x`. <https://doi.org/10.1142/s242478632150016x>
Retrieved from the crossref API on 2026-08-06.

Working with high-frequency crude oil futures data, the authors report that trading volume forecasts the sign of return autocorrelation: inside roughly an hour it points to continuation, at longer intraday horizons it points to reversal, and beyond a day it says nothing stable. They present the result as holding up under a range of controls.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1142/s242478632150016x':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Two different payers, one on each side of the horizon flip, which is what makes this worth compute rather than merely interesting. Inside the hour, heavy volume marks information arriving and being absorbed gradually; the payer is the participant reading the same news late and buying after the move, and he keeps paying because being slow is a structural condition rather than a decision. Over the longer horizon, heavy volume marks somebody demanding liquidity in size; the payer is that trader, and the compensation flows to whoever absorbed the inventory and now needs the price back where it started. Urgency is worth money, and a fund or a refiner that must move today books the reversal as an execution cost rather than a loss. The mechanism carries its own warning: the sign flips, so a wrong holding period makes the identical signal take exactly the wrong side.

## Signal in Crucible terms

- Instrument `CLM2024`, timeframe `5m`. One instrument and one timeframe per config, and it must be a raw contract — continuous aliases are refused for the grid commands.
- Volume state slot: `zscore(48, volume)` — a trailing four-hour window on 5-minute bars, giving a comparable reading of whether the completed bar traded heavily.
- Continuation arm (the within-hour leg): `enter_long: vol_z > 1.5 and close crosses_above ema_12`, `exit_long: close crosses_below ema_12`, with `enter_short: vol_z > 1.5 and close crosses_below ema_12` and the symmetric exit.
- Reversal arm (the mid-horizon leg, a SEPARATE config under the same family): `enter_short: vol_z > 1.5 and close crosses_above bollinger_96.upper`, `exit_short: close crosses_below bollinger_96.mid`, with the symmetric long side.
- Fidelity caveat that does not break grade A: the grammar has no holding-period parameter, so the horizon is expressed only through the length of the indicator in the exit rule. That is a proxy for the paper's horizon, not the horizon itself, and the report must say so.
- The volume operand is the completed bar's contract count (D-0079), so nothing here reads a partial bar and nothing needs a session aggregate.

## Data

- Owned and sufficient: curated 1-minute CL bars from 2010-06-06 to 2026-07-28, aggregated to `5m` on read against the energy calendar's own sessions.
- Owned: an energy calendar (D-0089), so session clock readings and the resample grid are anchored on the real open rather than on midnight UTC.
- Not owned: any trade-direction signing. Volume here is unsigned contract count, which is what the paper used, so this is a match rather than a gap.
- Not owned: a measured spread for CL, ever. The L1 entitlement lapsed and CL has no `tbbo`, so `half_spread_ticks = 1` is a permanent assumption (D-0120) and one tick on CL is $10.
- Sample ceiling: one raw contract per config, and CL lists monthly rather than quarterly, so a single config's liquid window is shorter than the ES case rather than longer.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- S0 first: `min_abs_ic = 0.02` at horizons of 5, 15, 30 and 60 minutes, AND the mean forward return's bootstrap interval must exclude zero at the same horizon — basis: D-0085, because a magnitude on its own reads well above 0.02 on pure noise given enough bars.
- The sign-flip gate, which is the one that can kill this even if it makes money: the information coefficient must be positive at the short horizon and negative at the longer one. Same sign at both, or no significance at either, is Kill — basis: the mechanism on trial is the flip, and a profitable run without it is a different finding that needs its own registration.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: 250 sessions is the smallest pooled window in which a per-bar conditioning rule has a countable sample; a single CL contract does not reach it.
- `min_oos_sharpe_after_costs = 0.5` — basis: the backlog's constant floor, so this file is not graded on a softer scale than its neighbours.
- `kill_if_dead_at_ticks = 1.0` — basis: a 5-minute rule pays the spread often, and the reversal leg's whole mechanism is that somebody else paid for immediacy; if we cannot survive one tick we are the one paying.
- `max_permutation_p = 0.05` and `require_controls_beaten = true` — basis: volume-conditioned entry rules are a large family and the matched random-entry control is the cheapest way to find out whether we simply traded more often at busy times.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper is in the International Journal of Financial Engineering, a minor outlet, and neither the sample period nor the exchange is stated in the abstract the index returned — so we do not actually know whether their crude is CME WTI or something else.
- A sign that flips with horizon is exactly the shape a fishing expedition produces: two horizons, two signs, and one of them will fit. The paper describes it as robust to controls, but robustness checks are chosen by the same people who chose the specification.
- Our cost side rests entirely on `half_spread_ticks = 1` for CL and always will. If the mid-horizon reversal is the size of the spread, it is real, publishable and untradeable from here, and this file would rather report that than discover it later.
- CL over 2010 to 2026 includes 2014-16 and the April 2020 negative settlement, both of which are regime breaks large enough that a volume-conditioned rule may be measuring two different markets.
- The paper reports its own statistical figures; none are restated here, and none of them forecasts anything about what this engine would produce.

## Triage grade

**A.** Every operand exists: `zscore(48, volume)`, price fields of the completed bar, `ema`, `bollinger`, and the comparison set. It runs this week against curated CL with no new Rust. But runnable is not answerable — one raw contract is a short window, shorter for CL than for a quarterly root, and no sample floor worth registering is satisfiable at that length. Today's run is guaranteed to be killed for sample size, correctly, by the machine, until registry pooling across contracts lands. It is triage, not a verdict.
