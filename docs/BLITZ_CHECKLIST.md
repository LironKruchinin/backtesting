# Blitz day — the commands, in order

`docs/DATA_PLAN.md` says *what* to buy and `docs/RUNBOOK_BLITZ.md` says *why*
in that order and how to recover. This file is neither. It is the thing you
keep open on blitz day: every command, already written, in the order they run,
with a box beside each.

Every command below was checked against the CLI as it exists on
**2026-07-28**. Flags that do not appear here do not exist.

**Substitutions.** Write the real values in once, here, and then copy the
commands verbatim:

**Filled in for the run of 2026-07-28.** Reset these if you run it again.

- `<TODAY>` — today's UTC date: **2026-07-28**
- `<TODAY-30D>` — 30 days before that: **2026-06-28**
- `<TODAY-12MO>` — 12 months before that: **2025-07-28**
- Subscription start: **2026-07-28** · **cancel on day 25: 2026-08-22**
  (day-30 hard deadline **2026-08-27** — do not plan for it)

**Two things that are true of every `pull` below.**
`--execute --max-cost-usd 0.00` means *refuse rather than bill*: the moment the
entitlement stops covering a request, the command exits 3 and spends nothing.
And **exit 5 is not a failure** — it means the window is bought and
downloadable for 30 days and we stopped waiting. Re-run the identical command.
See RUNBOOK_BLITZ §3.0.

---

## 0. Pre-flight — costs nothing, do it the day before

```bash
cargo run -p crucible-cli -- env
cargo run -p crucible-cli -- verify
cargo run -p crucible-cli -- layout-check
cargo test --workspace
cargo test --workspace --all-features
cargo build -p crucible-cli --features databento
```

```powershell
Get-PSDrive -PSProvider FileSystem |
  Select-Object Name,@{n='FreeGB';e={[math]::Round($_.Free/1GB,1)}}
```

- [ ] `DATABENTO_API_KEY` reports "set".
- [ ] `CRUCIBLE_DATA_DIR` is outside the repo and its drive has **≥ 250 GB**
      free. The dry runs print *billable* sizes, which are uncompressed and
      about 3.4x what actually lands on disk — 250 GB is ~2.5x the ~100 GiB
      really expected. See RUNBOOK §0 for the arithmetic.
- [ ] `verify` says the archive is clean.
- [ ] Both test runs green.
- [ ] The `--features databento` build succeeds.

## 1. Subscribe, then prove it through our own gate

Do **not** skip this because the portal says "subscribed". The portal's claim
and the API pricing our exact requests at zero are different claims, and only
the second one matters.

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m \
       --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
       --start 2010-06-06 --end <TODAY> --window whole
```

- [ ] Every row's `cost` column reads `$0.0000`.
- [ ] The last line reads **"a flat-rate entitlement IS active"**.

Repeat with `--schema` set to each of `ohlcv-1s`, `definition`, `statistics`,
`trades`, `tbbo`, `mbo` (with that schema's own symbols and dates from §2–§3).
Dry runs are free and unlimited.

**If any of these is not $0.00, stop.** Do not raise the cap.

## 2. Perishable first — ES only, one schema at a time

Rolling windows slide forward every day. The 16-year bars have been sitting
there since 2010; the `mbo` window loses a day every day.

### 2a. `mbo` — last 1 month  (15.1 GiB billable ≈ 4.4 GiB on disk)

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema mbo --symbols ES.FUT \
       --start <TODAY-30D> --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] exit 0 (re-run on exit 5 until it is 0)
- [ ] `cargo run -p crucible-cli -- verify` → clean

### 2b. `tbbo` — last 12 months

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema tbbo --symbols ES.FUT \
       --start <TODAY-12MO> --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] exit 0
- [ ] `verify` → clean

### 2c. `trades` — last 12 months

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema trades --symbols ES.FUT \
       --start <TODAY-12MO> --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] exit 0
- [ ] `verify` → clean

## 3. The L0 backfill — all seven parents, 16 years

Not perishable. One schema at a time, longest timeout, and **`ohlcv-1s` is the
big one** (~19.7 GiB billable for the equity indices alone).

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m \
       --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
       --start 2010-06-06 --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 480
```

- [ ] `ohlcv-1m` exit 0 · `verify` clean

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1s \
       --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
       --start 2010-06-06 --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 480
```

- [ ] `ohlcv-1s` exit 0 · `verify` clean

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema definition \
       --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
       --start 2010-06-06 --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 480
```

