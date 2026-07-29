# ThetaData acquisition plan

Companion to `DATA_PLAN.md` (Databento). Adopted by **D-0050**, which supersedes
D-0010's post-M4 descope. Everything here was measured against a running Theta
Terminal, build **`202607271`** (`20260727:48b764d`), on 2026-07-29. Where a
number is an estimate rather than a measurement, it says so.

---

## 0. Standing rules

These govern the work, not just describe it. Each earned its place by catching
something in this project.

1. **The smaller true number over the larger convenient one.** Report what is
   proven, not what is nearly proven. "33 intents attempted" is not "33
   appended"; a coverage figure keyed on raw rows is not one keyed on distinct
   contracts. When the convenient number is bigger, that is exactly when to
   distrust it.
2. **Unattended runs declare themselves.** A session left running unattended
   states that fact in its report, and unattended runs use
   `--max-cost-usd 0.00`. The next forensic then starts from *declared* rather
   than *discovered* — the expensive half of the 2026-07-29 03:47 investigation
   was only necessary because nothing had announced itself.
3. **Evidence, not conclusions.** Record the capture, then the reading of it.
   The vendor's startup banner said `Starting server at http://127.0.0.1:25503/`
   while `netstat` showed `0.0.0.0` — the banner was a conclusion, `netstat` was
   evidence, and only one of them was true.
4. **A test whose failure mode produces the desired answer is not evidence and
   is never banked.** A WSL probe printed `BLOCKED` because `bash` was missing,
   not because a port was filtered. Discarded. This shape recurs in validators:
   a reconciliation that returns "clean" on an empty response, or a dedup check
   that passes when parsing failed, is the same bug wearing different clothes.
5. **Third-state policy.** Before concluding a control works, rule out the
   states where it is present but inert — a firewall rule in the persistent but
   not the active store, a profile with local-rule merge disabled, a stopped
   service. "Configured" and "effective" are different claims.

---

## 1. Entitlements — from the Terminal, not the published tables

The Terminal logs the authoritative answer at startup. The published tier tables
disagree with it in places; the Terminal serves the bytes, so it wins.

```text
Subscriptions: Stock: STANDARD  Options: PROFESSIONAL  Index: FREE  Rate: FREE
Max concurrent requests: 8
```

| Asset class | Tier | Granularity | History floor | Notes |
|---|---|---|---|---|
| Options | **PROFESSIONAL** | tick | 2012-06-01 | all endpoints incl. 3rd-order greeks, `expiration=*` wildcard |
| Stocks | **STANDARD** | 1-minute | 2016-01-01 | tick is PRO-only; per-symbol tape gaps (see §3.2) |
| Index | **FREE** | — | — | **no access**; `/index/history/price` → 403 |
| Rates | **FREE** | — | — | no access |

- **Concurrency is 8 globally**, not per asset class. A stocks tranche running
  alongside options shares that budget rather than adding to it.
- The Index denial costs less than it appears: `greeks/*` responses carry
  `underlying_price`, so SPX/NDX/RUT/VIX levels arrive with the option data
  (§3.5).

### 1.1 Endpoint surface (probed, 400=exists / 404=absent / 403=unentitled)

Options `history`: `eod`, `ohlc`, `quote`, `trade`, `trade_quote`,
`open_interest`, `greeks/{eod,all,first_order,second_order,third_order,implied_volatility}`,
`trade_greeks/{...}`. Snapshot mirrors most of these.
Stocks `history`: `eod`, `ohlc`, `quote`, `trade`, `trade_quote`.
**Absent:** `stock/history/{splits,dividends}` (404 — "coming soon" in v3),
`option/list/{dates,contracts}`.

---

## 2. Tranche specs and the locked sequence

Roots: **SPY, QQQ, IWM, SPX, SPXW, NDX, VIX, RUT, DIA** (9). All nine are
entitled for `eod` and `open_interest`. **NDX is `eod`+OI-only** for the
historical span — its greeks begin ~2026-06 (§3.4).

