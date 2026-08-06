---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: globex-intraday-volatility-shape
topic: intraday-seasonality
grade: A
hypothesis_family: cme-globex-intraday-volatility-shape
status: draft
created: 2026-08-06
doi: 10.1002/fut.20315
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Own-region versus imported volatility on a near-24-hour contract

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

Valeria Martinez, Yiuman Tse. *Intraday volatility in the bond, foreign exchange, and stock index futures markets*.
Journal of Futures Markets, 2008.
DOI `10.1002/fut.20315`. <https://doi.org/10.1002/fut.20315>
Retrieved from the crossref API on 2026-08-06.

For three CME contracts trading on a nearly round-the-clock electronic schedule — the E-mini S&P, the euro/dollar rate and the Eurodollar — the authors decompose intraday variance into a within-region component and a cross-region spillover, and find the within-region part dominates. They also report flow associated with higher variance while the outstanding position base is not, and attribute that contrast to the venue being electronic rather than a floor.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1002/fut.20315':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

For the descriptive claim there is no counterparty, because a decomposition of variance into own-region and imported parts is not a bet. The tradeable reading inverts it: if variance is generated mostly where the participants are, then the folk belief that a foreign-hours move tells you something about the coming local session is worth less than people act as though it is, and the payer would be whoever trades the overnight session on that belief. Whether such a participant exists in size is exactly what is unknown, and this paper does not address it. What the paper does supply is a reason to make session membership the conditioning variable rather than an afterthought, which happens to suit a grammar that already carries session clock readings as first-class operands. The burden falls entirely on the out-of-sample test, since the mechanism supplies structure but names no source of payment.

## Signal in Crucible terms

- Instruments `6EZ2024`, `ESZ2024` and `ZNZ2024`, timeframe `15m`. One instrument per config, so this family is three registrations sharing one hypothesis key.
- Overnight arm: `enter_long: is_overnight and close crosses_above bollinger_16.upper`, `exit_long: is_rth or close crosses_below bollinger_16.mid`, with the symmetric short off `bollinger_16.lower`.
- Regular-hours arm, a second config with identical parameters: replace `is_overnight` with `is_rth` and the exit gate with `is_post_rth`. The comparison between arms is what tests the own-region claim.
- The grammar has `is_rth`, `is_overnight` and `is_post_rth` as direct operands (D-0078), which is the rare case of a paper's conditioning variable existing verbatim in the config language.
- The volume half of the paper is partly expressible as `zscore(period, volume)`; the open-interest half is not, since open interest is not in curated data at all.
- Fidelity caveat: `is_rth` on 6E and ZN means the CME-declared regular session for those products, which is not the same clock as the regional trading day the paper decomposes. The mapping is approximate and the report must say so.

## Data

- Owned: curated 1-minute ES, 6E and ZN bars from 2010-06-06 to 2026-07-28, with `15m` aggregated on read against each product's own calendar.
- Owned: per-product calendars with session eras (D-0086, D-0089), so `is_rth` and `is_overnight` are answered from a real table rather than a hardcoded offset.
- Not owned: the Eurodollar contract. CME moved that complex to SOFR and this archive holds neither, so ZN is a substitute for the paper's rates leg, not the instrument it studied.
- Not owned: open interest in curated form. The raw `statistics` schema is archived but nothing transcodes it, so half the paper's liquidity comparison is unreachable.
- A dating caveat: ZN's pre-2011 era carries an open time whose source is marked UNVERIFIED, so `minutes_since_open` on ZN before 2011-10-03 rests on a table entry with no publication behind it.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The own-region gate, which is the one that decides this: the overnight arm and the regular-hours arm must differ by a registered margin on the same instrument. If they behave the same, session membership is not the conditioning variable that matters and the hypothesis is Killed regardless of profitability.
- `min_oos_sessions = 250` and `min_oos_trades = 200` — basis: a session-membership rule fires a handful of times per session, so 250 sessions is the minimum for a countable sample; ES, 6E and ZN are all quarterly, so one contract reaches neither and will be killed for sample adequacy.
- `min_oos_sharpe_after_costs = 0.5` — basis: the backlog's fixed floor, unchanged across the queue.
- `kill_if_dead_at_ticks = 1.0` — basis: overnight is the thin part of the session on all three of these contracts, so the cost sweep is the decisive test rather than a robustness check; ZN's tick is a 64th of a point and its overnight book is the thinnest of the three.
- `max_permutation_p = 0.05` — basis: a rule that fires in a fixed block of hours over sixteen years of bars will reach conventional significance on sample size alone; the block-permutation null is the correction.
- `require_controls_beaten = true` — basis: matched random entries confined to the same session block are the only fair benchmark for a rule whose entire content is when it trades.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The paper is from 2008 and its data predates that. Globex in the mid-2000s was a different market: lower participation overnight, wider spreads, and a floor still operating alongside it. Whatever the regional decomposition was then, eighteen years of electronic growth is a strong reason to expect it moved.
- One of the three instruments no longer exists in the form studied. Eurodollar futures were transitioned to SOFR, so the rates leg is a substitution rather than a replication.
- The Journal of Futures Markets is a legitimate refereed outlet, which puts this above most of the batch on provenance and below none of it on age.
- The paper reports its own statistical figures; they are not restated here, and none of them describes anything this engine would produce.
- The open-interest half of the finding cannot be checked at all with curated data, so any run under this key tests roughly half the paper.
- Cost realism: `half_spread_ticks = 1` is an assumption for 6E and ZN permanently (D-0120), and an overnight-only rule concentrates its trades in the hours where a flat one-tick assumption is most likely to be optimistic.

## Triage grade

**A.** `is_rth`, `is_overnight`, `is_post_rth`, `bollinger` and the completed bar's fields are all in the grammar, and all three instruments are curated, so this runs today with no new Rust. But runnable is not answerable: each of these roots is quarterly, one contract is a short window, and no sample-adequacy floor worth registering is reachable at that length. Today's run is guaranteed to be killed for sample size, correctly, by the machine, until registry pooling across contracts lands.
