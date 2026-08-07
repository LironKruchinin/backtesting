"""Controls for the two things this tool promises.

`draft.py` claimed its no-predictions check was covered "by the tool's own
test" and no such test existed. A detector nobody has watched fire is
decoration (CLAUDE.md §7, and that rule has no quality exemption), so the claim
is repaired by writing the tests rather than by deleting the sentence.

Each control below was watched failing against a planted defect before being
committed; what each one caught is recorded in its docstring.

Run them by hand — the project's CI is cargo-only and does not know this
directory exists:

    cd research/intake && python -m unittest discover -s tests -v
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
import urllib.parse
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from intake import cli, corpus, draft, sources  # noqa: E402


def _paper(**over) -> sources.Paper:
    base = dict(
        source="crossref",
        source_id="10.1234/x",
        title="A Study Of Something",
        abstract="An abstract that mentions a Sharpe ratio of 2.0.",
        year=2024,
        venue="Journal",
        authors=["A. Author"],
        doi="10.1234/x",
        url="https://doi.org/10.1234/x",
        accessed="2026-08-04",
        query="q",
        extra={},
    )
    base.update(over)
    return sources.Paper(**base)


class HostAllowlist(unittest.TestCase):
    """The compliance surface. Caught: an allowlist that only ran on `_get`."""

    def test_the_four_official_apis_are_allowed(self):
        for host in sources.ALLOWED_HOSTS:
            self.assertEqual(sources.check_host(f"https://{host}/x?y=1"), host)

    def test_scholar_and_ssrn_are_refused_by_name(self):
        for url in (
            "https://scholar.google.com/scholar?q=momentum",
            "https://www.ssrn.com/abstract=123",
            "https://papers.ssrn.com/sol3/papers.cfm?abstract_id=1",
            "http://evil.example.com/api",
        ):
            with self.assertRaises(sources.DisallowedHost):
                sources.check_host(url)

    def test_a_redirect_off_an_allowed_host_is_refused(self):
        """The allowlist must hold on every hop, not only on the URL we typed.

        Watched failing against the pre-fix code, which called `urlopen` and so
        followed redirects through urllib's default opener with no check at
        all.
        """
        handler = sources._AllowlistRedirectHandler()
        with self.assertRaises(sources.DisallowedHost):
            handler.redirect_request(
                None, None, 301, "Moved", {}, "https://scholar.google.com/x"
            )

    def test_the_opener_installs_the_allowlist_handler(self):
        self.assertTrue(
            any(
                isinstance(h, sources._AllowlistRedirectHandler)
                for h in sources._OPENER.handlers
            ),
            "every request must go through an opener that re-checks redirects",
        )


class NoPredictedPerformance(unittest.TestCase):
    """The backlog's binding rule, enforced in the tool rather than at review."""

    def test_a_planted_marker_is_found(self):
        """Caught: a `find_predictions` that returned `[]` unconditionally."""
        for planted in (
            "we expect a Sharpe of 1.8",
            "the annualized return should be 14%",
            "a win rate near 55%",
            "hit rate of 60%",
            "CAGR of 12%",
            "profit factor 1.6",
        ):
            with self.subTest(planted=planted):
                self.assertTrue(
                    draft.find_predictions(planted),
                    f"the check must see {planted!r}",
                )

    def test_ordinary_registration_prose_is_not_flagged(self):
        """The converse. Without it, a checker that flagged everything would
        pass the test above and refuse every draft ever written."""
        clean = (
            "Kill criteria are floors a machine checks. The mechanism names who "
            "is on the losing side. Instruments: ESH2024, timeframe 1m."
        )
        self.assertEqual(draft.find_predictions(clean), [])

    def test_a_generated_draft_passes_its_own_check(self):
        """The one that actually fired in anger.

        The first template named the banned metrics inside its own warning
        sentence, so every draft was refused by the rule's restatement of
        itself — and the second version reintroduced it in the sentence
        explaining why abstracts are not embedded. This test is what makes that
        class of mistake fail here instead of at the next harvest.
        """
        text = draft.draft_markdown(_paper(), topic="t", family_hint="fam")
        self.assertEqual(
            draft.find_predictions(text),
            [],
            "the drafter's own boilerplate must not trip its own check",
        )


