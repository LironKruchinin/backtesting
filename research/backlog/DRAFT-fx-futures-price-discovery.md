---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: fx-futures-price-discovery
topic: cross-asset-lead-lag
grade: C
hypothesis_family: 6e-spot-futures-price-discovery
status: draft
blocked_on: interdealer spot FX data (EBS) — not owned, not acquirable through the vendor this archive uses, and no milestone plans it
created: 2026-08-06
doi: 10.1002/fut.20352
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Which venue prices the euro first — CME futures or interdealer spot

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

Juan Cabrera, Tao Wang, Jian Yang. *Do futures lead price discovery in electronic foreign exchange markets?*.
Journal of Futures Markets, 2008.
DOI `10.1002/fut.20352`. <https://doi.org/10.1002/fut.20352>
Retrieved from the crossref API on 2026-08-06.

Across three electronically traded venues for two major currencies, the study measures how much of each venue's price movement is permanent information rather than transient noise, and reports that over the window examined the interdealer spot venue was where new information showed up first. The full-size and the smaller futures contract are reported as contributing no more than one another. It is a measurement of where information arrives, not a trading rule, and the paper does not offer one.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.20352':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

If one venue prices new information before another, the slower venue's resting quotes are stale for as long as the lag lasts, and whoever is quoting there pays for it. So the payer is nameable — a futures market maker who has not yet seen the spot print — and that is also the most heavily industrialised trade in electronic markets. It is fought over in microseconds by firms with cross-connected fibre and hardware in both data centres, and it has been fought over since well before this archive begins. The honest addition is therefore not that the payer is unknown but that we are certainly not the one collecting: at one-minute resolution the lag the paper measures has opened and closed thousands of times inside a single bar. Whatever a one-minute bar can still see is not a latency edge; it is what is left over after every latency participant has taken their fill.

## Signal in Crucible terms

- Instrument: the 6E chain (`6EM2024` and its neighbours) is the only FX in the archive. Timeframe `1m`, which is already coarser than the phenomenon.
- The construction would need two synchronised series — a spot mid and a futures mid — and the grammar has no operand that reads a second instrument. One instrument and one timeframe per config, by design.
- Even given both series, the rule is a difference or a ratio between them, and the grammar admits no arithmetic between operands at all.
- A degenerate single-instrument version is expressible — `zscore(20, return)` on 6E with `enter_long: zscore_return crosses_below -2.0` — but it tests 6E's own autocorrelation, not a cross-venue lead. Filing it under this family would charge trials for a question nobody asked.
- The claim lives below the bar. Curated grain is one minute for every root, and the only book data in the archive is ES.

## Data

- Owned: 6E `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one-minute; `5m`/`15m`/`1h`/`1d` aggregate on read from those bars.
- Not owned and not obtainable through this vendor path: EBS or any other interdealer spot series. No milestone plans one, and `missing` says so.
- No L1 for 6E and none acquirable — the entitlement lapsed, and the archive's only `tbbo` and `trades` records are ES for a single year (D-0120).
- The one month of `mbo` is also ES only, so even the single root with book data cannot be pointed at this question.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.05` for the spot-change predictor at a declared one-minute forward horizon. Basis: a seeded random walk of 20,000 bars gives away roughly 0.04 for free (D-0085), so a bar under that is the noise floor wearing a threshold's name.
- `max_permutation_p = 0.01`, block length declared before the run and swept over (D-0087). Basis: a minute-scale claim on an autocorrelated series is exactly where a single unswept block length hides a parameter inside the p-value.
- `kill_if_dead_at_ticks = 0.5`. Basis: a cross-venue lag is a fraction of a tick per event by construction, so half a tick — deliberately off the grid (D-0073) — is where the question is actually decided, not one tick.
- `min_oos_trades = 500` and `min_oos_sessions = 250`. Basis: a microstructure claim is a claim about a rate of events, so it needs a year of sessions; one 6E contract cannot supply that, so this gate kills the run until registry pooling lands, which is the correct answer today.
- `require_controls_beaten = true`. Basis: the matched random-entry control is the median of sixteen draws, and an entry rule carrying no information should lose to it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is a real derivatives journal, but this is a microstructure measurement paper rather than a strategy paper: it makes no claim that its finding survives costs, because it never asks. Turning a discovery-share statistic into a rule is our step, not theirs, and the failure would be ours too.
- Their sample is the electronic FX market of the mid-2000s. Our archive opens 2010-06-06 and holds none of the spot leg, so there is no window in which their comparison could be reproduced here at all.
- The market is half ours: we hold the CME futures leg and will never hold the venue the paper reports as leading.
- The paper reports its own information-share figures for each venue; they are its measurements on its sample, they are not restated here, and none of them describes anything this build would produce.
- Every cost number in this project rests on `half_spread_ticks = 1`, an assumption rather than a measurement, and for 6E it always will be (D-0120). A claim that lives inside the spread cannot be judged by a build that guesses the spread.

## Triage grade

**C.** C, and the missing piece is not code. `missing` names an interdealer spot series this archive does not hold, cannot buy through its vendor path, and no milestone plans. Building it would mean a second provider, a second availability rule, and a clock-synchronisation convention between two venues — a data-acquisition project, not a config edit. The FX futures leg we already own is one half of a two-sided comparison, and half a comparison is not a test.
