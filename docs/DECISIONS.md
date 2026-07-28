# Decision log

Append-only. One line of context beats an hour of re-litigating. Format:
`D-NNNN (date) — decision. Why. (supersedes D-XXXX if applicable)`

Claude Code sessions: when you make or change an architectural/semantic
choice, append an entry in the same commit. If a decision here blocks you,
propose a superseding entry — don't silently diverge.

- **D-0001** (2026-07-24) — Rust workspace, edition 2024, six crates
  (core/data/engine/strategies/funnel/cli). Why: enforceable dependency
  boundaries; core stays dependency-free.
- **D-0002** (2026-07-24) — Fixed-point everywhere in accounting: prices are
  i64 nanopoints (Databento-native 1e-9), money is i64 nano-USD. `f64` is
  allowed only in indicator/statistics space. Why: lossless ingest, no float
  drift in PnL, bit-exact reproducibility.
- **D-0003** (2026-07-24) — Bars are stamped with interval OPEN (`ts_open`,
  Databento convention); ordering/visibility use `avail_ts = ts_open + tf`.
  Why: replaying on `ts_open` reveals the future one bar early — the exact
  bias this project exists to kill.
- **D-0004** (2026-07-24) — `overflow-checks = true` in ALL profiles
  including release. Why: silently wrapped money is worse than a crash;
  measured cost is negligible for this workload.
- **D-0005** (2026-07-24) — Parallelism at run granularity only, in
  `crucible-funnel` (rayon). The engine is single-threaded, sync, and
  clock-free; async/tokio confined to `crucible-data` bin targets. Why:
  determinism, and grid search saturates cores without intra-run threads.
- **D-0006** (2026-07-24) — Fill models are named, explicit assumptions;
  `free_fills` exists but is sanctioned only for funnel stages S0–S1.
  Why: optimism must be visible and greppable, and "can't work even
  cost-free" is the cheapest kill available.
- **D-0007** (2026-07-24) — Indicators are hand-written streaming
  implementations (no TA crate). Why: O(1) updates for grid throughput,
  hand-verifiable numerics, no dependency on unmaintained crates.
- **D-0008** (2026-07-24) — EMA seeds with the SMA of the first `period`
  closes, then standard recursion (α = 2/(period+1)). Why: common
  convention; changing it silently shifts every EMA-based result.
- **D-0009** (2026-07-24) — Bollinger uses population variance (ddof = 0).
  Why: matches charting-package convention; pinned to keep results stable.
- **D-0010** (2026-07-24) — v1 descope: no news/earnings scraping (macro
  events enter as static release-calendar CSVs), no GPU, no options/
  ThetaData, SSRN-style cross-sectional strategies deferred post-M4. Why:
  weeks-to-thesis and a finishable semester scope.
- **D-0011** (2026-07-24) — Synthetic feeds use an inlined SplitMix64 over
  integer ticks with a fixed base timestamp. Why: bit-identical across
  platforms and Rust versions, no `rand` dependency, prices always
  tick-aligned.
- **D-0012** (2026-07-24) — Config identity = blake3 over canonicalized
  config (sorted keys), not file bytes and never `DefaultHasher`. Why:
  comments/whitespace must not change identity; `DefaultHasher` is not
  stable across Rust releases and is useless in a persistent registry.
- **D-0013** (2026-07-24) — Persisted-result invariant: every stored run
  carries config hash, git sha, and data manifest ids. Why: a number you
  can't reproduce is a rumor.
- **D-0014** (2026-07-24) — Manifest checksums are blake3 (lowercase hex) in
  a field named `file_blake3`, and the checksum IS the "data manifest id"
  required by D-0013 — no separate id field. Why: one identity that literally
  names the bytes, nothing to keep in sync; blake3 already blessed for
  archive checksums (the spec's `file_sha256` said "blake3 acceptable" — the
  field name now tells the truth).
- **D-0015** (2026-07-24) — Manifest ranges are half-open `[start_ts,
  end_ts)` UTC nanoseconds; the spec's `acquired_at` is renamed `acquired_ts`
  (§4 naming); `acquired_ts` is caller-supplied, never read from a clock in
  library code. Why: half-open ranges merge/subtract without off-by-one
  ambiguity and adjacent months concatenate seamlessly; §2.2 bans clocks.
