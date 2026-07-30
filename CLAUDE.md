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

**Current milestone:** M1 (data foundation) — see `docs/MILESTONES.md`.

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
| `rayon` | funnel only | run-level parallelism |
| `duckdb` | funnel only | registry/results store |
| `serde`, `toml`, `serde_json` | funnel, data, cli | configs, manifest |
| `blake3` | funnel, data, cli | config identity, archive checksums. `cli` because it is where a combo config is loaded today (D-0060); `crucible-strategies` renders the canonical form and never hashes it |
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

Hard NEVERs (in addition to §2):
- Never commit market data, `results/`, or anything matching `.gitignore`'s
  data patterns — including `.env`, which is where the API key now lives
  locally (D-0022). Never hardcode or log `DATABENTO_API_KEY`: it is read
  from the process environment, at the last moment, by bin targets only.
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
  (`curated/bars/ESH4/1m/2024-01.parquet`), and `curated/` is not in the
  manifest: one raw file fans out to one curated file per instrument, each
  naming exactly one source blake3 (D-0036). A year-named file would have to
  be merged, and merging is where duplication hides.
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
- **The bundled CME calendar has no 15:15–15:30 CT halt**, though CME's own
  E-mini contract-spec page lists one. Our archive falsifies it for the
  current era: ESH4 January 2024 has 315 bars with nonzero volume in exactly
  that window (D-0040). The archive is evidence; the spec page is a claim.
- **Most CME "holidays" are early closes at 12:00 CT, not closures**, and the
  evening before still opens. CME's trading-hours *landing page* says
  "Globex closed" and its per-holiday grids disagree; the grids win, and MLK
  2024-01-15 in our archive agrees with them. Encoding closures would delete a
  real overnight session plus a real morning from 16 years of backtests.
- **The calendar answers about 2013 even though it says `valid_from`
  2015-09-21**: the functions are total by design — there is nowhere to put a
  `Result` inside replay. `qa` and `backtest` warn when a span starts earlier;
  the two older session eras are documented in the table, not modelled.
- **`crucible qa` exits 4 when it finds something**, like `verify`. A
  scheduled job that reads "coverage 61 %" as success is worse than no job.
- **`AdjustedPrice` cannot be converted to `Price`, and that is the feature**
  (D-0042). Back-adjusted levels are for signals; PnL uses the tradeable price
  of the then-front contract. Adding a `From` impl reintroduces the classic
  silent-corruption bug this whole type exists to prevent.
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

---

## 10. Commands

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
cargo run -p crucible-cli --features databento -- transcode
cargo run -p crucible-cli --features databento -- transcode --include-spreads
cargo run -p crucible-cli -- backtest --instrument ESH4 --timeframe 1m \
  --start 2024-01-01 --end 2024-02-01 --fast 20 --slow 50

# The same run with protective levels, in ticks from each entry's fill price.
# Both flags are optional and either alone is legal; the header names the
# intrabar convention and the result counts the bars where it chose (D-0069).
cargo run -p crucible-cli -- backtest --instrument ESH4 --timeframe 1m \
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

**M0 complete** (2026-07-24). **M1 in progress** — data foundation.

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

Known open questions (decide when reached, log when decided): margin
modeling (M2); multi-instrument portfolio accounting (post-M4); Welford vs
periodic-rebase for indicator numerics (M2).
