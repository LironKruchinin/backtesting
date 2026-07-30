# Data plan — what we buy, what we refuse, and why

This is the shopping list for the one-month Databento Standard blitz described
in D-0023. It exists so the decision of *what to acquire* is made calmly,
once, in advance — and not while a 30-day subscription clock is running.

Prices and sizes drift. Every figure here is a **quote taken on 2026-07-24**,
not an estimate, and every one of them is re-derivable for free:

```bash
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m --symbols ES.FUT \
       --start 2010-06-06 --end 2026-07-01 --window whole
```

A dry run is the default. It prices the plan, subtracts what the archive
already owns, and exits without spending.

---

## The basket

Seven CME parents on `GLBX.MDP3`, requested with **parent symbology**
(`--stype-in parent`), which resolves server-side to every listed contract:

| key | instrument | why it is in the basket |
|---|---|---|
| `ES.FUT` | E-mini S&P 500 | the primary research instrument; deepest book, tightest spread |
| `NQ.FUT` | E-mini Nasdaq 100 | correlated equity index — the cross-instrument "rhyme check" (M3) |
| `RTY.FUT` | E-mini Russell 2000 | third equity index; small-cap behaviour differs enough to be a real test |
| `CL.FUT` | WTI crude | uncorrelated with equities; different session and volatility regime |
| `6E.FUT` | Euro FX | FX leg; 23-hour session with a genuinely different intraday shape |
| `ZN.FUT` | 10-year T-Note | rates leg; the macro-overlay instrument (M4) |
| `GC.FUT` | Gold | metals leg; another uncorrelated regime |

Three equity indices are deliberate. A strategy that works on ES and dies on
NQ and RTY has told you something; a strategy tested only on ES has not been
tested at all. The four non-equity legs exist so "it rhymes across
instruments" cannot quietly mean "it rhymes across three tickers that are
94% the same trade".

## The buy list

| schema | range | symbols | entitlement tier | what it is for |
|---|---|---|---|---|
| `ohlcv-1m` | 2010-06-06 → today | all seven | L0, 16 y | the workhorse: every S0–S2 backtest runs on these |
| `ohlcv-1s` | 2010-06-06 → today | all seven | L0, 16 y | intrabar path resolution; stop/target ordering (M2) |
| `definition` | 2010-06-06 → today | all seven | L0, 16 y | contract specs, expiries, tick sizes — the roll table's input |
| `statistics` | 2010-06-06 → today | all seven | L0, 16 y | settlements and open interest; volume-roll signal |
| `trades` | last 12 months | `ES.FUT` | L1, rolling 12 mo | spread/slippage calibration (M4) |
| `tbbo` | last 12 months | `ES.FUT` | L1, rolling 12 mo | measured half-spread by time of day — replaces the hand-set 1 tick |
| `mbo` | last 1 month | `ES.FUT` | L3, rolling 1 mo | queue-position fill model prototype (M4) |

**The order matters and it is not the order above.** See
`docs/RUNBOOK_BLITZ.md`: `mbo`, `tbbo`, and `trades` sit in rolling windows
that slide forward every day, so they are acquired *first*. The 16-year L0
bars are not going anywhere.

Job granularity follows D-0028: `--window whole` for the backfill (one job per
contiguous gap, ~28 jobs), `--window month` for the recurring job.

### Cost shape, and the thing that is counter-intuitive

Unit prices are per billable GB, and **the aggregates are the expensive part**:

| $/GB | schemas |
|---|---|
| 0.50 | `mbp-10` |
| 1.00 | `statistics` |
| 1.70 | `definition` |
| 1.80 | `mbo`, `mbp-1` |
| 28.00 | `trades`, `tbbo` |
| 70.00 | `ohlcv-1s`, `ohlcv-1m` |
| 190.00 | `ohlcv-1h`, `ohlcv-1d` |

16 years of `ohlcv-1s` for ES+NQ+RTY alone is 19.67 GiB — **$1,377** metered,
more than every L1/L2/L3 tier on this list combined. One-second bars, not the
order book, are what costs money. Do not reason about this from the tier
names; the whole bootstrap list is ~53 GiB / ~$1,901 metered, which is why
D-0023 buys it inside one $199 subscription month instead.

