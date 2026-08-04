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
function, `sources._get`, which refuses any host outside `ALLOWED_HOSTS`. There
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
drafter's own output and the tool *refuses* to write a draft that breaks it.
The paper's own abstract is excluded from that scan: it is quoted third-party
text under a `<details>`, it routinely contains performance figures, and
checking it would make the rule unsatisfiable for exactly the papers worth
reading.

> The first version of the template listed the banned metrics by name in its
> own warning sentence, so **every draft was refused by the rule's restatement
> of itself**. Same failure as a grep that matches its own documentation
> (CLAUDE.md §8.2), and worth recording rather than quietly fixing: a check
> that fires on its own boilerplate gets disabled within a week.

## Exit codes

Follows the project contract: `0` did the work, `2` usage error, `4` ran but a
source or query failed and the corpus is narrower than requested. A harvest
that lost a source and reported success would silently shrink the sweep —
Semantic Scholar returns `429` without a key often enough that this matters.
