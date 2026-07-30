# Session eras — what the archive says about when CME was open

*Measured 2026-07-30 against the fully transcoded archive: 863 instruments,
1,737 curated files, seven roots (ES, NQ, RTY, CL, GC, 6E, ZN), 2010-06-06 →
2026-07-28. The instrument is
`crates/crucible-data/examples/session_profile.rs`; every count below is
reproducible with the command printed beside it. Decision: D-0077.*

The governing principle, which is D-0040's and is why that entry exists: **the
archive is evidence, a spec page is a claim.** Where they disagree the archive
wins and the disagreement is written down with its counts. Where the archive
cannot settle a question, this file says so rather than picking an answer.

A second rule this workbook tries to keep: **a rule that cannot be falsified
from the archive is not thereby confirmed.** Each finding below is marked
`EVIDENCED` (the archive positively shows it) or `SURVIVES` (the archive is
consistent with it and could not have shown otherwise).

---

## 0. How the measurement works

Every bar with **nonzero volume** sets one bit in a `(local civil date, local
minute-of-day)` grid, in America/Chicago. Zero-volume bars are excluded: a
vendor may synthesise one, and a bar nobody traded in is not evidence the
market was open. Local *civil* date, never trading day — deriving a session
template cannot use a session template to bucket its own evidence.

Three derived views:

* `grid` — per year and weekday, the first and last minute presence reaches
  50 % of that weekday's trading dates, and every interior run of ≥ 5 minutes
  under 5 %. Session open, session close and any halt fall straight out.
* `window` — traded minutes inside a named local window, with runs of
  consecutive Mon–Fri dates that agree on whether **most** of the window
  traded. This is what locates an era boundary to the day.
* `holidays` — per weekday, the last traded minute before 17:00 CT (the *day*
  portion) and the first after it (the *evening*). A holiday changes them
  independently, which is what distinguishes an early close from a closure.

The 50 % / 5 % bands are deliberate: anything between them is printed as
`AMBIGUOUS` rather than resolved. Thin overnight minutes on ZN and 6E land
there routinely, and pretending otherwise would manufacture a session boundary
out of a liquidity fact.

---

## 1. The per-era findings table (equity index)

| era | span | open | close | halt | boundary evidence | table before D-0077 |
|---|---|---|---|---|---|---|
| 1 | 2010-06-06 .. 2012-11-16 | 15:30 CT on D−1 (17:00 for Monday) | 15:15 CT | 16:30–17:00 CT **on D−1** | 16:16–16:30 CT traded through 2012-11-15, then silent for 158 consecutive trading dates. Friday 2012-11-16 closed 15:15; Friday 2012-11-30 closed 16:15 | **wrong** — and still not modelled, see §1.1 |
| 2 | 2012-11-19 .. 2015-09-18 | 17:00 CT | 16:15 CT | 15:15–15:30 CT | 16:00–16:15 CT traded on all 78 Mon–Fri dates 2015-06-01..2015-09-18 and on none of the 200 from 2015-09-21 | **wrong** (`valid_from` excluded it) → now modelled |
| 3a | 2015-09-21 .. 2021-06-25 | 17:00 CT | 16:00 CT | 15:15–15:30 CT | 15:15–15:30 CT: 2,018 consecutive Mon–Fri trading dates averaging **0.04** traded minutes/date | **wrong** — the table had no halt → now modelled |
| 3b | 2021-06-28 .. 2026-07-28 | 17:00 CT | 16:00 CT | none | same window: **15.00** traded minutes/date on every one of 1,344 Mon–Fri dates | **right** |

Commands:

```text
session_profile grid   ES 1m 2010 2026
session_profile window ES 1m 2015-01-01 2026-07-29 15:15 15:30
session_profile window ES 1m 2010-06-01 2013-06-30 16:16 16:30
session_profile window ES 1m 2015-06-01 2016-06-30 16:00 16:15
session_profile daily  ES 1m 2010-06-01 2026-07-29 Fri
```

### 1.1 Era 1, and why it is documented rather than modelled

The 2010–2012 equity-index week, read off the weekly grid:

