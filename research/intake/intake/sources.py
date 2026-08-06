"""The only four hosts this tool is permitted to contact.

Compliance is enforced structurally rather than by convention. Every request
goes through :func:`_get`, which refuses any URL whose host is not in
:data:`ALLOWED_HOSTS`. There is no second HTTP call site in this package, and
the dependency list is empty — the standard library only — so verifying "what
does this tool talk to?" is reading one constant and one function.

**Google Scholar and SSRN are not scraped, and must never be added here.**
Both prohibit automated access in their terms; SSRN metadata reaches us
legitimately because Crossref and OpenAlex index SSRN DOIs, and an SSRN paper
is therefore harvested *through* those APIs rather than off the site. PDFs are
fetched by a human, by hand, when a human decides a paper is worth reading.
This is the same class of rule as the project's "never scrape CME" — the point
is not that scraping is hard, it is that the terms say no.

Every record carries its source API, its DOI where one exists, and the UTC date
it was retrieved, because a citation without an access date is the house
discipline's missing half.

**Paging.** Each harvester takes a zero-based ``page`` and each of the four
APIs spells paging differently — ``offset`` (Semantic Scholar, Crossref), a
1-based ``page`` number (OpenAlex), ``start`` (arXiv). The translation happens
in each harvester rather than behind a shared wrapper: a wrapper would have to
pretend the four idioms are one, and an off-by-one in it would silently
re-harvest page 1 four times while the corpus looked twice as broad as it was.
Paging does not touch the throttle or the single call site — every page is
still one :func:`_get`, still one request per 1.1 s per host.
"""

from __future__ import annotations

import json
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Iterator

#: The complete allowlist. Adding to it is a compliance decision, not a code
#: change: see this module's docstring and the tool README.
ALLOWED_HOSTS = frozenset(
    {
        "api.semanticscholar.org",
        "api.openalex.org",
        "api.crossref.org",
        "export.arxiv.org",
    }
)

#: Seconds between requests to one host. Deliberately generous: these are free,
#: unauthenticated, shared endpoints and a polite client is the price of using
#: them without a key. Semantic Scholar in particular rate-limits hard.
THROTTLE_SECONDS = 1.1

#: Crossref and OpenAlex both ask that unauthenticated clients identify
#: themselves and give a contact address; doing so moves you to a faster pool.
USER_AGENT = (
    "crucible-research-intake/0.1 "
    "(https://github.com/LironKruchinin/backtesting; mailto:lironkruch@gmail.com)"
)


class DisallowedHost(RuntimeError):
    """A request was aimed at a host outside :data:`ALLOWED_HOSTS`."""


@dataclass(frozen=True)
class Paper:
    """One harvested record, with the provenance a citation needs.

    ``doi`` is the dedupe key and may be ``None`` — arXiv preprints often have
    none, and dropping them for that reason would silently bias the corpus
    toward published work.

    ``topic`` is the topic file the harvest ran under. It is stamped at harvest
    time rather than derived at draft time, because ``query`` alone cannot
    answer it: two topic files may legitimately share a query string, and a
    query edited after a harvest would orphan every record it produced. It
    **defaults to empty** so records written before the field existed still
    parse — reader-first (CLAUDE.md §8), applied to a JSONL store that is
    append-only for the same reason the run registry is.
    """

    source: str
    source_id: str
    title: str
    abstract: str | None
    year: int | None
    venue: str | None
    authors: list[str]
    doi: str | None
    url: str | None
    accessed: str
    query: str
    extra: dict[str, Any] = field(default_factory=dict)
    topic: str = ""

    def to_json(self) -> str:
        return json.dumps(
            {
                "source": self.source,
                "source_id": self.source_id,
                "title": self.title,
                "abstract": self.abstract,
                "year": self.year,
                "venue": self.venue,
                "authors": self.authors,
                "doi": self.doi,
                "url": self.url,
                "accessed": self.accessed,
                "query": self.query,
                "extra": self.extra,
                "topic": self.topic,
            },
            ensure_ascii=False,
            sort_keys=True,
        )


def _today() -> str:
    return datetime.now(timezone.utc).date().isoformat()


_last_request_at: dict[str, float] = {}


