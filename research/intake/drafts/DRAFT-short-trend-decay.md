---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: short-trend-decay
topic: trend-horizon
grade: A
hypothesis_family: futures-short-trend-decay-regime
status: draft
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Fast-span trend decay and the tick-size split

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

Jutta G. Kurth, Zoltan Eisler, Adam Rej, Jean-Philippe Bouchaud. *Is Trend Still Your Friend?: A Microstructural Account of the Demise of Short-Term Trend-Following*.
arXiv q-fin, 2026.
**no DOI** (preprint). <http://arxiv.org/abs/2607.01550v1>
Retrieved from the arxiv API on 2026-08-06.

The paper argues that the fast end of trend following stopped working somewhere around 2009, and that the variable separating the contracts where it died from the ones where it survived is tick size measured against volatility — not asset class and not liquidity. Its proposed reason is a feedback loop: trend orders push price the way their own signal pointed, and modern market makers pull depth in front of that flow on contracts with thin books, so the loop stops closing.

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

A trend rule is a claim that someone hands you a worse price as the move develops. This paper is unusually direct about who: the market maker absorbing the trend follower's aggressive orders, plus whichever slower participant leans against a move that keeps going. The twist is that the trend follower's own impact is part of the payoff — signal triggers trade, trade pushes price, push validates signal — so the edge depends on being able to run the book over at bearable cost. On contracts whose tick is small relative to daily movement, the paper claims the liquidity provider now steps back rather than absorbing, and the loop no longer closes. Our archive carries one side of that split already: over half the minute bars in most ZN contracts do not move at all, which is exactly what a large tick relative to volatility looks like. Whether any of it holds at minute grain on seven roots is a much smaller question than the one the paper asked.

## Signal in Crucible terms

- Instrument: one raw contract per config, four-digit key — `ESH2024` (small tick relative to volatility) against `ZNH2024` (large tick). One instrument per config, so the comparison is two registrations, not one.
- Timeframe: `15m`, aggregated on read from the curated 1-minute bars (D-0077).
- `[indicators.fast] kind = "ema"`, `period = [8, 12, 20]`; `[indicators.slow] kind = "ema"`, `period = [50, 100, 200]`.
- `enter_long = "fast crosses_above slow"`, `exit_long = "fast crosses_below slow"`, `enter_short = "fast crosses_below slow"`, `exit_short = "fast crosses_above slow"` — stop-and-reverse, the same shape as the null harness.
- The grid is deliberately weighted to the fast end, because the fast end is the half the paper says is dead; a slow-only grid cannot falsify it.
- Not expressible: the volatility-normalised tick-size variable itself, which is a cross-sectional statistic over ~100 contracts. We can only run the two ends of it separately and read the difference by eye.

## Data

- All seven roots have curated 1-minute bars over 2010-06-06 → 2026-07-28, so both the small-tick and large-tick ends of the paper's split are present in some form.
- RTY's archive begins in 2017 — the contract did not list on CME earlier, so any pre-2017 hole in RTY is the instrument not existing, not a gap.
- We hold no pre-2010 data at all, which matters more here than anywhere else in this batch: the paper's central object is a structural break dated to roughly 2009, and our sample starts after it.
- Missing: the CTA proxy series the paper leans on to date the break, and any measured spread series for six of the seven roots.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a claim about a regime lasting a decade cannot be adjudicated on one contract quarter; 250 pooled sessions is roughly one trading year and is the smallest window worth calling a sample.
- `min_oos_trades = 200` — basis: crossover grids produce few round-trips, and below 200 the fold-to-fold dispersion is wider than any effect this paper describes.
- `min_oos_sharpe_after_costs = 0.5` — basis: the house floor for anything earning a second look, applied to S2's honest fills rather than the S1 screen.
- `kill_if_dead_at_ticks = 1.0` — basis: this is the gate that decides the idea. The paper's mechanism IS an execution-cost story, so an edge that evaporates at one tick has agreed with the paper — and must still be killed, because agreeing with a paper is not a reason to trade.
- `max_permutation_p = 0.05` — basis: a fast crossover on a directional sample is the easiest thing in this repository to fool yourself with; the block-permutation null is what separates the rule from the drift.
- `require_controls_beaten = true` — basis: buy-and-hold on an index contract over most quarters of the last decade is a real competitor, and a long-biased crossover routinely loses to it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their cross-section is roughly 100 liquid futures over three decades; ours is seven CME roots starting 2010-06-06. We cannot observe their break at all — only the regime after it — so a null result here is consistent with the paper and tells us nothing new.
- Their signal horizons are daily-scale trend spans on a long history. A 15-minute crossover over one contract quarter is a different object wearing the same name, and should not be reported as a replication.
- arXiv preprint, 2026, from a practitioner shop. Not refereed. A paper whose conclusion is 'the thing we sell stopped working at the fast end' is at least arguing against interest, which is worth a little, but not much.
- `half_spread_ticks = 1` is an assumption and not a measurement (D-0120), and for six of the seven roots it always will be — the L1 entitlement lapsed and only ES has `tbbo`. This paper's entire thesis lives in the spread, so our cost model is weakest exactly where its claim is strongest.
- The paper reports its own performance figures across contracts and horizons; they are not restated here, and nothing in this draft should be read as a prediction of what Crucible would produce.

## Triage grade

**A.** Two moving averages and a crossover — the grammar covers it exactly, and every root has raw contracts to run it on. But runnable is not answerable: a grid config replays one contract's active life, roughly sixty sessions for ES, and no sample floor worth pre-registering can be met at that length. This run will be killed for sample size, correctly and by the machine, until registry pooling across contracts lands.