- **D-0016** (2026-07-24) — Manifest records carry a required
  `schema_version` and deserialize with `deny_unknown_fields`; a malformed
  line, unknown field, or unknown version is a hard error naming the line.
  Why: mirrors the config policy (§5.5) — a silently tolerated manifest typo
  corrupts reproducibility exactly like a config typo corrupts research.
- **D-0017** (2026-07-24) — The catalog is the checksum gatekeeper:
  `Catalog::append` hashes the file on disk itself (callers cannot supply
  checksum/size); `file_path` must be relative, forward-slash, under `raw/`;
  duplicate paths are rejected on append and on load; `verify` re-hashes and
  reports every finding rather than failing fast. Why: a manifest whose
  hashes might not describe the bytes on disk is worthless; relative
  forward-slash paths keep the manifest byte-portable across OSes; an audit
  must show the extent of damage.
- **D-0018** (2026-07-24) — Catalog appends take an exclusive OS file lock
  and reload the manifest before duplicate-path validation; every non-empty
  manifest must end in `\n`. Why: stale catalog handles must not bypass raw
  immutability, and accepting an unterminated but otherwise-valid JSON tail
  would make the next append concatenate two records into one corrupt line.
- **D-0019** (2026-07-24) — Manifest framing is LF-only (any `\r` is a hard
  error naming the line), and `file_path` bans whitespace/control characters
  like dataset/schema/symbols already do. First-append durability limitation
  (no parent-dir fsync; a crash in that window can lose the fresh one-line
  manifest on some POSIX filesystems) is documented, not worked around. Why:
  a CRLF hand edit parses identically but silently breaks the byte-identical
  portability guarantee; path hygiene should be uniform; dir-fsync is
  platform-specific complexity for a loud, recoverable failure.
- **D-0020** (2026-07-24) — `Catalog::coverage` validates its
  `CoverageRequest` with the same rules as an append and therefore returns
  `Result`. Why: coverage is the input to a *paid* download — a symbol with a
  stray space matches no record, so an unvalidated request answers "you own
  none of it", funds the download, and only then gets refused by `append`;
  an empty symbols list returns an empty map that reads as "nothing to do".
- **D-0021** (2026-07-24) — `dataset`, `schema`, `symbols`, and `file_path`
  must be ASCII; non-ASCII is a hard error on append and on load. Why:
  Databento symbology is ASCII, and macOS normalizes filenames to NFD while
  Linux stores the bytes given, so a non-ASCII path can be byte-different per
  host while looking identical — the exact portability guarantee D-0019
  pinned down.
- **D-0022** (2026-07-24) — Bin targets may load a gitignored `.env` (repo
  root) into the process environment at startup via `dotenvy` (blessed in §6,
  bins only); real environment variables always win, a missing file is fine,
  and a malformed one is a hard exit. Library crates still never read the
  environment. Relaxes `ingest`'s original "never a file" wording, which was
  aimed at secrets living in *configs* — the invariant that survives is that
  the key never enters a config struct, CLI argument, manifest record, or log
  line. Why: one file beats re-exporting two variables in every new shell,
  and a half-loaded secrets file must fail as loudly as a config typo.
  Windows note: a backslash starts an escape sequence in dotenv syntax, so
  `E:/crucible-data` or `'E:\crucible-data'`, never bare `E:\crucible-data`
  (pinned by `crucible-cli/tests/env_file.rs`).
