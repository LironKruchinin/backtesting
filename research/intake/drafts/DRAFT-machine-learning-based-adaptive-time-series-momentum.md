---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: machine-learning-based-adaptive-time-series-momentum
topic: momentum-horizon
grade: TODO(human) — A/B/C is a cost judgement the drafter cannot make
hypothesis_family: TODO(human) — one family for the whole idea, not one parameterization
status: draft
created: 2026-08-04
source_api: crossref
harvested_from: crossref
accessed: 2026-08-04
---

# DRAFT — Machine Learning-Based Adaptive Time Series Momentum Strategies in Equity Index Futures: A Comparative Analysis Between S&amp;P 500 and CSI 300 Futures Markets

> **This is a DRAFT, not a registration.** It was generated from API metadata by
> `research/intake` and nobody has read the paper. Promote it into
> `research/backlog/` by hand, after reading, or delete it. Every
> `TODO(human)` below is a thing the tool could not honestly do.

## Citation

Qiumei Li, Xuwen Huang, Ke Huang, Zuominyang Zhang. *Machine Learning-Based Adaptive Time Series Momentum Strategies in Equity Index Futures: A Comparative Analysis Between S&amp;P 500 and CSI 300 Futures Markets*.
venue unrecorded, 2026.
DOI `10.20944/preprints202603.1400.v1`. <https://doi.org/10.20944/preprints202603.1400.v1>
Retrieved from the crossref API on 2026-08-04.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
The abstract is reproduced below as harvested and is **not** a substitute:
an abstract states what the authors set out to show, and the registration needs
what they actually claim to have shown.

<details><summary>Harvested abstract (unedited, third-party text)</summary>

<jats:p>This paper employs machine learning techniques based on market volatility to identify and construct trading signals for both short-term and long-term Time Series Momentum (TSM) strategies. Through a comparative study of China&amp;#039;s CSI 300 Index and the U.S. S&amp;amp;amp;P 500 Index, we conduct an empirical analysis from a cross-market perspective. The findings reveal that the performance of time series momentum strategies is jointly determined by their signal responsiveness and the prevailing market volatility regime. Using the Random Forest algorithm, this study effectively identifies critical thresholds for regime switching between low-volatility and high-volatility states in index futures markets. The empirical results demonstrate that during high-volatility periods, short-term TSM strategies significantly outperform their long-term counterparts, whereas the opposite holds true in low-volatility environments. Further analysis indicates that the short-term momentum alpha can be attributed to market timing ability. Our findings provide important theoretical and practical implications for optimizing trend-following strategies in commodity and financial futures markets through machine learning approaches.</jats:p>

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
