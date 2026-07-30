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