- **D-0023** (2026-07-24) — The archive is acquired inside **one Databento
  Standard month** ($199), not incrementally metered, and `mbp-10` is dropped
  from the recurring monthly job. Quotes verified that day against
  `GLBX.MDP3`: the bootstrap list is ~53 GiB / ~$1,901 at pay-as-you-go
  rates, and Standard *includes* unlimited historical inside its windows
  (L0 16y, L1 12mo, L2/L3 1mo) rather than metering on top, so one
  subscription month buys the lot; the recurring job afterwards is ~$63/month
  metered, below the $199 subscription, so we unsubscribe until M4 needs the
  live feed. Why (cost shape): unit prices are per GB and *aggregates are
  dearest* — `ohlcv-1s` and `ohlcv-1m` are both $70/GB — so 16y of 1s bars is
  $1,377, more than every L1/L2/L3 tier combined. The expensive thing is
  one-second bars, not the order book, which is the opposite of what tier
  names suggest and the reason this is written down. Why (`mbp-10`): `mbo` is
  the strictly richer L3 book and is what M4's queue model calibrates
  against, at 16.5 GiB/month against `mbp-10`'s 86.5 — five times the disk
  for a derivable view, and ~1 TB/year against a 1.4 TB drive. Consequence:
  the subscription is a 30-day clock against a large batch run, so `pull`
  must be finished and unattended-capable *before* it starts, and no data is
  bought until then. Prices drift; re-derive with `pull --dry-run` rather
  than trusting the table in `ingest`'s module docs.
- **D-0024** (2026-07-24) — `pull` defaults to a **dry run**: it quotes
  `get_cost` / `get_billable_size` per job, coverage-subtracted, and exits
  without spending; spending needs an explicit opt-in flag and honours a
  `--max-cost-usd` cap that hard-errors rather than proceeding. Why: the
  original `ingest` spec had no pre-purchase cost gate at all, while the
  archive's only cost control (`Catalog::coverage`, D-0020) prevents paying
  *twice* and says nothing about the price of paying *once*. It also makes
  the quote path the free, fully testable part of `pull` — coverage
  subtraction can be proven end to end for $0.00 before it is trusted to
  guard four figures of entitlement.
