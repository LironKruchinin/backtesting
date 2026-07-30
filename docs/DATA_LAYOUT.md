# Data layout — the archive tree, as a contract

Everything under `$CRUCIBLE_DATA_DIR`, which is never inside the repo.

This was a convention: several modules independently agreed on a shape, and it
held because they happened to keep agreeing. It is now checked —
`crucible layout-check` walks the tree and refuses on any departure, and
`crucible-data::layout` is the enforcement of what is written here. If this
file and that module disagree, they are both wrong until someone fixes both.

---

## The two invariants

**1. No directory ever holds two instruments' data.**
**2. No directory ever holds two kinds of data.**

Everything below is those two rules applied to a particular tree. Together they
buy three things that are easy to undervalue until they are gone:

- `rm -rf curated/bars/ESH2024` is complete and obviously correct. You do not have
  to reason about what else was in there.
- A glob is safe. `curated/bars/*/1m/*.parquet` cannot sweep up a neighbouring
  instrument, a different timeframe, or a different kind of data.
- A wrong path is *visible*. If instruments and kinds were interleaved, a file
  in the wrong place would look exactly like a file in the right place.

**One honest qualification to invariant 1.** Under `raw/`, the third level is
the **symbol key we bought under**, not an instrument: a parent-key pull of
`ES.FUT` delivers one file containing every ES outright and calendar spread in
the window (D-0033). So `raw/…/ES.FUT/` holds many instruments' bars inside its
files, while holding only `ES.FUT`'s *purchases* as directory entries.
`transcode` is exactly the boundary where the parent key is resolved into
instruments, which is why invariant 1 becomes literal the moment data crosses
into `curated/`.

---

## The canonical tree

```text
$CRUCIBLE_DATA_DIR/
├── manifest.jsonl                 append-only record of every acquisition
├── jobs.jsonl                     append-only ingest journal (intents → outcomes)
├── pull.lock                      one executing pull per archive
├── raw/                           immutable, vendor-truth order
│   └── {dataset}/{schema}/{symbol}/{window}.dbn.zst
├── curated/                       disposable, research order
│   └── {kind}/{instrument}/{grain}/{file}
├── staging/                       downloads in flight; emptied on success
│   └── {intent_id}/…
├── delivery/                      per-job support files kept after collection
│   └── {job_id}/condition.json, metadata.json, symbology.json
└── external/                      data that did not come through `pull`
    └── {vendor}/…                 vendor-specific shape + its own inventory
```

Files sit **four** components below `raw/` and `curated/`; directories never
more than three. That is the rule `layout-check` enforces, and it is why
neither tree can grow an extra grouping level without someone deciding to.

---

## `raw/` — `{dataset}/{schema}/{symbol}/{window}`

**Immutable. Append-only. Never moved, never renamed, never edited** (D-0017).

The path is the purchase order. A Databento batch job is exactly
(dataset, schema, symbol key, window), so the path records what was bought,
from whom, and over what range — without needing to open the file or consult an
index. Two consequences follow, and both are load-bearing:

- **Coverage subtraction never opens a file.** `Catalog::coverage` answers
  "what do we already own for `ES.FUT`?" purely in memory, by filtering
  manifest records on dataset, schema and symbol and subtracting their ranges.
  It does not read `file_path` and does not touch the filesystem. This is the
  query whose wrong answer costs money (D-0020), and the path's job is not to
  answer it — the path's job is to be the thing a *human* can read to see what
  a file is without opening it, and the thing that makes two purchases of the
  same window collide instead of coexisting.
- **A manifest `file_path` is part of a result's provenance.** A result cites a
  manifest id, the id names the bytes, and the record names where those bytes
  live (D-0013, D-0014). Rename an archived file and you break the chain for
  every result that ever read it — retroactively, silently, and with no way to
  tell which results were affected.

That last point is why there is no `--fix` anywhere near this tree, and why a
misplaced raw file is corrected the way D-0017 corrects a corrupt one: by
acquiring a new file at a correct path, and leaving the old one alone.

Equities need no new pattern — the dataset level already carries the venue:

```text
raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst
raw/XNAS.ITCH/ohlcv-1m/SPY/2024-01.dbn.zst        # when the SPY/QQQ micro-pull happens
```

`{schema}` is a **closed set** (`crucible-data::layout::KNOWN_SCHEMAS`).
`ohclv-1m` is a transposition that looks entirely plausible in a directory
listing, would hold real data nobody ever reads, and would be discovered when a
coverage query reports a gap we already paid to fill. Adding a schema is a
one-line change and a decision-log entry.

---

