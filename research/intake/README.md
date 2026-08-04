# research/intake — the paper factory

Turns a topic query into **registration drafts** for `research/backlog/`.
Standard library Python, outside the Cargo workspace, so CLAUDE.md §6's
dependency policy is untouched.

```bash
cd research/intake
python -m intake harvest --topic momentum-horizon --limit 20   # talks to the APIs
python -m intake draft   --topic momentum-horizon --count 3    # talks to nobody
```

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
survivor records every source it was seen in.

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

## Tests

```bash
cd research/intake && python -m unittest discover -s tests -v
```

**Run them by hand: the project's CI is cargo-only and does not know this
directory exists.** Nine controls over the two things the tool promises — the
host allowlist and the no-predicted-performance rule — and each was watched
failing against a planted defect before it was committed:

| planted defect | caught by |
|---|---|
| `find_predictions` returns `[]` | `test_a_planted_marker_is_found` (6 subtests) |
| drafter re-embeds the abstract | `test_the_abstract_is_not_reproduced` |
| redirect handler stops re-checking the host | `test_a_redirect_off_an_allowed_host_is_refused` |

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
