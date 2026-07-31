# CLAUDE.md — Crucible

This file is the contract between sessions. Read it fully before writing
code. When this file conflicts with your instincts, defaults, or training
priors, **this file wins**. When a task requires deviating from it, say so
explicitly, get agreement, and record the change in `docs/DECISIONS.md` in
the same commit — never diverge silently.

Companion documents, in reading order for a new session:
1. this file
2. `docs/DECISIONS.md` — append-only decision log (why things are the way they are)
3. `docs/MILESTONES.md` — current scope; work only within the active milestone unless told otherwise
4. the module docs of whatever you're touching — unimplemented modules carry
   their full spec in `//!` docs (e.g. `crucible-data::ingest`,
   `crucible-funnel::stats`). Those specs are the source of truth for M1+
   scope; keep them in sync when you implement them.

---

## 1. What Crucible is

Crucible is an event-driven backtesting engine for CME futures (Databento
data) whose product is **verdicts, not equity curves**: a strategy idea goes
in, and a kill/iterate/graduate scorecard comes out, produced under honest
execution assumptions with overfitting statistics attached. It is built by
one person as a quant-research portfolio project; correctness and statistical
honesty outrank features and performance, and performance outranks
convenience.

The research funnel (S0 signal triage → S1 free-fill coarse grid → S2
walk-forward with costs → S3 statistical battery) is described in
`crucible-funnel`'s crate docs. Most ideas must die cheaply at S0–S1.

**Current milestone:** M3 (funnel + statistics) — see `docs/MILESTONES.md`.
`crucible funnel <config>` runs S0, S1 and S2 end to end today; what is still
owed is the statistics battery (`crucible-funnel::stats`), which is why the
build **cannot award `Graduate`** (D-0075).

---

## 2. Prime invariants — NEVER violate these

Every rule here exists because its violation produces *silently wrong
research results* — the failure mode this project exists to prevent. Tests
enforcing them are merge-blocking. Do not weaken, feature-flag, or "TODO"
them. If a task seems to require breaking one, stop and surface the conflict
instead of proceeding.

### 2.1 No lookahead
- Every market event has an **availability time** (`avail_ts`): the earliest
  instant its information could be known. Bars are stamped with interval
  OPEN (`ts_open`, Databento convention); their `avail_ts` is
  `ts_open + timeframe` — computed by `Bar::avail_ts()`, nowhere else.
- All replay ordering, strategy visibility, and fill sequencing key on
  `avail_ts`. Nothing may ever be ordered, joined, or triggered on `ts_open`
  / `ts_event` except inside display/debug code.
- Orders fill only against events **strictly after** the event that
  triggered them (enforced in `replay.rs` step 1 — never reorder the
  fills→mark→decide sequence).
- Any new data source (macro calendar, definitions, news) must define its
  availability rule explicitly before integration: "as known when?" is the
  first design question, always.
- Normalization/statistics used *inside* a strategy or feature must be
  computed only from data available at decision time. Full-sample z-scores,
  full-sample quantiles, or "fit on everything then backtest" are lookahead.

### 2.2 Determinism
Same config + same data ⇒ **bit-identical** results, on any machine, in any
thread interleaving. Enforced by `crucible-engine/tests/determinism.rs` and
the CI double-run hash gate.
- Banned in result-affecting code: `SystemTime::now`, `Instant` (except
  benchmarks/logging), unseeded randomness, iteration over `HashMap`/
  `HashSet` where order can reach results, floating-point reductions whose
  order varies with thread count, pointer/address-based ordering.
- All randomness flows from explicit seeds carried in configs; derived seeds
  come from (config_hash, combo_index, fold) — never from time or thread id.
- Parallel results merge by sorting on run identity, never by completion
  order.

### 2.3 Integer accounting
- Prices: `Price` (i64 nanopoints, 1e-9 — Databento-native). Money:
  `NanoUsd` (i64 nanodollars). Positions: whole contracts.
- `f64` is legal ONLY in indicator space and statistics space (both consume
  converted copies: `Price::as_points_f64`, `nano_usd_to_f64`). Statistics
  never flow back into accounting. The sole price→money conversion point is
  `ContractSpec::pnl_nano_usd`.
- Indicators must consume **points** (`as_points_f64`), never raw nanos —
  squaring nano-scale i64-as-f64 destroys precision.
- Overflow checks stay on in all profiles (D-0004). Do not remove them to
  win a benchmark.

### 2.4 Costs are visible
- Execution assumptions are named `FillModel`s. There is no anonymous
  default execution anywhere.
- `FreeFills` is sanctioned only for funnel stages S0–S1. Any number
  reported outside a screening context states its fill model.
- Every scorecard includes the cost-sensitivity sweep (0/0.5/1/2 ticks).
- The **intrabar ordering convention** is a second named assumption, reported
  beside the fill model: an OHLC bar does not say whether its high or its low
  printed first, so a stop and a target inside one bar are path-ambiguous.
  `stop_first_intrabar` (`crucible-engine::bracket`, D-0069) resolves it, and
  the bars where it decided the outcome are counted and printed. A run whose
  PnL turns on many of them is a run to distrust.

### 2.5 Reproducibility of results
Every persisted result carries: config hash (blake3 over canonicalized
config, D-0012), git sha, data manifest ids, seed. A number that can't be
reproduced is a rumor and doesn't get stored.

### 2.6 Fair comparison
Grid combos are scored on **identical evaluation windows**: warmup is the
max across the grid, folds are identical across combos. A combo must never
gain an edge from consuming fewer warmup bars or different dates.

---

## 3. Architecture — crates and dependency rules

```
crucible-core        types/events/traits. NO dependencies, NO I/O, NO threads.
crucible-data        network+filesystem for market data. sync public API; async confined to ingest::databento (feature-gated).
crucible-engine      deterministic replay. depends on core ONLY. sync, single-threaded, clock-free.
crucible-strategies  indicators + strategies. depends on core ONLY.
crucible-funnel      orchestration/stats/registry/scorecards. the ONLY crate that spawns threads.
crucible-cli         thin binary wiring everything together.
```

Dependency edges (enforce in review; Cargo.tomls encode them):
- `core` ← everyone. `core` depends on nothing.
- `engine`, `strategies` → `core` only. They must compile with no I/O, no
  async, no threads, forever. Dev-dependencies on `data`/`strategies` for
  tests are fine.