## `curated/` — `{kind}/{instrument}/{grain}/{file}`

**Derived, disposable, freely deleted and rebuilt from `raw/`.**

The path is the research question. A backtest asks for "the March 2024 E-mini,
1-minute bars"; it never asks for "GLBX.MDP3's `ohlcv-1m` for ESH2024". The
vendor and the vendor schema are *provenance*, and they live inside the file —
Parquet key-value metadata carries the dataset, the vendor schema, and the
`file_blake3` of the raw file it came from (D-0036).

- `{kind}` — a closed set (`KNOWN_CURATED_KINDS`). Today `bars` and `rolls`.
- `{instrument}` — one instrument, percent-encoded (`SYN:RW` → `SYN%3ARW`),
  because `:` is a legal instrument character and an illegal Windows filename,
  and mapping it to `_` would file `SYN:RW` and `SYN_RW` together (D-0036).
  A **contract** is spelled with a four-digit year — see below.
- `{grain}` — the timeframe for aggregated kinds (`1m`, `1s`); a date partition
  for tick kinds, which have no timeframe.

Shapes pinned **now**, so the later work lands pre-separated instead of being
retrofitted into `bars/`:

```text
curated/bars/ESH2024/1m/2024-01.parquet         today
curated/rolls/ES/1m/v-confirm1.json             today  (root, not contract — a
                                                        roll is about two at once)
curated/trades/ESH2024/2024-01/…                M4: tick trades, date-partitioned
curated/book/ESH2024/2024-01/…                  M4: MBO-derived book snapshots
```

### A contract key carries a **four-digit** year (D-0072)

`curated/bars/ESH2024/`, never `curated/bars/ESH4/`. This is not tidiness; it is
the fix for a silent-corruption bug that had already reached the archive.

A CME year code is **one digit**, so it repeats every ten years. Every bar
window bought for this archive is **sixteen years** long (2010-06-06 →
2026-07-28). By pigeonhole, contracts alias — and they did. `GC.FUT ohlcv-1m`
wrote exactly **120 partitions = 12 month codes × 10 year digits**, and
`curated/bars/GCZ4/1m/…` held December-2014 gold and December-2024 gold
concatenated into one file spanning **14.5 years**. Rebuilt with resolved keys,
the same 10,275,830 bars land in **221** partitions, and `GCZ2014` (145,850
bars, 2010-06-08 → 2014-12-29) and `GCZ2024` (164,384 bars, 2019-06-21 →
2024-12-27) sum to exactly the 310,234 the merged file held.

**Nothing already here could have found it.** The strictly-increasing `ts_open`
check passed, because the two contracts trade in disjoint sequential periods and
concatenate in perfect order. The gap-inside-sessions check reported nothing,
because no bundled calendar claims gold, so `qa` had no definition of "expected"
and never looked — and even given one, a ten-year absence is whole missing
sessions, which is a coverage number, not a gap *inside* a session. Both facts
are asserted as tests, on the merged fixture, in `crucible-data::transcode`.

The year is resolved per record against **the contract's own expiry**, read from
the archived `definition` file for its root — never against a `DecadeAnchor`
constant, which has an answer for `GCZ4` and is right for half the bars. A bar
whose contract cannot be resolved **refuses the whole file**: unlike D-0070's
spread exclusion, which is a declared filter because a spread is merely a record
nothing replays yet, this is corruption of *meaning* that looks exactly like
correct data.

Two- and four-digit years are absolute and need no lookup — the vendor itself
switches to two digits for far-dated listings (`CLZ36`), and 16 such contracts
trade in the CL window. A key that is not a contract at all (`SYN%3ARW`, a
spread) keeps the vendor's spelling, because it has no delivery year to resolve;
`curated/rolls/` is keyed by **root** and carries no year either.

---

## Why `raw/` and `curated/` nest in **opposite** orders

This looks like an inconsistency and is not. Read it before "helpfully"
unifying them, because unifying costs one of the two properties below and there
is no version that keeps both.

`raw/` groups **provenance → kind → instrument**.
`curated/` groups **kind → instrument → grain**.

The instrument sits at the bottom of one and the middle of the other, because
the two trees are indexed by different questions:

| | `raw/` | `curated/` |
|---|---|---|
| answers | *what did we buy?* | *what do we want to replay?* |
| natural key | the purchase tuple | (instrument, grain) |
| mutability | immutable forever | rebuilt at will |
| vendor | in the **path** | in the **file metadata** |

