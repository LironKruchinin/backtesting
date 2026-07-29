# Cboe volatility data — provenance

Copy this file to `$CRUCIBLE_DATA_DIR/external/cboe/README.md` and fill it in
**at the moment you download**, not afterwards. See `docs/DATA_PLAN.md` for why
this data is manual and what the availability rule is.

Nothing here is acquired through `crucible pull`, so nothing here is in
`manifest.jsonl` and `crucible verify` does not check it. This file is the only
provenance these bytes will ever have.

---

## Downloads

Downloaded by: `____________________`
Downloaded on (UTC date): `____-__-__`

| file | source URL | rows | first date | last date |
|---|---|---|---|---|
| `VIX_History.csv` | | | | |
| `VIX9D_History.csv` | | | | |
| `VIX3M_History.csv` | | | | |
| `VIX1D_History.csv` | | | | |
| `VX_settlements.csv` | | | | |

Landing pages these were reached from:

- Indices: <https://www.cboe.com/tradable_products/vix/vix_historical_data/>
- VX futures: <https://www.cboe.com/us/futures/market_statistics/historical_data/>

## Format as received

Record the header line of each file verbatim — Cboe has changed it before, and
a loader that guesses column order will silently transpose OHLC.

```
VIX_History.csv    : ____________________________________________
VIX9D_History.csv  : ____________________________________________
VIX3M_History.csv  : ____________________________________________
VIX1D_History.csv  : ____________________________________________
VX_settlements.csv : ____________________________________________
```

## Availability rule (do not change without a decision-log entry)

A daily index value is knowable **at the close of the session it is stamped
with** — 15:00 CT, per `crucible-data::calendar`. It is *not* knowable at that
session's open, and it is *not* a value for the following morning.

Consequences a loader must respect:

- `avail_ts` = close of the same trading day, from the calendar. Never
  midnight, never the file's date at 00:00 UTC.
- Never join a Cboe row to a futures bar on calendar date alone. Join on
  `avail_ts`, like everything else (CLAUDE.md §2.1).
- `VIX1D` exists only from 2023. A backtest spanning earlier dates must either
  start in 2023 or use an explicitly named proxy — not a forward-filled blank.

## Known caveats

- Cboe restates history occasionally. If you re-download, do **not** overwrite:
  add a dated folder and record both, so a result computed against the old file
  can still be reproduced (§2.5).
- The pre-2004 VIX archive is a separate `.xls` under a different URL and uses
  the old VXO methodology for part of its range. Treat 1990–2003 as a different
  series unless you have checked otherwise.