```
Sun   17:00–24:00
Mon   00:00–15:15   15:30–16:30   17:00–24:00
Tue   00:00–15:15   15:30–16:30   17:00–24:00
Wed   00:00–15:15   15:30–16:30   17:00–24:00
Thu   00:00–15:15   15:30–16:30   17:00–24:00
Fri   00:00–15:15   (nothing after)
```

That is a trading day `D` running **15:30 on D−1 → 15:15 on D**, with a halt at
**16:30–17:00 on D−1**, and with the D−1 block absent whenever D−1 is not
itself a trading day (which is why Friday has no post-close block: Monday's
session opens Sunday at 17:00).

The table's session template can express neither an open block anchored to the
evening *before* the trading day that is conditional on that evening trading,
nor a halt on D−1. Three options were considered:

| option | cost |
|---|---|
| model as open 17:00 / close 15:15 | ~30,000 ES bars/contract reported as *outside any session* over era 1 — the check that is supposed to indict the calendar, indicting it correctly |
| model as open 15:30 / close 15:15, halt 16:30–17:00 | Sunday 15:30–16:30 falsely open: ~60 phantom expected bars a week, reported as *missing data* — the error that blames the archive |
| do not model | `valid_from = 2012-11-19`; `qa` and `backtest` warn for any span starting earlier, as they already did |

The third was taken. **2.4 of the archive's 16.1 years are unmodelled, down
from 5.3 before D-0077.** Extending the template to a list of blocks with day
offsets and a "requires the previous day to be a trading day" flag is the fix
if era 1 ever matters; it is written down here so that whoever needs it does
not have to re-derive the shape.

Era 1's holidays closed at **10:30 CT**, on fourteen dates, all exactly 10:30:
2010-07-05, 2010-09-06, 2010-11-25, 2011-01-17, 2011-02-21, 2011-05-30,
2011-07-04, 2011-09-05, 2011-11-24, 2012-01-16, 2012-02-20, 2012-05-28,
2012-07-04, 2012-09-03. Not encoded, because era 1 is not modelled.

### 1.2 The D-0040 correction, corrected

D-0040 removed the 15:15–15:30 CT halt after finding **315 nonzero-volume ESH4
bars** inside it in January 2024 — 15 minutes on each of 21 trading days. That
measurement is still right. The conclusion drawn from it was too broad:
January 2024 is era 3b, and era 3b is the only era with no halt.

| span | Mon–Fri trading dates | traded minutes/date in 15:15–15:30 CT |
|---|---|---|
| 2015-01-01 .. 2021-06-25 | 2,018 | 0.04 |
| 2021-06-28 .. 2026-07-27 | 1,344 | 15.00 |

The 0.04 is a settlement print in the 15:15 minute on roughly one date in
twenty-five, not a session — the `holidays`/`window` split was rebuilt to
require **most of the window** to trade after a looser predicate mistook 28
such prints on 28 dates in mid-2018 for an era change that never happened.

CME's SER-8788R (2021-06-24) eliminated the 15:15–15:30 CT halt effective
**2021-06-28** for equity futures and options on CME and CBOT, describing it as
having been "initially implemented to account for transactions conducted via
open outcry". Archive and exchange agree to the day. This is the **third side**
(CLAUDE.md §7): the archive said 2024 had no halt, the spec page said there was
one, and the case that makes them agree — the era boundary — names the cause.

### 1.3 Holiday treatment changed twice, independently of the session eras

| span | MLK / Presidents' / Memorial / July 4 / Labor / Thanksgiving |
|---|---|
| era 1 | early close 10:30 CT |
| 2013-01-21 .. 2014-02-17 | **full closure** |
| 2014-05-26 onwards | early close 12:00 CT |

`EVIDENCED`. 2013-01-21 (MLK) has no day session *and* no session on the Sunday
evening before it: only 6 of the 8 Sundays between 2013-01-06 and 2013-02-24
opened, the two missing being the eves of MLK and Presidents' Day. That is a
`closed` trading day in the table's sense. Memorial Day 2014-05-26 closed at
12:00 CT and its Sunday eve opened (16:44–23:59), which is where the switch is.
MLK and Presidents' Day stayed closures a year longer than the other four.