**If `curated/` adopted `raw/`'s order** — `curated/GLBX.MDP3/ohlcv-1m/ESH2024/1m/…`
— then the vendor and vendor schema would be baked into every research path.
Two things immediately break. Derived data does not have one source: a
continuous series spans many contracts and many raw files, and 5-minute bars
aggregated from 1-minute bars have a source that is itself derived. And the day
the same bars are re-bought from a different vendor, or re-derived at a
different schema, the tree either duplicates or lies.

**If `raw/` adopted `curated/`'s order** — `raw/bars/ESH2024/…` — the purchase
tuple is destroyed. You can no longer tell what you bought from whom; coverage
subtraction needs an index instead of a listing; the parent key `ES.FUT` has
nowhere to live, because it is not an instrument. And because raw is immutable,
the first reorganisation would also be the last.

**What joins the two trees is content, not shape.** `manifest.jsonl` records
raw paths and their blake3; every curated file carries the blake3 of the raw
file it came from. The provenance chain runs through *identity of bytes*, so
the two trees are free to be shaped by their own access patterns — and that
freedom is the point, not an oversight.

---

## `staging/` — `{intent_id}/…`

Downloads in flight. A payload is verified here (length and vendor SHA-256)
before it is renamed into `raw/`, so a truncated or corrupt download never
reaches the archive and never gets certified by the manifest.

Emptied on success. **Left in place on failure**, deliberately: if a support
file could not be filed, deleting the directory would destroy the very thing
the warning is about (D-0047).

Not depth-checked. It is ingest's working area, and imposing a shape on it
would be inventing a rule for the pleasure of enforcing it.

## `delivery/` — `{job_id}/…`

Support files the vendor ships beside a payload: `condition.json`,
`metadata.json`, `symbology.json`. Kept because `condition.json` is what the
data-QA report reads to tell a vendor outage from a hole in our own pipeline.

Keyed by **job**, not by contract — a job covers a parent key over a window, so
attributing its dates to one instrument would be a guess. `crucible qa` reads
these archive-wide and says so.

Not an acquisition, so not in the manifest.

## `external/` — `{vendor}/…`

Data that did not come through `pull`: manually downloaded, differently
licensed, differently trusted.

```text
external/cboe/VIX_History.csv, VIX9D_History.csv, …, README.md
external/thetadata/options/{underlying}/{granularity}/…
```

Three rules, and the third is the one that matters:

1. **Vendor-specific shape.** Each vendor gets a directory and brings its own
   layout; there is no cross-vendor scheme to conform to, and pretending
   otherwise would mean reshaping someone else's export on the way in.
2. **Never in `manifest.jsonl`.** The Databento manifest records *acquisitions
   made by this tool*, with a checksum this tool computed at the moment of
   placement (D-0017). A hand-downloaded CSV has none of that.
3. **Its own inventory file, per vendor.** `external/cboe/README.md` records
   the source URL, the download date, the row count and the availability rule.
   ThetaData will get `external/thetadata/inventory.json` on the same principle.
   This is not the Databento manifest and must never be merged into it:
   **different vendor, different trust**. The manifest's guarantee is "this
   tool fetched these bytes and hashed them here"; an inventory's is "a human
   says this came from there on that date". Collapsing the two would quietly
   downgrade the first to the second for everything in it.

---

## What `layout-check` enforces

Run it after every `transcode` and after every blitz tranche. It reads
directory entries only, so it costs milliseconds where `verify` — which
re-hashes every byte — costs minutes. Neither implies the other: a file can
hash correctly at the wrong path, and a file at the right path can be corrupt.

| # | violation | why it matters |
|---|---|---|
| 1 | a file at the wrong depth under `raw/` | breaks the purchase tuple and coverage subtraction |
| 2 | a schema directory outside the known set | data paid for that nothing will ever look in |
| 3 | a manifest `file_path` that does not parse as `dataset/schema/symbol/window` | a provenance record that cannot be resolved |
| 4 | `.parquet` under `raw/`, `.dbn*` under `curated/` | invariant 2: one directory, one kind |
| 5 | a curated file outside `kind/instrument/grain` | invariant 1, and `rm -rf` stops being safe |
| 6 | a curated kind outside the known set | the same typo argument as row 2, on the research side |
| 7 | anything unrecognized at the archive root | the tree grew something nobody decided on |
| 8 | a `curated/bars/` contract whose year is one digit | it can hold two contracts a decade apart, in perfect `ts_open` order, and every other check passes (D-0072) |

Every finding is reported — filesystem findings in walk order, manifest
findings in record order — because an operator deciding whether a tree is
salvageable needs the extent of the problem, not its first instance. Exit 4 on
any violation, 0 when clean.

**There is no `--fix`, and there should never be one.** See `raw/` above.