---

## Do not buy

| item | why not |
|---|---|
| **OPRA** (options) | Options are out of scope until post-M4 (D-0010), and OPRA is the largest feed Databento carries. Buying it would dominate both the bill and the disk for data no milestone consumes. |
| **Equities subscription** | The only equity data this project wants is SPY + QQQ 1-minute bars as a sanity check. That is a metered micro-pull of a few dollars, not a subscription tier. |
| **`mbp-10`** | Strictly poorer than `mbo`, which we already buy: `mbo` is the full L3 book and `mbp-10` is a derivable aggregation of it. It is also 86.5 GiB/month against `mbo`'s 16.5 — five times the disk for a *view*, and ~1 TB/year against a 1.4 TB drive (D-0023). |
| **Bulk L2 history** | Same argument extended over years. The M4 queue model calibrates against one month of L3; buying years of L2 would be storage spent on a question nobody is asking. |
| **Pre-stitched continuous symbology** (`ES.v.0` et al.) | ~10% cheaper to download, and declined anyway: a roll rule we did not choose is a research assumption we cannot defend. Continuous series are constructed locally in `crucible-data::continuous`. |

## Free, manual: the Cboe VIX complex (`external/cboe/`)

The volatility complex is the one dataset in the whole plan that costs nothing.
Cboe publishes daily index history as plain CSV, so it does not go through
`pull`, does not appear in `manifest.jsonl`, and is not an acquisition. It gets
its own convention instead.

**Where it lives.** `$CRUCIBLE_DATA_DIR/external/cboe/` — a sibling of `raw/`
and `curated/`, for the same reason `staging/` and `delivery/` are: `raw/` is
exactly what D-0017 says it is, and nothing else may pretend to be an
acquisition.

**What to take**, all from
<https://www.cboe.com/tradable_products/vix/vix_historical_data/>:

| file | index | note |
|---|---|---|
| `VIX_History.csv` | VIX — 30-day implied vol | 1990→present |
| `VIX9D_History.csv` | 9-day | the short end of the term structure |
| `VIX3M_History.csv` | 3-month | from 2009-09-18 |
| `VIX1D_History.csv` | 1-day | **only from 2023**; before that, compute a 0–1DTE proxy |
| VX settlement history | VIX futures | from <https://www.cboe.com/us/futures/market_statistics/historical_data/> |

The daily-index files are all
`https://cdn.cboe.com/api/global/us_indices/daily_prices/{SYMBOL}_History.csv`
with header `DATE,OPEN,HIGH,LOW,CLOSE` (verified 2026-07-28 for `VIX3M`). That
is a pattern, not a promise — record the URL you actually used.

**No scraper.** D-0010 descoped scraping and that applies here: a scheduled
fetch of a vendor page is a thing that breaks silently in month four and
back-fills a research dataset with whatever the page said that day. These files
are downloaded by hand, dated, and left alone.

**The availability rule — the first design question, always (§2.1).** A daily
index value is knowable **at that session's close**, not at its open and not on
the morning of the date it is stamped with. So `avail_ts` for a Cboe daily row
is the close of the *same* trading day, 15:00 CT, which is
`crucible-data::calendar`'s job to supply. Any loader that stamps these rows at
midnight, or joins them to a futures bar by calendar date, has invented
lookahead worth roughly one session — enough on its own to make a
volatility-timing strategy look profitable.

**Every download carries a README.** Copy `docs/templates/external-cboe-README.md`
to `external/cboe/README.md` and fill it in: the exact URL, the date you
downloaded it, the row count, and the last date each file contains. Data whose
provenance is not written down is a rumour (§2.5), and a CSV in a folder is
exactly the kind of thing that acquires a false history six months later.

**The loader is not built yet**, deliberately. It arrives with the regime work
in the post-M4 backlog, at which point it reads these files, applies the
availability rule above, and is tested against a hand-checked fixture. Until
then the convention exists so the *files* can be collected correctly today.

## Later, metered, small

