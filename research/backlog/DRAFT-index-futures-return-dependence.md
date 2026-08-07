---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: index-futures-return-dependence
topic: short-horizon-reversal
grade: A
hypothesis_family: equity-index-short-return-dependence
status: draft
created: 2026-08-06
doi: 10.2139/ssrn.314888
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Five-minute serial dependence in index futures

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

Shiyun Wang. *Dependence of the Intraday Nikkei Stock Index Futures*.
venue unrecorded, 2002.
DOI `10.2139/ssrn.314888`. <https://doi.org/10.2139/ssrn.314888>
Retrieved from the crossref API on 2026-08-06.

Using a Markov-chain treatment of intraday Nikkei index futures, the paper reports that the current five-minute move carries information about the next two five-minute intervals but not the third, so a random walk is rejected at the shorter horizons and not at the longer one. It also reports that the sign of the dependence flips between the five- and ten-minute views, and raises the bid-ask bounce, overreaction and a mean-reverting price component as competing explanations without settling among them.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.314888':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The paper itself lists the candidates and declines to choose, which is the honest position and also the reason to be pessimistic. If the dependence is bid-ask bounce, there is no payer at all: it is an artefact of transaction prices alternating between two quotes, and it disappears the instant you have to cross the spread to act on it. If it is genuine overreaction, the payer is the impatient participant who moved the price further than the information warranted and must be filled back — small, real, and exactly what a market maker is compensated for capturing. If it is a mean-reverting component in the underlying, the payer is whoever traded on transient index-arbitrage pressure. We cannot separate these from OHLCV bars. We hold `tbbo` for ES only, over one year, which is the sole window in the archive where the bounce question could even be asked. Registering this means registering a test we already know is confounded, and saying so before the run rather than after.

## Signal in Crucible terms

- Instrument: `ESM2024` — one raw equity-index contract, four-digit key. `NQM2024` as a second registration; `RTY` only for contracts from 2017 onward.
- Timeframe: `5m`, aggregated on read from curated 1-minute bars on the exchange's own sessions (D-0077) — so the buckets are session-anchored, not wall-clock-anchored as the paper's presumably were.
- `[indicators.stretch] kind = "zscore"`, `period = [12, 24, 48]`, `source = "close"` — 12 five-minute bars is one hour of context.
- `enter_long = "stretch crosses_below -1.5"`, `exit_long = "stretch >= 0.0"`, `enter_short = "stretch crosses_above 1.5"`, `exit_short = "stretch <= 0.0"`.
- The paper's core structure — dependence at lags one and two but not three — is a fixed-horizon statement, and the grammar has no bar counter. There is no way to write 'hold exactly two bars'. The z-score-return exit is a proxy with a different, path-dependent holding time, and the draft must not claim otherwise.
- An S0 registration is the better shape for this paper's actual claim: a forward-return information coefficient at a declared horizon measures serial dependence directly, without needing a tradeable rule at all.

## Data

- ES and NQ hold curated 1-minute bars 2010-06-06 → 2026-07-28; RTY from 2017, because the contract did not list on CME earlier.
- `5m` is resampled on read, never stored. `curated/bars/ESH2024/5m` does not exist and its absence is not an unfinished transcode.
- The equity-index calendar carries session eras (D-0086): the 15:15–15:30 CT halt exists in era 3a and was removed effective 2021-06-28. Any five-minute bucket study crossing that date is studying two different session shapes.
- Missing: the Nikkei. We hold no Japanese contracts, no Osaka or SGX data, and no cash index series of any kind.
- `tbbo` exists for ES only, 2025-07-28 → 2026-07-28 — the only window where the bid-ask-bounce confound could be examined at all, and it is one year long.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.03` at a two-bar forward horizon, with S0's bootstrap interval on the mean forward move required to exclude zero at the same horizon (D-0085) — basis: this is the paper's claim stated as a measurement rather than as a rule, and it is the criterion that most directly tests it.
- `kill_if_dead_at_ticks = 0.5` — basis: as with the FX candidate, the mechanism most likely to be generating the statistic is the spread itself. Half a tick is where a bounce artefact dies and a real overreaction does not.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills.
- `min_oos_trades = 400` — basis: a 1.5-sigma fade on five-minute bars fires several times a session; a lower floor would mean the grid wandered to thresholds that trade almost never, which is a different strategy.
- `min_oos_sessions = 250` — basis: one pooled trading year, unreachable on a single contract by construction.
- `max_permutation_p = 0.05` — basis: block permutation destroys serial dependence while preserving distributional shape, which makes it the exactly-matched null for a serial-dependence claim. If the p-value is large, the claim is dead in our sample and there is nothing to argue about.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The market studied is the Nikkei 225 futures contract. It is not a market we trade, on an exchange we hold no data for, and we cannot test the paper's claim — only the same question on a different index in a different decade.
- The record has no venue at all: an SSRN working paper from 2002, apparently never published. Twenty-four years old and unrefereed.
- Their sample predates ours entirely. Our archive begins 2010-06-06; the electronic microstructure of index futures changed beyond recognition between the two.
- A Markov-chain discretisation of returns throws away magnitude and keeps only sign-and-state. Rejecting a random walk in that reduced representation is a weak test, and rejecting it at two lags but not three is the shape of a result that has been searched over lags.
- The paper's own abstract raises bid-ask bounce as a possible cause and does not rule it out. That is a confound we also cannot rule out for six of seven roots, and `half_spread_ticks = 1` is an assumption rather than a measurement (D-0120).

## Triage grade

**A.** Expressible as a `zscore` fade on a resampled `5m` ES contract, plus an S0 information-coefficient registration that fits the paper's claim better than any rule does. Nothing is missing from the grammar. What is missing is sample: a grade-A config replays one contract's active life, roughly sixty sessions for ES, so this will be killed on the sample gate — correctly and by the machine — until registry pooling across contracts lands.
