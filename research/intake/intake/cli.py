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
from dataclasses import replace
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
            for page in range(args.pages):
                label = f"{source_name:16} {query!r} p{page}"
                try:
                    papers = list(harvester(query, args.limit, page))
                except DisallowedHost:
                    raise
                except Exception as error:  # noqa: BLE001 — reported, never swallowed
                    failures.append(f"{source_name} / {query!r} / page {page}: {error}")
                    print(f"  {label}: FAILED — {error}", file=sys.stderr)
                    # The next page of a source that just failed is very likely
                    # to fail the same way — a 429 in particular — so stop
                    # paging this pair rather than spending the throttle on it.
                    # Each failure is still reported individually above.
                    break
                if not papers:
                    # An empty page is the end of the result set, not an error:
                    # every one of the four APIs answers a past-the-end offset
                    # with an empty list rather than a 404.
                    print(f"  {label}: exhausted")
                    break
                # Stamped here rather than inside each harvester: a harvester's
                # job is one API, and the topic is a property of the sweep that
                # called it. `draft --topic` reads this back, and without it
                # every topic drafts the same head of the same corpus.
                papers = [replace(p, topic=args.topic) for p in papers]
                written = corpus.append(corpus_path, papers)
                total += written
                with_doi = sum(1 for p in papers if p.doi)
                print(f"  {label}: {written} records, {with_doi} with DOI")
    print(f"\n{total} records appended to {corpus_path}")
    if failures:
        # Exit 4 for "ran, and found something you must look at" — the same
        # contract the Rust commands use. A harvest that lost a source and
        # reported success would silently narrow the corpus.
        print(f"{len(failures)} source/query pair(s) failed", file=sys.stderr)
        return EXIT_PARTIAL
    return EXIT_OK


def in_topic(paper, name: str, topic: dict) -> bool:
    """Does ``paper`` belong to the topic ``name``?

    Two answers are accepted, and the second exists only for corpus records
    written before `Paper.topic` did. The stamped topic is authoritative; the
    query fallback is a best effort over history and is documented as such
    rather than being silently relied on.
    """
    if paper.topic:
        return paper.topic == name or name in (paper.extra.get("topics") or [])
    return paper.query in topic["queries"]


def cmd_draft(args: argparse.Namespace) -> int:
    corpus_path = Path(args.corpus)
    papers = corpus.dedupe(corpus.read(corpus_path))
    if not papers:
        print(f"no records in {corpus_path}; run `harvest` first", file=sys.stderr)
        return EXIT_USAGE
    topic = load_topic(args.topic)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    # `--topic` used to label the front matter and nothing else, so every topic
    # drafted the same head of the same corpus and the slug-based filenames
    # overwrote one another. Filtering here is what makes twelve topic runs
    # produce twelve different sets of drafts.
    pool = [p for p in papers if in_topic(p, args.topic, topic)]
    if not pool:
        print(
            f"no record in {corpus_path} belongs to topic {args.topic!r}; "
            "harvest it first",
            file=sys.stderr,
        )
        return EXIT_USAGE
    if args.min_year is not None:
        pool = [p for p in pool if (p.year or 0) >= args.min_year]

    with_abstract = [p for p in pool if p.abstract]
    selected = with_abstract[args.offset : args.offset + args.count]
    if not selected:
        print(
            f"topic {args.topic!r} has {len(with_abstract)} abstract-bearing "
            f"record(s); offset {args.offset} selects none of them",
            file=sys.stderr,
        )
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
        # `--out` is allowed to point outside the package, so the tidy relative
        # spelling is a nicety and not a guarantee. It used to be a bare
        # `relative_to(ROOT)`, which raises rather than falling back.
        try:
            shown = path.relative_to(ROOT)
        except ValueError:
            shown = path
        print(f"  wrote {shown}")
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
    h.add_argument(
        "--limit",
        type=int,
        default=20,
        help="records per (query x source x page). Semantic Scholar caps at "
        "100 and OpenAlex at 200; nothing here clamps, so an over-limit "
        "request is reported as a failed source rather than silently truncated",
    )
    h.add_argument(
        "--pages",
        type=int,
        default=1,
        help="pages per (query x source). Each of the four APIs spells paging "
        "differently and each translates its own; an empty page ends the pair",
    )
    h.add_argument(
        "--sources", nargs="+", default=sorted(SOURCES), choices=sorted(SOURCES)
    )
    h.set_defaults(func=cmd_harvest)

    d = sub.add_parser("draft", help="write registration drafts from the corpus")
    d.add_argument("--topic", required=True)
    d.add_argument("--corpus", default=str(DEFAULT_CORPUS))
    d.add_argument("--out", default=str(DEFAULT_DRAFTS))
    d.add_argument("--count", type=int, default=1)
    d.add_argument(
        "--offset",
        type=int,
        default=0,
        help="skip this many abstract-bearing records of the topic. Selection "
        "is head-of-list off the dedupe's (-year, title) sort, so without an "
        "offset a second run re-drafts exactly what the first one did",
    )
    d.add_argument(
        "--min-year",
        type=int,
        default=None,
        help="drop records published before this year. A record whose index "
        "carried no year is dropped by this filter, which is why it is off by "
        "default",
    )
    d.set_defaults(func=cmd_draft)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
