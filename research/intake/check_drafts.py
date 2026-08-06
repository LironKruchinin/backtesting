"""Run both binding-rule checks over every draft in `drafts/`.

`python -m intake draft` runs `find_predictions` on its own output, but a draft
a human then edits is outside that check — and every draft in `drafts/` is
hand-written over the generated skeleton. So the check has to be re-runnable
against the directory rather than against the drafter.

`find_reproduced_prose` needs the abstract, which lives only in the gitignored
corpus, so this script joins each draft back to its corpus record by DOI
(falling back to a title prefix for the DOI-less preprints). A draft whose
record cannot be found is REPORTED, not skipped: an unchecked draft that prints
nothing looks exactly like a clean one, which is the unfired-detector failure
CLAUDE.md §7 names.

Exit 0 clean, 4 if anything was found or could not be checked.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from intake.draft import find_predictions, find_reproduced_prose  # noqa: E402

ROOT = Path(__file__).resolve().parent
CORPUS = ROOT / "corpus" / "papers.jsonl"
DRAFTS = ROOT / "drafts"


def load_abstracts() -> tuple[dict[str, tuple[str, str]], list[tuple[str, str, str]]]:
    """Corpus records that carry an abstract, keyed for lookup.

    Values are ``(title, abstract)`` rather than the abstract alone, because the
    title is needed to apply the citation exemption below.
    """
    by_doi: dict[str, tuple[str, str]] = {}
    by_title: list[tuple[str, str, str]] = []
    if not CORPUS.exists():
        return by_doi, by_title
    for line in CORPUS.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        abstract = r.get("abstract")
        if not abstract:
            continue
        title = r.get("title", "")
        if r.get("doi"):
            by_doi.setdefault(r["doi"].lower(), (title, abstract))
        by_title.append((title.lower(), title, abstract))
    return by_doi, by_title


def without_cited_title(text: str, title: str) -> str:
    """Drop the paper's own title where the citation states it.

    A citation must name the title, so the title is the one span of third-party
    text a draft is *required* to reproduce — it is attribution, not prose. The
    exemption is deliberately narrow: only the exact italicised span the citation
    block writes, `*<title>*`, and only for the title of the record this draft
    was matched to. Everything else in the file, including any other occurrence
    of those words, is still scanned.

    Found by wave 2's checker firing on two wave-1 drafts whose paper titles
    happen to be repeated inside their own abstracts
    (`index-futures-return-dependence`, `inventories-and-oil-basis`). Both hits
    were the citation line and nothing else, so the alternative to this
    exemption was mangling two citations to satisfy a check — which is the wrong
    direction, and the sort of "fix" CLAUDE.md §7 refuses.
    """
    return text.replace(f"*{title}*", " ") if title else text


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    # A wave's drafts are checkable only against the corpus that produced them,
    # and the corpus is gitignored, so a later wave cannot re-check an earlier
    # one's prose. `--only` names the files this run is answering for; without
    # it every draft is attempted and the ones with no record are counted as
    # UNCHECKED rather than passed.
    only = set(argv[argv.index("--only") + 1 :]) if "--only" in argv else None
    by_doi, by_title = load_abstracts()
    offences = 0
    unchecked = 0
    checked = 0
    for path in sorted(DRAFTS.glob("DRAFT-*.md")):
        if only is not None and path.name not in only:
            continue
        text = path.read_text(encoding="utf-8")
        preds = find_predictions(text)
        if preds:
            print(f"PREDICTION  {path.name}: {preds}")
            offences += 1
        doi_match = re.search(r"^doi: (.+)$", text, re.M)
        doi = (doi_match.group(1).strip().lower() if doi_match else "null")
        record = by_doi.get(doi)
        if record is None:
            # A preprint legitimately has no DOI, and the draft's own H1 is the
            # drafter's title rather than the paper's — so matching on it finds
            # nothing and the draft would be scored UNCHECKED forever. The
            # citation block's lookup snippet carries the paper's real title
            # prefix, which is the only place in the file that does.
            probe_match = re.search(r'r\["title"\]\.startswith\((.+?)\)', text)
            probe = ""
            if probe_match:
                probe = probe_match.group(1).strip().strip("'\"").lower()
            for lowered, title, candidate in by_title:
                if probe and lowered.startswith(probe):
                    record = (title, candidate)
                    break
        if record is None:
            unchecked += 1
            print(f"UNCHECKED   {path.name}: no corpus abstract for doi {doi!r}")
            continue
        checked += 1
        title, abstract = record
        runs = find_reproduced_prose(without_cited_title(text, title), abstract)
        if runs:
            print(f"REPRODUCED  {path.name}: {runs}")
            offences += 1
    print(
        f"\n{checked} draft(s) checked against a corpus abstract, "
        f"{unchecked} without one, {offences} offence(s)."
    )
    return 4 if (offences or unchecked) else 0


if __name__ == "__main__":
    raise SystemExit(main())
