---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: auction-day-price-pressure-reversal
topic: treasury-auction-cycle
grade: C
hypothesis_family: zn-auction-window-response
status: draft
blocked_on: a Treasury auction calendar; the order-flow half needs L1/L3 data the archive lacks for ZN
created: 2026-08-07
doi: 10.59576/sr.1188
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Yields drift up before an auction and come back after it

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

Michael J. Fleming, Weiling Liu, Giang Nguyen. *Intraday Price Pressure and Order Flow Around U.S. Treasury Auctions*.
Staff Reports (Federal Reserve Bank of New York), 2026.
DOI `10.59576/sr.1188`. <https://doi.org/10.59576/sr.1188>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: using more than three decades of intraday Treasury
observations, the authors describe a within-day pattern around auctions — yields
rising ahead of the sale and retracing afterwards — report that the pattern is
larger when dealer risk-bearing capacity is tighter and smaller when demand is
strong, argue that net order flow is the channel carrying the constraint into
prices, and find no worsening in recent years.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.59576/sr.1188':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Same mechanism as the previous candidate, seen from the other side and one step
earlier. Before an auction, someone has to make room: dealers shorten duration to
create balance-sheet space for what they are about to buy, and that selling is
price pressure rather than information. Afterwards the pressure is released. The
payer is again the constrained intermediary, and the paper's finding that the
pressure scales with how constrained they are is what makes it a mechanism rather
than a pattern. Two things make this file worth having beside the previous one:
the direction of the pre-auction leg is the opposite sign to the post-auction leg,
so the two together are a round trip rather than a repeat, and this paper's finding
that the effect has *not* grown with debt issuance is a null that a naive reading
of the supply story would get backwards.

## Signal in Crucible terms

- Not expressible, and the same event-calendar block applies.
- The order-flow half has a second, independent block. Signed order flow needs
  quote-level or message-level data, and D-0120 records that the archive holds
  `tbbo` and `trades` for `ES.FUT` only, for one year of sixteen, with the
  entitlements lapsed. There is no ZN order flow in this archive and none can now
  be acquired.
- The price-pressure half needs only the calendar, so the two halves are blocked
  differently and should be unblocked separately. Registering them as one item
  would hide that.
- Both legs would be single-instrument, which is the one structural constraint this
  candidate does *not* run into.

## Data

- Owned: ZN `ohlcv-1m` and `ohlcv-1s`, sixteen years. Enough for the price half at
  a finer grain than the paper's.
- Not owned: the auction calendar, as above.
- Not owned and unobtainable: ZN order flow at any level. This is the sharper
  version of the D-0120 note — for six of seven roots the missing L1 is not a
  budget question, the entitlement windows lapsed and the vendor sells the past
  only through those windows.
- Not owned: dealer positioning or balance-sheet measures, which is the paper's
  conditioning variable.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today.
- Registrable now: the pre-auction and post-auction windows are declared before any
  run, and — this is the criterion that stops the file becoming a fishing licence —
  **both must be tested, and the signs must be opposite.** A pre-auction drift that
  does not retrace is a different phenomenon from the one described, and passing on
  one leg alone would be a result assembled after the fact.
- The recent-decade null is registered as a prediction: the effect must **not** be
  larger in the post-2020 sub-window. A run that finds it growing has contradicted
  the paper and should be treated as a data or calendar bug before it is treated as
  a discovery.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- A Federal Reserve staff report, which is careful work, and its price series is
  the cash Treasury market rather than the futures market. Cash and futures are
  different markets and this repository has already been burned by assuming
  otherwise once — CLAUDE.md §9 records that the futures calendar has no Columbus
  Day or Veterans Day although the cash market closes on both.
- The paper's own reported magnitudes are theirs and are not restated here.
- Their sample is thirty-three years; ours is sixteen, and the earlier two-thirds
  of theirs is where dealer structure was most different.
- The order-flow claim is the paper's headline and is the half we can never test.
  Grading the candidate on the half we could test would overstate what a run here
  would settle.

## Triage grade

**C.** C, and the missing pieces are the **Treasury auction calendar** for the price half
and **ZN order-flow data** for the mechanism half, the second of which is
unobtainable rather than merely unbought (D-0120). Splitting them is the useful
part: the calendar unlocks a testable price hypothesis on an owned instrument, and
nothing unlocks the channel.
