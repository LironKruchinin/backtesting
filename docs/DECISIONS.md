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
- **D-0039** (2026-07-28) — **Session calendar v1** is a TOML table compiled in
  with `include_str!`, holidays expressed as *rules* rather than dates, and
  `chrono`/`chrono-tz` confined to `crucible-data::calendar`. `chrono` is requested
  **without its `clock` feature** to say that the calendar must never read a
  clock — though that is intent, not a guarantee, because `parquet` enables the
  feature through Cargo's unification; the enforcement is D-0048's clippy gate. Why compiled in rather
  than read at runtime: a calendar is consulted from inside replay and
  annualization, and a file read there would let the same config mean two
  things on two machines. Why rules: a dated holiday list is correct until the
  year it runs out and then silently answers "open" for every future Christmas.
  Every rule carries a `source` URL and construction refuses a table that cites
  nothing. A trading day `D` runs `open_local` on `D−1` to `close_local` on `D`;
  a `Closed` holiday removes `D` *and* the evening before, because that evening
  exists only to open `D`. Local times in 01:00–03:00 are refused at load, which
  is what makes the runtime local→UTC conversion infallible — and it costs
  nothing, because US DST transitions happen 02:00 Sunday, inside the
  Friday-to-Sunday closure, so **no Globex session ever spans one**.
  `bars_per_year(tf)` is open-seconds-per-year ÷ tf, precomputed over a fixed
  reference span so it is O(1) and identical on every machine; `D1` counts
  **trading days** instead, because a daily bar exists once per session however
  long the session is. In `backtest` the precedence is explicit
  `--bars-per-year` > calendar > D-0038's sample measurement, and when the
  calendar and the sample disagree by more than 1 % **both are printed** — they
  answer different questions (intervals a year *contains* vs intervals that
  actually *traded*) and neither is wrong. On ESH4 January 2024 that is 354,319
  vs 366,766 (+3.5 %). *Two things the table deliberately does not do:* it
  describes only the current session era (`valid_from = 2015-09-21`; CME moved
  the equity-index close on 2012-11-19 and again on 2015-09-21, and `qa` and
  `backtest` warn when a span starts earlier), and it does **not** encode the
  15:15–15:30 CT halt that CME's own contract-spec page lists — see D-0040.
- **D-0040** (2026-07-28) — **The data-QA report is plain Rust, not `polars`**,
  and §6 is amended again: `polars` leaves the blessed set rather than being
  narrowed further. Why: every check is one ordered pass over a bar series
  `ParquetBarFeed` already decodes, and `polars-core` depends on `rayon`
  non-optionally, so adopting it would start a work-stealing threadpool inside
  `crucible-data` — the exact thing §3 reserves for `crucible-funnel` and that
  D-0037 refused for the feed path. A DataFrame buys nothing a `for` loop does
  not here. *The report's most valuable check runs backwards:* bars whose
  `ts_open` falls when the calendar says the exchange was closed. Real data is
  the ground truth, so those indict the **calendar**, not the archive — and on
  its first run against ESH4 January 2024 it found 315 of them: 15 minutes on
  each of 21 trading days, all carrying nonzero volume, all inside the
  15:15–15:30 CT window CME's contract-spec page calls a trading halt. The halt
  came out of the table; coverage went 99.209 % → 99.957 % and out-of-session
  bars 315 → 0. The same run confirmed the other half of the model: MLK Day
  2024-01-15 stops at 12:00 CT exactly as CME's per-holiday grids say, which is
  why the "holidays" are encoded as early halts rather than as the closures
  CME's own trading-hours *landing page* claims. Findings exit 4, like `verify`:
  a scheduled job that reads "coverage 61 %" as success is worse than no job.
- **D-0041** (2026-07-28) — **The roll instant is the availability of the
  deciding session, and the new contract is front only for bars whose
  `avail_ts` is strictly greater.** `roll_ts = max(front.avail_ts(D),
  next.avail_ts(D))` for the session `D` the rule fired on. Why: a roll decided
  from day `D`'s volume cannot take effect before day `D`'s data is available,
  and the bars whose availability *is* `roll_ts` are the bars the decision was
  made from — consuming them is one bar of lookahead in the place it hides best,
  because a contract switch is invisible on a chart of an adjusted series. The
  strict comparison is deliberately the same one `replay.rs` step 1 makes when
  filling orders, so the codebase has one rule, not two. Consequence: volume
  buckets key on **`avail_ts`**, never `ts_open` — a bucket key is a join key
  and §2.1 forbids joining on event time. *Amended the same day, after review:*
  the bucket is the **calendar's trading day** where a calendar claims the root,
  not the UTC civil day. 00:00 UTC falls in the middle of a Globex session, so a
  five-session CME week produces six UTC-day buckets and the extra one holds
  about an hour of Sunday-evening trade — the thinnest window of the week. A
  crossover rule weighed that sliver as a full session, and `confirm_days = 2`
  spanning a weekend could be half-satisfied by it, or reset by it. Both keys are
  availability keys, so nothing about the no-lookahead argument changes; the
  UTC-day fallback remains for roots no calendar claims, and `GatheredSeries`
  records which was used.
- **D-0042** (2026-07-28) — **Back-adjusted prices are a different TYPE, not a
  different variable.** `continuous::AdjustedPrice` is a newtype with no
  `From`/`Into`/`Deref`/`as_price()` and no escape hatch; its only exits are
  `as_nanos() -> i64` and `as_points_f64()` (indicator space, §2.3).
  `ContinuousBar` carries `tradeable: Bar` and `adjusted: AdjustedOhlc` side by
  side so a consumer must *name* which one it wants. Why: `pnl_nano_usd` — the
  workspace's only price→money conversion — takes `Price`, so an adjusted level
  cannot reach it. PnL against a back-adjusted series is wrong by the cumulative
  roll gap (hundreds of ES points over a decade) and leaves a perfectly
  smooth-looking equity curve, which is exactly the silent corruption this
  project exists to prevent. Enforced by a `compile_fail` doctest paired with a
  compiling one, so the rejection is proven to be about the type rather than a
  typo. Adjustment is applied **at load**, never baked into storage.
- **D-0043** (2026-07-28) — **Additive back-adjustment only; `Ratio` is not a
  variant.** The table stores per-roll gaps `close(to) − close(from)` at
  `roll_ts` and the loader accumulates them, leaving the newest segment
  untouched — every term an `i64` difference of `i64`s, exact to the nanopoint
  with no `f64` in the price path (§2.3). Storing gaps rather than running
  offsets means appending next month's roll does not rewrite every earlier row.
  Ratio adjustment multiplies by a running product that must be rounded once per
  roll, so a 2010 bar's adjusted value would depend on how many rolls happen
  *after* it, and two backtests ending in different years would disagree about
  the same historical bar. Deterministic schemes exist; each is its own design
  with its own tests. A name that parses and quietly means something else is
  worse than one that does not parse.
- **D-0044** (2026-07-28) — **Open interest (`.n`) is not a `RollRule` variant
  at all.** It needs the `statistics` schema's open-interest series, which M1
  has not curated. A variant that existed and always failed would be a config
  that typechecks and then dies at load; one that fell back to volume would be a
  table labelled `.n` that is not one. It arrives with the data.
- **D-0045** (2026-07-28) — **Roll tables live at
  `curated/rolls/{root}/{tf}/{rule-slug}.json` and are not in the manifest.**
  They are what D-0036 says curated data is — derived, disposable, rebuildable —
  and `manifest.jsonl` records *acquisitions*, which nobody bought here. It is
  the one curated artifact that cannot be named after its source window: a roll
  is a statement about two contracts at once and a table spans a whole root. So
  it is named after what determines its contents (root, interval, rule) and
  records the manifest id of every curated file it read, keeping D-0013's
  provenance chain intact with no second index. The table also records the
  `ts_open` span it was built from, and `ContinuousFeed::open` **refuses** a
  replay window reaching outside that span rather than quietly returning a
  series with contracts missing.
- **D-0046** (2026-07-28) — **Contract-symbol year rule, and the expiry
  backstop.** Two-digit years are absolute (`yy` → `2000 + yy`); one-digit years
  resolve to the year congruent mod 10 nearest a `DecadeAnchor`, ties to the
  earlier year, with the anchor a pinned constant (never a clock — §2.2) and
  recorded in every roll table so stored bytes always reparse to the same
  contracts. *Amended the same day, once the real 16-year `definition` archive
  existed:* a single anchor cannot work inside one file, because `ESM0` is both
  June 2010 and June 2020 and the archive contains both — the constant made
  them collide and `ExpiryConflict` refused every ES definition file. So
  `expiries_from_definitions` resolves a one-digit year against **each record's
  own expiration** rather than the constant, which separates them and is still
  fully deterministic. It anchors on the expiry *year* rather than reading the
  year straight off it, because some products expire the month before their
  contract month (`CLF5` expires in December 2024 and must still be 2025). The
  constant remains the fallback and remains what curated bar partitions parse
  against, since those have no expiry to consult. Calendar spreads (`ESH4-ESM4`) fail to parse *by design* and are
  reported-and-skipped, never refused — D-0033 legitimately keeps them in the
  archive. The rule decides *when* to roll; expiry decides *that* you must: if
  the rule never fires and the front stops trading first, the roll goes on the
  last session both traded, the last instant the gap is observable without
  inventing a price. A front contract still trading when the data ends has not
  handed over — the data merely stopped — so the chain ends there, which is why
  the real January-2024 ES archive yields a correct one-contract table instead
  of a fictional roll on the 31st.
- **D-0047** (2026-07-28) — **`keep_support_files` reports its failures through
  `ExecuteReport` instead of swallowing them, and leaves `staging/` in place
  when it could not move something.** Best-effort is right — the payload is
  already archived and recorded by then — but best-effort is not the same as
  silent: `condition.json` is what the data-QA report reads to tell a vendor
  outage from a hole in our own pipeline, and an operator who is never told it
  went missing finds out a milestone later. The library still never prints (§3);
  the warnings ride the same route `dropped_symbols` already took and the CLI
  renders them. Deleting staging after a failed rename would have destroyed the
  very file being warned about.
- **D-0048** (2026-07-28) — **The ban on reading the wall clock is a clippy
  gate, not a comment.** `clippy.toml` disallows `SystemTime::now`,
  `Instant::now`, `chrono::Utc::now` and `chrono::Local::now` workspace-wide,
  and the one sanctioned site — `crucible-cli`'s `SystemClock` (D-0032) — carries
  an `#[expect(clippy::disallowed_methods, reason = …)]` that names why. Why now:
  D-0039 originally claimed that requesting `chrono` without its `clock` feature
  made `Utc::now()` *not exist* in this build. Independent review falsified that
  — `parquet` enables `chrono/clock`, Cargo unifies features across the graph,
  and `Utc::now()` compiled fine. A comment asserting a guarantee that does not
  hold is worse than no comment, so the claim is corrected everywhere it appeared
  (Cargo.toml, the `calendar` module docs, CLAUDE.md §6, D-0039) **and** replaced
  with something real. `expect` rather than `allow` throughout, per §5.6: an
  exemption that stops being needed becomes a warning and gets deleted.
- **D-0049** (2026-07-28) — **The archive layout is a checked contract, written
  down in `docs/DATA_LAYOUT.md` and enforced by `crucible layout-check`.** Two
  invariants: *no directory holds two instruments' data, and none holds two
  kinds*. `raw/` is `{dataset}/{schema}/{symbol}/{window}` and `curated/` is
  `{kind}/{instrument}/{grain}/{file}` — files exactly four components deep,
  directories never more than three — with `{schema}` and `{kind}` closed sets,
  because `ohclv-1m` is a transposition that looks plausible in a listing and
  would hold paid-for data nothing ever reads. Seven violation classes, each
  with a negative control that has been seen firing (§7); findings reported in full,
  filesystem ones in walk order and manifest ones in record order, exit 4.
  **The two trees nest in opposite orders on purpose** and the doc argues it at
  length, because it reads like an inconsistency and the obvious "fix" destroys
  something: `raw/` is keyed by the purchase tuple, which is what makes coverage
  subtraction a directory listing and what a manifest `file_path` has to keep
  meaning forever; `curated/` is keyed by the research question, and its
  provenance lives in the file rather than the path precisely because derived
  data can have several sources. What joins them is content identity — the raw
  file's blake3 in every curated footer — not path shape, and that is why they
  are free to differ. **No `--fix`, ever**: manifest paths name the bytes a
  result read (D-0013/D-0014), so renaming an archived file retroactively and
  silently breaks the provenance of every result that read it. A misplaced raw
  file is corrected the way D-0017 corrects a corrupt one — a new acquisition at
  a correct path — and curated data is simply deleted and rebuilt. Future shapes
  are pinned now (`curated/trades/…`, `curated/book/…`,
  `external/thetadata/options/{underlying}/{granularity}/…`, equities under
  `raw/{equities-dataset}/…`) so the later work lands pre-separated. `external/`
  keeps a **per-vendor inventory file, never merged into `manifest.jsonl`**:
  the manifest's guarantee is "this tool fetched these bytes and hashed them
  here", an inventory's is "a human says this came from there on that date", and
  collapsing them would downgrade the first to the second for everything in it.
- **D-0050** (2026-07-29) — ThetaData is adopted as a second vendor for US
  equity/index **options** and US **equities**, landing under
  `external/thetadata/` with its own append-only `inventory.jsonl`.
  **Supersedes D-0010's** "no options/ThetaData" descope, which deferred them
  post-M4 on scope grounds. Why now: the subscription is live and time-boxed,
  so the acquisition window is the constraint, not the integration work — data
  not pulled while subscribed is data that costs a new subscription to get.
  Entitlements are whatever the running Terminal reports, never the published
  tables: it logs `Stock: STANDARD  Options: PROFESSIONAL  Index: FREE` and
  `Max concurrent requests: 8` (global, not per asset class), and the two
  sources have been observed to disagree. Index endpoints answer 403, which
  costs nothing, because the greeks endpoints carry `underlying_price` — a
  1-minute index level for SPX/NDX/RUT/VIX arrives with the option data.
- **D-0051** (2026-07-29) — `reqwest` is blessed for `crucible-data` under the
  non-default `thetadata` feature, `default-features = false` (no TLS backend
  at all). Why this crate and not another: it was **already in the dependency
  graph** transitively under the `databento` feature, so this adds a direct
  edge rather than a second HTTP stack to audit and compile; and the Theta
  Terminal is a plaintext HTTP/1.1 server on `127.0.0.1`, so a TLS backend
  would be dead weight. Cargo features are additive across the graph, so
  asking for none here cannot weaken what the `databento` client resolves.
  Same seam as D-0025: async lives in one module, behind a sync API, on a
  private current-thread runtime, and `crucible-data` still starts no threads.
  Retry-on-transport/429/5xx is unconditional here, unlike D-0035's carve-out
  for Databento `submit`, because every call on this API is an idempotent
  read — there is no submission analogue, and none may be invented.
- **D-0052** (2026-07-29) — Eastern wall-clock timestamps that are
  **ambiguous** (fall-back hour) are refused as corrupt, exactly like
  **nonexistent** ones (spring-forward gap); neither is silently resolved.
  Why: ThetaData stamps rows in local Eastern with no offset, so both
  pathologies are reachable, and every such stamp becomes an `avail_ts` (§2.1).
  Resolving an ambiguous stamp to the **earlier** candidate — the first
  implementation, and wrong — asserts the information existed an hour before it
  may have, making it visible to a strategy that could not have seen it: silent
  lookahead, manufactured in the one module whose job is preventing it. Delay
  is the conservative direction, since information withheld can only cost
  measured performance, never fabricate knowledge. Both windows fall 01:00–03:00
  on a **Sunday**, when no US equity or options session prints, so refusing is
  free and surfaces corruption instead of absorbing it. If a future feed
  genuinely trades through the ambiguous hour, the policy for a blind choice is
  the **later** instant, never the earlier.
- **D-0053** (2026-07-29) — The **canonical** greeks/IV surface for research is
  the one we compute (parity forward + discount factor + Black-76 from `eod`
  NBBO mids) over the whole 2012→now span. Vendor greeks are a **cross-check**
  where they exist, never the feature input. Why: vendor greeks begin mid-sample
  and at a different date per root — SPY ~2017, QQQ ~2013, NDX ~2026-06 — with
  undocumented methodology, so a feature sourced from them silently changes
  convention at each root's floor. That is a regime change baked into the data
  and indistinguishable from a real one: the silent-wrong-results class this
  project exists to prevent. One documented methodology across the full span,
  validated against the vendor where available, is the reproducible choice
  (§2.5), and it repairs the 2012–2017 greeks gap for free.
- **D-0054** (2026-07-29) — ThetaData's `option/history/eod` returns **every
  contract twice** in older eras: the vendor ran two post-close build passes and
  serves both snapshots, so `rows = 2 × distinct(expiration, strike, right)`.
  Measured vendor-wide (SPY, QQQ, SPX) and era-dependent: **duplicated through
  at least 2021-12-15, clean from 2022-01-03**, the same boundary on SPY and
  QQQ, so it is a vendor pipeline change at the 2021→2022 turn rather than
  anything per-root. The number of build passes is **not** fixed at two —
  2020-01-02 carries four distinct `created` values while every contract still
  appears exactly twice, i.e. contracts are split across passes — so dedup
  groups by contract key and never assumes a file-level count.
  `option/history/open_interest` is **not** affected (ratio 1.000 on 2014,
  2017, 2019 and 2024), so this is `eod`-specific, but the uniqueness gate
  applies to every endpoint because the mechanism is a build-pipeline artefact
  and nothing guarantees where it appears next. `greeks/eod` is already
  deduplicated (ratio 1.000 wherever it exists). Consequences, all
  merge-blocking: completeness accounting keys on **distinct contracts, never
  raw rows** — an OI-weighted or GEX-style aggregate over raw `eod` rows would
  silently double for affected eras, and this is worst below the greeks floor
  where `eod` is the only source and has no `greeks/eod` to reconcile against.
  Dedup groups by contract key and keeps `max(created)` — the final build, and
  the conservative `avail_ts` direction per D-0052 — never assuming exactly two
  builds. Pairs whose market fields are byte-equal are deduplicated silently
  with a count recorded; pairs that **conflict** across builds are a revision
  and are surfaced in the validation report as QA signal; the same
  `(contract, created)` twice is a different bug and refuses the file. Dup rate
  is recorded per file in the inventory as an era fingerprint. Note also that
  column 5 is `created` (build-run time) in `eod` but `timestamp` (per-contract
  update time) in `greeks/eod`: parsing is validated against a pinned header by
  **name, never by position**, and the two land as separate `_ts` columns.
- **D-0055** (2026-07-29) — **Row uniqueness is declared per endpoint, and the
  all-zero-OHLC refusal applies only where OHLC is the whole payload.** D-0054
  established that `eod` repeats a contract across build passes; it does not
  follow that every endpoint's rows are keyed the same way. So each endpoint
  declares a `RowIdentity`: contract-per-day with a build/update *discriminator*
  for `eod`/`greeks/eod`/`open_interest`, `(contract, timestamp)` with **no**
  discriminator for the interval endpoints, and timestamp alone for stocks.
  Where there is a discriminator, repeats deduplicate by keeping the maximum;
  where there is none, a repeat refuses the file. The interval case is why one
  rule could not serve: a contract legitimately appears 391 times in a
  1-minute day, and dedup-by-contract would silently discard the session — the
  same shape of bug as counting raw rows, in the opposite direction.
  **The all-zero gate was scoped by evidence that nearly went the other way.**
  §4.3 of `docs/THETADATA_PLAN.md` reads as a file-level rule, and applying it
  to `eod` would have been catastrophic: VIX `eod` for 2024-01-02 returns 1,058
  contracts of which 672 carry `0.00,0.00,0.00,0.00` and zero volume, and 614
  of *those* carry a real bid. On `eod` a zero OHLC block means "this contract
  did not trade today" — most of a chain on most days — and the NBBO beside it
  is the real data. The gate therefore applies only where OHLC is the entire
  payload and no quote can carry information instead: `stock/history/ohlc`,
  which is the measured case (SPY 2016-01-04), and `option/history/ohlc` by
  structural analogy, which is recorded as analogy rather than measurement.