### 1.4 Weekend observance, measured

| holiday landing on a Saturday | Friday before | rule |
|---|---|---|
| Christmas 2010-12-25, 2021-12-25 | **no session at all** | `nearest_weekday` |
| New Year 2011-01-01, 2022-01-01 | full session | `sunday_to_monday` |

`EVIDENCED`, twice each, and they differ — which is why the table now writes
them differently. Before D-0077 Christmas used `sunday_to_monday` and the
Christmas Eve rule gave 2021-12-24 a 12:15 CT close it did not have.

### 1.5 The day before Independence Day

`EVIDENCED` on eight dates: ES closed 12:15 CT on 2013-07-03, 2014-07-03,
2017-07-03, 2018-07-03, 2019-07-03, 2023-07-03, 2024-07-03, 2025-07-03 —
every year from 2013 where 4 July fell Tuesday–Friday, and no year where it did
not. 2012-07-04 was a Wednesday and 2012-07-03 traded in full, so the rule
starts in 2013 rather than being backdated over era 1.

This is the condition D-0059 recorded as inexpressible and deferred, in
`us_equity_options.toml` and, by the same argument, in `cme_globex.toml`.
`HolidayRule::WeekdayBefore` now carries `anchor_weekday`, both tables use it,
and the six phantom NYSE early closes D-0059 named (2015-07-02, 2016-07-01,
2020-07-02, 2021-07-02, 2022-07-01, 2026-07-02) are ordinary sessions again.

---

## 2. The four new tables

### 2.1 Session templates

| root | calendar | open | close | halt | era changes in 16 y |
|---|---|---|---|---|---|
| CL | `cme_globex_energy` | 17:00 CT | 16:00 CT (16:15 before 2015-09-21) | none | 1 |
| GC | `cme_globex_metals` | 17:00 CT | 16:00 CT (16:15 before 2015-09-21) | none | 1 |
| 6E | `cme_globex_fx` | 17:00 CT | 16:00 CT | none | **0** |
| ZN | `cme_globex_rates` | 17:00 CT (17:30 before 2011-10-02) | 16:00 CT | none | 1 |

`EVIDENCED` for every cell. The 2015-09-21 close change is the same advisory as
equity index and lands on the same date for CL, GC and NQ; the 16:00–16:15 CT
window traded on all 78 Mon–Fri dates from 2015-06-01 to 2015-09-18 and on none
of the 200 from 2015-09-21 to 2016-06-29, for each root separately. 6E and ZN
were already at 16:00 and the same query is silent across the whole span for
both, so the advisory did not touch them.

The ZN open moved on **Sunday 2011-10-02**: Sunday evenings first traded at
17:30 CT through 2011-09-25 and at 17:00 CT from 2011-10-02, with no mixed
week. No CME document for this change could be retrieved, so the boundary rests
on the archive alone and the table says so.

**No product other than equity index ever had the 15:15–15:30 CT halt**, in any
year. `EVIDENCED`: the window trades on essentially every session for all four.

### 2.2 One date, four answers

MLK Day **2022-01-17**, last traded minute before 17:00 CT:

| root | close | 
|---|---|
| ES | 12:00 CT |
| ZN | 12:00 CT |
| CL | 13:30 CT |
| GC | 13:30 CT |
| 6E | 15:58 CT — a full session |

This single date is the whole argument for four tables rather than one.

### 2.3 Recurring-holiday treatment

| root | MLK / Pres / Mem / Juneteenth / Jul 4 / Labor / Thanksgiving | day after Thanksgiving | Christmas Eve | Good Friday | Good Friday + NFP |
|---|---|---|---|---|---|
| ES | 12:00 CT (from 2014/2015; closures 2013–2014) | 12:15 CT | 12:15 CT | closed | **08:15 CT** |
| CL, GC | 12:00 CT to 2021, **13:30 CT** from 2022 | 12:45 → **13:45** from 2024 | 12:45 CT | closed | **closed** |
| 6E | 12:00 CT to 2021, **none** from 2022 | 12:15 → **13:45** from 2024 | 12:15 → 12:45 from 2024 | closed | **10:15 CT** |
| ZN | 12:00 CT throughout | 12:15 CT | 12:15 CT | closed | **10:15 CT** |

