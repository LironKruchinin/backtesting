# Runbook — the one-month acquisition blitz

D-0023 buys the entire archive inside **one $199 Databento Standard month**
rather than metering ~$1,901 pay-as-you-go. That makes the subscription a
30-day clock against a large batch run, and this file is the checklist that
runs against it.

Read `docs/DATA_PLAN.md` first — it says *what* is being bought. This says
*in what order*, *with what checks*, and *how to stop*.

**Do not start until `pull` is finished and unattended-capable.** That is the
whole point of the milestone: the clock starts the day you subscribe, not the
day you are ready.

---

## 0. Pre-flight (before subscribing — costs nothing)

```bash
cargo run -p crucible-cli -- env          # both variables resolved?
cargo run -p crucible-cli -- verify       # archive intact before we add to it
cargo test --workspace                    # everything green
```

- [ ] `DATABENTO_API_KEY` reports "set" (`env` prints the length, never the key).
- [ ] `CRUCIBLE_DATA_DIR` points at a directory **outside the repo** with at
      least **250 GB** free. Measure it, do not estimate it:

      ```powershell
      Get-PSDrive -PSProvider FileSystem |
        Select-Object Name,@{n='FreeGB';e={[math]::Round($_.Free/1GB,1)}}
      ```

      **Billable size is not on-disk size, and every figure in `DATA_PLAN.md`
      is billable.** Databento bills on the *uncompressed* byte count; what
      lands in `raw/` is zstd-compressed DBN. Measured on 2026-07-28: the
      one-month `mbo` job quoted **15.10 GiB billable** and archived as
      **4.44 GiB** — a 3.4x difference. Sizing a disk from billable numbers
      overstates the need by roughly that factor, and sizing a *bill* from
      on-disk numbers understates it by the same. Keep the two words apart.

      Where 250 comes from, for the seven-parent basket quoted 2026-07-28
      (**99.4 GiB billable**, which is what the dry runs print):

      | | billable | expected on disk |
      |---|---|---|
      | `raw/` archive | 99.4 GiB | ~30-45 GiB |
      | `staging/` peak | 15.1 GiB | ~5 GiB (one delivery at a time) |
      | `curated/` Parquet | — | ~30-60 GiB (bars only, decoded) |

      That is ~100 GiB expected. **250 GB is deliberately about 2.5x that**:
      compression ratios differ by schema, the curated fan-out is the least
      predictable term, and running out of disk mid-blitz costs a day of the
      30 you are paying for while running out of headroom costs nothing.
      **`DATA_PLAN.md`'s ~53 GiB is billable and for ES+NQ+RTY only** — all
      seven parents nearly triples `ohlcv-1s` alone (19.7 -> 53.6 GiB
      billable). The earlier 80 GiB and 150 GB figures predate both
      corrections.
      Running out of disk mid-blitz loses no data — a delivered job stays
      downloadable for 30 days — but it costs a day.
- [ ] `verify` is clean.
- [ ] The build with the client works: `cargo build -p crucible-cli --features databento`.

Every `pull` below needs `--features databento`, and so does `transcode` (it
decodes DBN). `env`, `verify`, and `backtest` do not. The feature is off by
default so ordinary builds and CI stay free of the async client (D-0025).

## 1. Subscribe, and confirm the windows in the portal

- [ ] Subscribe to **Standard** ($199/month).
- [x] Write the subscription start date here: **2026-07-28**
- [ ] **Day-25 cancellation reminder** (start + 25 days): **2026-08-22** — put it
      in a calendar now. Cancelling on day 25 keeps every entitlement to day 30
      and leaves five days of slack for a job that will not finish.
- [ ] Day-30 hard deadline (start + 30 days): **2026-08-27**
- [ ] In the Databento portal, confirm the entitlement windows the plan
      assumes: **L0 bars 16+ years**, **L1 `trades`/`tbbo` rolling 12 months**,
      **L3 `mbo` rolling 1 month**. If any window is narrower than this,
      stop and re-plan — the buy list in `DATA_PLAN.md` assumes these.

## 2. Prove the entitlement is live — through our own gate, not the portal

The portal saying "subscribed" and the API pricing our exact requests at zero
are different claims. Only the second one matters.

For **every** job in the plan, run the dry run and read two things:

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m \
       --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
       --start 2010-06-06 --end <today, UTC> --window whole