**SPY + QQQ 1-minute bars.** A few dollars, pay-as-you-go, no subscription.
The purpose is a cross-venue sanity check — if a futures result has no analogue
in the cash ETF, that is worth knowing before it reaches a write-up. Deferred
because the dataset choice (`XNAS.ITCH` vs a consolidated equities feed)
should be made when the question is actually being asked, not now.

---

## Caveats that survive into the research

These are properties of the data, not of the code, and they will still be true
in month four when the write-up needs a limitations section. Recorded here at
the moment they were learned rather than rediscovered later.

- **`mbo` does not exist before 2017-05-21.** CME introduced full order-event
  granularity (MBOFD) on that date. Earlier data is MDP 2, whose deepest
  schema is `mbp-10`. A 16-year `mbo` request is a request for data that does
  not exist; `plan()` clips it against the live per-schema range rather than
  quoting it.
- **Every MDP 2 record has `F_BAD_TS_RECV` set.** Pre-2017-05-21 data is
  sourced from FIX flat files with no capture timestamps, so `ts_recv` is set
  equal to `ts_event`. This is normal and expected, and it means any analysis
  that leans on the gap between event time and receive time is meaningless
  before 2017.
- **Timestamps are millisecond-resolution before 2015-11-20.** CME introduced
  nanosecond timestamps on that date. Sub-millisecond ordering claims about
  earlier data are not supported by the data.
- **Status data is sparse before November 2015.** Status changes outside the
  normal trading schedule did not generate messages, so session-boundary
  detection on early data must lean on the calendar, not on `status`.

The honest consequence: a 16-year backtest is not 16 years of homogeneous
data. It is ~9 years of nanosecond-timestamped MDP 3 and ~7 years of
millisecond-ish MDP 2, and any result whose sign depends on the early period
deserves a truncation test before it is believed.

---

## Operational record: the 2026-07-30 manifest outage

The worked example behind CLAUDE.md §8's **reader-first** rule. Kept because
the interesting thing about it is that nothing broke.

**What happened.** The D-0066 repair appended 8 symbol-supplement records — a
*new line kind* — to the live `manifest.jsonl` at **15:54**, while the code
that parses that line kind was still sitting unmerged in a worktree.
`ManifestRecord` carries `#[serde(deny_unknown_fields)]`, so from that instant
every command built from `main` refused the entire manifest at
`Catalog::open`: `transcode` exit 2, `layout-check` exit 2, and any `verify`
would have followed. Roughly **40 minutes** until the reader merged
(`bc04b1d`), after which `layout-check` returned exit 0 over all 33 records.

**What did not happen.** No byte of the archive was altered, no record was
rewritten, nothing was lost, and no result was computed from a
half-understood manifest. The append itself was measured as append-only: the
original 33 lines occupied exactly the 1,167,298 bytes they had occupied
before, with the new lines after them.

**The reading.** This is `deny_unknown_fields` doing its job. A permissive
parser would have skipped the eight unknown lines, reported coverage that
silently omitted 21,736 symbols, and let `coverage` re-buy them — the D-0033
re-buy bug, arriving quietly and looking like success. Instead the archive
refused to be read at all, loudly, everywhere, at once. **A total refusal is
the cheapest possible failure: it costs a merge, and it cannot be
mistaken for a result.** The fix is never to widen the parser; it is to land
the reader first.

**The rule, in one line:** any change to a persisted record shape lands its
READER on `main` before any writer emits the new shape.

## Operational caution: `G:` is an SMR drive

`G:` is a **Seagate ST4000DM004 — 4 TB SMR**, not CMR and not an SSD.
Measured cold sequential read: **131.6 MB/s at one stream, 69.5 MB/s at two**
— concurrency *costs* 47 % in aggregate, which is why bulk transcode runs at
two streams at most and pairs a large record with a small one.

The unmeasured hazard is writes, not reads. SMR drives absorb interleaved
small writes into a limited CMR cache and **collapse when it exhausts**, with
throughput falling by an order of magnitude until the drive destages. A bulk
`transcode` writes ~17.6 GB as tens of thousands of files with interleaved
row-group appends, which is the shape that exhausts it. **If write throughput
craters mid-transcode, suspect the drive before the code**: pause, let it
destage, resume. Curated output is derived and disposable, so a paused
transcode costs only its own re-run.