The NFP-Good-Friday row is `EVIDENCED` on 2015-04-03, 2021-04-02, 2023-04-07
and 2026-04-03 — four independent years, three different answers, every year
agreeing with the others.

### 2.4 One disagreement with CME's published hours, per table

The brief asks for one disagreement per new table or an explicit statement that
none was found. Honestly stated:

* **CL (`cme_globex_energy`) — no disagreement found.** CME publishes
  "Sunday–Friday 5:00 p.m.–4:00 p.m. CT with a 60-minute break each day
  beginning at 4:00 p.m. CT", and CME's own summary of the holiday schedule
  says energy and metals close at 1:30 p.m. CT on Monday holidays while
  interest-rate and equity futures halt at noon. The archive agrees with both,
  to the minute. What the published pages do **not** say is that the 13:30 close
  is a 2022 change — before it, energy closed at noon like everything else, and
  in 2013–early 2014 it did not open at all. That is an omission rather than a
  contradiction, and the table carries it with `first_year` / `last_year`.
* **GC (`cme_globex_metals`) — no disagreement found**, same sources, same
  agreement. The one thing CME does not publish and the archive does is that
  CL and GC agree date for date on every session boundary and every holiday
  across sixteen years, which is why the two tables differ only in their
  regular-hours window.
* **6E (`cme_globex_fx`) — DISAGREEMENT.** Every published summary of CME
  holiday hours puts FX with the rest of the exchange on a noon halt. The
  archive says FX stopped observing the US-holiday early close entirely in
  2022: MLK 2022-01-17 traded 6E to 15:58 CT on a day ES stopped at 12:00, and
  the same is true of every recurring holiday from 2022 to 2026 (four years,
  seven holidays a year). The archive wins.
* **ZN (`cme_globex_rates`) — DISAGREEMENT, and the expected one inverted.**
  The bond market observes Columbus Day and Veterans Day — SIFMA recommends a
  full close, and `docs/THETADATA_PLAN.md` §8.1 records Veterans Day as a day
  the NYSE trades and the bond market does not. **CBOT Treasury futures on
  Globex traded a full session on every Columbus Day and every Veterans Day in
  the archive.** Cash and futures are different markets; this table is about
  the futures and has neither holiday. The prior was checked and refuted rather
  than assumed.

### 2.5 What the four tables deliberately do not model

| gap | size | why |
|---|---|---|
| 6E and ZN closed 15:15 CT on the Friday before a Monday holiday, on 16 dates 2012-01-13 .. 2015-05-22 | 45 min × 16 | Identical date sets for both roots, absent for ES/CL/GC, stops after 2015-05-22, and includes the Friday before Columbus Day in 2012 **only**. No rule fits a pattern with one exception in it, and no CME document explaining it could be retrieved. Shows up as gaps in `qa` for ZN; outside 6E's `valid_from` |
| CL and GC close 12:00 CT rather than 13:30 when the holiday is a **Friday** | 90 min × 3 (2025-07-04, 2026-06-19, 2026-07-03) | Three for three, but `Effect::EarlyClose` carries one close time and cannot say "unless it is a Friday". Encoding 13:30 and listing the three is cheaper than a rule vocabulary extension for a 90-minute error |
| 6E and ZN closed 12:15 CT on 2010-12-31 while ES/CL/GC traded in full | one date | A single New Year's Eve, no other year in the archive shows it, outside both tables' `valid_from` |
| The 2025-11-28 Globex outage | one morning, all seven roots | The archive shows every root opening at 07:30 CT instead of 00:00. No CME notice retrievable; the same reason 2019-02-27 is unencoded |
| `rth_open_local` / `rth_close_local` for the four new tables | labels only | The **one** thing in these tables not measured from the archive. Open outcry ended for CL and GC on 2016-12-30 and CME publishes no RTH window for any of them; the values are the inherited floor hours, cited, and read only by `session_of`. Nothing in `open_intervals`, `is_open`, `is_trading_day` or `bars_per_year` touches them |
| The day-after-Thanksgiving 13:45 CT close (CL, GC, 6E) | 60 min × 2 so far | `EVIDENCED` on two years only, one of which (2025-11-28) is also the outage date, so it is really one clean observation. Encoded with `first_year = 2024` **and this sentence**, because a table wrong for the last two years is worse than one whose newest rule rests on a thin sample and says so |

