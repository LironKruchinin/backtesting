# research/intake — the paper factory

Turns a topic query into **registration drafts** for `research/backlog/`.
Standard library Python, outside the Cargo workspace, so CLAUDE.md §6's
dependency policy is untouched.

```bash
cd research/intake
python -m intake harvest --topic momentum-horizon --limit 20 --pages 2  # talks to the APIs
python -m intake draft   --topic momentum-horizon --count 3             # talks to nobody
```

`--limit` is **per (query × source × page)**, so a topic's total is
`limit × queries × sources × pages`. Volume is not the constraint; relevance
is. **Do not exceed `--limit 100`** — Semantic Scholar caps there and OpenAlex
at 200, nothing clamps, and an over-limit request is reported as a failed
source rather than silently truncated.

## The hard constraint: official APIs only

This tool contacts **four hosts and no others**:

| API | why it is here |
|---|---|
| `api.semanticscholar.org` | Graph API; best abstract coverage |
| `api.openalex.org` | open catalogue; indexes SSRN DOIs |
| `api.crossref.org` | the DOI registry itself |
| `export.arxiv.org` | q-fin preprints |

**Google Scholar is never scraped. SSRN is never scraped.** Both prohibit
automated access in their terms. This is the same class of rule as the
project's "never scrape CME": the point is not that it would be difficult, it
is that the terms say no and a research process built on a terms violation is
not a research process anyone can publish from.

SSRN papers still arrive — Crossref and OpenAlex index SSRN DOIs, so an SSRN
paper is harvested *through* those APIs. The first draft this tool ever
produced was `10.2139/ssrn.7008318`, reached that way. **PDFs are fetched by a
human, by hand**, when a human decides a paper is worth reading.

Enforcement is structural, not conventional. Every request goes through one
function, `sources._get`, which refuses any host outside `ALLOWED_HOSTS` — and
through an opener that re-checks the allowlist on **every redirect hop**. There
is no second HTTP call site and there are no dependencies, so auditing "what
does this talk to?" is reading one constant and one function. Requests are
throttled to one per 1.1 s per host and the client identifies itself, because
these are free shared endpoints without a key.

## What lands where, and what is committed

```
intake/     the tool                        committed
topics/     query configs, one per theme    committed
drafts/     generated registration drafts   committed
corpus/     harvested metadata + abstracts  GITIGNORED
```

The corpus is gitignored because it is third-party text — other people's
abstracts — and because it is rebuildable from the APIs at any time. It is
append-only JSONL for the same reason the run registry is (D-0074): append is
the only write, an interrupted harvest cannot corrupt what came before, and a
reader folds the file rather than trusting an index. Records carry their source
API, DOI and **UTC access date**, because a citation without an access date is
the house discipline's missing half.

Dedupe is by DOI, falling back to normalized title + year for the preprints
that have none — dropping DOI-less records would quietly bias the corpus toward
published work. Duplicates are **merged**, not first-wins: Crossref usually has
the DOI and venue while Semantic Scholar usually has the abstract, and the
survivor records every source it was seen in — and every **topic** it was seen
under, because a cross-asset paper harvested under two themes belongs to both
and collapsing it to one would delete it from the other.

### Paging, and why each source translates its own

Each harvester takes a zero-based `page`, and the four APIs spell paging four
ways: `offset` (Semantic Scholar, Crossref), a **1-based** `page` number
(OpenAlex), `start` (arXiv). The translation lives in each harvester rather
than behind a shared wrapper — a wrapper would have to pretend the four idioms
are one, and an off-by-one inside it would silently re-fetch page 1 four times
while the corpus looked twice as broad as it was. That is the exact defect
`test_page_zero_asks_for_the_first_page_of_every_source` was watched failing
against.

Paging changes nothing about compliance: every page is still one `_get`, still
one request per 1.1 s per host, still one call site. An empty page ends the
pair (all four APIs answer a past-the-end offset with an empty list, not a
404); a *failed* page ends it too, because the next page of a source that just
returned `429` will almost certainly return `429` as well.

### `--topic` selects, and used not to

`draft --topic` once supplied only the front-matter label and the family hint:
selection was `[p for p in papers if p.abstract][:count]` over the **whole**
corpus. Twelve topic runs therefore emitted twelve stamps of the same
head-of-list papers with different `topic:` fields, and the slug-based filename
made them overwrite one another — so a twelve-topic sweep produced one topic's
worth of drafts and no error.