| Tranche | Content | Depth | Est. size |
|---|---|---|---|
| **T0** | `greeks/eod` above each root's greeks floor; `eod` below it; `open_interest` always, all 9 roots | full 2012-06-01 → now | ~3–5 GB parquet, 2–3 h |
| **T0.5** | one liquid contract per root-day at `greeks/first_order`, `interval=1m` → a 1-minute underlying series for SPX/NDX/RUT/VIX | full span where greeks exist | trivial (~391 rows/root-day) |
| **T1** | `quote` @ `interval=1m`, `max_dte=45`, whole chain — SPX, SPXW, SPY, QQQ, IWM | full span | ~1.75 TB CSV → **ratio unmeasured** |
| **T2** | conditional on measured T0+T1 ≤ 900 GB: 1-min greeks/IV for SPX/SPXW at DTE ≤ 7; then trade OHLC + volume for the same near-dated sets | 2016 → now | measure-first per sub-tranche |

**T0 fetches both `eod` and `greeks/eod` above the floor** — not one instead of
the other. Row parity was disproven and then explained (§3.1); the pair is now a
per-day reconciliation, not redundancy.

### 2.1 Locked sequence

```text
five DECISIONS entries ─┐
golden-raw round-trip   ├─► T0 ─► T0 validation gate ─► T0.5 ─► measure
validator + controls    │         (coverage vs calendar,      remaining T1
inventory schema        │          distinct-contract          roots ─► T1
pinned schemas ─────────┤          accounting, 3 edges)
pacer constants         │
loopback gate ──────────┘
```

T1 additionally requires: measured compression ratio, measured SPX/QQQ/IWM
per-day sizes, off-host block proof, and Databento blitz reconciled to terminal
state. **T1 never runs while a Databento blitz tranche is downloading** — it is
a multi-day pull and must not share the pipe.

---

## 3. Vendor findings

Each of these can corrupt research silently. They are the reason the validator
exists in the shape it does.

### 3.1 `eod` duplicates every contract in older eras (D-0054)

The vendor ran multiple post-close build passes and serves **all** of them, so
`rows = 2 × distinct(expiration, strike, right)`.

| Date | rows | distinct | ratio | distinct `created` |
|---|---|---|---|---|
| SPY 2014-07-02 | 5,912 | 2,956 | 2.000 | 2 |
| SPY 2017-06-15 | 9,176 | 4,588 | 2.000 | 2 |
| QQQ 2017-06-15 | 5,680 | 2,840 | 2.000 | 2 |
| SPX 2017-06-15 | 5,524 | 2,762 | 2.000 | 2 |
| SPY 2019-01-02 | 14,040 | 7,020 | 2.000 | 2 |
| **SPY 2020-01-02** | 14,684 | 7,342 | 2.000 | **4** |
| SPY 2021-12-15 | 19,720 | 9,860 | 2.000 | 2 |
| SPY 2022-01-03 | 9,528 | 9,528 | **1.000** | 1 |
| SPY 2024-01-02 | 7,950 | 7,950 | 1.000 | 1 |

- **Boundary: duplicated through 2021-12-15, clean from 2022-01-03.** Same
  boundary on QQQ, so it is a vendor pipeline change at the 2021→2022 turn, not
  per-root.
- **2020-01-02 is the counterexample that forbids hardcoding two builds**: four
  distinct `created` values, every contract still appearing exactly twice —
  contracts are split across passes. Dedup groups by contract key and never
  assumes a file-level count.
- The paired rows are byte-identical in `last_trade`, OHLC, volume, count and
  the full NBBO; only `created` differs.
- `open_interest` is **not** affected (ratio 1.000 on 2014, 2017, 2019, 2024).
- `greeks/eod` is already deduplicated (ratio 1.000 wherever it exists).
- Worst consequence sits below the greeks floor, where `eod` is the only source
  and has no `greeks/eod` to reconcile against.

### 3.2 Zero is the vendor's "absent", never null

- `stock/history/ohlc` for **SPY 2016-01-04** returns HTTP 200 with 390 rows of
  `0.0,0.0,0.0,0.0,0,0`. QQQ on the same date returns real prices — so it is
  per-symbol, not per-date. CTA-tape symbols (incl. SPY) do not reach back to
  the 2016 stocks floor; QQQ does.
