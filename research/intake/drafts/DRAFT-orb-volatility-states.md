---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: orb-volatility-states
topic: breakout-range-expansion
grade: B
hypothesis_family: orb-conditioned-on-volatility-state
status: draft
blocked_on: a session-anchored rolling high/low (opening-range) indicator; the volatility-state half is already expressible via `stdev(period, source="return")`
created: 2026-08-06
doi: null
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Opening-range breakout sorted by volatility state

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

Christian Lundström. *Day trading returns across volatility states*.
Umeå Economic Studies, 2013.
**no DOI** (preprint). <https://openalex.org/W2594536298>
Retrieved from the openalex API on 2026-08-06.

The paper takes a standard opening-range breakout day-trading rule, applies it to long histories of crude oil and S&P futures, and sorts the daily outcomes by the volatility state the underlying was in. It reports a large gap between the best and worst volatility buckets in both markets, and reads that as evidence that day-trading outcomes depend heavily on the volatility regime rather than on the rule itself.

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

The breakout half carries the usual stop-sweep story: the range edges hold resting stop orders, the break triggers price-insensitive flow, and whoever anticipates it collects from whoever placed the stops. Fine as far as it goes. The volatility half has no payer at all, and the honest reading of this paper is deflationary rather than encouraging. If outcomes are largely a function of which volatility bucket the day fell in, then the rule is closer to a levered exposure to realised volatility than to an edge, and the counterparty question collapses to 'who is short volatility' — which is a completely different trade with completely different risk, and one nobody should stumble into by accident. Note what the stated result does not say: a large spread between best and worst buckets is compatible with the rule losing money in every one. A difference between conditional outcomes is not a claim that any of them is positive, and the indexed abstract does not establish that any is.

## Signal in Crucible terms

- What it would be: `CLM2024` and `ESM2024`, `1m`; latch the first N minutes' high and low after `minutes_since_rth_open == 0`, enter on a close beyond either, flat by `minutes_to_rth_close <= 5`, and gate the entry on a volatility state.
- Where the breakout half breaks: no session-anchored rolling high/low, same gap as the previous candidate. `minutes_since_rth_open` gives the timing and nothing about the level.
- Where the volatility half does NOT break: `[indicators.rv] kind = "stdev"`, `period = [30, 60]`, `source = "return"` is expressible today, and `enter_long = "<range_break> and rv > 0.0012"` is exactly the conditioning the paper describes.
- The threshold is the weak point of the expressible half: a fixed constant is instrument-specific and era-specific. A cut that names the high bucket in 2019 names the low bucket in March 2020, and the archive spans both. The paper's buckets are relative; a config constant is absolute, and the two are not the same conditioning.
- `source = "return"` costs one extra warmup bar and it is declared rather than absorbed, so `Grid::max_warmup_bars` aligns the whole grid on it (D-0080).

## Data

- CL and ES hold curated 1-minute bars 2010-06-06 → 2026-07-28 — both of the paper's markets, at the paper's grain, which is the best data match in this batch.
- The equity-index calendar carries the era 3a halt at 15:15–15:30 CT, removed effective 2021-06-28 (D-0086); the energy calendar takes a 16:15 CT close before 2015-09-21 with six pre-holiday early closes knowingly unmodelled (D-0089). A session-anchored study has to be aware of both.
- Missing: the session-anchored high/low latch, which is the graded gap and blocks the breakout half entirely.
- Missing: any measured volatility state definition matching the paper's. We can compute trailing realised dispersion; whether that is what they bucketed on is unknown from the abstract.
- `half_spread_ticks = 1` is an assumption (D-0120) for both roots, and see the honesty note — it interacts badly with this specific claim.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 500` — basis: raised above the batch default because the sample is being cut into volatility buckets. A session floor that is adequate unconditioned is inadequate once it is split three ways.
- `min_oos_trades = 200` — basis: one to two entries per session, further reduced by the volatility gate; below this the conditioning cannot be evaluated separately from the rule.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills, and applied to the gated rule rather than to the best bucket.
- `kill_if_dead_at_ticks = 1.0` — basis: a breakout entry crosses the spread at the worst moment; and see the honesty note, because the volatility conditioning makes this gate more binding rather than less.
- `require_plateau = true` — declared, not evaluated in this build (S3 owes it). Basis: a volatility cut is exactly the parameter most likely to show a spike rather than a plateau, since it is a single threshold on a continuous variable and the grid will happily find the value that flatters the sample.
- `max_pbo = 0.5` — evaluated since D-0109. Basis: conditioning a rule on a state variable multiplies the effective grid by the number of threshold points, and PBO is the gate that prices that multiplication rather than ignoring it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Umeå Economic Studies is a university department's working-paper series. There is no peer review behind this record at all.
- The result as stated is a difference between buckets, not a claim that the rule makes money. A large gap between the highest and lowest volatility states is fully compatible with the rule losing in every state, and nothing in the indexed abstract rules that out.
- Sorting a day-trading rule by a conditioning variable after the fact is the standard way a dead rule gets revived, and the number of candidate conditioning variables is effectively unlimited. This is the concern `max_pbo` and `require_plateau` are registered against.
- `half_spread_ticks = 1` is an assumption and not a measurement (D-0120), and it runs directly against this paper's claim: spreads widen in high-volatility states, so a fixed one-tick assumption is most optimistic precisely in the bucket the paper says carries the result. The effect is to flatter the specific conclusion under test.
- Their samples are long histories of crude and S&P futures predating ours; our archive begins 2010-06-06. The paper reports its own bucket-level performance figures; they are not restated here.

## Triage grade

**B.** Half of this is already expressible — `stdev(period, source = "return")` is the volatility state, and the session clock handles entry timing and the flatten. The missing half is the session-anchored high/low latch, and without it there is no opening range to break. Closing it costs a session-resetting indicator class delivered through the seam the clock operands already use, plus per-session warmup accounting; the volatility conditioning then works unchanged on top of it.