The fix is a `topic` field stamped onto each record **at harvest time**, not
derived from `query` at draft time: two topic files may legitimately share a
query string, and a query edited after a harvest would orphan every record it
produced. The field defaults to empty so corpus lines written before it existed
still parse (reader-first, CLAUDE.md §8), and `cli.in_topic` falls back to
matching `query` for those.

`--offset` and `--min-year` exist for the same reason: selection is
head-of-list off the dedupe's `(-year, title)` sort, so without an offset a
second run over one topic re-drafts exactly what the first one did.

## A draft is not a registration

Drafts land in `drafts/`, **never** in `research/backlog/`. A human reads the
paper and promotes it by hand, or deletes it. Nothing reaches the funnel
without Liron's "approved", per hypothesis, by name.

The tool fills the mechanical half — citation, provenance, the section skeleton
in the order `research/backlog/README.md` §4 fixes — and marks every section it
cannot honestly do with `TODO(human)` saying what is owed. A blank mechanism
paragraph would look like a *considered* mechanism; the marker is the
difference. It allocates no `id` and no grade: the id is claimed against the
backlog at promotion time, for the same reason CLAUDE.md §8.2 allocates
decision numbers at merge, and the grade is a cost judgement nobody who has not
read the paper can make.

**The backlog's binding rule — no predicted performance figure in any file —
is enforced here, not left to review.** `draft.find_predictions` scans the
whole draft and the tool *refuses* to write one that breaks the rule. There is
no exemption any more: the abstract used to be embedded and excluded from the
scan, which meant the checker could not see the very figures most likely to
break §1. Drafts now carry no abstract at all, so the scan covers everything in
the file and the exemption is gone with the thing it existed for.

> The first version of the template listed the banned metrics by name in its
> own warning sentence, so **every draft was refused by the rule's restatement
> of itself**. Same failure as a grep that matches its own documentation
> (CLAUDE.md §8.2), and worth recording rather than quietly fixing: a check
> that fires on its own boilerplate gets disabled within a week. It then
> happened a *second* time, in the sentence explaining why abstracts are not
> embedded — which is why `test_a_generated_draft_passes_its_own_check` exists.

## Drafts carry no third-party prose

A draft **references** its corpus record and never embeds the abstract. Two
rules meet here and both are already this repository's. A source paper's own
performance figures belong in the **Honesty note** and nowhere else
(`research/backlog/README.md` §1), and an abstract routinely leads with one —
so an embedded abstract puts those figures in the Citation section by
construction, and promoting the draft unedited would breach §1 as written. And
the corpus is gitignored precisely so this repository carries no third-party
text; a draft that quoted it would commit exactly what that rule keeps out.

The front matter already carries the DOI and access date, and the draft prints
the one-liner that reads the abstract back out of the corpus. The human reads
it there, or in the paper.

**Not embedding it is not the same as not reproducing it.**
`test_the_abstract_is_not_reproduced` proves the *drafter* pastes nothing in,
and can prove nothing at all about a draft a human wrote while reading one — a
hand-written draft that follows the abstract closely carries the same
third-party prose the rule exists to keep out, one clause at a time, while
looking like original writing. `draft.find_reproduced_prose` closes that: it
reports any run of **eight** words shared between a draft and its abstract.
Eight is long enough that ordinary technical phrasing does not reach it and
short enough to catch a reproduced clause before it becomes a sentence; a run
made only of stopwords is ignored. Watched failing against both a stubbed
`[]` return and a threshold raised to twenty.

### Re-checking the drafts a human edited

`draft` runs `find_predictions` on **its own output**, and every draft in
`drafts/` is then hand-written over that skeleton — so the check the tool
performs is not the check the committed file needs. `check_drafts.py` re-runs
both rules over the directory:

```bash
cd research/intake && python check_drafts.py                    # everything
cd research/intake && python check_drafts.py --only A.md B.md   # this wave's files
```

Exit 0 clean, 4 if anything was found **or could not be checked**. The second
half matters: `find_reproduced_prose` needs the abstract, the abstract lives only
in the gitignored corpus, and a corpus is not rebuilt identically by a later
harvest — so a wave can only answer for its own drafts, and everything else is
reported `UNCHECKED` rather than silently passed. An unchecked draft that prints
nothing looks exactly like a clean one, which is the unfired-detector failure
CLAUDE.md §7 names.

