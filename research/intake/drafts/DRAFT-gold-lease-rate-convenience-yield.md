---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: gold-lease-rate-convenience-yield
topic: metals-lease-rates-carry
grade: C
hypothesis_family: gc-lease-rate-carry
status: draft
blocked_on: a gold lease-rate / forward-rate series, and COMEX warehouse inventory
created: 2026-08-07
doi: 10.1504/ijferm.2014.058766
source_api: openalex
harvested_from: crossref, openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The gold lease rate as the convenience yield, and its link to warehouse stocks

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

Giovanni Barone Adesi, Helyette Geman, John Theal. *On the lease rate, convenience yield and speculative effects in the gold futures market*.
International Journal of Financial Engineering and Risk Management, 2014.
DOI `10.1504/ijferm.2014.058766`. <https://doi.org/10.1504/ijferm.2014.058766>
Retrieved from the openalex API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors compare two candidate measures of gold's
convenience yield, argue the leasing rate is the better one, relate it to
discretionary warehouse stocks with a negative sign, and note a widening gap
between futures and forward prices that grows with maturity and with speculative
pressure.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1504/ijferm.2014.058766':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Gold's carry is unusual because gold can be *lent*. A holder of bullion earns a
lease rate for parting with it, and that rate is the price of the metal's
availability — the convenience yield the theory of storage talks about, made
directly observable rather than inferred from a curve. The payer is whoever needs
physical metal now and must borrow it: a refiner, a bullion bank short of stock, a
short seller facing delivery. That is a real, nameable counterparty, which puts
this well above most of the harvest. The claim that the futures–forward gap widens
with speculative pressure is the tradeable-sounding half, and it is also the half
whose data we do not have.

## Signal in Crucible terms

- Not expressible. The lease rate is an exogenous series and there is no operand
  for one; the archive holds prices and volumes and nothing else.
- The futures–forward gap needs a forward curve, which is an over-the-counter
  object with no CME contract behind it.
- The warehouse-stock leg needs COMEX inventory reports, which are published daily
  and are not in `docs/DATA_PLAN.md`.
- No substitute is honest. Trailing realized volatility is not speculative
  pressure, and volume is not inventory; using either would register a hypothesis
  about gold's tape while claiming to test a hypothesis about gold's vault.

## Data

- Owned: GC `ohlcv-1m` for 221 contracts. That is the futures leg, complete.
- Not owned: lease rates or the forward rates they are derived from. The published
  benchmark this literature uses was discontinued in 2015, which means the series
  is *not* simply unacquired — for half our archive's span it does not exist in the
  form the paper used.
- Not owned: COMEX warehouse stocks, registered or eligible.
- Worth flagging for whoever picks this up: the discontinuation date sits inside
  our window, so any replication would have a structural break in its main
  regressor exactly in the middle of the sample.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable today, and unusually clearly so: two of the three series in the
  paper do not exist here and one of them no longer exists anywhere in its
  original form.
- Registrable now is the negative record: **a gold carry hypothesis stated in
  lease-rate terms is not reachable from this archive**, and the substitute a future
  session will be tempted to reach for — the calendar spread between two GC
  maturities — measures the *implied* carry, not the lease rate, and the difference
  between them is precisely the paper's subject.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their instruments are the lease-rate and forward-rate series plus COMEX
  inventory; ours is the futures tape. The overlap is one leg of three.
- The paper's own reported estimates are theirs and are not restated here.
- The journal is a small specialist one and the sample predates 2014, so the
  post-2015 world — in which the benchmark rate was withdrawn — is untested by it.
- The implied-carry substitute would be a genuinely interesting hypothesis in its
  own right. It should be registered as its own file if anyone wants it, not
  smuggled in under this citation.

## Triage grade

**C.** C, and the missing pieces are a **gold lease-rate or forward-rate series** and
**COMEX warehouse inventory**, neither owned and one of them discontinued mid-span.
It is the clearest example in wave 2 of a candidate whose blocker is an acquisition
that no amount of machinery removes.
