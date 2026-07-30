//! Derives an exchange's session structure **from the bars**, not from a page.
//!
//! CLAUDE.md §9 records two findings that were earned this way: the
//! 15:15–15:30 CT halt CME's own contract-spec page lists and our archive
//! falsifies, and the "most CME holidays are early closes, not closures"
//! entry. Both came out of counting bars with nonzero volume inside a window a
//! published schedule called shut. This is the instrument that does that
//! counting, generalised, so the same argument can be made for a product that
//! has no session table yet and for an era the current table does not describe.
//!
//! ## The one structure everything is read off
//!
//! Every bar with **nonzero volume** sets one bit in a `(local civil date,
//! local minute-of-day)` grid, in the exchange's own timezone. Zero-volume
//! bars are excluded deliberately: a vendor may synthesise one, and a bar
//! nobody traded in is not evidence that the market was open. The grid is a
//! bitset — sixteen years of minutes is about a megabyte — and all five modes
//! below are queries over it.
//!
//! Local *civil* date, never trading day: deriving a session template cannot
//! use a session template to bucket its own evidence. A Sunday-evening open
//! therefore appears under Sunday, which is exactly what makes the weekly grid
//! legible.
//!
//! ## Modes
//!
//! ```text
//! cargo run -p crucible-data --release --example session_profile -- \
//!     grid     <ROOT> <TF> <FROM-YEAR> <TO-YEAR>
//! cargo run -p crucible-data --release --example session_profile -- \
//!     window   <ROOT> <TF> <FROM-DATE> <TO-DATE> <HH:MM> <HH:MM>
//! cargo run -p crucible-data --release --example session_profile -- \
//!     minutes  <ROOT> <TF> <FROM-DATE> <TO-DATE> <HH:MM> <HH:MM>
//! cargo run -p crucible-data --release --example session_profile -- \
//!     daily    <ROOT> <TF> <FROM-DATE> <TO-DATE> [WEEKDAY]
//! cargo run -p crucible-data --release --example session_profile -- \
//!     holidays <ROOT> <TF> <FROM-DATE> <TO-DATE> [HH:MM]
//! ```
//!
//! * `grid` — per calendar year and per weekday: the first and last minute the
//!   market is *usually* trading, and every interior window it is usually not.
//!   Reads the session open, the session close and any halt straight off.
//! * `window` — how many nonzero-volume bars fall in a named local window, on
//!   how many distinct dates, and the runs of consecutive Mon–Fri dates that
//!   agree on whether *most* of the window traded. The D-0040 argument as a
//!   command, and what locates an era boundary to the day.
//! * `minutes` — presence minute by minute across a window. A halt is a block
//!   of near-zeroes; a settlement print is one minute at 5 %.
//! * `daily` — the first and last traded minute of each local date, run-length
//!   encoded over consecutive dates that agree, optionally narrowed to one
//!   weekday. Friday is where a session *close* is legible, because Monday
//!   through Thursday run past local midnight.
//! * `holidays` — every weekday whose *day* session (before 17:00 CT) is absent
//!   or short, printed beside its *evening* session. A closure removes both; an
//!   early close removes neither.
//!
//! Read-only. Opens curated Parquet through the production [`ParquetBarFeed`]
//! and the timezone through the production [`Calendar`], so nothing here is a
//! second implementation of a rule it is auditing (the `sym_audit` argument).
//! Touches no inventory, no `raw/`, and writes nothing.
//!
//! [`ParquetBarFeed`]: crucible_data::ParquetBarFeed
//! [`Calendar`]: crucible_data::calendar::Calendar

use std::path::PathBuf;

use crucible_core::events::MarketEvent;
use crucible_core::traits::Feed;
use crucible_core::types::{InstrumentId, TimeFrame, Ts};
use crucible_data::ParquetBarFeed;
use crucible_data::calendar::Calendar;
use crucible_data::catalog::TsRange;
use crucible_data::continuous::parse_parts;
use crucible_data::curated::list_instruments;
use crucible_data::ingest::window::{
    CivilDate, civil_from_days, days_from_civil, parse_civil_date,
};

/// Minutes in a day — the grid's row length.
const MINUTES_PER_DAY: i64 = 1440;

/// Presence at or above this fraction of a weekday's dates reads as "open".
const OPEN_FRACTION: f64 = 0.50;

