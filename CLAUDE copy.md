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
crucible-data        network+filesystem for market data. async ONLY in bin targets.
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
- `data` → `core`. Only crate touching network/filesystem for market data;
  tokio confined to its bin targets; library code stays sync.
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
- **Orders**: positive `qty` magnitude + `Side`; **positions**: signed
  (long > 0). `Actions::target_position` is the idiomatic way to trade.
- **Timestamps**: UTC nanoseconds (`Ts`) everywhere. Timezone/session logic
  exists only in `crucible-data::calendar`.
- **Config field names**: `snake_case`, units suffixed like code
  (`fee_per_contract_usd`, `train_months`).

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
| `blake3` | funnel, data | config identity, archive checksums |
| `databento`, `dbn` | data only | acquisition + DBN decoding |
| `tokio` | data **bin targets** only | Databento client is async |
| `polars` | data only | Parquet transcode/QA. NEVER in engine. |
| `chrono` + `chrono-tz` | data::calendar only | the one timezone-aware module |
| `rand_chacha` | where seeded randomness is needed | resampling/permutation |
| `clap` | cli | args (when >1 real command) |
| `criterion` | dev | benchmarks |
| `proptest` | dev | property tests (accounting invariants) |
| `anyhow` | cli/bins | error context |
| `thiserror` | libs | error enums |

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

---

## 8. Session workflow (Claude Code)

Start of session: read this file, `docs/DECISIONS.md`, the active milestone
in `docs/MILESTONES.md`, and the module docs you'll touch. State which
milestone item you're executing.

While working:
- Stay within the active milestone unless the human redirects scope.
- Semantic/architectural choices → `docs/DECISIONS.md` entry in the same
  commit (template in that file's header).
- Keep module-doc specs in sync with what you implement; the spec comment is
  done only when the module is.
- Definition of done: fmt + clippy + tests green locally, docs updated,
  decision log updated if applicable, `demo` still behaves (run it).

Hard NEVERs (in addition to §2):
- Never commit market data, `results/`, or anything matching `.gitignore`'s
  data patterns. Never hardcode or log `DATABENTO_API_KEY` (env only).
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

---

## 10. Commands

```bash
cargo test --workspace                          # everything
cargo clippy --all-targets -- -D warnings       # CI-equivalent lint
cargo fmt --all                                 # format
cargo run -p crucible-cli -- demo               # the vertical slice
cargo run -p crucible-cli -- demo --hash-only   # determinism hash (CI gate)
```

---

## 11. Status snapshot

**M0 complete** (2026-07-24): workspace + core types with availability
invariant; deterministic engine (fills→mark→decide loop, integer
accounting); `free_fills`/`spread_cross`; SMA/EMA/Bollinger + `SmaCross`;
seeded synthetic feeds; golden + determinism + hand-computed indicator
tests; CI with determinism gate; decision log D-0001…D-0013; module-doc
specs for all of M1/M3.

**Next: M1** — data foundation. Entry point: `crucible-data::ingest` and
`::catalog` module docs, then `docs/MILESTONES.md` M1 checklist. Note the
rolling-window archival deadlines described in `ingest` — the monthly pull
is time-sensitive by design.

Known open questions (decide when reached, log when decided): margin
modeling (M2); multi-instrument portfolio accounting (post-M4); Welford vs
periodic-rebase for indicator numerics (M2).
