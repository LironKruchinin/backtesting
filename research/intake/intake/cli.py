"""`python -m intake` — harvest a topic, then draft from the corpus.

Two verbs, deliberately separate:

* ``harvest`` talks to the four APIs and appends to the corpus. It is the only
  verb that touches the network.
* ``draft`` reads the corpus and writes drafts. It touches no network at all,
  so a draft can be regenerated forever without spending anyone's rate limit.

Splitting them is the same seam the Rust side keeps between `pull` and
`transcode`: acquisition is expensive and rate-limited, derivation is free and
repeatable, and mixing them makes the cheap half inherit the expensive half's
failure modes.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import corpus, draft
from .sources import SOURCES, DisallowedHost

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CORPUS = ROOT / "corpus" / "papers.jsonl"
DEFAULT_DRAFTS = ROOT / "drafts"
TOPICS = ROOT / "topics"

EXIT_OK = 0
EXIT_USAGE = 2
EXIT_PARTIAL = 4


def load_topic(name: str) -> dict:
    path = TOPICS / f"{name}.json"
    if not path.exists():
        available = ", ".join(sorted(p.stem for p in TOPICS.glob("*.json"))) or "none"
        raise SystemExit(f"unknown topic {name!r}; available: {available}")
    return json.loads(path.read_text(encoding="utf-8"))


def cmd_harvest(args: argparse.Namespace) -> int:
    topic = load_topic(args.topic)
    corpus_path = Path(args.corpus)
    total, failures = 0, []
    for query in topic["queries"]:
        for source_name in args.sources:
            harvester = SOURCES[source_name]
            try:
                papers = list(harvester(query, args.limit))
            except DisallowedHost:
                raise
            except Exception as error:  # noqa: BLE001 — reported, never swallowed
                failures.append(f"{source_name} / {query!r}: {error}")
                print(f"  {source_name:16} {query!r}: FAILED — {error}", file=sys.stderr)
                continue
            written = corpus.append(corpus_path, papers)
            total += written
            with_doi = sum(1 for p in papers if p.doi)
            print(f"  {source_name:16} {query!r}: {written} records, {with_doi} with DOI")
    print(f"\n{total} records appended to {corpus_path}")
    if failures:
        # Exit 4 for "ran, and found something you must look at" — the same
        # contract the Rust commands use. A harvest that lost a source and
        # reported success would silently narrow the corpus.
        print(f"{len(failures)} source/query pair(s) failed", file=sys.stderr)
        return EXIT_PARTIAL
    return EXIT_OK


def cmd_draft(args: argparse.Namespace) -> int:
    corpus_path = Path(args.corpus)
    papers = corpus.dedupe(corpus.read(corpus_path))
    if not papers:
        print(f"no records in {corpus_path}; run `harvest` first", file=sys.stderr)
        return EXIT_USAGE
    topic = load_topic(args.topic)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    selected = [p for p in papers if p.abstract][: args.count]
    if not selected:
        print("no record in the corpus has an abstract to draft from", file=sys.stderr)
        return EXIT_USAGE

    written = 0
    for paper in selected:
        text = draft.draft_markdown(
            paper, topic=args.topic, family_hint=topic["hypothesis_family_hint"]
        )
        # The tool refuses to emit a draft that breaks the backlog's binding
        # rule, rather than emitting it for review. A rule enforced only at
        # review is a rule that holds until someone is busy.
        offences = draft.find_predictions(text)
        if offences:
            print(
                f"REFUSED {paper.title[:60]!r}: draft contains prediction "
                f"language {offences}",
                file=sys.stderr,
            )
            continue
        path = out / f"DRAFT-{draft.slugify(paper.title)}.md"
        path.write_text(text, encoding="utf-8", newline="\n")
        print(f"  wrote {path.relative_to(ROOT)}")
        written += 1
    print(f"\n{written} draft(s) in {out}. None of them is a registration.")
    return EXIT_OK


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="intake", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    h = sub.add_parser("harvest", help="query the four APIs and append to the corpus")
    h.add_parser = None  # type: ignore[attr-defined]
    h.add_argument("--topic", required=True)
    h.add_argument("--corpus", default=str(DEFAULT_CORPUS))
    h.add_argument("--limit", type=int, default=20)
    h.add_argument(
        "--sources", nargs="+", default=sorted(SOURCES), choices=sorted(SOURCES)
    )
    h.set_defaults(func=cmd_harvest)

    d = sub.add_parser("draft", help="write registration drafts from the corpus")
    d.add_argument("--topic", required=True)
    d.add_argument("--corpus", default=str(DEFAULT_CORPUS))
    d.add_argument("--out", default=str(DEFAULT_DRAFTS))
    d.add_argument("--count", type=int, default=1)
    d.set_defaults(func=cmd_draft)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
