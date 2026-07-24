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