/// Presence at or below this fraction reads as "closed". Between the two is
/// reported as its own band rather than resolved: an instrument that trades on
/// a fifth of its sessions at 03:00 CT is not evidence either way, and saying
/// so is the point (CLAUDE.md §9 — refuse rather than guess).
const CLOSED_FRACTION: f64 = 0.05;

/// Shortest interior closed run worth printing, in minutes. One missing minute
/// on a thin contract is noise; a halt is at least a quarter of an hour.
const MIN_CLOSED_RUN: i64 = 5;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage:\n  \
         session_profile grid   <ROOT> <TF> <FROM-YEAR> <TO-YEAR>\n  \
         session_profile window  <ROOT> <TF> <FROM-DATE> <TO-DATE> <HH:MM> <HH:MM>\n  \
         session_profile minutes <ROOT> <TF> <FROM-DATE> <TO-DATE> <HH:MM> <HH:MM>\n  \
         session_profile daily   <ROOT> <TF> <FROM-DATE> <TO-DATE> [WEEKDAY]\n  \
         session_profile holidays <ROOT> <TF> <FROM-DATE> <TO-DATE> [HH:MM]";
    let Some(mode) = args.first().map(String::as_str) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };

    let root =
        PathBuf::from(std::env::var("CRUCIBLE_DATA_DIR").expect("CRUCIBLE_DATA_DIR must be set"));
    // Any Chicago calendar will do: the conversion asks the table for its
    // timezone, and every CME Globex product publishes its hours in CT. The
    // calendar's *session* claims are not consulted — that is the whole point,
    // because for CL, GC, 6E and ZN there are none to consult.
    let clock = Calendar::by_id("cme_globex_equity_index").expect("a bundled calendar must parse");

    match (mode, args.len()) {
        ("grid", 5) => {
            let (product, tf) = (args[1].clone(), parse_tf(&args[2]));
            let from = args[3].parse::<i64>().expect("FROM-YEAR must be a year");
            let to = args[4].parse::<i64>().expect("TO-YEAR must be a year");
            let start = CivilDate {
                year: from,
                month: 1,
                day: 1,
            };
            let end = CivilDate {
                year: to + 1,
                month: 1,
                day: 1,
            };
            let grid = Grid::build(&root, &clock, &product, tf, start, end);
            grid.report_weekly(&product, tf);
        }
        ("window", 7) => {
            let (product, tf) = (args[1].clone(), parse_tf(&args[2]));
            let start = date(&args[3]);
            let end = date(&args[4]);
            let (from_min, to_min) = (minute_of(&args[5]), minute_of(&args[6]));
            let grid = Grid::build(&root, &clock, &product, tf, start, end);
            grid.report_window(&product, tf, from_min, to_min, &args[5], &args[6]);
        }
        ("minutes", 7) => {
            let (product, tf) = (args[1].clone(), parse_tf(&args[2]));
            let start = date(&args[3]);
            let end = date(&args[4]);
            let (from_min, to_min) = (minute_of(&args[5]), minute_of(&args[6]));
            let grid = Grid::build(&root, &clock, &product, tf, start, end);
            grid.report_minutes(&product, tf, from_min, to_min);
        }
        ("holidays", 5 | 6) => {
            let (product, tf) = (args[1].clone(), parse_tf(&args[2]));
            let start = date(&args[3]);
            let end = date(&args[4]);
            let early_before = args.get(5).map_or(EARLY_CLOSE_BEFORE, |t| minute_of(t));
            let grid = Grid::build(&root, &clock, &product, tf, start, end);
            grid.report_holidays(&product, tf, early_before);
        }
        ("daily", 5 | 6) => {
            let (product, tf) = (args[1].clone(), parse_tf(&args[2]));
            let start = date(&args[3]);
            let end = date(&args[4]);
            let only_weekday = args.get(5).map(|name| {
                WEEKDAY_NAMES
                    .iter()
                    .position(|w| w.eq_ignore_ascii_case(name))
                    .unwrap_or_else(|| panic!("{name:?} is not one of {WEEKDAY_NAMES:?}"))
            });
            let grid = Grid::build(&root, &clock, &product, tf, start, end);
            grid.report_daily(&product, tf, only_weekday);
        }
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

fn parse_tf(text: &str) -> TimeFrame {
    text.parse::<TimeFrame>()
        .unwrap_or_else(|_| panic!("{text:?} is not a timeframe"))
}

fn date(text: &str) -> CivilDate {
    parse_civil_date(text).unwrap_or_else(|| panic!("{text:?} is not a YYYY-MM-DD date"))
}

fn minute_of(text: &str) -> i64 {
    let (h, m) = text.split_once(':').expect("a local time is HH:MM");
    h.parse::<i64>().expect("hour") * 60 + m.parse::<i64>().expect("minute")
}

/// 0 = Sunday … 6 = Saturday. 1970-01-01 was a Thursday.
fn weekday_index(day: i64) -> usize {
    usize::try_from((day + 4).rem_euclid(7)).unwrap_or(0)
}

const WEEKDAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The `(local date, local minute)` presence bitset, plus what produced it.
struct Grid {
    /// First local day, as a day index from the Unix epoch.
    first_day: i64,
    /// Number of local days covered.
    days: usize,
    /// One bit per `(day, minute)`, row-major.
    bits: Vec<u64>,
    /// Contracts whose partitions were read.
    contracts: usize,
    /// Bars read, before the zero-volume filter.
    bars_read: u64,
    /// Bars that carried volume, and so set a bit.
    bars_traded: u64,
}

impl Grid {
    fn build(
        data_dir: &std::path::Path,
        clock: &Calendar,
        product: &str,
        tf: TimeFrame,
        start: CivilDate,
        end: CivilDate,
    ) -> Grid {
        let first_day = days_from_civil(start);
        let last_day = days_from_civil(end);
        let days = usize::try_from(last_day - first_day).expect("the span must be positive");
        let cells = days * usize::try_from(MINUTES_PER_DAY).unwrap_or(1440);
        let mut grid = Grid {
            first_day,
            days,
            bits: vec![0u64; cells.div_ceil(64)],
            contracts: 0,
            bars_read: 0,
            bars_traded: 0,
        };

        // The local window is asked for in local time, but bars are filed in
        // UTC. Chicago is 5 or 6 hours behind, so widening the UTC request by a
        // day on each side cannot miss a local date and cannot include one
        // twice — the bitset is keyed on the local reading either way.
        let range = TsRange::new(
            Ts((first_day - 1) * 86_400_000_000_000),
            Ts((last_day + 1) * 86_400_000_000_000),
        )
        .expect("the widened span must be positive");

        let all = list_instruments(data_dir, tf).expect("the curated store must list");
        for name in all {
            // A whole root (`ES`) or one contract (`ESM2018`). The second form
            // is what turns "the archive shows trading in a window that should
            // be halted" into "*this contract* shows it", which is a different
            // finding.
            let is_root = parse_parts(&name).is_ok_and(|p| p.root == product);
            if name != product && !is_root {
                continue;
            }
            let instrument = InstrumentId::new(&name);
            let mut feed = match ParquetBarFeed::open(data_dir, &instrument, tf, Some(range)) {
                Ok(feed) => feed,
                // A contract with no bars inside the window is not a failure —
                // it is most of them, most of the time.
                Err(_) => continue,
            };
            grid.contracts += 1;
            while let Some(MarketEvent::Bar(bar)) = feed.next_event() {
                grid.bars_read += 1;
                if bar.volume == 0 {
                    continue;
                }
                let local = clock.local_reading(bar.ts_open);
                let day = days_from_civil(local.date);
                if day < first_day || day >= last_day {
                    continue;
                }
                grid.bars_traded += 1;
                let cell = (day - first_day) * MINUTES_PER_DAY + local.seconds_of_day / 60;
                let cell = usize::try_from(cell).unwrap_or(0);
                grid.bits[cell / 64] |= 1u64 << (cell % 64);
            }
        }
        grid
    }

    fn get(&self, day_offset: usize, minute: i64) -> bool {
        let cell = day_offset * usize::try_from(MINUTES_PER_DAY).unwrap_or(1440)
            + usize::try_from(minute).unwrap_or(0);
        self.bits[cell / 64] & (1u64 << (cell % 64)) != 0
    }

    /// True when the local date traded at all — the denominator for presence.
    fn traded_at_all(&self, day_offset: usize) -> bool {
        (0..MINUTES_PER_DAY).any(|m| self.get(day_offset, m))
    }

    fn header(&self, product: &str, tf: TimeFrame) {
        println!(
            "session profile — {product} {tf}   {} contract(s), {} bars read, \
             {} with volume",
            self.contracts, self.bars_read, self.bars_traded
        );
    }

    /// Per calendar year and weekday: where the market usually is, and is not.
    fn report_weekly(&self, product: &str, tf: TimeFrame) {
        self.header(product, tf);
        println!(
            "  presence = share of that weekday's TRADING dates with >=1 \
             nonzero-volume bar in the minute"
        );
        println!(
            "  open >= {:.0}%, closed <= {:.0}%, anything between is printed as \
             AMBIGUOUS",
            OPEN_FRACTION * 100.0,
            CLOSED_FRACTION * 100.0
        );

        let first_year = civil_from_days(self.first_day).year;
        let last_year =
            civil_from_days(self.first_day + i64::try_from(self.days).unwrap_or(0) - 1).year;
        for year in first_year..=last_year {
            // Denominators are per weekday, and count only dates that traded at
            // all: a Monday holiday must not drag every Monday minute's
            // presence down and make the session look shorter than it is.
            let mut denom = [0u32; 7];
            let mut present = vec![[0u32; 7]; usize::try_from(MINUTES_PER_DAY).unwrap_or(1440)];
            for offset in 0..self.days {
                let day = self.first_day + i64::try_from(offset).unwrap_or(0);
                if civil_from_days(day).year != year || !self.traded_at_all(offset) {
                    continue;
                }
                let w = weekday_index(day);
                denom[w] += 1;
                for minute in 0..MINUTES_PER_DAY {
                    if self.get(offset, minute) {
                        present[usize::try_from(minute).unwrap_or(0)][w] += 1;
                    }
                }
            }
            if denom.iter().sum::<u32>() == 0 {
                continue;
            }
            println!("\n  {year}");
            for w in 0..7 {
                if denom[w] == 0 {
                    continue;
                }
                let n = f64::from(denom[w]);
                let ratio =
                    |minute: i64| f64::from(present[usize::try_from(minute).unwrap_or(0)][w]) / n;
                let open: Vec<i64> = (0..MINUTES_PER_DAY)
                    .filter(|m| ratio(*m) >= OPEN_FRACTION)
                    .collect();
                let Some(&first) = open.first() else {
                    println!(
                        "    {} ({:3} dates)   no minute reaches {:.0}% presence",
                        WEEKDAY_NAMES[w],
                        denom[w],
                        OPEN_FRACTION * 100.0
                    );
                    continue;
                };
                let last = *open.last().unwrap_or(&first);
                print!(
                    "    {} ({:3} dates)   {} .. {}",
                    WEEKDAY_NAMES[w],
                    denom[w],
                    hhmm(first),
                    hhmm(last + 1)
                );
                // Interior windows the market is usually shut: the halt hunt.
                let mut interior = Vec::new();
                let mut run: Option<(i64, i64)> = None;
                for minute in first..=last {
                    if ratio(minute) <= CLOSED_FRACTION {
                        run = Some(match run {
                            Some((s, _)) => (s, minute),
                            None => (minute, minute),
                        });
                    } else if let Some((s, e)) = run.take()
                        && e - s + 1 >= MIN_CLOSED_RUN
                    {
                        interior.push((s, e));
                    }
                }
                if let Some((s, e)) = run
                    && e - s + 1 >= MIN_CLOSED_RUN
                {
                    interior.push((s, e));
                }
                for (s, e) in &interior {
                    print!("   CLOSED {}..{}", hhmm(*s), hhmm(*e + 1));
                }
                // And the band that is neither, inside the session.
                let ambiguous = (first..=last)
                    .filter(|m| {
                        let r = ratio(*m);
                        r > CLOSED_FRACTION && r < OPEN_FRACTION
                    })
                    .count();
                if ambiguous > 0 {
                    print!("   AMBIGUOUS {ambiguous} min");
                }
                println!();
            }
        }
    }

    /// The D-0040 count: bars with volume inside a window a schedule calls shut.
    fn report_window(
        &self,
        product: &str,
        tf: TimeFrame,
        from_min: i64,
        to_min: i64,
        from_text: &str,
        to_text: &str,
    ) {
        self.header(product, tf);
        let mut dates = 0u32;
        let mut minutes = 0u64;
        let mut trading_dates = 0u32;
        let mut first: Option<CivilDate> = None;
        let mut last: Option<CivilDate> = None;
        // Runs of consecutive trading dates that agree on whether the window
        // traded. This is what locates an era boundary to the day: the run
        // table has one row per side of the change and the dates are its edges.
        //
        // "Traded" means **most of the window** traded, not one minute of it.
        // The looser predicate mistook a single settlement print at 15:15 CT
        // for a session and reported an era change in May 2018 that never
        // happened: 28 minutes on 28 dates, exactly one a day.
        let width = to_min - from_min;
        let mut runs: Vec<(CivilDate, CivilDate, bool, u32, u64)> = Vec::new();
        for offset in 0..self.days {
            if !self.traded_at_all(offset) {
                continue;
            }
            // Mon–Fri only. A Sunday session opens at 17:00 CT, so no
            // afternoon window can trade on one; leaving Sundays in chops
            // every run of traded weekdays into fives and buries the boundary
            // the run table exists to show.
            let day = self.first_day + i64::try_from(offset).unwrap_or(0);
            if !(1..=5).contains(&weekday_index(day)) {
                continue;
            }
            trading_dates += 1;
            let hit = (from_min..to_min).filter(|m| self.get(offset, *m)).count();
            let d = civil_from_days(day);
            if hit > 0 {
                dates += 1;
                minutes += hit as u64;
                first.get_or_insert(d);
                last = Some(d);
            }
            let mostly = i64::try_from(hit).unwrap_or(0) * 2 >= width;
            match runs.last_mut() {
                Some((_, end, traded, n, sum)) if *traded == mostly => {
                    *end = d;
                    *n += 1;
                    *sum += hit as u64;
                }
                _ => runs.push((d, d, mostly, 1, hit as u64)),
            }
        }
        println!(
            "  local window {from_text}..{to_text}: {minutes} traded minute(s) on \
             {dates} of {trading_dates} trading date(s)"
        );
        if let (Some(f), Some(l)) = (first, last) {
            println!("  first {f}, last {l}");
        }
        // A weekend or a holiday is not evidence of an era change, so runs of
        // one or two dates are folded away from the summary and the long runs
        // — the ones an era boundary produces — are printed in full.
        println!("  runs of >= 5 consecutive trading dates agreeing (window is {width} min wide):");
        for (s, e, traded, n, sum) in &runs {
            if *n >= 5 {
                println!(
                    "    {s} .. {e}   {n:>4} date(s)   {}   {:.2} traded min/date",
                    if *traded { "TRADED" } else { "silent" },
                    per_date(*sum, *n)
                );
            }
        }
    }

    /// Presence, minute by minute, across one local window.
    ///
    /// The definitive form of a halt claim: a published schedule says these
    /// fifteen minutes are shut, and this says on what share of sessions each
    /// of them traded. A halt is a block of near-zeroes; a settlement print is
    /// one minute at 30 %.
    fn report_minutes(&self, product: &str, tf: TimeFrame, from_min: i64, to_min: i64) {
        self.header(product, tf);
        let mut denom = 0u32;
        let mut present = vec![0u32; usize::try_from(to_min - from_min).unwrap_or(0)];
        for offset in 0..self.days {
            if !self.traded_at_all(offset) {
                continue;
            }
            // A weekday-only denominator: Sunday's session starts at 17:00 CT,
            // so counting Sundays would dilute every afternoon minute by a
            // fifth for a reason that has nothing to do with a halt.
            let day = self.first_day + i64::try_from(offset).unwrap_or(0);
            if !(1..=5).contains(&weekday_index(day)) {
                continue;
            }
            denom += 1;
            for minute in from_min..to_min {
                if self.get(offset, minute) {
                    present[usize::try_from(minute - from_min).unwrap_or(0)] += 1;
                }
            }
        }
        println!("  {denom} Mon-Fri trading date(s); presence per local minute:");
        for (i, count) in present.iter().enumerate() {
            let minute = from_min + i64::try_from(i).unwrap_or(0);
            let pct = f64::from(*count) / f64::from(denom.max(1)) * 100.0;
            println!("    {}   {count:>6}   {pct:6.2} %", hhmm(minute));
        }
    }

    /// Every weekday the archive says was not an ordinary session.
    ///
    /// A local date is split at 17:00 CT into its **day** portion (the tail of
    /// the trading day that closes on it) and its **evening** portion (the head
    /// of the next one). Both are reported, because a holiday changes them
    /// independently: an early halt at 12:00 CT shortens the day and leaves the
    /// evening alone, while a full closure removes the day *and* the previous
    /// evening. Reading one without the other is how "Globex closed" and
    /// "Globex closed at noon" get confused, which is the CLAUDE.md §9 entry
    /// this mode exists to keep honest.
    fn report_holidays(&self, product: &str, tf: TimeFrame, early_before: i64) {
        self.header(product, tf);
        println!(
            "  weekdays whose DAY session is absent or ends before {}",
            hhmm(early_before)
        );
        println!("    date         day session          evening session");
        let mut flagged = 0u32;
        for offset in 0..self.days {
            let day = self.first_day + i64::try_from(offset).unwrap_or(0);
            if !(1..=5).contains(&weekday_index(day)) {
                continue;
            }
            let day_last = (0..EVENING_OPEN).rev().find(|m| self.get(offset, *m));
            let day_first = (0..EVENING_OPEN).find(|m| self.get(offset, *m));
            let eve_first = (EVENING_OPEN..MINUTES_PER_DAY).find(|m| self.get(offset, *m));
            let ordinary = day_last.is_some_and(|m| m + 1 >= early_before);
            if ordinary {
                continue;
            }
            flagged += 1;
            let d = civil_from_days(day);
            let day_text = match (day_first, day_last) {
                (Some(f), Some(l)) => format!("{} .. {}", hhmm(f), hhmm(l + 1)),
                _ => "NONE".to_owned(),
            };
            let eve_text = match eve_first {
                Some(f) => format!("opens {}", hhmm(f)),
                None => "NONE".to_owned(),
            };
            println!("    {d}   {:<18}   {eve_text}", day_text);
        }
        println!("  {flagged} weekday(s) flagged");
    }

    /// First and last traded minute per local date, run-length encoded.
    ///
    /// `only_weekday` narrows to one weekday, which is how a session *close*
    /// becomes legible: Monday through Thursday the session runs past local
    /// midnight, so their last traded minute is 23:59 in every era and says
    /// nothing. Friday's is the close.
    fn report_daily(&self, product: &str, tf: TimeFrame, only_weekday: Option<usize>) {
        self.header(product, tf);
        match only_weekday {
            Some(w) => println!(
                "  {} only — runs sharing the same first/last traded minute",
                WEEKDAY_NAMES[w]
            ),
            None => println!("  local date runs sharing the same first/last traded minute"),
        }
        let mut run: Option<(CivilDate, CivilDate, i64, i64, u32)> = None;
        for offset in 0..self.days {
            if !self.traded_at_all(offset) {
                continue;
            }
            let day = self.first_day + i64::try_from(offset).unwrap_or(0);
            if only_weekday.is_some_and(|w| weekday_index(day) != w) {
                continue;
            }
            let d = civil_from_days(day);
            let first = (0..MINUTES_PER_DAY)
                .find(|m| self.get(offset, *m))
                .unwrap_or(0);
            let last = (0..MINUTES_PER_DAY)
                .rev()
                .find(|m| self.get(offset, *m))
                .unwrap_or(0);
            match &mut run {
                Some((_, end, f, l, n)) if *f == first && *l == last => {
                    *end = d;
                    *n += 1;
                }
                _ => {
                    if let Some((s, e, f, l, n)) = run.take() {
                        println!(
                            "    {s} .. {e}   {:>3} date(s)   {} .. {}",
                            n,
                            hhmm(f),
                            hhmm(l + 1)
                        );
                    }
                    run = Some((d, d, first, last, 1));
                }
            }
        }
        if let Some((s, e, f, l, n)) = run {
            println!(
                "    {s} .. {e}   {:>3} date(s)   {} .. {}",
                n,
                hhmm(f),
                hhmm(l + 1)
            );
        }
    }
}

/// Local minute at which a Globex trading day's *day* portion is over and its
/// *evening* portion has not begun. 17:00 CT is the open in every era of every
/// product in this archive except CBOT rates before 2011-10-02.
const EVENING_OPEN: i64 = 17 * 60;

/// A day session that ends before this is not a normal close in any era —
/// the latest normal close observed anywhere here is 16:15 CT.
const EARLY_CLOSE_BEFORE: i64 = 15 * 60;

fn hhmm(minute: i64) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// Mean traded minutes per date. Diagnostic arithmetic, not accounting (§2.3).
#[expect(
    clippy::cast_precision_loss,
    reason = "a diagnostic average over a few thousand dates"
)]
fn per_date(sum: u64, dates: u32) -> f64 {
    sum as f64 / f64::from(dates.max(1))
}
