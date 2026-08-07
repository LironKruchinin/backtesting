---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: crude-intraday-reversal
topic: short-horizon-reversal
grade: B
hypothesis_family: cl-intraday-reversal-first-halfhour
status: draft
blocked_on: an anchored reference price — a price captured at a named past instant (the first half-hour's close) and held for the session
created: 2026-08-06
doi: 10.1016/j.econmod.2021.01.005
source_api: semanticscholar
harvested_from: semanticscholar
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Night-session return predicting the crude day, with the sign flipped

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

Danyan Wen, Yudong Wang, Yaojie Zhang. *Intraday return predictability in China’s crude oil futures market: New evidence from a unique trading mechanism*.
venue unrecorded, 2021.
DOI `10.1016/j.econmod.2021.01.005`. <https://www.semanticscholar.org/paper/9b3aa0cf40d11f5cf65d02bb5c1f06c9c683a030>
Retrieved from the semanticscholar API on 2026-08-06.

Studying China's INE crude oil futures, which runs an unusual session structure with a separate night block, the paper reports that the return over the earlier block predicts the return over the later one with a negative sign — a reversal, where the widely-cited US equity result is a continuation. It reports the finding holds out of sample as well as in, and that the predictability concentrates when volume and volatility are high and liquidity is low.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.econmod.2021.01.005':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The candidate payer is the night-session participant whose move was not information. Where the overnight block is thin and skewed toward one participant type, a push there is impatience more often than news, and the deeper day session unwinds it — so the overnight trader pays the day trader. That is coherent. What kills the transfer is that the paper's own framing rests on the specific structure of that venue: a distinct night block, position and price limits, capital controls, and a retail-heavy mix with no CME analogue. A mechanism built on who is in the room does not survive changing the room. CME crude runs a nearly continuous session with a global institutional base; the same measurement there tests a different population. Note also the sign: the US intraday literature reports continuation and this reports reversal, so a transfer to CME is a coin flip on direction before any estimate is made, which is the condition under which a result gets found in whichever direction the sample leaned.

## Signal in Crucible terms

- What it would be: `CLM2024` at `15m` or `1h`; capture the return from the session open to a named instant (the close of the first half hour, or the end of the overnight block), hold that reference for the remainder of the session, and enter against its sign.
- Where it breaks: every operand in the grammar is either the completed bar's own field, a trailing-window indicator, or a session-clock reading. There is no operand that latches a value at a named instant and holds it. `minutes_since_open` tells you when you are; it does not tell you what the price was then.
- The closest expressible thing is `[indicators.shock] kind = "zscore"`, `period = 24`, `source = "return"` gated by `is_overnight` and `is_rth` — but a trailing z-score is a rolling statistic, not an anchored one, and substituting it silently changes the hypothesis from 'the night block moved X' to 'the last two hours moved X'.
- The session-clock half is fully available: `is_overnight`, `is_rth`, `is_post_rth`, `minutes_since_open`, `minutes_to_close`. It is only the anchored price that is absent.
- Note that `minutes_to_close` shortens on an early close while `minutes_to_rth_close` does not (D-0078); a session-anchored idea has to say which of the two it means.

## Data

- CL holds curated 1-minute bars 2010-06-06 → 2026-07-28, which is far more data than the INE contract has existed for.
- The energy calendar carries era boundaries (D-0089): CL takes a 16:15 CT close before 2015-09-21, and six pre-holiday early closes are knowingly unmodelled and appear as missing bars.
- Missing, and this is the graded gap: an anchored reference price. It is a grammar gap, not a data gap.
- Missing entirely: INE crude. We hold no Chinese exchange data, no RMB-denominated contracts, and no way to test the paper on its own market.
- `half_spread_ticks = 1` is an assumption for CL and always will be (D-0120), which matters unusually much here — see the honesty note.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: the signal fires at most once per session by construction, so sessions are the natural sample unit and one pooled trading year is the floor.
- `min_oos_trades = 200` — basis: at one entry per session, this floor and the session floor are nearly the same constraint, which is the correct shape for a once-a-day rule.
- `min_oos_sharpe_after_costs = 0.5` — basis: house floor after honest fills.
- `kill_if_dead_at_ticks = 1.0` — basis: the paper claims the effect concentrates in low-liquidity periods, which is exactly where a fixed one-tick half-spread is most optimistic. If the edge dies at the assumed cost, the true cost would have removed it long before.
- `max_permutation_p = 0.05` — basis: a once-per-session directional rule over one contract produces few independent observations, and the block null is the only gate that prices that honestly.
- `require_controls_beaten = true` — basis: the matched random-entry control is the right comparator for a rule that enters at a fixed time every day; beating zero is not the same as beating entering at random.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The market studied is INE crude in Shanghai. It is not a market we trade, not an exchange we hold data for, and not denominated in our currency. The contract launched in 2018, so the paper's sample is short by construction.
- The paper's result has the opposite sign to the US intraday momentum literature that this backlog already registers separately as H-001. A transfer to CME therefore has no prior direction, and finding either sign on our data would be unsurprising.
- Economic Modelling is a general economics journal rather than a finance or microstructure one. Intraday-predictability papers there are not reviewed by people who trade.
- The claimed concentration in low-liquidity periods is the part most likely to vanish under honest costs, and it is exactly the part our `half_spread_ticks = 1` assumption cannot represent (D-0120) — a fixed spread assumption is most wrong when the book is thinnest, and wrong in the flattering direction.
- The paper reports its own in-sample and out-of-sample predictability figures; they are not restated here.

## Triage grade

**B.** The missing piece is an anchored reference price: a value captured at a named session instant and held for the rest of the session. The grammar has trailing windows and clock readings and nothing that latches. Closing it costs a new operand class, a reset rule driven by the session boundary, a decision about what it reads on a session that has no such instant (early close, holiday, a halt inside the window), and per-session rather than per-series warmup accounting.
