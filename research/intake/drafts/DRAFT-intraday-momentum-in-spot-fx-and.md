---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: intraday-momentum-in-spot-fx-and
topic: momentum-horizon
grade: TODO(human) — A/B/C is a cost judgement the drafter cannot make
hypothesis_family: TODO(human) — one family for the whole idea, not one parameterization
status: draft
created: 2026-08-04
source_api: crossref
harvested_from: crossref
accessed: 2026-08-04
---

# DRAFT — Intraday Momentum in Spot FX and Currency Futures: Signal Persistence, the JPY Amplification Mechanism, and the Cost Barrier to Retail Exploitability

> **This is a DRAFT, not a registration.** It was generated from API metadata by
> `research/intake` and nobody has read the paper. Promote it into
> `research/backlog/` by hand, after reading, or delete it. Every
> `TODO(human)` below is a thing the tool could not honestly do.

## Citation

Leander Seeck. *Intraday Momentum in Spot FX and Currency Futures: Signal Persistence, the JPY Amplification Mechanism, and the Cost Barrier to Retail Exploitability*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.7008318`. <https://doi.org/10.2139/ssrn.7008318>
Retrieved from the crossref API on 2026-08-04.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
The abstract is reproduced below as harvested and is **not** a substitute:
an abstract states what the authors set out to show, and the registration needs
what they actually claim to have shown.

<details><summary>Harvested abstract (unedited, third-party text)</summary>

<jats:p>We test the intraday momentum effect documented by Gao et al. (2018) and Baltussen et al. (2021) across five spot FX CFD instruments and one currency futures contract. Using M5 Dukascopy data for spot pairs (2012–2024) and M1 Databento data for 6J CME futures (2019–2024), with a strict in-sample/out-of-sample split (IS: 2012–2018; OOS: 2019–2024), we document three principal findings. First, the London Open 30-minute sign signal is statistically significant on five of six instruments in both periods (permutation p &amp;lt; 0.001), with GBPUSD exhibiting a reversed signal direction consistent with the absence of JPY carry-trade amplification, confirming that the intraday momentum mechanism extends to retail market structures. Second, we identify a JPY Amplification Mechanism: JPY-denominated instruments exhibit regression coefficients approximately 3.8× larger than non-JPY pairs (average OOS β: 0.000859 vs. 0.000226), with GBPUSD treated separately due to its structurally reversed signal direction, consistent with the structural role of JPY as a global risk-sentiment amplifier and carry-trade funding currency. Third, after applying realistic round-trip transaction costs from actual platform data, USDJPY spot is the sole instrument producing a positive cost-adjusted edge (OOS Sortino: +0.748), while four spot pairs and 6J futures fail to clear their respective cost hurdles when standard prop-firm position sizing constraints are applied. A direct spot-futures comparison reveals a mechanistically important divergence in 2022: while BOJ Yield Curve Control interventions destroyed the spot signal, 6J futures maintained positive performance (annual Sharpe: +0.383 vs. −0.557), indicating that futures microstructure partially insulates the signal from central bank intervention. Regime filter tests confirm the USDJPY baseline is filter-robust. These results establish that the intraday momentum mechanism is pervasive across retail FX instruments but faces a steep cost barrier, with JPY carry dynamics as the primary exploitable source of intraday directional information.</jats:p>

</details>

## Mechanism

TODO(human) — one paragraph: why this could work, and **who is on the losing
side**. That second half is required, not decoration. A strategy is a claim
that someone is systematically paying you; if you cannot name who and why they
keep doing it, say so here and the grade follows from that.

## Signal in Crucible terms

TODO(human) — instruments, timeframe, features, rules, in §4's vocabulary.
Timeframes are spelled from the fixed set (`1s 1m 5m 15m 1h 1d`); instruments
are Databento symbols or the continuous aliases.

## Data

TODO(human) — what the archive already holds for this, and what it lacks,
flagged explicitly. `crucible coverage` answers the first half.

## Pre-registered kill criteria

TODO(human) — numeric, chosen **now**, judged by machines. Written before any
equity curve exists, or the file is a rationalization with a date on it.

No predicted performance figure of any kind appears anywhere in this file,
including in this section — the backlog's binding rule. Kill criteria are
floors a machine checks, not performance anyone expects.

(That sentence is deliberately worded without naming the specific metrics it
bans. `find_predictions` scans the drafter's own output, and the first version
of this template listed them by name — so every draft was refused by the rule's
own restatement of itself. Same failure as a grep that matches its own
documentation: a check that fires on its own boilerplate gets disabled within a
week, which is worse than not having it.)

## Honesty note

TODO(human) — their data against ours, sample overlap, known biases. If the
paper's sample is a market or an era this archive does not cover, that belongs
here rather than being discovered at S2.

## Triage grade

TODO(human) — the grade, and the **specific** reason for it. The grade answers
"what does it cost to test this?" and nothing else; a grade C with honest
reasons outranks an inflated grade A.
