---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: speculator-flow-momentum-reversal
topic: short-horizon-reversal
grade: C
hypothesis_family: commodity-speculator-flow-decomposition
status: draft
blocked_on: trader-position data (CFTC Commitments of Traders or equivalent) — not owned, not in `docs/DATA_PLAN.md`, and not in any milestone
created: 2026-08-06
doi: 10.2139/ssrn.6425598
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Splitting commodity returns into speculator flow and residual

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

Shen Zhao, Yiyi Ding, Jianfeng Yu, Wenjin Kang. *Momentum and Reversal on the Short-Term Horizon: Evidence from Commodity Markets*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6425598`. <https://doi.org/10.2139/ssrn.6425598>
Retrieved from the crossref API on 2026-08-06.

Using position data on who traded, the paper splits each week's commodity futures return into a piece attributable to speculators' net trading and an orthogonal remainder. It reports that the remainder predicts the following week positively while the flow piece predicts it negatively, so momentum and reversal appear at the same horizon rather than at the segregated horizons the standard account assumes. It attributes the momentum piece to trend-chasing by speculators and claims the signal can be aggregated to strengthen a conventional intermediate-horizon momentum rule.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6425598':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

This candidate has an unusually explicit account of who loses, which is exactly why it is out of reach. The negative-signed flow component is the classic liquidity-provision story: speculators demanding immediacy push price away from where it would otherwise sit and pay whoever absorbs them, so the speculator himself is the payer and the reversal is that payment coming back. The positive-signed residual is attributed to trend-chasing behaviour, where the loser is whoever is slow. Both halves are statements about who traded rather than about what price did, and that is precisely the information this archive does not hold. We have bars. We can see the price move; we cannot see whose order caused it. Without the decomposition there is no idea here — a price-only version would just be a weekly momentum rule, which is a different and much older hypothesis with a much worse reputation, and registering it under this paper's name would be dishonest.

## Signal in Crucible terms

- What it would be: for each root, a weekly series of speculator net position, joined to price at its own availability time, and used to split the weekly return into two components that enter as separate predictors.
- Where it breaks, first: we hold no trader-position data of any kind. CFTC Commitments of Traders is not owned, is not in `docs/DATA_PLAN.md`, and is not scheduled in any milestone.
- Where it breaks, second: the availability rule is a genuine §2.1 design question nobody here has answered. COT covers positions as of Tuesday and publishes Friday afternoon — a multi-day lag that must be encoded as an `avail_ts` before a single bar is joined, or the whole thing is lookahead.
- Where it breaks, third: position data is revised. A revised record has two availability windows, which is the same shape as the expiry-revision problem D-0090 solved for `definition` files, and it would need the same treatment.
- Where it breaks, fourth: the grammar has no arithmetic between operands, so 'return minus its flow component' cannot be written even if both series existed.
- Where it breaks, fifth: weekly grain against a sixty-session contract life is roughly twelve observations. Even pooled, the effective sample is thin.

## Data

- CL and GC hold curated 1-minute bars 2010-06-06 → 2026-07-28, which is the price half of the input and the easy half.
- Missing and unscheduled: COT or any equivalent position series. The brief's archive inventory lists trader-position data explicitly among what is not owned.
- Missing: a broad commodity cross-section. The paper's claim is about the whole cross-section of sample commodities; we hold two commodity roots.
- Missing: any ingest, manifest or availability machinery for a weekly government release. That is a new data source with a new availability rule, which §2.1 says is the first design question and not an afterthought.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 1000` — basis: weekly signals over roughly two hundred weeks. Anything shorter and the two opposite-signed components cannot be distinguished from each other, let alone from zero.
- `min_oos_trades = 150` — basis: a weekly rebalance produces on the order of one entry per week, so this floor and the session floor are the same constraint stated twice; that is deliberate.
- `min_oos_sharpe_after_costs = 0.4` — basis: below the house floor because a weekly rule turns over rarely and its claim is persistence rather than intensity.
- `kill_if_dead_at_ticks = 2.0` — basis: loose, because cost is not the binding constraint for a weekly rule. Setting it tight would kill the idea for a reason that has nothing to do with the paper's claim.
- `max_permutation_p = 0.05` — basis: the paper claims to overturn a canonical ordering of horizons, and an extraordinary claim gets the standard null rather than a softer one.
- `min_abs_ic = 0.05` at a one-week forward horizon, with S0's bootstrap interval excluding zero (D-0085) — basis: the paper's claim is a predictive one about two components, and an information coefficient at the stated horizon is the direct measurement; the floor is raised above the batch default because two predictors are being tested, not one.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- SSRN working paper, 2026, unrefereed, with an SSRN handle rather than a journal DOI. Nothing has been reviewed by anyone.
- The claim explicitly overturns a canonical result about which horizons carry momentum and which carry reversal. A canonical result is canonical partly because many people have tried to break it; overturning it in a working paper raises the prior of a specification artefact rather than lowering it.
- Position data is reported with a multi-day lag and is subsequently revised. A study that uses it without treating the revision timeline carefully is at risk of lookahead in exactly the way §2.1 names, and we cannot check from an abstract whether they did.
- Their universe is a broad commodity cross-section; we hold two commodity roots. Even with the data, this would be a badly underpowered test.
- The paper reports its own predictive and portfolio results; they are not restated here.
- `half_spread_ticks = 1` is an assumption (D-0120), though it is not the binding problem for this candidate — the binding problem is that the input series does not exist here.

## Triage grade

**C.** The missing piece is trader-position data — not owned, not planned, not in any milestone. Closing it costs a new external source with its own acquisition path, manifest entries and checksum verification; an explicit availability rule for a lagged weekly release, which §2.1 makes the first design question; a revision policy of the shape D-0090 built for expiries; and arithmetic between operands, which the rule grammar does not have. Four separate unlocks, none of them small.
