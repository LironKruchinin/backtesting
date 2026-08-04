"""The append-only corpus, and the dedupe that reads it.

The corpus is JSONL for the same reason the run registry is (D-0074): append
is the only write, so a harvest can be interrupted without corrupting what came
before, and a later reader folds the file rather than trusting an index.

It is **gitignored**. The records are third-party metadata and abstracts, they
are rebuildable from the APIs at any time, and committing them would put other
people's copyrighted text in this repository. What *is* committed is the tool
and the drafts a human promoted.
"""

from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path
from typing import Iterable, Iterator

from .sources import Paper


def append(path: Path, papers: Iterable[Paper]) -> int:
    """Append records, returning how many were written."""
    path.parent.mkdir(parents=True, exist_ok=True)
    written = 0
    with path.open("a", encoding="utf-8", newline="\n") as handle:
        for paper in papers:
            handle.write(paper.to_json() + "\n")
            written += 1
    return written


def read(path: Path) -> Iterator[Paper]:
    """Replay every record. A malformed line raises rather than being skipped."""
    if not path.exists():
        return
    with path.open("r", encoding="utf-8") as handle:
        for number, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                raw = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"{path}:{number} is not JSON: {error}") from error
            yield Paper(**raw)


def _key(paper: Paper) -> str:
    """Dedupe key: the DOI when there is one, else title+year.

    A DOI is the identity two indexes agree on, which is the whole reason to
    query four of them. Records with no DOI — arXiv preprints, mostly — fall
    back to a normalized title, which is weaker but keeps them in the corpus:
    dropping every DOI-less record would quietly bias the corpus toward
    published work and away from exactly the preprints this field lives on.
    """
    if paper.doi:
        return f"doi:{paper.doi}"
    title = " ".join(paper.title.lower().split())
    return f"title:{title}|{paper.year or ''}"


def dedupe(papers: Iterable[Paper]) -> list[Paper]:
    """Collapse duplicates, keeping the record with the most content.

    Merged rather than first-wins: Crossref usually has the DOI and venue while
    Semantic Scholar usually has the abstract, so first-wins would throw away
    whichever half arrived second. The surviving record names every source it
    was seen in, so provenance survives the merge instead of being replaced by
    the winner's.
    """
    best: dict[str, Paper] = {}
    seen_in: dict[str, list[str]] = {}
    for paper in papers:
        key = _key(paper)
        seen_in.setdefault(key, [])
        if paper.source not in seen_in[key]:
            seen_in[key].append(paper.source)
        current = best.get(key)
        if current is None:
            best[key] = paper
            continue
        merged = replace(
            current,
            abstract=current.abstract or paper.abstract,
            doi=current.doi or paper.doi,
            venue=current.venue or paper.venue,
            year=current.year or paper.year,
            url=current.url or paper.url,
            authors=current.authors or paper.authors,
        )
        best[key] = merged
    out = []
    for key, paper in best.items():
        extra = dict(paper.extra)
        extra["seen_in"] = sorted(seen_in[key])
        out.append(replace(paper, extra=extra))
    # Sorted so two runs over the same corpus produce the same order — the
    # same determinism argument the Rust side makes about merging by identity.
    out.sort(key=lambda p: (-(p.year or 0), p.title.lower()))
    return out