- `data` → `core`. Only crate touching network/filesystem for market data.
  Its **public API is sync, always**. The Databento client is async, so the
  one module that calls it (`ingest::databento`) owns a private
  *current-thread* tokio runtime and `block_on`s behind the sync
  `ingest::BatchProvider` trait, gated on the non-default `databento`
  feature. `async fn`, `.await`, and `tokio` appear nowhere else in the
  workspace (D-0025, superseding D-0005's "bin targets" clause).
- `funnel` → `core` + `engine` + `strategies` (+ rayon/duckdb when M3
  starts). All parallelism lives here.
- `cli` → everything, but stays thin: arg parsing, wiring, printing. Logic
  in the CLI is a smell — move it into the owning crate.

Where does new code go?
- A new indicator/strategy → `crucible-strategies`.
- A new execution assumption → `crucible-engine::execution`.
- Anything reading/writing disk or network for data → `crucible-data`.
- Anything spawning threads, expanding grids, computing cross-run statistics
  → `crucible-funnel`.
- A new shared type/trait → `crucible-core`, only if ≥2 crates need it.

---

## 4. Domain glossary — pinned semantics

Terms mean exactly this everywhere: code, comments, configs, docs. Do not
introduce synonyms; do not repurpose these words.

| Term | Meaning |
|---|---|
| **run** | one backtest execution: (strategy, params, instrument, timeframe, fold, fill model, seed) |
| **combo** | one point in an expanded parameter grid |
| **grid** | cartesian expansion of a config's parameter ranges |
| **trial** | any run counted against a hypothesis family (feeds deflated Sharpe) |
| **hypothesis family** | config-declared key grouping all trials of one idea (`meta.hypothesis_family`) |
| **stage** | funnel gate S0/S1/S2/S3 (see `crucible-funnel`) |
| **verdict** | `Kill` / `Iterate` / `Graduate` — the funnel's output |
| **fold** | one train/test split in walk-forward |
| **IS / OOS** | in-sample (train) / out-of-sample (test) window of a fold |
| **warmup** | bars consumed before the eval window opens; max across a grid (§2.6) |
| **round-trip / episode** | position leaves flat and returns to flat; unit of trade stats |
| **plateau** | contiguous grid region of similar performance (what we trust — vs a spike, which we don't) |
| **avail_ts** | availability time (§2.1). The universal ordering key. |
| **ts_open / event time** | when the thing happened in the market. Display only. |
| **bracket** | protective stop and/or target attached to the order that opens a position, in ticks from *its fill price*; one-cancels-the-other |
| **path-sensitive** | a result that depended on the intrabar ordering convention: some bar touched both a stop and a target, so the bar's own path had to be assumed (§2.4, D-0069) |

Pinned conventions:
- **Units in names.** Money fields end `_nano_usd`; percentages end `_pct`
  (f64, 0–100 scale); tick counts end `_ticks`; timestamps end `_ts`. A bare
  `i64` money/time field fails review.
- **Timeframe strings**: `1s 1m 5m 15m 1h 1d` — the only spellings, parsed
  by `TimeFrame::from_str`. Extending the enum = decision-log entry.
- **Instrument ids**: Databento raw symbols (`ESU6`), continuous aliases
  (`ES.v.0` volume-roll, `ES.c.0` calendar-roll), synthetic `SYN:*`.
- **Fill model names** (config values): `free_fills`, `spread_cross`,
  `queue_sim` (M4). Adding one = decision-log entry.
- **Intrabar ordering convention name**: `stop_first_intrabar` — the only one,
  orthogonal to the fill model and applying inside all of them (D-0069).
  Adding a second = decision-log entry, and it must be reported per result.
- **Orders**: positive `qty` magnitude + `Side`; **positions**: signed
  (long > 0). `Actions::target_position` is the idiomatic way to trade.
- **Timestamps**: UTC nanoseconds (`Ts`) everywhere. Timezone/session logic
  exists only in `crucible-data::calendar`.
- **Config field names**: `snake_case`, units suffixed like code
  (`fee_per_contract_usd`, `train_days`).
- **A number derived by convention rather than sourced carries its basis
  beside it**, in its own field, never folded into the value. `initial_*_usd`
  in `configs/accounts/personal_*.toml` is 1.1 × maintenance — the CME
  speculator convention — and each row says so in `initial_basis`. The field
  exists so the convention cannot hide inside the number: a reader can tell a
  measurement from an assumption without knowing the domain, and a result that
  turns on the assumption can say so. Sourced numbers carry a citation and an
  access date instead; every number carries one or the other.
- **Fold windows are measured in trading days**, never months or bars
  (`train_days`, `test_days`, `step_days` — D-0062). A month holds a number of
  sessions the exchange's holiday schedule picks; a bar count ends
  mid-session.

---

## 5. Code style

1. **Errors.** Libraries: per-crate error enums implementing
   `std::error::Error` (hand-rolled now; `thiserror` when blessed). Binaries
   may use `anyhow` (when blessed). `.unwrap()` is lint-denied;
   `.expect("INVARIANT: …")` only where violation is provably impossible —
   the message must say why. Constructor `assert!`s for config-bug
   validation are correct and documented with `# Panics` (fail loudly at
   build-the-strategy time, never NaN at runtime).
2. **No `unsafe`.** Workspace-forbidden. No exceptions — there is no
   performance problem in this project that unsafe solves.
3. **Documentation.** Every `pub` item gets `///`. Module docs (`//!`)
   explain *why* and carry contracts/specs, not restated signatures.
   Unimplemented modules keep their full spec in module docs until
   implemented (they are the design memory between sessions).
4. **Tests.** Colocated `#[cfg(test)]` for units; `tests/` for integration;
   fixtures per `testdata/README.md`. Numeric tests use hand-derived
   expected values with the derivation in a comment.
5. **Configs.** TOML, `serde` with `deny_unknown_fields` on every config
   struct, `schema_version` field required. A config typo must be a hard
   error at load — silently-ignored fields corrupt research.
6. **Formatting/lints.** rustfmt defaults; CI runs `clippy -D warnings`.
   Suppress with `#[expect(lint, reason = "…")]` (never bare `allow`) — an
   `expect` that stops firing becomes a warning and gets cleaned up.
7. **Commits.** Conventional style (`feat:`, `fix:`, `test:`, `docs:`,
   `refactor:`, `perf:`, `chore:`), imperative subject, milestone tag where
   useful (`feat(data): M1 manifest coverage check`). Golden-value changes
   follow `testdata/README.md` rule 3.
8. **Naming.** Follow §4. Functions that can fail return `Result`; `Option`
   means "legitimately absent", never "error I didn't feel like handling".

---

## 6. Dependency policy

The skeleton is pure-std by design. Dependencies are added **per milestone,
when the code that uses them lands**, never speculatively. Blessed set:

| Crate | Where | Purpose |
|---|---|---|
| `rayon` | funnel only | run-level parallelism. The ONLY crate that spawns threads |
| ~~`duckdb`~~ | — | **not adopted** (D-0074). Its `bundled` build fails on this toolchain — a vendored header is missing and MSVC exits 2 — so the registry is append-only JSONL honouring the identical contract. The five rules are about ordering and identity, not SQL |
| `serde`, `toml`, `serde_json` | funnel, data, cli | configs, manifest, registry records |
| `blake3` | funnel, data, cli | config identity, archive checksums, registry run ids. `cli` because it is where a combo config is loaded today (D-0060); `crucible-strategies` renders the canonical form and never hashes it |
| `dbn` | data only | DBN decoding (sync). Consumed via the `databento::dbn` **re-export**, never pinned separately — a decoder that drifts from the client that wrote the file is a silent bug (D-0031) |
| `databento` | data, `databento` feature, `ingest::databento` only | acquisition; async |
| `tokio` | data, `databento` feature, `ingest::databento` only | current-thread runtime behind the sync `BatchProvider` seam (D-0025) |
| `sha2` | data, `databento` feature | verifying a delivery against the vendor's published digest before it enters the immutable archive (D-0031) |
| `time` | data, `databento` feature, `ingest::databento` only | not a choice — the vendor API spells its timestamps `time::OffsetDateTime` |
| `parquet` | data only | Curated Parquet, low-level typed columns, `default-features = false` + `zstd`. NEVER in engine (D-0037) |
| ~~`polars`~~ | — | **removed** (D-0040). The data-QA report is one ordered pass over a bar series; `polars-core` depends on rayon non-optionally, and only `funnel` spawns threads |
| `chrono` + `chrono-tz` | data::calendar only | the one timezone-aware module. Requested without `clock`, but `parquet` enables it via feature unification — the real clock ban is `clippy.toml` (D-0048) |
| `toml` | funnel, data, cli | configs, and the compiled-in calendar tables (D-0039) |
| `rand_chacha` | where seeded randomness is needed | resampling/permutation |
| `clap` | cli | args (when >1 real command) |
| `criterion` | dev | benchmarks |
| `proptest` | dev | property tests (accounting invariants) |
| `anyhow` | cli/bins | error context |
| `thiserror` | libs | error enums |
| `dotenvy` | **bin targets** only | `.env` → process environment (D-0022) |

Anything else: propose it with a one-line justification and add a
`docs/DECISIONS.md` entry when adopted. Pin minor versions. When a blessed
crate lands, delete its placeholder comment from the relevant `Cargo.toml`.

---

## 7. Testing & verification bars

- **Merge-blocking always:** `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings`, `cargo test --workspace`, CI determinism double-run.
- **Golden tests** (`crucible-engine/tests/golden_smoke.rs` and successors):
  never edit expected values to make a failure pass. Policy in
  `testdata/README.md` — hand arithmetic or it doesn't merge. If a semantics
  change is intended: decision-log entry + rederived values.
- **New indicator:** hand-computed fixture test + warmup-boundary test,
  mandatory.
- **Engine/portfolio/fill changes:** unit tests with hand-derived numbers +
  golden suite green + determinism green.
- **Correctness harnesses (from M3):** permutation and truncation-invariance
  suites become merge-blocking the day they land. A strategy retaining
  performance on shuffled data is treated as an engine-bug alarm first,
  discovery second.
- **Performance claims** require criterion evidence in the PR/commit
  description. No "should be faster".
- **Negative controls:** when building detection machinery (leak detectors,
  permutation harnesses), also build the test that plants a known bug/leak
  and asserts detection fires. A detector nobody has seen fire is decoration.
  This has **no quality exemption**: work that looks right and passes its
  gate is still unverified until each control has been broken deliberately
  and watched firing, and the record says which mutation each one caught.
- **When two things disagree, add the third case that makes them agree** —
  it turns a difference into a diagnosis. A test proving captured and
  reconstructed equity differ says only "something differs"; the companion
  test showing they agree *once the reconstruction adopts the engine's
  intrabar convention* proves the divergence **is** that convention and
  nothing else. Two-sided controls locate a discrepancy; the third side
  names its cause.

---

## 8. Session workflow (Claude Code)

Start of session: read this file, `docs/DECISIONS.md`, the active milestone
in `docs/MILESTONES.md`, and the module docs you'll touch. State which
milestone item you're executing.

While working:
- Stay within the active milestone unless the human redirects scope.
- Semantic/architectural choices → `docs/DECISIONS.md` entry in the same
  commit (template in that file's header).
- **Reader-first.** Any change to a persisted record shape lands its READER
  on `main` before any writer emits the new shape. `deny_unknown_fields`
  turns a premature writer into an archive-wide refusal BY DESIGN — that is
  the strictness succeeding, not failing. Never weaken the parser; order the
  deployment. Worked example in `docs/DATA_PLAN.md`.
- Keep module-doc specs in sync with what you implement; the spec comment is
  done only when the module is.
- Definition of done: fmt + clippy + tests green locally, docs updated,
  decision log updated if applicable, `demo` still behaves (run it).

### 8.1 One merger, and it is the primary session

Concurrent agents are normal here — several worktrees under
`.claude/worktrees/` at once is the usual shape. What is **not** normal is more
than one of them writing to `main`. Three rules, and they are not negotiable:

1. **A subagent delivers a branch. It never merges.** Finish, commit, leave the
   branch name in the report, exit. The branch is the deliverable.
2. **Only the primary session merges into `main`**, one merge at a time, and it
   reads the diff it is merging.
3. **No process ever merges into a working tree another process is using.** A
   merge that stops on a conflict leaves the tree unbuildable and the index
   half-staged; if the merging process then exits, whoever is left inherits a
   repository that does not compile and a conflict they did not create and
   cannot adjudicate.

**The motivating case, 2026-07-31.** The `feat/calendar-eras` subagent finished
and merged itself into the *primary session's* checkout of `main` while that
session was mid-task. It left three unresolved conflicts (`CLAUDE.md`,
`crucible-data/src/calendar/mod.rs`, `docs/DECISIONS.md`), an unclosed delimiter
that broke `cargo build`, a half-staged index — and then its process exited. The
primary session's own uncommitted work was sitting in the same tree. Recovery
was: back up the in-flight files, `git merge --abort`, restore, and re-do the
merge deliberately later. Nothing was lost, but only because the branch itself
was intact; the cost was a full stop in the middle of unrelated work.

Two smaller rules fall out of the same incident:

- **`cd` does not persist across a merge you did not start.** Prefer
  `git -C <path>` over `cd` in every multi-worktree session: a working directory
  that outlives the command that set it will eventually run a checkout in the
  wrong tree. That happened too, the same day.
- **A scripted edit to a contract file (`CLAUDE.md`, `docs/DECISIONS.md`,
  `docs/MILESTONES.md`) must assert its match or be made by hand.** A
  string-replace that silently matches nothing leaves the file saying the
  opposite of what the commit claims, and these three files are the ones a
  future session trusts without re-deriving. Commit 4297fbb is that failure.

### 8.2 Decision numbers are allocated at MERGE, by the primary session

**A subagent never allocates a decision number.** Concurrent branches cannot
see each other's `docs/DECISIONS.md`, so two of them appending "the next free
number" both pick the same one — and the collision is invisible until merge,
by which time the number is in code comments, doc prose, test names and commit
messages on both sides.

The protocol:

1. **A branch carries a placeholder**, spelled `D-TBD(short-topic-slug)` —
   for example `D-TBD(commodity-calendar-eras)` or
   `D-TBD(expiry-availability)`. It goes everywhere the real number would: code
   comments, module docs, `DECISIONS.md` entry heading, and the commit message.

   > These two examples are **illustrative and must stay unresolved.** The
   > commodity-calendar merge renumbered this very line to `D-0089` with a
   > blanket `sed`, because the rule's own example looked exactly like a live
   > placeholder — the protocol ate its own documentation. Step 3 below is
   > therefore a *targeted* rewrite, never a whole-tree substitution: on a merge
   > where both meanings of a number coexist, decide per occurrence.
2. **The slug is required and must be distinctive.** `D-TBD` alone is not a
   placeholder, it is a collision waiting to happen the moment two branches use
   it; the slug is what makes the rewrite mechanical and greppable.
3. **The primary session assigns the number at merge**, from the next free one
   *at that moment*, and rewrites every placeholder **in the merge commit** —
   not in a follow-up. A tree that carries both `D-TBD(x)` and `D-0091` for the
   same decision is the stale-reference problem §8.1's third bullet is about.
4. **No placeholder survives a merge.** The check is that `D-TBD(` appears
   nowhere on `main` **except** where this section or a decision entry is
   *describing* the protocol:

   ```bash
   grep -rn 'D-TBD(' --include='*.rs' --include='*.md' --include='*.toml' . \
     | grep -v '^./.claude' | grep -v 'CLAUDE.md' | grep -v 'placeholder'
   ```

   Written as an exclusion because the naive `grep -r "D-TBD"` matches this
   rule's own text and the two decision entries that explain why a number was
   assigned directly — the first run of it did exactly that. A check that fires
   on its own documentation gets ignored within a week, which is worse than not
   having one.

**The motivating record — four collisions, all on merge, all avoidable:**

| number | claimants | resolution |
|---|---|---|
| **D-0077** | resampler · session eras · S0 predictor seam | resampler kept it; seam → D-0081; eras → D-0086 |
| **D-0085** | S0 caller · expiry-availability rule | caller kept it; expiry → **D-0090** at merge |

Three squatters on one number, then a fourth collision on a second number
within two days. Each cost a renumber commit touching every reference — ten
files for the seam, eleven for the eras — and each was discovered only because
someone read the log before merging. The next one would be discovered by a
reader in six months wondering why D-0085 describes two unrelated things.

The rule is cheap and the failure is not: a placeholder costs one `sed` at
merge; a collision costs an archaeology session.

Hard NEVERs (in addition to §2):
- Never commit market data, `results/`, or anything matching `.gitignore`'s
  data patterns — including `.env`, which is where the API key now lives
  locally (D-0022). Never hardcode or log `DATABENTO_API_KEY`: it is read
  from the process environment, at the last moment, by bin targets only.
- Never allocate a decision number on a branch — placeholders only, assigned at
  merge by the primary session (§8.2).
- Never merge into a working tree another process is using, and never merge
  into `main` from a subagent (§8.1).
- Never mutate or delete files under the raw archive (`raw/`) from code.
- Never add a dependency outside §6 without asking.
- Never weaken a lint, delete a failing test, or loosen a golden value to
  get green.
- Never introduce `async`, threads, clocks, or I/O into `core`, `engine`,
  or `strategies`.
- Never leave `combo`/`stats`/`registry` module-doc specs stale after
  implementing them.

---

## 9. Things that look like bugs but are decisions

Read this before "fixing" anything on the list. Each entry has a decision-log
reference; supersede the decision if you disagree — don't hotfix.

- **Bars appear "one interval late"** (a 09:30 1m bar is processed at
  09:31): that's `avail_ts` ordering, the core anti-lookahead device
  (D-0003). Not a bug. Never "optimize" it away.
- **Orders never fill on the bar that triggered them**: replay step order is
  fills→mark→decide with strict `placed_ts < avail_ts` filling (§2.1).
- **The demo strategy loses money under costs**: correct and intended — the
  demo data is a seeded random walk; there is nothing to find. A change
  making the demo profitable under `spread_cross` is a red alert, not a win.
- **`FreeFills` exists**: sanctioned screening tool for S0–S1 only (D-0006).
- **Money as `i64` nanodollars, not `f64`**: D-0002. Do not "simplify" to
  floats.
- **Indicators take points-f64, not nanos**: precision (§2.3).
- **Release builds panic on overflow**: D-0004. A crash beats silently
  corrupted PnL.
- **SMA/Bollinger rolling sums drift over very long streams**: known,
  accepted until the M2 Welford/rebase task. Don't "fix" by recomputing the
  window per bar (O(period) per update kills grid throughput).
- **`SmaCross` is not supposed to be profitable**: it's the reference
  fixture for tests/demo, chosen for simplicity, not merit.
- **Synthetic feed clamps prices at a floor**: keeps long random walks
  positive; documented in `synthetic.rs` (D-0011).
- **`pull` exits 5 rather than 0 when jobs are still processing**: the data is
  bought, journalled, and downloadable for 30 days — but a cron that reads
  "still processing" as success never comes back for it (D-0034). Re-running
  the identical command resumes and submits nothing twice.
- **`pull` refuses when two vendor jobs match one window**: adopting the wrong
  one archives the wrong bytes and submitting anyway buys the window twice, so
  ambiguity stops the run (D-0029). Refusing costs a re-run; guessing costs
  money or correctness.
- **`submit` is never retried on a dropped connection, while every other
  vendor call is**: a transport failure on a submission is ambiguous — the job
  may exist — and the next run's reconciliation resolves it for free (D-0035).
  Do not "fix" the inconsistency by retrying everything.
- **Manifest lines can be tens of KB**: a 16-year parent pull resolves to every
  outright and calendar spread in the window, and all of them are recorded so
  `coverage` tells the truth per contract (D-0033). Filtering to outrights
  reintroduces the re-buy bug.
- **The archive keeps `staging/`, `delivery/`, `jobs.jsonl`, and `pull.lock`
  beside `raw/`**: none are acquisitions, so none appear in the manifest.
  `raw/` stays exactly what D-0017 says it is.
- **Curated files are named after the raw window, not the year**
  (`curated/bars/ESH2024/1m/2024-01.parquet`), and `curated/` is not in the
  manifest: one raw file fans out to one curated file per instrument, each
  naming exactly one source blake3 (D-0036). A year-named file would have to
  be merged, and merging is where duplication hides.
- **A curated contract directory carries a FOUR-digit year** — `GCZ2014`, never
  the vendor's `GCZ4` (D-0072). A CME year code has one digit and repeats every
  ten years; every bar window here is sixteen years long, so `curated/bars/GCZ4/`
  held Dec-2014 and Dec-2024 gold concatenated, spanning 14.5 years. Two digits
  would be arithmetically enough and still one character from the vendor's
  spelling (`CLZ36` really means 2036); four can never collide with a year code.
  The year is resolved **per record against the contract's own expiry**, read
  from the archived `definition` file — never against a `DecadeAnchor` constant,
  which has an answer for `GCZ4` and is right for half the bars.
- **`transcode` refuses a whole file when a bar's contract cannot be resolved**,
  with no fallback to the anchor constant (D-0072). This looks like D-0070's
  spread filter and is its opposite: a spread is a record nothing replays *yet*,
  so it is filtered and counted; a bar under the wrong contract is corruption of
  *meaning* that looks exactly like correct data, and the silent path is what
  produced the bug. A missing `definition` fails loudly.
- **A spread partition still carries the vendor's spelling** under
  `--include-spreads`, decade ambiguity and all (D-0072). Resolution applies to
  outrights, because an outright is what a strategy replays; a spread does not
  parse as a contract and has no delivery year. Bounded and stated, not hidden —
  making spreads replayable means resolving their legs first.
- **`backtest --instrument GCZ4` refuses rather than picking one** when two
  curated contracts answer to that spelling, and prints `ESH4 -> ESH2024` when
  only one does (D-0072). The shorthand is convenience; answering an ambiguous
  one would move the archive bug into the CLI.
- **Neither the `ts_open` monotonicity check nor the gap-inside-sessions check
  could have caught D-0072**, and there are tests asserting they still pass on
  the merged fixture. Ordering is a statement about neighbours and aliasing is
  one about identity; the ten-year hole falls *between* sessions, and gold had no
  bundled calendar so `qa` never even looked. Do not "strengthen" either check to
  cover this — the partition key is what fixes it. **Gold has a calendar now**
  (D-0086), so the second reason is gone and the same planted merge is loud —
  over 1.87 M missing bars, coverage under 0.1 %. That is a side effect of
  building a metals session table, and there is a companion test asserting it;
  the original test keeps its assertion for the calendar-less call the bug
  report actually made.
- **`ParquetBarFeed` loads bars into RAM rather than mmap'ing them**, and
  `ParquetBarFeed::open` does all the failing: `Feed::next_event` returns
  `Option`, not `Result`, so a feed must have no errors left to report by the
  time it yields. MILESTONES said "mmap'd" before the format was chosen;
  Parquet pages are encoded and compressed, so there is nothing to map.
- **`transcode` refuses a whole file over one bad record** (unaligned
  `ts_event`, `UNDEF_PRICE`, an unmapped `instrument_id`) instead of skipping
  it. Curated data is derived and rebuildable, so a refusal costs one re-run —
  the calculus that made D-0033 *drop* a symbol does not apply here, because
  nothing has been paid for.
- **A zero or negative price is not a bad record**, and the validity test is
  `!= UNDEF_PRICE`, never `> 0` (D-0070). CL settled at −$37.63 on 2020-04-20 as
  an *outright*, and a calendar spread's differential is negative whenever the
  market is in contango — together, 8.9 % of every bar record in the archive.
  `UNDEF_PRICE` is the vendor's way of saying "no price"; below zero is a price.
  Re-adding the positivity check refuses `GC.FUT ohlcv-1m` at record #0 and
  makes the archive untranscodable, which is how it was found.
- **`transcode` excludes spread instruments by a declared filter with a count,
  not by a refusal, and the count prints even when it is zero** (D-0070). A
  spread is not a record this build cannot read, it is a record nothing replays
  yet, so refuse-the-whole-file does not apply; `--include-spreads` writes them
  and `raw/` keeps them forever. The predicate is *contains `-`, `:`, or a
  space* — a marker test rather than an outright-shape test, deliberately,
  because the default excludes: mistaking an outright for a spread silently
  omits real bars, while mistaking a spread for an outright only writes a
  partition nobody reads. Naming a spread in `--symbols` without the flag is
  refused rather than answered with an empty report.
- **`transcode` re-decodes a file to discover it has nothing to write**: which
  contracts a window actually produced is only knowable after decoding it. A
  parent key's symbology maps every contract it resolves to and most never
  trade — January 2024 `ES.FUT` maps 41 and produces 16 — so treating the
  mapped set as the expected set makes a finished transcode look permanently
  unfinished.
- **`backtest` measures `bars_per_year` from the sample** instead of using the
  demo's 347,760 constant: real `ohlcv` data has no bar for an interval that
  did not trade, and the constant would flatter Sharpe (D-0038). `calendar`
  v1 takes this over.
- **`backtest` prints two annualization numbers when they differ.** The
  calendar counts the `tf` intervals a year of sessions *contains*; measuring
  the sample counts the intervals that actually *traded*, which is fewer,
  because `ohlcv` has no bar for an interval with no trade. Neither is wrong
  and the calendar is used (D-0039, superseding D-0038's sample default). A
  large gap means a thin contract or a hole — that is the point of showing it.
- **The 15:15–15:30 CT halt exists in one era of the equity-index calendar and
  not in the next**, and both are right (D-0086). D-0040 deleted it after
  finding 315 nonzero-volume ESH4 bars inside it in January 2024; that
  measurement stands, and January 2024 is era 3b. Over the whole archive the
  window carries 0.04 traded minutes per date on 2,018 dates from 2015-01-01 to
  2021-06-25 and 15.00 on every one of the 1,344 from 2021-06-28 — and CME's
  SER-8788R removed it effective exactly 2021-06-28. The archive is still the
  evidence; what changed is that the table can now hold two answers.
- **A calendar carries session ERAS, and `[calendar.session]` is the current
  one** (D-0086). Earlier templates are `[[calendar.era]]` entries with their
  own `from`, open, close, halts and RTH. A `reference_span` that crosses an era
  boundary is REFUSED at load: `bars_per_year` averaged over two different
  exchanges describes neither, which D-0039 said in prose and which stopped
  being true the moment era 3 turned out to be two eras.
- **Era 1 of CME equity index (2010-06-06 .. 2012-11-16) is documented and NOT
  modelled**, and `valid_from = 2012-11-19` says so. Its trading day opens 15:30
  CT on D−1 with a halt at 16:30–17:00 CT *on D−1*, and that evening block is
  absent whenever D−1 is not itself a trading day — the template can express
  neither. Both approximations were measured before being rejected (≈30,000
  out-of-session bars per contract, or ≈60 phantom expected bars a week);
  `docs/SESSION_ERAS.md` §1.1 has the shape so nobody re-derives it. `qa` and
  `backtest` still warn for a span starting earlier, and the functions stay
  total — a date before every era gets the OLDEST era's answer, because a later
  era's hours would be a bigger lie about an earlier exchange.
- **Most CME "holidays" are early closes rather than closures — but not at the
  same time, and not at the same hour for every product.** CME's trading-hours
  *landing page* says "Globex closed" and its per-holiday grids disagree; the
  grids win, and MLK 2024-01-15 agrees with them. What the archive adds
  (D-0086): equity index ran **full closures** from **2012-11-22** to
  2014-02-17 — proved by the Sunday and Wednesday evenings that did not open —
  and 10:30 CT closes in era 1; and on MLK 2022-01-17 the last traded minute was
  12:00 CT for ES and ZN, 13:30 for CL and GC, and 15:58 for 6E, which traded a
  full session. One date, four answers, which is why there are four commodity
  calendars and not one. **All four commodity products were closed on the same
  nine dates**, and checking them against ES and NQ is what found D-0086's start
  date off by one holiday — it read 2013-01-21, three trading days *after* the
  equity table's own `valid_from`, so the calendar reported a normal session on a
  day the exchange was shut (D-0089).
- **The four commodity calendars describe 2010-06-06 onwards, and 26 dates
  inside that span are knowingly wrong by 45 minutes**
  (D-0089). CL and GC carry a 16:15 CT close before
  2015-09-21 and ZN a 17:30 CT open before 2011-10-03, as `[[calendar.era]]`
  entries; the pre-2013 holiday close was 12:15 CT for energy and metals and
  12:00 for FX and rates. What is *not* modelled is a 15:15 CT pre-holiday close
  on 26 dates for 6E and ZN and 6 for CL and GC, because the pattern has three
  regimes and a hole in it. That trade is deliberate and is the D-0040 argument:
  an unmodelled early close is reported as *missing bars*, which blames the
  archive, and it costs ~1,200 minutes over five years — against ~20,000 minutes
  per contract of real bars that `valid_from = 2015-09-21` reported as **outside
  any session**, which indicts the calendar. May 2011 out-of-session counts went
  CL 202 → 0 and GC 283 → 0. ZN's 17:30 open is the one era in these tables with
  **no publication behind it**; its `source` says `UNVERIFIED` and
  `docs/SESSION_ERAS.md` §6 lists what would settle it.
- **`cme_globex_rates` has no Columbus Day and no Veterans Day**, although the
  cash Treasury market closes on both and `docs/THETADATA_PLAN.md` §8.1 records
  Veterans Day as a day the NYSE trades and the bond market does not. CBOT
  Treasury **futures** on Globex traded a full session on every one of them in
  sixteen years (D-0086). Cash and futures are different markets; the prior was
  checked and refuted, not assumed.
- **ZN bars at 16:00 CT are SETTLEMENT PRINTS, the era table's close is right,
  and `qa` calling them out-of-session is correct** (measured 2026-07-31,
  confirming D-0089). A report of out-of-session ZN bars at **21:00Z** on
  2014-10-02 and 2014-10-10 asks whether the settlement window ends later than
  `close_local = "16:00"` says. It does not, and the archive settles it in one
  measurement: across **1,436 Mon–Fri dates in both eras** (era 1
  2010-06-06..2011-09-26, era 2 2011-10-03..2015-12-31), the minutes **16:01
  through 16:04 carry volume on ZERO dates** — not one bar, in five and a half
  years. A session that ran later would print there. The 16:00 minute itself
  trades on **47.34 %** of era-1 dates and **2.46 %** of era-2 dates, against
  **92–95 %** for each of 15:55–15:59: an order of magnitude too rare to be a
  session minute, and exactly the signature `session_profile`'s own module doc
  names — *"a halt is a block of near-zeroes; a settlement print is one minute
  at 5 %"*. The table already recorded this for era 1 in its `source` comment
  ("carries a settlement print on 47 % of dates, which is why the close is read
  off Friday") — the 2.46 % figure is the same behaviour, rarer, after the 2011
  era change, which independently corroborates that era boundary.
  **Moving the close to 16:01 to absorb them would be the D-0040 argument run
  backwards**: it would declare a session minute that is silent on 97.5 % of
  dates, so `qa` would report a missing bar at 16:00 on nearly every date in the
  archive. An unmodelled early close blames the archive; a modelled minute
  nobody trades blames it just as loudly and 40 times more often.
  **The UTC time is a DST artefact and not a property of the print.** 16:00 CT
  is 21:00Z in CDT and 22:00Z in CST, so the same settlement minute appears at
  two UTC times depending on the season. An operator grepping a fixed UTC hour
  sees half of them.
  **What the same query DID find is a real hole**: ZN 2014-10-02 (Thursday) ends
  at its 16:00 settlement print with **no evening session at all**, and
  **2014-10-03 has no bars whatsoever** — not a holiday, not an early close. That
  is a genuine absence and is the window `--refetch-window` targets, entirely
  separate from the settlement-print question that prompted the look.
- **Christmas landing on a Saturday closes the Friday before and New Year's Day
  landing on a Saturday does not**, so the two rules are written differently
  (D-0086). 2010-12-24 and 2021-12-24 have no session at all in the archive;
  2010-12-31 and 2021-12-31 are full ones. Making them symmetric re-introduces
  the 12:15 CT close 2021-12-24 never had.
- **`rth_open_local`/`rth_close_local` on the four commodity tables are a cited
  convention, not a measurement**, and are the only field in them that is.
  Open outcry ended for CL and GC on 2016-12-30 and CME publishes no RTH window
  for any of the four, so the values are the inherited floor hours. They are
  read only by `Calendar::session_of`; nothing in `open_intervals`, `is_open`,
  `is_trading_day` or `bars_per_year` touches them.
- **`curated/bars/ESH2024/5m` never exists, and that is not an unfinished
  transcode** (D-0077). The archive stores the grain the vendor sent — `1s` and
  `1m` — and `5m`/`15m`/`1h`/`1d` are aggregated on read, on the exchange's own
  sessions. Writing them would need the read-modify-write merge D-0036 exists to
  prevent: raw windows are monthly and a month boundary lands *inside* a CME
  session, so a daily bar's constituents live in two raw files. Every report says
  which of the two produced its bars, including when nothing was resampled.
- **A resampled daily bar opens at 17:00 CT the previous evening, not at
  midnight** (D-0077). It is a trading-day bar; the bucket grid is anchored on
  `Calendar::session_open`, so no resampled bar can span a session boundary. A
  UTC-day grid would put the last minutes of 3 January and the first minutes of
  the fourth's session in one "daily" bar — they are 61 minutes and one trading
  day apart.
- **An early close makes the last bucket short and is NOT reported as a
  truncated window** (D-0077). `last_bar_may_be_partial` is computed against the
  session close, so it says "the request cut this bar" and never "the exchange
  did". A 12:15 CT close makes bucket 19 of an hourly resample fifteen minutes
  long, which its volume states.
- **`resample` refuses a halt PER TRADING DAY, not per calendar** (D-0077,
  D-0086). The bucket grid is anchored once per trading day, so a halt — which
  is a session boundary — could sit inside a bucket, and refusing beats emitting
  a bar whose constituents straddle a break. The gate was calendar-wide until
  the eras merge, when equity index grew a real halt in era 3a: a calendar-wide
  answer would then have refused **every modern ES bar** for a halt that ended
  on 2021-06-28. `Calendar::declares_halts_on(date)` is what the grid asks;
  `declares_halts()` survives as the coarse question and is documented as too
  blunt to gate with.

  Worth noting how this was caught, because it is the pattern working: the
  resampler's author left a test asserting *no bundled calendar declares a
  halt*, whose failure message read "D-0077's per-day bucket anchor needs
  revisiting". It fired on the merge, on the exact day a table grew a halt.
  A tripwire that names its own remedy is worth more than a comment.
- **`crucible qa` exits 4 when it finds something**, like `verify`. A
  scheduled job that reads "coverage 61 %" as success is worse than no job.
- **`qa`'s spike count is enormous on crisis contracts, and those spikes are
  REAL** — classified 2026-07-31, `docs/SPIKE_FORENSIC.md`. ESH2020 flags 6,974
  and every one of its top 20 falls in Feb–Mar 2020; the largest lands on the
  *minute* of the Fed's emergency cut, two more land one minute after a Sunday
  reopen, and all four level-1 circuit-breaker days are present. CL contracts
  cluster the same way on 2012-09-17, 2015-02, 2013-06-20 and 2020-04-21 — the
  day after WTI settled negative, which §9 already documents. Do not "fix" the
  archive over these.
- **The spike sigma's defect is RESOLUTION, not staleness, and the earlier
  "dominated by calm 2019" account was measured and refuted** (D-0099).
  `sigma = 1.4826 × median(|Δclose|)`; every price is an integer multiple of the
  tick, so every move is, so the median is — the estimator can only ever return
  an integer number of ticks. Across all 863 curated contracts there are **43
  distinct sigma values in the whole archive**. ESH2020's 0.3707 pt is *identical*
  to ESH2011, ESM2013, ESU2016, ESZ2018, ESM2021 and ESU2023: it is one ES tick
  and says nothing about 2019 or any other year. The floor binds on 44 of 67 ES
  contracts, so for those the 8σ gate is a constant **2.9656 points** whatever
  the market was doing — while GC ranges 1→20 ticks and NQ 1→43, so it is a
  volatility estimate, just one with no resolution between adjacent integers.
  **A rolling median would not repair this** — it saturates harder at a shorter
  window. Any estimator built from an order statistic of a lattice-valued
  variable inherits the lattice.
- **`qa` prints NO spike line at all when it cannot compute a sigma, and 47
  contracts hit that path** (D-0099). `if mad <= 0.0 { return; }` fires whenever
  over half a contract's bars did not move: **44 of 68 ZN contracts** (a 1/64
  tick and quiet minutes), plus three deep-deferred CL. The line is *absent*
  rather than zero, so an automated read scores it as "checked, clean" —
  **4,002,334 of 70,641,676 curated bars (5.7 %) have been reported clean
  without being examined**, and every count in `docs/SPIKE_FORENSIC.md` is drawn
  from that undeclared subsample. Printing the line with its reason and a
  skipped-bar count is a reporting fix independent of any statistical one, and
  it is owed first.
- **The replacement estimator is still deliberately unimplemented**: old and new
  counts must be reported together, only the old ones exist, and a change argued
  from plausibility alone is the thing this section exists to refuse.
  `docs/SPIKE_FORENSIC.md` lists what the implementer owes — including a
  planted-print control, a converse control written first, and an explicit
  answer for the zero-scale case.
- **`AdjustedPrice` cannot be converted to `Price`, and that is the feature**
  (D-0042). Back-adjusted levels are for signals; PnL uses the tradeable price
  of the then-front contract. Adding a `From` impl reintroduces the classic
  silent-corruption bug this whole type exists to prevent. It lives in
  `crucible-core::types` because `crucible-strategies` — where indicators are —
  may not depend on `crucible-data` (D-0076); `crucible_data::continuous::
  AdjustedPrice` is a re-export and the compile-fail proof exists on both paths.
- **Every `Bar` carries a `signal_offset`, and every indicator reads
  `bar.signal_*()` rather than `bar.close`** (D-0076). The four `Price` fields
  are always tradeable — fills, marks and `pnl_nano_usd` use those. The offset
  is zero for every outright contract and every synthetic bar, so the two views
  coincide and nothing moved; it is nonzero only on a stitched series. An
  indicator switched back to `bar.close` sees a step at every roll while its
  neighbours in the same rule do not, which is a signal that fires on the roll
  table rather than on the market.
- **A back-adjusted level is not §2.1 lookahead for a shift-invariant strategy,
  and IS for a level-sensitive one** (D-0076). Every bar visible at time `T`
  carries the same additive constant, so crossovers, returns and differences
  are unaffected and it cancels; `close > 4500` is not, because where a bar sits
  depends on rolls that had not happened. That asymmetry is why `backtest`
  replays `ES.v.0` and `combo` / `walk-forward` refuse it — one runs a strategy
  an operator named, the other expands rules it has not seen.
- **A position carried across a roll books the raw gap as PnL and pays nothing
  for the roll** (D-0076). A roll is a *position* event and this build models it
  as a price event only; the fills a real roll generates arrive in M2.
  `backtest` prints the bound — `Σ |gap| × point_value × qty` — so the omission
  has a size rather than a mention. On the 16-year ES run it is $56,950 against
  a $3.44 M loss.
- **`backtest` narrows a date request to the roll table's span and says so;
  `ContinuousFeed::open` still refuses** (D-0076). `--start 2010-06-06` is a
  calendar day and the first ES bar opens at 22:00 that day, so the request
  overhangs the span by the difference between a date and an instant, which no
  rebuild fixes. A request with no overlap at all still refuses. The narrowing
  is printed with the table path and the rebuild command, because a table the
  archive has outgrown is the case D-0045 actually cares about.
- **`backtest` prints an `INSOLVENT` block instead of hiding the ratios it
  invalidates** (D-0076). There is no margin model, so a replay can carry a
  position no broker would and equity goes negative. The dollar figures stay
  exact; the naive Sharpe is computed only over steps whose starting equity was
  positive, so it describes the solvent prefix — +0.23 beside a −3,443 % return
  on the ES run — and max drawdown passes 100 %. Fixing `Summary::compute` is
  its own decision; suppressing the number would be worse than explaining it.
- **`raw/` and `curated/` nest in opposite orders** — `{dataset}/{schema}/
  {symbol}` vs `{kind}/{instrument}/{grain}`. Deliberate, argued at length in
  `docs/DATA_LAYOUT.md` (D-0049); unifying them destroys either coverage
  subtraction or curated provenance. `layout-check` enforces both shapes.
- **`layout-check` has no `--fix`** and must never grow one: a manifest
  `file_path` names the bytes a result read, so renaming an archived file
  breaks provenance retroactively and silently (D-0049).
- **`ContinuousFeed::open` refuses a replay window outside the roll table's
  build span** rather than returning a series quietly missing contracts
  (D-0045). Rebuild the table instead: it is curated data, and disposable.
- **A contract's expiry is keyed on `max(ts_recv)`, and NEVER on
  `max(expiration)`** (D-0090). The vendor restates 4 contracts of this
  archive's 1,002, and in **4 of 4** the later record carries the *earlier*
  expiry — the exchange pulled the settlement forward. So the `max(expiration)`
  spelling, which D-0054's "dedup keeps `max(created)`" invites by analogy,
  selects the stale record every single time and moves ZN's and 6E's rolls onto
  the wrong session in silence. The two implementations differ by one word and
  one of them is silent corruption. `the_key_is_max_ts_recv_and_never_max_
  expiration` exists to fail on it.
- **The expiry is filtered against the roll's own decision instant even though
  plain "latest wins" would give the same table on 100 % of this archive**
  (D-0090). Every correction here landed 14–18 months before its roll, so the
  filter costs nothing today; what it buys is refusing to absorb a correction
  that lands *after* the roll it would move, which is §2.1 lookahead and which
  latest-wins cannot even detect, never having read `ts_recv`. `rolls` prints
  the count — `0` on this archive — rather than staying silent about it.
- **Two expiries for one contract are a REVISION, not a conflict**, and only
  *overlapping* availability windows refuse (D-0090, superseding D-0046's
  refusal). D-0046 refused the file, which is what stopped `crucible rolls` on
  GC, ZN and 6E entirely. The refusal that remains names **every** offending
  contract in the root: returning on the first is why `ZNM2012` sat unreported
  behind `ZNZ2011`, and a refusal that reports one of two makes the archive look
  better than it is.
- **The `.c` rule resolves a fixed point without iterating**, and there is no
  "did not converge" error to write (D-0090). `due` depends on the expiry and
  the expiry depends on the decision instant, but the *candidate set* does not:
  a candidate session's decision instant comes from the bars alone. So the
  equation decomposes into one self-consistent test per session and the answer
  is the earliest that passes — one ascending scan, bounded by the session
  count.
- **A `.v` roll table records `expiry_source = "none"` even on a root whose
  `definition` file is archived** (D-0090). The volume rule reads no expiries —
  proven by a control that builds it under an absurd history and gets a
  byte-identical table — so resolving them is work whose only effect was to
  make `rolls --root GC` exit 4 on an input it never read. The field says what
  the table *used*, not what was lying around.
- **Every combo's equity curve starts with a flat prefix as long as the
  *grid's* warmup, not its own**, so a 10-bar combo sits idle for 200 bars
  next to a 200-bar one. That is §2.6 working (D-0061): the alternative is a
  short combo scored on a longer, differently-timed sample. The prefix is
  identical across the grid, so rankings are fair; each naive Sharpe carries
  the same `sqrt(n_eval/n_total)` factor. `combo` prints the suppressed-order
  count so the effect is visible rather than absorbed, and points at
  `walk-forward`, which slices the window out of the metrics and so does not
  carry the factor (D-0063).
- **A float parameter axis cannot be written as `{ start, end, step }`**
  though an integer one can (D-0060). Repeated addition of 0.1 lands on
  2.0000000000000004, so the *number of points* on the axis — and with it the
  combo count, every combo index, and every trial charged to the hypothesis
  family — would depend on floating-point accumulation. Write the values out.
- **There is no full-sample z-score in `crucible-strategies::indicators`, and
  adding one is not a convenience** (D-0080). Every statistic there is a
  trailing window behind `Indicator::update`, which takes one bar; no
  constructor takes a series, and no `IndicatorKind` names a full-sample
  variant, so `controls::LeakyZScore` stays reachable from Rust and unreachable
  from TOML. The property is truncation invariance, and its control is a
  full-sample function written **inside the test file** — the only place it may
  exist.
- **`zscore` returns `None` on a flat window while `stdev` returns `0.0`**
  (D-0080). The z-score's numerator and denominator are both zero, and `None`
  is what the grammar already means by "no opinion"; a deviation of zero is a
  real answer. Neither is a NaN caught downstream by accident.
- **`source = "return"` adds a bar to `Grid::max_warmup_bars`** (D-0080). A
  return needs two closes, and the bar is declared rather than absorbed so §2.6
  aligns the whole grid on it — otherwise a mixed-source grid would start its
  return combos one bar late while reporting they started with everyone else.
- **A combo config whose rules read the session clock is REFUSED against a
  synthetic feed**, rather than run with those rules silent (D-0078). A
  `minutes_since_open < 30` with no calendar behind it has no opinion on any
  bar — which is a backtest of a different strategy from the one the config
  describes, and looks exactly like a strategy that never found a signal.
- **`minutes_to_close` shortens on an early close and `minutes_to_rth_close`
  does not** (D-0078). The first asks "is the exchange still open", the second
  "how far into the scheduled trading day are we", and on CME's 12:00 CT
  Independence Day they disagree by four hours. Collapsing them makes one of the
  two questions unaskable.
- **The last regular-hours bar of a day reads `is_rth`, though its interval ends
  exactly at the closing bell** (D-0078). Open intervals are half-open, so the
  session is asked one nanosecond before `avail_ts`; asking at `avail_ts` would
  report every day's final bar as closed and "flatten on the last bar" would
  never fire.
- **Rule evaluation never short-circuits, and all four rules are evaluated
  every bar** even when the position makes the answer irrelevant. A
  `crosses_*` node that misses a bar compares against a reading from two bars
  ago and fires late — a lookahead-shaped bug with none of the symptoms.
- **`combo` refuses a config declaring two instruments or two timeframes**
  rather than running the first. The cross-product over a universe is the
  funnel's job; a partial answer printed in the shape of a whole one is worse
  than a refusal that costs a config edit.
- **A combo where `enter_long` and `enter_short` fire together takes no
  position**, and the count is printed. Picking one arbitrarily would be a
  silently-wrong result; the config, not the strategy, is what is broken.
- **`walk-forward` refuses `step_days < test_days`** rather than running
  overlapping out-of-sample windows (D-0062). The headline pools the test
  windows; pooling overlapping ones counts a session twice, which inflates the
  sample size and flatters every statistic that reads it. A step *wider* than
  `test_days` is allowed, and the sessions no fold reaches are printed.
- **`walk-forward` drops the partial session the warmup ends inside**, so the
  first fold opens on a whole trading day and its "60 sessions" is 60 sessions
  (D-0062). The dropped bars are counted in the report, not absorbed.
- **A round-trip opened in a training window and closed in a test window is a
  test-window trade** (D-0063). The equity series still splits the *money* at
  the boundary correctly — each window keeps the marks inside it — but the
  trade count and win rate follow the closing fill, which is when the PnL
  settles into cash.
- **Per-fold percentages are computed against the config's declared capital,
  not against the equity the account had drifted to** (D-0063). Position size
  is a fixed contract count, so a window's dollar PnL does not scale with the
  account, and rebasing is what makes fold 7 comparable to fold 1.
- **`walk-forward` prints per-fold detail for the first N combos by grid
  index, never for "the best N"**. A report sorted by the number you are about
  to quote is a selection step wearing a report's clothes.
- **Every (combo, fold) carries a derived seed although nothing consumes
  randomness yet** (D-0064). Building the derivation before its first consumer
  is the point: otherwise the first randomized component invents its own, in
  the least visible place.
- **A bar that touched both the stop and the target fills the STOP**, always,
  never the target and never the nearer level (D-0069). The bar does not record
  which printed first; resolving ambiguity in the strategy's favour pays it for
  a fact the data does not contain, and pays more the tighter the bracket. The
  count of such bars is printed with the result — that is the flag, not a
  debugging aid.
- **A gapped level fills at the opening print, not at the level** — including
  a gapped *target*, where the opening print is better than the level (D-0069).
  The rule is "never manufacture a price the market did not offer", and it is
  symmetric. It also means a bar that opens through the target fills the target
  even if the low later reached the stop: the alternative asserts the price
  passed a resting limit without filling it.
- **A gapped exit is not counted as path-sensitive** though both levels may lie
  inside the bar: the opening print settled the ordering, so nothing was
  assumed. Counting it would bury the bars that actually depend on the
  convention.
- **A stop fills when the price merely touches it, but a target needs a print
  strictly through it.** Deliberately asymmetric (D-0069): a trade at the stop
  proves the market got there and a stop is a market order from that instant; a
  trade at a resting limit proves only that there was a queue in front of it.
  Both halves are the pessimistic reading, so they point opposite ways.
- **A bracket can stop out on the same bar its entry filled on**, and under
  `spread_cross` a stop closer than the half-spread stops out *immediately*
  (D-0069). Both are correct. The bracket rests from the moment the parent
  filled, and a stop one tick below a price you paid the offer for is sitting on
  the bid.
- **`combo` and `walk-forward` print an intrabar-convention line saying the
  count is zero and why.** No config can declare a bracket in this build, so
  "no number printed" and "no ambiguous bars" would look identical to a reader
  otherwise — and only one of them means the returns are safe to quote.
- **The engine takes trading-day keys as an `&[i64]` argument rather than
  asking a calendar**, and `DayRecord.trading_day_key` is a bare `i64` rather
  than a date type (D-0071). `crucible-engine` may not depend on
  `crucible-data`, and neither may `crucible-funnel`, so `crucible-cli`
  computes `days_from_civil(Calendar::trading_day(avail_ts))` **once** and both
  consumers read the same slice — the D-0015 / D-0060 / `FoldPlan` device again.
  Two independent attributions of "which day" is how a daily-loss-limit breach
  lands on a different date in two reports. Adding a `Calendar` argument, a
  slicer trait, or a day derived from the timestamp inside the engine
  reintroduces exactly that.
- **A captured trading day opens at the PREVIOUS day's close, not at its own
  first mark** (D-0071), so a day whose first print gaps down reports that gap
  in `trough_from_open_nano_usd`. Anchoring on the first mark makes the
  overnight move invisible and breaks the recursion that lets a whole-day
  bootstrap answer an intraday question.
- **MAE/MFE are sampled at bar closes and are therefore lower bounds** on what
  a position actually endured, like every other number on a mark grid
  (`ACCOUNT_EVAL_SPEC.md` §3.3.2). A round-trip can report `mae_nano_usd = 0`
  while closing at a loss: no *mark* caught it down. They are also measured on
  realized-plus-unrealized PnL of the episode, so a scale-out's banked profit
  counts — measuring unrealized alone reports a trade that was $500 up as
  never having been up at all.
- **The intraday high-water is an O(1) reducer, not a retained series**, so
  nothing downstream can ask what the account's equity was at bar 4,000,000
  (D-0071). A 16-year 1-second replay is ~334 M marks: a second per-bar series
  costs another 4.98 GiB beside the per-bar equity vector that already costs
  that much. The breach question is a running maximum and a running
  maximum-drop, and neither needs history — `ACCOUNT_EVAL_SPEC.md` §3.3.1
  proves the retained per-day summary decides it exactly. That is why the
  artifact is 56 bytes a *session* rather than 16 bytes a *bar*, and why
  `the_high_water_reducer_is_sixteen_bytes` and `a_day_record_is_fifty_six_bytes`
  are pinned: growing either should be a failing test, not a memory regression
  nobody measures. Turning `HighWaterState` into a `Vec` "to keep the option
  open" is the change those two tests exist to refuse.
- **`approximate_day_count` is 0 or 1, never more, and the day it names is
  flagged even when the crossing plausibly happened at that day's close**
  (D-0071). A running peak is non-decreasing, so it crosses a ratchet lock's
  level exactly once per path; and a day's `peak_from_open` is by definition at
  least its `close_pnl`, so a summary can never certify that the peak first
  reached the level *at* the final mark rather than earlier inside the day.
  Certifying it anyway resolves an ambiguity in the flattering direction, which
  is the one thing the whole spec is against. An account whose ratchet is
  `highest_daily_closing_equity` advances only at a close, so it has no
  approximate day at all — a zero there is the right answer, not a detector
  that failed to fire.
- **`crucible funnel` can never print `GRADUATE`**, however good a combo looks
  (D-0075). §4 defines Graduate as "survived the full battery"; the battery is
  S3 — deflated Sharpe, PBO/CSCV, permutation nulls — and S3 is not in this
  build, so the ceiling is `Iterate` and every report and scorecard says so in
  those words. A reader who saw no `GRADUATE` and no explanation would conclude
  nothing was good enough, which is a much more flattering claim than the true
  one. Do not "enable" it by relaxing the stage list.
- **A config declaring `stages = ["s3", ...]` is REFUSED**, not run with the
  missing stage skipped (D-0075). A config that asks for the permutation battery
  and silently receives a fold table has been answered with a different question
  than the one it asked, and the answer looks exactly like the one it wanted.
  The refusal names what the stage needs. **`s0` left this list on 2026-07-31**
  when its caller landed (D-0085) — in that same commit, never earlier.
- **A `[s0]` block is required exactly when `stages` names `s0`, and refused
  when it does not** (D-0085). A stage with no criteria is a stage with no
  pre-registration; a block nothing evaluates is a config that thinks it asked
  for something.
- **S0 passes only when `|IC| >= min_abs_ic` AND the mean forward return's
  bootstrap interval excludes zero, at the SAME horizon** (D-0085). `|IC|` alone
  reads 0.0378 on 20,000 bars of seeded random walk, which clears any bar low
  enough to be useful — size without significance is what a large enough sample
  of noise gives away for free. The criterion was corrected when the null
  harness exposed this; the threshold was deliberately *not* raised to fit the
  fixture, which is the direction the seed-29 rule below forbids.
- **S0 DOES NOT CATCH LEAKS, and must never be "fixed" into a detector that
  does** (D-0085). A leak planted in the forward-return join saturates the
  information coefficient to **1.0000** and the verdict does **not** move: on a
  zero-drift random walk the mean forward return is still indistinguishable
  from zero, so S0's significance half fails and the combo is killed anyway —
  for the right reason, by accident.

  The saturated IC is a **leak signature**, and printing it is the whole of
  S0's contribution here: a reader who sees `|IC| = 1.0000` on a real signal is
  looking at a bug, because nothing predicts the next ten minutes perfectly.
  What S0 cannot do is *decide* that, and the reason is the one this section
  already gives for `LeakyZScore`: a leaked edge is indistinguishable from a
  real one by any statistic computed **on the leaked run**, and the IC is such
  a statistic. A threshold that killed leaks would kill real signals at the
  same rate.

  Catching a leak means asking a different question — does the edge survive
  when the future is shuffled, or deleted — and that belongs to the
  **permutation and truncation harnesses** arriving with
  `crucible-funnel::stats` (`docs/plans/m3-full.md`, block A). Adding a
  "suspicious IC" kill to S0 would put a detector nobody has watched fire in
  front of the one that can be, and would make the planted-leak acceptance test
  below passable without ever building the harness meant to pass it.
- **`funnel` exits 5 when every combo is killed.** Not a failure and not
  success: most ideas must die, and a scheduled job that reads "everything was
  killed" as exit 0 learns nothing — the same argument `qa`'s exit 4 makes.
- **A hypothesis file's config block and `configs/hypotheses/<id>.toml` are the
  same bytes, and the duplication is the point** (D-0101). A pre-registration
  states before the run what will be tested; that is worth nothing if the block
  was never runnable, because then the file that gets run is a different one,
  written later, by someone who has already seen the data. Both H-007 and H-008
  sat unrunnable from the day they were written — H-007 declared `s2` with no
  `[walk_forward]`, H-008 declared neither `s0` nor `[s0]` while its own first
  two gates are S0 measurements. `backlog_registration.rs` asserts byte-identity
  rather than "both parse", because two valid configs can describe different
  experiments and that difference is exactly what pre-registration exists to
  prevent. Do not "de-duplicate" this by replacing the block with a link.
- **`funnel` refuses `fill_model = "free_fills"`** although `combo` and
  `walk-forward` accept it (D-0006, D-0075). The funnel runs the free-fill
  screen itself at S1 and then asks S2 whether the edge survives honest costs;
  a config declaring `free_fills` makes those the same run and turns the
  mandatory cost sweep into one number repeated four times.
- **A cost sweep at 0 ticks still charges commission**, and the sweep's fill
  price at 0.5 ticks is deliberately **off the tick grid** (D-0073). The sweep
  moves the *spread*; a broker does not stop billing because the book got
  tight, and half a tick is an average over sessions rather than a price any
  single print pays. Rounding it back to the grid deletes the sweep's middle
  row, which is the one that answers "does this edge die at half a tick?".
- **The matched random-entry control is the median of 16 draws, not one draw.**
  A single seeded benchmark is a sample of size one, and a strategy that loses
  to it has lost a coin flip rather than a comparison. The count of draws the
  combo beat is printed beside the median and is the one empirical p-value this
  control can honestly give. It is **not** the permutation null, which shuffles
  real returns in blocks and is S3's.
- **A control that could not be built FAILS its criterion rather than passing
  it**, and renders as `ABSENT` with a reason (D-0075). An absent denominator is
  not a cleared bar, and a zero in that cell would read like a benchmark that
  was beaten.
- **A scorecard with an incomplete honesty box produces NO FILE**, not a page
  with a blank field. It is the one place in this codebase where an empty value
  aborts a render, because every other omission on such a page reads as "not
  applicable" and "we did not record the git sha" must not.
- **The scorecard renders the plateau heatmap, the regime table and the
  permutation null as *named holes*** rather than omitting them. A reader who
  does not see a null comparison cannot tell "there wasn't one" from "it
  passed".
- **`funnel` re-runs combos the registry says are already finished**, and the
  dedupe is still real: what it protects is the **trial count**, which does not
  advance on a re-run. The replay repeats because this build's registry stores
  metrics rather than equity curves, and a scorecard cannot be rebuilt from a
  metrics row. Skipping the replay would leave holes in the page.
- **Re-running a funnel appends a second `run_finished` line for a run that
  already had one.** The store is append-only by design (D-0074): the run
  genuinely ran again, and a log that mutated the first line would lose that.
  The index folds later lines over earlier ones, so the reader is unaffected.
- **A leaky strategy is checked into this repository on purpose, and the
  detector now catches it** (D-0087). `crucible-strategies::controls::
  LeakyZScore` fits on the whole series and then backtests — the lookahead §2.1
  names by name — and `crucible-funnel/tests/planted_leak.rs` asserted for
  months that the gates returned `Iterate` for it. **On 2026-07-31 the
  permutation null landed and that expectation flipped to `Kill`**, reached the
  only way §9 permits: by building the harness. Nothing else moved — not the
  strategy, not its registration, not a threshold.

  **Why tightening a threshold was never the repair.** For the months the test
  asserted `Iterate`, it was asserting a *failure* — deliberately. A leaked edge
  is indistinguishable from a real one by any statistic computed **on the leaked
  run**, so a threshold low enough to kill this would have killed real
  strategies at the same rate. The only way out was to ask a different question:
  does the edge survive when the future is **shuffled**, or **deleted**? Those
  are the permutation and truncation harnesses, and until they existed the
  honest thing was to record the gap rather than paper over it.

  **The shape of the catch is the lesson.** Every gate before S3 still passes —
  admission, S1, S2's Sharpe, the kill-level sweep, and both controls — and the
  test asserts that they do. If a future change made the leak die earlier, the
  verdict would still read `Kill` while the file had quietly stopped measuring
  the detector. The leak dies at S3 on `p = 0.2079` against a pre-registered
  `0.05`: it **re-fits on every permutation**, so its observed run is an
  ordinary draw from its own null.

  **That flip WAS M3's acceptance test**, not a step towards one.
  `docs/MILESTONES.md` reads "a deliberately-leaky test strategy is caught by
  the permutation/truncation harnesses (negative-control test)"; the strategy
  was already checked in and already registered as uncaught, so nothing had to
  be *built* for that clause beyond the harnesses themselves. Changing
  `Verdict::Iterate` to `Verdict::Kill` by any route other than a harness
  catching it would have ticked the milestone's last box with a lie in it —
  which is why the route matters as much as the result, and why that rule still
  binds anything that later claims to catch a defect.
- **The permutation null's block length `L` has two opposing jobs, and there is
  no correct value — only a declared one and a sweep** (D-0087). `L` preserves
  dependence *and* destroys the signal. Too long and the strategy's own horizon
  fits inside a block, so the null keeps the edge and the test is conservative
  to the point of uselessness; too short and the null loses autocorrelation the
  market really has, so an ordinary strategy looks extreme against a straw man.
  The spec (`stats`' module doc) says "block length ≳ strategy horizon to
  preserve autocorrelation structure", which reads as *preserve* while the test
  needs *destroy* — **that ambiguity is flagged here rather than resolved
  silently.** What this build does: state the null in one sentence (`returns are
  exchangeable at block scale L`), declare `L` before the run, and require a
  sweep over it — the same demand `ACCOUNT_EVAL_SPEC.md` §4.3 already makes of
  its own bootstrap. A single unswept `L` is a result with a parameter hidden
  inside it, and picking the `L` that makes a p-value small is the seed-shopping
  the rule below forbids.
- **The leak fixture uses seed 29 rather than the smoke config's 42**, and that
  is confound removal rather than fixture-fitting. Buy-and-hold is a *criterion*,
  and a random walk's drift across any particular set of test windows is
  arbitrary; on seed 42 it rises 4.5 % and kills the leak for a reason that has
  nothing to do with the leak. Seed 29's walk moves −0.04 % across its test
  windows, so what remains is the leak against the criteria. Chosen after
  measuring eighteen seeds, and chosen in the direction that makes the *gates'*
  job easiest.

  **The general rule, because this will look like seed-shopping to a future
  reader and the difference is a direction, not a degree:** fixture parameters
  may be selected to **isolate the phenomenon under test, provided the
  selection is documented and biases against the machinery, never for it**. A
  negative control's job is to document what the machinery *cannot* see;
  picking the seed where it accidentally could would overstate gate power,
  which is the same error as quoting the best combo in a grid. Choosing seed 29
  *removed* a confound that was helping the gates by accident — so the recorded
  failure is a floor on how bad the blind spot is, not a cherry-picked ceiling.
  A selection that moved the other way would be exactly the thing this project
  exists to catch, and the test is easy to apply: ask which side of the
  comparison the choice flatters.

---

## 10. Commands

### Exit codes — one contract, because these run unattended

Every command shares the same five, and the shape is what a **scheduler** needs
rather than what a shell prompt needs. `pull` and `funnel` are the two that
run on a timer, and they are the two that use all of it.

| code | meaning | who returns it |
|---|---|---|
| 0 | did the work; nothing needs a human | all |
| 2 | usage or config error — nothing ran | every command that takes arguments or a config |
| 3 | **refused on the money gate**: the quote exceeded `--max-cost-usd`, or `--execute` came without one (D-0024) | `pull` |
| 4 | ran, and **found something you must look at** — or failed partway | `verify`, `qa`, `layout-check`, `transcode`, `symbol-supplement`, and the data-unreadable path of every replay command |
| 5 | **completed, and the answer is one you must not read as success** | `pull`, `funnel` |

Code 5 is the one worth understanding, because the two commands that return it
mean different things by it and both mean "come back":

- **`pull` exits 5** when vendor jobs are still processing. The data is bought,
  journalled and downloadable for 30 days — but a cron that reads "still
  processing" as success never returns for it (D-0034). Re-running the
  identical command resumes and submits nothing twice.
- **`funnel` exits 5** when **every combo was killed** (D-0075). That is not a
  failure — most ideas must die, and cheaply, which is the entire point of the
  funnel. It is not success either: a scheduled sweep that reported exit 0 for
  "the whole grid is dead" would be indistinguishable from one reporting exit 0
  for "we have a candidate", and the difference is the only thing the run was
  for. Exactly the argument `qa`'s exit 4 makes about a 61 %-coverage archive.

So the honest cron line for a funnel sweep treats 0 and 5 as *both* normal and
tells them apart, rather than `|| true`-ing the difference away:

```bash
crucible funnel --config configs/idea.toml --out results
case $? in
  0) notify "survivors — read results/scorecard-*.html" ;;
  5) log "grid fully killed; nothing to review" ;;
  *) alert "funnel broke" ;;
esac
```

```bash
cargo test --workspace                          # everything
cargo clippy --all-targets -- -D warnings       # CI-equivalent lint
cargo fmt --all                                 # format
cargo run -p crucible-cli -- demo               # the vertical slice
cargo run -p crucible-cli -- demo --hash-only   # determinism hash (CI gate)
cargo run -p crucible-cli -- env                # what .env/env actually resolved to
cargo run -p crucible-cli -- verify             # re-hash the archive vs the manifest
cargo run -p crucible-cli -- layout-check      # is the tree the shape DATA_LAYOUT.md says?

# The replay path. `transcode` needs the feature (it decodes DBN); `backtest`
# does not. Curated data is disposable: `--force` rebuilds, deleting is safe.
# Spreads are excluded by default and the excluded records are counted in the
# report (D-0070); `--include-spreads` writes them.
#
# A curated contract is spelled with a FOUR-digit year (D-0072) — `ESH4` is a
# shorthand that still resolves while it names exactly one curated contract, and
# refuses when it names two (`GCZ4` is both Dec-2014 and Dec-2024 gold). The
# canonical form is written here so these commands do not depend on how much of
# the archive has been transcoded.
cargo run -p crucible-cli --features databento -- transcode
cargo run -p crucible-cli --features databento -- transcode --include-spreads
cargo run -p crucible-cli -- backtest --instrument ESH2024 --timeframe 1m \
  --start 2024-01-01 --end 2024-02-01 --fast 20 --slow 50

# The same run on a COARSER grain (D-0077). The archive stores 1s and 1m; 5m,
# 15m, 1h and 1d are aggregated on read, on the exchange's own sessions, so
# there is nothing to build first and `curated/bars/ESH2024/5m` never exists.
# A daily bar is a TRADING-day bar, opening at 17:00 CT the evening before.
# Every report names the grain and says whether it was stored or resampled.
cargo run -p crucible-cli -- backtest --instrument ESH2024 --timeframe 15m \
  --start 2024-01-01 --end 2024-02-01 --fast 20 --slow 50

# The same command over a CONTINUOUS series (D-0076). `--instrument` takes the
# §4 aliases `{root}.v.0` (volume roll) and `{root}.c.0` (calendar roll); each
# needs its roll table built first. `--adjustment back_adjust|none` names how
# the strategy's indicators see the stitch — it never reaches PnL, which uses
# the tradeable price of the then-front contract in both cases.
cargo run -p crucible-cli -- rolls --root ES --timeframe 1m --write
cargo run -p crucible-cli -- backtest --instrument ES.v.0 --timeframe 1m \
  --start 2010-06-06 --end 2026-07-28 --fast 20 --slow 50

# The calendar rule needs expiries, so it needs the `definition` schema — and
# `--features databento` to decode it. The volume rule above reads none and so
# needs neither (D-0090); all seven roots build both rules today.
cargo run -p crucible-cli --features databento -- \
  rolls --root ZN --timeframe 1m --calendar-days 8 --write

# The same run with protective levels, in ticks from each entry's fill price.
# Both flags are optional and either alone is legal; the header names the
# intrabar convention and the result counts the bars where it chose (D-0069).
cargo run -p crucible-cli -- backtest --instrument ESH2024 --timeframe 1m \
  --start 2024-01-01 --end 2024-02-01 --stop-ticks 8 --target-ticks 12

# The combo path: a strategy defined in TOML rather than in Rust. Expansion
# alone spends nothing and touches no archive; `--run` replays every combo on
# the config's declared data source.
cargo run -p crucible-cli -- combo --config configs/example-combo.toml
cargo run -p crucible-cli -- combo --config configs/combo-smoke.toml --run
cargo run -p crucible-cli -- combo --config configs/combo-smoke.toml \
  --run --hash-only                            # the grid determinism gate

# The same grid, cut into train/test folds by trading day (D-0062). Every
# number states the window it was computed on, so D-0061's warmup-prefix
# caveat does not apply to this output — it still applies to `combo`.
cargo run -p crucible-cli -- walk-forward --config configs/combo-smoke.toml
cargo run -p crucible-cli -- walk-forward --config configs/combo-smoke.toml \
  --hash-only                                  # the walk-forward determinism gate

# The funnel: the same grid, judged against the criteria its config declared
# BEFORE the run, with both mandatory controls and the 0/0.5/1/2-tick sweep.
# Unattended. Writes results/registry.jsonl (append-only, D-0074) and one
# self-contained scorecard per config. Exit 5 means every combo was killed,
# which on the null harness is the correct answer (D-0075).
cargo run -p crucible-cli -- funnel --config configs/combo-smoke.toml
cargo run -p crucible-cli -- funnel --config configs/combo-smoke.toml \
  --out results --hash-only                    # the funnel determinism gate

# Judge a config and stop: every refusal the funnel makes before it reads a bar,
# and none of the work after. Touches no archive, charges no trial, writes
# nothing. It is what `crucible-cli/tests/backlog_registration.rs` points at
# every embedded config block in `research/backlog/`, so a registration that
# declares a stage without that stage's section is caught the day it is written
# rather than the day someone tries to run it (D-0101).
cargo run -p crucible-cli -- funnel \
  --config configs/hypotheses/H-008-short-horizon-overreaction.toml --check-config

# The research memory is a text file. The graveyard is a query over it:
grep '"kind":"verdict"' results/registry.jsonl | grep '"verdict":"kill"'

# The acquisition path. `--features databento` is required (D-0025); without
# it `pull` exits 2 saying so. A pull is a DRY RUN by default and spends
# nothing — `--execute` needs `--max-cost-usd` alongside it (D-0024).
cargo clippy --all-targets --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m --symbols ES.FUT \
       --start 2024-01-01 --end 2024-02-01
```

Acquisition planning lives in `docs/DATA_PLAN.md`; the reasoning for the
subscription month is `docs/RUNBOOK_BLITZ.md`, and the copy-paste command list
for the day itself is `docs/BLITZ_CHECKLIST.md`.

---

## 11. Status snapshot

**M0 complete** (2026-07-24). **M3 in progress** — funnel + statistics.

*What is done lives in exactly one place:* the checkboxes in
`docs/MILESTONES.md`. This section deliberately does not restate them — two
copies of the same state means one is lying by next week, and §8 makes every
session read a stale snapshot first. Tick the checkbox in the commit that
does the work.

Where to start reading in M1: `crucible-data::catalog` (the archive/manifest
contract), then `crucible-data::ingest` (acquisition, and the rolling-window
deadlines that make the monthly pull time-sensitive by design), then
`crucible-data::curated` (the file format the engine actually replays) and
`crucible-data::transcode` (how raw becomes curated). `calendar` defines when
the exchange is open, `continuous` stitches contracts, and `qa` is the module
that reads the others back and asks whether the archive is any good — start
there if you want to know what the data actually looks like. Every module is
implemented. `docs/DECISIONS.md` is the source of truth for why anything looks
the way it does.

Where to start reading in M3: `crucible-funnel::funnel` is the orchestration —
claim, replay, screen, sweep, control, judge, record — and reading it top to
bottom is the fastest way to see what a verdict is made of. Then
`crucible-funnel::stages` for the criteria and why `Graduate` is unreachable,
`crucible-funnel::registry` for the five contract rules and why the backend is
JSONL, and `crucible-strategies::controls` for the two benchmarks and the
planted leak. The **S0 predictor seam landed 2026-07-31** — D-0082 the
measurement (`crucible-funnel::s0`: the forward-return join, the information
coefficient, quantile buckets, session block bootstrap) and D-0085 the caller
(`ComboScorer`, the `[s0]` config block, the registry row, and the gate ahead of
S1). **Block A of `docs/plans/m3-full.md` landed 2026-07-31**: the
block-permutation null (D-0087) and the truncation-invariance harness (D-0088),
both with converse controls and pinned hashes, and `planted_leak.rs` flipped
from `Iterate` to `Kill` on the first of them. What M3 still owes is the rest of
`crucible-funnel::stats` — **deflated Sharpe and PBO/CSCV** — plus registry
pooling and the account evaluator. The plan is `docs/plans/m3-full.md`, blocks
B through E.

Known open questions (decide when reached, log when decided): margin
modeling (M2); multi-instrument portfolio accounting (post-M4); Welford vs
periodic-rebase for indicator numerics (M2).
