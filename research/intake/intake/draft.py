"""Turning a corpus record into a registration DRAFT.

A draft is not a registration. It lands in ``drafts/``, never in
``research/backlog/``, and a human promotes it after reading the paper. The
tool's job is the mechanical half — citation, provenance, the section skeleton
in the order the backlog README fixes — and the honest marking of the half it
cannot do.

**The binding rule of the backlog is that no file contains a predicted return,
Sharpe, win rate or hit rate**, not even the paper's own. A drafter that copied
the paper's headline number into the file would break that rule at scale, which
is why :func:`draft_markdown` writes the paper's claim into the citation
section verbatim only when a human has transcribed it, and otherwise leaves a
TODO. Nothing here paraphrases an abstract into a performance expectation.

Every section the tool cannot fill is emitted as an explicit ``TODO(human)``
line saying what is owed. A draft with a blank mechanism paragraph would look
like a draft with a *considered* mechanism; the marker is the difference.
"""

from __future__ import annotations

import re
from datetime import datetime, timezone

from .sources import Paper

#: Words whose presence in a draft would break the backlog's binding rule.
#: Checked by `find_predictions`, and by the tool's own test.
PREDICTION_MARKERS = re.compile(
    r"\b(sharpe|annualized return|win rate|hit rate|cagr|profit factor|"
    r"expected return of|we expect .* return)\b",
    re.IGNORECASE,
)


def slugify(title: str) -> str:
    words = re.sub(r"[^a-z0-9\s-]", "", title.lower()).split()
    return "-".join(words[:6]) or "untitled"


def find_predictions(text: str) -> list[str]:
    """Every phrase in ``text`` that would violate the no-predictions rule.

    Used as a self-check before a draft is written: the tool refuses to emit a
    draft that breaks the rule rather than leaving it for review, because the
    whole value of the rule is that it holds without anyone policing it.
    """
    return sorted({m.group(0) for m in PREDICTION_MARKERS.finditer(text)})


def draft_markdown(paper: Paper, *, topic: str, family_hint: str) -> str:
    """Render one registration draft.

    Deliberately produces no ``id`` and no grade: both are human calls. The
    ``id`` is allocated against the backlog at promotion time — the same
    "allocate at merge, never on a branch" discipline CLAUDE.md §8.2 applies to
    decision numbers, and for the same reason, since two drafts written the
    same afternoon would otherwise both claim H-016.
    """
    authors = ", ".join(paper.authors[:6]) + (
        " et al." if len(paper.authors) > 6 else ""
    )
    doi_line = f"DOI `{paper.doi}`" if paper.doi else "**no DOI** (preprint)"
    generated = datetime.now(timezone.utc).date().isoformat()
    slug = slugify(paper.title)
    abstract_state = (
        "the index carried an abstract"
        if paper.abstract
        else "**the index carried no abstract** — the paper itself is the only source"
    )
    doi_literal = repr(paper.doi)
    title_literal = repr(paper.title[:40])

    return f"""---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: {slug}
topic: {topic}
grade: TODO(human) — A/B/C is a cost judgement the drafter cannot make
hypothesis_family: {family_hint}
status: draft
created: {generated}
source_api: {paper.source}
harvested_from: {", ".join(paper.extra.get("seen_in", [paper.source]))}
accessed: {paper.accessed}
---

# DRAFT — {paper.title}

> **This is a DRAFT, not a registration.** It was generated from API metadata by
> `research/intake` and nobody has read the paper. Promote it into
> `research/backlog/` by hand, after reading, or delete it. Every
> `TODO(human)` below is a thing the tool could not honestly do.

## Citation

{authors + ". " if authors else ""}*{paper.title}*.
{paper.venue or "venue unrecorded"}{f", {paper.year}" if paper.year else ""}.
{doi_line}. {f"<{paper.url}>" if paper.url else ""}
Retrieved from the {paper.source} API on {paper.accessed}.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.

**The abstract is deliberately not reproduced here.** Read it in the corpus
record ({abstract_state}) or in the paper itself:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == {doi_literal} or r["title"].startswith({title_literal}):
        print(r["abstract"]); break
PY
```

Two reasons, and both are rules this repository already holds. A source
paper's own performance figures belong in the **Honesty note** and nowhere
else (`research/backlog/README.md` §1) — an abstract routinely leads with the
paper's own headline performance figure, so embedding one here would put that
figure in the Citation section by construction. And an abstract is
third-party prose: the corpus is gitignored precisely so this repository does
not carry other people's copyrighted text, and a draft that quoted it would
have committed the same text the corpus rule exists to keep out.

An abstract is not a substitute for the claim in any case: it states what the
authors set out to show, and the registration needs what they claim to have
shown.

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
"""
