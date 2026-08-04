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

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from intake import draft, sources  # noqa: E402


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

    def test_the_committed_drafts_carry_no_abstract_block(self):
        drafts = sorted((Path(__file__).resolve().parent.parent / "drafts").glob("*.md"))
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


if __name__ == "__main__":
    unittest.main()