**One narrow exemption, and it was found by the check firing.** A citation must
state the paper's title, so the title is the one span of third-party text a draft
is *required* to reproduce. Two wave-1 drafts
(`index-futures-return-dependence`, `inventories-and-oil-basis`) were flagged on
runs that turned out to be their own citation lines, because both papers repeat
their title inside their abstract. `without_cited_title` drops exactly the
italicised `*<title>*` span the citation block writes, for exactly the record the
draft was matched to, and nothing else — the alternative was mangling two
citations to satisfy a checker, which is the direction §7 refuses. The exemption
was verified not to disable the check: with it in place, a clause lifted verbatim
from an abstract and planted in a draft's Mechanism section was still reported
(seven overlapping runs), and the file restored to its pre-mutation blob
`6438705d`, printed before and after.

## Wave 2 (2026-08-07): twenty more topics, and what the throttle did

The second sweep added twenty topic files aimed at seams the first did not
touch — storage, curve shape, carry, auctions, positioning, order flow,
execution, jumps — deliberately weighted away from equity index, because the
archive's CL, GC, 6E and ZN holdings were almost untouched by the backlog. Five
queries each, `--limit 30 --pages 1`, run back to back.

**Running twenty topics in one pass is enough to get refused by three of the four
sources.** Measured over the 100 (topic × query) pairs:

| source | failed pairs |
|---|---|
| `crossref` | **0 / 100** |
| `arxiv` | 51 / 100 |
| `openalex` | 72 / 100 |
| `semanticscholar` | 88 / 100 |

Semantic Scholar's rate is the expected one and matches wave 1's experience.
OpenAlex and arXiv failing at all is new, and it is a volume effect rather than a
policy change: `THROTTLE_SECONDS = 1.1` is per host, and four sources rotating
means each host is asked roughly every fourth request, which is fine for a
handful of topics and not for a hundred consecutive pairs. A second pass over the
thin topics with a 45-second gap between them recovered most of the loss.
**Budget a gap between topics, not only between requests** — the throttle is
sized for one topic at a time.

Crossref answering 100/100 is worth recording too: it is the most reliable of the
four by a wide margin and the only one that never refused, which is why the
top-up pass excluded it.

## Tests

```bash
cd research/intake && python -m unittest discover -s tests -v
```

**Run them by hand: the project's CI is cargo-only and does not know this
directory exists.** Controls over what the tool promises — the host allowlist,
the no-predicted-performance rule, the no-third-party-prose rule, and that a
topic selects its own papers — and each was watched failing against a planted
defect before it was committed:

| planted defect | caught by |
|---|---|
| `find_predictions` returns `[]` | `test_a_planted_marker_is_found` (6 subtests) |
| drafter re-embeds the abstract | `test_the_abstract_is_not_reproduced` |
| `find_reproduced_prose` returns `[]`, or its threshold raised to 20 | `test_a_reproduced_clause_is_found` |
| redirect handler stops re-checking the host | `test_a_redirect_off_an_allowed_host_is_refused` |
| OpenAlex passes `page` through instead of `page + 1` | `test_page_zero_asks_for_the_first_page_of_every_source` and `test_each_source_pages_with_its_own_idiom` |
| Semantic Scholar's offset written `page` instead of `page * limit` | `test_each_source_pages_with_its_own_idiom` |
| the topic filter deleted from `cmd_draft` | `test_two_topic_runs_write_different_drafts` |

Every one of those restores was verified by printing `git hash-object` before
and after, per CLAUDE.md §7 — one mutation at a time, never stacked.

Two converse controls exist beside the positive ones and were written first:
`test_ordinary_registration_prose_is_not_flagged` and
`test_original_prose_on_the_same_subject_is_not_flagged`. Without them a
checker that flagged everything would pass its positive test and refuse every
draft ever written, which is a failure this file already has a scar from
(`test_a_generated_draft_passes_its_own_check`).

The allowlist is re-checked on **every redirect hop**, not only on the URL the
tool typed: `urllib` follows redirects itself, so a `301` off an allowed host
would otherwise be followed silently while this README claimed structural
enforcement. None of today's four endpoints redirects off-host, so it is
unexploitable in practice — the point is that "structural" has to mean the
structure holds.

## Exit codes

Follows the project contract: `0` did the work, `2` usage error, `4` ran but a
source or query failed and the corpus is narrower than requested. A harvest
that lost a source and reported success would silently shrink the sweep —
Semantic Scholar returns `429` without a key often enough that this matters.

**It matters in practice, not in principle.** The 2026-08-06 twelve-topic
sweep lost **34 of 36** Semantic Scholar (topic × query) pairs to `429`; the
other three sources delivered in full. Semantic Scholar is the best abstract
source of the four, so exit 4 there is a real narrowing of the corpus and not
a formality — and there is no API-key handling in `sources.py` to fix it with.
Budget for the loss; do not read exit 4 as a broken run.
