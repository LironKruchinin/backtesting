# Archive holes — the measured survey

Measured 2026-07-31 with `crates/crucible-data/examples/session_profile.rs`
(read-only; opens curated Parquet through the production `ParquetBarFeed` and
the production `Calendar`, touches no `raw/`, writes nothing).

This file exists because a refetch ruling was issued against a two-item hole
list, and the survey that should have preceded it found the list incomplete in
a way that changes what a refetch should target. **Nothing has been refetched.
No money has been spent.** See §4 for what stopped.

---

## 1. Method, and the one thing it cannot see

Every bar with **nonzero volume** sets a bit in a `(local civil date, local
minute)` grid, in the exchange's own timezone. A trading day is flagged when its
*day* session is absent, read beside its *evening* session, so a closure
(removes both) is distinguishable from an early close (removes neither).

**What this cannot see is a hole inside a file that `coverage` says we own.**
`Catalog::coverage` subtracts acquired manifest ranges from the requested range;
a monthly raw file whose record claims 2012-09-01..2012-10-01 credits every
instant in that month whether or not the bytes contain bars for it. Every hole
below is of exactly that kind — which is why none of them has ever appeared in
a `coverage` report, and why they were found by counting bars instead.

## 2. The two holes on the ruling's list — both real, both ONE trading day

Both are a **single trading day**, and each is missing from **one root only**.

### GC — trading day 2012-09-12

| local date | traded minutes | reading |
|---|---|---|
| 2012-09-10 (Mon) | 00:00 .. 24:00 | normal |
| 2012-09-11 (Tue) | 00:00 .. 16:15 | day session only — **evening missing** |
| 2012-09-12 (Wed) | 17:00 .. 24:00 | evening only — **day session missing** |
| 2012-09-13 (Thu) | 00:00 .. 24:00 | normal |

The gap is contiguous from 17:00 CT on the 11th to 16:15 CT on the 12th — which
is precisely the trading day 2012-09-12 under GC's pre-2015-09-21 era (open
17:00, close 16:15 CT, D-0089). Not a holiday. **ES, CL and NQ are complete
across the same window**, so it is not an exchange event.

### ZN — trading day 2014-10-03

| local date | traded minutes | reading |
|---|---|---|
| 2014-10-02 (Thu) | 00:00 .. 16:00 | day session, ending on its settlement print — **evening missing** |
| 2014-10-03 (Fri) | *no bars at all* | **day and evening both absent** |
| 2014-10-05 (Sun) | 17:00 .. 24:00 | normal |

Not a holiday, not an early close. `holidays` mode flags 2014-10-03 with
`NONE / NONE`. **ES, CL and GC are complete on 2014-10-03.**

> The 16:00 CT prints that prompted the ZN look are a separate matter and are
> **not** a hole: they are settlement prints at the close minute, the era
> table's close is correct, and CLAUDE.md §9 carries the measurement.

## 3. The hole the list did not have — and it is the big one

Widening the same query past the two named dates found a **multi-root cluster in
late September 2014**:

| root | missing weekdays, 2014-09-22 .. 2014-09-30 |
|---|---|
| ES | 2014-09-25 |
| CL | 2014-09-25 |
| GC | 2014-09-25 |
| NQ | 2014-09-23, 2014-09-24, 2014-09-25 |
| 6E | 2014-09-23, 2014-09-24, 2014-09-25 |
| **ZN** | **none — complete** |
| ~~RTY~~ | *not applicable, see below* |

**2014-09-25 is missing from five of the six roots that existed then.** ZN having
it is what rules out an exchange closure: a closed exchange closes for rates too.
So this is an acquisition-side gap, it is at least 3× the size of the two holes
on the list, and it was not on it.

**RTY is excluded and its flags there are vacuous.** RTY's archive begins in
**2017** — `grid RTY 1m 2010 2018` returns no row before it — because the
E-mini Russell 2000 did not list on CME until then. Any pre-2017 RTY "gap" is
the instrument not existing, not data missing, and a survey that counted them
would inflate every total. Worth stating once here so the next survey does not
re-derive it.

## 4. Why nothing was refetched

The refetch ruling specifies, verbatim, that an empty vendor response is
recorded as *"absence records with new cause `VendorGap`"* and that
*"qa/coverage must then report the sessions as explained-absent"*.

**There is no absence-record system on this path to add a cause to.**
`AbsenceCause` (`SolverSentinel`, `AmbiguousDuplicate`) belongs to the
**ThetaData options inventory** — it was built for the 794 refusals (D-0092,
D-0094, D-0095). The Databento futures manifest has no counterpart: its only two
line kinds are `ManifestRecord` and `SymbolSupplement`, and `Catalog::coverage`
computes gaps by pure range subtraction with no notion of a gap being
*explained*.

So "add a new cause" is not additive here; it means designing absence records
for the manifest, and that runs straight into **D-0014**, which makes a record's
identity the blake3 of its file's bytes. **An absence record has no bytes.** What
identifies it, what `verify` does with it, and whether `coverage` may subtract a
range nothing was ever written for are decisions, not implementation details —
and the last one changes what "coverage 98 %" means archive-wide.

That is one of two reasons the build stopped. The other is §3: the ruling was
issued against a hole list that turns out to be missing its largest entry, and
refetching two single-day per-symbol gaps first would heal the least of the
problem while spending money to do it.

## 5. What a decision would have to settle

1. **Identity of a byte-less record.** D-0014 says manifest id = blake3 of the
   file. An absence record names a window nothing was written for. Does it get a
   synthetic id, no id, or does it live outside `manifest.jsonl` entirely?
2. **Does `coverage` subtract an explained absence?** If yes, a run's data
   manifest ids no longer account for every instant it replayed, and "complete"
   stops meaning "bytes exist". If no, `qa` and `coverage` disagree forever.
3. **What makes a vendor's empty response trustworthy?** An empty response and a
   request that silently matched nothing look identical. Recording `VendorGap`
   on the second is how a hole becomes permanent and *documented*, which is
   worse than an undocumented one.
4. **Refetch scope.** Given §3, is the target the two named days, the
   2014-09-25 cluster, or a full archive-wide bar-count survey first? The survey
   in this file took minutes and is read-only; it should probably run to
   completion across all roots and all years before anything is bought.