- `greeks/*` rows carry `underlying_price = 0.0` with `iv_error = 100` where the
  solve failed — seen at 09:30 but **treated as a condition, never a row
  position** (§4.3).

### 3.3 Unknown query parameters are ignored silently

`greeks=true` and `columns=all` on endpoints without them return HTTP 200 and
byte-identical columns. The response header is the only evidence of what was
actually requested — hence pinned schemas (§4.1).

Corollary, and it cost a probe to learn: an unknown *parameter* is silent, but a
bad *value* is loud. **v3 spells the sampling interval `interval=1m`.** Every
v2-era example uses milliseconds, and `interval=60000`, `interval=60` and
`interval=1min` all answer `400 Invalid interval`. Because only the value
announces itself, the only safe way to establish a parameter's spelling here is
to send a deliberately wrong value and see whether the vendor objects — a 200
proves nothing. `greeks/first_order` additionally refuses `expiration=*`
("Cannot specify '*' for the date") and must be asked for one expiration at a
time (D-0057).

### 3.4 History floors are per root **and** per endpoint

**Measured for all nine roots** by `crucible theta-floors`, 132 requests total,
each answer verified (the previous session answers 472, and three sessions
sampled above the boundary all carry data):

| Root | `eod` | `greeks/eod` | previous session |
|---|---|---|---|
| SPY | 2012-06-01 | **2017-01-03** | 2016-12-30 → 472 |
| IWM | 2012-06-01 | **2017-01-03** | 2016-12-30 → 472 |
| SPX | 2012-06-01 | **2017-01-03** | 2016-12-30 → 472 |
| SPXW | 2012-06-01 | **2017-01-03** | 2016-12-30 → 472 |
| VIX | 2012-06-01 | **2017-01-03** | 2016-12-30 → 472 |
| DIA | 2012-06-01 | **2017-01-03** | 2016-12-30 → 472 |
| RUT | 2012-06-01 | **2018-12-03** | 2018-11-30 → 472 |
| QQQ | 2012-06-01 | **≤ 2012-06-01** | at or below the subscription floor |
| NDX | 2012-06-01 | **2026-05-08** | 2026-05-07 → 472 |

**Six roots share one date to the day, and that is the finding.** It is not six
independent data availabilities; it is a **vendor pipeline switch-on at the
2016→2017 turn**, structurally the same kind of boundary as D-0054's 2021→2022
dedup change. Both are build-pipeline events rather than market events, which
means neither is a property of the instrument and neither will show up in
anything computed *from* the data — the only way to know is to have probed the
edge. It also corroborates the earlier hand probe that put SPY at "200 on
2017-01-03". QQQ and RUT are the exceptions, so the switch-on was not uniform,
and RUT's own 2018-12-03 is a second, smaller switch.

**Measured beats documented, and the discrepancy is recorded rather than
quietly overwritten.** This document and D-0053 both said NDX greeks began
**~2026-06** (from "empty 2026-05-01, 1.5 MB 2026-07-01"). Bisection puts it at
**2026-05-08**, verified — three weeks earlier, and inside the interval the
older probe had bracketed but not resolved. The measurement wins; the earlier
figure was an interpolation between two samples and never claimed otherwise.

