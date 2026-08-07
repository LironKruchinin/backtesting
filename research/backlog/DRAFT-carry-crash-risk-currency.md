---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: carry-crash-risk-currency
topic: fx-carry-interest-differentials
grade: C
hypothesis_family: fx-carry-crash-asymmetry
status: draft
blocked_on: an interest-differential feature (or a two-maturity FX curve), and a currency cross-section
created: 2026-08-07
doi: 10.1086/593088
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Carry returns are paid for crash risk, and unwind when funding tightens

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

Markus K. Brunnermeier, Stefan Nagel, Lasse Heje Pedersen. *Carry Trades and Currency Crashes*.
NBER Macroeconomics Annual, 2008.
DOI `10.1086/593088`. <https://openalex.org/W2123888799>
Retrieved from the openalex API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors report that the currency pairs a carry
trade is built on carry a left tail rather than a symmetric one, attribute that
asymmetry to positions being closed in a hurry once leverage and risk appetite
contract, find that funding measures carry information about subsequent moves, and
observe that pairs sharing a rate level move together more than they should.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1086/593088':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The carry trade is the clearest example in finance of a return that is a payment
for a specific risk rather than a free lunch, and the payment's shape is the point:
small gains most of the time, occasional large losses, arriving together across
every currency with a similar rate. Who pays whom is unusually well identified —
the carry trader is *paid* by hedgers and by borrowers who want the funding
currency, and gives it all back when leverage is withdrawn. For Crucible the
important consequence is not the strategy but the warning: any FX rule whose
returns are negatively skewed and whose losses cluster is likely to be a carry
trade in disguise, however it was constructed. That is a reason to look at the
distribution of a surviving 6E rule rather than only at its aggregate.

## Signal in Crucible terms

- Not expressible. The signal is an interest-rate differential and no operand names
  one.
- There is a tempting route that must be named and rejected: in FX futures the
  differential is embedded in the *calendar spread* — the price difference between
  two maturities of the same contract is the carry, mechanically. The archive owns
  every 6E maturity, 149 curated contracts. So the data is fully present and the
  gap is that a config cannot read two maturities at once. This is the same missing
  piece as the commodity curve candidates, arriving from a completely different
  literature, which is worth noticing when deciding what to build.
- The cross-sectional half — sorting many currencies by rate — needs a currency
  cross-section, and 6E is the only FX root owned.
- The skewness observation is measurable on any strategy the funnel already runs,
  and does not need this paper's machinery. It is a reporting question, not a
  signal.

## Data

- Owned: 6E `ohlcv-1m`, 149 curated contracts, 2010-06-06 → 2026-07-28 — every
  maturity, which is what makes the carry *implicitly* present.
- Not owned: interest rates for any currency, funding-liquidity measures, or any
  currency other than the euro.
- Not owned: a two-maturity reader. This is machinery over owned data, not an
  acquisition.
- Sample note: their era is dominated by the 2008 unwind and ours begins in 2010,
  after it. The single largest event in the paper's evidence is outside our window.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable as a strategy. Registrable as a **diagnostic**, and this is the
  useful part: any 6E rule that reaches S2 should have the skew of its per-fold
  returns and the clustering of its losses reported beside the headline, because a
  rule that has quietly become short crash risk looks identical to a rule with an
  edge until the crash.
- Whoever builds the two-maturity feature should fix, before the run, which pair of
  maturities defines the carry and what happens in the roll window — the answer
  cannot be chosen after seeing which pair produced a signal.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their instruments are spot and forward exchange rates across many currencies;
  ours is one euro futures contract at a time. The cross-section is the paper's main
  device and we have none.
- The paper's own reported figures are theirs and are not restated here.
- This is among the most heavily cited papers in the harvest and its central claim
  has held up better than most, which is precisely why the derived warning — watch
  the skew — is worth more here than the strategy.
- Costs rest on `half_spread_ticks = 1` for 6E and no measured alternative can ever
  exist for it (D-0120).

## Triage grade

**C.** C, and the missing pieces are **an interest-differential feature — reachable as a
two-maturity FX curve over data we fully own — and a currency cross-section we do
not own**. The first is the same machinery gap as the commodity term-structure
candidates; the second is an acquisition. Listing them separately matters, because
building the first delivers a time-series carry signal for 6E without touching the
second.