- [ ] `definition` exit 0 · `verify` clean
      *(ES/NQ/RTY definitions were archived on 2026-07-28 ahead of the
      subscription, so `coverage` subtracts them and only CL/6E/ZN/GC are
      actually bought here. Re-requesting the owned windows costs nothing —
      the plan simply comes back smaller.)*

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema statistics \
       --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
       --start 2010-06-06 --end <TODAY> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 480
```

- [ ] `statistics` exit 0 · `verify` clean
      *(2026-07-30: all seven parents **appended** — `6E` on 07-29, the other six
      adopted under their original `GLBX-20260729-*` job ids on 07-30, nothing
      re-submitted. Box stays open only because `verify` has not been re-run
      since; do that once T0 stops competing for disk I/O. **Note when you do:**
      on a re-run, `--end` must be the date the jobs were submitted with
      (`2026-07-29`), not today — a wider window is a different intent id and
      mints new jobs instead of collecting the paid ones.)*

## 4. After each tranche — verify, then curate

`verify` re-hashes the archived bytes against the manifest. It cannot see a
missing Tuesday, which is what `qa` is for.

```bash
cargo run -p crucible-cli -- verify
cargo run -p crucible-cli -- layout-check
cargo run -p crucible-cli --features databento -- transcode
cargo run -p crucible-cli -- layout-check
cargo run -p crucible-cli -- qa --instrument ESH6 --timeframe 1m
```

- [ ] `verify` clean after every tranche.
- [ ] `layout-check` clean after every tranche **and again after `transcode`**
      — the second run is the one that catches a curated file written to the
      wrong path, which is the only way this project has ever grown a tree it
      did not intend. Milliseconds; run it as often as you like.
- [ ] `transcode` run after the bar tranches (it needs the feature; it decodes DBN).
- [ ] `qa` run on at least one front-month contract per parent, per timeframe.
      Findings exit 4. Coverage below ~99 % on a liquid front month, or any
      "bars outside any session", is worth understanding **before** the
      subscription ends and the data stops being re-downloadable.
      **Only ES/NQ/RTY have a bundled calendar.** For CL, 6E, ZN and GC, `qa`
      warns and skips coverage, out-of-session and DST checks — the
      self-consistency checks (zero volume, spikes, OHLC) still run. Adding
      those calendars is the obvious follow-up; their session template differs
      (no equity-index holiday pattern), so do not point `--calendar
      cme_globex_equity_index` at crude oil.

Note: `transcode` and `qa` handle `ohlcv` only. `definition`, `statistics`,
`trades`, `tbbo` and `mbo` stay as raw DBN in the archive until a later
milestone reads them.

## 5. Build the roll tables — free, and only possible with the data here

```bash
cargo run -p crucible-cli --features databento --   rolls --root ES --timeframe 1m --write
```

- [ ] One roll table per parent, checked for a plausible number of rolls
      (roughly four a year for the equity indices).
- [ ] Each table reports `expiries  databento-definition`, not
      `nominal-third-friday`. The nominal rule is the third Friday of the
      contract month, which is the *equity-index* convention — CL, ZN and 6E
      all expire on different rules, so a nominal table for those roots is
      wrong in a way nothing else will catch. `--features databento` is what
      lets `rolls` read the archived `definition` files; without it the flag
      silently falls back.

## 6. Cancel

- [ ] **Day 25 — 2026-08-22**: cancel the Standard subscription. Cancelling
      does not revoke the month already paid for. It removes the only way this
      project accidentally spends $199 twice.
- [ ] Day 30 is the hard deadline. Do not use it as the plan.
- [ ] After cancelling, the recurring monthly job (~$65 metered) resumes being
      cheaper than the subscription (D-0023). It runs at `--max-cost-usd 0.00`
      so it refuses rather than bills if the entitlement situation changes.

## 7. When something goes wrong

The table in `docs/RUNBOOK_BLITZ.md` ("When something goes wrong") is the
reference. The short version:

| exit | meaning | do |
|---|---|---|
| 0 | done | next box |
| 2 | usage or config error | fix the command |
| 3 | refused to spend | **stop** — go back to §1. Never raise the cap |
| 4 | provider or filesystem failure, **including "another pull is running"** | read the message, re-run |
| 5 | still processing | re-run the identical command |

`HTTP 504` from the vendor surfaces as exit 4 mid-poll. Nothing is lost — the
jobs are submitted and journalled — and re-running the identical command adopts
them. It happened on the 2026-07-28 definitions pull.
