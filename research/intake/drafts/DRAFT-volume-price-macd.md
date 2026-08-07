---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: volume-price-macd
topic: volume-price
grade: B
hypothesis_family: equity-index-volume-adjusted-macd
status: draft
blocked_on: arithmetic between operands — MACD is a DIFFERENCE of two moving averages, and the grammar compares operands but never combines them
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — A volume- and range-adjusted moving-average oscillator

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

Luyun Lin, Lixing Lin, Zhen Zhang, Moxuan Zheng, Yiqing Wang. *A Volume-Price-Adjusted MACD Trading Strategy with Sensitivity Calibration for U.S. Equity Indices*.
arXiv q-fin, 2026.
**no DOI** (preprint). <http://arxiv.org/abs/2604.26063v1>
Retrieved from the arxiv API on 2026-08-06.

The authors argue that standard MACD rules react late and fire on noise, and build a variant that folds volume, a volatility measure and the bar's own range into the oscillator, plus a tunable knob that lets entries trigger sooner. They calibrate on 2018 through 2022 US index data and evaluate on 2023 through early 2026, reporting the variant ahead of plain MACD while trading less often.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == None:
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The counterparty cannot be named, and no attempt is made to name one. A difference of two exponential averages is a trend-speed proxy; scaling it by participation and by the bar's own range is an attempt to separate moves that many people actually traded from moves that drifted on nothing. That intuition is not unreasonable, but it is also not a claim that anybody is systematically paying a moving-average trader, and after fifty years of published moving-average variants the prior on the next one is poor. Worse, the sensitivity knob is the mechanism: it is fitted on a calibration window, and a parameter whose entire purpose is to make entries earlier will always find a setting that made entries earlier in the sample where it was chosen. Absent an identified payer this is shape-fitting, and the grade follows from that fact rather than from the size of the code gap.

## Signal in Crucible terms

- Instruments `ESZ2024` and `NQZ2024`, timeframe `1h` or `1d`. The paper's third index is the Dow, and there is no YM contract in this archive.
- The construction WOULD be: `macd = ema(12) - ema(26)`, a signal line `ema(9)` over that difference, then a volume and range adjustment applied multiplicatively, with the sensitivity knob shifting the crossing threshold.
- Where it breaks: every one of those steps is arithmetic between operands. The grammar compares operands with `<`, `>`, `crosses_above` and friends; it never subtracts, divides or multiplies them, so neither the difference nor the adjustment can be written.
- The nearest expressible thing is `enter_long: ema_12 crosses_above ema_26` — which is `SmaCross` with different periods, and `SmaCross` is the reference fixture chosen for simplicity rather than merit. Running it under this file's name would be substituting the null for the hypothesis.
- The volume input the paper uses is consolidated index volume. Ours is a single futures contract's contract count, which is a different quantity with a different meaning, so even with arithmetic the input would not match.
- A range term needs `high - low`, which is the same arithmetic gap by another route.

## Data

- Owned: curated 1-minute ES and NQ bars from 2010-06-06 to 2026-07-28, covering both the paper's calibration and evaluation windows with a decade to spare.
- Not owned: any Dow contract, and no cash index series at all. Two of the paper's three indices have a futures counterpart here; the third does not.
- Not owned: consolidated equity volume. Futures contract volume is a different series and rolls between contracts, which the paper's input does not do.
- Owned: `tbbo` for ES only and for 2025-07-28 to 2026-07-28, which overlaps a sliver of the paper's evaluation window and nothing of its calibration window.
- No acquisition is required; the gap is entirely a grammar extension.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The adjusted oscillator must beat a plain two-EMA crossing on `min_oos_sharpe_after_costs` by a registered margin, else Kill — basis: the paper's claim is that the adjustment adds something, so a run where the plain version does as well has refuted it even if both are profitable.
- `max_permutation_p = 0.05` — basis: moving-average variants are the single most mined family in technical trading, and a block-permutation null is the only cheap test that asks whether the result survives when the future is shuffled.
- `require_plateau = true` over the sensitivity knob and both EMA lengths — basis: a result at one knob setting with nothing on either side is a spike, and this parameter was introduced specifically to be tuned.
- `max_pbo = 0.5` — basis: three tunable lengths plus a sensitivity knob makes a large grid, and the backtest-overfitting probability is the statistic that reads grid size directly.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: the paper reports fewer signals as a feature, and a rule that trades rarely needs more sessions rather than fewer to reach a countable sample; one contract reaches neither.
- `require_controls_beaten = true`, buy-and-hold in particular — basis: the evaluation window is 2023 to 2026, during which US equity indices rose a great deal, so a long-biased trend rule will look good against nothing.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- This is an arXiv preprint with no peer review, five authors and a 2026 date. The calibration and evaluation windows are adjacent slices of the same rising market, which is the weakest possible form of an out-of-sample test.
- Three highly correlated US equity indices are close to one observation, not three. The S&P, the Nasdaq-100 and the Dow move together, so agreement across them is not independent confirmation.
- The paper's instruments are cash indices, which nobody trades directly. A futures implementation pays basis and roll costs that the paper's arithmetic never sees.
- The paper reports its own performance figures; they are not restated here.
- The sensitivity knob is described as allowing earlier entry. A parameter that makes a rule act sooner will look better on any sample with a trend in it, and the two windows here both have one.

## Triage grade

**B.** The data is owned for two of the three indices; the missing piece is arithmetic between operands, which is the single largest gap in the combo grammar. Adding it means an expression layer — parsing, evaluation, and the canonical rendering that D-0012's config hash is computed over — plus its effect on grid expansion and on how a config's identity is spelled. That is a schema change with a decision-log entry behind it, not a new indicator, and it unblocks many files at once rather than this one.
