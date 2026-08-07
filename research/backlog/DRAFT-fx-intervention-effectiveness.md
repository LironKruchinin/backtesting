---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: fx-intervention-effectiveness
topic: fx-intervention-central-bank
grade: C
hypothesis_family: fx-official-intervention-response
status: draft
blocked_on: intervention event dates, and an instrument whose central bank intervenes
created: 2026-08-07
doi: 10.1257/mac.20150317
source_api: openalex
harvested_from: openalex
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Official FX intervention works, mostly in regimes the euro is not in

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

Marcel Fratzscher, Oliver Gloede, Lukas Menkhoff, Lucio Sarno, Tobias Stöhr. *When Is Foreign Exchange Intervention Effective? Evidence from 33 Countries*.
American Economic Journal Macroeconomics, 2019.
DOI `10.1257/mac.20150317`. <https://openalex.org/W3124976919>
Retrieved from the openalex API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the authors assemble daily intervention records for
thirty-three countries over 1995–2011 and report that intervention is common and
often achieves its stated aim, that it works best at smoothing moves and at holding
a rate inside a declared band, and that shifting the level of a floating rate needs
size, publicity and supporting communication.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1257/mac.20150317':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A central bank intervening is the one participant in a market who is explicitly not
maximising profit, which makes it the cleanest possible counterparty for a
speculator — and the paper's result says so: it works when the target is narrow and
the bank is loud. The payer, in a band regime, is anyone fighting the band. But the
finding that matters for this archive is the boundary condition, and it is a
negative one: moving the *level* of a *floating* rate is the hard case, and the
euro is a large floating currency whose central bank has intervened in it
essentially twice in this archive's lifetime. The mechanism is real and our
instrument is the wrong side of its boundary.

## Signal in Crucible terms

- Not expressible, and not close. Intervention days are the entire signal and no
  operand names a date or an event.
- The deeper problem is the instrument, not the grammar. The archive's only FX root
  is 6E, and coordinated intervention in the euro is close to absent over
  2010–2026, so even with a perfect event calendar the sample would be a handful of
  days.
- No expressible substitute exists. Trailing volatility is not intervention, and a
  large-move filter would select every macro shock rather than the official ones.

## Data

- Owned: 6E `ohlcv-1m` from 2010-06-06. The price leg is complete.
- Not owned: intervention records. Some central banks publish them with a lag and
  some do not publish them at all, and the lag *is* the availability rule (§2.1) —
  an intervention known only in the following month's report is not tradeable
  information on the day.
- Not owned: any of the thirty-two other currencies in the paper's panel, which is
  where nearly all of its events live.
- Sample: their window ends in 2011 and ours begins in 2010, so the overlap is about
  eighteen months at the very start of our archive.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- Not registrable, and the reason is worth stating precisely because it is unusual
  in this directory: the block is not machinery and not data alone — **the
  phenomenon is largely absent from the market we own**. A file that registered
  thresholds anyway would be a pre-registration for an experiment with no events.
- The one thing worth recording: if a future session acquires an intervention
  calendar, it should check the euro event count *before* writing criteria. A test
  with five events is not a test.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- A large, careful, refereed panel study — and its instruments are mostly managed
  and emerging-market currencies, which is where interventions happen. Ours is the
  euro.
- The paper's own reported success rates are theirs and are not restated here.
- Recording this candidate as a C is more useful than not recording it: the
  intervention literature looks superficially like a rich seam for FX futures, and
  the reason it is not is a fact about the instrument we hold rather than about the
  literature. A future harvest that re-finds this seam should find this note first.
- The verbal-intervention branch — statements rather than transactions — is a
  separate and larger literature and would need a timestamped statement corpus,
  which is a further acquisition again.

## Triage grade

**C.** C, and there are two missing pieces of different kinds: **intervention event dates
with an availability rule**, and **an instrument whose central bank actually
intervenes**. The second is not purchasable in any useful sense — it is a property
of the euro — which puts this candidate closer to the safe-haven and price-discovery
entries than to the calendar-blocked ones.