```

- [ ] The per-window `cost` column reads **$0.0000** for every row.
- [ ] The entitlement line reads **"a flat-rate entitlement IS active"**.

If either is false, **stop**. A non-zero quote means the subscription is not
covering this request and executing would bill you at the metered rate — which
for 16 years of `ohlcv-1s` is four figures. This is the check the whole
dry-run-by-default design exists to make cheap; do not skip it because the
portal looks right.

Repeat for each schema in the buy list. Dry runs cost nothing and can be run
as many times as you like.

## 3. Pull — in this order

**Rolling windows decay daily. Perishable data first.** The 16-year L0 bars
have been sitting there since 2010 and will still be there next week; the
`mbo` window loses a day every day.

Every command runs with `--max-cost-usd 0.00`, so it **refuses rather than
bills** the moment the entitlement is not covering the request.

### 3.0 Expect exit 5. It is not a failure.

The vendor queue is FIFO and these are large jobs; several of them will
**outlive any timeout you set**. `--timeout-mins` defaults to **60**, which is
far too short for this list. When the timeout expires `pull` prints "still
processing after Ns" and exits **5**.

Exit 5 means: *the window is bought, journalled, and downloadable for 30 days;
we simply stopped waiting.* Nothing was lost and nothing must be redone. The
recovery is to **re-run the identical command** — `intent_id` is deterministic,
so the re-run recognises its own previous attempt, adopts the existing job, and
submits nothing twice (D-0034, D-0029).

- Use `--timeout-mins 240` for the L1/L3 tranches and `--timeout-mins 480` for
  the 16-year L0 backfill. These are the values in the commands below.
- Raising the timeout does not make a job finish sooner; it only reduces how
  often you re-run. Leaving a terminal open overnight and re-running in the
  morning is a perfectly good strategy.
- Do **not** treat exit 5 as a reason to change flags, widen the cost cap, or
  re-plan. The only two exit codes that mean "stop and think" are **3**
  (refused to spend) and **4** (provider or filesystem failure).

A cron wrapper must therefore keep re-running on 5 rather than alerting on it —
which is exactly why 5 is not 0.

**A poll that loses its connection currently exits 4, not 5.** `RETRIES` in
`ingest::databento` is 4 attempts with ~15 s of backoff, which is tuned for the
burst of metadata calls `quote()` makes — not for a poll loop that runs for
hours, where a two-minute network outage exhausts it and fails the tranche. The
job is unharmed (submitted and journalled), so re-running is always correct, but
an unattended wrapper has to know that this particular exit 4 is retryable while
others are not. Arguably the poll should resolve an unknowable job state to
exit 5 — "still in flight, re-run" — which is exactly what it means. That change
is money-path code and has not been made; it is written down here so the next
session does not rediscover it.

### 3a. `mbo` — ES, last 1 month (most perishable)

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema mbo --symbols ES.FUT \
       --start <today - 30d> --end <today> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] Done. **15.1 GiB billable**, which archived as **4.44 GiB** on disk when
      this was run on 2026-07-28 (billable is the vendor's uncompressed billing
      metric — see §0). `DATA_PLAN.md`'s 16.5 GiB is an older quote for a
      slightly different window. Expect a long processing wait: this one took
      14 minutes end to end, but the queue is FIFO and that is not a promise.

### 3b. `tbbo` — ES, last 12 months

```bash
... --schema tbbo --symbols ES.FUT --start <today - 12mo> --end <today> \
    --window whole --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] Done.

### 3c. `trades` — ES, last 12 months

```bash
... --schema trades --symbols ES.FUT --start <today - 12mo> --end <today> \
    --window whole --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] Done.

### 3d. The L0 backfill — bars, definitions, statistics, all seven parents

Not perishable. Run these last, one schema at a time:

```bash
for SCHEMA in ohlcv-1m ohlcv-1s definition statistics; do
  cargo run -p crucible-cli --features databento -- \
    pull --dataset GLBX.MDP3 --schema "$SCHEMA" \
         --symbols ES.FUT,NQ.FUT,RTY.FUT,CL.FUT,6E.FUT,ZN.FUT,GC.FUT \
         --start 2010-06-06 --end <today> --window whole \
         --execute --max-cost-usd 0.00 --timeout-mins 480
