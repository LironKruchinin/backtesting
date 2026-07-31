# T0 refusal forensic — what the 794 actually are

**Date:** 2026-07-31. **Method:** detached resume of the T0 tranche over its full
documented span, plus `theta-golden` reproductions against the live Terminal. No
data purchased — ThetaData is a flat subscription and every request here is a
re-read of an entitled window. Nothing was written into the archive.

D-0092 established that the 794 missing `greeks/eod` records were **refusals**
rather than vendor-absent data, from the shape of the code: `Outcome::Refused`
appends nothing to the inventory, so a refused request is indistinguishable from
one never attempted. That was an inference from the writer's structure. This
document is the **measurement**, and it confirms the inference and then goes
past it: the refusals have **two distinct causes**, not one, and only one of them
is the build being conservative.

## The measurement

A resume over the full T0 span (`2012-06-01 → 2026-06-30`, per
`THETADATA_PLAN.md` §83) reconciles exactly:

| quantity | value |
|---|---|
| already in the inventory | 82,668 |
| outstanding | **794** |
| endpoints outstanding | `/option/history/greeks/eod` **only** |
| `eod`, `open_interest` outstanding | **0** |

The outstanding set is the refusal set. `eod` and `open_interest` are whole,
which is what the post-mortem found and what makes the greeks column the only
one short.

Re-attempting them produced **200 attempts, 0 written, 0 empty, 200 refused**,
at which point the refusal-rate breaker halted the run and said so:

```json
{"schema_version":1,"kind":"halted",
 "reason":"refusal rate 100.0% over 200 attempts exceeds the 2.0% limit — that is
 this build misreading the feed, not a vendor hiccup. Nothing refused was
 inventoried, so a fixed build resumes cleanly",
 "attempted":200,"written":0,"empty":0,"refused":200}
```

**A plain resume cannot recover these records.** The refusal is deterministic in
the response, so the same request refuses on every re-run. Whatever fixes them is
a change to this build, not another pull.

### Both breaker outcomes were correct, and they are not in tension

The original run refused 794 of 83,489 — **0.95 %**, below the 2 % limit, so the
breaker correctly did not trip. This run refused 200 of 200 — **100 %**, so it
correctly did. The population changed, not the phenomenon: a resume re-attempts
*only the previously-refused set*, so its refusal rate is ~100 % by construction.
Neither reading is a malfunction, and a future reader comparing the two numbers
should not treat the difference as a regression.

## Two causes, not one

Sampling the outstanding dates separates them cleanly:

| date | rows | rows with `iv_error ≥ 100` | cause |
|---|---|---|---|
| 2020-10-29 | 4,999 | 4,998 | **A** — all-zero series |
| 2021-03-02 | 5,947 | 5,946 | **A** |
| 2021-12-23 | 7,235 | 7,234 | **A** |
| 2023-09-19 | 12,493 | **0** | **B** — duplicate row |
| 2023-09-27 | 12,841 | **0** | **B** |

### Cause A — `AllZeroSeries`, via the IV sentinel

Every row of the affected chain-days carries `iv_error = 100.0000`.
`is_zero_sentinel` is `underlying_price == 0.0 || iv_error >= IV_ERROR_SENTINEL`,
so every row is dropped as the vendor's "absent", `kept` empties, and the
file-level gate refuses. This is the code **working exactly as designed** — there
is an existing test pinning `is_zero_sentinel(4742.83, 100.0) == true`, i.e. a
real underlying price with a failed solve is a sentinel by intent.

Whether the refusal is *right* is a judgement about the data, and the evidence
points both ways, so it is recorded rather than resolved:

- **For refusing:** `implied_vol` and `iv_error` are exactly `0.5000` / `100.0000`
  across the whole chain-day — the signature of an IV solver that never
  converged and emitted its initial guess with a maximum-error flag. Greeks
  derived from a non-converged fit are not data.
- **Against refusing:** `underlying_price` is real and correct (SPX 3870.29 on
  2021-03-02), the contract set reconciles with `eod` at exact parity
  (5,946 = 5,946), and non-IV columns are populated. The `eod` and
  `open_interest` files for the same days validate and archive normally.

The honest statement is that these days have a **real underlying and an
unusable IV surface**, and the current gate is all-or-nothing at file level.

### Cause B — `DuplicateRow`, same contract *and* same timestamp

```
2025-01-17|4375.000|CALL appears 2 times sharing timestamp=2023-09-19T00:00:00.000
```

D-0054's dedup handles a contract repeating across build passes, discriminated by
its timestamp. The same contract at the *same* timestamp has no discriminator and
nothing explains it, so the file is refused. This cause is unrelated to cause A
and is not an IV problem at all.

## Three operational findings, worth more than the count

1. **A detached pull loses `.env`.** `.env` is discovered relative to the working
   directory (D-0022). A `schtasks` task runs from `C:\Windows\System32`, so
   `CRUCIBLE_DATA_DIR` never resolves and the run exits 4 before doing anything.
   The task must `cd /d` into the repo. The refusal itself was correct — the CLI
   declining to run with no archive root is the exit-code contract working.
2. **A detached pull with nowhere to put stdout is unobservable, and looks
   hung.** Progress and the refusal list go to `println!`. Under Task Scheduler
   that goes nowhere, and the process sits with eight established connections,
   ~0.5 % CPU and no I/O — indistinguishable from a deadlock. Redirecting to a
   file resolved it instantly. D-0092's heartbeat is what should cover this, and
   it did once the path was read correctly — it lands at `data_dir` root
   (`G:\Crucible\heartbeat.txt`), **not** inside `external/thetadata/`.
3. **The breaker bounds processing, not fetching, when `outstanding ≤
   BREAKER_CHECKPOINT`.** The window is 2,048 and the outstanding set was 794, so
   the whole run is one window: the breaker set `halted` at attempt 200, and the
   remaining 594 requests were still fetched over the network and discarded
   unprocessed. The module doc's rationale — avoiding "draining 80,000 fetches it
   has already decided not to keep" — holds for a large tranche and not for a
   small resume. Nothing is corrupted by this; it costs wall-clock and vendor
   load on a run that has already decided to stop.

Measured latency under the run's own 8-way concurrency: **median 150.6 s**, p95
406 s, max 460 s, against **8.7 s** for the identical request issued in
isolation. Whole-chain SPX greeks days are the heaviest request in the tranche
and they contend hard with each other.

## What is owed

Nothing here is fixed, and deliberately so — the archive is unchanged and no
number in it moved.

1. **Classify all 794**, not the five sampled. The A/B split above is a sample;
   the full partition is one pass with no writes.
2. **Decide cause A on its merits.** Either the IV sentinel is right and these
   chain-days are genuinely unusable — in which case they should be recorded as
   *empty* rather than refused, so a resume stops asking — or a file whose
   underlying is real deserves a narrower gate than all-or-nothing. Both are
   decision-log changes, and the second one changes what the archive contains.
3. **Cause B needs its own answer**, and it is not the same answer. A contract
   duplicated at an identical timestamp is a vendor defect this build has no
   model for; refusing is defensible, and so is keeping one copy once somebody
   has established the two rows are identical.
4. **A negative control for whichever gate changes** (§7): plant a genuinely
   all-zero day and a genuinely duplicated row, and watch the gate fire on each.