- **D-0025** (2026-07-24) — `crucible-data`'s public API is sync. The async
  Databento client is reached through the sync `ingest::BatchProvider` trait,
  whose only network implementation lives in `ingest::databento` behind the
  non-default `databento` cargo feature and owns a private **current-thread**
  tokio runtime; `async fn`/`.await` appear in no other file. The trait names
  no `databento::` type. *Supersedes only the "tokio confined to bin targets"
  clause of D-0005 and §3; D-0005's parallelism decision stands.* Why: the
  original rule was unimplementable as written — `crucible-data` has no bin
  targets, Cargo dependencies are per-crate rather than per-target (so a bin
  would pull tokio into the library's graph anyway), and a second binary
  would break M1's "one command" acceptance criterion. The trait is also what
  makes the cost gate testable at all: a scripted fake with a call log turns
  "a dry run submits nothing" into an assertion instead of a hope.
  Current-thread, not multi-thread, so `crucible-data` spawns no worker pool
  (§3 reserves thread-spawning for `funnel`); the honest caveat is that any
  HTTP client resolves DNS on a blocking thread, which is not
  result-affecting parallelism but should not be discovered by surprise.
- **D-0026** (2026-07-24) — Archive windows are named `{yyyy-mm}` when they
  cover exactly one calendar month and `{yyyy-mm-dd}--{yyyy-mm-dd}`
  otherwise, and requested ranges must be UTC-midnight aligned on both ends.
  Why: with a month-only template, a cheap one-day validation pull of
  2024-01-02 and a later full-month pull of January 2024 compute the *same*
  `file_path`, so the second is rejected by `Catalog::append` as a duplicate
  (raw is immutable, D-0017) — **after** the bytes have been bought. Partial
  windows are not an edge case: they are the norm at dataset-range boundaries
  and at the "up to yesterday" end of the monthly job. Coverage is unaffected
  because it subtracts ranges, not paths. Day alignment is required because a
  sub-day window has no unambiguous name.
- **D-0027** (2026-07-24) — Vendor-quoted costs convert to `NanoUsd` at the
  API boundary, rounding **up**, and `--max-cost-usd` parses decimal text to
  nanodollars without ever constructing an `f64`; the budget comparison is
  integer and is `quoted <= cap`. Why: §2.3's ban on `f64` in accounting
  covers money leaving the machine, not just PnL. `(1.2597_f64 * 1e9).ceil()`
  is 1_259_700_001 — a naive conversion is wrong in a way that looks right —
  and `"0.20"` through `f64` is not 0.2, so a cap could refuse a quote that
  exactly equals it. Rounding up means an understated quote can never
  authorise a charge nobody consented to, and a sub-nanodollar quote reads as
  one nanodollar rather than as free. Making the comparison `<=` is what lets
  the recurring archival job run at `--max-cost-usd 0.00`: it proceeds while
  the subscription entitles the data at zero and refuses the month the
  entitlement lapses, instead of quietly billing for it.
- **D-0028** (2026-07-26) — **One batch job = one window = one archive file =
  one manifest row.** Submissions pin `split_duration=none`, `split_size=None`,
  `split_symbols=false`; a delivery that is not exactly one `.dbn.zst` is a
  hard error. `PullRequest.window_span` chooses how wide a window is:
  `Month` (the recurring cron) or `Whole` (one window per contiguous coverage
  gap — the bootstrap backfill). `BatchProvider::dataset_range` also gains a
  `schema` argument, because availability is per schema, not per dataset.
  Why: verified against the vendor docs the same day — batch jobs have **no
  documented size limit**, are processed **one at a time in a FIFO queue** (so
  sharding buys no parallelism whatsoever), are capped at **20 submissions per
  minute per IP**, and have no idempotency key, no cancel, and no refund. At
  month granularity the 16-year backfill is 7 parents × 4 schemas × 193 months
  = 5,404 separate purchases; `Whole` makes it ~28. Fewer jobs is fewer chances
  to buy something twice. Why the split parameters: the vendor default is
  `split_duration=day`, which would deliver ~31 files for one month and ~5,800
  for one 16-year job, and the archive's path template names one file per
  window. Rejected alternative: one large job with `split_duration=month`,
  mapping delivered filenames back to windows — a month containing no data
  yields no file, leaving a coverage hole that is re-bought forever. Why
  per-schema ranges: on `GLBX.MDP3` everything starts 2010-06-06 except `mbo`,
  which starts 2017-05-21; a dataset-wide answer leaves a 16-year `mbo`
  request unclipped and quotes seven years of data that does not exist.
- **D-0029** (2026-07-26) — A **job journal** (`jobs.jsonl` at the data-dir
  root, framed exactly like the manifest — LF-only, fsynced, locked,
  `deny_unknown_fields`, `schema_version`) records an `Intended` entry
  **before** any submission, and every run reconciles the plan against
  `batch.list_jobs` before submitting anything, even on a first attempt.
  Identity is `intent_id` = blake3 over (dataset, schema, key, stype_in,
  start_ts, end_ts) — deterministic and clock-free, so re-running the same
  command recognises its own previous attempt. Matching is on civil dates
  rather than raw nanoseconds, and **anything ambiguous refuses**: two
  candidate jobs, or one whose symbology cannot be confirmed, stops the run
  rather than guessing. Why: a submission is the only irreversible act in the
  crate, and the vendor offers no idempotency key, no cancel, and no refund —
  so a resubmit is simply a second purchase. What it does offer is a 30-day
  download window measured *from submission*, which makes a job whose id we
  still know always recoverable for free. Reconciling even without a journal
  entry is what protects a run whose journal was deleted or never fsynced.
  Adopting the wrong job archives the wrong bytes; submitting anyway buys the
  window twice; refusing costs a re-run.
- **D-0030** (2026-07-26) — `execute` holds an exclusive OS lock on
  `pull.lock` for its whole duration (a second `pull` exits 2), and the
  "destination already exists" refusal is **skipped for a resuming intent**
  whose journal shows `Downloaded`: there the destination is re-hashed against
  the vendor digest and the run proceeds straight to the manifest append.
  Why the lock: the fold→reconcile→intend→submit sequence spans network calls,
  so two pulls started seconds apart would each read an identical journal, each
  find nothing submitted vendor-side, and each submit — a race the journal's
  own append lock cannot prevent, because the race is between the submissions.
  Why the exception: the crash window between renaming a verified payload into
  `raw/` and recording it in the manifest is real, and the blanket refusal —
  correct for a fresh intent, where `fs::rename` would silently replace an
  archived file on Windows — would otherwise strand a paid, correctly-placed
  file behind an error written for a different situation.
- **D-0031** (2026-07-26) — A delivered file must match `BatchFileDesc.size`
  exactly **and** hash to the vendor's published SHA-256 before it is renamed
  into `raw/`. `sha2` is blessed as an optional dependency enabled by the
  `databento` feature; `time` likewise, because the vendor API spells its
  timestamps `time::OffsetDateTime` and the adapter must name the type. DBN
  decoding comes from the **`databento::dbn` re-export**, not a separately
  pinned `dbn`. Why: `Catalog::append` hashes whatever is on disk (D-0017), so
  an unverified truncated download would be blake3'd, recorded, and thereafter
  certified by the manifest as the real January slice — the manifest would be
  lying with full ceremony. Why the re-export: a standalone `dbn` version can
  drift from the client that produced the file, which is a decoding bug with
  no upside.
- **D-0032** (2026-07-26) — The execute path reaches the outside world through
  three injectable seams: `BatchProvider` (the vendor), `DeliveryInspector`
  (checksum and DBN symbols), and `Clock` (time and sleeping). **No
  `SystemClock` exists in `crucible-data`** — the only implementation that
  touches the OS lives in the `crucible-cli` bin target. Why: D-0015 pinned
  `acquired_ts` as caller-supplied so library code never reads a clock, and
  §2.2 bans `SystemTime::now` from result-affecting code; keeping the impl in
  the bin honours both. The practical payoff is larger than the principle: with
  all three faked, the whole state machine — poll loops, timeouts, checksum
  refusals, every crash-resume path — is tested offline in microseconds, in a
  default build with no vendor dependencies at all.
- **D-0033** (2026-07-26) — `ManifestRecord.symbols` is the requested key
  **plus** every raw symbol observed in the delivered DBN metadata, sorted and
  deduped with the key first. Symbols the catalog would reject are **dropped
  and reported loudly**, not refused. Why: recording only the key makes
  `coverage("ESH4")` report an owned range as missing and re-buy it forever.
  Why dropping rather than refusing: omitting a symbol only ever makes coverage
  *understate* what we own — the cost is a re-purchase, never a silent gap —
  whereas refusing the append would strand a file that has already been paid
  for and correctly placed. Expect fat manifest lines: a 16-year `ES.FUT` pull
  resolves to every outright and calendar spread in the window, so a record may
  carry thousands of symbols. That is correct; coverage truthfulness outranks
  tidiness, and filtering to outrights would reintroduce the re-buy bug.
- **D-0034** (2026-07-26) — `clap` (derive) lands in `crucible-cli`; `pull` is
  a dry run by default with `--dry-run` as an explicit no-op alias; exit codes
  are **0** done, **2** usage/config, **3** refused to spend, **4** provider or
  filesystem failure, **5** in flight and resumable. The `databento` feature is
  non-default in both `crucible-data` and `crucible-cli`, and CI gains an
  `--all-features` clippy + test pass. Why the alias: D-0023 says
  `pull --dry-run` while D-0024 makes dry run the default; accepting the flag
  reconciles them without a superseding entry. Why 5 is not 0: a cron that
  reads "still processing" as success never comes back for data it paid for.
  Why opt-in: default builds and the default CI gates stay free of the async
  client's dependency graph (D-0025), and `cargo check` stays instant — but a
  feature nothing compiles is a feature that rots, hence the second CI pass.
- **D-0035** (2026-07-26) — Idempotent vendor calls retry on transport
  failure (bounded, with backoff); **`submit` never does**. A 429 is retried
  everywhere, including on `submit`. Why: the first live pull failed on the
  first `get_job_details` with "connection closed before message completed" —
  a stale pooled connection, and a direct consequence of D-0025's
  current-thread runtime, since between `block_on` calls nothing drives
  reqwest's background tasks and the poll loop leaves 15-second gaps for the
  server to close a socket the pool then hands out anyway. Reading a job's
  state costs nothing to repeat. A dropped connection on a *submission* is
  ambiguous — the server may have accepted the job — so retrying it is exactly
  the double purchase this milestone is built to prevent; it surfaces instead,
  and the next run's reconciliation (D-0029) resolves it for free. A 429 is
  safe everywhere because throttling means the request was rejected outright.
  Rejected alternative: disabling connection pooling via the SDK's
  `http_client_builder`, which needs `reqwest` as a directly pinned dependency
  — the version skew D-0031 avoids for `dbn`.
- **D-0036** (2026-07-28) — The curated store is
  `curated/bars/{instrument}/{tf}/{source_window_stem}.parquet`, six `INT64`
  columns (`ts_open`, `open`, `high`, `low`, `close`, `volume`), with
  instrument, timeframe, provenance, row count, and both version numbers in
  Parquet **file key-value metadata**. `avail_ts` is not stored; the
  instrument is percent-encoded into its path component. *Supersedes the
  `curated/bars/{symbol}/{tf}/{yyyy}.parquet` sketch in `catalog`'s layout
  comment.* Why one file per **source window** rather than per year: a
  year-named file forces a read-modify-write merge whenever two raw windows
  land in the same year — the only place in the codebase curated data would
  ever be merged, and merging is where silent duplication lives. One raw file
  fans out to one curated file per instrument, each recording exactly one
  `source_file_blake3` — the manifest id (D-0014) — so a result can name the
  precise bytes it read (D-0013) with no second index to drift, and
  `rm -rf curated/` stays safe. Why metadata rather than a sidecar or a
  manifest row: a footer cannot be separated from the bytes it describes, and
  `manifest.jsonl` records *acquisitions*, which curated files are not. Why
  not store `avail_ts`: it is `ts_open + tf` computed by `Bar::avail_ts`
  (§2.1), and a persisted ordering key is one that can come to disagree with
  its source. Why percent-encoding: `SYN:RW` is a legal instrument and an
  illegal Windows filename, and mapping `:` to `_` would file `SYN:RW` and
  `SYN_RW` together. Why `volume` is signed with a `try_from` at each
  boundary: an unsigned logical type buys nothing at futures volumes and
  costs a reinterpreting cast. **Two versions, failing differently:**
  `curated_schema_version` describes the file and a mismatch is a hard
  refusal (this build cannot know what the bytes mean);
  `transcoder_version` describes DBN→`Bar` semantics, and a mismatch warns and
  is printed beside results rather than blocking them — a cosmetic bump must
  not invalidate a 50 GB archive, but nor may it be silent.
- **D-0037** (2026-07-28) — Curated Parquet uses the **`parquet` crate
  (arrow-rs)** with `default-features = false, features = ["zstd"]` and its
  low-level typed-column API, *not* the `polars` that §6 pre-blessed. §6 is
  amended: `polars` stays listed, narrowed to the M1 data-QA report, and is
  banned from the feed path. Why: `polars-core` depends on `rayon`
  non-optionally, so adopting it would start a work-stealing threadpool inside
  `crucible-data` — and §3 reserves thread-spawning for `crucible-funnel`
  alone, a rule D-0025 restated when it chose a *current-thread* tokio runtime
  for exactly this reason. The secondary reasons all point the same way: the
  low-level API pins the physical column types by construction rather than by
  inference, which is the entire point of an integer-only data path (§2.3);
  key-value metadata is first-class in both directions; and the dependency is
  ~12 transitive crates against polars' ~200, on a manifest whose stated value
  is that `cargo check` stays instant. Cost of being wrong: the fallback is
  adding the crate's `arrow` feature (+5 crates, still no rayon and no async),
  a one-line change.
- **D-0038** (2026-07-28) — `crucible backtest` is the M1 exit artifact and is
  deliberately thin: one instrument, one strategy (`SmaCross`), one fill model
  (`spread_cross`), no grid, no folds, no benchmark, no trial count. Contract
  and cost arguments (`--tick-points`, `--point-value-usd`,
  `--initial-cash-usd`, `--fee-per-contract-usd`) parse decimal **text to
  integers without ever building an `f64`** — `Price::from_points_str` is
  added to `crucible-core` for the price-scale half, on D-0027's argument: a
  tick size one nanopoint off snaps every fill in a run onto the wrong grid,
  quietly and only on some instruments. The cost flags exist because §2.4
  requires execution assumptions to be named and visible, not because the
  command is meant to be configurable; they default to today's hand-set values
  and are echoed in the output header. `bars_per_year` defaults to a value
  **measured from the loaded sample** rather than the demo's 347,760 constant:
  real `ohlcv` data has no bar for an interval that did not trade, so the
  constant overstates the bar count, which overstates the annualization
  factor, which flatters Sharpe. `calendar` v1 takes ownership of it.