def check_host(url: str) -> str:
    """Return ``url``'s host, or raise [`DisallowedHost`].

    Public because it is the whole compliance surface and a test asserts on it
    directly. There is nowhere else in this package that decides what may be
    contacted.
    """
    host = urllib.parse.urlsplit(url).hostname or ""
    if host not in ALLOWED_HOSTS:
        raise DisallowedHost(
            f"{host!r} is not an allowed host. This tool contacts official APIs only "
            f"({', '.join(sorted(ALLOWED_HOSTS))}); Google Scholar and SSRN are never "
            "scraped — see research/intake/README.md."
        )
    return host


class _AllowlistRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Re-checks the allowlist on **every** redirect hop.

    Without this the allowlist would guard only the URL we typed. `urllib`
    follows redirects by itself, so a `301` from an allowed host to any other
    host would be followed silently and the tool would contact something it
    promised not to — while the README claimed structural enforcement.

    Today's four endpoints are HTTPS and none of them redirects off-host, so
    this is unexploitable in practice. It is here because "structural" has to
    mean the structure actually holds, not that the happy path happens to.
    """

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: D102
        check_host(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


#: Built once. `urlopen`'s default opener has no allowlist handler, so every
#: request in this package goes through this one instead.
_OPENER = urllib.request.build_opener(_AllowlistRedirectHandler)


def _get(url: str, *, accept: str = "application/json") -> bytes:
    """Fetch ``url``, refusing any host outside the allowlist.

    The single HTTP call site in this package. Throttled per host, and it
    raises rather than returning a sentinel: a harvest that silently skipped a
    source would produce a corpus whose gaps nobody could see.
    """
    host = check_host(url)
    elapsed = time.monotonic() - _last_request_at.get(host, 0.0)
    if elapsed < THROTTLE_SECONDS:
        time.sleep(THROTTLE_SECONDS - elapsed)
    request = urllib.request.Request(
        url, headers={"User-Agent": USER_AGENT, "Accept": accept}
    )
    try:
        with _OPENER.open(request, timeout=30) as response:
            body = response.read()
    finally:
        _last_request_at[host] = time.monotonic()
    return body


def _norm_doi(raw: str | None) -> str | None:
    if not raw:
        return None
    doi = raw.strip().lower()
    for prefix in ("https://doi.org/", "http://doi.org/", "doi:"):
        if doi.startswith(prefix):
            doi = doi[len(prefix) :]
    return doi or None


def semantic_scholar(query: str, limit: int, page: int = 0) -> Iterator[Paper]:
    """Semantic Scholar Graph API — free, no key needed for this endpoint.

    Pages on a record ``offset``, so page *n* starts at ``n * limit``.
    """
    fields = "title,abstract,year,venue,authors,externalIds,url"
    url = (
        "https://api.semanticscholar.org/graph/v1/paper/search?"
        + urllib.parse.urlencode(
            {
                "query": query,
                "limit": limit,
                "offset": page * limit,
                "fields": fields,
            }
        )
    )
    payload = json.loads(_get(url))
    accessed = _today()
    for item in payload.get("data", []):
        ids = item.get("externalIds") or {}
        yield Paper(
            source="semanticscholar",
            source_id=str(item.get("paperId") or ""),
            title=(item.get("title") or "").strip(),
            abstract=(item.get("abstract") or None),
            year=item.get("year"),
            venue=(item.get("venue") or None),
            authors=[a.get("name", "") for a in (item.get("authors") or [])],
            doi=_norm_doi(ids.get("DOI")),
            url=item.get("url"),
            accessed=accessed,
            query=query,
            extra={"arxiv": ids.get("ArXiv"), "corpus_id": ids.get("CorpusId")},
        )


def openalex(query: str, limit: int, page: int = 0) -> Iterator[Paper]:
    """OpenAlex — open catalogue, indexes SSRN DOIs among many others.

    Pages on a **1-based** ``page`` number rather than a record offset, which
    is why the argument is translated here rather than passed through.
    """
    url = "https://api.openalex.org/works?" + urllib.parse.urlencode(
        {
            "search": query,
            "per-page": limit,
            "page": page + 1,
            "mailto": "lironkruch@gmail.com",
        }
    )
    payload = json.loads(_get(url))
    accessed = _today()
    for item in payload.get("results", []):
        host = (item.get("primary_location") or {}).get("source") or {}
        yield Paper(
            source="openalex",
            source_id=str(item.get("id") or ""),
            title=(item.get("title") or "").strip(),
            abstract=_openalex_abstract(item.get("abstract_inverted_index")),
            year=item.get("publication_year"),
            venue=host.get("display_name"),
            authors=[
                (a.get("author") or {}).get("display_name", "")
                for a in (item.get("authorships") or [])
            ],
            doi=_norm_doi(item.get("doi")),
            url=item.get("id"),
            accessed=accessed,
            query=query,
            extra={"type": item.get("type"), "cited_by": item.get("cited_by_count")},
        )


def _openalex_abstract(index: dict[str, list[int]] | None) -> str | None:
    """OpenAlex ships abstracts as an inverted index; rebuild the text."""
    if not index:
        return None
    positions: list[tuple[int, str]] = []
    for word, spots in index.items():
        positions.extend((spot, word) for spot in spots)
    positions.sort()
    return " ".join(word for _, word in positions) or None


def crossref(query: str, limit: int, page: int = 0) -> Iterator[Paper]:
    """Crossref — the DOI registry itself, and where SSRN DOIs resolve.

    Pages on a record ``offset``, like Semantic Scholar but spelled differently.
    """
    url = "https://api.crossref.org/works?" + urllib.parse.urlencode(
        {
            "query": query,
            "rows": limit,
            "offset": page * limit,
            "mailto": "lironkruch@gmail.com",
        }
    )
    payload = json.loads(_get(url))
    accessed = _today()
    for item in payload.get("message", {}).get("items", []):
        titles = item.get("title") or []
        issued = (item.get("issued") or {}).get("date-parts") or [[None]]
        containers = item.get("container-title") or []
        yield Paper(
            source="crossref",
            source_id=str(item.get("DOI") or ""),
            title=(titles[0] if titles else "").strip(),
            abstract=item.get("abstract"),
            year=issued[0][0] if issued and issued[0] else None,
            venue=containers[0] if containers else None,
            authors=[
                " ".join(filter(None, [a.get("given"), a.get("family")]))
                for a in (item.get("author") or [])
            ],
            doi=_norm_doi(item.get("DOI")),
            url=item.get("URL"),
            accessed=accessed,
            query=query,
            extra={"type": item.get("type"), "publisher": item.get("publisher")},
        )


def arxiv(query: str, limit: int, page: int = 0) -> Iterator[Paper]:
    """arXiv q-fin. Atom XML rather than JSON, parsed with the stdlib.

    Pages on ``start``, a record offset spelled a fourth way. All four idioms
    are translated at their own call site rather than abstracted: the shapes
    genuinely differ, and one wrapper pretending otherwise would be the place a
    silently-wrong page number would live.
    """
    import xml.etree.ElementTree as ET

    url = "https://export.arxiv.org/api/query?" + urllib.parse.urlencode(
        {
            "search_query": f"cat:q-fin.* AND all:{query}",
            "max_results": limit,
            "start": page * limit,
            "sortBy": "relevance",
        }
    )
    body = _get(url, accept="application/atom+xml")
    ns = {"a": "http://www.w3.org/2005/Atom", "arxiv": "http://arxiv.org/schemas/atom"}
    root = ET.fromstring(body)
    accessed = _today()
    for entry in root.findall("a:entry", ns):
        published = (entry.findtext("a:published", default="", namespaces=ns) or "")[:4]
        doi_node = entry.findtext("arxiv:doi", default=None, namespaces=ns)
        yield Paper(
            source="arxiv",
            source_id=(entry.findtext("a:id", default="", namespaces=ns) or "").strip(),
            title=" ".join(
                (entry.findtext("a:title", default="", namespaces=ns) or "").split()
            ),
            abstract=" ".join(
                (entry.findtext("a:summary", default="", namespaces=ns) or "").split()
            )
            or None,
            year=int(published) if published.isdigit() else None,
            venue="arXiv q-fin",
            authors=[
                (a.findtext("a:name", default="", namespaces=ns) or "")
                for a in entry.findall("a:author", ns)
            ],
            doi=_norm_doi(doi_node),
            url=(entry.findtext("a:id", default=None, namespaces=ns) or None),
            accessed=accessed,
            query=query,
            extra={},
        )


#: Name → harvester. The CLI iterates this and nothing else.
SOURCES = {
    "semanticscholar": semantic_scholar,
    "openalex": openalex,
    "crossref": crossref,
    "arxiv": arxiv,
}