class DraftsCarryNoThirdPartyProse(unittest.TestCase):
    """Ruling 5: a draft references the corpus record, never embeds it.

    Two rules meet here. A source paper's own figures belong in the Honesty
    note and nowhere else (`research/backlog/README.md` §1), and an abstract
    routinely leads with one — so an embedded abstract puts them in the
    Citation section by construction. And the corpus is gitignored precisely so
    this repository carries no third-party prose; a draft that quoted it would
    commit the text the corpus rule exists to keep out.

    Watched failing against the pre-fix drafter, which embedded the abstract
    under a `<details>` block.
    """

    def test_the_abstract_is_not_reproduced(self):
        marker = "UNIQUE-ABSTRACT-SENTINEL-9137"
        text = draft.draft_markdown(
            _paper(abstract=f"{marker} and a Sharpe of 3.0."),
            topic="t",
            family_hint="fam",
        )
        self.assertNotIn(marker, text)
        self.assertNotIn("<details>", text)
        self.assertIn("deliberately not reproduced", text)

    def test_a_reproduced_clause_is_found(self):
        """The gap the embedding test cannot see.

        Caught, by mutation: a `find_reproduced_prose` returning `[]`, and a
        `size` raised to 20 — both of which pass the embedding test above while
        letting a hand-written draft carry the abstract clause by clause.
        """
        abstract = (
            "We document that the first half hour return predicts the last "
            "half hour return in index futures markets."
        )
        lifted = (
            "## Mechanism\n\nWe document that the first half hour return "
            "predicts the last half hour return in index futures markets."
        )
        self.assertTrue(draft.find_reproduced_prose(lifted, abstract))

    def test_original_prose_on_the_same_subject_is_not_flagged(self):
        """The converse, written first.

        Without it a checker that flagged everything would pass the test above
        and refuse every draft, which is the failure mode this file already
        has a scar from (`test_a_generated_draft_passes_its_own_check`).
        """
        abstract = (
            "We document that the first half hour return predicts the last "
            "half hour return in index futures markets."
        )
        original = (
            "## Mechanism\n\nThe claim is that early-session order flow is "
            "unfinished business, so a position taken near the open is closed "
            "into the settlement window by participants who cannot carry it "
            "overnight. Who is on the losing side: the constrained holder."
        )
        self.assertEqual(draft.find_reproduced_prose(original, abstract), [])

    def test_a_record_with_no_abstract_reproduces_nothing(self):
        self.assertEqual(draft.find_reproduced_prose("any text at all", None), [])

    def test_the_committed_drafts_carry_no_abstract_block(self):
        # Drafts were promoted into research/backlog/ on 2026-08-07 and keep
        # their DRAFT- prefix there, so this glob follows them rather than
        # scanning an empty directory. The assertion below is what caught the
        # move: without it this test would have passed over an empty list and
        # gone on claiming to check something.
        repo = Path(__file__).resolve().parents[3]
        drafts = sorted((repo / "research" / "backlog").glob("DRAFT-*.md"))
        self.assertTrue(drafts, "there should be at least one committed draft")
        for path in drafts:
            text = path.read_text(encoding="utf-8")
            with self.subTest(draft=path.name):
                self.assertNotIn("<details>", text)
                self.assertNotIn("Harvested abstract", text)
                self.assertEqual(
                    draft.find_predictions(text),
                    [],
                    f"{path.name} carries source-paper performance figures",
                )


#: Empty-but-well-formed payloads, one per source. Enough for the harvester to
#: parse and yield nothing, which is all a URL-shape assertion needs.
_EMPTY_BODY = {
    "semanticscholar": b'{"data": []}',
    "openalex": b'{"results": []}',
    "crossref": b'{"message": {"items": []}}',
    "arxiv": b'<feed xmlns="http://www.w3.org/2005/Atom"></feed>',
}

#: Each API spells paging differently, so each expectation is written out
#: rather than derived. `page` is zero-based on the way in; OpenAlex's is
#: 1-based on the way out, which is the whole reason this table is explicit.
_PAGING = {
    # source: (param, value at page 0, value at page 2 with limit 10)
    "semanticscholar": ("offset", "0", "20"),
    "crossref": ("offset", "0", "20"),
    "arxiv": ("start", "0", "20"),
    "openalex": ("page", "1", "3"),
}


def _captured_url(source_name: str, *, page: int, limit: int = 10) -> str:
    """Run one harvester against a stubbed `_get` and return the URL it built."""
    seen: list[str] = []

    def fake_get(url, **_kw):
        seen.append(url)
        return _EMPTY_BODY[source_name]

    with mock.patch.object(sources, "_get", fake_get):
        list(sources.SOURCES[source_name]("q", limit, page))
    return seen[0]


