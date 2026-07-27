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
      least **80 GiB** free — ~53 GiB of archive plus headroom for staging,
      which holds a full copy of the largest single delivery before it is
      renamed into `raw/`.
- [ ] `verify` is clean.
- [ ] The build with the client works: `cargo build -p crucible-cli --features databento`.

Every command below needs `--features databento`. It is off by default so
ordinary builds and CI stay free of the async client (D-0025).

## 1. Subscribe, and confirm the windows in the portal

- [ ] Subscribe to **Standard** ($199/month).
- [ ] Write the subscription start date here: `____-__-__`
- [ ] Day-30 cancellation deadline (start + 30 days): `____-__-__`
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

### 3a. `mbo` — ES, last 1 month (most perishable)

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema mbo --symbols ES.FUT \
       --start <today - 30d> --end <today> --window whole \
       --execute --max-cost-usd 0.00 --timeout-mins 240
```

- [ ] Done. This is ~16.5 GiB billable; expect a long processing wait.

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
cargo run -p crucible-cli -- verify
```

Re-hashes every archived file against the manifest. Findings are reported in
manifest order; anything other than "archive clean" stops the blitz until it
is understood. Raw is append-only — a corrupt file is corrected by acquiring a
new one at a new path, never by editing (D-0017).

## 5. Cancellation

- [ ] Cancel the Standard subscription **before day 30**: `____-__-__`

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
| exit **2**, "another pull is running" | A second `pull` tried to start against the same archive. | Wait for the first to finish. Two concurrent pulls could each submit the same window. |
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
