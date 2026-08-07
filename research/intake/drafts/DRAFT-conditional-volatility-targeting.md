---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: conditional-volatility-targeting
topic: vol-managed-exposure
grade: A
hypothesis_family: futures-extreme-volatility-state-gate
status: draft
created: 2026-08-06
doi: 10.1080/0015198x.2020.1790853
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Changing exposure only in the volatility tails

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

Dion Bongaerts, Xiaowei Kang, Mathijs A. van Dijk. *Conditional Volatility Targeting*.
Financial Analysts Journal, 2020.
DOI `10.1080/0015198x.2020.1790853`. <https://openalex.org/W3083166940>
Retrieved from the openalex API on 2026-08-06.

The authors report that scaling exposure continuously by trailing volatility does not reliably help in global equity markets and can deepen the worst drawdowns. They propose instead changing exposure only when volatility sits in its high or low extreme, leaving the middle alone, and report that this version behaves better across major equity markets and momentum factors while keeping turnover and leverage modest.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1080/0015198x.2020.1790853':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Volatility clusters, so trailing volatility genuinely forecasts future volatility; what it does not do is forecast returns monotonically. The middle of the volatility distribution carries almost nothing, while the tails carry both the crash episodes and the grinding calm advances. Acting only in the extremes is therefore a claim that the information sits in the tails and the middle is turnover you should not pay for. The losing side is unusually easy to name here, and that is the main reason this idea has standing: mechanical volatility targeters must de-lever into exactly the high-volatility windows, every time, and risk-parity and volatility-control mandates make that selling non-discretionary. They keep doing it because the mandate is written as a volatility number rather than as a price. Anyone adding exposure in those windows is taking the other side of a forced seller. The catch is symmetrical — the same crowding argument says the trade decays as those mandates grow.

## Signal in Crucible terms

- One config per root, e.g. `ESM2024`, `CLZ2024`, `ZNZ2024`, at `timeframes = ["1d"]` (trading-day bars, opening 17:00 CT the evening before, D-0077).
- `[indicators.vol] kind = "stdev", period = [10, 20, 40], source = "return"` — the state variable, and `[indicators.trend] kind = "sma", period = [20, 50]` — the exposure core.
- `enter_long = "vol < 0.004 and close crosses_above trend"`; `exit_long = "vol > 0.012 or close crosses_below trend"`. The two thresholds are the low and high extremes, and the untouched middle is what makes this the conditional version rather than the continuous one.
- Mirrored for the short side. Both thresholds are enumerated as explicit float lists (D-0060), never stepped.
- The honest weakness, stated in the construction: the thresholds are ABSOLUTE constants because the grammar has no arithmetic to build a relative one, so each root and grain needs its own pair. `zscore(period, return)` normalizes the return, not the volatility, so it does not solve this.
- Comparison arm under the same family: the identical trend core with no `vol` terms. If the gate does nothing, that arm says so.

## Data

- Owned: all seven CME roots at 1-minute grain 2010-06-06 → 2026-07-28, resampled on read to 1h and 1d, which is the grain this idea belongs at.
- Owned: enough asset-class spread to say something about where the effect lives — three equity-index roots, energy, metals, FX and rates.
- RTY's archive begins in 2017 because the contract did not list on CME before then; a pre-2017 gap there is the instrument not existing, not a hole.
- Not owned: VIX or any implied-volatility series, so the forward-looking version of the state variable is unreachable. The state here is strictly trailing realized dispersion.
- Constraint: one raw contract per config, so covering seven roots means many configs, and every one is charged as a trial against `futures-extreme-volatility-state-gate`.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a volatility-state gate must see both tails, and a sample that contains no high-volatility episode has not tested the gate at all. Unreachable on one contract today.
- `min_oos_trades = 100` — basis: a gate this coarse fires infrequently by design, and below this the tail states are represented by single episodes.
- `min_oos_sharpe_after_costs = 0.3` — basis: below the shipped 0.5 because the object under test is a conditioner added to a core, not a standalone strategy.
- `kill_if_dead_at_ticks = 1.0` — basis: the paper's own selling point is low turnover, so a version that dies at one tick of half-spread was never the low-turnover thing it claims to be.
- The discriminator: the gated arm must beat the ungated arm on the identical window, and `require_controls_beaten = true` must hold. If the gate adds nothing over the plain core, volatility state carries no usable information at this grain and this is Killed.
- `require_plateau = true` over both thresholds and `max_pbo = 0.5` — basis: two thresholds plus a lookback is a three-dimensional search, and a result at one corner of it with nothing around it is what PBO and the plateau requirement exist to refuse.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Financial Analysts Journal, 2020 — a practitioner venue whose publication filter favours methods that improved on a standard, which is exactly the selection this backlog's working prior distrusts.
- Their universe is major equity indices and factor portfolios at daily or lower frequency over long histories. Ours would be single futures contracts over roughly 60 sessions each. Almost no overlap in construction.
- The paper reports its own performance figures; they are not restated here and none of them would be reproduced by this build.
- Conditional targeting has strictly more free parameters than plain targeting — two thresholds and a lookback instead of one target — and 'the plain version failed so we added conditions' is the canonical shape of a fitted improvement. The plateau and PBO gates above are registered against that specific worry.
- The absolute-threshold weakness is ours, not theirs: because the grammar cannot express a relative extreme, each root's thresholds are chosen by hand, and choosing them after looking at the data would be the failure pre-registration exists to prevent. They must be enumerated in the config before any run.
- All cost figures rest on `half_spread_ticks = 1` (D-0120), an assumption for six of the seven roots permanently.

## Triage grade

**A.** A: the conditional form is a boolean state gate, and boolean state gates are exactly what the grammar has — unlike the continuous version, which is not expressible at all. Runnable is not answerable: one raw contract is roughly 60 sessions against `min_oos_sessions = 250`, so the machine kills these runs for sample size, correctly, until registry pooling lands. The absolute thresholds are a real weakness, not a formality.