---

## 3. Archive QA, by era and by root

`crucible qa` on the front contract of one mid-quarter month per era per root,
1-minute bars. Coverage is *bars present ÷ bars the calendar expects over the
span actually held*; the shortfall is overwhelmingly overnight minutes in which
nothing traded, which `ohlcv` does not emit a bar for (D-0039).

| era sample | ES | NQ | RTY | CL | GC | 6E | ZN |
|---|---|---|---|---|---|---|---|
| 2011-05 (before every equity `valid_from`) | 97.19 % | 86.71 % | — | 94.26 % | 92.56 % | 99.25 % | 86.19 % |
| 2014-05 (era 2) | 97.16 % | 90.02 % | — | 89.84 % | 93.11 % | 88.19 % | 81.73 % |
| 2018-05 (era 3a) | 99.72 % | 99.50 % | 80.24 % | 94.85 % | 95.05 % | 99.06 % | 91.09 % |
| 2024-05 (era 3b) | 99.90 % | 99.97 % | 94.32 % | 89.79 % | 94.16 % | 96.07 % | 88.87 % |

Coverage for the 2011-05 and 2014-05 rows is measured against a template that
does not describe those dates for ES/NQ (before `valid_from` 2012-11-19) or for
CL/GC/6E (before 2015-09-21); `qa` prints that warning and it is reproduced
here rather than quietly dropped.

### 3.0 The check that runs backwards: bars outside any session

`qa`'s second check indicts the *calendar*, not the archive (D-0040), so it is
the number to read first. Per run, `BARS OUTSIDE ANY SESSION`:

| era sample | ES | NQ | RTY | CL | GC | 6E | ZN |
|---|---|---|---|---|---|---|---|
| 2011-05 | 249 | 146 | — | 202 | 283 | 0 | 10 |
| 2014-05 | 2 | 1 | — | 188 | 269 | 1 | 1 |
| 2018-05 (era 3a) | 34 | 25 | 36 | 5 | 9 | 6 | 18 |
| 2024-05 (era 3b) | **0** | **0** | **0** | **0** | **0** | **0** | **0** |

Three different things, and the counts separate them:

* **Hundreds** — the 2011-05 and 2014-05 CL/GC/ES/NQ rows. These spans are
  before those tables' `valid_from`, so the calendar is answering with the
  wrong era: the bars sit at 21:00–21:15 UTC = 16:00–16:15 CT, exactly the
  quarter hour the older era traded and the modelled one does not. `qa` warns
  on every one of these runs. This is the warning working, not the table
  failing.
* **Five to thirty-six, at exactly two minutes of the day** — the 2018-05 row.
  Every one is stamped 20:15 UTC (15:15 CT, the first minute of the halt) or
  21:00 UTC (16:00 CT, the close minute). Across the whole of era 3a, ES has a
  print in the 15:15 minute on **71 of 1,496** trading dates and in the 16:00
  minute on **72 of 1,496** — under 5 % each, and the last 16:00 print is
  2018-08-31. A bar stamped 15:15 covers `[15:15, 15:16)` and the halt begins
  at 15:15:00, so a settlement trade at exactly that instant lands inside it.
  Moving the halt to 15:16 would manufacture a minute of session to absorb a
  boundary artifact, which is fitting the table to the noise; the count is
  recorded instead.
* **Zero, everywhere, in era 3b.** All seven roots, all seven calendars.

The shape is what distinguishes a real error from a boundary artifact:
D-0040's 315 was **15 minutes on each of 21 days**, systematic; this is **one
minute on one date in twenty**. Had an era boundary been placed a day wrong,
the first pattern is what would have appeared.

### 3.1 Three different things that look identical in a naive count