- **D-0056** (2026-07-29) — **One global pacer, and its circuit breaker ends
  the run instead of pausing.** The Terminal advertises `Max concurrent
  requests: 8` and enforces a second, undocumented ceiling: a `JettyRateLimiter`
  that drops connections under sustained load. Both are properties of one
  process, so both belong to one shared object — a per-task limiter multiplies
  rather than bounds, and eight tasks each politely holding one request per
  interval still launch eight per interval between them, which is the pattern
  that tripped the limiter during probing. Constants, all recorded in §5 of the
  plan: concurrency 8 (the vendor's own figure), a **150 ms** global launch
  floor chosen from the measured 0.3–2.7 s request band, 4 attempts, 500 ms
  backoff doubling to a 30 s cap, `Retry-After` honoured up to 120 s, breaker at
  5 consecutive drops. `Retry-After`'s HTTP-date form is deliberately
  unsupported: parsing it means trusting this machine's clock against the
  server's, and a skewed clock computes a negative delay and hammers the
  endpoint that just asked for a pause. The breaker **fails the run** rather
  than pausing and retrying, because resume is an inventory diff: stopping costs
  a re-run of what had not completed and nothing for what had, while a client
  that keeps hammering a Terminal that has stopped answering is how a soft limit
  becomes a revoked subscription. Replacing `fetch_batch`'s fixed-size chunking
  with a shared semaphore also removed a real cost: a chunk barrier made every
  batch of eight pay for its slowest member, and with a 0.3–2.7 s spread that
  idled seven requests behind one for hours.
- **D-0057** (2026-07-29) — **The CSV→Parquet ratio is measured, not assumed,
  and it is not one number.** §7.3 carried ~10× as an explicit assumption. The
  golden-raw round trip (`crucible theta-golden`, VIX 2024-01-02, 6,221,352
  cells compared individually against the source CSV) measures instead:
  `quote` @1m **18.61×**, `greeks/first_order` @1m **11.88×**,
  `open_interest` **5.79×**, `eod` **4.28×**, `greeks/eod` **2.46×**. The
  spread is the point and it is not noise: quote rows are repetitive and
  dictionary/delta-encode well, while `greeks/eod` is 44 high-entropy doubles
  per contract with almost nothing to exploit. Using one blended figure would
  be wrong in both directions at once. Consequences: T1's ~1.75 TB of CSV lands
  near **~95 GB** rather than the assumed 175 GB, so it fits the cap with room
  to spare — but **T0's "~3–5 GB parquet" estimate is too low** and is corrected
  to a measured-ratio projection, because T0's endpoints are the ones that
  compress *worst*. Also pinned here: v3 spells the sampling interval
  `interval=1m`. Every v2-era example uses milliseconds, and `interval=60000`,
  `60` and `1min` all answer `400 Invalid interval`. This had to be probed
  rather than read, because §3.3's silent-parameter behaviour means only a bad
  *value* announces itself — an unknown *parameter* returns 200 and is ignored.
  `greeks/first_order` additionally refuses `expiration=*`.
- **D-0058** (2026-07-29) — **A session may open on its own calendar day, and
  US equities/options get their own bundled calendar.** The loader hard-refused
  any table whose close was not strictly before its open — "a trading day must
  open on the previous calendar day and close on its own" — which encoded *every
  market is CME* into the one module whose docs claim to describe exchanges in
  general. `[calendar.session] shape` now selects `overnight` (the default, so
  every existing table keeps its exact meaning and no golden value moves) or
  `same_day`. Three behaviours are shape-dependent and all three were wrong for
  a same-day market: the open date of a session, whether the evening belongs to
  tomorrow's trade date, and which inversion of open/close is an error.
  **Why a second table rather than reusing `cme_globex_equity_index`:** because
  the trading-day sets genuinely differ inside the ThetaData span, and borrowing
  would have manufactured findings. The NYSE was closed for two consecutive days
  for Hurricane Sandy (2012-10-29 and 30) while Globex traded electronically and
  cancelled only its floor session; the two national days of mourning
  (2018-12-05, 2025-01-09) closed equities outright where CME ran an abbreviated
  session; and CME abbreviates Good Friday when payrolls land on it while the
  NYSE simply closes. Reusing CME's calendar would have reported four real
  closures as missing vendor data — plausibly, and wrongly. There is a test
  asserting the disagreement in both directions.
  **The table claims only the roots whose hours it describes**: SPY, QQQ, IWM,
  DIA. SPX/SPXW/VIX/NDX/RUT share the holiday set but not the session — Cboe
  global hours, 16:15 ET closes — so claiming them would give `is_open` and
  `bars_per_year` a confident wrong answer for exactly the roots this project
  cares most about. `for_instrument` returns `None` for them by design, and
  intraday index-option hours are a future table with its own sourced session
  block, never a `roots` line added here. The holiday set is validated by a
  hand-derived count: 2024 holds 262 weekdays minus ten weekday holidays =
  **252 sessions**, the figure the exchange publishes.
- **D-0059** (2026-07-29) — **A calendar can govern a root's DATES without
  governing its HOURS, and the split is a type rather than a convention.**
  D-0058 left `us_equity_options` claiming four ETF roots, which left §4.4's
  coverage edge uncomputable for the five index roots T0 also acquires — half
  the tranche unmeasurable on completeness. The resolution is not to widen
  `roots`: SPX, SPXW, NDX, VIX and RUT keep the cash-equity holiday set and run
  different sessions, so widening would hand out a confidently wrong `is_open`
  and `bars_per_year` for exactly the roots this project cares most about.
  Instead the table gains `day_level_roots`, reachable only through
  `TradingDayCalendar` — a view exposing `is_trading_day` and `day_effect` and
  **not** `is_open`, `session_of` or `bars_per_year`. The same device as D-0042:
  the way to stop a value being used where it does not belong is to make the
  call not exist. `coverage_vs_calendar` takes the narrow type, so the barrier
  binds at the use site rather than in a comment.
  **The day-level claim is sourced, and by enumeration rather than by
  assertion.** Cboe's published US-options hours page lists its 2026 holiday
  schedule — New Year's Day, MLK, Presidents' Day, Good Friday, Memorial Day,
  Juneteenth, Independence Day observed 3 July, Labor Day, Thanksgiving,
  Christmas, plus early closes after Thanksgiving and on Christmas Eve — which
  is exactly the rule set in the table, closure for closure. Nobody states
  "Cboe follows NYSE"; the two enumerated lists agree, which is better evidence
  than the claim would have been. `day_level_source` is required whenever
  `day_level_roots` is non-empty, because an unsourced claim about someone
  else's exchange is what this table format exists to prevent. Verified against
  2026 as published; earlier years follow the same rules but were not
  re-verified page by page, and the gate ledger says so rather than rounding it
  up to "verified".
  **A gap found while sourcing it, and recorded rather than carried silently:**
  the NYSE closes early on 3 July only when 4 July falls Tuesday–Friday, and
  `weekday_before` cannot express that condition, so the table produces a
  spurious 13:00 close on six dates (2015-07-02, 2016-07-01, 2020-07-02,
  2021-07-02, 2022-07-01, 2026-07-02) — confirmed absent from Cboe's 2026
  schedule. Worth ~18 hours across ~3,539 sessions (~0.08 % of
  `bars_per_year`) and **zero** effect on the trading-day set, which is the only
  thing this calendar is used for today. `cme_globex.toml` carries the identical
  gap for the identical reason.
- **D-0060** (2026-07-29) — **The combo layer splits along the dependency
  boundary, not along the feature.** Spec types, parameter axes, the rule AST,
  grid expansion and the strategy factory are plain data and pure functions in
  `crucible-strategies::combo`; TOML parsing (`serde`, `toml`) lives in
  `crucible-cli::config`; and the config hash is computed by the caller from a
  `ComboSpec::canonical_form()` string this crate renders but never hashes.
  Why not put serde in `strategies`: §3 says `engine` and `strategies` must
  compile with no I/O, no async and no threads *forever*, and §6 blesses
  `serde`/`toml` for `funnel`, `data` and `cli` only. A `#[derive(Deserialize)]`
  on the spec types would be one line and would quietly make the crate that the
  funnel instantiates thousands of times per data pass carry a parser it never
  runs. Why the hash is supplied rather than computed: D-0012 pins config
  identity to blake3, which is not a `strategies` dependency and must not
  become one — so this crate produces the *canonical form* (sorted slots,
  materialized axes, rules re-rendered from the AST rather than echoed from
  source text, so comments, whitespace and redundant parentheses cannot change
  an identity) and the crate that owns blake3 hashes it. Same device as D-0015,
  which made `acquired_ts` caller-supplied so library code could not read a
  clock: the way to keep a capability out of a crate is to make the call not
  exist there. **§6 is amended:** `blake3`'s row gains `cli`, because the CLI is
  where a config is loaded today (`funnel` inherits it in M3), and `serde` +
  `toml` become real dependencies of `crucible-cli` rather than blessed-but-
  unused. *Consequence to be aware of:* `crucible-funnel::grid`'s module doc
  spec'd expansion as M3 funnel work. Expansion moved here because §2.6's
  fair-comparison rule needs `max_warmup` across the grid at *build* time —
  the funnel cannot align eval windows for combos it cannot yet construct —
  and because expansion is a pure function over plain data with nothing to
  parallelize. What stays in `funnel::grid` is what actually needs the funnel:
  combo-count guardrails against a config, registry dedupe on
  `(config_hash, combo_index, fold)`, and scheduling.
- **D-0061** (2026-07-29) — **Warmup alignment is a `Strategy` decorator, and
  it discards orders rather than withholding bars.** `strategies::align::Aligned`
  wraps any strategy, forwards every bar to it — so indicators warm on exactly
  the data they would otherwise see — and drops the intents it emits until the
  grid's `max_warmup_bars` have been consumed. Why a decorator rather than an
  engine feature: §2.6 is a statement about a *set* of runs, and the engine
  deliberately knows nothing about grids; giving `replay.rs` a "start trading at
  bar N" parameter would put a fairness rule inside the loop whose only job is
  the no-lookahead rule, and every hand-written strategy would then be able to
  ignore it. Why drop intents rather than skip `on_event`: an indicator that
  does not see the warmup bars is not warm, so the short-warmup combo would
  simply start late instead of starting aligned — which is the opposite of the
  §2.6 guarantee. **The honest cost, stated because it shows up in a printed
  number:** every combo's equity curve carries a flat prefix of exactly
  `max_warmup` bars. It is identical across the grid, so a comparison within
  the grid is fair and rankings are untouched, but the naive Sharpe of each
  combo is scaled by the same `sqrt(n_eval / n_total)` factor against a run
  that started at its own warmup. Slicing the eval window out of the metrics
  belongs to the walk-forward runner, which has to slice folds anyway; until it
  lands, `crucible combo` prints the eval-window start so the factor is visible
  rather than absorbed.
- **D-0062** (2026-07-29) — **Walk-forward folds are counted in TRADING DAYS,
  and the config field names say so.** `[walk_forward]` is `scheme` +
  `train_days` / `test_days` / `step_days`, superseding the `train_months` /
  `test_months` spelling that CLAUDE.md §4 used as its units-suffix example
  (§4's example is updated in the same commit; the convention it illustrates —
  units in the name — is unchanged, and is exactly what this rename obeys).
  **Why not months:** a "6-month" CME window holds between roughly 122 and 130
  sessions depending on where it lands, because holidays cluster (Thanksgiving,
  Christmas, Good Friday) and Easter moves. Fold *k*'s out-of-sample Sharpe is
  then estimated on a sample whose size the exchange's holiday schedule chose,
  the pooled headline weights folds by that accident, and two configs differing
  only in start date get different effective layouts. Sessions make every fold's
  *n* a number the researcher wrote down. Trading days are also immune to DST,
  which a wall-clock month is not.
  **Why not bars:** a bar-counted window ends mid-session, splitting one trading
  day across the train/test boundary — the single place where a position carried
  across the seam looks most like leakage. Every window here is a whole number
  of whole sessions.
  **Three boundary rules, all of which move numbers:** (1) the first fold starts
  at the first *whole* trading day at or after the grid's warmup, and the bars of
  the partial session the warmup ended inside are reported as `partial_day_bars`
  rather than absorbed; (2) `step_days` defaults to `test_days` (which tiles the
  sample exactly) and `step_days < test_days` is **refused**, because the
  headline pools the test windows and overlapping windows count a session twice
  — inflating *n*, shrinking the standard error, and flattering everything
  downstream that reads it; a step *wider* than `test_days` merely wastes
  sessions and is allowed, with the unused tail reported; (3) both schemes place
  the *same* test windows and differ only in the training window, so an anchored
  and a rolling run are comparable to each other.
  **Where the definition of a trading day lives:** not in `crucible-funnel`.
  `FoldPlan::build` takes `day_keys: &[i64]`, one nondecreasing key per bar, and
  the CLI computes them as
  `days_from_civil(calendar.trading_day(bar.avail_ts()))`. Same device as
  D-0015's caller-supplied `acquired_ts` and D-0060's caller-supplied config
  hash: the way to keep a capability (here `crucible-data`, and with it `chrono`
  and the compiled-in calendar tables) out of a crate is to make the call not
  exist there. §3's `funnel → core + engine + strategies` edge is unchanged; the
  `engine` and `strategies` edges are taken up now that code uses them, and
  their placeholder comments are deleted from `Cargo.toml` per §6.
  **Keyed on `avail_ts`, never `ts_open`** (§2.1). A fold boundary decides which
  bars are ordered into which window, and ordering decisions use availability
  time. For 1m bars the two agree; for coarser ones they need not, and the
  availability answer is the one that cannot see the future.
  **Instruments no calendar governs** (every `SYN:*` feed, and any root without a
  bundled table) fall back to UTC civil days, and the report says so on every
  run. For a synthetic feed that is exactly right — it has no exchange. For a
  real instrument it means a session spanning midnight UTC is split, which is why
  it is printed rather than assumed.
  **`schema_version` stays 1.** That field exists to protect the interpretation
  of fields something has *read*; `[walk_forward]` had never been consumed by any
  build, so there is no interpretation to protect and no stored result that could
  be misread. A config carrying `train_months` now fails on `unknown field`,
  naming the line and the four fields that replaced it — the loud failure §5.5
  asks for.
- **D-0063** (2026-07-29) — **A fold is a metric WINDOW over one replay, not a
  separate backtest, and three slicing conventions make its numbers mean what
  they say.** `walkforward::runner` replays each combo once over the shared bar
  series and cuts the resulting equity curve; it does not re-replay per fold.
  Nothing here re-fits anything — walk-forward exists to produce an
  out-of-sample *sample*, and parameter selection across combos is the funnel's
  job (M3) — so a per-fold replay would recompute identical fills. A continuous
  replay is also what a deployed strategy does: resetting indicator state at each
  boundary would make every test window open with a strategy that has forgotten
  the market, which is a property of the fold layout rather than of the strategy,
  and it would move the numbers.
  **The anchor bar.** A window `[start, end)` is measured from the equity point
  at `start − 1`, so the move *into* its first bar is its first return. Measuring
  from `start` silently drops that return.
  **Rebasing to declared capital.** Each window's curve carries its own per-bar
  equity *deltas* applied to the config's `initial_cash_usd`, not to whatever
  equity had drifted to when the window opened. Position size is a fixed contract
  count (`run.qty_contracts`), so a window's dollar PnL does not scale with the
  account, and quoting fold 7 against fold 7's opening equity would make two
  identical windows report different percentages purely because of what preceded
  them. The rebase is additive integer nano-USD — no float re-enters accounting
  (§2.3).
  **Pooling by deltas, not by levels.** The headline out-of-sample curve is the
  concatenation of every test window's deltas. Concatenating *levels* across the
  training windows between them would invent a jump at each seam and hand
  `max_drawdown_pct` a drawdown nobody could have suffered. Tiled windows share a
  seam bar, and the seam contributes one return, not two — otherwise every pooled
  Sharpe would be biased downward by an artifact of the fold count.
  **The honest consequence, stated because it is a printed number:** a round-trip
  opened in a training window and closed in a test window counts as a
  test-window *trade*. The mark-to-market series still splits the *money*
  correctly (each window keeps the marks inside it); only the trade count and win
  rate attribute the whole round-trip to where it was realized. M2's
  stops/targets work will make this more visible.
  **The engine change this required:** `Portfolio` now records
  `ClosedTrade { closed_ts, net_nano_usd }` and `FeeEvent { ts, fee_nano_usd }`
  rather than a bare `Vec<NanoUsd>` and a running total, and `BacktestResult`
  carries both. Without timestamps, a fold reporting "14 round trips" or a fee
  figure would be quoting the whole run under a window's label. Fee events are
  recorded only when nonzero, so a `FreeFills` run has none at all — the honest
  shape for an execution assumption that charges nothing. `demo --hash-only` and
  `combo --run --hash-only` are byte-for-byte unchanged by the refactor.
  **D-0061's caveat retires for this command and only this command.** Every
  number `crucible walk-forward` prints excludes the grid's warmup and every
  training window, so the `sqrt(n_eval / n_total)` factor does not apply to it.
  `crucible combo` still carries it, because it still reports one window and that
  window still contains the warmup — and it now says so in its footer and points
  at the command that slices.
- **D-0064** (2026-07-29) — **Derived seeds exist before the first thing that
  consumes randomness, and the config's own `[run].seed` is part of the tuple.**
  `walkforward::seed::derive_seed(config_hash, root_seed, combo_index,
  fold_index) → u64` is computed and recorded for every (combo, fold) even though
  nothing in this build draws a random number. Why now: the alternative is that
  the first randomized component — a block permuter, a bootstrap resampler —
  invents its own seeding in a hurry, in the place where it is least visible,
  which is how a "deterministic" pipeline stops being one. Why `[run].seed` joins
  §2.2's `(config_hash, combo_index, fold)` triple: that triple is a floor, and
  `config_hash` is blake3 over a `ComboSpec::canonical_form` — slots, rules and
  size — which deliberately does not cover `[run].seed`. Leaving it out would
  make two configs differing only in their declared seed derive identical
  per-fold seeds, defeating the field. The mixer is hand-rolled (FNV-1a absorb,
  SplitMix64 finalize) for the same reason the CLI's `Fnv64` is: `DefaultHasher`
  is not stable across Rust releases, and a seed that changes with the toolchain
  is not a seed. It is not a PRF and does not need to be; it needs to be stable
  and to avalanche, so adjacent `(combo, fold)` pairs do not start correlated
  streams. One value is pinned by test — changing the derivation changes every
  seed in every stored result, which is a decision-log event rather than a
  refactor.
- **D-0065** (2026-07-30) — **`Inventory::append` retries a sharing violation
  ten times over ~4.5 s before failing; observing a run must not be able to end
  it.** On Windows `Get-Content` and `System.IO.StreamReader` open with
  `FileShare.Read`, which denies writers, so a reader merely counting lines makes
  the next append fail with `os error 32` — and a failed inventory append ends
  the tranche, correctly, because a placed file with no inventory line is an
  orphan. Why now: on 2026-07-30 a progress count taken by this agent killed a
  live T0 run 2 h 41 m and 830 requests in. Resume made the loss cheap, and that
  is precisely the trap — "resume is free" turned a fragile interface into an
  acceptable one, and the same glance would have cost far more inside T1's larger
  payloads. Only the sharing violation is retried (`ERROR_SHARING_VIOLATION` 32,
  `ERROR_LOCK_VIOLATION` 33): a missing directory or a permission denial does not
  clear on its own, and a retry loop over those would delay the report by five
  seconds and teach nothing. The predicate is `#[cfg(windows)]` because POSIX has
  no mandatory sharing and errno 32 is `EPIPE` there — retrying *that* would
  paper over an unrelated failure. Bounded, not infinite: a genuinely stuck
  holder must fail the run rather than hang it, and the exhausted error names the
  cause and points at `RUNBOOK_BLITZ.md`'s observation table. Both halves are
  tested against a real `share_mode(0)` handle — a retry nobody has watched
  survive a lock is decoration, and one nobody has watched give up is a hang
  waiting to happen. `ShutdownBlockReasonCreate` during active runs is recorded
  in the RUNBOOK as a candidate, not built.
- **D-0066** (2026-07-30) — **The manifest's `symbols` list is incomplete for
  CL.FUT and ZN.FUT, the cause is a validation rule and not a serialization
  limit, and the repair is an append-only supplement — M1-close-block, not a
  hotfix.** Quantified by `cargo run -p crucible-data --features databento
  --example sym_audit`, which re-reads each archived file's DBN metadata and
  classifies every observed symbol with the *same* `catalog::is_valid_symbol` the
  append used: **21,736 of 108,696 observed symbols are absent from the manifest
  (20.00 %), across 8 of 33 lines**. It is confined to two roots — CL.FUT 9,427
  observed / 7,120 recorded (2,307 dropped) and ZN.FUT 3,335 observed / **208
  recorded** (3,127 dropped, 94 %) — and appears once per schema that pulled all
  seven parents (`definition`, `ohlcv-1m`, `ohlcv-1s`, `statistics`: 5,434 each).
  ES/NQ/RTY/GC/6E drop nothing. **The mechanism is not the manifest format.**
  `symbols` is already a JSON array (`"symbols":["ES.FUT","ESH4",…]`) and `serde`
  round-trips a space perfectly well, so nothing here needs format evolution,
  versioned records, or escaping. The disqualifier is the **space** in CME's
  exotic spread names (`CL:BF F0-G0-H0`, `UD:ZN: TL 0110987001`), rejected by
  `validate_symbols`'s whitespace ban; colons pass, since only `validate_file_path`
  rejects those. That ban exists because "symbols appear inside ingest's path
  template" — but `plan.rs` builds `raw/{dataset}/{schema}/{key}/{stem}.dbn.zst`
  from the **requested key alone**. Only `symbols[0]` is ever a path component;
  the rest are pure coverage data that never touch the filesystem. So the rule is
  over-broad, and the fix is to narrow its scope rather than change the format:
  full path-safety for the key, and non-empty/ASCII/no-control for the recorded
  coverage symbols. **Recovery source is the archive, not `delivery/`.** The
  delivery support files carry no resolved symbology — `metadata.json` holds only
  the request (`symbols:["CL.FUT"]`), `manifest.json` a file list, `condition.json`
  per-date conditions. The resolved set lives in each `.dbn.zst`'s own DBN header:
  immutable, already checksummed by `verify`, and the exact source the original
  append read. A supplement record is still the right vehicle, because the
  manifest is append-only and an existing line must not be rewritten — the
  correction references the original line's blake3 and the archive stays
  immutable. **No silent era exists:** `merge_symbols`, `dropped_symbols` and
  `is_valid_symbol` all landed together in `05eeed1` (2026-07-27), the commit that
  created the execute path, so every append that dropped a symbol printed the
  warning — the records were lost with the stdout, not never written. Consequence
  until repaired: `coverage` reads those 21,736 symbols as missing and would buy
  them again, which is the re-buy bug D-0033 exists to prevent. §9's closing
  sentence may be written before the repair lands, with the exception named: "every
  plan item appended" stays true; "the manifest tells the whole truth per
  contract" has a known, quantified exception in CL.FUT and ZN.FUT.
  **Approved fix, four specifics.** (1) *Narrow the predicate to the one place a
  symbol is a path component* — the requested key. Array members accept vendor
  symbols verbatim; no escaping and no format change, because there was never a
  format problem. (2) *Supplement records are sourced from the DBN headers through
  the same decoder `transcode` uses* — never a second parser. A forensic parser
  that disagreed with the production one would write a manifest nobody can
  reproduce, which is the failure D-0031 already pinned for the decoder/client
  pair. (3) *Negative control, mandatory* (CLAUDE.md §7): plant a spaced symbol and
  assert it survives `append` **and** round-trips through `coverage` — a
  completeness fix whose test only checks the append proves half of nothing, since
  coverage credit is what was actually broken. (4) *Post-repair `sym_audit` reads
  0 dropped archive-wide, and the §9 exception clause is deleted in the same
  commit* — it stops being true, and a caveat that outlives its cause is how a doc
  starts lying. `examples/sym_audit.rs` is kept for exactly this: it is the
  completion proof, not scaffolding. Severity is the justification and was
  unknowable before the audit — **ZN.FUT at 94 % missing** is the number that made
  this a close-blocker rather than a cleanup.
- **D-0067** (2026-07-30) — **Account evaluation is a first-passage problem over
  a captured intraday equity path, estimated by day-block bootstrap; accounts are
  data, not code; headlines are out-of-sample only.** Spec:
  `docs/ACCOUNT_EVAL_SPEC.md`; sixteen accounts in `configs/accounts/*.toml`,
  every figure read off the firm's own page on 2026-07-30 and cited there.
  Nothing is implemented — this lands with the funnel in M3, and it deliberately
  does not respecify `crucible-funnel::stats` (deflated Sharpe, PBO/CSCV,
  permutation nulls stay M3's).
  **The failure mode it exists to prevent, named:** an intraday trailing
  threshold evaluated on daily closing marks *understates* breach probability,
  one-directionally, because the maximum decline from a running high-water is
  never smaller than the decline between two endpoints of the same path. The
  cheap version of this measurement is optimistic by construction and looks
  careful, which is the worst combination available.
  **The intraday-vs-EOD shorthand is wrong and the schema splits it in two.**
  `ratchet_basis` says what advances the threshold upward (continuous peak
  including unrealized PnL — Apex, TPT PRO — versus highest end-of-day balance —
  Topstep, TPT Test); `breach_basis` says what is tested against it. Every firm
  that documents the second one tests it on **intraday unrealized equity**,
  including the "EOD" accounts: Topstep's own page says the MLL "updates at the
  end of each trading day but is monitored in real time throughout the session".
  A model that only checks daily closes is wrong about a Topstep account, not
  approximately right about it. One formula covers all sixteen —
  `threshold(t) = min(max(0, max_{s≤t} B(s)) − D, L)`, breach on `≤` — and it
  reproduces every worked example on every firm's page, which is why those
  examples become the golden fixtures.
  **Everything is modelled in cumulative-PnL space**, not balance: a Topstep
  Combine starts at $50,000 with its limit $2,000 below, and a Topstep Express
  Funded Account starts at **$0** with its limit at −$2,000 locking at $0. Same
  account in cum-PnL space, two accounts in balance space.
  **Capture, not reconstruction.** The intraday high-water series is a streaming
  O(1) reducer inside the engine's existing mark-to-market loop (`replay.rs`
  step 2). It is never rebuilt from OHLC afterwards, because a reconstruction
  re-opens the intrabar path ambiguity that the engine's worst-case fill
  convention already resolved, and would measure the drawdown of a path the
  account never took. A 16-year 1s replay is 334 M bars: the per-bar equity
  vector alone is ~5 GiB, so full retention is refused and a 72-byte per-day
  summary (290 KB for 4,032 sessions) replaces it — **with a proof that the
  summary is exactly sufficient** for the breach question, plus the one
  conservative case (a lock engaging mid-day) disclosed and counted rather than
  absorbed.
  **Block bootstrap, `L = 20` trading days, resampling whole days as objects**
  rather than scalar daily PnL — that is what lets a daily bootstrap answer an
  intraday question without inventing a within-day path. `L = 20` from three
  independent arguments: the `O(n^{1/3})` rate (10–16 for 1–16 years of
  sessions), volatility-clustering persistence of weeks, and the fact that first
  passage is driven by runs of consecutive losing days, so the report prints the
  empirical longest loss streak beside `L`. An i.i.d. bootstrap scatters a bad
  month and **understates** breach probability; `L = 1` is kept in the mandatory
  sweep so that understatement is a number rather than an argument.
  **Account selection is priced, not forbidden.** `account_id` joins
  `(config_hash, combo_index, fold)` in the run identity and the seed derivation
  (extending D-0064), the registry insert happens before the run, and every extra
  account tried is a trial charged to the hypothesis family. Choosing the account
  size after seeing results is otherwise a maximum over sixteen draws reported as
  an expectation — the same error as quoting the best grid combo's Sharpe.
  `personal_*` gets **risk of ruin against a pre-declared threshold**, never a
  synthetic drawdown limit: inventing a failure mode to make a comparison table
  look uniform measures the invention.
  **Refusals recorded rather than guessed** (twelve, in the spec's §8): CME
  performance bonds are not encoded at all — CME's site blocks automated access
  and its rates change by clearing advisory, so `[margin]` is empty and says why;
  TPT's micro ratio, TPT's flat-by time (their own copy says both "5PM CT" and
  "5PM EST", and 17:00 CT is the Globex *open*), and three firms' target-basis
  wording are absent or encoded as the stricter reading with the ambiguity
  written down. Take Profit Trader's PRO rules **prohibit automated trading
  outright**, so those files are a reference point rather than a target, and that
  is a fact about the product, not a caveat about the model. Ten baseline
  figures were corrected against the firms' pages, including Apex's 150K drawdown
  ($4,000, not $4,500 — corroborated by their own $154,100 safety net) and
  Topstep's consistency rule (best day ≤ 50 % of the *profit target*, not 30 % of
  total profit).
- **D-0068** (2026-07-30) — **D-0066 as shipped: the symbol predicate is split in
  two, `manifest.jsonl` gains a second line kind, and the archive's 21,736
  missing symbols are credited by 8 appended supplement records. Repair
  complete — `sym_audit` reads 0 missing and 0 dropped over 108,696 observed
  symbols.** *Implements D-0066; changes two of its details, both recorded
  below.*
  **The predicate.** `validate_symbols` is gone, replaced by two rules with
  different jobs. `validate_coverage_symbol` — non-empty, ASCII, no control
  characters — governs a record's `symbols[1..]` and every
  `CoverageRequest.symbols` member: spaces and colons pass verbatim, because
  `CL:BF F0-G0-H0` is a real CME spread name and no member of those lists is
  ever a path component. `validate_symbol_key` adds full path safety (no
  separator, no colon, no whitespace, not `.`/`..`) and governs `symbols[0]`,
  the requested key, whose letter now matches `validate_file_path`'s
  per-component rules exactly — so a key this accepts can always be archived,
  and one it rejects is rejected before money moves rather than after. The
  colon ban is new for keys (the old rule banned only separators and
  whitespace); it is inside D-0066's "full path safety" because
  `validate_file_path` has always refused colons, and no existing key contains
  one. Public API: `is_valid_symbol` now means the coverage rule (its callers —
  `execute::merge_symbols`, `sym_audit` — want exactly that), and
  `check_symbol_key` is new, returning `Result` rather than `bool` because its
  caller refuses a pull over it and a refusal must say why.
  **Difference 1 from D-0066: the money guard moved instead of narrowing in
  place.** D-0066 read as though `coverage` would keep the strict rule for
  `symbols[0]`. It cannot: a coverage *query* has no key — its first element is
  just the first symbol someone asked about — and specific (3) requires
  `coverage("CL:BF F0-G0-H0")` to answer rather than refuse. So `coverage` now
  applies the loose rule to every member, and the strict check lives in
  `ingest::plan`, which is the one place a symbol becomes a directory name. It
  runs *before* the free `dataset_range` call, so an unarchivable key now costs
  nothing at all, where D-0020's placement made it cost a coverage round trip.
  D-0020's property is preserved and strengthened: `plan.rs`'s new control
  asserts the provider is never touched.
  **The supplement record.** `manifest.jsonl` now holds `ManifestRecord` lines
  and `SymbolSupplement` lines: `{schema_version, supplements_blake3,
  added_symbols, source, recorded_ts, reason}`, where `supplements_blake3` is
  the target's manifest id (D-0014) and doubles as the **structural
  discriminator** — no record has that field, no supplement has `file_path`, and
  both are `deny_unknown_fields`, so neither can be read as the other. A reader
  that knows nothing of supplements therefore *hard-errors* rather than
  under-reporting coverage, which is the failure being fixed; that loudness is
  why the supplement went into `manifest.jsonl` rather than into a side file
  beside it. `source` is a closed enum (`dbn_metadata`), not free text, for
  D-0049's reason. `coverage` unions record symbols with supplements naming the
  record's id, so a caller sees one truth, while `records()` still returns
  exactly what acquisition time wrote and `supplements()` says what was learned
  later. Corrections add only: a supplement pointing at an id no *earlier* line
  carries is a hard error at load and at append (a dangling correction credits
  nothing, silently), one adding nothing already credited is refused (an
  append-only log earns trust by having no noise in it), and identical bytes at
  two paths share an id and are both credited — their DBN headers are the same
  bytes, so their symbology is the same symbology. `verify` ignores supplements
  because they name no bytes, and says so in its output.
  **Difference 2 from D-0066: `sym_audit` grew a column, because "0 dropped"
  turned out not to be the completion proof.** Loosening the predicate takes
  `dropped` to 0 on its own, while the 8 old records still credited 21,736
  symbols fewer than their files declare — so the audit would have printed the
  success number over the unfixed state. It now reports `missing` (observed
  minus credited, the number `coverage` would re-buy) beside `dropped` (what the
  predicate refuses, the mechanism), and reads the archive through
  `Catalog::open` + `credited_symbols` instead of parsing manifest lines itself:
  one parser, the production one, per D-0066 specific (2).
  **Executed on the live archive** with `crucible symbol-supplement --execute`
  (new command, dry run by default, exit 4 when it finds something like `verify`
  and `qa`): 8 lines appended crediting 2,307 CL.FUT + 3,127 ZN.FUT symbols per
  schema across `ohlcv-1m`/`ohlcv-1s`/`definition`/`statistics`, 0 unrecordable,
  0 undecodable. The 33 original lines occupy the same 1,167,298 bytes they did
  before the append. Post-repair: `sym_audit` 108,696 observed / 108,696
  credited / 0 missing / 0 dropped; `layout-check` clean; `verify` clean; demo
  hash `b55747513df596ed`, unchanged. One test's semantics changed on purpose:
  `coverage_rejects_invalid_request` used to assert `"ESU6 "` was refused, and
  now asserts a spaced vendor symbol is *answered* — that assertion was the bug,
  written down.
- **D-0069** (2026-07-30) — **Stops and targets ride along with the order that
  opens the position, and one named convention — `stop_first_intrabar` — decides
  what an OHLC bar refuses to say. Ambiguous bars are counted and printed, not
  refused.** M2's "worst-case intrabar ordering; flag path-sensitive results".
  *The problem is real and unavoidable.* A bar records four prices and no
  ordering. If a position's stop and its target both sit inside `[low, high]`,
  the bar is equally consistent with either having printed first, and the two
  readings differ by the entire width of the bracket — on the fixture in
  `crucible-engine/tests/bracket_golden.rs`, $250 on one bar. Every bar-based
  backtest picks a rule; the failure mode this project exists to prevent is
  picking one *silently*. So the rule has a name, a module that is its
  specification (`crucible-engine::bracket`), and a count that travels with
  every result it touched.
  **The rule, in three steps.** (1) *The opening print is known, and it wins.*
  The open is the bar's first trade by definition, so a level the open already
  passed was reached before anything else could happen — and the fill is **at
  the opening print, never at the level**. A bar that opened three points
  through your stop never offered your stop; filling there manufactures a price
  the market did not print, which is a worse sin than being wrong about
  ordering, because it is wrong about *prices*. This resolves gaps in both
  directions: a gapped stop is worse than its level, a gapped target is better
  than its level, and both are simply what traded. Rule 1 also settles the case
  that looks like a counterexample to "stop first" — a bar opening through the
  target fills the **target** even though the low later reached the stop,
  because the alternative asserts the price passed a resting limit without
  filling it, which is not pessimism, it is impossibility. (2) *Otherwise, both
  legs touched ⇒ the **stop** fills, at its level.* Worst case for the strategy,
  chosen because the ambiguity is genuine: a number handed to a reader must be
  the pessimistic end of the range the data permits. (3) Otherwise whichever
  single leg was touched, or neither. Only branch 2 increments the
  path-sensitivity count — counting gaps would drown the signal in bars whose
  path the open already settled.
  **Rejected: resolve ambiguous bars by proximity to the open** ("whichever
  level is nearer the open probably printed first"). It is intuitively
  appealing, it is what several retail backtesters do, and it is unusable here —
  it resolves roughly half of ambiguous bars *in the strategy's favour*, so a
  strategy can be paid for a fact the data does not contain, and the payment
  grows as the bracket tightens. A tight-bracket grid would show a plateau that
  is an artifact of the tie-break. Also rejected: a seeded coin flip
  (reproducible, but it makes the result a function of the seed, and §2.2's
  determinism is not the same thing as honesty), and **refusing to replay
  ambiguous bars at all**. Refusal is right when a wrong answer is
  unrecoverable and a re-run is cheap (D-0029, D-0033); it is wrong here,
  because ambiguity is not a defect in the input to be fixed upstream but a
  permanent property of OHLC data. A 1m ES series has ambiguous bars for any
  bracket narrow enough to be interesting, so refusing means refusing the
  feature. Counting is what makes it honest: `combo`, `walk-forward` and
  `backtest` all print the count beside the fill model, and `backtest` escalates
  to a loud block naming the share and what a target-first rule would have paid
  instead. A run whose PnL turns on many ambiguous bars is one to distrust, and
  the report says so in those words.
  **At the level exactly, the two legs are deliberately asymmetric**: a stop
  triggers on `low <= stop` (a print *at* the stop proves the market got there,
  and a stop is a market order from that instant), a target needs
  `high > target` (a print at a resting limit proves nothing — there was a queue
  in front of it). Both halves are the pessimistic reading, which is why they
  point opposite ways, and the target's strictness is the only defence this
  layer has against the queue-position optimism that keeps `OrderKind::Limit`
  out of the engine until M4's `queue_sim`. It is a floor on the pessimism, not
  a proof.
  **Offsets from the fill price, not absolute levels, and the bracket is live on
  the bar its parent filled on.** A strategy cannot know its entry price when it
  places the entry (§2.1), so absolute levels would have to be computed from a
  price it hoped for, or installed a bar late — leaving every entry naked for
  exactly one bar, which is the bar a stop is for. Riding along with the parent
  means the levels are the ones the position actually got, and the bracket
  inherits the parent's `placed_ts`, so §2.1 still holds: it is only ever tested
  against events strictly after the event that asked for it. Replay step 1
  splits into 1.1 (market orders, at the open) then 1.2 (the bracket, against
  this bar's range) — in that order, because the open is the first trade and a
  resting level cannot be touched before it. A bracket is
  one-cancels-the-other, is dropped when the position goes flat or flips, and
  exits the whole position. Consequence worth naming: under `spread_cross`, a
  stop closer than the half-spread lands at or beyond the opening print and
  stops out immediately. That is the fill model being honest — you bought the
  offer and put your stop on the bid.
  **`FillModel` gains a second *required* method, `fill_protective_exit`.** Not
  defaulted: a default would price every bracketed strategy's exits under an
  assumption nobody named, and §2.4 does not allow that to exist. The two legs
  are not the same trade — a stop is a market order once touched and crosses the
  spread, a target is a resting limit the market came to and does not — so
  `spread_cross` charges the half-spread on the stop leg only, and the
  commission on both. `FreeFills` charges nothing on either, because a
  half-costed screening model is worse than a frankly uncostless one (D-0006).
  Ordering lives in the engine and costing in the fill model, so two fill models
  can never disagree about the path a bar took.
  **Not a new fill model name.** §4's `free_fills` / `spread_cross` /
  `queue_sim` list is unchanged; this is an orthogonal named assumption that
  applies inside all of them, and it is reported next to the fill model rather
  than folded into it. **Not yet a config axis, either**: the combo grammar
  cannot declare a bracket in this build, so `combo` and `walk-forward` print
  zero and say *why* it is zero, and brackets reach a replay through
  `crucible backtest --stop-ticks/--target-ticks` (or `strategies::Bracketed`,
  the decorator that gives every strategy the same protective behaviour from one
  implementation, as `Aligned` does for warmup). Grid axes over stop distance
  are an S1 question and land with the funnel; the capability and its convention
  land first, with the three determinism hashes (demo `b55747513df596ed`, combo
  `0e1ab52d474b862b`, walk-forward `711e1cb34a2ee2b4`) provably unmoved.
  **Negative controls, per §7.** Every branch has a fixture with hand arithmetic
  and the counterfactual value written out, and the detector was watched
  failing: disabling the `path_sensitive` flag fails four tests, making the rule
  target-first fails the stop-first fixtures with exactly the $100,150 their
  comments predict, and filling a gapped stop at its level instead of the open
  fails with exactly the $99,900 its comment predicts. A bracketed run with no
  ambiguous bar reports zero, and an unbracketed run reports zero however
  violent its bars — a flag that fired on every exit would be
  indistinguishable from one that worked.
  **Ratified 2026-07-30, with two framings the original entry left implicit.**
  (a) *Stopping out on the installation bar is the CONSERVATIVE direction.* It
  penalizes and never flatters, so it composes with the worst-case intrabar
  doctrine rather than sitting awkwardly beside it, and it violates nothing in
  §2.1 — no information from the future is consulted, only the spread the
  position already paid. A sell stop one tick under an offer you just lifted is
  resting at the bid, and a real market fills it immediately; the engine
  agreeing is microstructure, not a modelling artifact. (b) *Grids must KEEP
  sub-spread stop distances rather than excluding them.* The point of the
  behaviour is that the cost-sensitivity sweep shows that region **being
  dominated, as data** — a reader can see the stop distances that cannot
  survive their own spread. Filtering them out of the grid would replace that
  evidence with the absence of evidence, and quietly: the region would not look
  bad, it would look like nobody tried it. An axis is allowed to contain points
  that lose.
- **D-0070** (2026-07-30) — **A price is valid iff it is not `UNDEF_PRICE`; the
  `> 0` half of the test is deleted. Spread instruments are excluded from the
  curated set by a DECLARED FILTER that counts what it excludes, not by a
  refusal. `transcode`'s "already curated" lookup becomes set-shaped.**
  *Narrows what §9's "`transcode` refuses a whole file over one bad record"
  counts as bad; the refusal itself is untouched.*
  **The measurement.** A recon pass decoded every `ohlcv` record in the archive:
  **103,201,649 of 1,164,446,426 bar records (8.9 %) were being refused**, every
  one of them legitimate. `GC.FUT ohlcv-1m` refused at record **#0**, `ES.FUT
  ohlcv-1m 2010-06-06--2024-01-01` at record #14. One line did it —
  `if value <= 0 { return Err(...) }` — and because validation runs *before* the
  `--symbols` filter, no invocation could work around it. The full archive was
  therefore untranscodable, which is the whole M1 exit path.
  **Why the predicate was always wrong, not merely strict.** Two independent
  reasons, either sufficient. (1) *Outrights go negative.* CL settled at
  **−$37.63 on 2020-04-20**; the rule refused the single most-studied session in
  the archive, and would have gone on refusing every future one. (2) *A spread
  prices a difference*, and a market in contango prices it below zero — that is
  what most of the 8.9 % is. Zero is likewise a price, not an absence: the
  vendor already has a way to say "no price here", and it is `UNDEF_PRICE`.
  Nothing downstream ever needed positivity: `curated::read` has no such check,
  `qa`'s spike detector compares adjacent closes by **difference**, and
  `continuous` back-adjusts **additively** (D-0042 chose that for determinism
  and bought negative-price safety for free). `Price` is a signed i64 and
  `ContractSpec::pnl_nano_usd` is linear in it, so nothing in accounting cares
  either — proved rather than assumed, in
  `crucible-engine/tests/negative_prices.rs`: a hand-derived short from 9.99
  through the −37.63 settle to a −36.99 cover, its exact mirror on the losing
  side, and the tick-rounding control (`round_to_tick` rounds half *away from
  zero*, symmetrically; half-up would have quietly moved every negative fill).
  **Why the spread exclusion is a filter and not a refusal.** A parent window
  resolves to far more spreads than outrights — `GC.FUT` to 12,782 symbols of
  which 12,661 are spreads, `CL.FUT` to 7,120 of which 6,905 — and nothing in
  this project replays one. Refusing them would be a category error: a spread is
  not a record this build cannot read, it is a record nothing reads yet, and
  refuse-the-whole-file stays reserved for genuine corruption. So
  `TranscodeOptions::include_spreads` (default **false**, `--include-spreads` on
  the CLI) excludes them, and every excluded record is **counted** —
  `spread_records_skipped` and `spread_instruments_skipped`, per manifest record
  and as a run total, printed per source window and in the summary. The summary
  line prints on every run *including when the count is zero*, for D-0069's
  reason: "nothing was excluded" and "exclusions were not counted" must not read
  identically. Cost of the default being wrong: one rebuild. `raw/` keeps every
  spread forever (D-0017) and curated data is disposable (D-0036), so if
  calendar-spread research ever arrives the flag flips and the tree is rebuilt.
  **The predicate: a symbol names a spread iff it contains `-`, `:`, or a
  space.** Derived from the archive, not guessed. All 27,099 distinct symbols
  the manifest carries — which after D-0068 is every symbol the DBN headers
  declare — fall into exactly three buckets: 5,434 contain `:` (and every one of
  those also contains a space) such as `CL:BF F0-G0-H0` and
  `UD:ZN: TL 0110987001`; 21,044 contain `-` and every one splits into exactly
  two alphanumeric legs (`RTYU7-RTYZ7`); and 614 are plain `[A-Z0-9]+`, of
  length 4–5, **all 614** matching root + month code + 1–2 year digits. Nothing
  falls outside. A marker test and a positive outright test therefore agree
  exactly on this archive, and the marker test is the one used **because of
  which way each fails**: the default excludes, so calling an outright a spread
  silently omits real bars, while calling a spread an outright writes a
  partition nobody reads. "Contains a marker no outright contains" errs toward
  writing; "fails to match the outright pattern" errs toward dropping.
  Rejected alternative: `InstrumentDefMsg::instrument_class`, the vendor's own
  authoritative answer. It lives in the `definition` schema — a different file —
  so using it would make transcoding a bar window depend on having also bought
  `definition` for that root and span, and would add a cross-file join whose
  failure mode is silence. Recorded in the module docs as the upgrade if
  `definition` coverage ever becomes universal.
  **Two refusals kept, one added.** `UNDEF_PRICE` and the alignment rule stand,
  with controls asserting each still fires (the mutation that deletes the
  `UNDEF_PRICE` check must fail a test, and does). Added: naming a spread in
  `--symbols` while the filter is on is refused up front rather than answered
  with an empty report — it would decode the whole archive to write nothing, and
  an empty result in the shape of a finished one is worse than a refusal that
  costs a flag. It exits 2 (usage), not 4.
  **`TRANSCODER_VERSION` deliberately NOT bumped**, though this changes DBN→bar
  semantics. Every bar this makes writable is a bar the old build refused the
  *whole file* over, so no existing curated file contains one; and every bar the
  old build did write is byte-identical under the new predicate. A bump would
  warn on every already-correct partition (71 of them at the time of writing)
  while distinguishing nothing — exactly the cosmetic bump D-0036 says must not
  invalidate an archive.
  **The scan, measured.** `already` (which instruments this window has already
  curated) was a `Vec` scanned linearly once per bar record. On a first run it is
  empty and free; on a **no-op re-run** it is at its fullest, so the cheap
  idempotent re-run was the expensive case. Measured on `GC.FUT ohlcv-1m`
  (15,531,201 records) with `--include-spreads`, so `already` holds 1,428
  entries: linear scan **17.30 / 17.38 / 18.89 s**, `BTreeSet` **4.17 / 4.18 /
  4.31 s** — **4.1×**, and the scan was ~74 % of wall time. The re-run had been
  **1.6× slower than the original transcode that produced it** (11.47 s), which
  is the property that makes any resume loop or retry look hung; it is now
  0.37×. Honest qualification, because the measurement outranks the reasoning
  that predicted it: on the **default** path the spread filter removes the
  records before the lookup, so `already` holds 120 rather than 1,428 and the
  two are indistinguishable (4.71 / 4.95 s vs 4.81 / 5.03 s). The set matters
  under `--include-spreads` and on the wide `ohlcv-1s` windows; the filter is
  what fixes the common case. `BTreeSet` rather than `HashSet` for §2.2.
  **Executed on the live archive.** The two records that previously refused at
  the very start now transcode: `RTY.FUT ohlcv-1m` exit 0, 1.48 s, 76.6 MB peak
  working set, 38 partitions, 3,334,405 bars, 209,753 spread records across 61
  spread instruments excluded; `GC.FUT ohlcv-1m` (which refused at record #0)
  exit 0, 7.19 s, 278.2 MB peak, 120 partitions, 10,275,830 bars, 5,255,371
  spread records across 1,308 spread instruments excluded. Note 1,308 spreads
  *traded* against 12,661 *mapped* — D-0036's point restated. Determinism
  unchanged: demo `b55747513df596ed`, combo `0e1ab52d474b862b`, walk-forward
  `711e1cb34a2ee2b4`. One test's semantics changed on purpose:
  `a_nonpositive_price_is_refused` is replaced by
  `a_zero_or_negative_price_is_written_not_refused` — that assertion was the
  bug, written down.
  **Deliberately not done, and left to arbitration.** The ruling asked for the
  count "in the inventory of the curated set" as well as in the report. There is
  no such artifact: the curated set's only self-description is each partition's
  Parquet footer, and a spread count belongs to the *source window*, not to a
  file whose rows are one outright's. D-0036 defines that footer as "everything
  about a partition that is fixed before the first row is written", and this
  number is only known after the last. Putting it there would also mean either a
  schema bump (hard-refusing every partition already on disk) or an
  absent-key-tolerated read. So the count lives in the transcode report, and the
  durable record of the choice is the flag's default plus this entry. One
  optional metadata key away if that is judged wrong.
- **D-0071** (2026-07-30) — **The account-evaluation series are CAPTURED inside
  the mark loop, the trading day arrives as caller-supplied keys, and the
  retained artifact is 56 bytes per session rather than 16 bytes per bar.**
  Implements `docs/ACCOUNT_EVAL_SPEC.md` §3 (D-0067). Capture only: the breach
  test, the block bootstrap, P(pass) and payout cadence are §4 and land with the
  funnel in M3. `crucible-engine::series`, plus two fields on `ClosedTrade`.
  **Capture, not reconstruction, and now with a number attached.** The intraday
  high-water folds the same equity value the curve takes, on the same line of
  `replay.rs` step 2. Rebuilding it from OHLC afterwards re-opens the intrabar
  ordering `stop_first_intrabar` already settled (D-0069), and the drawdown it
  computes is the drawdown of a path the account never took. §5.12's control is
  implemented and was watched: on a bar touching both a stop at 98.00 and a
  target at 103.00 from a long filled at 100.00, the capture reports peak $0 /
  max drop $100 and a target-first rebuild reports peak $150 / max drop $0 —
  the whole drawdown, plus a $150 peak that never existed, out of one
  unknowable ordering. On a single-tick fixture where the stop still fires, all
  three series agree exactly, which is what makes the divergence attributable to
  the ambiguity rather than to a bug in the rebuild. Flipping the engine's own
  convention to target-first makes the first control fail with exactly the
  $150/$100 its comment predicts.
  **The trading day is data (the fourth application of the D-0015 device).**
  `crucible-engine` depends on `crucible-core` only, and a trading day is an
  exchange fact that lives in `crucible-data::calendar`. So `AccountCapture`
  takes `day_keys: &[i64]` — one nondecreasing
  `days_from_civil(Calendar::trading_day(avail_ts))` per bar — which is
  *literally the slice* `walkforward::folds::FoldPlan::build` already takes.
  Not a trait object (that smuggles the calendar in behind a v-table), not a
  boundary derived from timestamps inside the engine (that re-implements the
  17:00 CT roll in the one crate forbidden to know about it). No dependency edge
  moves. **One producer, both consumers**, and the reason is concrete: two
  independent attributions of "which day" is how a daily-loss-limit breach lands
  on a different date in two reports, with neither report looking wrong on its
  own. `crucible-cli::walkforward::trading_days` is the producer; the funnel
  cannot call the calendar either, so `cli` is the only layer that can.
  The reconciliation is asserted (`day_slicing_and_fold_attribution_reconcile_
  to_the_nanodollar`: every evaluable day's `close_pnl_nano_usd` equals the fold
  machinery's window delta, exactly) and was watched failing — handing the
  capture wall-clock keys reports "day 1 occupies different bars in the two
  consumers: 3..9 vs 6..12". The wall-clock counter-fixture manufactures two
  −$300 sessions (−$600 and −$400) out of a run whose worst calendar session was
  −$150, purely by cutting each episode at its intraday high.
  **A day opens at the previous day's CLOSE, not at its own first mark.** The
  overnight gap is a real move of the account; anchoring on the first mark makes
  it invisible in both excursions and breaks §3.3.1's recursion, which is what
  lets a whole-day bootstrap answer an intraday question. The within-day
  running peak is seeded at that same open — conservative, and inventing
  nothing, because the open is a level the account genuinely stood at.
  **Memory, derived rather than asserted.** `HighWaterState` is 16 bytes and
  O(1) in the bar count: two integer comparisons per mark, never a `Vec`.
  `DayRecord` is 56 bytes (`i64` key + `Range<usize>` + four `i64`), so 252 × 16
  sessions is **226 KB** — against 4.98 GiB for a per-bar equity vector at a
  16-year 1-second grain. `ClosedTrade` grows 16 → 40 bytes, ≤ 2 MB for 50,000
  round-trips. Both sizes are pinned by test, because a field added without
  thought should fail a test rather than become a memory regression nobody
  measures. What is deliberately **not** done: spec §3.3's requirement 2, the
  per-bar equity vector's opt-out. That is a change to an artifact the
  walk-forward runner consumes, not a capture hook.
  **The one approximate case is counted, not hidden.**
  `intraday_peak_crossing(days, level)` returns the day on which the running
  intraday peak first reaches `level` — M3 passes `L + D` — and
  `approximate_day_count` is 0 or 1, never more, because a running peak is
  non-decreasing and crosses a level once. The crossing day is *always* flagged:
  a day's `peak_from_open` is by definition at least its `close_pnl`, so a
  summary can never certify that the peak first reached the level at the day's
  final mark rather than earlier inside it, and certifying it anyway would be
  resolving an ambiguity in the flattering direction. An account whose ratchet
  is `highest_daily_closing_equity` advances its peak only at a close, so its
  lock engages at a close and it has no approximate day at all.
  **`f64` appears nowhere in any of it.** Percentiles use nearest-rank on
  integers — interpolating between two days would put a float where a dollar
  amount belongs, and the answer is meant to name a day that happened.
  **Capture is additive observation, and the hashes prove it.** `demo
  --hash-only` `b55747513df596ed`, `combo --run --hash-only`
  `0e1ab52d474b862b`, `walk-forward --hash-only` `711e1cb34a2ee2b4` — all three
  unchanged, and `capturing_changes_no_number_in_the_result` asserts a captured
  run and a plain run produce identical `BacktestResult`s field by field.
  **Every control was watched firing** (§7), and the record says which mutation
  each one caught. Thirty defects were planted one at a time in
  `series.rs`, `portfolio.rs`, `replay.rs`, `bracket.rs` and
  `calendar/mod.rs`, each run against the whole audited set: the nanodollar
  off-by-one in the decline fails five (three unit, two integration); deleting
  the trough update fails four; anchoring a day on its own first mark fails
  two; a day's bar range off by one fails four including the fold
  reconciliation; never recording the day's closing level fails seven,
  including the planted daily-loss control. Flipping `stop_first_intrabar` to
  target-first fails **both** halves of §5.12 that should move — the
  divergence control and the third-side control — and leaves the single-tick
  control green, which is the two-sided/third-side structure working. Deleting
  the 17:00 CT roll from the calendar fails both §5.6 controls. Making the
  capture branch touch the portfolio fails
  `capturing_changes_no_number_in_the_result` and nothing else.
  **One control did not fire, and that is part of this record.**
  `a_flip_resets_the_excursions_with_the_episode` stayed green with both
  excursion resets deleted from `apply_fill`: on its fixture the second
  episode's MAE is deeper than the first's and both MFEs are zero, so the
  running extremes are identical whether or not they reset. It asserted the
  right numbers and detected nothing.
  `a_flip_does_not_inherit_the_previous_episodes_excursions` was added beside
  it — first episode −$1,000/+$1,000, second −$50/+$100, so inheriting either
  side is visible — and the same mutation fails it. This is what §7's
  no-quality-exemption clause buys: the gap was invisible to `fmt`, `clippy`,
  a green `cargo test --workspace --all-features`, and all three determinism
  hashes.
- **D-0072** (2026-07-30) — **The curated partition key is an expiry-resolved
  contract with a four-digit year (`curated/bars/GCZ2014/`), not the vendor's
  raw symbol. A bar whose contract cannot be resolved refuses the file.**
  *Supersedes D-0036's `{instrument}` where that instrument is a futures
  contract; the file naming, the footer metadata, and the one-file-per-source-
  window rule are untouched. Narrows D-0046's "the constant remains what curated
  bar partitions parse against" — that clause was the bug.*
  **The measurement.** A CME year code is one digit, so it repeats every ten
  years; every bar window in this archive is sixteen years long (2010-06-06 →
  2026-07-28). By pigeonhole, contracts alias. They did: `GC.FUT ohlcv-1m` wrote
  exactly **120 partitions = 12 month codes × 10 year digits**, and there was not
  one two-digit year code anywhere under `curated/bars/`. `crucible qa
  --instrument GCZ4 --timeframe 1m` reported `span 2010-06-08T17:09:00Z ..
  2024-12-27T18:03:00Z (310234 bars)` — a December-2014 gold contract does not
  trade for 14.5 years. That file was Dec-2014 gold and Dec-2024 gold
  concatenated. Decoding all seven roots' `ohlcv-1m` windows against their
  `definition` files puts a number on the scope: **101 of GC's 120** raw outright
  symbols have a first and a last bar belonging to *different* contracts, 111 of
  CL's 136, 31 of NQ's 40, 31 of 6E's, 28 of ZN's, 20 of ES's — and RTY's 0,
  because RTY only lists from 2017. `layout-check` finds **173** offending
  directories in the live tree, which is every outright one.
  **Why both existing detectors were structurally blind, which is the part worth
  keeping.** `PartitionWriter::push` enforces strictly increasing `ts_open` and
  passed, because the two contracts trade in *disjoint sequential* periods and
  concatenate in perfect order — ordering is a statement about neighbours,
  aliasing is a statement about identity, and no ordering check can see it.
  `qa`'s gap detector reported "gaps inside sessions none" for two independent
  reasons: no bundled calendar claims GC, so `qa` had no definition of "expected"
  and skipped coverage entirely (it printed `calendar none`); and even given a
  calendar, a ten-year absence is whole missing *sessions*, which is a coverage
  number, not a gap *inside* one. Both facts are now asserted as tests on the
  merged fixture (`transcode::blind_detector_controls`), so the record says which
  detectors could not have caught it rather than leaving it to be rediscovered.
  **The device, reused rather than paralleled.** `expiry.rs` already solved this
  exact ambiguity on the `definition` path (D-0046, amended): a 16-year file
  contains `ESM0` twice, and the record separates them by resolving the one-digit
  year against **the contract's own expiry** instead of a constant. The `ohlcv`
  path now uses the same rule through the same types — `ContractCycles::resolve`
  takes a vendor symbol and a bar's `ts_open` and answers which cycle it printed
  in. No second contract-symbol type was invented; `ContractSymbol`,
  `DecadeAnchor` and `parse_parts` are shared, and the decode of a `definition`
  file is one pass with two policies over it. A `DecadeAnchor` constant is **not**
  a fallback and must never become one: it has an answer for `GCZ4` and it is
  right for half the bars, which is precisely how this shipped.
  **The window rule.** A contract owns `(previous same-family expiry, its own
  expiry]` — same root, month code and year digit, so the previous member is
  exactly ten years earlier. The windows tile with no gap and no overlap, so the
  answer is unique when it exists. A family's earliest member opens
  `CONTRACT_CYCLE_DAYS` (3,653 — the most days ten Gregorian years hold) before
  its expiry; that bound is load-bearing only there, and nothing legitimate comes
  near it, because the longest CME listing horizon is crude oil's nine years.
  Measured: same-family expiry gaps in this archive run **3,647 to 3,657** days
  (the expiry date drifts within its month), which is exactly why the
  multi-member case uses the neighbour's expiry rather than the constant. **Every
  outright bar in the archive resolves** — zero unresolved records across all
  seven roots.
  **The refusal, and why refuse-the-whole-file is right here and nowhere else.**
  No expiry, or no cycle containing the bar, refuses the source file
  (`TranscodeError::UnresolvedContract`). D-0070 made spread exclusion a
  *declared filter with a count* on the argument that a spread is not corrupt —
  it is a record nothing replays yet. This is the opposite case: a bar filed
  under the wrong contract is corruption of *meaning*, it looks exactly like
  correct data, and the silent path is what produced the defect. A missing
  `definition` therefore fails loudly. Curated data is disposable, so a refusal
  costs one re-run. All seven parents hold a 16-year `definition` file, so the
  join is satisfiable today.
  **What is deliberately NOT resolved.** A symbol that does not parse as an
  outright keeps the vendor's spelling — which is D-0070's rule unchanged, since
  this predicate gates a *rename* and an unrecognised shape must stay visible
  rather than be refused. Every spread lands there (`ESH4-ESM4`,
  `CL:BF F0-G0-H0`), so a spread partition written under `--include-spreads`
  still carries the vendor's decade ambiguity. Bounded and stated rather than
  hidden: nothing in this project replays a spread, the flag is off by default,
  and making spreads replayable means resolving their legs first. Two- and
  four-digit years are absolute and need no lookup at all — the vendor itself
  switches to two digits for far-dated listings, and 16 such contracts
  (`CLZ30`…`CLZ36`) trade in the CL window.
  **Why four digits and not two.** Two would be arithmetically unambiguous and
  still ambiguous to a reader: `GCZ14` is one character from the vendor's `GCZ4`,
  and `CLZ36` is a real vendor spelling meaning 2036, so a listing mixing our
  keys with the archive's would depend on knowing which convention wrote each
  name. Four digits cannot collide with a CME year code, which has at most two.
  `ContractSymbol`'s `Display` renders it and `parse` accepts 1, 2 and 4 digits —
  a strictly widening parser, landing in the same commit as the writer, with the
  four-digit form floored at 1970 so `ZN1234` does not parse as root `Z`, month
  `N`, year 1234.
  **A latent regression this found and fixed.** `calendar` carried its own
  contract-recognition rule ("a month code and one or two year digits"), a third
  spelling of the same idea. Under it `ESH2024` stopped being an ES contract,
  **no calendar claimed it**, and `backtest` silently fell back to measuring
  `bars_per_year` from the sample — changing the annualization factor and
  therefore every Sharpe (D-0039), with nothing failing. `Calendar::governs` now
  takes the root from `parse_parts`. One parser, one answer, and a control
  asserting all three spellings of one contract reach the same calendar.
  **CLI ergonomics.** `backtest --instrument ESH4` still works and prints
  `ESH4 -> ESH2024`, because a shorthand naming exactly one curated contract is
  not ambiguous. One that names two **refuses and lists them**: on the live
  archive `--instrument GCZ4` now says "names 2 curated contracts: GCZ2014,
  GCZ2024". Answering it with either would be this same bug moved from the
  archive into the CLI. CLAUDE.md §10 is switched to the canonical spelling so
  the documented command stops depending on how much of the archive is
  transcoded.
  **Executed on the live archive.** The contaminated tree was deleted (214
  directories, 230 files, 241 MB — 173 aliased outrights, 41 spreads; `raw/`
  untouched) and `GC.FUT ohlcv-1m` rebuilt: **221 partitions, 10,275,830 bars,
  5,255,371 spread records across 1,308 instruments excluded** — bar and spread
  counts *identical* to the run recorded in D-0070, so the data was re-keyed and
  not re-counted. `GCZ2014` spans **2010-06-08T17:09:00Z .. 2014-12-29T17:44:00Z
  (145,850 bars)** and `GCZ2024` spans **2019-06-21T11:14:00Z ..
  2024-12-27T18:03:00Z (164,384 bars)**; 145,850 + 164,384 = **310,234**, exactly
  what the merged `GCZ4` held, and the merged span was precisely the earlier
  contract's start joined to the later one's end. `layout-check` is clean. The M1
  acceptance run reproduces bit-identically — 30,167 bars, −23.51 %, 665 round
  trips, 27.1 % win rate, $76,486.25 — and all three determinism hashes are
  unmoved (demo `b55747513df596ed`, combo `0e1ab52d474b862b`, walk-forward
  `711e1cb34a2ee2b4`), because `configs/combo-smoke.toml` declares `SYN:RW`,
  which carries no year to resolve.
  **Found and left alone.** `expiries_from_definitions` — the *roll* reader —
  refuses the real `GC.FUT` definition file, because `GCX1` is defined twice with
  expiries **one hour apart** (`6EM23`: seventy-two hours). D-0046 chose that
  refusal deliberately, and a calendar roll firing on a different session is a
  real consequence, so it is not weakened here. The *cycle* reader tolerates it
  and refuses only `ExpiryYearConflict`, two expiries in different years, where
  the identity itself becomes unknowable — measured at zero across all seven
  roots. Reported rather than hotfixed: `crucible rolls --expiries auto` on GC
  still exits 4, and superseding D-0046 is its own decision.
- **D-0073** (2026-07-30) — **A half-spread is a DISTANCE, not a tick count, and
  a fractional one deliberately lands off the tick grid.**
  `SpreadCrossFills.half_spread_ticks: i64` became
  `half_spread_nano_points: i64`, constructed by `from_ticks(n, tick)` or
  `from_tick_halves(h, tick)`.
  **Why:** §2.4 makes the cost-sensitivity sweep — 0 / **0.5** / 1 / 2 ticks —
  mandatory on every scorecard, and an `i64` tick multiple cannot express the
  middle level. Carrying the field as a count made the most decision-relevant
  row of the sweep the one the type refused to hold, so the sweep would have had
  to round it to 0 or 1 and quietly delete the row that answers "does this edge
  die at half a tick?".
  **The visible consequence, stated because it is a fill price:** at a whole
  number of ticks the fill stays on the tick grid, because a bar's open is on it
  and `open ± k·tick` is too. At half a tick it does not, and that is what a
  fractional half-spread *is* — an average over sessions where the spread was a
  tick and sessions where the trade printed at the touch. No single print pays
  half a tick, and rounding it back would be modelling a cost nobody can pay by
  charging one nobody did.
  **`round_to_tick` came off the slipped price**, and that is a no-op for every
  existing result rather than a semantics change: rounding an already-aligned
  price is the identity. The proof is empirical — `demo --hash-only`,
  `combo --run --hash-only` and `walk-forward --hash-only` are **byte-identical
  to the pre-change baseline** (b55747513df596ed / 0e1ab52d474b862b /
  711e1cb34a2ee2b4), and every hand-derived golden in `golden_smoke.rs`,
  `bracket_golden.rs` and `negative_prices.rs` passes unchanged.
  **Sweep levels are integers, in half-ticks** (`0 / 1 / 2 / 4`), not `f64`
  ticks, for D-0060's reason: a level reached by floating-point arithmetic is a
  level whose existence depends on floating-point arithmetic. A config writes
  `0.5`, and `Criteria::new` refuses anything not within a billionth of a
  half-tick multiple, naming the value it refused.
  **`ClosedTrade` gained `direction` in the same commit** (40 → 48 bytes,
  re-pinned by test): the matched random-entry control has to reproduce the
  long/short *mix* of the trades it benchmarks, and a control that quietly gives
  itself a different mix measures the market's drift instead of the strategy's
  timing.
- **D-0074** (2026-07-30) — **The trial registry's backend is an append-only
  JSONL log, not DuckDB. The contract is unchanged; only the storage is.**
  CLAUDE.md §6 blesses `duckdb` for `crucible-funnel`, and it was tried first,
  before anything was written. On this toolchain `duckdb 1.10505.0` with
  `bundled` **fails to build**: the vendored amalgamation's
  `catalog_entry/list.hpp` includes
  `duckdb/catalog/catalog_entry/aggregate_function_catalog_entry.hpp`, which is
  not in the shipped tree, and MSVC 14.43 exits 2 with `fatal error C1083` after
  ~2 GB of object files. Measured, not assumed.
  **What did not change, because it is the part that matters:** insert before
  running; dedupe on `(config_hash, account_id, combo_index, fold, seed)`;
  automatic trial counting per hypothesis family; the pre-registered criteria
  stored verbatim on the row; the graveyard as a query. Those five rules are
  statements about *ordering and identity*, not about SQL, and each has a test.
  **Why JSONL is not a consolation prize:** it is the shape this archive already
  uses and already trusts — `manifest.jsonl` (D-0014, D-0017) and its second
  line kind (D-0068) — append-only, greppable, diffable, and readable by
  anything in five years. A finished run appends a second line rather than
  mutating the first, exactly as the manifest does. An unknown line kind is a
  **refusal**, never a skip: skipping would under-count the trials of whatever
  family wrote it, and an under-counted trial count is the one error here that
  flatters every deflated Sharpe downstream.
  **A trial is `(config_hash, account_id, combo_index)`** — folds of one combo
  are one trial, because charging eight would deflate a Sharpe by an artifact of
  the fold layout; a second account is a new trial, because D-0067 prices account
  selection. The **seed is in the run key** though not in the trial:
  `config_hash` is blake3 over a `ComboSpec::canonical_form` and deliberately
  does not cover `[run].seed` (D-0064), so without it two configs differing only
  in their declared seed would dedupe into each other and the second would never
  run.
  **No clock in the crate.** `started_at`, `finished_at` and `decided_on` are
  caller-supplied strings, and the CLI reads them through `SystemClock` — which
  is now the workspace's one OS-clock reader in *every* build rather than only
  under the acquisition features, making D-0032's claim literally rather than
  approximately true.
- **D-0075** (2026-07-30) — **The funnel refuses a stage it cannot run, and
  therefore cannot award `Graduate`. Its ceiling is `Iterate`, and it says so on
  every report.**
  S1 (free-fill screen) and S2 (walk-forward under costs, the mandatory sweep,
  the two controls) are implemented. **S0 and S3 are refused at config load**,
  each with a message naming what it needs: S0 buckets forward returns by signal
  quantile and needs a continuous score, which the combo rule grammar does not
  produce — its rules yield *positions*; S3 is deflated Sharpe, PBO/CSCV, the
  permutation nulls and the cross-instrument rhyme check, which is
  `crucible-funnel::stats`, still a module-doc spec.
  **Why refuse rather than skip:** a config that asks for the permutation
  battery and silently receives a fold table has been answered with a different
  question than the one it asked — and the answer looks exactly like the one it
  wanted.
  **The consequence follows from the glossary, not from taste.** §4 defines
  Graduate as "survived the full battery"; the battery is S3; so no combo in
  this build can graduate, and `assess` cannot return it. Every report and every
  scorecard says that in those words, because otherwise a reader infers that
  nothing graduated for want of merit, which is a different and much more
  flattering claim.
  **Two S3 criteria are parsed and echoed but not evaluated** (`max_pbo`,
  `require_plateau`), and the scorecard renders each as a *named hole* rather
  than omitting it: a reader who does not see a null comparison cannot tell
  "there wasn't one" from "it passed".
  **The controls are runs, not formulas**, because `PROJECT_PLAN.md` §7.4 lists
  them as denominators and a denominator computed by a different method than its
  numerator is not a comparison. The matched random-entry control reproduces the
  combo's trade count, holding lengths and direction mix and re-places them at
  seeded-random times inside each *test window* — never across a seam, because
  the gap between two test windows is a training window the pooled curve does not
  look at. It is the **median of 16 draws**: one draw is a sample of size one,
  and a strategy can lose to a single coin flip by luck. The count of draws
  beaten is printed beside it, which is the one empirical p-value this control
  can honestly give. An **absent control fails its criterion** and never renders
  as a zero.
  **`free_fills` is refused as a config's own fill model here**: the funnel runs
  that screen itself at S1, so declaring it would make S1 and S2 the same run and
  the cost sweep one number repeated four times (D-0006).
  **`funnel` exits 5 when every combo is killed** — not a failure, and not
  success either. Most ideas must die; a scheduled job that reads "everything was
  killed" as exit 0 learns nothing, which is the argument `qa`'s exit 4 already
  makes.
  **And the honest gap is recorded rather than assumed.**
  `crucible-strategies::controls::LeakyZScore` is a planted lookahead defect — a
  full-sample z-score, the failure §2.1 names by name — and
  `crucible-funnel/tests/planted_leak.rs` asserts that today's gates return
  **Iterate** for it, on a drift-free random walk where it beats both controls.
  That assertion is a record of a failure, not a goal: these gates are not leak
  detectors, and no threshold could make them into one, because a leaked edge is
  indistinguishable from a real one by any statistic computed on the leaked run.
  The day the permutation harness flips that expectation to `Kill` is the day a
  detector was watched firing on a defect planted before it existed, which is
  exactly what §7's no-quality-exemption clause asks for.
- **D-0076** (2026-07-30) — **A `Bar` carries two price views: the tradeable
  prices it always did, plus an additive `signal_offset` into signal space.
  `ContinuousFeed` is wired into `crucible backtest`, so `--instrument ES.v.0`
  and `ES.c.0` replay a stitched series. `combo` and `walk-forward` stay
  outright-only, for a new reason.**
  *Extends D-0042 (the `AdjustedPrice` barrier is unchanged and now lives in
  `crucible-core::types`); resolves the limitation `continuous::feed`'s module
  docs named as M2's; supersedes nothing.*
  **What was missing.** `ContinuousFeed` was complete and had no consumer: a
  workspace grep found no user outside its own module. M1 ticked "Continuous
  contracts v1" for the roll table, the back-adjustment and the type, and the
  replay path was never connected — `backtest --instrument ES.v.0` looked for a
  curated directory literally named `ES.v.0` and said it did not exist.
  **The seam, and why it is on `Bar` and not on `MarketEvent`.** `feed.rs`
  named the two ways to route both series to the engine: a second
  `MarketEvent` variant, or an adjusted-price channel on `Bar`. The channel
  won. `Bar` gains `signal_offset: Price` and four `signal_*()` accessors
  returning `AdjustedPrice`; indicators and rule operands read those, and
  fills, `Portfolio::mark` and `pnl_nano_usd` keep reading `open`/`high`/
  `low`/`close` exactly as before. A variant would have forced every `match`
  and every irrefutable `let MarketEvent::Bar(bar)` in the workspace — 25 sites
  — to be revisited so each could go on doing precisely what it already did,
  which is churn shaped like review. The offset is `Price::ZERO` for every
  outright contract and every synthetic series, so the two views coincide
  numerically everywhere they did before and **no golden value and no
  determinism hash moves**.
  **Why `AdjustedPrice` moved into the core.** The type was defined in
  `crucible-data::continuous::adjust` while nothing replayed a continuous
  series. Its first real consumer is the indicator set, which lives in
  `crucible-strategies` — a crate that depends on `crucible-core` only (§3).
  Leaving it in `data` meant either a dependency edge §3 forbids or a second
  "this number is not tradeable" type in the core, and a second spelling of an
  invariant is how the first one stops being enforced. The barrier is
  bit-for-bit the same: no `From`, no `Into`, no `Deref`, no `as_price()`, and
  the only exit is `as_points_f64`. `crucible_data::continuous::AdjustedPrice`
  is now a re-export, so D-0042's `compile_fail` doctest still compile-fails
  through the old path, and an identical pair sits on the definition itself.
  **Back-adjusted levels reaching a strategy is not §2.1 lookahead, and the
  reason is worth writing down.** Segment *j*'s offset is the sum of every roll
  gap *after* it, so an adjusted level does embed the future. But for a
  decision made at time *T*, every bar the strategy can see carries
  `offset_j = (gaps strictly between segment j and segment(T)) + C(T)`, where
  `C(T) = offset_{segment(T)}` is the **same constant for every visible bar**.
  Any shift-invariant function of the visible prices — a moving-average
  crossover, a return, a difference — is therefore identical to what a
  causally-adjusted series would have produced; only the additive constant
  differs, and it cancels. What does **not** cancel is a comparison against an
  absolute constant: `close > 4500` reads a level that back-adjustment took
  from rolls which had not happened. That is the reason for the scope split
  below, and it is stated on `PriceField` where a rule author will meet it.
  **Alias resolution.** `ContinuousAlias` parses the one spelling §4 pins,
  `{root}.{v|c}.0`, and refuses three things by name rather than by omission:
  `.n` (open interest is not a `RollRule` variant at all — D-0044), any depth
  but `0` (a deferred chain is built from contracts the table does not record),
  and anything that is not three dot-separated parts.
  `ContinuousAlias::looks_like` decides *by shape* which store a name points
  at, before either is opened, so `ES.n.0` gets an alias-shaped refusal instead
  of "no curated data for ES.n.0". An alias names the **rule** and never its
  parameters, so `roll_table_for_alias` searches `curated/rolls/{root}/{tf}/`
  for tables whose slug starts with the letter: none refuses and names the
  `crucible rolls` invocation that would build one, **two refuses and lists
  them** (D-0029's shape — a second candidate stops the run rather than losing
  to sort order), one is read and validated like any table off disk.
  **The window, and why narrowing is not a weakening of D-0045.**
  `ContinuousFeed::open` still refuses a replay window reaching outside the
  table's span, and a test drives it from both ends. What `backtest` does
  *first* is intersect a **date** request with the covered span and print that
  it did. `--start 2010-06-06` asks for a calendar day; the archive's first ES
  bar opens at 22:00 that day, so the request reaches 22 hours before the span
  through nothing but the difference between a date and an instant, and no
  rebuild can fix it — the table already spans the entire curated ES store. A
  request with **no** overlap still refuses, and the printed line names the
  table file, both windows, and the `crucible rolls --write` that would fix a
  table the archive has outgrown. Silence there would be the failure mode
  D-0045 exists to prevent; a printed narrowing is not.
  **`combo` and `walk-forward` stay outright-only, and the old reason is
  gone.** They refused a continuous alias because "nothing can say which of the
  two price series it wants (D-0042)". That is no longer true. The refusal
  stands on a different footing: `backtest` runs one strategy an operator
  named, while a grid expands rules it has not seen, and the rule grammar can
  write a comparison against an absolute constant, which the paragraph above
  shows is not safe on a back-adjusted series. The grammar cannot tell a
  shift-invariant rule from a level-sensitive one, so a grid would rank a sound
  combo against a leaking one. The refusal's *test* was updated to assert the
  new reason — a test asserting only "it refused" would have kept passing over
  a stale justification for as long as anyone cared to look.
  **What this does NOT fix, measured rather than described.** A roll is a
  position event, not a price event. The feed hands the engine the then-front
  contract's tradeable prices under one alias, so a position carried through a
  roll sees the price step by the gap and **books it**, where a real roll would
  have closed the old contract and opened the new one at the two prices that
  made that gap — no PnL, and two more spreads and fees to pay. This is bounded
  and now printed: `Σ |gap| × point_value × qty` over the rolls a run spans, on
  the 16-year ES run **$56,950** against a $3.44 M loss (1.7 %). A fixture
  plants it deliberately (`a_position_carried_across_a_roll_books_the_raw_gap`:
  every ESH2024 bar is 100 and every ESM2024 bar is 120, so a round trip
  straddling the roll is worth exactly the gap and one inside a contract is
  worth nothing). Fixing it is M2's, with the fills a roll should generate.
  **The result, which is the point.** `backtest --instrument ES.v.0 --timeframe
  1m --start 2010-06-06 --end 2026-07-28 --fast 20 --slow 50`: **5,640,031
  bars** over 66 contracts and 65 rolls, 2010-06-06T22:00Z .. 2026-07-27T23:59Z,
  back-adjusted (the oldest segment carries **+602.5 points**), under
  `spread_cross`. **129,536 round trips, 25.6 % win rate, final equity
  −$3,343,328.75 (−3,443.33 %), fees $323,841.25.** The loss is the fill model:
  259,073 contract-sides at one tick of half-spread is ≈ $3.24 M, plus $0.32 M
  of commission, against a gross of roughly +$0.12 M. SmaCross is the reference
  fixture and is not supposed to be profitable (§9); this is the cost-of-costs
  lesson at sixteen-year scale.
  **A new print, because that run found something.** The account reached zero
  at bar 157,805 — **3 % of the way in**, 2010-11-15 — and there is no margin
  model in this build, so the replay went on trading a position no broker would
  have carried. Every dollar figure stays exact, but `Summary::compute` skips a
  return step whose starting equity is not positive, so the naive Sharpe
  describes the *solvent prefix* and nothing after it: it reads **+0.23** beside
  a −3,443 % return. Max drawdown passes 100 % for the same reason. `backtest`
  now prints an `INSOLVENT` block naming the bar, the fraction of the run, and
  which figures to stop reading. The metrics themselves are untouched —
  changing them is its own decision, and suppressing the number would have been
  worse than explaining it.
  **Each control was watched firing** (§7, no quality exemption). Ten
  mutations, ten catches, each restored byte-exactly and re-verified:
  `Sma`/`Ema` reading `bar.close` → the indicator controls; the feed dropping
  `signal_offset` → the routing control; the feed adding the offset into the
  tradeable fields (the D-0042 corruption itself) → three controls at once; the
  roll boundary losing its strictness (`avail == roll_ts` joining the new
  contract) → the no-lookahead control **and** the PnL control; `open`'s span
  check weakened `||` → `&&` → the D-0045 control; `narrow` inventing coverage
  → the narrowing control; a rule operand read in tradeable space → the
  `PriceField` control; the insolvency note missing an account at exactly zero
  → its own control; `combo` no longer recognising an alias → the refusal
  control. One near-miss is part of the record: `cargo test -p crucible-cli
  --lib` exits non-zero because that package has no lib target, which looked
  like a caught mutation and was not — re-run against `--bins`, it was seen to
  fail on the assertion it was meant to fail on.
  **Archive action.** `curated/rolls/ES/1m/c-minus8d.json` was built so
  `ES.c.0` resolves to something, with `--calendar-days 8` chosen by this
  session and recorded in the filename rather than defaulted anywhere in code:
  eight days before expiry is an operator's parameter, not a sourced CME
  convention, and a `.c` alias with two stored tables refuses rather than picks.
  `raw/` untouched; roll tables are curated and disposable (D-0045).
- **D-0077** (2026-07-30) — **Coarser grains are RESAMPLED ON READ, never
  stored, and the bucket grid is anchored on the exchange's session open rather
  than on the UTC clock.** `crucible-data::curated::resample` turns curated
  1-minute bars into `5m` / `15m` / `1h` / `1d`, and `crucible-cli::grain`
  decides once — for `backtest`, `combo`, `walk-forward` and `funnel` alike —
  whether a request is answered from stored partitions or from aggregation.
  `TimeFrame::M5` and `TimeFrame::M15` have been deliberately unmapped in
  `transcode::timeframe_for_schema` since M1; this is the one path that produces
  them, and that mapping is unchanged.
  **Why nothing is written.** A curated partition is named after the raw window
  it came from and records exactly one `source_file_blake3`, so one raw file
  fans out to one curated file and nothing is ever merged (D-0036). Raw windows
  are monthly, and a month boundary lands *inside* a CME session:
  2024-02-01T00:00Z is 18:00 CT on 31 January, an hour into the trading day that
  opened at 17:00. A daily bar for that day therefore has constituents in two
  raw windows, so writing it needs exactly the read-modify-write merge D-0036
  exists to prevent — and merging is where silent duplication lives. Read-time
  aggregation has no such seam, because `ParquetBarFeed` already concatenates
  every window file in order and the resampler aggregates the concatenation. It
  is also cheap: one integer pass over sixteen years of ES 1-minute bars,
  against a second copy of the archive that can go stale against the first.
  **The bucket rule, and why the anchor is the session open.**
  `day = trading_day(avail_ts)` (D-0062's expression, spelled the way D-0071
  requires so no two consumers can disagree about a date), `anchor =
  session_open(day)`, `k = (ts_open − anchor) / target`, `ts_open = anchor +
  k·target`. Two bars in different trading days have different anchors, so the
  last minute before the maintenance break and the first minute after it can
  never land in one bar. On a UTC grid they would: a UTC-day daily bar for
  2024-01-03 holds that day's 00:00–22:00Z bars **and** the 23:00Z bars that
  opened the fourth's session — two trading days in one "daily" bar, which is
  the shape of a signal that fires on the calendar rather than on the market.
  With a 24-hour target the whole session is one bucket, so a daily bar is a
  trading-day bar by construction rather than by convention.
  **An early close needs no special case,** which is the test that the anchor is
  the right one: the buckets after the close simply have no bars in them, and
  the bucket the close lands inside is built from the minutes that traded. A
  12:15 CT close 19¼ hours after a 17:00 CT open makes bucket 19 of an hourly
  resample fifteen minutes long, and its volume says 15 where its neighbours say
  60.
  **Why this is not lookahead, and the one precondition.** `avail_ts` is still
  `ts_open + tf` computed by `Bar::avail_ts` and nowhere else (§2.1), so the
  resampled bar is knowable at `anchor + (k+1)·target`. That is at or after every
  constituent's availability **iff** the target is a whole multiple of the source
  *and* the anchor is a whole number of source intervals from the epoch — then
  the largest constituent `ts_open` in a bucket is `start + target − source` and
  its availability is exactly `start + target`. Both are checked. CME opens on
  the hour so a 1-minute source satisfies the second everywhere; a calendar
  opening at 09:30:30 does not and is refused per trading day rather than
  quietly making bars visible a minute early. Every coarser pair of the current
  `TimeFrame` variants divides, so the first check cannot fire today — and a
  test asserts exactly that, so the day a `4m` variant is added the test fails
  and the refusal starts working instead of a lookahead starting.
  **Bucketing keys on `ts_open` while the day keys on `avail_ts`, deliberately.**
  Which interval a bar's *content* describes is a question about the bar's own
  interval; which window a bar is *ordered into* is a question about when it
  could be known, and D-0062 settled that one. Bucketing on `avail_ts` would
  file a 10:04 bar's content under the bucket beginning 10:05 — the right answer
  to the ordering question and the wrong answer about what happened.
  **Four refusals, each with a control that has been watched firing:** a target
  that is not coarser or not a whole multiple; a calendar declaring an intraday
  halt (a halt is a session boundary and this grid is anchored per *day*;
  neither bundled table declares one since D-0040, so it costs nothing today and
  stops the promise above from silently becoming false); a session open off the
  source grid; and a source bar whose own interval starts before its trading
  day's session open, which straddles the boundary and belongs to neither side.
  Two of them carry a positive control beside the negative one — the same bars
  on an on-the-hour calendar resample, and a bar one minute later resamples —
  so the refusal is the fixture's defect and not the fixture.
  **Calendar gains four total accessors** — `session_open`, `session_close`,
  `regular_hours`, `declares_halts` — and `open_intervals` is rewritten in terms
  of the first two, so a bucket grid and a coverage check cannot come to
  disagree about when a session ran. `session_open` is deliberately independent
  of `day_effect`: an early close moves the close, never the open, and a closed
  day still has a template open. Answering for a non-trading day is not a bug
  for the reason `trading_day` answers for a Saturday — the functions are total
  because there is nowhere to put a `Result` inside replay.
  **Two edge facts are reported rather than hidden.** A sample's *first* bucket
  can be cut by the request, and its *last* can be cut by the request or by the
  session — so `last_bar_may_be_partial` is computed against the session close
  and is **false** for an early close, which ended the bars for a reason that is
  not truncation. "May", not "did": `ohlcv` has no bar for an interval that did
  not trade, so a thin first minute looks exactly like a truncated one, and both
  are worth knowing about the first bar of a sample.
  **Provenance is unchanged.** A resampled bar's `source_file_blake3` set is its
  constituents', so D-0013 holds transitively and `crucible verify` re-hashes
  exactly what fed the run. The grain itself joins the `data_source` string the
  registry and the scorecard store, because "5m as delivered" and "5m aggregated
  here on `cme_globex_equity_index` sessions" are different bars (§2.5), and it
  is printed on every curated run **including the ones that resampled nothing**
  — the §2.4 argument, applied to data.
  **Each control was watched firing** (§7, no quality exemption). Eight
  mutations, eight catches, each restored byte-exactly and re-verified: the
  anchor moved from the session open to UTC midnight → **12 of 16 tests**, which
  is what "the anchor is the whole design" looks like; bucketing moved from
  `ts_open` to `avail_ts` → 5; the `last_bar_may_be_partial` comparison stripped
  of its session-close term → the early-close test, which is the one that
  distinguishes a truncated window from a closed exchange; each of the three
  refusals disabled in turn → its own test and only its own; `close` left on the
  first constituent → the hand-derived OHLCV test; `high` taken from the last
  constituent instead of the maximum → the same. The two positive controls in
  that file (an on-the-hour calendar accepting the bars an off-grid one refuses,
  a bar one minute later resampling where a straddling one does not) are what
  make the refusals statements about the input rather than about the fixture.
  **Not in scope: a stitched series.** A bucket spanning a roll would mix two
  `signal_offset`s (D-0076), so `ES.v.0` at `5m` is not reachable and needs its
  own decision. Instrument shorthand resolution now tries the requested grain
  and falls back to 1-minute, because `curated/bars/ESH2024/5m` never exists and
  resolving `ESH4` at `5m` would refuse a contract that is right there; D-0072's
  ambiguity refusal is untouched at whichever grain answers.
- **D-0078** (2026-07-30) — **The rule grammar gains a session clock, and the
  clock is computed ONCE by the CLI and handed down as a slice.** Seven operands
  join the grammar — `minutes_since_open`, `minutes_to_close`,
  `minutes_since_rth_open`, `minutes_to_rth_close`, `is_rth`, `is_overnight`,
  `is_post_rth` — so that "the first half hour", "the last hour" and "RTH only",
  which is how essentially every published intraday result is stated, are
  expressible in TOML.
  **The device is D-0071's, applied to a second kind of key.**
  `crucible-data::calendar` gains `Calendar::session_clock(avail_ts) ->
  SessionClock`, five exact integers; `crucible-cli::combo::attach_sessions`
  computes one per bar in bar order and calls `Grid::attach_sessions`; every
  combo the grid builds reads the same slice by bar index. `crucible-strategies`
  and `crucible-engine` still depend on `crucible-core` and nothing else, and
  what crosses the boundary is five plain numbers plus a four-variant enum. Two
  combos scoring one bar therefore cannot disagree about what time it was —
  the same failure D-0071 names, where two attributions of "which day" land a
  breach on two different dates.
  **On the grid, not on the call.** The series is attached to the `Grid` rather
  than passed to `Grid::strategy`, because a per-call argument is a per-call
  opportunity to pass a different one. `ComboStrategy` indexes it with a bar
  counter, not with a timestamp: deriving "which bar is this" inside the
  strategy would be a second attribution of a fact the caller already computed.
  **Every reading is measured from or to `avail_ts`** (§2.1), because a rule
  fires when a bar completes and the order it emits fills on the next one. The
  **session** is asked one nanosecond earlier — at the last instant the bar's
  interval covers — because open intervals are half-open and a bar ending
  exactly at 16:00 CT traded entirely inside the session it just closed. Asking
  at `avail_ts` reports the final regular-hours bar of every day as `Closed`,
  and "flatten on the last bar" becomes a rule that never fires. The two
  instants can only name different sessions for a bar whose own interval
  straddles the session open, which no interval-aligned bar has and which
  `resample` refuses outright (D-0077).
  **Signs are meaningful and not clamped.** `minutes_since_rth_open` is
  negative through the overnight session, which is exactly what makes
  `minutes_since_rth_open > 0 and minutes_since_rth_open <= 30` mean "the first
  half hour of the regular session" and not "the first half hour of anything".
  **`minutes_to_close` honours an early close and `minutes_to_rth_close` does
  not**, deliberately. The first is "is the exchange still open"; the second is
  "how far into the scheduled trading day are we". On CME's 12:00 CT
  Independence Day the session is 1140 minutes rather than 1380, so a
  `minutes_to_close <= 30` exit fires 240 minutes earlier than on an ordinary
  day — while `minutes_to_rth_close` still counts toward 15:00 CT and goes
  negative after the market has shut. Collapsing them would make one of the two
  questions unaskable, and a rule written against a fixed 16:00 close would try
  to flatten four hours after the market shut, on exactly the days when being
  positioned into an illiquid close costs most.
  **No clock is `None`, never `false`** — the rule the grammar already applies
  to an unwarm slot, for the same reason: `not minutes_since_open < 30` would
  otherwise read as *true* on every bar of a feed that has no exchange, which is
  a position taken on the absence of a calendar. `crucible combo` /
  `walk-forward` / `funnel` **refuse** such a config before replaying it,
  naming the seven operands, because a rule silent on every bar produces a
  backtest of a different strategy from the one the config describes and looks
  exactly like a strategy that never found a signal. `ComboStrategy::
  session_gaps` counts any bar that reaches the evaluator without a reading and
  `combo` prints it as a bug rather than a caveat — it is supposed to be
  unreachable.
  **All seven names are reserved**, so a slot cannot shadow one, and each
  renders in the canonical form as itself, so a config hash covers which clock
  a rule read (D-0012).
  **Each control was watched firing** (§7). Five mutations, five catches: the
  session asked at `avail_ts` instead of one nanosecond earlier → the
  last-bar-of-a-session test; the close computed from the template instead of
  `session_close` → both early-close tests; a missing session reading 0 instead
  of no-opinion → the silence test; `is_rth` widened to mean "open" → the
  one-hot test; the series read one bar ahead → the bar-index test and the
  grid-wide one. Each of the behavioural tests carries the third case §7 asks
  for: the early-close assertions sit beside the identical wall-clock bar on an
  ordinary day, so what moved the rule is provably the holiday and not the
  clock.
  **Not in scope:** calendar predicates (day-of-week, day-of-month,
  turn-of-month). They need a different key — a position in a *month* of
  trading days rather than in a session — and belong with their own entry.
- **D-0079** (2026-07-30) — **`volume` is a rule operand, and it is its own
  operand rather than a fifth `PriceField`.** `Bar::volume` has reached the
  engine since M0; what was missing was a way to name it in a rule, which made
  every volume-conditioned idea grade B for the want of one line of grammar.
  **Why not a `PriceField` variant.** A price field is read in *signal space*
  and carries the `signal_offset` a stitched continuous series applies to every
  price (D-0076). A contract count has no signal space, and there is no
  arithmetic in which adding points to contracts is meaningful. Adding a
  `PriceField::Volume` would route volume through
  `Bar::signal_*()` and the compiler would not notice, because both sides are
  `f64` by then. A separate operand makes the mistake unrepresentable, which is
  the same argument `AdjustedPrice` makes about `Price` (D-0042) at a smaller
  scale. The control is a two-sided test: on one bar carrying a +20 point
  offset, `close` reads 120 and `volume` reads its own 137, and the same bar
  with no offset reads 100 — so the 120 is the offset arriving rather than the
  fixture.
  **Contracts, not a normalized figure.** `volume > 1000` means very different
  things on a 1-minute ES bar and on a daily one, and that is the operator's
  problem to state, not the grammar's to smooth over. Turning volume into a
  ratio — "twice the 20-day average" — needs a trailing window, which is a
  rolling statistic and belongs to `crucible-strategies::indicators`, not to an
  operand.
  **`f64` is exact here** to 2^53 contracts, eleven orders of magnitude past any
  bar this archive holds, and nothing on this path reaches accounting (§2.3).
  **`volume` is reserved**, so a slot cannot shadow it, and it renders as itself
  in the canonical form so a config hash covers it (D-0012).
  **It adds no warmup.** Volume is available on bar 0, so a rule reading it
  starts when its indicators do — pinned by test, because a silent lengthening
  of `Grid::max_warmup_bars` would shorten every combo's evaluation window for a
  reason nobody wrote down (§2.6).
  **Each control was watched firing** (§7). Three mutations, three catches:
  volume picking up the signal offset → the signal-space test; `volume` removed
  from the reserved list → the reservation test *and* the slot-shadowing
  refusal; the operand rewired to read the close → three tests at once.
- **D-0080** (2026-07-30) — **Rolling normalizers exist and a full-sample one is
  not expressible — by construction, not by discipline.**
  `crucible-strategies::indicators::rolling` adds `RollingZScore` and
  `RollingStdev` over a declared `RollingSource` (`close`, `volume`, `return`),
  and the combo grammar gains `kind = "zscore"` and `kind = "stdev"`. They
  answer the two things the grammar most obviously could not say: "this bar is
  two sigmas below its own recent range" — unwritable, because there is no
  arithmetic between operands, so `(close - bb.mid)/(bb.upper - bb.lower)` has
  no spelling — and "volume is twice its recent average", which an absolute
  `volume > 1000` (D-0079) cannot mean on more than one grain.
  **The §2.1 defence is structural.** A normalized feature is where lookahead
  usually enters a research pipeline, and it enters by convenience: the series
  is in memory, `mean()` and `std()` are one call each, and the z-score is
  standardized against a mean the market had not produced. So everything here
  is a private trailing window behind `Indicator::update`, which takes **one
  bar** and returns a reading for **that** bar. There is no constructor over a
  slice, no `fit`, no two-pass anything, and no `IndicatorKind` that names a
  full-sample statistic — so `controls::LeakyZScore`, the planted defect, stays
  reachable from Rust and unreachable from TOML. A test enumerates the five
  kinds so that adding a sixth fails until someone answers "does it look at
  anything it has not been shown yet?".
  **The property, and the planted control.** The claim is **truncation
  invariance**: a reading at bar *i* is a function of bars *i−period+1 … i*
  alone, so appending bars after *i* cannot change it. Both halves are asserted
  together, because only the pair is a diagnosis (§7): the rolling readings over
  a 100-bar prefix are **bit-identical** to the first hundred readings over the
  200-bar series, and the full-sample readings over the same prefix move by more
  than a full sigma. The full-sample function is written **in the test file and
  nowhere else** — the only way to compare against the defect is to build it
  where it cannot escape. On a trending fixture the two statistics differ by
  more than two sigma at the same bar, and the first warm bar reads deeply
  negative under the full-sample statistic (which "knows" the series climbs for
  180 more bars) and ordinary under the rolling one. That difference **is** the
  lookahead, measured rather than asserted.
  **The control has a control.** Rewriting the test's leaky reference as an
  *expanding* window — past-only, still causal — drops the measured movement to
  exactly **0** and both assertions fail. So the detector is measuring
  future-dependence and not accumulator noise, sample size, or the fixture.
  **`source` is required and is part of a slot's identity**, not one of its
  axes: it does not expand, and a 20-bar z-score of price and a 20-bar z-score
  of volume are different features that must not share a config hash (D-0012).
  It is rendered into the canonical form, and a CLI test asserts all three
  sources hash differently. There is no default, because "the z-score of what"
  is not a detail; an unknown spelling is refused naming the slot and listing
  the three that exist.
  **`source = "return"` costs one extra warmup bar** — a return needs two
  closes — and the bar is *declared* rather than absorbed, so §2.6 aligns the
  whole grid on it. A grid mixing sources would otherwise start its
  return-based combos one bar late while reporting that they started with
  everyone else.
  **`period >= 2`, refused at config-load time** naming the slot. A one-bar
  window has zero spread, so every reading over it is 0/0.
  **A flat window has no z-score and does have a deviation.** The z-score is
  `None` — the grammar's "no opinion", the same answer an unwarm slot gives —
  because the numerator and the denominator are both zero; the standard
  deviation is `0.0`, because that is a real answer. Returning a NaN and letting
  the operand's finiteness filter catch it would work by accident.
  **A z-score is shift-invariant**, so unlike `close > 4500` it is safe on a
  back-adjusted series (D-0076): the offset cancels between the value and the
  window's mean. Asserted to 1e-9 rather than bit-exactly, and the difference is
  worth stating — the rolling accumulator sums values around 350 differently
  from values around 100, which is the drift `indicators` already accepts until
  the M2 Welford/rebase task. A numerics gap, not a semantics one.
  **Each control was watched firing** (§7). Seven mutations, seven catches: the
  window read before it was full → four tests; the return source declaring no
  extra warmup → three; a flat window returning NaN instead of no-opinion →
  three; the volume source reading the close → the source-selection test, which
  carries its own control (the same bars' closes are flat, so a source being
  ignored would answer twice the same way); the source dropped from the
  canonical form → the identity test; the grid's warmup ignoring the source →
  the alignment test; and the leaky reference made causal → the planted control
  itself, above.
- **D-0081** (2026-07-30) — **The S0 predictor seam is named as the next build,
  and the M2.5 predictor workbench arrives AS the funnel's S0 rather than
  beside it.** Why: S0 is the only outstanding item that makes a *refused*
  question askable; everything else outstanding makes an answer we already
  produce better.
  **What is named.** A score-emitting evaluation path with forward-return
  joins: a signal emits a continuous score per bar, the seam joins each score
  to the return over configured horizons ahead, buckets, and reports — no
  orders, no fills, no equity curve. `crucible-funnel::stages`' module doc
  already carries half the contract (bucket forward returns by signal quantile,
  monotonic relationship, nonzero information coefficient) and H-008's Gate 0 /
  Gate 0b carry the other half (horizons at 1/5/10/20 minutes, a block
  bootstrap over sessions, and the effect size reported **in ticks** so it can
  be compared against the spread). **Both halves are owed** — they are not the
  same statistic, and shipping the IC without the tick-denominated effect size
  answers the general question while leaving H-008's registered gates
  unrunnable. The module-doc spec gets extended in the commit that implements
  it (CLAUDE.md §8), not after.
  **Why as S0 and not beside it.** `docs/PROJECT_PLAN.md` §6 scopes M2.5 as a
  score-emitting trait beside `Strategy` with its own report. Landing it there
  would create two places that answer "does this score predict forward
  returns", and only one of them charges a trial, declares a hypothesis family,
  stores its criteria before the run, or reaches the registry. A number
  produced outside the funnel is a number nobody counted (D-0074's rules are
  about ordering and identity, and a workbench sidesteps both). M2.5's exit
  criterion — one signal family evaluated predictor-first, in a report worth
  sending to an external reviewer — is *satisfied* by an S0 report and
  strengthened by it, because the S0 report arrives with a trial count
  attached.
  **Why it is first.** S0-refused blocks the **front** of the funnel for every
  predictor-shaped idea, and the block is not a delay — it is a stop. Six
  backlog files register a predictor-first Gate 0 (H-001, H-008, H-011, H-012,
  H-013, H-014), the registered gate order is binding, and a file may not run
  its equity-curve gate first because the predictor gate is inconvenient:
  reading a predictor result with the backtest already known is the exact
  failure pre-registration exists to prevent (`research/backlog/README.md`
  §2.4). So those six cannot advance by any route that does not pass through
  this build. The M3-full plan treats it as block one.
  **The first consumer is also the specification.**
  `research/backlog/H-008-short-horizon-overreaction.md` is marked
  `blocked_on: s0-predictor-seam` and states what the seam must do in the terms
  a consumer needs, which is why it is the specification rather than merely the
  first client. Its Gate 0b is the demanding half: *is the reversion bigger
  than one tick* is a question a backtest answers badly (the answer arrives
  entangled with the fill model) and a predictor report answers directly. An
  S0 that cannot produce "real, and smaller than our costs" has not been built.
  **The lookahead trap, stated before the code exists.** A forward return is by
  construction information from after the decision point. It is legal in
  *measurement* space and is §2.1 lookahead the instant it can reach a
  decision. Therefore: the join lives in the evaluation path and never in the
  `Strategy` / `Feed` path; nothing computed from a forward window may feed
  back into a score; and scores obey the ordinary rule — a score at bar `t` is
  computed from what was available at `t`, which for indicators means
  `bar.signal_*()` (D-0076). The negative control is the one that plants the
  defect rather than the one that shows the happy path: a "signal" that IS the
  forward return must report a perfect relationship, and a test asserting that
  is what proves the join runs the direction it claims rather than joining a
  score to its own past. Per CLAUDE.md §7 it must be watched firing on a
  deliberately reversed join.
  **What this does NOT do**, because the leverage here is easy to overstate.
  It changes **no triage grade** — grades measure cost to express and this
  measures whether we are allowed to look yet. It does **not** fix the sample
  problem: a grade-A config still replays one contract's life, and turning ~60
  sessions into a verdict sample is registry pooling, the fifth unlock, which
  is separate work. It does **not** make `Graduate` awardable — D-0075 stands
  until S3's battery exists. And it does **not** lift the stage refusal ahead
  of itself: `s0` stops being refused at load in the commit where S0 can
  actually run, never before, because a declared stage that is silently skipped
  is precisely what D-0075 refuses to ship.
  **Ordering against the other open block.** `docs/MILESTONES.md` said the
  account-evaluation bootstrap evaluator "is the next block". It is the next
  block **of `docs/ACCOUNT_EVAL_SPEC.md` §4**; the S0 seam is the first block
  of M3-full, and the milestone line is corrected to say which it means — two
  lines each claiming "next" is how a plan stops being one. The two are not
  rivals for the same component: both need a **block bootstrap over sessions**
  (§4 over daily PnL records, S0 over forward returns), so whichever lands
  first builds it in `crucible-funnel::stats` where the other consumes it,
  rather than each growing a private resampler whose seeds derive differently.
- **D-0082** (2026-07-30) — **S0's measurement half: the forward-return join,
  its semantics, and its negative control — planted before the seam's first
  real use.** `crucible-funnel::s0`. This is the first block of D-0081's seam
  and deliberately **not** the whole of it; what is *not* here is at the bottom
  of this entry, because a half-built seam that reads as finished is worse than
  no seam.
  **The join runs one way, and the direction is the design.** For a score at
  `avail_ts = t`, the partner is the **last bar whose `avail_ts` is at or before
  `t + horizon`**, and it must be strictly later than the scored bar. Never the
  first bar at or *after* the target: that reads a price the horizon had not
  reached, which is a one-bar lookahead wearing a measurement's clothes. A
  forward return is information from after `t` and is legal **only** in
  measurement space; nothing signal-side may read one.
  **Horizons are durations, not bar counts** — `ohlcv` has no bar for an
  interval that did not trade, so "ten bars ahead" is ten minutes only on a
  grain with no gaps, and H-008 registers a *ten-minute* horizon. The fixture
  `horizons_are_durations_not_bar_counts` puts a 37-minute hole in the series
  and asserts a 3-minute horizon does not jump it.
  **A window that runs off the end of the series is UNANSWERABLE, not
  answerable-with-a-shorter-window** — and this was found by a failing test
  rather than designed in, so it is recorded as the correction it was. Inside
  the series a missing bar means *nothing traded*, and the last bar in the
  window is the best price the horizon offered. At the end of the series the
  same absence means *the data stopped*, and from inside the two are
  indistinguishable. Pairing anyway measures a one-minute return and labels it
  ten: silent, survives every downstream statistic, and biases the tail of every
  sample toward whatever the last few bars happened to do. Such scores are
  dropped and **counted**, never zero-filled — a fabricated zero drags every
  mean toward the middle, and `pairs + dropped == input length` is asserted
  against the input length precisely so a quiet zero-fill cannot balance the
  books.
  **The quantile buckets are descriptive, not §2.1 lookahead, and the boundary
  is stated rather than assumed.** `buckets` cuts at quantiles of the whole
  measured sample, which is textually the thing §2.1 forbids. §2.1 forbids a
  full-sample statistic *used inside a strategy or feature*, where it decides a
  trade with information the trade could not have had. Nothing here decides
  anything. The moment a bucket edge is used to **trade** — "enter in the top
  quintile" — it is lookahead again and must come from a trailing window.
  **The bootstrap resamples whole sessions.** Resampling individual observations
  would call one minute's return independent of the next, which is false at
  every horizon this stage measures and yields an interval far too narrow. A
  session is the block: the unit the archive is organized in, the unit a fold
  boundary lands on (D-0062), long enough to carry short-horizon
  autocorrelation. `rand_chacha` is adopted here as its first real consumer and
  its §6 placeholder comment is deleted, as §6 requires.
  **The negative control, planted before the first real use and watched firing**
  (§7, no quality exemption). A "signal" that IS the forward return, run two
  ways on the same bars: through the leaky join it scores **IC = 1.000000**, and
  through the correct join the same planted signal scores **IC = −0.026527**.
  The third case names the cause rather than leaving a difference: the same leak
  measured at a *60-minute* horizon collapses to **0.277185**, so the
  near-perfect score belongs to the horizon the leak read and is not an artifact
  of the data or of the statistic (§7's "add the third case" rule).
  **Three mutations, each watched failing and each restored byte-exactly.**
  (1) deleting the fully-observed-window guard → caught by 2 controls;
  (2) making the join look **backward** → caught by 6, including the leak
  control ceasing to fire, which is what proves that control is not merely
  agreeing with the data; (3) making the information coefficient correlate the
  score against **itself** → caught by the *silent* control, which is the
  mutation that matters most, because a silent control that cannot fail is
  decoration.
  **What is deliberately NOT in this build, and why the refusal stands.** There
  is no config surface, no score extraction from a combo, no registry row, no
  CLI path and therefore **no S0 determinism hash** — the four existing hashes
  are unmoved (demo `b55747513df596ed`, combo `0e1ab52d474b862b`, walk-forward
  `711e1cb34a2ee2b4`, funnel `2f430893d2a79a8f`) because nothing calls this
  module yet. Accordingly `Stage::S0.is_implemented()` is **still false**, a
  config declaring `s0` is **still refused at load**, and
  `H-008`'s `blocked_on: s0-predictor-seam` **still stands**. D-0075 and D-0081
  both say the refusal lifts in the commit where S0 can actually run and never
  earlier; lifting it beside a module with no caller would be exactly the
  "declared stage, silently skipped" failure D-0075 exists to refuse.
- **D-0083** (2026-07-30) — **A hash gate is not a trial: `--hash-only` runs
  against an ephemeral registry, and rows already written by one are withdrawn
  by an appended `void` record rather than deleted.** Ruled a bug, not a §9
  entry.
  **What was wrong.** `crucible funnel --hash-only` opened the on-disk registry
  before checking the flag, so every determinism gate appended 30 lines — 24
  `run_finished` and 6 `verdict` — and the first one also claimed 24 `run` rows
  and charged 24 trials. Measured, not inferred: `sha256` of
  `results/registry.jsonl` before a gate and after it differ, and the store grew
  174 → 204 lines on a single `--hash-only` invocation.
  **Why it is a bug and not a decision.** A trial count is the denominator of
  every multiple-testing correction this project exists to compute — deflated
  Sharpe reads it, and §7.7 forbids reading it from anywhere else. A gate that
  charges trials makes the honesty machinery *expensive to test*, which is
  precisely the wrong incentive to build into it: the cheapest way to keep a
  trial count low would become "run the gates less". §9's neighbouring entry
  ("re-running a funnel appends a second `run_finished`") does **not** cover
  this: that entry is about a run that genuinely ran again as research, and a
  hash gate is a question about the *code*.
  **The fix, and why the ephemeral registry reads nothing either.**
  `Registry::ephemeral` honours all five contract rules in memory and never
  opens the file. It deliberately does not *read* the existing store, which is
  the less obvious half: a gate whose answer depended on how many times it had
  been run before would not be a determinism gate. Both halves are pinned —
  `a_hash_only_run_leaves_the_registry_byte_identical` runs a real funnel first
  so there is a populated store to disturb, then asserts three consecutive gates
  leave the bytes identical; and
  `the_hash_gate_answers_the_same_with_and_without_a_populated_registry` asserts
  the cold and warm answers match and that the gate does not even create the
  file.
  **The correction is appended, never erased.** The store is append-only
  (D-0074), and a row written in error is a fact — it happened. So a new line
  kind `void` names a prior row by its `run_id`, carries a mandatory reason, and
  the reader excludes voided runs from `Registry::trials_for` **by
  construction**: a trial counts while at least one run charged to it is still
  standing, so withdrawing one fold of a combo does not withdraw the combo
  (rule 3 preserved, and pinned by
  `voiding_one_fold_does_not_withdraw_a_combo_another_fold_still_holds`).
  Deleting the rows instead would make the file claim the mistake never
  occurred, which is the same class of dishonesty as editing a golden value to
  get green.
  **A verdict inherits voidedness from its trial**, because a verdict has no
  `run_id` — it is decided over a combo. `Registry::verdicts` still returns
  everything (the log is the record of what happened) and
  `Registry::verdicts_standing` is what statistics read. Two methods rather than
  a filter inside one, so "what does the log say" and "what counts" stay
  separately askable; collapsing them would make the correction invisible, which
  is the failure voiding exists to avoid.
  **Applied to this archive.** Every one of `results/registry.jsonl`'s 204 lines
  was gate contamination — the directory was created by the first gate of the
  session and no research run had ever written to it — so 24 void records were
  appended, one per claimed run. Verified through the real reader: the store
  parses, 24 runs read as voided, and `trials_for("null-harness-sma-cross")`
  reads **0**. `results/` is gitignored, so this is an archive action and not a
  commit.
- **D-0084** (2026-07-30) — **Sample adequacy is `admission`, not `S0`: a
  pre-trial admission check gets its own label, because it was squatting on the
  name of a gate that is refused at load.**
  **The squat.** `assess` initialized `decided_at = Stage::S0` and tagged both
  adequacy criteria `Stage::S0`, from before the predictor stage existed. The
  result was a report that could print "killed at s0" for a combo with too few
  sessions, while a config declaring `stages = ["s0"]` was **refused** in the
  same build (D-0075). Two different things wore one name, and the one a reader
  meets first is the one that never runs.
  **The fix.** `Stage::Admission`, ordered first so `decided_at` still sorts in
  evaluation order, rendering as `admission`. It is **not declarable**:
  `Stage::from_str` deliberately has no `"admission"` arm, so a config can
  neither ask for the adequacy check nor skip it — a check you can decline is
  not an admission check. `the_four_declarable_stages_still_round_trip` pins the
  four spellings a config may use and `a_config_cannot_declare_admission_as_a_
  stage` pins the refusal.
  **Why the distinction earns its keep.** "Not enough evidence to judge" and
  "judged and found wanting" are different verdicts about an idea, and the
  second is much the more flattering thing to report when the first is true.
  That is also why the scorecard now carries a legend saying so in the Verdicts
  section rather than leaving a reader to infer what `admission` means.
  **The funnel determinism hash did NOT move, and that is a measurement rather
  than a hope.** `verdict_hash` hashes `decided_at`, so the rename reaches it —
  but only for a combo actually decided there. Every combo of
  `configs/combo-smoke.toml` **passes** adequacy (316 pooled round-trips against
  5 required, 8 sessions against 4) and dies at `s1`, so the pinned hash stays
  **`2f430893d2a79a8f`**. The reach was proven rather than assumed, on a config
  built to fail adequacy (`min_oos_trades = 100000`): under the old label it
  hashes `1b6ac5e72c106c0b` and under the new one `8c6d9ed042df24b3`. So the
  label is live in the hash, the null harness simply never visits it — and no
  re-pin was manufactured to make the change look bigger than it is.
- **D-0085** (2026-07-31) — **The S0 caller: a config declares what to measure,
  a combo emits the score, the registry charges the trial, and the `s0` refusal
  lifts.** The second half of D-0081's seam; D-0082 was the measurement.
  **Score extraction.** `crucible-strategies::combo::ComboScorer` is a
  *score-emitting* projection of the same `ComboSpec` and `Combo` the strategy
  is built from — one continuous reading per bar, no orders, no position, no
  fills. Built from the same spec rather than a parallel definition, so the two
  cannot drift. The score is a named indicator slot (`"z"`, or `"bb.upper"` for
  a banded one); every indicator in that crate is trailing by construction
  (D-0080), which is what makes a score at `avail_ts` computable only from what
  was available then — the property the forward-return join depends on and
  cannot itself check.
  **The config surface.** `[s0]` carries `score`, `horizons_minutes`, `buckets`,
  `bootstrap_draws` and `min_abs_ic`, under `deny_unknown_fields`. It is
  required **exactly when** `stages` declares `s0` and refused when it does not:
  a stage with no block is a stage with no pre-registration, and a block for a
  stage nobody asked for is a config that thinks it asked for something. Both
  refusals name themselves. Horizons are **minutes**, not bars, for D-0082's
  reason: `ohlcv` has no bar for an interval that did not trade.
  **The criterion needs BOTH halves at ONE horizon** — `|IC| >= min_abs_ic`
  *and* a mean forward return whose bootstrap interval excludes zero — and this
  is the one design change the null harness itself forced. The first version
  read `|IC|` alone; run against 20,000 bars of seeded random walk it measured
  `|IC| = 0.0378` and **passed** a 0.02 bar. Size without significance is what a
  large enough sample of noise gives away for free, and significance without
  size is an effect too small to trade. The threshold was *not* raised to make
  the harness fail — that would have been fitting the number to the fixture, the
  direction §9's seed-29 rule forbids. The criterion was made correct instead,
  and the null harness now reads `0 of 6 combo(s) cleared S0`.
  **It is a gate, not a report beside one.** S0 runs ahead of S1/S2, its reading
  reaches `Evidence`, and `assess` evaluates it directly after admission and
  before S1 — so a score that predicts nothing is dead before any equity curve
  is judged, which is the entire argument for a predictor-first stage. Every
  combo gets a registry row claimed *before* it is measured and a trial charged
  to `meta.hypothesis_family`, exactly like S1 and S2; `fill_model` records
  `"none (s0 takes no position)"` rather than leaving the field to imply one
  (§2.4). On the null harness the funnel now exits 5 with all six combos
  **KILL at s0**.
  **The refusal lifts here and nowhere earlier.** `Stage::S0::is_implemented()`
  is true, a config declaring `s0` is accepted, and
  `research/backlog/H-008-short-horizon-overreaction.md` is unblocked — all in
  this commit, which is what D-0075 and D-0081 both asked for.
  **The S0 determinism hash is `91107aeb6e6802c0`**, from `crucible funnel
  --config configs/s0-smoke.toml --out results --hash-only`. Its inputs: the
  `configs/combo-smoke.toml` seeded random walk (`[data] source = "synthetic"`,
  **seed 42**, 20,000 bars, start 4000.00 points, 1-tick vol), a 6-point
  SMA-crossover grid plus `zscore(period=20, source="close")` as the score,
  horizons 1/5/10/20 minutes, 5 buckets, 500 bootstrap draws, `min_abs_ic =
  0.02`, and `[run].seed = 42`. Each combo's bootstrap seed derives from
  `derive_run_seed(config_hash, root_seed, account, combo_index, 0)` XOR a
  per-horizon constant, so two horizons of one combo never share a resample
  (D-0064). **The three engine hashes and the pre-existing funnel hash are
  unmoved** — S0 evidence enters `verdict_hash` only when a config declares
  `s0`, so every gate pinned before today still answers what it answered.
  **The negative control, end to end through the CLI.** D-0082's controls test
  the join; once a caller exists they are decoration on their own. The same
  `configs/s0-smoke.toml` command was run with the join correct and with a leak
  planted in it (the score replaced by the forward return it is about to be
  joined to): **IC −0.0378 → +1.0000** at every horizon, restored byte-exactly
  afterwards and re-measured at −0.0378. `s0_runs_end_to_end_and_kills_the_null_
  harness_at_s0` pins the silent side permanently. The firing side stays a
  **watched mutation rather than a permanent test**, and deliberately: making a
  leak reachable from TOML is exactly what D-0080 refuses, so there is no config
  that can plant one. That is a real limitation and it is written down rather
  than papered over.
  **What the leak does NOT do, which is worth knowing.** It moves the IC to
  1.0000 and does *not* flip the verdict: the mean forward return of a
  zero-drift walk is still indistinguishable from zero, so the significance half
  still fails and the combo is still killed. S0's *verdict* is therefore not a
  leak detector — the IC readout is. Detecting a leak is the permutation and
  truncation harnesses' job, which is `stats` and still owed.
- **D-0086** (2026-07-30) — **A session calendar carries session ERAS, and the
  four products that had no table now have one — all of it measured from the
  archive.** Full workbook in `docs/SESSION_ERAS.md`; the instrument is
  `crucible-data/examples/session_profile.rs`, which bits every nonzero-volume
  bar into a `(local civil date, local minute-of-day)` grid and reads open,
  close, halt and holiday behaviour off it. (Amends D-0039; corrects the scope
  of D-0040; closes the defect D-0059 deferred.)
  **The correction that motivated it.** D-0040 deleted the 15:15–15:30 CT halt
  from the equity-index table after finding 315 nonzero-volume ESH4 bars inside
  it in January 2024. The measurement was right and the generalisation was not:
  over the whole archive that window carries **0.04 traded minutes per date on
  2,018 Mon–Fri dates from 2015-01-01 to 2021-06-25, and 15.00 on every one of
  the 1,344 from 2021-06-28**. CME's SER-8788R eliminated the halt effective
  2021-06-28. So the table was right for the month it was checked against and
  wrong for the five and a half years before it — and a calendar with one
  session template cannot hold both answers. That is the third side (§7): the
  archive and the spec page disagreed, and the era boundary is what makes them
  agree.
  **The mechanism.** `[calendar.session]` stays the *current* era and gains an
  optional `from`; `[[calendar.era]]` entries carry earlier templates, each with
  its own open, close, halts and RTH window. The loader sorts them, refuses an
  era without a `from`, a `session` without one beside eras, an era newer than
  `session`, two templates on one date, and a `valid_from` earlier than the
  oldest era. `open_intervals`, `session_of` and `trading_day` pick the last era
  starting at or before the date; a date before every era gets the **oldest**
  era's answer, because the functions are total and a later era's hours would be
  a bigger lie about an earlier exchange. `reference_span` may no longer cross an
  era boundary — D-0039 stated that as prose and it became false the moment era 3
  turned out to be two eras, so it is a load-time check now.
  **Equity index gains two eras and loses one warning.** `valid_from` moves
  2015-09-21 → **2012-11-19**; era 2 (close 16:15, halt 15:15–15:30) and era 3a
  (close 16:00, halt 15:15–15:30) join era 3b. Unmodelled history drops from 5.3
  years to 2.4. **Era 1 is still not modelled, deliberately**: its trading day
  opens 15:30 on D−1 with a halt at 16:30–17:00 *on D−1*, and that block is
  absent whenever D−1 is not a trading day. The template can express neither, and
  both approximations were measured before being rejected — one produces ~30,000
  out-of-session bars per contract, the other ~60 phantom expected bars a week.
  `docs/SESSION_ERAS.md` §1.1 records the shape so nobody re-derives it.
  **Three more equity-index corrections, each measured.** Holiday treatment
  changed twice independently of the session eras (10:30 CT closes in era 1;
  **full closures** 2013-01-21..2014-02-17, proved by the Sunday evenings that
  did not open — 6 of 8 between 2013-01-06 and 2013-02-24, the two missing being
  the eves of MLK and Presidents' Day; 12:00 CT from 2014/2015). Christmas
  landing on a Saturday closes the Friday before (2010-12-24 and 2021-12-24 have
  no session at all) while New Year's Day landing on a Saturday does not
  (2010-12-31 and 2021-12-31 are full sessions) — so the two rules now differ,
  where before both were `sunday_to_monday` and 2021-12-24 got a 12:15 close it
  never had. And the day before Independence Day is a 12:15 CT close on eight
  dates, all and only the years from 2013 where 4 July fell Tuesday–Friday.
  **That last one closes D-0059's deferred defect.** `HolidayRule::WeekdayBefore`
  gains `anchor_weekday`, a condition on the **unobserved** anchor, and
  `us_equity_options.toml` uses it too: the six phantom NYSE early closes D-0059
  named (2015-07-02, 2016-07-01, 2020-07-02, 2021-07-02, 2022-07-01, 2026-07-02)
  are ordinary sessions again. The test that asserted the wrong behaviour on
  purpose was flipped, which is exactly what it existed for.
  **Four new tables, and they are four because the products differ.** MLK Day
  2022-01-17, last traded minute before 17:00 CT: **ES 12:00, ZN 12:00, CL 13:30,
  GC 13:30, 6E 15:58 — a full session.** One date, four answers. A Good Friday
  carrying the Employment Situation release splits them three ways again (ES
  08:15, ZN and 6E 10:15, CL and GC shut), on four independent years that agree.
  `cme_globex_energy` (CL), `cme_globex_metals` (GC), `cme_globex_fx` (6E) and
  `cme_globex_rates` (ZN) join the bundle; every root the acquisition basket
  holds now resolves to a calendar, so `bars_per_year` stops falling back to
  measuring the sample for four of the seven.
  **Two disagreements with CME's published hours, and the archive wins both.**
  (1) Every published summary puts FX on the same noon holiday halt as the rest
  of the exchange; 6E stopped observing it entirely in 2022 and has traded full
  sessions on every recurring US holiday since. (2) The bond market observes
  Columbus Day and Veterans Day and `docs/THETADATA_PLAN.md` §8.1 records
  Veterans Day as one the NYSE trades and the bond market does not — but CBOT
  Treasury **futures** traded a full session on every one of them in sixteen
  years, so `cme_globex_rates` has neither. Cash and futures are different
  markets; the prior was checked and refuted rather than assumed. CL and GC
  produced no disagreement: CME's published hours and holiday summary match the
  archive to the minute.
  **What is deliberately NOT modelled, with its size.** The 15:15 CT Friday
  closes 6E and ZN took before sixteen Monday holidays between 2012-01-13 and
  2015-05-22 (no rule fits a pattern that includes Columbus Day once and never
  again, and no CME document was retrievable); the 12:00 CT rather than 13:30 CT
  close CL and GC take when the holiday is a **Friday** (three for three, but
  `Effect::EarlyClose` carries one time); the 2010-12-31 12:15 CT close 6E and ZN
  took and nobody else did; the 2025-11-28 Globex outage. Each is listed in the
  table header with its date list and its cost.
  **`rth_open_local`/`rth_close_local` on the four new tables are the one field
  not measured**, and they say so: open outcry ended for CL and GC on 2016-12-30
  and CME publishes no RTH window for any of them, so the values are the
  inherited floor hours, cited, and read only by `session_of` — never by
  `open_intervals`, `is_open`, `is_trading_day` or `bars_per_year`.
  **What moved.** `bars_per_year(1m)` for equity index: **354,319 → 353,963**
  (−0.10 %), because the reference span moved 2016-01-01..2026-01-01 →
  2022-01-01..2026-01-01 to stop straddling the 2021-06-28 boundary. Neither
  span contains the halt, so the difference is entirely the holiday mix of a
  different set of years. The ESH4 January-2024 reference run is bit-identical
  (30,167 bars, −23.51 %, 665 round trips, $76,486.25), and all three
  determinism hashes are unchanged — dollars do not touch the calendar.
  **What the archive QA found.** 26 `qa` runs, one front contract per era per
  root. Out-of-session bars in era 3b: **zero, all seven roots**. In era 3a:
  5–36 per month, every one stamped 15:15 or 16:00 CT — a settlement print in
  the boundary minute, on 71 and 72 of 1,496 ES dates respectively, against
  D-0040's systematic 15 minutes × 21 days. Moving the halt to absorb them would
  be fitting the table to noise. **Two genuine archive holes**, each exactly one
  trading session and each reported `available` by the vendor: **GC 2012-09-12**
  and **ZN 2014-10-03**, 1,380 one-minute bars apiece. Every other whole-day
  absence is on the vendor's own `degraded` list (28 dates), and the thirteen
  dates the vendor calls `missing` are all Saturdays. Reported, not re-pulled.
  **A blind detector stopped being blind, as a side effect.** D-0072's
  `the_gap_inside_sessions_check_passes_on_the_merged_partition` had two reasons
  for silence, and the first — "no bundled calendar claims gold" — is gone. The
  test keeps its original assertion for the calendar-less call that the bug
  report made, and a companion asserts that the same planted merge is now loud
  (>1.87 M missing bars, coverage under 0.1 %). That is a consequence of building
  a metals table, not the "strengthening" §9 refuses, and the partition key is
  still what fixes the bug.
- **D-0087** (2026-07-31) — **The permutation null: the first real piece of S3,
  and the harness that catches the planted leak.** Block A of
  `docs/plans/m3-full.md`, and the acceptance test of M3's last clause.
  **What the null IS, stated because it is a choice.** `H₀: the return series is
  exchangeable at block scale L` — any dependence longer than `L` is absent,
  while the return distribution and dependence shorter than `L` are preserved
  exactly, because blocks move intact. A p-value here means "the observed metric
  was in the top `p` of what this strategy produces on series that keep the
  marginal distribution and the short-range dependence but lose the long-range
  ordering". It does not mean "profitable" and it does not mean "real".
  **Why it catches a leak that every prior gate passes.** `LeakyZScore`
  standardizes against the whole series and then trades. Permute the series and
  it **re-fits on the permutation**: its edge reproduces on every draw, so the
  observed run sits in the middle of its own null. On the null harness it reads
  **observed +5.947 %, null median +3.707 %, p = 0.2079** against a
  pre-registered `0.05` — killed at S3, while admission, S1, S2's Sharpe, the
  kill-level sweep and both controls all still **pass**. That is the point: a
  statistic computed *on the leaked run* cannot separate a leaked edge from a
  real one, which is why tightening any earlier threshold was always the wrong
  repair (§9).
  **`crucible-funnel/tests/planted_leak.rs` flips from `Iterate` to `Kill`**,
  and that flip is M3's acceptance test. It was reached by building the harness
  and by nothing else — the strategy, its registration and every threshold are
  untouched. The test additionally asserts that **every gate before S3 passes**,
  so a future change that made the leak die earlier would fail here rather than
  silently stop measuring the detector.
  **The converse control came first, and it earned its place.** A detector that
  kills everything is not a detector, so `a_real_edge_destroyed_by_shuffling_
  survives` plants a series with genuine long-range structure — regimes
  persisting 500 bars — and requires a trend-follower to **survive**: observed
  188.77 pt against a null median of 41.85 pt, **p = 0.0050**, the floor
  `1/(1+200)`. The first draft of that fixture was wrong (`4000 + dir*t*drift`
  is a sawtooth, and the trend-follower lost money on it); the converse control
  caught the broken fixture, which is precisely the argument for having one, and
  the fixture's doc comment records it rather than quietly correcting it. A
  third case names the cause: the **same** strategy on a structureless walk
  reads `p = 0.4030`, so the converse control measured the edge and not the
  harness (§7's "add the third case").
  **The p-value carries the +1 correction** — `(1 + #{null ≥ observed}) /
  (1 + draws)`. Without it a strategy that beat every draw reports `p = 0`,
  claiming more certainty than the resample count can support; with it the floor
  is the resolution the experiment actually has. An **absent** null has no
  p-value and **fails** its criterion rather than passing it (D-0075).
  Unevaluable draws are counted, never folded in as zeros — a zero is a result,
  and inventing one moves the p-value.
  **The pinned determinism hash is `9fe41f6f5b3653e7`**
  (`the_permutation_null_is_pinned`): blake3 over the observed metric, the block
  length, the unevaluable count and every null draw, for `walk(4_000, seed 29)`
  — the inlined SplitMix64 walk from 4000.00 points in quarter-point ticks —
  evaluated with the file's `leaky_zscore_pnl`, `block_len = 20`, `draws = 200`,
  `seed = 4`. Hashed over the whole distribution rather than a percentile so a
  drift anywhere fails, not only where a p-value happens to read. The five
  existing hashes are **unmoved**: the harness observes runs, it does not change
  them.
  **Mutations, each watched failing and restored byte-exactly.** (1) the
  decision rule's comparison `p <= alpha` → `p >= alpha`: caught by all three
  controls. (2) the tail comparison `null >= observed` → `null <= observed`:
  caught by the converse control and by the hand-derived p-value test.
  **Where the criterion lives, and why it is not gated on `stages`.**
  `Criteria::max_permutation_p` is evaluated whenever an `Evidence` carries a
  p-value, rather than when `stages` names `s3` — because **s3 as a declarable
  stage still needs the rest of the battery** (deflated Sharpe, PBO, truncation)
  and is still refused at load. D-0075's stage refusal is untouched and the
  `Iterate` ceiling still stands; lifting either is block D's, with its own
  superseding entry.
  **What is NOT here: the truncation-invariance harness.** Block A is two
  harnesses and this session delivered one complete rather than both half done.
  Truncation asks a different question — decisions computed on `data[0..t]` must
  be **bit-identical** to decisions `≤ t` computed on the full series — which is
  a determinism property asserting equality, not a statistical one asserting
  extremeness. It truncates the **end** (the future), because that is the
  direction lookahead flows from; truncating the start would test warmup
  sensitivity, which is a different and lesser question. It is not built.
- **D-0088** (2026-07-31) — **Truncation invariance: decisions on a prefix must
  equal the decisions the full series made over that prefix, compared
  bit-for-bit.** The second half of block A (D-0087), and a different *kind* of
  question from the permutation null.
  **Permutation asks whether a result is extreme; truncation asks whether two
  decision streams are equal.** One answers with a p-value and a threshold, the
  other with yes/no and the bar where it stopped being yes. Permutation is
  statistical and can be fooled by a strategy that reproduces its edge on every
  draw; truncation is not statistical at all — it is a determinism property.
  **Which end, and why.** The **end**. Lookahead flows from the future into the
  past — a full-sample statistic, a peek at tomorrow's close, an indicator
  fitted on everything — so deleting the future is exactly the perturbation
  that exposes it, and a strategy that used no future information cannot notice
  the deletion. Truncating the **start** would measure warmup sensitivity: a
  real property, a lesser one, and a strategy failing it is usually just
  under-warmed rather than leaking.
  **Bit-identity, with no tolerance knob.** Decisions compare as **bytes**
  through `Decision::encode`. A strategy whose orders differ by one tick after
  the future is deleted has read the future, and a tolerance that forgives one
  tick forgives exactly the defect the harness exists to find. Any tolerance
  would be a §9 decision with an entry behind it, never a default. Floats never
  reach the comparison: an `OrderIntent` is a side, an integer quantity, a kind
  and an optional integer bracket, so byte equality is exact and
  platform-stable (§2.3).
  **Faithful by decoration, not reconstruction.** `Recording<S>` wraps the real
  strategy and records what the real engine acted on, against the real
  `PortfolioView`. Reconstructing the decisions would have measured the
  reconstruction — the same argument D-0071 makes about rebuilding equity from
  OHLC.
  **The converse control ran first**, and it is what makes the rest mean
  anything: `SmaCross` — trailing windows only — compared **189 decisions across
  four cuts with 0 divergences**. The detector then fired: `LeakyZScore`, refit
  per prefix, produced **4 divergences**, one at every cut. And the third case
  names the cause: holding the fit **fixed** while still deleting the future
  gives 8 comparisons and **0 divergences**, so what the detector caught is the
  full-sample fit and not the strategy's shape (§7's "add the third case").
  **A mutation found a real hole, and the hole is the interesting part.**
  Mutating the equality to compare **timestamps only** — dropping `bytes` — left
  **every test green**. The detector had been firing on *when* the leaky
  strategy decided and never on *what* it decided; a leak that changed size or
  side without changing timing would have walked straight through. That is a
  missing control, not a passing test, so
  `a_content_only_leak_is_caught_because_decisions_are_compared_byte_for_byte`
  now plants exactly that defect: a strategy trading on a fixed schedule whose
  **quantity** depends on the length of the series it was built for. It diverges
  at timestamps both runs decided on. With it in place the mutation is caught.
  **Mutations, each watched failing and restored byte-exactly.** (1) equality
  inverted (`!=` → `==`): caught by three controls. (2) `bytes` dropped from the
  comparison: **initially survived** — see above — and is caught after the
  content-only control was added.
  **Pinned determinism hash `91b9ff5b9bbcdb25`**: blake3 over the comparison
  count, the divergence count, the cut points, and the full decision stream
  itself, for `SyntheticFeed::random_walk(seed 29, 3,000 bars, 1m, start
  5000.00 pt, tick 0.25, vol 4)` replaying `SmaCross(20, 50, qty 1)` under
  `FreeFills`, cuts `[1200, 1800, 2400, 2800]`. The stream is hashed and not
  only the counts, so a change in *what the strategy did* fails here even when
  the number of comparisons happens to match. The six existing hashes are
  unmoved — this harness observes runs and does not change them.
  **Number assigned directly rather than as a `D-TBD(...)` placeholder**,
  because this landed on `main` through the primary session, which is what §8.2
  allows; the placeholder protocol binds branches.
- **D-0089** (2026-07-31) — **The four commodity
  calendars are backfilled to the archive's first date, 2010-06-06, with
  `[[calendar.era]]` entries and three earlier holiday regimes; and D-0086's
  equity-index Thanksgiving closure is corrected to start in 2012.** (extends
  D-0086)

  D-0086 built `cme_globex_energy`, `cme_globex_metals`, `cme_globex_fx` and
  `cme_globex_rates` and started three of them on 2015-09-21 and the fourth on
  2011-10-03. The archive starts 2010-06-06, so 5.3 of its 16.1 years were
  answered by a template that did not describe them — about 145 of the first 423
  contracts an archive-wide sweep touches. `docs/SESSION_ERAS.md` §5 is the
  workbook; every count below is reproducible with
  `crates/crucible-data/examples/session_profile.rs`, read-only.

  **Eras.** CL and GC gain one: 17:00 → **16:15** CT from 2010-06-06 to
  2015-09-18. ZN gains one: **17:30** → 16:00 CT from 2010-06-06 to 2011-09-30.
  6E gains none — 17:00 → 16:00 for all sixteen years, so it has one template
  and `valid_from` alone moves. No product ever had an intraday halt. The
  2015-09-21 boundary is 78-of-78 dates trading 16:00–16:15 CT before it and
  0-of-200 after, for CL and GC separately; the 2011-10-02 one is 0.00 % on
  every minute from 16:01 to 17:29 across 338 ZN dates, then 17:30 at 80 %.

  **The one third-party source, and what it is for.** ATAS, published
  2015-09-20, accessed 2026-07-31, names the product groups the 2015 advisory
  covered — "CME Equity, CBOT Equity, COMEX, NYMEX". The archive independently
  says CL and GC moved and 6E and ZN did not, and CME FX and CBOT rates are
  absent from that list. Two measurements of different kinds naming each other
  (§7's third case). Nothing was fetched from cmegroup.com, by the same ToU
  ruling the margin work follows.

  **Holiday regimes: four, not two, and the middle one is shared.** Before 2013
  energy and metals closed **12:15 CT** on a US holiday and FX and rates
  **12:00** — fifteen minutes, for three years, published nowhere found. Then
  **all four products were fully closed** on the same nine dates, 2012-11-22
  through 2014-02-17, `EVIDENCED` twice on each (no day session *and* no session
  on the evening before). Then the 12:00 regime D-0086 already encoded. Written
  as `first_year`/`last_year` pairs that never overlap, so which entry fires
  never depends on file order.

  **The equity-index correction.** Checking those nine dates against ES and NQ
  found `cme_globex.toml`'s `Thanksgiving Day (closure era)` starting in 2013
  when 2012-11-22 was a closure too — three trading days after that table's own
  `valid_from`, so the calendar reported a normal session on a day the exchange
  was shut, inside the span it claims to describe. `first_year = 2012` now. The
  four commodity tables found a bug in the equity one, which is the argument for
  measuring the same question five ways. Measured on ESZ2012 over
  2012-11-19..2012-12-01: coverage **88.894 % → 99.003 %**, and the month's
  largest reported gap — 1,335 bars from 2012-11-21T23:00Z to 2012-11-22T21:15Z
  — was the phantom session and is gone. `qa` had been printing it since D-0086
  landed, and it read as thin data rather than as an invented trading day.

  **What moving `valid_from` costs, stated because it is not zero.** 26 dates
  for 6E and ZN and 6 for CL and GC closed at 15:15 CT before a holiday, in a
  pattern with three regimes and a hole in it (Columbus Day in 2010, 2011 and
  2012 but never after; Thanksgiving eve in 2010 but not 2011 or 2012; CL and GC
  stop four years before 6E and ZN). Unencoded, listed by date. That is 45 min ×
  26 and 60 min × 6 of session the calendar now claims and the exchange did not
  hold — reported as *missing bars*, the error that blames the archive. Against
  it: ~20,000 minutes per contract of **real bars** that the old `valid_from`
  reported as *outside any session*, which is the error that indicts the
  calendar (D-0040) and is the one this project cares about. On May 2011,
  measured both ways five minutes apart with only the two TOML files swapped:
  CL 202 → **0** out-of-session bars, GC 283 → **0**, ZN coverage 86.19 % →
  **88.11 %**. Refusing to guess a rule from a three-regime pattern stays the
  cheaper error, exactly as D-0086 said of the sixteen dates it could see.

  **The remaining unverified claim, named.** ZN's 17:30 open rests on the
  archive alone: no CME document, no third-party coverage, and no cftc.gov rule
  filing for it was found. Its `source` field says `UNVERIFIED` in those words,
  and `SESSION_ERAS.md` §6 lists the four questions a hand-fetch would settle
  and refuses to construct a URL for any of them.

  **One evening is wrong and is asserted rather than discovered later.**
  `Calendar::trading_day` picks the era from the calendar date an instant falls
  on — its own doc comment calls this a cost of at most one session per era — so
  17:00–17:30 CT on Sunday 2011-10-02 reads closed while
  `open_intervals(2011-10-03)` correctly opens at 17:00. Thirty minutes, once,
  at the only bundled boundary that moves an *open* time.
  `the_rates_open_moved_from_seventeen_thirty_in_2011` pins both halves.

  **Nothing numeric moved.** Every `reference_span` is 2016-01-01..2026-01-01
  (2022-01-01 for equity index) and every new era boundary, `first_year` and
  one-off falls outside it, so `bars_per_year` is unchanged for all five
  calendars. ESH2024 January 2024 is bit-identical (30,167 bars, −23.51 %, 665
  round trips, fees $1,663.75); `demo b55747513df596ed`,
  `combo 0e1ab52d474b862b`, `walk-forward 711e1cb34a2ee2b4` all unmoved.
- **D-0090** (2026-07-31) — **A contract's expiry is a function of when you ask.
  `expiry(contract, decision_ts)` is the latest `definition` record whose
  `ts_recv` is not in that decision's future.** *Supersedes D-0046's expiry
  clause — the refusal on a contract "defined twice with different expiries".
  D-0046's contract-symbol year rule, its expiry backstop, and its
  spread-skipping are untouched, as is D-0072's cycle reader.*

  ```text
  expiry(contract, decision_ts) =
      records.filter(|r| r.ts_recv <= decision_ts)
             .max_by_key(|r| r.ts_recv)
             .expiration
  ```

  **THE TRAP, first, because it is one word wide.** **The key is
  `max(ts_recv)`. It is never `max(expiration)`.** D-0054's "dedup keeps
  `max(created)`" invites the second spelling by analogy, and it would be wrong
  in **4 of 4** real cases: in every one of them the LATER record carries the
  EARLIER expiry, because the exchange pulled the settlement forward and the
  vendor restated the contract. A `max(expiration)` implementation therefore
  selects the **stale** record *every single time* — not half the time — and
  moves ZN's and 6E's rolls onto the wrong session with nothing visible in the
  output. `the_key_is_max_ts_recv_and_never_max_expiration` (`continuous::
  expiry`) and `a_restatement_moves_the_calendar_roll_and_the_key_is_ts_recv`
  (`continuous::roll`) both fail on it; both were watched failing.

  Do **not** import D-0052/D-0054's "conservative direction" justification
  either. That argument is about *availability* — delay can only cost measured
  performance, never fabricate knowledge. Here the later record moves the expiry
  *earlier*, which is neither conservative nor aggressive: it is simply correct,
  because the exchange amended its calendar and the vendor said so. What D-0054
  does contribute is the shape of the **residual** refusal, below.

  **The measurement.** All seven archived `definition` files, decoded through
  the production decoder (`databento::dbn`, D-0031): **4 contracts of 1,002
  (0.40 %)** carry two `expiration` values — `GCX2021` (1 h apart), `ZNZ2011`
  (24 h), `ZNM2012` (24 h, previously hidden behind `ZNZ2011`), `6EM2023`
  (72 h). ES, NQ, RTY and CL are genuinely clean. All four are **revisions of
  one contract**: same `instrument_id`, same `raw_symbol`, disjoint and strictly
  ordered `ts_recv`, exactly one flip in file order and never a flip back. The
  vendor marks the transition `security_update_action = 'M'` on `GCX2021` only —
  on the other three both groups carry `'A'` — so **that field does not
  distinguish them; `ts_recv` does**. Our own bars corroborate independently:
  the contract's last curated 1m bar lands 1–2 minutes before the *newer* expiry
  and a full session or more before the older one, in all three cases where the
  archive can speak. The `rolls` report now prints the statements, e.g.
  `ZNZ2011  known 2010-09-05 -> expires 2011-12-21 (24 records) /
  known 2010-10-03 -> expires 2011-12-20 (384 records)`.

  **Why the availability filter and not plain "latest wins".** Verified to
  coincide with plain latest-wins on **100 %** of this archive — every
  correction landed between 14 and 18 months *before* its own roll — so it costs
  nothing today. It is chosen anyway because it **refuses to absorb a correction
  that lands after its own roll**, which is §2.1 lookahead exactly, and which
  plain latest-wins cannot even detect: never reading `ts_recv`, it has no way
  to know. `late_expiry_corrections` makes the refusal countable, and `rolls`
  prints `expiry corrections landing after their own roll: 0` rather than
  silence, because a detector nobody has seen report anything is decoration
  (§7). The planted case is `a_correction_landing_after_its_own_roll_does_not_
  move_it`.

  **The residual refusal, which is D-0054's shape.** Two statements are a
  *revision* when their availability windows are disjoint. They are a
  **conflict** when the windows overlap — then the source asserts both at some
  instant, there is no latest, and `ExpiryConflict` refuses. That is the direct
  analogue of D-0054's "the same `(contract, created)` twice is a different bug
  and refuses the file". The refusal now names **every** offending contract in
  the root, never the first: `expiries_from_definitions` returned on the first
  `BTreeMap` hit, which is precisely why `ZNM2012` sat unreported behind
  `ZNZ2011` from the day the archive landed, and a refusal that reports one of
  two makes the archive look better than it is. A record with no `ts_recv` at
  all has no answer to "as known when?" and refuses too
  (`UnavailableExpiry`) — on the roll path only; the cycle reader never asks.

  **The `.c` fixed point, and why it needs no iteration.** `due` depends on the
  expiry and the expiry now depends on the decision instant, so the calendar
  rule reads as circular: `D = g(f(D))`. It is not, because **the candidate set
  does not depend on the expiry**: a candidate is a session both contracts
  traded, and its decision instant `max(front.avail_ts(day), next.avail_ts(day))`
  comes from the bars alone. The equation therefore decomposes into one
  independent test per candidate — *given what was knowable at this candidate's
  own instant, was the roll due by this session?* — a candidate that passes is a
  solution, and the answer is the earliest one, because a roll is the first time
  a rule fires. `calendar_roll_day` resolves it in a single ascending scan,
  bounded by the front contract's session count, with no iteration to diverge
  and no ordering or float dependence (§2.2). There is consequently no "did not
  converge" refusal to write: there is no loop that could fail to converge. A
  session whose instant precedes every statement is simply not due — a rule
  cannot fire on an expiry nobody had published — so the scan continues; a
  contract the source never names at all is still `MissingExpiry`.

  **Expiries are now resolved LAZILY, per rule** (`RollRule::reads_expiries`).
  `build_roll_table` reads `input.expiries` only in the
  `CalendarDaysBeforeExpiry` arm, so a `.v` table is byte-identical under any
  history — including an absurd one, which is the control. But
  `crucible-cli::rolls` resolved `--expiries auto` *unconditionally*, so
  `crucible rolls --root GC` (which defaults to `.v`) exited 4 on an input it
  provably never read. That is fixed by deciding from the rule rather than the
  flag; the flag's *spelling* is still checked either way, because a typo costs
  nothing to catch. This weakens no refusal — it stops one firing where the
  refused input is unread. A `.v` table now records `expiry_source = "none"`,
  which is the truth: it consulted none.

  **Executed on the live archive.** All seven roots build both `.v` and `.c`
  (`rolls --write`, 14 tables), where GC, ZN and 6E previously exited 4 on both.
  Measured against the forensic's predictions, and reproducing them exactly:
  `.v` — **0 roll rows differ** on every root; the four pre-existing tables are
  byte-identical but for the `expiry_source` string. `.c` at 8 days — **GC 0
  rows differ** (its two statements are 1 h apart and fall on the same date, so
  `expiry_day − 8` is unchanged), **ZN 2 rows differ**
  (`ZNZ2011 -> ZNH2012` 2011-12-12, gap −0.796875; `ZNM2012 -> ZNU2012`
  2012-06-12, gap −0.8125), **6E 1 row differs**
  (`6EM2023 -> 6EN2023` 2023-06-08, gap unchanged at 0.00195, date moved two
  sessions). ES `.c` is byte-identical to the table built before this change.
  All three determinism hashes unmoved (demo `b55747513df596ed`, combo
  `0e1ab52d474b862b`, walk-forward `711e1cb34a2ee2b4`), and the 16-year
  `ES.v.0` backtest is identical line for line under the old and rewritten roll
  tables save the header's `expiries` label — ES has no restatements, so
  nothing could move.

  **Representation, and why it is allowed.** A 16-year `definition` file
  restates every listed instrument every session — `6EM2023` alone is 1,580
  records — so `ExpiryHistoryBuilder` keeps one entry per *distinct* expiry with
  the first and last `ts_recv` that stated it, rather than one per record. That
  collapse is lossless for `as_of` exactly when the windows are disjoint, which
  is the same condition the conflict check enforces, so the check earns the
  representation rather than the representation assuming the check.
  `the_collapsed_history_answers_exactly_as_the_raw_records_would` proves it
  against the rule written out verbatim over raw records, at every boundary.

  **The third side (§7).** `max(ts_recv)` and `max(expiration)` disagree on the
  real data's direction. `the_two_keys_agree_when_a_correction_moves_the_expiry_
  later` shows they **agree** the moment the correction points the other way —
  so what makes them differ is the *direction* of the correction and nothing
  else about the fixture, and the reason this archive is dangerous is that all
  four of its corrections point the same way.
- **D-0091** (2026-07-31) — **`open_intervals` calls `session_open` and
  `session_close` again: D-0077's one-definition invariant restored BY
  CONSTRUCTION, not by monitoring.**
  **What the invariant is.** A bucket grid anchored on the session open must
  never disagree with the intervals a coverage check walks. D-0077 secured that
  the only way it can be secured — by having one definition of the two instants
  and calling it twice — with a comment saying so, per D-0071's argument that
  two independent attributions of the same fact is how two reports come to
  disagree about a date.
  **How it was lost, which is worth recording because nothing was ever wrong.**
  The D-0086 eras merge hit a real conflict at exactly this site: main's
  two-line version read flat `Calendar` fields (`self.shape`, `self.open_local`,
  `self.close_local`) that the eras branch had **deleted** in favour of per-era
  templates, while the branch's version was era-aware but computed the bounds
  inline. Taking the branch's side wholesale resolved the conflict and silently
  replaced a *call* with a *copy*. Both copies were era-aware and produced
  identical instants — no output ever differed — so the defect was not a wrong
  number. It was that a wrong number had become **expressible**: an edit to
  `session_close` and not to `open_intervals` would have desynchronised the
  bucket grid from the coverage walk with nothing to catch it.
  **The repair is the call, not a test.** Line-for-line the two computations
  were already identical — `session_open`'s shape/open handling and
  `session_close`'s `EarlyClose` handling are what the inline copy had
  reproduced — so unwinding was mechanical and needed no structural
  concession. A tripwire comparing the two implementations across era
  boundaries was the sanctioned fallback and is **not** used, deliberately:
  monitoring a divergence is strictly weaker than making it unsayable, and a
  tripwire would have institutionalised the duplication it was watching.
  `era_of` is still read inside `open_intervals`, for `halts_local` — halts
  *inside* a session are that function's own concern, not the session bounds.
  **Nothing moved.** All seven pinned hashes byte-identical (demo
  `b55747513df596ed`, combo `0e1ab52d474b862b`, walk-forward
  `711e1cb34a2ee2b4`, funnel `2f430893d2a79a8f`, S0 `91107aeb6e6802c0`,
  permutation `9fe41f6f5b3653e7`, truncation `91b9ff5b9bbcdb25`); 772 tests
  green on both clippy profiles; `qa ESH2024` still reads 27.755 % coverage and
  the 15m resample still emits 2,012 bars from 30,167 source bars over 23
  trading days. A refactor that changed a number here would have been the
  invariant failing, not the repair succeeding.
- **D-0092** (2026-07-31) — **The T0 pull's 794 missing greeks records were
  REFUSALS, not vendor-absent data — and the reason died with the process.
  Telemetry added so the next one cannot.**
  **The verdict, reached from the code rather than from a relaunch.** The first
  T0 run attempted 83,489 requests and inventoried 82,695. `eod` and
  `open_interest` reconcile **exactly** to the dry run (31,851 each);
  `greeks/eod` is 794 short, and the shortfall lands on four roots — SPX -267,
  VIX -360, SPXW -84, RUT -83 — summing to precisely 794. Every root reaches the
  plan's end date, so nothing was truncated.
  **What settles it is `run_tranche`'s outcome handling, not a hypothesis.**
  `Outcome::Written` appends an inventory line. `Outcome::Empty` appends one too
  — `empty_record`, whose own comment says *resume keys on the request, so this
  is enough to stop the same empty question being asked every run*. So a
  genuinely vendor-absent date **would be in the inventory**, with an empty
  `file_path`. `Outcome::Refused` appends **nothing**, by design: the halt
  message says *Nothing refused was inventoried, so a fixed build resumes
  cleanly*. A request missing from the inventory entirely was therefore
  **refused**, and refusal has exactly three sources — a client error that is
  neither `is_no_data()` nor `CircuitOpen`, a validation failure, or a Parquet
  write failure.
  **Corroborated by the expansion.** `measured_greeks_floor` gives SPY, IWM,
  SPX, SPXW, VIX and DIA the same floor of 2017-01-03, and the observed first
  greeks date for all six **is** 2017-01-03. Same floor, same end date, same
  calendar means the same number of planned requests. DIA, IWM and SPY each have
  2,385; SPX, SPXW and VIX have fewer. The requests were expanded and issued;
  some did not come back.
  **The design is right and is not changed.** Not inventorying a refusal is what
  makes resume-by-diff correct: the next run re-requests exactly those 794. What
  was wrong is that the *reason* lived only in `RunReport.refusals`, an
  in-memory `Vec` printed to a console that no longer exists.
  **The rate is the part worth pinning.** 794 / 83,489 = **0.951 %**, below
  `REFUSAL_RATE_LIMIT` (2 %), so the systemic breaker correctly did **not** trip.
  A sub-threshold refusal rate is precisely the case that needs a durable
  record: too small to halt the run, too large to be nothing. The breaker
  protects against a build misreading the feed; it was never meant to be the
  only witness.
  **`external::thetadata::telemetry`.** `heartbeat.txt`, overwritten every
  `HEARTBEAT_EVERY = 25` records with a timestamp and the four counts; and
  `last_exit.json`, one JSON object written on every exit path — completed,
  halted, failed — carrying `kind`, `reason` and the same counts. Both are
  **best-effort and never branched on**: a monitor that can break the thing it
  monitors is worse than no monitor. Both are **operational telemetry, not
  research records** — overwritten freely, carrying no provenance, and nothing
  downstream may read a number out of them into a result. `inventory.jsonl`
  remains the append-only archive (D-0074's shape), unchanged.
  **The heartbeat's real job is to remove a temptation.** The reason this
  project nearly lost a live pull is that a default Windows reader on a file a
  writer holds can deny the writer. `heartbeat.txt` exists so nobody ever has a
  reason to open `inventory.jsonl` to ask whether it is still going, and
  `the_heartbeat_is_readable_while_a_writer_holds_it` pins exactly that: stat
  and read both succeed while a writer holds the file, and the writer survives.
  **The counts travel as one struct.** `refused` only means something beside
  `attempted` — 794 is a catastrophe out of 800 and a rounding error out of
  83,489 — so a signature that let a caller supply one without the other was an
  invitation to the mistake this entry is about. Clippy's arity lint prompted
  the change; the reason to keep it is not the lint.
  **Still owed, and stated rather than implied**: the panic path.
  `ExitKind::Panicked` exists and nothing writes it, because the CLI installs no
  panic hook — verified, not assumed. A panic still leaves no exit record. That
  is the one gap this entry does not close.
- **D-0093** (2026-07-31) — **The inventory gains a third kind of line: an
  ABSENCE record**, carrying `absence` (a cause enum) and `absence_detail` (the
  refusal verbatim). Plan-minus-inventory treats it as satisfied.
  **Why.** A line could previously say two things — *here is a file*, or *asked,
  answered, nothing there* (the vendor's 472, written with an empty
  `file_path`). A refusal said nothing at all, which is why the T0 tranche's 794
  missing `greeks/eod` records were indistinguishable from requests never
  attempted, and why every resume re-issued all 794 and got the same answer
  (D-0092; the measurement is `docs/T0_REFUSAL_FORENSIC.md`). An absence record
  is the third statement: **asked, answered, and the answer cannot become
  archive — here is why.**
  **The mechanism is deliberately boring.** Resume keys on the rendered request
  string, and an absence record carries one, so it settles its request with no
  special case anywhere in the diff. `absence_records_satisfy_the_plan_and_drive_outstanding_to_zero`
  is that arithmetic as a test, at the real width: 794 requests outstanding, 794
  absence records appended, 0 outstanding, **zero network requests issued**. A
  fixture of three would have passed while proving nothing about the case this
  exists for.
  **The cause is an enum, not free text.** The reason string is recorded too —
  a human needs it in order to disagree with the classification — but a reader
  aggregates on the cause, and free text cannot be counted. Two causes were
  measured and they need different remedies (D-0094, D-0095); collapsing them
  into "refused" would destroy the only distinction that decides what to do.
  **Classification refuses to guess, and that is the load-bearing half.**
  `absence_cause_of` returns `None` for any error it cannot name, and a `None`
  writes no record — so the request stays outstanding and is asked again.
  Absence is a claim that data cannot be had; a build may only make it where it
  has been measured. `the_two_causes_are_distinguishable_and_an_unknown_refusal_is_neither`
  pins both directions, including that a `MalformedRow` — this build failing to
  read, not the vendor failing to have — is never an absence.
  **Classification is by error variant and never by message text**, so that
  improving a sentence cannot silently reclassify an archive.
  **The cost is stated rather than hidden**: a build that later learns to read
  one of these responses will not re-fetch it on its own. Removing an absence
  class is a deliberate act — delete those lines, or widen the plan — never
  something that happens by accident. `absences_by_cause` exists so that "0
  outstanding" can never be read as "everything was acquired", and the run
  report prints the count **even when it is zero**, for the reason §9's
  intrabar-convention line gives.
  **Reader-first (§8) is satisfied by construction and checked, not assumed.**
  The on-`main` reader is a field-extractor that ignores unknown fields and maps
  a missing or `null` field to `None` — which is exactly what the 82,668
  existing lines meant. `a_v1_line_without_the_absence_field_reads_as_data`
  parses a hand-written pre-absence line and asserts it is data, not absence:
  the failure this guards against is a v1 line being read as a settled absence,
  which would retire a request nobody settled.
- **D-0094** (2026-07-31) — **Cause A: a response whose every row carries the
  vendor's failed-solve sentinel is recorded as `SolverSentinel` absence.** The
  refusal itself is unchanged; what changes is that it now leaves a record.
  **Why not repaired instead.** `is_zero_sentinel` is
  `underlying_price == 0.0 || iv_error >= IV_ERROR_SENTINEL`, and there is a
  pre-existing test pinning `is_zero_sentinel(4742.83, 100.0) == true` — a real
  underlying beside a failed solve is a sentinel **by intent**, not by accident.
  On the measured days (SPX 2020-10-29, 2021-03-02, 2021-12-23) `implied_vol`
  and `iv_error` are exactly `0.5000` and `100.0000` across the entire
  chain-day: an IV solver that never converged and emitted its initial guess
  with a maximum-error flag. Greeks derived from a non-converged fit are not
  data, so nothing is salvaged into the archive and **no sentinel row is ever
  written** (`planted_a_whole_chain_of_failed_solves_refuses_and_writes_no_row`).
  **The salvage option, recorded because it was asked for and is real.** The
  `underlying_price` on these responses IS correct (SPX 3870.29 on 2021-03-02),
  and the contract set reconciles with `eod` at exact parity. A search for
  consumers found **none**: `transcode.rs` maps the column's type on the way
  out, `validate.rs` reads it only for this sentinel, and nothing in
  `crucible-engine`, `crucible-strategies` or `crucible-funnel` reads greeks at
  all. So absence is recorded **whole**, per the ruling. What makes this worth
  revisiting rather than closing: `external::thetadata`'s own module doc records
  that the Index tier is FREE — `/index/history/price` answers 403 — and that
  "the greeks endpoints carry an `underlying_price` column, so the SPX/VIX/RUT
  levels arrive with the option data anyway". That is a *planned* consumer with
  no code yet. If it is built, these days hold a real index level behind an
  unusable IV surface, and the file-level gate is all-or-nothing. Splitting the
  channel is a decision with its own control, not a tweak.
  **The converse control is what keeps the gate honest.**
  `one_converged_solve_is_enough_to_keep_the_file` asserts a single ordinary
  `iv_error = 0.0006` row among failures keeps the file — without it, a gate
  that refused every `greeks/eod` response would pass the planted control and
  nobody would know.
- **D-0095** (2026-07-31) — **Cause B: one identity repeated at ONE
  discriminator value collapses keep-first when the rows are byte-identical, and
  refuses as `AmbiguousDuplicate` when they differ.** (narrows the blanket
  refusal D-0054's dedup left in place)
  **Why, and it is a fact about the bytes rather than a preference.** SPX
  `greeks/eod` 2023-09-19 answers with 12,492 rows over 6,246 identities —
  **6,246 distinct FULL rows**, so every duplicate half is byte-equal, and the
  same holds for 2023-09-27. Two rows that agree in every column are one datum
  written twice: keeping the first is deterministic, loses nothing, and is not
  an adjudication. Two rows that *disagree* about one contract at one instant
  cannot both be true and nothing ranks them, so the file is refused and
  recorded as an absence — **no guessing, per the ruling.** The differing case
  has never been seen on this archive and is implemented and controlled anyway,
  because "never seen" is a statement about the past.
  **These days are recoverable data, and that is the point.** The measured 2023
  rows carry `iv_error = 0.0006` and a real underlying — ordinary data that was
  being refused for a vendor artefact. Cause B is a repair; cause A is not.
  **Two pre-existing merge-blocking controls changed meaning, and were
  rederived rather than relaxed** (§7's "decision-log entry + rederived
  values"). `planted_duplicate_contract_with_the_same_build_stamp_refuses_the_file`
  and the `open_interest` uniqueness gate both planted **byte-identical**
  repeats and asserted a refusal. Each was narrowed to plant a *differing*
  repeat — preserving the original intent, that a repeat nothing explains is
  refused — and each gained a companion asserting the identical case now
  collapses. The pair is the point: without the companion, "the gate fires" and
  "the gate fires on everything" look identical.
  **`identical_repeats_collapsed` is counted, not silent.** Nothing in the
  vendor's model explains why one build emits one contract twice, so a rate that
  moves is a vendor change nobody announced — the same argument the dup-rate era
  fingerprint already makes.
- **D-0096** (2026-07-31) — **A halted run stops ISSUING requests, not merely
  stops keeping them.** `fetch_streaming`'s callback returns `ControlFlow`, and
  a `Break` aborts the `JoinSet`.
  **Why.** The breaker's own rationale was that the window "gives the
  refusal-rate breaker a place to stop the run rather than draining 80,000
  fetches it has already decided not to keep". Measured on 2026-07-31, it did
  not: `BREAKER_CHECKPOINT` is 2,048 and the outstanding set was **794**, so the
  whole run was ONE window — every request was already spawned into the
  `JoinSet` before the first response arrived. The breaker tripped at attempt
  200 and the remaining 594 were fetched over the network and discarded
  unprocessed. The guarantee held for a large tranche and was vacuous for a
  small resume, which is exactly the shape a resume has.
  **A return value, not a shared flag**: the decision is made inside the
  callback on the result it was just handed, and an `AtomicBool` would be the
  same thing with a race in it. Aborted tasks surface as
  `JoinError::is_cancelled` and are ignored — without that they would hit the
  `debug_assert!` that exists to catch a panicking fetch task, turning the
  repair into a debug-build panic.
  **Nothing in flight is written**, so resume is unaffected: an aborted request
  was never inventoried and is picked up untouched.
  **The breaker's rate now excludes classified absences** (D-0093), and this is
  not a loosened threshold. A resume re-attempts *only* the previously-refused
  set, so its refusal rate is ~100 % by construction; counting absences would
  halt every repair run at attempt 200 — the run that is *fixing* the 794. What
  the breaker still asks is the question it was written for: "am I misreading
  the feed?", where a refusal this build cannot name is the evidence.
- **D-0097** (2026-07-31) — **Deflated Sharpe lands as an estimator with its
  controls, and is NOT yet wired into a scorecard number.** Block B's first half;
  PBO/CSCV is deliberately not in this commit.
  **The estimator.** Bailey & López de Prado (2014), in
  `crucible-funnel::stats::deflated`: an observed Sharpe is corrected for the
  number of trials it was selected from, and for the skew and kurtosis of the
  returns that produced it, giving `P(true Sharpe > 0)`.
  **The trial count has exactly one door.** `trials_from_registry` is a one-line
  wrapper over `Registry::trials_for`, and it exists so that "never trust a
  hand-entered trial count" is enforced by there being no other way in.
  `trials_for` excludes voided runs **by construction** (D-0083), and
  `voiding_a_trial_raises_the_deflated_sharpe` is the proof that the exclusion is
  real rather than cosmetic: 8 trials → 7, DSR **0.916187 → 0.926764**. It lives
  here rather than in the registry because here is where the number is consumed.
  **The converse control came first, and it caught a real bug — which is the
  entire argument for writing it first.** `expected_max_z` is a **z-score**:
  dimensionless, in units of the trial Sharpes' dispersion. The first
  implementation compared it directly against an observed Sharpe — a z-score
  against a ratio — and the planted real edge (2,000 observations, true Sharpe
  0.12, ≈5.4σ) scored **DSR 0.000000** against a benchmark of 1.98. A statistic
  that condemns everything passes every detector test ever written, and without
  the converse the lucky-best-of-N control below would have "passed" on a
  constant. The fix is that the benchmark is `dispersion × expected_max_z`, and
  `expected_max_sharpe`'s old name is gone so the units cannot be confused again.
  **The dispersion fallback is stated, not silent.** The paper's benchmark uses
  the variance of the trial Sharpes; where only the winner is in hand,
  `trial_sharpe_dispersion: None` substitutes the estimated standard error of
  that one Sharpe. Where a grid's combos are near-duplicates — exactly the
  account dimension's case — the true dispersion is smaller, so the fallback
  deflates *harder* than the paper requires. Accepted, in the plan's declared
  direction: the preferred error is against the strategy.
  **Measured, on seeded fixtures, all printed by the tests:**
  a planted real edge scores **DSR 0.999882** at 24 trials and **0.979154** at
  3,840 (240 combos × 16 accounts, Block E's worst case) — so the correction is
  not so aggressive that a real edge can never pass. The lucky best of **40
  zero-edge** strategies scores a naive Sharpe of 0.1068 with **t = 2.39** — it
  looks significant — and **DSR 0.567937**, which is what forty coin flips
  produce. Trial-count sensitivity: the same run reads **0.584767** at one trial
  and **0.038726** at twenty-four.
  **One thing is deliberately NOT asserted**, and the reason is statistical
  rather than convenient: the detector control does not require the zero-edge
  winner to fall *below* the point benchmark. The expected maximum is a **mean**,
  so the realised maximum of N zero-edge trials exceeds it roughly half the time
  by construction — here 0.1068 against 0.0990. The verdict is the DSR, which
  divides that gap by the Sharpe's standard error. An earlier draft asserted the
  point comparison, it failed on this fixture, and the assertion was wrong rather
  than the statistic.
  **Pinned hash `dc7f94f25235df6c`**, blake3 over the seeded series' moments and
  the (benchmark, standard error, DSR) triple at trial counts 1/24/240/3,840.
  Inputs: `ChaCha8Rng::seed_from_u64(11)`, 2,000 Box–Muller draws at mean 0.12
  and unit vol. The other seven hashes are byte-identical and unmoved.
  **Mutation-verified, each watched failing and restored byte-exactly:** the
  selection correction's **sign** (benchmark added rather than subtracted), the
  **trial-count exponent** (benchmark computed at `n_trials = 1`), the **skew
  sign** (negative skew narrowing rather than widening the error bar), the
  **units regression** (benchmark unscaled by dispersion — the original bug), and
  the **void** ceasing to reach the count. Each killed exactly its own control
  and nothing else.
  **The scorecard renders the trial count's COMPOSITION**, not a bare integer:
  `N combos × N accounts × N folds × N seeds − voided = total`. §4 makes every
  account a trial (D-0067), so Block E's sixteen accounts will multiply the count
  and drop every deflated Sharpe — correct, and a *stated* consequence rather
  than a surprise. Accounts, folds and seeds render as `1` in this build because
  that is what this build does, and rendering the ones is the point: the line
  changes visibly when the dimension lands.
  **The scorecard does NOT claim a deflated number, and this was nearly got
  wrong.** The honesty box was first rewritten to say the DSR is "computed per
  combo" — while nothing feeds per-combo returns to the estimator. That is a page
  claiming a number it does not show, which is the exact failure this module's
  own doctrine names. It now reads "the estimator exists, the wiring does not",
  says every Sharpe on the page is naive and an upper bound, and the scorecard
  test asserts that phrase — so the prose cannot drift ahead of the arithmetic
  again without a test failing.
  **PBO/CSCV is not in this commit**, per the instruction that it waits until
  deflated Sharpe lands complete with both controls. When it comes, its fold
  structure must respect the existing `FoldPlan` machinery or be flagged under
  §9 — no quiet fold redefinition.
- **D-0098** (2026-07-31) — **Operational telemetry lives in a decided root
  directory, `telemetry/`, archive-wide and not per vendor.**
  **The problem it fixes.** D-0092 wrote `heartbeat.txt` and `last_exit.json`
  bare at the archive root. `layout-check` rule 7 — "anything unrecognized at the
  archive root" — refused both, so **every run exited 4**, permanently. A red
  check that is always red trains a reader to ignore it, which costs more than
  the thing the check was watching. Three documents disagreed at once: the
  module doc said "inside the tranche's data directory", the files were at the
  root, and `DATA_LAYOUT.md` had never heard of them.
  **Why a new root entry rather than widening the parser.** Rule 7 exists to
  catch exactly this — the tree growing something nobody decided on — so
  tolerating bare files at the root would delete the rule to silence its one
  true positive. The fix is to *decide*, which is what `staging/` and
  `delivery/` already did for operational non-data: not depth-checked, never in
  `manifest.jsonl`, documented as siblings. `telemetry/` joins them on the same
  terms and for the same reason: none of the three is an acquisition.
  **Why archive-wide and NOT `external/thetadata/`.** A heartbeat answers "is
  the machine still working?", and that is not a question about whichever vendor
  happens to be running. The next Databento tranche wants the same file at the
  same path; filing it per vendor means a second copy of the same idea the first
  time anything else runs unattended, and — worse — an operator checking whether
  a run is alive would have to already know which vendor was running in order to
  find out whether anything was. Per-vendor placement was considered and
  rejected on that ground, not on tidiness.
  **A latent bug fixed on the way.** Both writers are best-effort and swallow
  their errors, so once the path gained a directory component a missing parent
  would have meant writing **nothing, silently** — the exact failure D-0092
  exists to end. Both now create the directory, and
  `the_writers_create_the_telemetry_directory_when_it_is_absent` asserts it on a
  fixture that starts without one.
  **Done when `layout-check` exits 0**, and it does: `layout clean`, 33 raw
  files, 1,751 curated files, 33 manifest records.
- **D-0099** (2026-07-31) — **The spike sigma's defect is RESOLUTION, not
  staleness; the "dominated by calm 2019" account is struck. And `qa` reports
  nothing at all on 47 contracts, which is the larger finding.**
  *Corrects `docs/SPIKE_FORENSIC.md` and the CLAUDE.md §9 entry that repeated it.
  No code changes; the estimator remains deliberately unimplemented.*
  **What was measured.** A read-only `qa --timeframe 1m` sweep over **all 863
  curated contracts**, recording each contract's robust sigma.
  **The mechanism.** `sigma = 1.4826 × median(|Δclose|)` over adjacent
  same-interval bars. Every price is an integer multiple of the tick, so every
  `Δclose` is, so their median is — the estimator **cannot return anything but an
  integer number of ticks**. The whole archive holds **43 distinct sigma values**.
  ES reads 1 tick on 44 of 67 contracts (2011 → 2023, ESH2020 among them), 2 on
  13, 3 on 9, 7 on one; GC spans 1→20 ticks, NQ 1→43, RTY never reads 1. So it
  *is* a volatility estimate, quantised to the tick lattice and floored at one,
  with **no resolution between adjacent integers** — a 40 % volatility change is
  invisible.
  **Three struck claims, each refuted rather than softened.** (1) "Every CL
  contract shares a sigma of 0.0148" — CL takes eight values across 221
  contracts and only 42 read 0.0148. (2) "ESH2020's 0.3707 is computed over
  Jan-2019 → Mar-2020 and therefore dominated by calm 2019" — 0.3707 is
  *identical* on ESH2011, ESM2013, ESU2016, ESZ2018, ESM2021 and ESU2023; it is
  one ES tick and carries no information about any year. (3) The framing that the
  defect is staleness. The arithmetic that made ESH2020 look regime-conflated
  still holds — a 2-point jump in quiet July 2019 sits at 5.4σ — but because 8σ
  is pinned to the tick grid at a constant 2.9656 points, not because a calm year
  diluted a crisis.
  **The verdict the document reached is UNAFFECTED and stands.** The six-contract
  event attributions — the Fed's emergency cut minute, the Sunday-reopen
  limit-downs, all four level-1 circuit-breaker days, negative WTI — were
  clustering arguments about *when* the flagged bars fall. They never depended on
  why the threshold sat where it did. A wrong mechanism did not make the flagged
  moves less real.
  **The larger finding: 47 contracts get no spike check and nothing says so.**
  `if mad <= 0.0 { return; }` fires whenever over half a contract's bars did not
  move — **44 of 68 ZN contracts (65 %)**, a 1/64 tick against quiet minutes,
  plus CLZ2029/2030/2031. (A further 16 contracts of ≤3 bars hit the separate
  `moves.len() < 3` return, and 28 deep-deferred ones are indistinguishable from
  outside; those are benign.) `qa` prints **no `spikes` line at all** in that
  case — not "0 spikes", nothing — so an automated extraction scores the missing
  field as zero and a skipped contract reads exactly like a clean one.
  **4,002,334 of 70,641,676 curated bars (5.7 %) have been reported clean
  without being examined**, and every count in `SPIKE_FORENSIC.md` — the 0–6,974
  range, the ~249 median — is therefore drawn from an **undeclared subsample**.
  That is now stated in the document rather than left to be rediscovered.
  **Why this changes the proposed repair.** "Replace the full-span sigma with a
  rolling one" would not fix ES: a rolling median of ES 1-minute moves is one
  tick in essentially every window too, and a rolling median saturates *harder*
  than a full-span one — fewer samples, more ties at the floor — reaching the
  zero-scale case more often, not less. The general form: **any order statistic
  of a lattice-valued variable is lattice-valued.** An estimator that escapes it
  has to average rather than rank. That design is not specified here.
  **What is owed first, and it is not the estimator.** `qa` should print the
  spike line even when it cannot compute a sigma, saying so and counting the bars
  it skipped. That is a reporting fix, independent of every open statistical
  question, and until it lands no archive-wide spike number can be quoted.
- **D-0100** (2026-07-31) — **Every `qa` check emits an explicit outcome:
  computed, or SKIPPED with a named reason. Silence is banned.**
  *Implements the reporting half of D-0099. Independent of the estimator, which
  remains unimplemented.*
  **The defect.** A check whose precondition failed used to `return` with
  nothing printed. An absent field and a zero field render identically, so an
  automated sweep scored the missing `spikes` line as zero and a human read it
  as "nothing to report" — a contract that was never examined looked exactly
  like a clean one. **4,002,334 of 70,641,676 curated bars (5.7 %)** were
  reported clean without being checked, across 47 contracts, 44 of them ZN.
  **The rule is general, not a patch on one call site.** `SkipReason` and
  `CheckStatus` are the mechanism; `QaReport::checks` records one entry per
  check in run order and the renderer prints every skip unconditionally. Four
  reasons exist today — `ZeroScale`, `TooFewMoves`, `NoCalendar`, `NoBars` —
  and the point is the shape: any future check with a precondition inherits the
  obligation rather than re-deriving it. `moves.len() < 3` will keep skipping
  thin contracts forever and that is fine; it now says so.
  **The reasons stay distinguishable on purpose.** "This contract is too thin to
  measure" and "this build has no scale to measure against" want different
  responses — the first is a fact about the contract, the second is a gap in the
  statistic. A control asserts the two render differently, because collapsing
  them would restore the ambiguity in a new place.
  **The wording is load-bearing.** A skipped spike check renders
  `NOT a clean result — this contract has NOT been checked (D-0099)`. A neutral
  "skipped" would be read as "not applicable", which is what the silence already
  meant. There is a mutation control on exactly that phrase.
  **`NoCalendar` is included although nothing regressed there**, because it is
  the shape that hid D-0072: gold had no bundled calendar, `qa` printed
  `calendar none`, and a partition holding two decades concatenated passed every
  visible check. That path now names itself too.
  **Controls, mutation-verified and each watched failing, restored
  byte-exactly:** the zero-scale skip going silent again (the original bug); the
  "NOT a clean result" wording softened; the two reasons rendering identically;
  every check marked skipped (a detector that fires on everything — caught by
  the converse); and the render loop dropping the skip lines.
  **What this does NOT fix.** Those 47 contracts are still unchecked. The gap is
  now visible rather than invisible, which is the whole of the claim.
  `docs/MILESTONES.md` carries it as a blocker on M4's rates leg, since ZN is
  one of M4's calibration instruments and 65 % of it has never been spike
  checked.
- **D-0101** (2026-07-31) — **A hypothesis registration in `research/backlog/`
  must be a config this build can run, and the runnable config is the SAME
  BYTES as the registration.** Enforced by
  `crucible-cli/tests/backlog_registration.rs`, which reads the requirements
  out of the build via a new `crucible funnel --check-config`.
  **The defect.** A backlog file's embedded TOML is a pre-registration: it
  states before the run what will be tested and what will kill it, and the whole
  value of writing it first is that it cannot be adjusted after. That guarantee
  is void if the block was never runnable, because then the file that actually
  gets run is a *different* file, written later, by someone who has already seen
  the data. Two of the two registrations in the directory were in that state and
  had been since they were written:
  **H-007** declared `stages = ["s1", "s2"]` with no `[walk_forward]` section —
  S2 *is* a walk-forward under costs, so the funnel refuses the config outright.
  **H-008** declared `stages = ["s1", "s2"]` while both its Gate 0 and Gate 0b
  are S0 measurements, and carried a comment asserting `s0` "is REFUSED at
  load", which had been true and stopped being true when the predictor seam
  landed (D-0085). A registration describing a build that no longer existed.
  **The requirements are read, never listed.** The lint runs
  `crucible funnel --check-config`, which calls the funnel's own pre-flight —
  the identical function the real run calls, refusing for the same reasons in
  the same order. A lint carrying its own copy of "s2 needs `[walk_forward]`,
  s0 needs `[s0]`" would be a second source of truth, and the day a third
  requirement landed the copy is what would not learn about it. This is the
  D-0015 / D-0060 / `FoldPlan` device again: one computation, two consumers.
  **Byte-identity, not "both parse".** Two configs can each be valid and still
  describe different experiments, and the registered grid differing from the run
  grid is exactly what pre-registration exists to prevent. So the block and
  `configs/hypotheses/<id>.toml` are asserted equal byte for byte, and each
  names the other. They are one artifact that happens to appear twice; drift is
  not detected, it is made impossible.
  **`--check-config` charges nothing and writes nothing** — no archive read, no
  trial, no registry row, no scorecard, no output directory. It is a question
  about a file, and a question about a file must be free to ask, or the lint
  would be too expensive to run on every `cargo test` and would be run on none.
  The git sha is deliberately outside it: provenance is not configuration, and a
  registration is well-formed regardless of whether the process can see a
  checkout.
  **The lint reports EVERY offender, never the first.** Stopping at the first
  failure makes a backlog with two broken registrations look like a backlog with
  one — the same argument D-0090 makes about naming every contract in a refusal
  rather than returning on the earliest. It found 2 of 2.
  **A new refusal came with it:** an `[s0].score` naming an indicator slot that
  does not exist is now refused at config time rather than after the grid has
  been replayed. Same argument `Criteria::new` already makes about the cost
  sweep — a criterion that turns out to be unevaluable after an hour of replay
  has already cost the hour.
  **Controls, each watched firing before the fix landed:** the two real
  registrations, failing 2 of 2 with their own reasons quoted; `stages` naming
  `s0` with the `[s0]` block deleted; an `[s0]` block with `s0` absent from
  `stages`; `[s0].score` naming a slot that does not exist; and `[walk_forward]`
  removed from an otherwise-valid config. The converse is asserted too — the
  shipped configs pass — because a checker that only ever passes is not a
  checker.
  **The escape hatch is closed.** A block is judged a registration when it
  declares `schema_version`, which is the field the loader itself reads first.
  A block that does not is an illustrative fragment (H-001 and H-012 write bare
  rule lines with `<feature 1>` placeholders), and a third test asserts a
  fragment carries no `[meta]`, `[funnel]`, `[universe]` or `[run]` — so a real
  config cannot opt out of being checked by deleting one line.
- **D-0102** (2026-08-01) -- **S0 has one typed evidence
  product, and every consumer renders that product without silently replacing
  conditional evidence with unconditional drift.** The forward join retains
  every tradeable bar while making the score optional: an entry needs a score,
  but its horizon partner does not. Each retained pair owns its entry and exit
  prices, so a bucket's mean tick move is the mean of each observation's
  `(exit - entry) / contract_tick`; multiplying a fractional aggregate by a
  representative price is forbidden.
  **The result shape makes absence visible.** Horizons remain in configured
  order, and `BucketSet` cannot represent an empty, non-finite, zero-count,
  overlapping, or reversed available result. Every missing IC, interval, or
  bucket set carries a closed typed reason. A missing required interval remains
  UNEVALUATED rather than becoming a measured failure. Too few observations
  therefore render as UNAVAILABLE in stdout and HTML instead of an empty table
  or a fabricated zero. Duplicate availability timestamps are refused because
  neither same-time close can honestly be called the forward observation.
  **One owner, four consumers.** `FunnelReport` owns the `S0Report`; assessment,
  stdout, the scorecard, and the determinism encoding borrow that same typed
  value. The former pass-only side map is deleted, so a measured S0 criterion
  failure cannot be reconstructed as absent IC. The full score identity,
  declared S0 specification, pair/drop counts, IC, unconditional bootstrap
  interval, bucket bounds/counts/returns/ticks, criterion outcome, and absence
  reasons enter the canonical evidence bytes. The criterion is derived from
  the evidence in memory, serialized with it, and rejected on read if the two
  contradict. Before any trading run is claimed, the report must match the
  exact grid cardinality/order/index/label, S0 declaration, and contract tick.
  **H-008 Gate 0b remains UNEVALUATED.** The typed evidence scope says this is
  a global capability limitation, not an H-008 run result. Its population is closes
  beyond the Bollinger band. Equal-count z-score buckets are not proved to be
  that population. The bootstrap interval remains useful but is labelled
  UNCONDITIONAL everywhere and cannot stand in for a conditional bucket or for
  Gate 0b.
  **Controls were written before the output was trusted.** The hand-derived
  separable fixture has low-score observations followed by +2-tick moves,
  high-score observations followed by -2-tick moves, zero unconditional drift,
  and IC -1. Its unequal entry prices also distinguish the required 2.0-tick
  observation mean from the prohibited 2.25-tick aggregate conversion. The
  converse has common drift but no separation and cannot clear the criterion;
  a 3-observation/5-bucket case is explicitly unavailable; and a target bar
  without a score still supplies the exit price. Presentation controls use
  five distinct buckets at three deliberately non-sorted horizons and assert
  every one of the 15 rows in both stdout and HTML. The converse is rendered
  through both consumers and remains explicitly UNEVALUATED, never an edge.
  **Five deliberate mutations were watched failing, then restored byte-exactly:**
  omitting HTML bucket rendering was caught by
  `s0_scorecard_renders_declared_horizon_and_low_score_bucket_order`; replacing
  observation tick moves with aggregate conversion, reversing bucket order,
  and substituting the unconditional mean for bucket evidence were each caught
  by `separable_tails_are_visible_while_unconditional_drift_is_zero`; making
  constant-return data report a perfect IC was caught by
  `common_unconditional_drift_without_tail_separation_does_not_fire`.
  `scorecard.rs` returned to SHA-256
  `9668f80d41e2fe7e1837b0e7004f18c28b30ffc0a6d4b39d6846ead0ddb87b53`;
  `s0.rs` returned after every mutation to
  `4848902204bfdd8a5751bee5ee358d301fb3483cc561012f9800bed0608d651d`.
  **Open §9 identity blocker.** The existing `config_hash` binds the combo
  grammar, not the S0 horizons/buckets/draws/threshold, contract tick, or data
  window, and `run_s0` currently recomputes after an `AlreadyDone` claim. This
  entry does not invent a migration for historical run/trial identity. No S0
  writer may deploy until an explicit identity ruling makes distinct
  declarations incapable of aliasing one registry key.
  **This does not claim persistence.** Successful production S0 measurement
  still writes `metrics: null`. A reader-only persisted shape must deploy and
  merge before any writer emits it.
- **D-0103** (2026-08-01) -- **The S0 determinism pin
  moves from `91107aeb6e6802c0` to `825356c88295ce94`; the other seven pinned
  gates do not move.** This is a deliberate semantic repin, isolated from the
  evidence-product commit. The old D-0085 value remains a historical fact: it
  hashed the pass-only summary available then. The new gate consumes the one
  strict serialized typed product and therefore covers evidence scope, full
  score identity and declaration, declared horizon order, pair/drop counts,
  every IC and absence, every bucket bound/count/fraction/tick mean, the
  separately labelled unconditional interval, and the derived criterion.
  Omitting any of those fields from a consumer can no longer leave the S0 gate
  unchanged. The new value was observed twice from
  `crucible funnel --config configs/s0-smoke.toml --out <isolated> --hash-only`.
  Observed unchanged in the same checkout: demo `b55747513df596ed`, combo
  `0e1ab52d474b862b`, walk-forward `711e1cb34a2ee2b4`, funnel
  `2f430893d2a79a8f`, permutation `9fe41f6f5b3653e7`, truncation
  `91b9ff5b9bbcdb25`, and deflated-Sharpe `dc7f94f25235df6c`.
- **D-0104** (2026-08-01) -- **The registry
  reader learns the complete typed S0 aggregate before any production writer
  may emit it.** A measured predictor run cannot honestly inhabit trading-only
  `RunMetrics`: fabricated equity, trade, fee, and win-rate zeros would turn
  absence into measurements. The new `S0RunMetrics` instead owns the evidence
  population plus one validated `S0ComboReport`, which already contains score
  identity, contract tick, the full S0 declaration and horizon order,
  pair/drop counts, IC, unconditional interval, lowest-first bucket evidence
  including per-observation tick means, the derived criterion, and every typed
  absence reason.
  **The old wire does not move.** Existing non-null trading objects retain their
  byte shape. A successor object is explicitly tagged
  `{"metric_kind":"s0","value":...}` inside the existing `metrics` field.
  The real replay reader indexes both, while historical `metrics: null` becomes
  the named `LegacyAbsent` result rather than a zero, a pass, or a missing map
  entry. The compatibility union is closed: unknown metric kinds, extended S0
  aggregates, unknown trading fields, omitted nullable trading fields, and an
  omitted `metrics` member refuse instead of being partially accepted. Explicit
  JSON null remains distinct from omission at both levels.
  **A result cannot contradict its claim.** The reader retains the claimed
  combo index, fold, parameter label, fill model, S0-stage declaration, and
  pre-registered IC threshold. Typed S0 evidence must match all of them and must
  belong to the non-fold, no-position S0 row; a trading fold in an S0-enabled
  config cannot masquerade as predictor evidence. The converse is enforced too:
  a no-position S0 row refuses trading metrics filled with invented zeros,
  before append, while explicit legacy null remains readable. Horizons, bucket
  geometry, tick, and data window still are not present in the current run
  identity, so this check narrows but does not pretend to resolve the open
  identity blocker.
  **The deployment order is part of the record contract.** This commit exposes
  no S0 finish API and the existing S0 CLI control asserts production still
  writes `metrics: null` and no `metric_kind: s0`. After this reader commit has
  merged to `main`, a later writer commit must make `S0Report` own the same
  once-constructed `S0RunMetrics` values borrowed by persistence, stdout,
  scorecard, and determinism; constructing a second persistence envelope is
  forbidden. Its typed finish path must validate before appending, so a bad
  result cannot durably poison the log. That writer remains
  separately blocked by D-0102's run-identity defect;
  reader-first does not authorize an identity migration.
  **Compatibility controls use the actual JSONL reader.** One test pins the
  exact legacy trading finish lines (including explicit nullable fields) and
  reopens them; one reopens historical null as explicit legacy absence; one
  appends a reader-only tagged S0 fixture,
  reopens it, and compares every non-float field plus every floating field's
  IEEE-754 bits and canonical JSON; strict-shape controls distinguish omitted
  fields from null and reject legacy/typed hybrids; and claim controls reject
  combo, label, threshold, fold, and fill-model disagreement. A two-sided claim
  control rejects trading metrics on S0 while accepting an ordinary trading
  fold from an S0-enabled config. No production writer emits the new record in
  this commit.
- **D-0105** (2026-08-01) -- **A `TrialKey` binds to exactly one
  `hypothesis_family`, and a contradictory claim is refused before append.**
  The first claim for `(config_hash, account_id, combo_index)` owns the family
  for that trial permanently. Folds and seeds remain runs inside the one trial;
  they cannot move its multiple-testing charge to another denominator.
  **One canonical family replaces per-run family copies.**
  `Registry::charged` now maps a `TrialKey` directly to its canonical family.
  `run_trial` stores only the `TrialKey`, so voiding cannot consult a family
  copied from an individual fold. The same
  canonical value increments the trial count on the first claim and selects
  the family decremented when the last standing member is voided. Claim
  validation runs before the `AlreadyDone`/retry shortcuts and before JSONL
  append; replay validates again, so a contradictory historical line refuses
  rather than silently changing an in-memory denominator. The wire format does
  not change.
  **Controls.** `a_trial_key_refuses_a_different_family_before_append` plants a
  second fold under another family, observes `TrialFamilyMismatch`, proves the
  file is byte-identical, and repeats the refusal after reopening the real
  reader. `void_decrements_the_family_that_owns_the_trial_charge` proves two
  folds own one canonical family, the first void leaves its count at one, the
  second moves that same family to zero, and replay preserves the binding. The
  refusal control also exercises exact-key `AlreadyDone`, running, and failed
  shortcuts and appends a planted conflicting historical row that the real
  replay path refuses.
  The local registry was inventoried read-only before tightening the reader:
  3,644 run rows, 130 distinct TrialKeys, and zero cross-family keys.
  **Mutation verification.** Reversing the family comparison (`!=` to `==`)
  made the planted mismatch return `Inserted::New`; the named refusal control
  failed at its `expect_err`. Restoring the comparison returned
  `registry.rs` byte-exactly to SHA-256
  `b2baa39bd28055783d896dfbd2197498aa64e17efca1ed2fd676017e09c9fc9f`,
  and both controls passed again.
  **Concurrency boundary.** This enforcement inherits the registry's existing
  single-writer contract. Two independently opened writers are not serialized
  by this change; adding cross-process locking would be a separate storage
  decision, not an unreviewed extension of this claim-time ruling.
  **Adjacent holes are not silently decided here.** A new same-family member
  added after every prior member of its trial was voided currently does not
  restore the standing-trial count, and verdict rows repeat a family label but
  are not run claims. Their semantics require separate rulings; this entry
  changes neither while enforcing the reviewer-issued claim-time boundary.
- **D-0106** (2026-08-01) -- **An S0 registration owns a versioned composite
  run identity, writes the typed D-0104 aggregate, and resumes only from that
  registry-owned result.** The old D-0012 strategy hash remains visible, but
  a funnel that declares S0 now uses
  `blake3("crucible-s0-registration/1" || canonical fields)` as the existing
  `RunKey.config_hash` for both its S0 row and all downstream trading folds.
  The canonical, length-prefixed encoding binds the base strategy hash; score
  slot; horizons in declared order; bucket count; bootstrap draws;
  `min_abs_ic` bits; contract tick nanopoints; typed curated window or complete
  synthetic-generator declaration; the sorted/deduplicated manifest-id set;
  observed first/last availability timestamps and bar count; and every ordered
  OHLCV/signal-offset/session-key value. Thus the requested data window is in
  the identity, while an interior-bar or bootstrap-block change cannot collide
  merely because its endpoints agree. Both `run_s0` and `run_funnel`
  independently derive D-0012 from the grid's owned `ComboSpec` and then
  derive this composite before any claim, so a public caller cannot pair an
  arbitrary base/effective hash or a foreign spec with the grid. Synthetic
  bar counts must match the observed population and cannot carry archive
  manifests; a nonempty curated population must carry at least one manifest.
  The CLI remains the authority that parses curated date endpoints and applies
  them to archive reads; the library binds both the declaration and delivered
  population but does not independently rederive civil-date containment.
  The run seed remains a separate `RunKey` component under D-0064.
  **One registration, one trial.** S0 and S1/S2 receive the same effective
  hash, so folds do not create a second trial per combo. Configs without S0
  retain their D-0012 identity and their existing determinism pins. Scorecard
  provenance and filenames use the registration/run hash while retaining the
  strategy hash beside it; the stdout S0 product names the same identity.
  D-0064 seed derivation now consumes that effective identity: changing any
  bound S0/data input deliberately changes the S0 bootstrap stream and the
  downstream fold/control streams as well as the key. This avoids reusing a
  seed lineage across distinct registered populations, at the cost of not
  supplying common random numbers across those registrations.
  **Reader-first writer activation.** D-0104 is already on main. This commit
  adds the crate-private `Registry::finish_s0`, which can write only a
  successful typed `S0RunMetrics` in D-0104's already-readable tagged shape.
  It receives the single S0 run path's derived result contract and validates
  registration, combo, score identity, warmup, declaration, and tick before
  append; external callers cannot inject a parallel contract. The generic
  legacy writer now also
  refuses a successful S0 `metrics:null` before append, while ordinary trading
  claims retain their old nullable compatibility path. Historical null rows
  remain accepted only by replay. This does not alter the JSONL schema.
  `S0Report` owns the registration hash and the exact `Vec<S0RunMetrics>`
  values passed to persistence; assessment, stdout, HTML, and determinism
  borrow each value's `combo`, so there is no parallel renderer aggregate to
  drift. Before assessment, `run_funnel` requires each report value to be in
  `Done` status and byte-equal to the real reader's registry-owned typed value.
  Scorecard rendering also refuses when its provenance registration hash and
  the S0 report's registry-owned hash disagree.
  **Resume is a read, never a reconstruction.** `run_s0` branches on
  `Inserted::AlreadyDone` before constructing a scorer, updating a bar,
  joining a horizon, or drawing a bootstrap. It clones the typed result from
  `Registry::result_of`; missing, trading, or historical `metrics:null` on a
  Done S0 row is an explicit `CachedResult` refusal and appends nothing. Old
  null rows remain readable as `LegacyAbsent` and keep their historical trial
  charge. The composite v1 namespace means they are never reinterpreted as a
  new measured result.
  **Controls.** `every_declared_s0_and_data_window_input_changes_run_identity`
  independently changes the horizon set and order, buckets, draws, threshold
  bits, tick, curated start and end, synthetic declaration, observed
  population, session keys, score slot, and base strategy; each changes
  identity, while manifest reordering does not. A separate source-honesty
  control refuses contradictory synthetic bar counts, synthetic manifests,
  and missing curated manifests.
  `already_done_serves_typed_registry_result_without_remeasurement` observes
  one measurement start per fresh combo and zero on real-reader resume, proves
  byte-identical JSONL and report/determinism equality, compares every report
  aggregate to `PersistedRunResult::S0`, and proves a planted legacy-null Done
  row, injected through the historical replay seam, refuses with zero
  measurement starts and no append. The same control refuses an arbitrary
  caller-supplied identity before claim, request-incompatible cached score and
  warmup provenance before measurement, a non-Done typed result, and a
  report/reader numeric disagreement. The registry control proves a live
  writer cannot emit null or contract-incompatible typed evidence, and a
  planted S0/provenance hash disagreement proves the scorecard cannot render
  contradictory identities. A cached-threshold control changes the current
  criteria from `0.05` to `0.99`: both `run_s0` and `run_funnel` refuse it
  before measurement or append because criteria and declaration thresholds
  must be bit-identical. The CLI control reopens the real registry after
  the six-combo S0 null harness: all successful S0 rows are typed, no
  `metrics:null` exists, S0 plus all trading folds still charge exactly six
  trials, and the HTML filename uses the effective hash.
  **Mutation verification.** Omitting horizon values from the identity made
  the declared-order assertion fail; marking an `AlreadyDone` branch as a
  measurement made the zero-work assertion report one; reversing the derived
  identity refusal let an arbitrary hash measure and persist; changing the
  live-null guard from `Done` to `Failed` let the planted fresh null finish;
  reversing report/reader byte equality let divergent ticks pass; and
  reversing scorecard hash equality rendered contradictory identities.
  Accepting any current S0 threshold instead of requiring bit equality let a
  cached `0.05` result cross into `0.99` criteria; the zero-work resume control
  failed at its expected refusal. The named controls failed in every case.
  Inverse patches restored byte-exact SHA-256 values: `s0.rs`
  `6557994a9f4cfcfc7af655722ef689959d1f52275a7247539f3e2872eb5d3141`,
  `registry.rs`
  `df64ccadde7686dcae4c9a36bed5c748ed2be92b60b5b4e9224c9839e49c56ef`,
  `funnel.rs`
  `6f018da373e6fef74b0d753a0948c3c29b5ce3a8e60c11b5a57771d1ca3354b3`,
  and `scorecard.rs`
  `17f2083460ee4aa5b769b2249b80fac5ba872b0945b89f4ed8219fea59ef30bc`.
  **Pin discipline.** Binding identity and replacing the legacy-null writer
  intentionally changes S0's determinism bytes. The old
  `825356c88295ce94` remains the historical D-0103 pin; the new value is
  measured twice and recorded separately in D-0107, not smuggled into this
  implementation commit.
  **Adjacent holes are not decided here.** `FunnelReport::trials_before` is
  sampled after S0 claims, so its label is not an invocation-start count.
  Also, an AlreadyDone metric was measured under the original claim's git SHA
  while the current scorecard names the renderer checkout. Exposing original
  measurement provenance and correcting before/after semantics require
  separate rulings; this commit does not silently choose them. A voided Done
  S0 row also still satisfies the registry's status-only `AlreadyDone`
  shortcut even though its trial is outside the standing denominator. Whether
  void invalidates resume evidence is part of D-0105's reserved post-void
  semantics, not silently decided here.
- **D-0107** (2026-08-04) -- **The S0 determinism pin moves from
  `825356c88295ce94` to `e74766eb3f7becfc`; the other seven pinned gates do
  not move.** This is a deliberate semantic repin, isolated from the D-0106
  implementation commit that caused it, for the same reason D-0103 was
  isolated from D-0102: one hash, one commit, so the diff that moves a gate
  contains nothing a reader has to disentangle it from.
  **The evidence is the failure, not an argument.** At the implementation
  commit `88dfebd`, with the working tree byte-identical to the reviewed
  eleven-file diff, `crucible funnel --config configs/s0-smoke.toml --out
  <isolated> --hash-only` printed `e74766eb3f7becfc` against a documented
  `825356c88295ce94`, and the four other doc-pinned gates printed their
  documented values in the same checkout: demo `b55747513df596ed`, combo
  `0e1ab52d474b862b`, walk-forward `711e1cb34a2ee2b4`, funnel
  `2f430893d2a79a8f`. The three code-pinned gates asserted their own literals
  green in `cargo test --workspace`: permutation `9fe41f6f5b3653e7`,
  truncation `91b9ff5b9bbcdb25`, deflated Sharpe `dc7f94f25235df6c`. That
  `configs/combo-smoke.toml` declares `stages = ["s1", "s2"]` and no `[s0]`
  block is why the funnel gate is expected to hold and did: D-0106 leaves a
  config without an S0 registration on its D-0012 identity, and the unmoved
  funnel hash is the measurement of that claim rather than a restatement of
  it.
  **Why the bytes moved.** D-0106 binds the S0 declaration, the contract tick
  and the delivered data window into the run identity, and replaces the
  legacy-null writer with the typed D-0104 aggregate. The determinism hash
  consumes the S0 report, and the report now carries its registration hash and
  the exact persisted `S0RunMetrics` values rather than a separate renderer
  aggregate. Both changes are semantic, so a gate that did *not* move would
  have been the finding.
  **Derived twice before it was written down**, from separate invocations into
  separate fresh `--out` directories, both printing `e74766eb3f7becfc`. The
  requirement is not ceremony: a registration hash that folded in the
  delivered population would put nondeterminism directly into the identity
  every future trial count and argmax reads, and a single observation cannot
  tell a stable value from a lucky one.
  **A third side names the resume path specifically.** `--hash-only` builds a
  `Registry::ephemeral` and never opens the file (D-0083), so it can never
  exercise `AlreadyDone` — three `--hash-only` runs agreeing is three
  recomputes agreeing, which is a weaker statement than it looks. The resume
  claim was therefore measured without it: two ordinary `funnel` runs into one
  `--out` directory, where the second resumes S0 from the registry. The S0
  stdout product is byte-identical across them (18,070 bytes, `cmp` clean),
  both name registration
  `83cd9d9b0894e60a85c64c03934098455442b861a13c801641d9a253b8845bcb`, the
  registry holds six typed `metric_kind: "s0"` rows and zero `metrics: null`,
  and the trial count reads `6` after both — the re-run charges nothing, which
  is D-0106's "one registration, one trial" measured rather than asserted.
  Both runs exit 5, which on the null harness is the correct answer (D-0075).
- **D-0108** (2026-08-04) -- **A literal decision number already present in a
  diff is treated as `D-TBD`: re-verified free at commit time, and on collision
  the DIFF is renumbered, never the log.** §8.2 rules 1–4 describe a branch that
  obeys the placeholder protocol, and rule 4's grep is the enforcement. That
  grep can only fire on a branch that *complied*, because it searches for the
  `D-TBD(` placeholder spelling and nothing else. A
  branch that skipped the protocol and wrote a confident `D-0106` is invisible
  to it. The failure the protocol exists to prevent therefore had an
  unmonitored path straight to `main`, and rule 5 closes it.
  **This was found by walking into it.** The S0-registration diff (D-0106) sat
  unstaged for days carrying literal `D-0106` and `D-0107` — in code comments,
  in the appended decision entry's own heading, and in the text promising the
  repin would be recorded separately. No placeholder existed anywhere in the
  tree, so rule 4's grep exited clean, which is the correct answer to the
  question it asks and the wrong answer to the question that mattered. Both
  numbers were still free against a log topping out at `D-0105`, so
  verification cost one `git grep` and the commits stood.
  **Free was luck, and the rule is written against the luck rather than the
  outcome.** The branch had no way to know: §8.2's own motivating record is four
  collisions, three of them on one number, every one discovered at merge by
  someone who happened to read the log first. A diff that names its own number
  is making the same unverifiable bet those four made; that this one paid is not
  evidence the bet is sound, and a process that only records its collisions
  learns half of what it could.
  **Why the diff renumbers and not the log.** They are not symmetric. An entry
  on `main` has been published to every later reader, code comment, test name
  and commit message that cites it, and §8.1's third bullet is about exactly
  the stale references that follow from moving one. An unmerged diff has one
  reader and a mechanical, greppable rewrite. The asymmetry is why the rule can
  be stated as an absolute instead of a judgement call.
  **The check is a read, not a tool.** Re-read the log tail immediately before
  committing — not at the time the diff was written, which may be days earlier
  and is precisely the gap that lets two branches agree on a number. The report
  says the number was pre-claimed and re-verified, so a reader can tell a
  verified pre-claim from an allocation nobody checked.
- **D-0109** (2026-08-04) -- **The deflated Sharpe and PBO/CSCV are on the run
  path; the CSCV blocks are the FOLD PLAN's folds and nothing else.** Block B of
  `docs/plans/m3-full.md`. Two of the scorecard's named holes are now numbers,
  and two of S3's criteria are evaluated instead of declared and ignored.
  **The trial count comes from the registry or from nowhere.**
  `trials_from_registry` is the only door, so D-0083's exclusion of voided runs
  applies by construction rather than by anybody remembering it, and a local
  recount cannot exist to disagree with it. `Evidence` carries `n_trials`
  beside every deflated Sharpe because a headline ratio without its denominator
  is the single most common way a backtest lies.
  **A Sharpe cannot be deflated without the shape of the series it came from**,
  so `crucible-engine::Summary` now carries `ReturnShape` — sample size, skew
  and non-excess kurtosis — computed in the pass that already materializes the
  returns. Deriving those moments later from a *different* series, daily marks
  instead of per-bar say, would deflate one number using another number's
  shape. A flat window reports `None` rather than zero, on D-0080's grounds: a
  zero skew reads as "symmetric", which is a claim the data has not made.
  **The blocks are folds.** `FoldPlan` is this codebase's sole authority on
  which observations are out of sample (D-0062, D-0071), so `stats::pbo` takes
  an already-blocked matrix and never cuts a sample itself. A second splitter —
  bar counts, months, an even division of the row count — would be a second
  attribution of out-of-sampleness, which is the class of bug D-0071 exists to
  refuse. The per-cell metric is the fold's out-of-sample **return** rather than
  its Sharpe: a fold with no trades has a return and no Sharpe, so a Sharpe
  matrix would make PBO absent exactly on the quiet configs it is most useful
  for, and D-0063 already rebases every per-fold percentage to declared capital
  so combos and folds share one scale.
  **Three refusals, no fallbacks.** An odd block count is refused rather than
  trimmed to an even one — dropping a fold changes the sample the number
  describes while the number keeps claiming to describe the run. A split count
  above `MAX_SPLITS` is refused rather than subsampled, because a subsampled
  CSCV is a different estimator under the same name (§9's no-silent-caps rule).
  A non-finite cell is refused before any argmax, because a NaN would decide
  the winner silently. Each renders as ABSENT with its reason, and an absent
  estimate **fails** its criterion rather than passing quietly (D-0075).
  **Resolution is stated, not smoothed.** PBO can only take values `k /
  C(S, S/2)`; a four-fold plan gives sixths. The split count and the resolution
  are printed beside the value so "about a half" can be read against the grid it
  came from. Raising `S` until the number looks better is the fixture-fitting
  this log already forbids.
  **On the null harness PBO = 0.3333** — 2 of 6 splits, on 4 folds and 6
  combos. The acceptance criterion asks for *near 0.5* and warns that *near
  zero means the split is leaking*. 2/6 is one resolution step below the
  coin-flip value and decisively **not** near zero: under a true PBO of 0.5,
  `P(X <= 2)` on six splits is 22/64 ≈ 0.34, so the observation is an ordinary
  draw; under a leaking split it would be near-impossible. **Six splits cannot
  confirm 0.5 and this entry does not claim they do** — what they can do is
  exclude the leak the criterion was written to catch, and that is what is
  claimed. A longer fold plan would sharpen it and is not a reason to change
  this one.
  **`decided_at` moves from S2 to S3 for a config that passes everything**, and
  the ceiling does not move with it. Part of the battery now runs, so part of
  the battery is now the last stage reached; `Graduate` stays unreachable while
  the permutation null and truncation harness are unwired and the plateau and
  regime tables are still holes (D-0075). Changing that needs the superseding
  entry D-0075 demands, not a longer reasons list.
  **A control that survived its own mutation, recorded rather than patched.**
  `the_same_run_scored_at_one_and_twenty_four_trials_differs_materially` did
  **not** fail against an `expected_max_z` that ignored `n_trials` entirely,
  because `n_trials <= 1` short-circuits to a zero benchmark before the mutated
  line is reached — so the pair compared "no search" against "some search" and
  would pass for any implementation distinguishing only those two states. Four
  sibling controls did catch that mutation. A 2-vs-24 assertion now closes it,
  both operands above the short-circuit, and it was re-run against the same
  mutation and watched failing (`DSR@2 = DSR@24 = 0.379935`). This is the
  truncation harness's lesson repeating: the mutation that survives is the one
  worth writing down.
  **Not decided here.** No effective-N shrinkage of the account dimension: the
  raw product over-deflates, deliberately, because the preferred error is
  against the strategy, and shrinking it needs its own entry and a control
  showing it does not resurrect a known-spurious edge. The permutation null
  remains unwired to the run path and is still rendered as a named hole.
- **D-0110** (2026-08-04) -- **The block-B battery gets a determinism pin,
  `ef703dfd8d19fdd3`, and it is the ninth live gate.** One hash over both of
  block B's statistics, in its own commit, isolated from the D-0109
  implementation that produced it — the same discipline D-0103 and D-0107
  followed for the S0 pin, and for the same reason: a diff that moves a gate
  should contain nothing a reader has to disentangle it from.
  **What it covers**, which is exactly what `docs/plans/m3-full.md` asks a
  block-B pin to cover — config, trial count, block count and seed: the fixture
  seed, the block and combo counts, the split count, the underperforming-split
  count and the PBO value; then, over the same matrix at a trial count of 24,
  every deflated Sharpe's observed value, selection benchmark, standard error,
  DSR and echoed trial count. The two halves of block B are pinned **against
  one another** rather than separately, so a change that moved the PBO and
  compensated in the deflation could not pass.
  **Hashed rather than asserted field by field** so drift anywhere fails, not
  only where an assertion happens to look — the argument the permutation and
  truncation gates already make. The fixture is a deterministic SplitMix64
  matrix rather than a hand-written one because the rank logic is only
  exercised across many combos and folds, and the generator is inlined for the
  same reason the permutation gate inlines its own: a pinned fixture must not
  move when a dependency's internals do.
  **Derived twice in separate clean invocations before being written down**,
  the second after `cargo clean -p crucible-funnel`, both `ef703dfd8d19fdd3`.
  **The other eight gates do not move**, and were re-measured at the
  implementation commit and again here: demo `b55747513df596ed`, combo
  `0e1ab52d474b862b`, walk-forward `711e1cb34a2ee2b4`, funnel
  `2f430893d2a79a8f`, S0 `e74766eb3f7becfc`, permutation `9fe41f6f5b3653e7`,
  truncation `91b9ff5b9bbcdb25`, deflated Sharpe `dc7f94f25235df6c`. Both smoke
  configs kill every combo before S3 is reached — `combo-smoke` at S1,
  `s0-smoke` at S0 — so block B's new criteria cannot reach a verdict there and
  the funnel gates are unmoved for a stated reason rather than a lucky one.
  **The live-gate count is now nine**: five doc-pinned values across four
  hash-emitting CLI entry points (`funnel` is pinned against two configs) and
  four code-pinned digest assertions. D-0108's enumeration of eight stands as
  the count before this entry.
- **D-0111** (2026-08-04) -- **Paper intake is API-only: Semantic Scholar,
  OpenAlex, Crossref and arXiv, and Google Scholar and SSRN are never
  scraped.** `research/intake` is a standard-library Python tool outside the
  Cargo workspace, so §6's dependency policy is untouched and adding it costs
  the Rust build nothing.
  **The constraint is a terms-of-service one, not a technical one**, and it is
  the same class of rule as this project's "never scrape CME". Both Google
  Scholar and SSRN prohibit automated access. A research pipeline whose first
  step violates a terms of service is not one anybody can publish from, and the
  cost of obeying here is close to zero because the legitimate route is
  strictly better: Crossref *is* the DOI registry and OpenAlex indexes SSRN
  DOIs, so SSRN papers arrive **through** those APIs with structured metadata
  attached — the first draft the tool produced was `10.2139/ssrn.7008318`,
  reached exactly that way. PDFs are fetched by a human, by hand.
  **Enforcement is structural.** Every request passes through one function that
  refuses any host outside a frozen allowlist; there is no second HTTP call
  site and there are no dependencies, so auditing what the tool contacts is
  reading one constant and one function rather than trusting a policy. A
  convention would have been enough today and would not survive the first
  contributor who adds a `requests` call in a hurry.
  **The corpus is gitignored and the tool and drafts are committed.** Abstracts
  are third-party text and the corpus is rebuildable from the APIs, so
  committing it would put other people's copyrighted prose in this repository
  to no benefit. It is append-only JSONL on D-0074's reasoning. Dedupe is by
  DOI with a title+year fallback, because dropping the DOI-less records would
  bias the corpus toward published work and away from the preprints this field
  actually runs on; duplicates **merge** rather than first-wins, since Crossref
  carries the DOI and venue while Semantic Scholar carries the abstract.
  **A draft is not a registration, and the tool cannot promote one.** Drafts
  land in `research/intake/drafts/` and a human moves them into
  `research/backlog/` after reading the paper. The drafter allocates no `H-0NN`
  id and no triage grade: the id is claimed against the backlog at promotion
  time for the same reason §8.2 allocates decision numbers at merge, and the
  grade is a cost judgement nobody who has not read the paper can make. Every
  section the tool cannot fill carries an explicit `TODO(human)`, because a
  blank mechanism paragraph reads like a considered one.
  **The backlog's no-predicted-performance rule is enforced in the tool rather
  than at review**, which is the only way it holds when someone is busy: the
  drafter scans its own output and refuses to write a file that breaks it. The
  quoted abstract is excluded from that scan — it is labelled third-party text
  and routinely contains performance figures, so scanning it would make the
  rule unsatisfiable for precisely the papers worth reading.
  **The check fired on its own boilerplate first, and that is recorded rather
  than quietly patched.** The template's own warning sentence named the banned
  metrics, so every draft was refused by the rule's restatement of itself —
  the identical failure §8.2 documents for a `D-TBD` grep that matches its own
  documentation. The sentence is now worded without naming them. A check that
  fires on its own boilerplate gets disabled within a week, which is worse than
  not having one.
- **D-0112** (2026-08-04) -- **A registration draft references its corpus
  record and never embeds the paper's abstract.** Superseding the shape D-0111
  shipped, where the drafter reproduced the harvested abstract verbatim under a
  `<details>` block and the no-predicted-performance checker skipped that block.
  **Two rules this repository already holds meet here, and the original shape
  broke both.** `research/backlog/README.md` §1 puts a source paper's own
  performance figures in the **Honesty note and nowhere else**; an abstract
  routinely leads with one, so an embedded abstract placed them in the
  *Citation* section by construction. The first draft the tool ever produced
  demonstrated it — an SSRN paper whose abstract carries its own Sharpe,
  Sortino and p-value — and promoting that file unedited would have breached §1
  as written. Separately, the corpus is gitignored precisely so this repository
  carries no third-party prose, and two drafts had already committed exactly
  the text that rule exists to keep out.
  **The fix makes the divergence inexpressible rather than policed.** The
  front matter already carries the DOI, source API and access date, so the
  draft prints the one-liner that reads the abstract back out of the corpus and
  the human reads it there or in the paper. Nothing is lost: an abstract was
  never a substitute for the verbatim claim, which is the `TODO(human)` beside
  it.
  **The checker's exemption is gone with the thing it existed for.**
  `find_predictions` now scans the whole draft. The exemption was not a
  mistake given an embedded abstract — scanning quoted third-party text would
  have made the rule unsatisfiable for exactly the papers worth reading — but
  it meant the checker was blind to the figures most likely to break §1. Ruling
  5's phrasing is the right one: fix the structure so the divergence cannot be
  expressed, rather than adding a check for it.
  **The self-firing template happened twice.** D-0111 recorded the first: the
  warning sentence named the banned metrics, so every draft was refused by the
  rule's restatement of itself. Writing this entry's fix reintroduced it in the
  sentence explaining *why* abstracts are no longer embedded. Twice is a
  pattern, not an accident, and the remedy is a test rather than more care —
  `test_a_generated_draft_passes_its_own_check` asserts the drafter's own
  boilerplate survives its own checker.
  **The tests D-0111 claimed now exist.** `draft.py` said the check was covered
  "by the tool's own test" and no test existed; the repair is writing them, not
  deleting the sentence (§7, no quality exemption). Nine stdlib `unittest`
  controls under `research/intake/tests/`, each watched failing against a
  planted defect first: `find_predictions` returning `[]` (six subtests), the
  drafter re-embedding the abstract, and the redirect handler ceasing to
  re-check the host. **They are run by hand — CI is cargo-only and does not
  know this directory exists**, which the README says rather than implies.
  **The allowlist now holds on every redirect hop.** It guarded only the URL
  the tool typed, while `urllib` follows redirects itself, so a `301` off an
  allowed host would have been followed silently — with the README claiming
  structural enforcement. None of the four endpoints redirects off-host, so
  this was never exploitable; it is fixed because "structural" has to mean the
  structure holds and not that the happy path happens to.
- **D-0113** (2026-08-04) -- **A mutation control restores through git or
  proves its restoration, runs one mutation at a time, and has no fallback in
  its backup path.** §7 already required every control to be watched firing and
  the record to say which mutation each one caught. It said nothing about *how*
  to run one, and the protocol turns out to have a failure mode of its own.
  **The motivating incident, 2026-08-04.** A backup written as
  `cp f /tmp/f.bak 2>/dev/null || cp f f.bak` succeeded on its first branch —
  Git Bash has a `/tmp` — so the fallback never created `f.bak`, every
  subsequent `cp f.bak f` failed, and three mutations stacked on one file. The
  run then showed a dozen failing tests after each mutation and could not say
  which control had caught which defect: the second mutation's report already
  contained the first's failures, and the third's contained both. Every
  attribution had to be re-measured one mutation at a time.
  **What that would have cost if nobody had re-run it.** The attribution table
  is the deliverable — "a detector nobody has seen fire is decoration" is only
  discharged by naming which control caught which mutation. A stacked run
  produces a table that *looks* complete and is unfounded, and it would have
  gone into a decision entry as measured fact. The wasted pass was cheap; the
  entry would not have been.
  **The rule, and why each clause is there.** Restore with
  `git checkout -- <file>` where the file is committed, because git cannot
  half-succeed at it the way a shell copy can; where it is not, print
  `git hash-object` before and after and require the identical blob, which is
  the proof the S0 registration commit already used. One mutation at a time,
  because a stacked run's failures are unattributable and unattributable
  failures are what the control exists to prevent. And no `||` in a restore
  path — a fallback that silently does not run is the same class of defect as
  §8.1's scripted edit that silently matches nothing, and both were found the
  same way: by someone checking whether the thing had actually happened.
  **The rule is written where it binds.** §7 governs the controls, so it lives
  there rather than in §8's session workflow: the reader who needs it is the
  one about to break a detector on purpose.
- **D-0114** (2026-08-04) -- **Pooled sessions are the count of DISTINCT
  trading days across contracts, never the sum, and the sum is reported beside
  it so the overlap is visible.** The arithmetic half of block C
  (`crucible-funnel::pooling`). The orchestration half — the config schema, the
  multi-contract replay and its pin — is **not** in this entry and is recorded
  as owed below.
  **Why the union.** Two contracts of one root trade the same calendar days for
  as long as both are listed; ESH2024 and ESM2024 overlap for months. Adding
  their session counts claims a sample twice the size of the one that exists,
  and everything downstream reads that number — the admission floor, the
  deflated Sharpe's observation count, the fold plan's session budget. It is
  D-0062's argument against overlapping out-of-sample windows applied across
  contracts rather than within one: a session counted twice inflates `n` and
  flatters every statistic computed from it.
  **The sum is reported anyway, with the overlap named.** Not because it is
  useful — nothing may consume it as a sample size — but because a reader shown
  only "412 sessions" cannot tell whether the pooling was honest, while one
  shown "412 distinct, 631 summed, 219 overlapping" can. Same device as the
  suppressed-order count (D-0061) and the path-sensitive bar count (D-0069):
  the quantity a wrong implementation would have invented is printed rather
  than absorbed.
  **Cross-instrument breadth is not extra `n`.** Pooling ES with NQ over the
  same 250 sessions is one 250-session sample plus a claim that the effect
  appears in two instruments. The second claim is the rhyme check and is worth
  making; it is not sample size, and the arithmetic here refuses to let it be
  counted as any. This is the case where adding is most tempting, because the
  contracts look independent, so it carries its own control.
  **Pooling meets a floor and never lowers one.** H-007 and H-008 both register
  250 sessions and both say the floors come down "only when registry pooling
  supplies the sessions honestly, never to make a short run pass". One ES
  contract's active life is ~60 sessions, so a single-contract run still dies
  at admission — correctly — and five contracts of disjoint life supply 300 and
  the floor discriminates for the first time.
  `the_two_hundred_and_fifty_session_floor_is_met_by_pooling_not_lowered`
  asserts both halves, including the dishonest route: five contracts over *one*
  contract's sessions sum to 300 and are worth 60.
  **Pooling N contracts charges N trials**, so the larger sample costs
  something and block B's selection benchmark rises with the pool. The count is
  derived from the pool rather than declared, so a caller cannot name a trial
  count that disagrees with the evidence; `Registry::trials_for` stays
  authoritative and this is what it must equal once the claims land.
  **Four refusals, no partial counts.** No contracts; a contract with no
  evaluable day (it would charge a trial for no evidence); the same symbol
  twice — the double-count in its most direct form; and non-ascending day keys,
  because distinct-day counting must read the caller's ordered key slice
  (D-0071) rather than form a second opinion about ordering. A session total
  that quietly dropped a contract is the same lie as one that double-counted
  it.
  **D-0076 is not touched.** Pooling replays real contracts and pools their
  evidence; it is the sanctioned route to a long sample *precisely so that*
  stitching does not have to be. Nothing here enables `ES.v.0` for grids, and a
  future design that wants back-adjusted grids supersedes D-0076 explicitly.
  **What block C still owes**, stated so the next session does not have to
  infer it: the `[pooling]` config surface with `deny_unknown_fields` and its
  refusals; the CLI orchestration that loads N curated contracts and pools
  their replays; the pooled-run determinism pin (the tenth gate, its own commit
  and its own MILESTONES row); and the acceptance run — a config that died at
  admission on 60 single-contract sessions passing on pooled ones — which needs
  curated multi-contract data rather than a synthetic fixture. The arithmetic
  landed first deliberately: it is where the silently-wrong-result risk lives,
  and it is provable without the archive.
- **D-0115** (2026-08-04) -- **`[pooling].root` is what makes more than one
  instrument legal, and it is the only thing that does.** The declaration
  surface of block C, landed **inert**: nothing consumes it yet, exactly as
  D-0114's arithmetic landed before any caller. That ordering is §8's
  reader-first rule applied to a config — the parser and its refusals reach
  `main` before the orchestration that will depend on them, so a half-wired
  pooling path can never produce a wrong pooled number in between.
  **The contracts are `universe.instruments`, not a second list.** Two lists
  that must agree are two lists that will eventually disagree; the block
  carries the *policy* — which root they all belong to — and its presence is
  the permission. Configs without it keep the original single-instrument
  refusal, whose message now names the remedy instead of only the rule.
  **The root is declared, never inferred.** Inferring it from the symbols
  would let a typo'd contract silently define its own root, and a pool is a
  claim that these contracts are one instrument across time — a claim the
  config should have to make out loud.
  **Four refusals, each with a reason a reader can check.** Fewer than two
  contracts, because pooling one would print a pooled-run header over a
  single-contract result. The same symbol twice, which is the double-count in
  its most direct form — `crucible-funnel::pooling` refuses it again on the day
  keys (D-0114), and this is the earlier and cheaper of the two. A contract
  outside the declared root, because two roots is a cross-instrument claim and
  that is breadth rather than sample size. And **a continuous alias inside a
  pool**, which is D-0076 holding: pooling replays real contracts precisely so
  that a long sample does not require stitching, and admitting `ES.v.0` here
  would enable back-adjusted grids by the back door. That is a supersession and
  Liron's call, not a pooling declaration.
  **Controls, each watched failing in isolation under D-0113.** Removing each
  of the four refusals failed exactly one test and no others — alias →
  `a_continuous_alias_inside_a_pool_is_refused_naming_d0076`, duplicate →
  `the_same_contract_twice_in_a_pool_is_refused`, root →
  `a_contract_outside_the_declared_root_is_refused`, arity →
  `pooling_fewer_than_two_contracts_is_refused`. The converse,
  `a_well_formed_pool_of_one_root_is_accepted_by_the_parser`, comes first in
  the file for the usual reason: without it a parser that rejected `[pooling]`
  outright would satisfy every refusal above.
  **A protocol mistake, recorded because D-0113 is one day old and this is its
  first real exercise.** Restoring the first mutation with
  `git checkout -- config.rs` reverted the file to `HEAD` and discarded the
  entire uncommitted feature; the blob moved from `b4dace01` to `4da245fe` and
  the loss was visible only because the protocol requires printing it. D-0113's
  checkout branch is valid **only for a file whose working-tree state is
  committed** — for uncommitted work the blob-proof branch is the one that
  applies, and the two are not interchangeable. Nothing was lost beyond the
  redo: the tests were in a second file and survived, and the four edits were
  reapplied and re-proved. The remaining mutations used a single unconditional
  copy with the blob printed after each restore, all four returning
  `f786fca2fe0e8ad2924b35861624f03fd90c5109`.
  **What block C still owes**, unchanged from D-0114 except for this item:
  the CLI orchestration that loads N curated contracts and pools their
  replays; the pooled-run determinism pin and its MILESTONES row; and the
  acceptance run on real ES contracts. The archive holds 69 curated ES
  contracts at `1m`, so the acceptance run is data-ready and blocked only on
  the orchestration.
- **D-0116** (2026-08-04) -- **A mutation is restored against its PRE-MUTATION
  blob, by unconditional copy-back, and `git checkout -- <file>` is not a
  sanctioned mechanism.** Amending D-0113's §7 bullet one day after it landed,
  because that bullet listed the destructive mechanism first and unqualified,
  and a session following it exactly destroyed an uncommitted feature.
  **Recording the incident was not enough.** D-0115 recorded it, and
  `DECISIONS.md` is append-only so that was the right home for the record — but
  `CLAUDE.md` is the contract a future session trusts *without re-deriving*, it
  is meant to be edited, and until this commit it still instructed the next
  session to run `git checkout -- <file>`. That is §8.1's failure exactly: a
  contract file saying the opposite of what the record knows.
  **The reframing is what makes the trap inexpressible.** The reference is the
  **pre-mutation blob**, printed before the edit; a restore is proven by
  equality against *that*, never against `HEAD`. Say it that way and
  `git checkout --` is obviously wrong on uncommitted work without anybody
  having to remember a condition — it restores to `HEAD`, which equals the
  pre-mutation state only by coincidence. Say it the old way and the mechanism
  looks like a convenience. This is the same device as D-0090's
  `max(ts_recv)`-not-`max(expiration)`: two spellings that agree on the common
  case and differ silently on the one that matters, so the rule names the one
  that is always right rather than the one that usually is.
  **One mechanism, not two.** A mechanism safe only in a condition nobody
  checks is a trap rather than an option, so the amended bullet has a single
  procedure — unconditional copy, edit, copy back, print both blobs — and the
  `||`, one-mutation-at-a-time and isolated-attribution clauses are unchanged.
  **Both incidents are named in the rule**, as its siblings are: the
  `2>/dev/null ||` backup whose fallback never ran and stacked three mutations,
  and the `git checkout -- config.rs` that reverted an uncommitted feature,
  `b4dace01` → `4da245fe`. Both were caught **only** because the protocol
  prints what it did, which is the argument for printing.
  **Gate names now sit beside gate hashes** (`docs/MILESTONES.md`). A batched
  sweep that greps for a test name which does not exist prints nothing and
  looks identical to a green gate; on 2026-08-04 a guessed
  `the_truncation_hash_is_pinned` produced no output while the real name was
  `the_truncation_sweep_is_pinned`, and eight of nine gates were nearly
  reported as nine. The table's third column now names the exact command or
  `#[test]` per gate, and the instruction beside it is to run them individually
  and treat a gate that printed nothing as RED until proven otherwise. A
  verification whose silence is indistinguishable from success is not a
  verification.
- **D-0117** (2026-08-04) -- **A well-formed `[pooling]` declaration is REFUSED
  until the orchestration that honours it exists.** Closing a half-wired window
  D-0115 opened the same day.
  **What D-0115 got wrong.** It relaxed the single-instrument rule so a
  `[pooling]` config could name many contracts, and it landed the four *shape*
  refusals — but nothing refused a **well-formed** pool. `collect_events` reads
  `universe.instruments[0]`, so `crucible funnel` against a curated pooled
  config would have replayed **ESH2024 alone** and printed the result under a
  config that asked for a pool. Measured rather than reasoned: the first
  attempt refused only incidentally, on the synthetic feed's instrument
  mismatch, and swapping the data source to curated exposed it.
  **This is D-0075's shape exactly.** "A config declaring `stages = ["s3"]` is
  REFUSED, not run with the missing stage skipped — a config that asks for the
  permutation battery and silently receives a fold table has been answered with
  a different question than it asked, and the answer looks exactly like the one
  it wanted." A pooled declaration answered with a single-contract result is
  the same sentence with different nouns, and the single-contract number is the
  *more* dangerous of the two because it is a plausible verdict rather than an
  obvious hole.
  **The refusal ordering carries information.** The four shape refusals fire
  first, so a malformed pool still gets its own diagnosis — duplicate,
  continuous alias, foreign root, arity — and only a pool that is *correct in
  every respect* reaches the blanket "this build cannot run it". That ordering
  is also what makes the converse control meaningful: a well-formed pool
  reaching the orchestration refusal proves the shape rules are diagnosing
  shape rather than rejecting the block on sight.
  **The refusal is lifted in the same commit whose orchestration controls go
  green, never before.** That is the device that makes a one-pass orchestration
  safe, and it is the third use of the inert-first ordering in this block:
  D-0114 landed arithmetic nothing called, D-0115 landed a declaration nothing
  consumed, and this entry makes the gap between them un-runnable rather than
  merely unfinished.
  **The lesson, which is not "add a refusal".** Inert-first is only inert if
  the *permission* is withheld too. D-0115 shipped the permission (many
  instruments are now legal) without the refusal that makes it harmless, and
  the two halves were one commit apart. When a change relaxes a rule to make
  room for future work, the relaxation and the not-yet-implemented refusal
  belong in the same commit — otherwise the tree spends the interval accepting
  configs it cannot answer.
  Control: removing the refusal fails
  `a_well_formed_pool_reaches_the_not_yet_orchestrated_refusal` and nothing
  else; pre-mutation and post-restore blob
  `7a167120237695076dea47c6576af2566474beb8`, restored by unconditional copy
  under D-0116.
- **D-0118** (2026-08-04) -- **A negative search proves nothing until its
  pattern has matched something: run a positive control before concluding
  absence.** Generalizing D-0116's gate-table clause, which stated the rule for
  one case — a pinned-hash test that printed nothing — to every negative
  search: greps, `cargo test` filters, `git log` pathspecs, any `grep -c`
  returning 0.
  **The two causes are indistinguishable from the output.** A search returning
  nothing means either the thing is absent or the pattern is wrong, and the
  terminal shows the same thing either way. That is the defining property of an
  unfired detector, which §7 already refuses to accept as evidence when it is a
  test; a search is a detector a reviewer builds on the spot, and it inherits
  the same rule. The control is one command: point the pattern at something it
  should find and watch it match.
  **Three incidents, none caught by the search itself.** A scripted edit whose
  string-replace matched nothing left `CLAUDE.md` asserting the opposite of what
  its commit claimed (§8.1, 4297fbb). A guessed test name
  `the_truncation_hash_is_pinned` — the real one is
  `the_truncation_sweep_is_pinned` — printed no output during a gate sweep and
  eight of nine gates were nearly reported as nine (D-0116). And a review grep
  spelled `is_continuous|continuous_alias|refuse.*continuous`, snake_case
  against code spelling it `ContinuousAlias::looks_like`, returned nothing and
  was read as "the D-0076 refusal exists only as a doc comment" — while
  `crucible-cli/src/combo.rs:299` carried fourteen lines of reasoning and an
  enforced refusal naming D-0076 and §2.1, which the CLI prints on `combo`,
  `walk-forward` and `funnel` alike.
  **Every one was found by a human or a second pass re-checking the claim, not
  by the mechanism that produced it.** That is the argument for the rule: the
  failure is silent by construction, so it cannot be caught downstream, and the
  only cheap moment to catch it is before the conclusion is drawn.
  **Why this is a contract clause and not advice.** It governs the measurements
  the next several commits will make. Block C's claims are almost all negative
  searches — no second splitter exists, no caller reaches this path, no pin
  moved — and a wrong pattern in any of them produces a confident false
  statement in a decision entry, which is the failure mode this log exists to
  prevent. A rule that governs measurements has to exist before them, which is
  why it landed as its own commit ahead of C1 rather than inside it: C1's whole
  claim is "pure refactor, moves no hash", and that claim is checkable exactly
  because C1 contains nothing else.
  **Scope, fixed at ruling time**: one §7 clause and this entry. Sweeping the
  existing greps in the repository, building tooling, or adding a lint are
  separate items and go back to the queue.
- **D-0119** (2026-08-04) -- **A pooled run counts FRONT-MONTH sessions, cuts
  folds inside each contract's front window, unions sessions but sums trades,
  and reports what it skipped.** The ruling half of block C. It is a semantic
  choice affecting every pooled result and is reversible only by an explicit
  superseding entry.
  **The floor counts front-month sessions, not curated days.** ESH2024 carries
  239 curated trading days (2022-12-26 .. 2024-03-14) and is front month for
  64 of them. Four reasons, in binding order. *The direction test*: curated
  days inflate the sample and make admission easier, front-month days make the
  floor harder, and where a choice flatters the strategy this project takes the
  other one. *The union does not fix it*: D-0114 corrects double-counting
  between contracts on the same date and does nothing about counting a date on
  which only deferred contracts traded thinly — four ES contracts' curated
  union claims ~2 years where the tradeable reality is ~1. *Independence*,
  which is what a sample-adequacy floor is for: a deep-deferred contract's bars
  are largely an echo of the front month's price process, so they add `n`
  without adding information. *§2.4*: `spread_cross`'s half-spread is
  calibrated for the liquid contract, and replaying thin deferred bars under it
  assumes a book that was not there.
  **The mechanism is the existing `.v` volume roll table**, not a new
  heuristic. The volume rule already answers "when was this contract where the
  trading actually was", by measurement rather than calendar guess, and every
  pooled report names which rule produced the attribution.
  **This does NOT supersede D-0076**, and the distinction is the entry's job to
  state so nobody reads it as a back door. Using the roll table to decide which
  sessions belong to which contract is a **windowing** decision. Nothing is
  stitched and nothing is back-adjusted; each contract is still replayed on its
  own real tradeable prices. D-0076's concern is that a level on a
  back-adjusted series is not a level — no level is transformed here.
  **Warmup is unaffected.** A contract's pre-front bars are its own real bars
  and remain legitimate warmup input, exactly as D-0062 already treats warmup
  before an eval window. The window governs evaluation, session counting and
  trade attribution, not what an indicator may warm on. That is also what makes
  the geometry below affordable: the whole ~66-session front window is
  available for train and test.
  **Folds are cut INSIDE each contract's front window** — not across the pooled
  concatenation, and not at contract granularity. The decisive reason is that
  the alternative forces a roll-gap answer: cutting folds across the
  concatenation puts 63 rolls inside windows, and a position carried across one
  either books the raw gap as PnL — which §9 already refuses to let go silent —
  or is flattened at every boundary, which is a **new named execution
  assumption** that changes what the strategy does every ~3 months. Folds
  inside a front window never contain a roll, so the question does not arise
  and block C stays bounded. Contract-granularity folds were rejected against
  D-0062: front windows run 84–98 calendar days, which is exactly the "a month
  holds a number of sessions the holiday schedule picks" objection D-0062 was
  written against. Folds across the concatenation remain available later as a
  second pooling mode, with its own entry and its own execution assumption.
  **No geometry is blessed.** A pooled run takes the config's declared
  `train_days`/`test_days`/`step_days` and holds no opinion. `26 / 10 / 10` was
  computed this session as *feasible* — four folds in a 66-session window — and
  is deliberately not a default: it has no conventional source, it was chosen
  to fit the data's shape, and inheriting an unsourced geometry is exactly how
  H-007 and H-008 took combo-smoke's 5/2/2 and died at admission.
  **Sessions union, trades sum, and the asymmetry is real.** Two contracts can
  trade the same session, so sessions double-count and must be unioned; a
  round-trip belongs to exactly one contract's replay, so trades cannot
  double-count and summing is exact. **The floor is two-part** —
  `min_oos_sessions` *and* `min_oos_trades` — so a pool clearing the session
  half has cleared half a floor. A rule firing once a fortnight over 250
  sessions produces ~18 round-trips.
  **Measured per contract and summed, never extrapolated.** ES 2024 front
  windows measured 64, 70, 66 and 66 sessions over 84–98 calendar days. `N ×
  a constant` is wrong by construction, and that extrapolation is what produced
  this session's false claim that four contracts clear 250.
  **A contract fitting no complete fold is skipped with a count, printed even
  when the count is zero** — D-0070's spread-filter pattern, a declared filter
  with a number beside it rather than a silent absence. A pool quietly dropping
  two short contracts reports a smaller sample than it declared and nothing on
  the page would say so.
  **Gaps are reported, not refused.** A deliberately non-consecutive pool is a
  legitimate experiment — every March contract across years, for seasonality —
  and a hole nobody can see is not. Each gap prints its span and size, between
  the contracts that bound it, exactly as D-0114 prints the sum beside the
  union.
  **Path-dependent statistics must not be computed across the pooled
  concatenation.** A pooled sample is many short non-contiguous out-of-sample
  windows, not one long track record. Max drawdown, longest losing streak and
  time under water across the seams describe a path no account ever walked, and
  the seams join windows months apart. Each contract's replay starts and ends
  flat, so no spurious *return* appears at a seam — the trap is only in reading
  concatenated equity as one path. Path-independent aggregates pool correctly.
  **What was NOT added, and why.** A `sum == union` invariant was proposed
  twice — first as pooling arithmetic, then relabelled as a roll-table validity
  check — and is in neither. Front-month windows are half-open and consecutive,
  so the sum over disjoint sets equals the union unconditionally: the assertion
  cannot fail, which makes it decoration by §7. As a validity check it could
  only ever fire where `RollTable::validate` already fires, and would replace
  that check's precise diagnosis — out-of-order rolls, a chain that goes
  backwards in delivery — with a vaguer one. Fixtures point at the real check
  instead. Before adding a validity check, find the one that already refuses
  that input by name.
- **D-0120** (2026-08-04) -- **The rolling L1/L3 entitlement windows are allowed
  to LAPSE, so every cost number this project reports rests on an assumed
  one-tick half-spread — ES included — and for six of the seven roots it always
  will.** M1's last open box was a monthly cron for `trades`, `tbbo` and `mbo`,
  whose vendor entitlements roll (12 months for L1, one month for L3). It will
  not be built. The box stays **unticked**, because the work was not done; what
  changes is that it now says so and says why, instead of reading as pending.
  **What the archive holds, so the bound is a number rather than a worry.**
  Exactly one `tbbo` record and one `trades` record, both `ES.FUT`
  2025-07-28..2026-07-28, and one `mbo`, `ES.FUT` 2026-06-28..2026-07-28 — the
  full rolling windows as they stood on the blitz date. Those bytes are
  immutable and already paid for (D-0017), so **nothing decays**; what lapses is
  *extension*. Every month from 2026-07-28 forward is a month the archive will
  never have.
  **The consequence is not confined to the roots we did not buy.**
  `docs/DATA_PLAN.md`'s L1 rows are `ES.FUT` only, so NQ, RTY, CL, GC, ZN and 6E
  have no `tbbo` and no `trades` in this archive at all and can never acquire
  any. M4's "measured half-spread by time of day replaces the hand-set 1 tick"
  is therefore an **ES-only, single-window** claim even after it is done, and
  until it is done every config in `configs/` declares `half_spread_ticks = 1` —
  an assumption wearing the same field name a measurement would wear.
  **So it is stated where it is read, not only here.** §4 already requires a
  number derived by convention to carry its basis beside it rather than folded
  into the value, and an unmeasured half-spread is exactly such a number. A
  report quoting a non-ES cost sweep says so, on the same argument that makes
  `initial_basis` its own field in `configs/accounts/`.
  **Which direction the assumption errs is NOT known, and that is the point.**
  ES is usually one tick wide, which would make a one-tick *half*-spread
  pessimistic by roughly a factor of two — but that expectation is itself
  unmeasured, it is the precise quantity the lapsed data would have settled, and
  the six other roots are thinner in ways nobody here has quantified. Naming the
  assumption is what is owed. Asserting its sign from the armchair would be the
  convention hiding inside the number all over again, which is the failure §4's
  basis rule exists to prevent.
  **Why the trade is accepted.** The subscription month is spent
  (`docs/RUNBOOK_BLITZ.md`), keeping a rolling window alive is a recurring cost
  against a one-person portfolio project, and the sixteen-year `ohlcv` spine the
  funnel actually replays is complete, verified and immutable. What is given up
  is *calibration*, which is M4's; what is kept is every verdict M3 produces —
  each of which now carries an execution assumption that is named rather than
  measured, which is what §2.4 has demanded all along. The honest form of this
  decision is a stated assumption, not a quiet one.
- **D-0121** (2026-08-06) -- **A pooled contract is replayed over a LEAD-IN plus
  its front window, and a contract whose own curated data cannot supply the
  grid's warmup is SKIPPED with a count and a reason rather than evaluated on a
  shorter window.** Implements D-0119's warmup clause, which was asserted and
  never wired: that entry ruled that pre-front bars supply warmup so "the whole
  ~66-session front window is available for train and test", and the collector
  was handed the front window alone, so the front window was doing double duty
  as both the warmup source and the evaluation window.
  **The decisive reason is COUPLING, not the chain head.** Without a lead-in,
  warmup is taken out of the front window, so the number of evaluable sessions
  *falls as the grid's warmup rises*. "Does this pool clear 250 sessions?" then
  depends on the indicator periods: a config with a 5,000-bar Bollinger needs
  more contracts than one with a 20-bar SMA, for a reason nobody reading either
  config would predict. With the lead-in wired, the evaluable window **is** the
  front window and the pooled session count is a property of the pool alone —
  which is what makes an admission floor answerable before the grid is chosen.
  **Nothing measured so far is wrong, and the size of the old error is stated
  rather than waved at**: at a 200-bar warmup it is ~0.15 of a session per
  contract. It also retroactively repairs D-0119's geometry argument, whose
  fourth reason was explicitly that the entire front window is available for
  train and test — true only with the lead-in, so that table was approximately
  right for a reason that had not been built.
  **The lead-in comes from the contract's OWN curated partition, never the
  previous contract's bars.** Reaching back past `front_start` into the outgoing
  contract's series is stitching, which is D-0076 territory and needs a
  supersession rather than an implementation detail. Reading one instrument's
  partition makes that structural rather than arithmetic: there are no other
  contract's bars in it to reach.
  **When the lead-in suffices, evaluation begins exactly at `front_start`** —
  not at the warmup's end. The third option is the wrong one and is named here
  so it is not rediscovered as a convenience: letting evaluation start *late*
  when the lead-in is short reintroduces exactly the grid-dependence above, for
  precisely the contracts nearest the chain head, and does it without a count.
  Skip-with-a-reason is right because it refuses to half-answer.
  **`InsufficientWarmup { available, needed }` joins `NoCompleteFold`**, and
  like it the count prints even when zero (D-0070's declared-filter pattern). A
  pool quietly dropping a contract reports a smaller sample than it declared and
  nothing on the page would say so.
  **Measured before it was ruled**, on all 66 curated ES contracts: the chain
  head `ESM2010` has **0** pre-front bars — its curated data and its front
  window both begin 2010-06-06 — while every other contract has at least 5,043
  and typically 13,000–29,000. `ESM2010` is also short (6,885 front-window bars,
  ~5 sessions against ~64), so at any realistic geometry it is already skipped
  as `NoCompleteFold`. **That it is caught today is a coincidence of this
  archive, not a rule**, which is the same argument that decided the trailing
  fragment: an archive head landing earlier in a contract's life gives a chain
  head with ample sessions and no warmup at all.
  **The converse control is the one that carries the attribution**, for the
  third time in this block: a contract with ample lead-in must lose *nothing* —
  no fewer evaluable sessions than its front window holds — because a lead-in
  implementation that silently shortened every contract would pass the skip test
  while being wrong everywhere. Written first, as in C4b-i's D4 fix and the
  trailing-fragment drop.
  **The lead-in range and the evaluation range stay distinct in the type**, not
  one range plus a bar count. They answer different questions — what may be read
  versus what may be judged — and D-0119 already separates them in prose; a
  signature that conflated them would be the seam where the distinction gets
  lost, which is why `eval_range` became a named seam in the first place.
- **D-0122** (2026-08-06) -- **A pinned value that changes with the optimisation
  level is a §2.2 violation, and the fix is to remove the compiler's freedom —
  never to bless a profile.** `the_deflated_sharpe_determinism_hash_is_pinned`
  produced `dc7f94f25235df6c` under `dev` and `fec6ffe24b0447a8` under
  `--release`. Repinned to **`fec6ffe24b0447a8`**, which both profiles now agree
  on; D-0097's value is historical.
  **"Which profile do we pin" was the wrong question, and rejecting it is the
  decision.** Canonising either value institutionalises a number that still
  moves with the compiler — it makes the gate green while leaving §2.2 broken,
  and §2.2 says "on any machine" without carving out an `-O` level. What was
  fixed is the freedom, not the symptom.
  **The mechanism, bisected rather than guessed.** The failing test hashes
  `sharpe`, `skew` and `kurtosis` *before* `deflated_sharpe` is called, so
  hashing the moments alone decides between "it is in `moments()`" and "it is
  downstream". It is `moments()`. `f64::powi` lowers to `llvm.powi`, whose
  expansion into multiplications may associate differently at different
  optimisation levels, and float multiplication is not associative. Rust itself
  never reassociates — there is no fast-math — so writing the powers out makes
  the association a property of the source.
  **The third case that makes it an attribution.** `powi(2)` has exactly one
  association and cannot diverge; `powi(3)` and `powi(4)` have two or more and
  did. Measured: the input series hashed **identically** in both profiles
  (`a3a4a2b7ea207428`), `sharpe` was byte-identical, and only `skew` and
  `kurtosis` moved — `-8.79018770777955371631e-2` against
  `-8.79018770777955094076e-2`, and `2.88711809985583700566e0` against
  `2.88711809985583744975e0`. A divergence with an identical input and a
  same-profile-stable sibling is located, not merely observed. `variance` keeps
  `powi(2)` for that reason, stated in the code so the naive form is not
  restored as a tidy-up.
  **Acceptance was AGREEMENT, not the new number.** Derived twice in each
  profile — four identical readings — before anything was written down. Two
  derivations in one profile is exactly what certified `dc7f94f25235df6c`, and
  it certified less than it appeared to.
  **Only one pin moved, and the one that did not is informative.** The block-B
  battery consumes deflation and is unmoved and green in both profiles, so the
  divergence was **input-dependent**: a combo's own moments never reached
  magnitudes where the association mattered. That is why a gate built on the
  same arithmetic stayed green for as long as the broken one did, and it is the
  reason a single-input pin is not a proof of profile-stability for its callers.
  **CI now runs `--release`, and that is part of this decision rather than a
  follow-up.** The gate was invisible because `ci.yml` ran only `dev` — it had
  been measuring dev-profile determinism while claiming determinism, which is a
  gate narrower than its own claim. The workspace suite now runs in both
  profiles and the five hash-emitting CLI gates are compared **across** profiles
  by value with no-output detection, rather than each against a second run of
  itself. A fix without its regression guard is a fix that can silently return.
- **D-0123** (2026-08-06) -- **The next free decision number is the MAX of
  allocated numbers, not the tail of the log — and neither the ordering nor the
  query is trusted to a procedure any more, because §8.2 has now failed six
  times.** `crucible-cli/tests/decision_log.rs` asserts that every `- **D-NNNN**`
  heading is unique and strictly ascending, and that the max equals the last.
  **The incident.** D-0122 was appended in the MIDDLE of the log, between
  D-0120 and D-0121, and sat there through a push. The edit anchored on the
  string `decision is a stated assumption, not a quiet one.` — the tail of
  **D-0120** — and asserted `count == 1`, which passed because that string is
  unique in the file. **Unique is not last.** The assertion certified something
  narrower than the claim it appeared to make, which is the same defect shape
  this project has now found in a determinism gate that only ever ran one build
  profile, in a root check that only worked because no root was a prefix of
  another, and in a `<=` that a whole-run trade count satisfied.
  **Measured, both before and after the repair, because the obvious query is
  wrong twice over.** `grep -oE 'D-0[0-9]{3}' docs/DECISIONS.md | tail -1`
  returned **D-0119** against a real maximum of D-0122 — three behind, because
  of the misplacement. After the entries were reordered it returned **D-0097**,
  which is *worse*: D-0122's body cites D-0097, and the last **occurrence** of
  the pattern is not the last **heading**. So the tail query was never sound;
  the misplacement only made it visibly wrong. Only the max over headings is
  right whatever the file order and whatever the prose mentions.
  **§8.2 prescribed the broken query, and that is the point.** Rule 5 said
  "re-read the log tail and re-verify that number is still free". Four numbers
  have been claimed twice (D-0077 three ways, D-0085 twice), one branch
  pre-claimed D-0106/D-0107 and was free only by luck, and this is the sixth
  failure. Every previous repair was a better *procedure*, and procedures keep
  failing for one reason: they need someone to run the right query at the right
  moment. The rule now names the query, and a test enforces the property the
  query depends on.
  **The control fired on a REAL defect, not a planted one.** The test was
  written before the reorder and watched failing on the file as it stood —
  `D-0121 at line 4477 follows D-0122 at line 4431` — then watched passing
  after. That is the same route that made the profile-divergence fix stronger
  than a mutation would have: a detector whose first firing is on a genuine
  fault has been proven against the thing it exists for.
  **Reordering the log is not an append-only violation.** Append-only protects
  decisions from *revision*: an entry's text, once published, is not rewritten,
  and none was — the moved block is byte-identical and the diff is 46
  insertions against 46 deletions, a pure move. Nothing anywhere references an
  entry by line number; every reference is by number, which is exactly why the
  numbering has to be trustworthy. What was repaired is the file's ordering,
  which is a property of the record rather than a claim inside it.
- **D-0124** (2026-08-06) -- **A pooled contract's run is identified by a THIRD
  composite in the effective `config_hash` — strategy + contract + front window
  — never by a wider `RunKey`.** N pooled contracts then yield N distinct
  `TrialKey`s and charge N trials (D-0114), the same contract twice collides
  into one, and no persisted shape changes.
  **Why not widen the key, which is the obvious design.** `RunKey` is
  **persisted** and `TrialKey` is **derived at read time**. Adding a contract
  field is therefore a stored-shape change, and §8's reader-first rule then
  requires the reader on `main` before any writer emits the new shape — under a
  contract D-0083 (voided runs still counting correctly) and D-0105 both
  constrain. A derived composite changes nothing stored, so every existing
  reader keeps working and every historical row keeps meaning what it meant.
  This is the same device D-0106 already used for S0, and reusing it rather
  than inventing a second mechanism is most of the argument.
  **What is bound, and why the window is in it.** The front window is part of
  the identity because it is *what the run was evaluated on*. Rebuild the `.v`
  roll table against a longer archive and a contract's front window can move,
  which makes a different run of the same strategy on the same symbol — and two
  results that are not comparable must not share a trial. The boundaries are
  the window's own nanosecond instants, never civil dates: C4b-i exists because
  a date round-trip moved them to UTC midnight, and an identity built from the
  rounded form would call two different windows the same run.
  Fields are length-prefixed, so `("ab","c")` and `("a","bc")` cannot alias
  into one trial, and the domain tag is versioned so a future change to what is
  bound cannot silently collide with this scheme.
  **The rename threshold is named rather than left to drift.**
  `RunKey.config_hash` now enumerates all three composites (D-0012 strategy,
  D-0106 S0, this one), and **a fourth is the point at which the field is
  renamed.** Three meanings are still enumerable in a doc comment a reader will
  finish; four is a field whose name has stopped describing its contents, which
  is how a reader six months out builds a wrong model of what a row identifies.
  **N pooled contracts are NOT N independent searches, and the count is used
  anyway.** Pooling is one search over a larger sample: the same strategy, the
  same grid, more data. Charging N trials therefore **over-deflates** — the
  deflated Sharpe falls by more than the multiple-testing argument strictly
  justifies. That is accepted for the reason `docs/plans/m3-full.md` gives for
  Block E's sixteen accounts: the preferred error is against the strategy, and
  an over-deflated Sharpe makes a real edge harder to claim while an
  under-deflated one makes a spurious edge easier to publish. **It is stated
  where it renders** — C5 puts the sentence on the scorecard — so a reader sees
  a conservative correction rather than mistaking it for a precise one. Nobody
  may "fix" this by dividing the contract dimension out; that needs its own
  entry and a control showing it does not resurrect a known-spurious edge.
  **The identity is computed BEFORE the replay**, at the seam the pooled replay
  left for it, because D-0074's insert-before-run means the identity a run is
  recorded under exists before the run does. An identity derived after the loop
  produced numbers would be insert-after-run wearing the right name.
- **D-0125** (2026-08-06) -- **A deflated Sharpe is computed in PER-OBSERVATION
  units, and both the observed ratio and the trial dispersion are converted
  there.** The run path passed an ANNUALIZED `sharpe_naive` against a per-bar
  `n_returns`, which are different frequencies, and Bailey & López de Prado's
  estimator requires one.
  **The symptom was a headline that looked maximally confident and carried no
  information.** At 1-minute bars the annualization factor is
  `sqrt(525_949) ≈ 725`. The observed ratio inflates by that; the standard
  error cannot absorb it; `P(true Sharpe > 0)` saturates to **exactly 1.0000**
  or 0.0000 and nothing between. Measured: the mismatched form returns 1.0 at
  1-minute bars *and* at 1-hour bars, where the factor is still ~94.
  **The dispersion is converted by the same factor, and that half is not
  cosmetic.** It is the standard deviation of the grid's own annualized
  Sharpes, and it scales the expected-maximum z-score into Sharpe units.
  De-annualizing the observation while leaving the benchmark annualized would
  compare a converted number against a threshold ~725× too large and kill every
  combo — the same defect with its sign flipped. `DeflationInputs` already said
  this in its own doc ("already annualized or not — as long as
  `n_observations` matches it"); nothing enforced it.
  **Converted DOWN rather than up**, because `n_observations` is a count and
  cannot be rescaled without inventing a sample size.
  **The prescribed control was insufficient, and finding that out is the useful
  part.** A 1m-vs-1h invariance was specified: the same returns must deflate to
  the same DSR at either annualization. It passes on the BROKEN build too,
  because the broken build saturates at both frequencies — **saturation is
  invariant**. What discriminates is not stability but informativeness: a DSR
  strictly inside (0, 1) rather than pinned to an endpoint. Both assertions are
  kept, and the entry records why the second is the load-bearing one.
  **The first control also could not fail, for a different reason worth
  recording.** A unit test of `stats::deflated` with hand-built matching inputs
  proves the *formula* and passes on the broken build, because the defect was
  never in the estimator — it was in the wiring. Restoring the defect in
  `funnel.rs` left 177 tests green. The conversion is therefore extracted as
  `deflation_units`, a named seam where the units are decided, and the controls
  sit there. Same lesson as `eval_range` and `warmup_trim`: a check has to live
  where the decision is made, not where the arithmetic is.
  **No determinism gate moved, and that is a finding rather than a relief.**
  Both smoke configs kill every combo before S3, and the hashes cover verdicts,
  returns, naive Sharpes, the sweep, the controls and the day records — **not
  the deflated Sharpe**. So the nine gates could not have detected this defect
  in either direction, and a fix to the project's headline overfitting statistic
  landed without moving a single pinned value. That is the fixture-reach
  limitation D-0122 recorded, in the place it matters most: a gate certifies
  what its fixture reaches, and no combo in either fixture reaches S3.
- **D-0126** (2026-08-06) -- **`f64::powi` is banned from result-affecting code,
  and D-0122's clause exempting `powi(2)` is SUPERSEDED — it was an inference
  stated as a measurement.** Three sites are written out here; the six
  `powi(2)` sites follow in their own commit because they move values.
  **What was measured, and it is three separate facts.** Over 1,512,658 sampled
  inputs: `powi(3)` differs from `d*d*d` on **390,842 (25.8 %)** in `dev` and on
  **zero** in `release`. Release's `powi(4)` equals the **balanced**
  `(d*d)*(d*d)` on all 1,512,658 and the linear `((d*d)*d)*d` on only 989,165 —
  so the balanced form is what preserves every value release already computes,
  and it was measured rather than chosen for symmetry. And `powi(2)` differs
  from `x*x` on **725** samples in dev and zero in release, at ordinary normal
  finite values: `x = 1.00985740004162628e-6` gives `3d71f0d32627d8be` against
  `3d71f0d32627d8bd`, one ULP.
  **That third fact refutes D-0122's own reasoning, which is mine.** D-0122 said
  `variance` may keep `powi(2)` because "`z*z` has only one association, so it
  has no freedom to exercise — and that is not an assumption, it is the third
  case in the same measurement." The association argument is sound and
  irrelevant: squaring *does* have one association, and `powi(2)` diverges
  anyway, because LLVM's `-O0` expansion is not a plain multiply. The
  "measurement" it cited was that a Sharpe built on `powi(2)` agreed across
  profiles **on one fixture** — fixture-level evidence generalized into a claim
  about an operation.
  **The rule that follows, and it is the reusable part.** A claim about an
  OPERATION needs measurement over inputs; a claim about a FIXTURE needs only
  that fixture. D-0122 made the second kind of measurement and stated the first
  kind of conclusion, and its sentence asserting its own evidentiary status is
  what made the error uncheckable downstream. **An inference recorded in a
  decision entry becomes indistinguishable from a measurement to everyone who
  reads it, including its author** — this entry cites measurements over inputs,
  and says so, for that reason.
  **D-0122's fix was therefore incomplete on its own terms.** It repaired
  `moments()` and left `deflated.rs:199` and `deflated.rs:391` — both `powi(2)`,
  both inside the estimator it was written to fix. Nine result-affecting sites
  exist in total; three are written out here and six remain.
  **No gate moved**, in either profile, exactly as predicted: release already
  computed the written-out associations, so this makes universal what release
  was already producing. The six `powi(2)` sites will move gates, and that is
  expected evidence rather than a finding.
  **The powers stay written out and the comments say why.** The naive form looks
  tidier, and a reader who simplifies it back re-creates the defect — which is
  how the class survived D-0122 in the first place.

- **D-0127** (2026-08-07) -- **A pooled statistic across contracts is DEFINED as
  the pairwise combine of per-contract sufficient statistics, folded in declared
  contract order. It is not "the statistic of the concatenated equity curve",
  and the difference is not a rounding detail.**
  `ReturnStats::combine` implements Pébay's pairwise update for arbitrary-order
  moments (Sandia report SAND2008-6212, 2008), whose `M2` term is Chan, Golub &
  LeVeque's (1979) parallel variance formula. Written out rather than spelled
  with `powi`, per D-0126.
  **What is concatenated is the RETURN SERIES, never the equity curves.** Joining
  two contracts' curves end to end manufactures one extra return across the seam
  — the step from contract A's last equity to contract B's first — and that step
  is not a trade, not a mark, and not a number any strategy could have earned. It
  is the same seam `RunTrace::pooled_with_stats` already refuses to double-count
  between folds, one level up, and for the same reason.
  **The fold order is part of the definition, because the operation is neither
  bit-identical to a single pass nor associative.** Combining two halves and
  reducing the whole in one pass are different sequences of floating-point
  operations; measured on the test fixture they agree to a relative 1e-12 on the
  mean and `m2` and 1e-10 on `m3` and `m4`, which is what the control asserts.
  This does **not** weaken §2.2. Determinism requires that the same config and
  the same data give the same answer — not that the answer equal some other
  route's. What it does require is that the order be *declared*: a caller pooling
  N contracts folds them in contract order, never in completion order, which is
  the rule §2.2 already states for merging parallel results.
  **The total return is exempt, and deliberately so.** `net_delta_nano_usd` is
  summed in integer arithmetic, so a pooled total return carries no float error
  at all whatever the moments do. Only the *moments* are order-sensitive. That is
  why the field is an `i64` and not an `f64` (§2.3), and it is why the number a
  reader quotes most often is the one that cannot drift.
  **What must NOT be pooled this way, restating D-0119 because this is the
  machinery that makes it tempting**: max drawdown, losing streaks and time
  under water. Those are path statistics, and the pooled series is not a path any
  account walked — its seams join windows months apart. `sharpe_and_shape` exists
  precisely so a stitched series can yield a Sharpe and moments *without* a
  drawdown being computed and left sitting there looking available.
  **Nor is a pooled `n` evidence of more searching.** D-0114 is emphatic that
  agreement across contracts is not sample size; pooling gives a larger sample of
  the same hypothesis, not more independent trials, and the deflation entry at C5
  owes that sentence beside the number.
  **Where it lives, and why that is not a §3 violation.** `combine` sits in
  `crucible-engine` beside `ReturnStats` itself, following that type's own
  precedent: it is arithmetic on a reducer and knows nothing about runs,
  contracts or grids — it takes two summaries and returns one. The *decision to
  pool contracts*, which is the cross-run statistic §3 assigns to
  `crucible-funnel`, stays in the funnel. Engine gains no I/O, no threads and no
  clock.
  **The controls, and the converse was written into the suite rather than run
  once.** The two-route control combines two halves and compares against one pass
  over the whole; because equal means would make every correction term vanish and
  let a combine that merely added the moments pass, the fixture's means are
  asserted to differ first. `dropping_the_mean_correction_is_caught_by_those_
  tolerances` is the permanent converse: it shows the naive `a.m2 + b.m2` exceeds
  the same tolerance the agreement test applies. And the tolerance was watched
  discriminating rather than assumed to: mutating the `m4` correction's
  coefficient from 6 to 5 failed the control at a relative **7.0e-3** against a
  1e-10 bar, seven orders of magnitude over (D-0116 protocol; pre-mutation blob
  `216e9196`, restored and re-printed identical).
  **Nothing calls it yet.** D-0117 still refuses every pooled config, and the
  combine reaches no run until C6b lifts that refusal. No gate moved.
- **D-0128** (2026-08-07) -- **A combo is assessed ONCE, on evidence pooled
  across its contracts — and the pooled session count is handed to the pooling
  step rather than derived by it, because sufficient statistics cannot know that
  two contracts shared a Tuesday.**
  `contract_evidence` now produces a contract's *contribution*;
  `pool_contract_evidence` folds N of them into the one `Evidence`; `assess`
  runs once per combo. The `Evidence` construction moved out of the per-contract
  function, which is what discharges the `too_many_arguments` note C6a-iii-a
  deferred to this step: the trial count, the dispersion, the de-annualization
  and the PBO were only ever there to build a per-contract `Evidence` that
  nothing now assesses, so they moved to `PoolingInputs` and the per-contract
  function went from nine arguments to four.
  **Why not assess per contract and reconcile the verdicts.** Every criterion in
  `assess` is a threshold on a pooled quantity, so a rule for combining N
  verdicts — "most of them", "all of them", "the worst" — would be a second
  gate, unwritten and unregistered, sitting behind the pre-registered one. It
  also would not work: Block C exists because one contract's ~60 sessions cannot
  clear a registered 250-session floor, and N contracts each failing that floor
  separately is the same answer at N times the cost.
  **Trades are summed; sessions are not.** A round-trip belongs to exactly one
  contract, so two contracts' trade counts count disjoint events and adding them
  invents nothing. A trading day belongs to no contract — ESH2024 and ESM2024
  trade the same Tuesday — so adding session counts claims a sample twice the
  size of the one that exists (D-0114). The asymmetry is deliberate and is
  demonstrated rather than asserted: the same two contracts pooled under two
  different declared session counts report those two counts, while their trade
  count does not move.
  **Why the session count is a PARAMETER, and this is structural rather than
  disciplinary.** `ReturnStats` carries no day identity. Summing is therefore the
  only thing `pool_contract_evidence` *could* do if it derived the number — the
  honest answer is not available to it — and summing is precisely the defect
  D-0114 exists to prevent. So the union is computed where the day keys actually
  are, by `pooling::PooledSessions`, and passed in. `ContractEvidence` carries no
  session count at all, which makes the wrong answer unwritable here rather than
  merely discouraged.
  **An absent control on ANY contract makes the pooled control absent**, and the
  same rule governs a sweep level missing from any contract's sweep. Never "pool
  the ones that ran": that compares a strategy measured over N contracts against
  a benchmark measured over N−1, which is a smaller denominator reported as a
  result, when D-0075 is already explicit that an absent control **fails** its
  criterion rather than passing it.
  **Path statistics cannot be pooled from here at all.** What a fold yields is a
  `PooledSeries`, which has no drawdown to offer (D-0119) — `sharpe_and_shape`'s
  argument one level up: a computed-then-discarded drawdown is one refactor away
  from being surfaced, and whoever does that refactor sees a field sitting there
  looking available.
  **The pooled total return is formed final-over-initial, not
  delta-over-initial.** The same quantity arithmetically; not the same float
  operations, and the first spelling is `Summary::compute`'s — so a pool of one
  is bit-identical to the number that combo already reported, which is what keeps
  this step inert while D-0117 holds every pool to one contract.
  **What the determinism gates can and cannot see here, measured in both
  directions, because "no gate moved" would otherwise be read as more than it
  is.** `verdict_hash` hashes the *per-contract* artifacts directly — the
  walk-forward summary, the free-fill summary, each sweep level, each control's
  return — and reaches the pooled `Evidence` only through the two-field
  projection `(verdict, decided_at)`. So: adding 100 points to
  `free_fill_return_pct` moved the funnel gate `2f430893d2a79a8f` →
  `770bcd88157f18c3`, which proves the gate *reaches* this function; and swapping
  the costed channel to read the free-fill statistics — a defect four unit tests
  caught — left the gate at its pinned value exactly, because every combo in
  `configs/combo-smoke.toml` dies at S1 on the free-fill number and the
  projection is saturated before the costed quantities are consulted. **The unit
  tests are the evidence for this function; the gates' silence is evidence about
  `combo-smoke.toml` and nothing more.**
  **Controls, each planted alone and restored by unconditional copy-back under
  D-0116** (pre-mutation blob `2766ee49`, re-printed identical after all four):
  deriving sessions from the contracts, dropping the other contracts' trades,
  pooling only the controls that ran, and reading the wrong series for the costed
  channel — caught by 2, 1, 1 and 4 tests respectively.
  **One of those four is a near miss worth recording.** The session mutation was
  `contracts().count() * 48`, which for the fixture's two contracts evaluates to
  96 — *exactly* the session count the first assertion expected. The test caught
  it only on its second call, which pools the same two contracts under a
  different declared count of 141. A single-call version of that test would have
  passed the planted defect while appearing to check the rule. That is the
  by-construction trap in its fixture form: what made the check real was varying
  the parameter, not asserting the value.
  **Nothing reaches a pool of more than one.** D-0117 still refuses every
  well-formed `[pooling]` config, so `rest` is empty at the only call site;
  C6b lifts the refusal. The single-contract path goes through the pooled one
  anyway, so C6b is a wiring change rather than a rewrite of the number five
  gates pin.
  **Correction, recorded rather than rewritten.** The commit messages for
  `5e412bd` and `63fdb76` both say "all ten gates". There are **nine** — five CLI
  hash gates and four code-pinned digests, which is what `docs/MILESTONES.md`'s
  table lists and what was actually run in both profiles. The tenth is the
  pooled-run pin and it is **C7's**; it does not exist yet. Those commits are
  pushed, so the miscount stays in their messages, and this line is what a
  session reading them should find. Left as a correction rather than a rebase
  for the reason D-0075 gives about absent numbers generally: a reader who meets
  "ten gates" before C7 lands would conclude the pooled pin exists, which is a
  more flattering claim than the true one.
- **D-0129** (2026-08-07) -- **A pooled scorecard prints `max DD` as a NAMED
  HOLE — "not pooled: path statistic (D-0119)" — never a blank, never a
  per-contract number, and never a dropped column.**
  The cost-sensitivity table is mandatory (§2.4) and has five columns; a pooled
  run can honestly fill four of them. Returns and Sharpes come from the folded
  sufficient statistics (D-0127) and fees are summed — a fee is charged per fill
  and a fill belongs to exactly one contract, so summing double-counts nothing,
  which is the trades-sum-sessions-union asymmetry (D-0114, D-0119) applied to a
  third quantity. The drawdown is the one that cannot: a curve whose seams join
  contracts months apart describes a path no account walked.
  **Each of the three alternatives fails differently, which is why the hole
  wins.** A blank cell reads as *not applicable*. A per-contract number wearing
  a pooled label is the silent-corruption case D-0119 exists to prevent — the
  most dangerous, because it is a plausible number rather than an obvious gap.
  Dropping the column hides that the question was asked at all. The precedent is
  this module's own: the plateau heatmap, the regime table and the permutation
  null already render as named holes so a reader can tell "there wasn't one"
  from "it passed".
  **§9's honesty-box rule does not bite here, and the two rules look alike
  enough to be worth separating.** The honesty box — config hash, git sha,
  manifest ids, seed — ABORTS the render when a field is empty, and it is the
  one place in this codebase where an omission does. `max DD` is not in it; it
  is a cell in a results table, governed by the weaker but different rule that a
  hole must be visible as a hole.
  **The guarantee is in the TYPE, not in the renderer's discipline.**
  `PooledCostLevel` has no `max_drawdown_pct` field, so a renderer holding one
  cannot print a pooled drawdown by accident — there is nothing to print. Same
  device as `PooledSeries` one layer down and `sharpe_and_shape` one below that
  (D-0119's "a computed-then-discarded drawdown is one refactor away from being
  surfaced").
  **Controls, and they discriminate rather than firing together.** Four tests:
  the hole appears once per row and the poolable numbers are printed; the row
  keeps five cells, counted structurally rather than by header text, because a
  header survives a column whose cells are gone; the cell is not blank; and the
  cell carries no digits, with the digit scan asserted to match in the return
  and fee cells of the same rows so its silence in the hole cells is evidence
  rather than a broken pattern. Two mutations under D-0116 (pre-mutation blob
  `d45dbe3e`, restored and re-printed identical after each): rendering a blank
  failed three of the four, and rendering a plausible `12.34%` failed two — and
  **not the same two**, which is the property being claimed. A blank and a
  number are different defects and the suite tells them apart.
  **What is still owed, named rather than left to be discovered.** The scorecard
  also prints a "whole replay" column (`win_rate`, `max_drawdown_pct` over the
  unsliced run) and a worst-day table. A pooled run has no single whole replay,
  so that column needs the same treatment on the same precedent; the worst-day
  records are per-day rather than per-path and pool as a set (D-0071), so they
  do not. Wiring both is C6b-iii's, and this entry is where the obligation is
  written down so the wiring cannot quietly skip it.
  **Nothing renders this yet.** D-0117 still refuses every pooled config; the
  renderer is reachable only from its tests, like every other pooled piece
  before it.
