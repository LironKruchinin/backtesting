---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: overnight-news-returns
topic: overnight-intraday
grade: C
hypothesis_family: equity-overnight-news-attribution
status: draft
blocked_on: a timestamped news corpus and its availability rule; also a cash-equity session structure that a 23-hour futures market does not have
created: 2026-08-06
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Attributing the overnight/intraday split in US equities to news topics

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

Paul Glasserman, Kriste Krstovski, Paul Laliberte, Harry Mamaysky. *Does Overnight News Explain Overnight Returns?*.
arXiv q-fin, 2025.
**no DOI** (preprint). <http://arxiv.org/abs/2507.04481v1>
Retrieved from the arxiv API on 2026-08-06.

Starting from the observation that essentially all of the US stock market's long-run gain has accrued outside of trading hours while the in-hours component has been flat or worse, the authors use a large archive of news articles and a topic method chosen for its ability to track same-period returns. They argue that both when topics appear and how prices respond to them contribute to the split, and report out-of-sample forecasts of which names do well overnight and badly during the day.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == None:
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The overnight/intraday split in cash equities has a well-worn structural explanation: the market is shut for seventeen hours, so the overnight move is a single jump between two auctions with no trading in between, and whoever must transact at the open — retail orders queued overnight, funds tracking the opening print — pays whoever carried the risk through the night. Market makers unwinding inventory into the close are the other half. That names a payer, and it also names why this candidate is hard: the payer exists because the venue closes. ES trades roughly twenty-three hours a day and has no comparable gap, so the overnight window we can define is a stretch of thin continuous trading rather than a halt. The paper's addition is that news topics explain much of the split, which requires knowing when each article became knowable — an availability rule that would have to be designed before a single article could enter the engine.

## Signal in Crucible terms

- Faithful construction: a cross-section of US single names, an article-level news archive with publication timestamps, and a supervised topic model. None of the three exists here and none is on a milestone.
- The `avail_ts` question comes first, not last: 'as known when?' has to be answered for a news article before it can be joined to anything (§2.1), and a wire timestamp, an embargo lift and a first-index time are three different answers.
- The expressible fragment is the pattern, not the explanation: `is_overnight` / `is_rth` on `ESM2024` splits the return into two blocks. That is H-002's territory, it says nothing about news, and a positive result there would be evidence for several mechanisms at once.
- Even the pattern is compromised: their overnight block is a closure and ours is a thin trading session, so the two are not the same object measured on two instruments — they are two different objects.
- Their forecasts are cross-sectional (which stocks, not whether the index). Cross-sectional accounting is post-M4.

## Data

- Owned: ES `ohlcv-1m` from 2010, enough to split returns by session block with a real calendar behind it.
- Not owned: any news corpus, at any timestamp granularity. There is no acquisition proposed and no loader; this is a purchase and a design problem, not a coding task.
- Not owned: US single-name equity data. The project has deliberately declined an equities subscription, so the cross-sectional forecasts have no path here.
- Not owned: opening and closing auction prints. A futures contract has no auction of the kind the mechanism turns on.
- `half_spread_ticks = 1` (D-0120) applies uniformly, and an overnight-hold rule pays it at both ends of every session it trades.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- No run is authorized under this key until a news corpus with a written availability rule exists. These are registered for the descendant so they are ready if that ever changes.
- `min_oos_sessions = 500` — basis: an effect claimed over three decades should be measurable across two years without needing the other twenty-eight.
- `min_oos_trades = 250` — basis: a session-block rule fires roughly once a session, so this is two years of firings and no fewer.
- `kill_if_dead_at_ticks = 1.0` — basis: an overnight-hold rule pays the spread twice per session with no intraday compounding, so the 2-tick column of the mandatory sweep is the one to read and one tick is a generous test.
- The kill that matters: if the effect is concentrated in fewer than 20 % of sessions, it is an event effect, the news attribution cannot be separated from the events themselves without the corpus, and the hypothesis is Killed here rather than escalated.
- `max_permutation_p = 0.05` and `require_controls_beaten = true` — basis: an overnight-long rule on an index in a bull sample is buy-and-hold with extra steps, and the buy-and-hold control is what catches that.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- A 2025 arXiv preprint with no refereed venue recorded. Nobody has read it; this restatement comes from the indexed abstract alone.
- Their evidence base is 2.4 million articles and a cross-section of US stocks. We have neither, and the equities subscription was declined on purpose for reasons that have not changed.
- The headline stylized fact is well documented elsewhere and is also well known to be concentrated in a small number of event days — which is why the concentration criterion above is a kill and not a note.
- The mechanism's premise is a closed market. Our instrument trades nearly around the clock, so even a perfect replication here would be testing something the paper is not about.
- The paper reports its own out-of-sample results; they are not restated here, and they are cross-sectional stock selection, which this build cannot express or account for.
- A supervised topic model selected on its ability to track contemporaneous returns is a fitting procedure with a large search space. Whatever they did to control that, we cannot inspect it.

## Triage grade

**C.** C stands, and this is the batch's clearest genuine data gap rather than a code gap. The missing piece is a timestamped news corpus plus an availability rule that has to be designed before any join — a purchase and a §2.1 design decision, not a milestone item. The second missing piece cannot be bought at all: their mechanism needs a market that closes, and ours does not.