**(a) A session the exchange did not hold.** Every `NONE` on a recurring
holiday, plus the thirteen dates the vendor marks `missing` — 2026-02-14,
02-21, 02-28, 03-07, 03-14, 03-21, 03-28, 04-04, 04-11, 04-18, 04-25, 05-02,
05-09 — which are, every one of them, **Saturdays**. Not holes. A naive count
of "dates the vendor did not call available" is 41 and the honest one is 28.

**(b) A vendor-reported degraded day.** Every delivery folder carries a
`condition.json`, and the union across the thirty-three of them is these 28
dates marked `degraded`:

```
2014-06-11  2014-06-12  2014-06-13  2014-06-15
2014-09-22  2014-09-23  2014-09-24  2014-09-25
2014-12-31  2017-11-13  2018-10-21  2019-01-15
2019-02-22  2019-03-13  2019-03-26  2020-02-27
2020-02-28  2020-06-30  2020-07-01  2021-12-05
2022-01-02  2025-09-17  2025-09-24  2025-11-28
2026-03-15  2026-03-16  2026-04-10  2026-05-24
```

Every whole-day absence found independently from the bars is on this list:
ES/CL/GC/6E have no bars at all on 2014-06-13 and 2014-09-23..25; ES/GC/6E none
on 2014-12-31; ES/6E stop at 09:59 CT on 2020-02-28 while CL/GC/ZN have nothing
that day; all seven roots stop at 09:11 CT on 2020-06-30 and resume 19:00 CT.
The vendor said so first. These are **not holes in our pipeline** and a re-pull
would return the same bytes.

**(c) A genuine hole in our archive.** Exactly two, each one whole trading
session, each **reported `available` by the vendor**:

| root | window (UTC) | trading day | missing 1m bars |
|---|---|---|---|
| **GC** | `2012-09-11T22:00Z .. 2012-09-12T21:00Z` | 2012-09-12 | 1,380 |
| **ZN** | `2014-10-02T22:00Z .. 2014-10-03T21:00Z` | 2014-10-03 | 1,380 |

Both are root-wide (the measurement unions every contract of the root) and
neither has any counterpart in another root on the same date — ES traded
normally on 2012-09-12, and ZN's neighbours traded normally on 2014-10-03.
**Reported, not acquired**: a re-pull is serial, `--execute` with
`--max-cost-usd 0.00`, after `verify`, and it is the orchestrator's to
authorise.

### 3.2 Other findings, by kind

* **Zero-volume runs: none**, in every run of the sweep. `transcode` refuses a
  file over one bad record and `ohlcv` emits no bar for an untraded interval,
  so a zero-volume bar would be a vendor synthesis — there are none.
* **Impossible bars (OHLC self-contradiction): none.** Expected: `transcode`
  refuses the whole file over one.
* **Spikes:** single digits per month-long run, the largest an 8.1-sigma
  −3.00-point move on ESM2014 at 2014-06-17T12:31Z. Spikes are not counted as
  findings (`QaReport::is_clean`) because index futures move several robust
  sigmas at every RTH open.
* **DST boundaries:** none inside the one-month sweep windows, by construction.
  The structural claim — that a US transition never falls inside a Globex
  session, because it happens 02:00 local on a Sunday — is unchanged and is
  what makes the local→UTC conversion total.
* **A Saturday bar exists.** GC 2014, one date, 19:33–19:34 CT. One minute, on
  a Saturday, in sixteen years. Recorded, not chased.

---

## 4. What moved, and what did not

* `bars_per_year` for `cme_globex_equity_index` on 1-minute bars: **354,319 →
  353,963** (−0.10 %). The reference span moved from 2016-01-01..2026-01-01 to
  2022-01-01..2026-01-01 because the old one straddles the 2021-06-28 era
  boundary and would have averaged two different exchanges. Neither span
  contains the halt, so the change is entirely the holiday mix of a different
  set of years.
* The ESH2024 January-2024 reference run is **bit-identical**: 30,167 bars,
  −23.51 %, 665 round trips, $76,486.25, fees $1,663.75. Dollars do not touch
  the calendar.
* All three determinism hashes unchanged: `demo b55747513df596ed`,
  `combo 0e1ab52d474b862b`, `walk-forward 711e1cb34a2ee2b4`.
