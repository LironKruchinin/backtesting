---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: year-end-rate-seasonality
topic: calendar-effects
grade: C
hypothesis_family: rates-year-end-seasonality
status: draft
blocked_on: the instrument itself — this is a short-rate/LIBOR phenomenon and the archive's only rates root is ZN, a 10-year Treasury note future; LIBOR is also discontinued
created: 2026-08-06
doi: 10.3905/jod.2006.616867
source_api: semanticscholar
harvested_from: openalex, semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The year-end turn in short-rate derivatives

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

Christopher J. Neely, Drew B. Winters. *Year-End Seasonality in One-Month LIBOR Derivatives*.
The Journal of Derivatives, 2006.
DOI `10.3905/jod.2006.616867`. <https://www.semanticscholar.org/paper/7d53c0e205281bdfa6dc02a37f239f4e93e8764a>
Retrieved from the semanticscholar API on 2026-08-06.

The authors document a December pattern in a one-month interbank benchmark — both its level and its variability — and then ask whether the futures and options written on that benchmark anticipate it. Their reported answer is that those markets largely do, even though they are biased forecasters of the benchmark in general.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.3905/jod.2006.616867':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The turn of the year is a balance-sheet reporting date, so banks shrink what they fund across it and short-term cash gets expensive for a few days. The losing side is genuinely nameable: whoever must borrow over the reporting date and cannot move the date. They keep paying because the date is a regulatory fact and not a preference. Two things kill it as a strategy for us anyway. First, the paper's own conclusion is that the derivatives already charge for it, so the payer is paying a quoted price rather than an exploitable one — a documented mechanism that is priced is exactly the outcome an efficient market should produce. Second, our only rates instrument is a ten-year note future whose price is dominated by duration and term premium, and a few days of funding cost is not a term the long end can see.

## Signal in Crucible terms

- Instrument: the archive's only rates root is ZN, a ten-year Treasury note future, spelled `ZNZ2024`. The phenomenon lives at the very short end, so this is not a proxy — it is a different market.
- Timeframe: `1d` would be the grain; the effect is a matter of days around a fixed date.
- Feature: distance in trading days to the calendar year end. No operand names a date, so the calendar predicate blocks this like everything else in this topic — but that is the second blocker here, not the first.
- Rule as it would be written: take a position in a short-rate contract over the turn and unwind after it. There is no short-rate contract to take a position in.
- This file exists mainly so that a future acquisition decision has the hypothesis already stated. It cannot be run in any form today and the draft should not imply a partial version is worth attempting.

## Data

- Owned: ZN `ohlcv-1m` 2010-06-06 to 2026-07-28, with a modelled rates session table (D-0086) that deliberately omits Columbus Day and Veterans Day because the futures traded on both.
- Not owned: any short-rate futures root, any cash funding series, any repo series. The instrument class this paper is about is absent from the archive entirely.
- The benchmark the paper studies has since ceased publication and was replaced by a rate constructed on an entirely different basis, so even acquiring the historical series would not give a continuing hypothesis.
- Not owned: options on anything, so the paper's implied-volatility arm has no counterpart here at all.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The first gate is instrument admissibility, and this hypothesis fails it before any bar is read: the config that would express it names a root the archive does not hold. `funnel --check-config` is the right place for that refusal, and it costs nothing.
- If a short-rate root were ever acquired: `min_oos_sessions = 750` — basis: three years of sessions to obtain three year-ends, which is still an absurdly thin sample and says so.
- `min_oos_trades = 20` — basis: one round-trip a year means twenty years of data before a trade count means anything, which is the honest scale of this hypothesis.
- `max_permutation_p = 0.01` — basis: with a handful of annual observations, a conventional threshold cannot distinguish a pattern from an accident, and the stricter bar at least states that.
- `kill_if_dead_at_ticks = 0.5` — basis: short-rate contracts trade in fractions of a basis point and the effect is measured in basis points, so cost sensitivity is the binding constraint rather than an afterthought.
- A standing kill that overrides all of the above: the source paper's own reported conclusion is that the derivatives price the effect. A registration whose source says the market already charges for the thing should be killed on arrival unless something changed, and the burden is on the change.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The instrument is wrong and no substitution fixes it. A ten-year note future is not a one-month funding instrument, and treating it as one would be the kind of silent proxy substitution this project exists to refuse.
- The benchmark itself no longer publishes. Its successor is constructed differently, so the historical pattern may not have a continuing referent.
- The paper reports its own December-versus-rest-of-year figures for the benchmark and its own tests of forecast bias; they are not restated here and none of them describes anything Crucible would produce.
- The paper is from 2006, before the post-crisis reserve regime, before the liquidity and leverage rules that changed year-end balance-sheet management, and before the two subsequent funding-market episodes that reshaped the turn. Even the mechanism's institutional basis has been rewritten twice since publication.
- The venue is a respectable practitioner-facing derivatives journal, and the paper's reported conclusion is a negative one about tradability — which makes it more credible, not less, and also makes it a poor candidate to spend compute on.

## Triage grade

**C.** The missing piece is the instrument, and that is an acquisition rather than a build: a short-rate futures root the archive has never held, with an availability rule defined before integration, plus the calendar operand every file in this topic needs. The cost is real money and new data plumbing, and the return on it is a hypothesis whose own source paper reports the effect is already priced.