done
```

- [ ] `ohlcv-1m` done.
- [ ] `ohlcv-1s` done (the big one — ~19.7 GiB billable for the equity indices alone).
- [ ] `definition` done.
- [ ] `statistics` done.

## 4. Verify after each step

```bash
cargo run -p crucible-cli -- verify         # are these the bytes we recorded?
cargo run -p crucible-cli -- layout-check   # is everything where it belongs?
```

Two different questions, and neither implies the other: a file can hash
correctly at the wrong path, and a file at the right path can be corrupt.
`layout-check` reads directory entries only, so it costs milliseconds against
`verify`'s minutes — run it freely. It enforces `docs/DATA_LAYOUT.md` and exits
4 on any violation. It never moves anything: manifest paths are load-bearing,
so a misplaced raw file is corrected by acquiring a new one at a correct path,
never by renaming (D-0017).

`verify` re-hashes every archived file against the manifest. Findings are reported in
manifest order; anything other than "archive clean" stops the blitz until it
is understood. Raw is append-only — a corrupt file is corrected by acquiring a
new one at a new path, never by editing (D-0017).

## 5. Cancellation

- [ ] Cancel the Standard subscription on the **day-25 reminder**, day 30 at the
      latest: **2026-08-22 / 2026-08-27**. Cancelling does not revoke the month you paid for,
      and it removes the only way this project accidentally spends $199 twice.

After cancellation the recurring monthly job (~$63 metered) is cheaper than
the subscription, so we stay unsubscribed until M4 needs the live feed
(D-0023). The recurring job runs at `--max-cost-usd 0.00` precisely so it
refuses instead of billing if the entitlement situation ever changes without
anyone noticing.

---

## When something goes wrong

The design assumption is that **the same command, run again, is always the
right recovery**. `intent_id` is deterministic, so a re-run recognises its own
previous attempt; it will never submit a window twice.

| what you see | what it means | what to do |
|---|---|---|
| exit **5**, "still processing after Ns" | The job is bought, recorded, and downloadable for 30 days from submission. Nothing was lost. | Re-run the identical command. Raise `--timeout-mins` if it keeps happening. |
| exit **3**, "refusing to spend" | The quoted total exceeded the cap — usually the entitlement is not covering the request. | Go back to step 2. Do **not** raise the cap to make it pass. |
| exit **4**, "another pull is running" | A second `pull` tried to start against the same archive. | Wait for the first to finish. Two concurrent pulls could each submit the same window. |
| exit **4**, "provider failed while checking a batch job's state" | A network blip during the **poll** loop. The job is submitted and journalled; only our view of it was lost. Seen three times across 2026-07-27/28 — an HTTP 504 and two TCP connect timeouts. | **Re-run the identical command.** Reconciliation adopts the existing job and submits nothing (D-0029). This is the one exit-4 that is safe to retry unattended, and only because the submission already happened. |
| "cannot tell which vendor job covers …" | Two vendor jobs match one window. | Inspect them in the portal. Refusing is correct: adopting the wrong one archives the wrong bytes, submitting buys the window twice. |
| "does not match the vendor checksum" | The download is corrupt or truncated. Nothing reached `raw/`. | Re-run. The re-download is free inside the 30-day window. |
| "delivered N payload files, expected exactly 1" | The window resolved to no data, or the split parameters did not take effect. | Re-run once. If it repeats, narrow the window and check the dataset condition in the portal. Nothing was placed and the job stays downloadable. |
| "has expired and can no longer be downloaded" | The 30-day window closed before collection. Re-buying costs money. | Only if you accept the charge: re-run with `--repurchase-expired`. |

### The journal

`$CRUCIBLE_DATA_DIR/jobs.jsonl` is an append-only record of every intent and
what became of it. It is safe to *read* — `intended` → `submitted` →
`downloaded` → `appended` is the full life of one window.

**Do not hand-edit it.** It parses with `deny_unknown_fields` and a pinned
`schema_version`, so a hand edit is more likely to hard-error on the next run
than to do what you meant — and the failure mode of a *successful* hand edit
is a second purchase.

### Known gap: a permanently stuck intent

There is currently no operator command to retire an intent that can never
complete — for example a job that legitimately delivered zero data files
because the window contains no data. `--repurchase-expired` does not apply
(the job has not expired), and hand-editing the journal is worse than the
problem. For now: narrow the window so the empty stretch is not requested, or
leave the intent alone — it costs nothing to leave in place, since nothing
re-submits it. A `crucible jobs abandon <intent>` command is the obvious fix
and is not built yet.

## Unattended runs

Overnight and unattended runs are **permitted**, with two conditions:

1. **`--max-cost-usd 0.00`.** An unattended execute that cannot spend is a
   different risk category from one that can. The cap refuses rather than bills
   if the flat-rate entitlement has lapsed, so the failure mode of an
   unattended run outliving the subscription is an error message, not a charge.
2. **The session declares itself.** A session left running unattended states
   that fact in its report. This is the cheap half of what made the 2026-07-29
   forensic expensive: the run was authorized and free, but nothing had
   announced it, so attributing it required first ruling out scheduled tasks,
   `Run`/`RunOnce` keys and Startup entries.

The rule exists so the next forensic starts from *declared* rather than
*discovered*. Unowned automation on the money path must have an owner even at
$0.00 — "harmless and unknown" is not a combination this project keeps. Any
scheduler task found later must be recorded here and retired as part of §5
cancellation: a $0.00-capped task firing post-lapse refuses harmlessly, but
that is not a reason to leave an unknown one running.

### Recorded: the 2026-07-29 03:47 run

Four `definition` intents (`6E.FUT`, `CL.FUT`, `ZN.FUT`, `GC.FUT`) were
submitted 2026-07-29 03:47 local by Liron's authorized unattended agent
session; scheduler / `Run`-key / Startup checks all negative (retained as the
evidence that narrowed it); adopted and appended 14:42 local by the session
resume at $0.00. Confirmed by Liron.

The three earlier definition intents (`ES`, `NQ`, `RTY`) are a separate run —
submitted 2026-07-28 15:56, appended 19:53–19:54 the same day, ahead of the
subscription, as §3 states. Per-intent timestamps come from `jobs.jsonl`, which
is the arbiter whenever the narrative and the checklist disagree.
