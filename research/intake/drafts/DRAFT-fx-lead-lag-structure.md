---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: fx-lead-lag-structure
topic: cross-asset-lead-lag
grade: C
hypothesis_family: fx-lead-lag-network
status: draft
blocked_on: multi-instrument configs, and a currency cross-section the archive does not hold — 6E is the only FX root owned
created: 2026-08-06
doi: 10.1016/j.physa.2019.122986
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — A lead-lag network among currency pairs at one-minute grain

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

Lasko Basnarkov, Viktor Stojkoski, Zoran Utkovski, Ljupco Kocarev. *Lead-lag Relationships in Foreign Exchange Markets*.
arXiv q-fin, 2019.
DOI `10.1016/j.physa.2019.122986`. <http://arxiv.org/abs/1906.10388v3>
Retrieved from the arxiv API on 2026-08-06.

The study looks for one-minute-ahead predictability between exchange rates using lagged correlation, lagged partial correlation and Granger tests. It reports that most pairs show nothing while a minority clear significance, assembles those survivors into a directed graph, and ranks its nodes by centrality. The authors read the result as information taking measurable time to propagate. No trading rule, position sizing or cost treatment appears anywhere in it.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.1016/j.physa.2019.122986':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

Nobody is named as paying, and there is no story here that names one. A directed graph of Granger links says one series' past helps forecast another's; it does not say a participant was slow, or that anyone took the wrong side repeatedly. Under this project's working prior that absence is the important fact rather than a detail: an edge with no payer is a statistical shape, and statistical shapes are what a screen over many ordered pairs produces for free. The paper's own framing supports the concern rather than settling it — most pairs are reported as showing nothing, so the reported links are the survivors of a large search, and what we hold records no accounting for how many comparisons produced them. The only payer a minute-scale currency lead could name is a slow quoting participant, and that population has been competed out of this business for well over a decade.

## Signal in Crucible terms

- The paper's object is a cross-section of exchange rates. The archive holds one FX root, so the cross-section has a single member and there is no network to build.
- A config admits one instrument and one timeframe. Even with three FX roots owned, no rule could read the second one.
- The lagged-correlation construction is a product of two series' returns; the grammar has no arithmetic between operands, so the statistic itself is inexpressible independently of the instrument problem.
- What IS expressible on 6E alone: `zscore(20, return)` with `enter_long: zscore_return crosses_below -2.0` and its mirror. That is a single-series mean-reversion test and is not this hypothesis; running it under this family would charge trials against a question the paper did not ask.
- Timeframe `1m` matches the paper's grain, which is the single point of agreement between the claim and the build.

## Data

- Owned: 6E `ohlcv-1m`, 2010-06-06 → 2026-07-28, curated at one minute.
- Not owned: every other currency future. The seven roots are ES, NQ, RTY, CL, GC, 6E and ZN, and there is no plan to extend the set.
- Not owned: spot or aggregator FX quotes of the kind the study appears to use. A futures bar and a spot quote are not interchangeable — the timestamps differ, and one is a trade record while the other is a quote.
- The archive's only book data is ES for one year (D-0120), so nothing here can be examined at the resolution this claim lives at.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_abs_ic = 0.05` at a one-minute forward horizon on the leading series. Basis: a seeded random walk hands over roughly 0.04 at 20,000 bars (D-0085), so a smaller threshold is the noise floor with a threshold's name on it.
- `max_permutation_p = 0.01`, block length declared before the run and swept (D-0087). Basis: the hypothesis is a survivor of a screen over many ordered pairs, so the null has to be stricter than the conventional bar to absorb a selection step we did not perform.
- The trial count charged to this family starts at the number of ordered pairs the source screened, not at one. Basis: the deflation must pay for their search as well as ours, and a family opening at one trial is arithmetic done in our own favour.
- `kill_if_dead_at_ticks = 0.5`. Basis: a one-minute cross-pair lead is a fraction of a tick per event by construction, so the half-tick row of the sweep is the one that decides it.
- `min_oos_trades = 1000` and `min_oos_sessions = 250`. Basis: a minute-scale claim should generate many events, and if it does not then it is not this claim.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- The venue is an econophysics outlet, not a finance one. Statistical structure is what such papers report and what they are refereed for; net-of-cost tradeability is generally not asked, and is not asked here.
- No cost treatment, no execution assumption and no position sizing appears in anything we hold about this paper. A significant Granger link and a tradeable edge are different objects, and the gap between them is the whole of this project.
- Their instruments are exchange rates; ours is one CME futures contract on one of them. Even a perfectly reproduced result would be a statement about a different traded thing.
- Their sample is a stretch of the late 2010s; ours runs to 2026-07-28. Any minute-scale predictability documented then has had years of public exposure since, in the most competitive market that exists.
- The paper reports its own significance counts and centrality rankings; those are its results on its data and none of them is restated here as a Crucible quantity.

## Triage grade

**C.** C, and the harder half of `missing` is the data, not the code. Multi-instrument configs are a real gap, but the archive has exactly one FX root and the seven-root set is fixed, so there is no cross-section to run a network over and no acquisition planned that would create one. Multi-instrument configs alone would leave this untestable. What 6E can answer today is a different hypothesis that happens to share a word with this one.
