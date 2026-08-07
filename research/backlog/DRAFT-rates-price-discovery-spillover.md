---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: rates-price-discovery-spillover
topic: cross-asset-lead-lag
grade: C
hypothesis_family: rates-derivatives-price-discovery
status: draft
blocked_on: interest-rate swap data and multi-instrument configs; the archive's rates holding is ZN futures alone
created: 2026-08-06
doi: 10.1057/s41599-024-02788-x
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Do bond futures or swaps price the curve before the cash market

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

Congxiao Chen, Wenya Chen, Li Shang, Haiqiao Wang, Decai Tang, David D. Lansana. *Price discovery and volatility spillovers in the interest rate derivatives market*.
Humanities and Social Sciences Communications, 2024.
DOI `10.1057/s41599-024-02788-x`. <https://openalex.org/W4392378045>
Retrieved from the openalex API on 2026-08-06.

Working on one national market's government bond futures, its interest-rate swaps and the underlying cash bonds, the study fits an information-share model and a spillover index. It reports that both derivative markets incorporate information ahead of the cash market, that the cash series carries structural breaks the derivatives do not, and that the futures leg is a net transmitter of volatility to the others. The market examined is China's, which the abstract states in its closing sentence.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1057/s41599-024-02788-x':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

No payer is named and none is implied. An information share says where a common price first moves; a spillover index says variance travelled between markets. Neither asserts that a participant took the wrong side repeatedly, which is what a strategy has to assert. The nearest tradeable version would be that the cash market lags long enough to be traded against — but that is a two-leg trade in an instrument this archive does not hold, on an exchange this project does not touch, and it requires a financing and repo treatment no fill model here contains. The one participant who could be systematically paying in such a setup is a cash-bond holder rebalancing at published levels rather than at the derivative's, and nothing in the metadata tells us whether such a participant exists in the market studied, or whether the lag is wide enough to clear a bid-offer. Treat the payer as unnamed.

## Signal in Crucible terms

- The paper's instruments are a national treasury futures contract, a swap curve and cash bonds. The archive holds `ZNH2024` and the rest of the ZN chain — a CBOT contract on a different sovereign — and nothing else in rates.
- The construction compares two price series at a common timestamp. A config reads one instrument, and the grammar has no arithmetic to form the difference even if it read two.
- An information share is estimated from a cointegrating system fitted over the sample. Fitting on everything and then trading it is the lookahead §2.1 names by name; a runnable version would have to be a trailing-window statistic, and every statistic in the grammar already is one.
- What could be run on `ZNH2024` alone is a single-series trend or reversion rule, which tests neither leg of this claim.

## Data

- Owned: ZN `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one minute. It is the only rates root and it is on the wrong sovereign for this paper.
- Not owned: interest-rate swap rates or fixings in any currency, cash treasury prices, and any repo or financing series.
- Not owned: any instrument from the exchange the paper studies. That is not a gap a config or a milestone closes.
- No L1 for ZN and none acquirable (D-0120), so the spread half of any lead-lag question here is an assumption rather than a measurement.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.05` for the derivative-lead predictor at a declared horizon. Basis: below roughly 0.04 the number is what noise supplies at this sample length (D-0085).
- `max_permutation_p = 0.05`, block length declared and swept (D-0087). Basis: rates series are strongly autocorrelated, so the block scale carries the whole argument and a single unswept value buries a parameter inside the p-value.
- `min_oos_sessions = 500`. Basis: the paper's own result is that the lead structure changed across sub-periods, so a sample short enough to sit inside one regime cannot test the claim it makes.
- `kill_if_dead_at_ticks = 1.0`. Basis: ZN's tick is a small fraction of a point and a cross-venue lead is small by construction, so one tick is where this is decided rather than a formality.
- `require_controls_beaten = true` and `max_pbo = 0.5`. Basis: horizon and window are free parameters, and CSCV is what prices a rule chosen across them.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The market studied is not one we trade. The abstract's own conclusion is about China's interest-rate derivatives market; our rates holding is a CBOT contract on US treasuries. Different sovereign, different exchange, different participation rules, different hours. Neither the paper nor this file establishes anything about transferability.
- The venue is a broad-scope multidisciplinary title covering humanities and social sciences, not a finance or econometrics journal. Under this project's working prior that counts against, rather than being neutral.
- The sample window is not in the metadata we hold. Whoever promotes this must read the paper for it and record the overlap against 2010-06-06 → 2026-07-28 before any run.
- Information-share and spillover statistics carry no cost treatment. The paper offers no execution assumption and makes no claim about net-of-cost tradeability, so the leap to a strategy is entirely ours.
- The paper reports its own information shares and spillover magnitudes; they are its measurements on its market and none describes a Crucible output.

## Triage grade

**C.** C, and closer to a decline than to a queue. `missing` names swap data and multi-instrument configs, but the deeper problem is the asset itself: the subject is a market on another exchange whose instruments this project does not hold and has no route to. Even if swap curves appeared tomorrow, reproducing the claim would mean acquiring a foreign sovereign's futures. What ZN can answer is a different question in a different market, and should be registered as such.