A request below a floor answers **HTTP 472** ("No data found for your
request"), which is an ordinary outcome to record, not a failure to retry.

> **Bisect over sessions, never over calendar days.** The first version of the
> probe searched calendar days and reported SPY's floor as **2021-03-22** —
> wrong by four years, and self-confirming, because 2021-03-21 is a Sunday and
> answered 472 like any date below the floor. Weekends make the predicate
> non-monotone over days, so a midpoint landing on one drags the lower bound
> past years of real history. SPY returns 4,818 contracts on 2017-01-03. The
> probe now indexes into the sessions `us_equity_options` lists (D-0058), and
> verifies the answer instead of trusting the loop invariant.

### 3.5 `underlying_price` rides along at 1-minute

| Probe | rows | distinct `underlying_price` |
|---|---|---|
| SPXW 0DTE `greeks/first_order` 1m | 391 | **368** |
| NDX 1m | 391 | **387** |
| SPY 1m | 391 | 208 (penny increments) |

Genuinely intraday-varying, not an EOD value smeared across the day. This is the
basis of **T0.5**. The 09:30 row reads `0.0` on index roots and must be dropped
by the sentinel condition, not by position.

### 3.6 Other pinned facts

- **v2 is retired.** Every `/v2/*` path returns HTTP 410. All `bulk_hist`
  material is obsolete; bulk is now `expiration=*` wildcards.
- **`created` vs `timestamp`:** column 5 is `created` (build-run time) in `eod`
  but `timestamp` (per-contract update) in `greeks/eod`, and `eod` carries an
  extra `last_trade` before the OHLC block. `open` sits at index 6 in `eod` and
  index 5 in `greeks/eod`. Parsing is by **name**, never position.
- **`OI ⊆ eod`**: OI covers fewer contracts than `eod` on every sampled day
  (SPY 2014-07-02: 2,221 OI vs 2,956 distinct eod) — plausible, since OI rows
  exist only where interest does. Pinned as a reconciliation edge (§4.4).
- **Terminal build `202607271` ignores `config.toml`'s `host`** — banner only;
  it logs `127.0.0.1` while binding `0.0.0.0`. `host = "127.0.0.1"` is retained:
  harmless now, correct if a later build honours it. Reachability is controlled
  by a firewall rule instead (§6). Worth reporting upstream.
- **Rate limiting:** a `JettyRateLimiter` drops connections under sustained
  sequential whole-chain load; two probe runs died with `HTTP 000` while the
  process stayed alive. Concurrency 8 is not the only ceiling (§5).
- Index-option roots have their own symbology: `SPX → SPXW, SPXQ, SPXPM`,
  `RUT → RUTW, RUTQ`, `NDX → NDXP`, `VIX → VIXW`, `XSPA → XSP, XSPPM, XSPAM`.

---

## 4. Validator specification

Every clause below is merge-blocking and carries a planted-bug negative control.
A detector nobody has watched fire is decoration (CLAUDE.md §7).

### 4.1 Pinned schemas
Header validated **by name and in order** against a per-endpoint pin; any drift
— added, dropped, renamed, reordered — **refuses the response**. Never widen a
pin to make it parse. Re-pinning procedure: record the Terminal version,
re-fetch golden-raw fixtures, diff old vs new bodies, re-pin consciously, add a
one-line DECISIONS entry.

### 4.2 Dedup (D-0054)
- Group by `ContractKey(expiration, strike, right)`; keep **`max(created)`** —
  the final build pass, and the conservative `avail_ts` direction (D-0052).
- Record the **`n_builds` distribution** per file. Never assume 2.
- **Identical** pairs (market fields byte-equal): dedup silently, count recorded.
- **Conflicting** pairs (market fields differ across builds): keep-later still
  applies, but they are **counted and surfaced in the validation report** — a
  revision between builds is QA signal, not noise to swallow.
- **Same `(contract key, created)` twice**: a different bug entirely — **refuse
  the file**.
- Completeness accounting keys on **distinct contracts, never raw rows**.

### 4.3 Zero-sentinel
Condition, not position: refuse/drop rows where
`underlying_price == 0.0 || iv_error >= 100`.
`>=` not `==` deliberately: a fit whose error is at or beyond 100 % is unusable
whether it is the vendor's sentinel or a genuine divergence at 137. Does not
overfire — VIX rows carry `iv_error = 0.0021` and are ordinary data.
Separately, the **all-zero-OHLC file gate**: a series that is entirely zeros is
refused, not archived as a quiet day (SPY 2016-01-04).

### 4.4 The three reconciliation edges, per (root, day)

| Edge | Expected | Positive delta | Negative / inverted |
|---|---|---|---|
| `distinct(eod)` ↔ `rows(greeks/eod)` | **0** (verified: 4,588/4,588, 7,020/7,020, 2,840/2,840) | contracts lacking greeks → log as **coverage asymmetry**, not failure; the computed surface covers them | greeks holding contracts `eod` lacks is impossible under the established mechanism → **refuse the day** |
| `keys(OI)` ⊆ `keys(dedup(eod))` | subset; **coverage fraction recorded** | — | a contract with OI but no `eod` row inverts the mechanism → **refuse the day** |
| coverage vs calendar | every expected trading day present | — | missing/extra sessions reported |

Plus a `(contract, minute)` distinct check on T1 golden sample days.

### 4.5 Planted controls (each must be seen firing)
duplicate-contract row · `(key, created)` repeat · mid-day zero-underlying row ·
mid-day `iv_error = 100` row · all-zero OHLC series · `eod` body parsed as
`greeks/eod` (and reverse) · header drift in each of four directions · OI key
absent from `eod` · negative eod↔greeks delta · DST gap and ambiguous-hour
timestamps.

---

## 5. Pacer requirements

One **global** pacer shared across the JoinSet — never per-task:
semaphore (≤ 8, the Terminal's own figure) **plus a minimum launch interval**;
exponential backoff on connection-drop bursts, capped; honour `Retry-After` when
a 429 carries one; circuit-breaker after N consecutive drops → pause, resume by
inventory diff. Acquisition-side timing is not result-affecting, so none of this
touches determinism (§2.2).

**Constants, chosen and recorded** (D-0056, `external::thetadata::pacer`):

| Constant | Value | Why this value |
|---|---|---|
| `MAX_CONCURRENCY` | **8** | the Terminal's own startup figure, global across asset classes |
| `MIN_LAUNCH_INTERVAL` | **150 ms** | from the measured 0.3–2.7 s band: eight in flight sustain ~3–13 req/s on completion alone, and a 150 ms floor caps launches at 6.7/s — inside the band, so it never throttles the slow end and takes the peak off the fast end |
| `MAX_ATTEMPTS` | **4** | |
| `BACKOFF_BASE` / `BACKOFF_CAP` | **500 ms** doubling to **30 s** | |
| `MAX_RETRY_AFTER` | **120 s** | beyond this a `Retry-After` is a refusal to serve, not a nap: a pull parked for an hour is indistinguishable from a hung one |
| `CIRCUIT_TRIP_CONSECUTIVE` | **5** | consecutive, never cumulative — an hours-long pull will see scattered failures, and a cumulative count would eventually trip on bad luck alone |

Two behaviours worth stating because their opposites are the tempting default.
The breaker **fails the run** rather than pausing and retrying: resume is an
inventory diff, so stopping costs a re-run of what had not completed and nothing
for what had. And `Retry-After`'s HTTP-date form is **not** supported — parsing
it means trusting this machine's clock against the server's, and a skewed clock
computes a negative delay and hammers the endpoint that just asked for a pause.

`fetch_batch` no longer processes fixed-size chunks with a barrier between them.
That shape made every batch of eight pay for its slowest member, and against a
0.3–2.7 s spread it idled seven requests behind one for hours at a time.

**Revised T0 wall-clock.** T0 is ~2.5 requests per root-day over ~31,950
root-days ≈ **80,000 requests**. At eight in flight and a ~1.5 s mean that is
~5.3 req/s ≈ **4.2 h**, not the 2–3 h §2 estimates. The estimate stands
corrected rather than the pacer loosened: 150 ms is not the binding constraint
at that rate, the Terminal's response time is.

---

## 6. Storage, sampling, inventory

- **Cap 1.2 TB** for `external/thetadata/` total; **stop and report at 1.0 TB**;
  keep **≥ 400 GB free** at all times (blitz curated output, results and the
  registry still need room). `G:` had ~1.3 TB free at planning time.
- **Single copy**, no mirror — the subscription is the backup while live, and
  every file is re-fetchable by inventory diff.
- **`golden_raw/`**: the vendor's original response for **one sample day per
  year per (root, data-type)**, compressed as delivered, inventoried like
  everything else. Pins transcode fidelity permanently for a few GB.
- **Metadata gets redundancy even though the data does not**: at the end of every
  tranche, copy `inventory.jsonl` and the plan/measurement/validation docs to a
  second location. If `G:` dies, the inventory *is* the re-fetch script.
- Corruption: full blake3 verify at the end of every tranche; a mismatched file
  is deleted and re-fetched, never patched.

### 6.1 Layout and inventory

```text
external/thetadata/{options|stocks}/{root}/{type}/{grain}/…
external/thetadata/golden_raw/…
external/thetadata/inventory.jsonl
```

`inventory.jsonl` is append-only, LF-framed, with the same lock-and-reload
discipline as `manifest.jsonl` — and is **never merged into it** (D-0049:
different vendor, different trust). Resume is an **inventory diff**, never a
directory listing, so a half-written file cannot look complete.

Per-line fields (sketch): `schema_version`, `endpoint`, `root`, `grain`,
`start_date`, `end_date`, `request` (rendered path+query), `file_path`,
`file_blake3`, `size_bytes`, `row_count`, `distinct_contracts`, `dup_rate`,
`n_builds_distribution`, `conflicting_pairs`, `sentinel_rows_dropped`,
`reconciliation` (three edge results), `fetched_ts`.

Header/manifest-level fields: **Terminal version** (`202607271` — what makes pin
drift attributable), per-root **greeks floors**, per-era **dup rates**, and the
pacer constants in force.

---

## 7. Measurements

### 7.1 T0 costing — `greeks/eod`, 2024-01-02, CSV

| Root | `greeks/eod` | `eod` | OI | eod rows |
|---|---|---|---|---|
| SPXW | 4.30 MB | 1.84 | 0.88 | 14,045 |
| QQQ | 2.63 | 1.14 | 0.52 | 8,779 |
| SPY | 2.40 | 1.04 | 0.48 | 7,951 |
| SPX | 2.01 | 0.86 | 0.39 | 6,557 |
| NDX | (none pre-2026) | 0.84 | 0.38 | 6,414 |
| DIA | 1.09 | 0.47 | 0.22 | 3,649 |
| IWM | 1.35 | 0.58 | 0.26 | 4,521 |
| RUT | 0.41 | 0.18 | 0.08 | 1,351 |
| VIX | 0.31 | 0.14 | 0.06 | 1,059 |

≈ 14.5 MB CSV per day across 8 greeks-capable roots. Request timings
**0.3–2.7 s**.

### 7.2 T1 sizing — 1-min `quote`, `max_dte=45`, whole chain, CSV

| Root | 2013-01-02 | 2019-01-02 | 2025-01-02 |
|---|---|---|---|
| SPY | 28.4 MB / 346 k rows | 133.9 MB / 1.60 M | 103.3 MB / 1.26 M |
| SPXW | 17.4 MB / 209 k rows | 242.1 MB / 2.89 M | 320.8 MB / 3.85 M |

Throughput 12–20 MB/s per request.

**Era-integrated per-day totals across the five T1 roots** (SPY+SPXW measured;
SPX/QQQ/IWM extrapolated from `eod` size ratios — **not measured, must be
measured before T1 finalizes**):
~**100 MB/day** (2013) · ~**712 MB/day** (2019) · ~**746 MB/day** (2025).

Integrating over ~3,554 trading days by era:

```text
2012-06→2015-12  ~890 d  @ ~100 MB   →   ~89 GB
2016→2018        ~755 d  @ ~400 MB   →  ~302 GB
2019→2022       ~1006 d  @ ~700 MB   →  ~704 GB
2023→2026-07     ~903 d  @ ~750 MB   →  ~677 GB
                                        ≈ 1.75 TB CSV
```

A flat 460 MB/day × 3,554 d gives ~1.6 TB — close, because the dense years
dominate. An earlier 985 GB figure was **under-derived** (flat 0.6 factor on a
midpoint instead of integration) and is superseded.

### 7.3 Compression ratio — **MEASURED** (D-0057)

The ~10× assumption is retired. `crucible theta-golden` fetched one response per
endpoint (VIX, 2024-01-02), kept the vendor's bytes, transcoded to Parquet+zstd,
read the Parquet back and compared **6,221,352 cells individually** against the
source CSV. Every endpoint round-tripped.

| Endpoint | CSV | Parquet | **Ratio** | rows | cells verified |
|---|---|---|---|---|---|
| `option/history/quote` @1m | 34.41 MB | 1.85 MB | **18.61×** | 413,678 | 5,377,814 |
| `option/history/greeks/first_order` @1m | 6.72 MB | 579.4 kB | **11.88×** | 45,356 | 771,052 |
| `option/history/open_interest` | 62.5 kB | 10.8 kB | **5.79×** | 1,058 | 6,348 |
| `option/history/eod` | 139.3 kB | 32.5 kB | **4.28×** | 1,058 | 21,160 |
| `option/history/greeks/eod` | 312.8 kB | 127.1 kB | **2.46×** | 1,058 | 44,978 |

**There is no single ratio, and averaging them would be wrong in both
directions.** Quote rows are repetitive — long runs of unchanged NBBO,
dictionary-encoded `symbol`/`expiration`/`right`, delta-encoded timestamps — so
they compress about as hoped. `greeks/eod` is 44 high-entropy doubles per
contract with almost nothing to exploit, and compresses barely more than twice.
The endpoints T0 fetches are precisely the ones that compress **worst**.

Consequences, and they point opposite ways:

- **T1 gets cheaper.** ~1.75 TB CSV at 18.61× ≈ **~95 GB** parquet, against the
  175 GB the assumption implied. T1 fits the 1.2 TB cap with room to spare, and
  the compression measurement is no longer a scope risk.
- **T0 was under-estimated.** The "~3–5 GB parquet" figure below assumed the
  ~10× that does not hold here. Re-derived from §7.1's per-day CSV totals
  (`greeks/eod` 14.50 MB + `eod` 7.09 MB + OI 3.27 MB ≈ **24.9 MB/day** across
  the nine roots, at 2024 chain sizes) over ~3,554 trading days ≈ 88 GB CSV, at
  the measured ~3× blend for these three endpoints ≈ **~29 GB parquet**.
  That is an upper bound rather than a forecast: 2024 is the dense end of the
  sample, pre-2016 chains are far smaller, and below each root's greeks floor
  only `eod`+OI are fetched. Call it **10–30 GB**, an order of magnitude above
  the original estimate and still comfortably inside the cap.

Re-run the gate after any Terminal upgrade: the golden-raw responses are kept
permanently (§6) so the comparison is against the same bytes, and a transcode
that drifts shows up as a cell mismatch rather than as a plausible file.

---

## 8. Gate ledger

### CLOSED

| Gate | Evidence |
|---|---|
| Entitlements established | Terminal startup log; per-root probes, all 9 roots HTTP 200 |
| Endpoint surface | 400/404/403 existence probes; v2 → 410 |
| Eastern→UTC conversion | `calendar/eastern.rs`, 8 tests, hand-derived epoch arithmetic; DST-transition + day-before control (D-0052) |
| Pinned schemas | `external/thetadata/schema.rs`, 7 tests incl. the `created`/`timestamp` cross-parse control |
| `eod` duplication forensic | §3.1 table; mechanism, boundary, four-builds counterexample (D-0054) |
| OI not duplicated | ratio 1.000 on 2014/2017/2019/2024 |
| Loopback binding (gate 4, on-machine) | ActiveStore rule capture; `mpssvc` Running; `AllowLocalFirewallRules` NotConfigured (= local rules honoured); all three profiles Enabled |
| **Off-host block proof** | Rule enabled at test time (ActiveStore, Block, Inbound, Any, TCP 25503+25520). Phone Wi-Fi IP **10.100.102.96** (DHCP, no proxy) — in-subnet with 10.100.102.7, so on-link by routing table regardless of cellular state. Chrome: **`ERR_TIMED_OUT`**, both frames 15:07 local. Watch loop 15:02:49–15:07:50: **zero foreign connections**, self-address baseline recorded. Reading: `TIMED_OUT` not `CONNECTION_REFUSED` — the Terminal listens on `0.0.0.0`, so unfiltered would serve data and a closed port would RST into "refused"; a silent drop is the Block rule's signature. Absence became conclusive because the request is proven in-window. |
| Databento blitz reconciled *(to the attempted set)* | 26 intents complete `intended→submitted→downloaded→appended`; plan cross-check found `statistics` missing entirely (§9) |

| **Golden-raw round-trip + measured ratio** | `crucible theta-golden`, VIX 2024-01-02, five endpoints, **6,221,352 cells** compared individually against the source CSV; ratios in §7.3 (D-0057) |
| **Validator per §4 with planted controls** | `external/thetadata/validate.rs`; every detector paired with a planted bug that is asserted to fire — cross-parse both ways, header drift in four directions, `(contract, created)` repeat, repeated contract-minute, mid-day zero-underlying, `iv_error` at and beyond 100, all-zero series, inverted eod↔greeks, OI key absent from eod, DST gap and ambiguous hour |
| **`inventory.jsonl` schema** | `external/thetadata/inventory.rs`; append-only, resume by request diff, torn-final-line control |
| **Pacer constants** | §5, recorded and unit-tested (D-0056) |
| **Greeks floors, all nine roots** | `crucible theta-floors`, 132 requests, each verified against its previous session and three samples above it (§3.4, D-0057) |
| **Coverage vs calendar, edge 3** | `us_equity_options` (D-0058) + `TradingDayCalendar` (D-0059). **Authoritative** for SPY/QQQ/IWM/DIA, whose hours and dates are both sourced. **Sourced day-level** for SPX/SPXW/NDX/VIX/RUT: Cboe's published US-options holiday schedule enumerates the same closure and early-close set, verified against the 2026 schedule as published and inferred from the same rules for earlier years rather than re-verified page by page |

### OPEN

**OPEN DEFECT — blocks the session-hours layer, not T0.** `us_equity_options`
fires a phantom 13:00 early close on **2015-07-02, 2016-07-01, 2020-07-02,
2021-07-02, 2022-07-01, 2026-07-02**. The exchange closes early on 3 July only
when 4 July falls Tue-Fri, and `HolidayRule::WeekdayBefore` cannot express the
condition; the fix is a rule-engine extension carrying an anchor-weekday
predicate. Deferred deliberately, with a test asserting the current wrong
behaviour so a fix must consciously flip it. **Consequence: equity
`bars_per_year` and T1's intraday minute grids do not run over this table until
it is fixed.** `is_trading_day` is untouched, so day-level coverage and every
T0 reconciliation edge are unaffected (D-0059).

**Before T0** — the acquisition driver (`external::thetadata::plan`): tranche
expansion, resume by inventory diff, the free-disk guard, and the dry run T0
fires through. Nothing else.

**Before T1** — measured SPX/QQQ/IWM per-day sizes · Databento blitz at terminal
state (`statistics` still at `submitted`).

---

## 9. Interaction with the Databento blitz

Plan reconciliation (journal ∪ `BLITZ_CHECKLIST` §3) found that **the journal
witnesses only attempted work** — a tranche never submitted leaves no trace, so
an intent count measures attempts, not the plan.

| Plan item | Intents | State |
|---|---|---|
| `ohlcv-1m` × 7 | 9 (ES split ×3) | appended |
| `ohlcv-1s` × 7 | 7 | appended |
| `definition` × 7 | 7 | appended |
| `mbo`/`tbbo`/`trades` ES | 3 | appended |
| **`statistics` × 7** | 7 | **`submitted` — open** |

`statistics` had **no intent at all** until 2026-07-29; quoted at **$0.0000**
(5.42 GiB; metered would be $5.4168 at $1.00/GB — a flat-rate entitlement is
active). Submitted under `--max-cost-usd 0.00`, which refuses rather than bills
if the entitlement lapses.

**The blitz closing sentence is not yet written.** It reads "33 intents, every
plan item appended, zero gaps" only after `statistics` appends and `verify` +
`layout-check` are clean and `BLITZ_CHECKLIST` §3 is ticked — not on intent
count.
