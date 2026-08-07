---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: network-momentum-leadlag
topic: trend-horizon
grade: C
hypothesis_family: futures-network-momentum-spillover
status: draft
blocked_on: multi-instrument configs — `combo` refuses a config declaring two instruments, and momentum spillover is a statement about pairs
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Lead-lag spillover added to a univariate trend signal

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

Linze Li, William Ferreira. *Follow the Leader: Enhancing Systematic Trend-Following Using Network Momentum*.
arXiv q-fin, 2025.
**no DOI** (preprint). <http://arxiv.org/abs/2501.07135v1>
Retrieved from the arxiv API on 2026-08-06.

The paper builds a trend indicator for commodity futures that mixes each market's own trend reading with information borrowed from markets that appear to lead it, estimated by two different lead-lag detection methods and combined into a network-derived score. It compares a portfolio driven by that combined indicator against one driven by the univariate signal alone, and reports improvement on its own performance measures using bootstrapped resamples of real price histories.

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

If a lead-lag relationship is real, the payer is nameable and boring: whoever trades the lagging market without watching the leading one. That is a slow-information-diffusion story, and it is the kind of thing that used to be true across related commodity markets before everything was on the same colocated infrastructure. The difficulty is that a lead-lag network estimated over a wide cross-section is a fitted object with an enormous search space — every ordered pair is a candidate, and the number of pairs grows with the square of the universe. So the second and more likely answer to who is on the losing side is: nobody, and the apparent edge is being paid for by the researcher's own degrees of freedom. Bootstrapping resampled price paths does not fix this, because the network was chosen with the original sample in view. Until the pairs are pre-registered rather than discovered, this is a claim about an estimator, not about a market.

## Signal in Crucible terms

- What it would be: two instruments in one config — a leader such as `CLM2024` and a follower such as `GCM2024` — with the follower's entry conditioned on the leader's trend state.
- Where it breaks, first: `combo` refuses a config declaring two instruments rather than silently running the first. A partial answer printed in the shape of a whole one is worse than a refusal.
- Where it breaks, second: there is no cross-instrument operand. Every operand in the grammar reads the completed bar of the config's single instrument, an indicator over that instrument, or the session clock. There is no syntax for 'the other market's `fast`'.
- Where it breaks, third: the network estimation itself is a cross-sectional fitting step with no home in this architecture — it is not an indicator, not a rule, and would have to be a pre-run artefact with its own availability rule under §2.1.
- Seven roots give 42 ordered pairs. That is a trial count that must be charged to the hypothesis family honestly, and the registry would need to know it before the first run.

## Data

- Seven CME roots with curated 1-minute bars over a shared 2010-06-06 → 2026-07-28 window, so the raw material for a lead-lag study across equity index, energy, metals, FX and rates does exist.
- RTY starts in 2017, so any pair involving it has a shorter common window than the others — a detail a naive pairwise estimator would silently absorb.
- Missing: multi-instrument replay. This is the graded gap and it is a code gap, not a data one.
- Missing: the broad commodity cross-section the paper's network is built on. Seven roots is not a network; it is a handful of nodes, and a network claim tested on it is underpowered by construction.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: higher than the batch's usual floor, because a pair claim needs enough co-movement observations for both legs, and the effective sample is the shorter of the two contracts.
- `min_oos_trades = 200` — basis: a conditioned entry fires less often than an unconditioned one, and the conditioning is the thing under test.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor; the paper's claim is an improvement over a univariate baseline, so the baseline must also be run and must clear nothing — the comparison is what matters.
- `max_pbo = 0.4` — evaluated since D-0109. Basis: tightened below the usual 0.5 because a pair-selection step multiplies the effective grid by the number of candidate pairs, and PBO is the only gate that prices selection.
- `max_permutation_p = 0.01` — basis: tightened by an order of magnitude from the batch default, because a discovered network is a multiple-comparison problem and a 0.05 threshold over 42 pairs expects two false positives by construction.
- `require_controls_beaten = true` — basis: the univariate trend rule is the paper's own baseline; if the network version does not beat it, the claim is refuted regardless of any absolute threshold.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- arXiv preprint, 2025, not refereed. Lead-lag and network-momentum papers are a crowded recent genre and almost none of them report a live track record.
- Their universe is commodity futures broadly; ours is two commodity roots plus five others across unrelated asset classes. A network claim does not survive that reduction.
- The abstract's evidence is bootstrapped resamples of actual price series, compared against a baseline the same authors constructed. Both the network and the baseline were chosen with the sample visible. The paper reports its own comparative performance figures; they are not restated here.
- `half_spread_ticks = 1` is an assumption (D-0120). A pair strategy doubles the number of legs and therefore doubles the cost exposure, so this assumption is exactly twice as load-bearing here as elsewhere.
- Even with multi-instrument configs, the honest test would still be one pair at a time against a pre-registered pair list. Discovering the pairs from our own archive and then testing them on it would be the mining this whole directory exists to refuse.

## Triage grade

**C.** The missing piece is multi-instrument configs: `combo` refuses two instruments by design, and there is no operand that reads another market's bar. Closing it costs a universe expansion in the config schema, a cross-instrument feed alignment keyed on `avail_ts` rather than `ts_open`, trial accounting for a pair universe, and a decision on what a pair's warmup is when the two contracts have different lives. Every one of those is a §2.1 or §2.6 question, not plumbing.