class Pagination(unittest.TestCase):
    """One page per source was the whole harvest until this landed.

    Caught, by deliberate mutation: `openalex` passing `page` straight through
    instead of `page + 1`, which silently re-fetches OpenAlex page 1 as page 0
    and skips the real second page — a corpus that looks twice as broad as it
    is. Also caught a `page * limit` written `page` in `semantic_scholar`,
    which returns records 2..11 for page 2.
    """

    def test_page_zero_asks_for_the_first_page_of_every_source(self):
        for name, (param, at_zero, _) in _PAGING.items():
            with self.subTest(source=name):
                query = urllib.parse.parse_qs(
                    urllib.parse.urlsplit(_captured_url(name, page=0)).query
                )
                self.assertEqual(query[param], [at_zero])

    def test_each_source_pages_with_its_own_idiom(self):
        for name, (param, _, at_two) in _PAGING.items():
            with self.subTest(source=name):
                query = urllib.parse.parse_qs(
                    urllib.parse.urlsplit(
                        _captured_url(name, page=2, limit=10)
                    ).query
                )
                self.assertEqual(
                    query[param],
                    [at_two],
                    f"{name} must advance its own paging parameter",
                )

    def test_paging_still_goes_through_the_one_allowlisted_call_site(self):
        """Paging must not have grown a second way out to the network."""
        for name in sources.SOURCES:
            with self.subTest(source=name):
                sources.check_host(_captured_url(name, page=3))


class TopicSelection(unittest.TestCase):
    """`--topic` labelled the front matter and selected nothing.

    Caught: the pre-fix `cmd_draft`, whose selection was
    `[p for p in papers if p.abstract][:count]` over the whole corpus — so
    twelve topic runs emitted twelve stamps of the same head-of-list papers,
    and the slug-based filename made them overwrite one another.
    """

    def _corpus_with_two_topics(self, directory: Path) -> Path:
        path = directory / "papers.jsonl"
        corpus.append(
            path,
            [
                _paper(
                    title="Alpha In Topic One",
                    doi="10.1/one",
                    topic="trend-horizon",
                    abstract="a",
                ),
                _paper(
                    title="Beta In Topic Two",
                    doi="10.1/two",
                    topic="calendar-effects",
                    abstract="b",
                ),
            ],
        )
        return path

    def test_a_topic_selects_only_its_own_records(self):
        topic = {"queries": []}
        one = _paper(doi="10.1/one", topic="trend-horizon")
        two = _paper(doi="10.1/two", topic="calendar-effects")
        self.assertTrue(cli.in_topic(one, "trend-horizon", topic))
        self.assertFalse(cli.in_topic(two, "trend-horizon", topic))

    def test_a_record_seen_under_two_topics_belongs_to_both(self):
        merged = corpus.dedupe(
            [
                _paper(doi="10.1/x", topic="trend-horizon"),
                _paper(doi="10.1/x", topic="cross-asset-lead-lag"),
            ]
        )
        self.assertEqual(len(merged), 1)
        self.assertEqual(
            merged[0].extra["topics"], ["cross-asset-lead-lag", "trend-horizon"]
        )
        for name in ("trend-horizon", "cross-asset-lead-lag"):
            with self.subTest(topic=name):
                self.assertTrue(cli.in_topic(merged[0], name, {"queries": []}))

    def test_a_record_written_before_the_topic_field_still_parses(self):
        """Reader-first: the field defaults, so an older corpus line loads."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "old.jsonl"
            row = json.loads(_paper().to_json())
            row.pop("topic")
            path.write_text(
                json.dumps(row) + "\n", encoding="utf-8", newline="\n"
            )
            loaded = list(corpus.read(path))
        self.assertEqual(len(loaded), 1)
        self.assertEqual(loaded[0].topic, "")

    def test_two_topic_runs_write_different_drafts(self):
        """The end-to-end shape of the bug, not just its unit."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            corpus_path = self._corpus_with_two_topics(root)
            out = root / "drafts"
            written: dict[str, set[str]] = {}
            for name in ("trend-horizon", "calendar-effects"):
                with mock.patch.object(
                    cli, "load_topic", return_value={
                        "queries": [],
                        "hypothesis_family_hint": "fam",
                    }
                ):
                    code = cli.main(
                        [
                            "draft",
                            "--topic",
                            name,
                            "--corpus",
                            str(corpus_path),
                            "--out",
                            str(out / name),
                            "--count",
                            "5",
                        ]
                    )
                self.assertEqual(code, cli.EXIT_OK)
                written[name] = {p.name for p in (out / name).glob("*.md")}
        self.assertEqual(len(written["trend-horizon"]), 1)
        self.assertEqual(len(written["calendar-effects"]), 1)
        self.assertEqual(
            written["trend-horizon"] & written["calendar-effects"],
            set(),
            "two topics must not draft the same paper",
        )


if __name__ == "__main__":
    unittest.main()
