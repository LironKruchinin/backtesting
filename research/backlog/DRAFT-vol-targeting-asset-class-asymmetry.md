---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: vol-targeting-asset-class-asymmetry
topic: vol-managed-exposure
grade: B
hypothesis_family: futures-vol-target-asset-class-asymmetry
status: draft
blocked_on: continuous position sizing — the grammar has boolean entries and a fixed contract count, so a scaled notional cannot be expressed
created: 2026-08-06
doi: 10.3905/jpm.2018.45.1.014
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Volatility scaling helps risk assets and not bonds, currencies or commodities

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

Campbell R. Harvey, Edward Hoyle, Russell Korgaonkar, Sandy Rattray, Matthew Sargaison, Otto Van Hemert. *The Impact of Volatility Targeting*.
The Journal of Portfolio Management, 2018.
DOI `10.3905/jpm.2018.45.1.014`. <https://openalex.org/W3125613883>
Retrieved from the openalex API on 2026-08-06.

Against a literature reporting that volatility-scaled equity exposure beats constant notional, the authors argue the result is confined to risk assets such as equities and credit and attribute it to the negative volatility-return relationship there. For bonds, currencies and commodities they report the effect is immaterial. Separately, they argue that scaling reduces the frequency of extreme outcomes in every class, because the worst episodes tend to arrive when volatility is already elevated and the scaled position is already small.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.3905/jpm.2018.45.1.014':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

For equities and credit, volatility rises as prices fall, so a volatility-scaled position is smaller after declines and larger through calm advances — arithmetically a slow trend overlay wearing risk-management clothing. That is why an improvement shows up for risk assets and not for bonds, currencies and metals, where the volatility-return link is weak or absent. Where a payer exists, it is whoever is forced to de-lever in the same windows for the same reason, so the scaled position buys from stressed sellers and sells back to them; and it is paid for in choppy markets, which is the trend premium's standard bill. What is notable is that this paper reports where the effect is absent, which runs against the usual publication incentive and is the main reason to take it seriously at all. The tail-reduction half is close to mechanical — nearly any de-levering rule reduces extremes — and deserves far less weight than the asymmetry.

## Signal in Crucible terms

- The traded object is a continuously scaled notional: contracts held proportional to an inverse trailing volatility. The grammar has boolean entries and a fixed `qty_contracts`, so this cannot be written at all.
- The nearest expressible thing is a two-state gate, which is the previous candidate's hypothesis and a materially different claim — the asymmetry across asset classes is what this one is about, and a coarse gate does not test it.
- What it would cost: a sizing seam so a strategy can emit a target position that varies bar to bar, plus a scoring convention for how a varying notional enters the per-fold percentages (D-0063 rebases against declared capital, which assumes a fixed size).
- With the seam, the construction is one config per root — `ESM2024`, `NQM2024`, `RTYM2024`, `CLZ2024`, `GCZ2024`, `6EZ2024`, `ZNZ2024` — at `1d`, scaling a trend core by a trailing `stdev(period, return)` inverse.
- The seven roots map onto the paper's own classification almost exactly, which is what makes this worth building rather than a curiosity.

## Data

- Owned: all seven roots at 1-minute grain 2010-06-06 → 2026-07-28, resampled to 1d. Equity index ×3, energy, metals, FX and rates — the archive genuinely spans the classification the paper's asymmetry is stated over.
- Not owned: credit, which is one of the two classes carrying their positive result. The equity half is testable and the credit half is not.
- Not owned: cross-asset portfolio accounting, so the balanced-portfolio and risk-parity parts of the paper are post-M4 regardless of the sizing seam.
- Not built: any margin model. Scaling notional up implies leverage, and this build will happily replay a position no broker would fund — `backtest` prints an INSOLVENT block rather than hiding it.
- RTY starts in 2017; the other six start in 2010, so a cross-root comparison must either accept unequal windows or trim to the shortest.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The registered test is TWO-SIDED, and this is the point of the file: the scaled arm must beat the constant-notional arm on ES, NQ and RTY, AND must NOT beat it on ZN, 6E, GC and CL. Failing either half kills the hypothesis, including the flattering half.
- `min_oos_sessions = 500` pooled across contracts per root — basis: an asymmetry across four asset classes needs enough of each that the null of no difference is actually being tested.
- `min_oos_sharpe_after_costs = 0.3` on the risk-asset arm — basis: a modest floor, because the claim is relative improvement rather than absolute merit.
- `kill_if_dead_at_ticks = 1.0` — basis: scaling generates continuous small adjustments, so it is far more turnover-exposed than a two-state gate and the cost sweep is the first place it should fail.
- `max_pbo = 0.5` and `max_permutation_p = 0.05` — basis: seven roots times a lookback grid is a wide search, and PBO is what charges for choosing the root where it worked.
- A separate registered non-criterion: the tail-reduction claim is NOT registered as a gate, because almost any de-levering rule satisfies it and a criterion nothing can fail is decoration.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The Journal of Portfolio Management, 2018. The authors are at a firm that sells systematic strategies of exactly this kind; that does not make the result wrong, and it does bear on which results get written up.
- Their samples are long daily histories per asset class. Ours would be minute bars aggregated to trading days, one contract at a time, with pooling across contracts as the only route to a comparable length.
- The paper reports its own performance figures; they are not restated here.
- The tail-reduction claim is nearly definitional: exposure is small when volatility is high, extremes happen when volatility is high, so extremes are attenuated. Reporting it as a finding across all classes inflates the apparent generality of the paper.
- The asymmetry result is the credible part precisely because it is a null for three of four classes, and a paper reporting where its effect is absent is rarer than it should be.
- Costs rest on `half_spread_ticks = 1` (D-0120), and a continuously scaled position pays that assumption on every adjustment rather than on every trade.

## Triage grade

**B.** B: the data is entirely owned — all four of the paper's asset classes bar credit — and the missing piece is continuous position sizing, which the grammar cannot express because entries are boolean and `qty_contracts` is fixed. The cost is a sizing seam in the engine plus a scoring convention for a varying notional in the per-fold percentages, which today assume fixed size. No acquisition is involved.
