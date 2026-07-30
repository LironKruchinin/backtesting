//! Resampler tests.
//!
//! Almost everything here drives a **hand-written fixture calendar** rather
//! than the bundled CME one, for `calendar`'s reason: the bundled table records
//! facts about a real exchange and will be corrected as sources are checked,
//! and a test suite that moves every time a holiday rule is fixed tests
//! nothing. The fixture is deliberately the same shape as CME's — a 17:00→16:00
//! overnight session in America/Chicago — so the arithmetic below is the
//! arithmetic that runs in production.
//!
//! Every expected instant is derived in a comment from CST (UTC−6) and CDT
//! (UTC−5) and the epoch constant `2024-01-01T00:00:00Z = 1_704_067_200`.

use super::*;
use crate::curated::meta::PartitionSource;
use crate::curated::write::write_partition;
use crate::testutil::TempDir;

// ------------------------------------------------------------------ fixtures

/// The production shape: overnight session, no halts, one dated early close.
const PLAIN: &str = r#"
schema_version = 1

[[calendar]]
id = "test_resample"
description = "17:00-16:00 overnight session, no halts, one early close"
timezone = "America/Chicago"
roots = ["ZZ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00"
close_local = "16:00"
rth_open_local = "08:30"
rth_close_local = "15:00"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2025-01-01"
rationale = "one calendar year"

[[calendar.one_off]]
date = "2024-07-03"
name = "Test early close"
effect = { kind = "early_close", close_local = "12:15" }
source = "hand-written test fixture"
"#;

/// The same session with an afternoon halt — the shape this module refuses.
const HALTED: &str = r#"
schema_version = 1

[[calendar]]
id = "test_halted"
description = "the same session with a 15:15-15:30 halt"
timezone = "America/Chicago"
roots = ["ZZ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00"
close_local = "16:00"
halt_local = [["15:15", "15:30"]]
rth_open_local = "08:30"
rth_close_local = "15:00"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2024-01-08"
rationale = "one week"
"#;

/// A session opening half a minute past the hour, so its anchor is not a whole
/// number of 1-minute intervals from the epoch.
const OFF_GRID: &str = r#"
schema_version = 1

[[calendar]]
id = "test_off_grid"
description = "a session opening at 17:00:30"
timezone = "America/Chicago"
roots = ["ZZ"]
valid_from = "2000-01-01"
sources = ["hand-written test fixture"]

[calendar.session]
open_local = "17:00:30"
close_local = "16:00"
rth_open_local = "08:30"
rth_close_local = "15:00"
source = "hand-written test fixture"

[calendar.reference_span]
start = "2024-01-01"
end = "2024-01-08"
rationale = "one week"
"#;

fn calendar_from(text: &'static str, id: &str) -> Calendar {
    Calendar::parse_table("fixture.toml", text)
        .expect("the fixture table parses")
        .into_iter()
        .find(|c| c.id() == id)
        .expect("the fixture declares that calendar")
}

fn plain() -> Calendar {
    calendar_from(PLAIN, "test_resample")
}

const SEC: i64 = 1_000_000_000;
const MIN: i64 = 60 * SEC;

/// `2024-01-01T00:00:00Z`.
const EPOCH_2024: i64 = 1_704_067_200 * SEC;
/// Trading day 2024-01-03 opens 2024-01-02 17:00 CST = 2024-01-02 23:00Z.
/// `2024-01-02T00:00:00Z` is `EPOCH_2024 + 86_400s`; add 23h.
const JAN3_OPEN: i64 = EPOCH_2024 + 86_400 * SEC + 23 * 3600 * SEC;
/// Trading day 2024-01-03 closes 2024-01-03 16:00 CST = 2024-01-03 22:00Z.
const JAN3_CLOSE: i64 = EPOCH_2024 + 2 * 86_400 * SEC + 22 * 3600 * SEC;
/// Trading day 2024-01-04 opens 2024-01-03 17:00 CST = 2024-01-03 23:00Z —
/// one hour after JAN3_CLOSE, and on the same UTC calendar date.
const JAN4_OPEN: i64 = EPOCH_2024 + 2 * 86_400 * SEC + 23 * 3600 * SEC;

/// Rows as `(ts_open, open, high, low, close, volume)`.
fn cols(rows: &[(i64, i64, i64, i64, i64, u64)]) -> BarColumns {
    BarColumns {
        ts_open: rows.iter().map(|r| r.0).collect(),
        open: rows.iter().map(|r| r.1).collect(),
        high: rows.iter().map(|r| r.2).collect(),
        low: rows.iter().map(|r| r.3).collect(),
        close: rows.iter().map(|r| r.4).collect(),
        volume: rows.iter().map(|r| r.5).collect(),
    }
}

/// One flat 1-minute bar per minute of `[start, end)`, each with volume 1 so a
/// resampled bar's volume **is** its constituent count.
fn minute_bars(start: i64, end: i64, price: i64) -> BarColumns {
    let mut bars = BarColumns::default();
    let mut ts = start;
    while ts < end {
        bars.ts_open.push(ts);
        bars.open.push(price);
        bars.high.push(price);
        bars.low.push(price);
        bars.close.push(price);
        bars.volume.push(1);
        ts += MIN;
    }
    bars
}

// ------------------------------------------------------------- aggregation

/// Hand-computed. Five 1-minute bars, all inside the first five minutes of
/// trading day 2024-01-03 (which opens at `JAN3_OPEN`, so bucket 0 of a
/// 5-minute grid is `[JAN3_OPEN, JAN3_OPEN + 300s)`):
///
/// | bar | open    | high    | low     | close   | volume |
/// |-----|---------|---------|---------|---------|--------|
/// | 0   | 5000.00 | 5000.75 | 4999.75 | 5000.50 | 10     |
/// | 1   | 5000.50 | 5002.25 | 5000.25 | 5002.00 | 20     |
/// | 2   | 5002.00 | 5002.50 | 4998.00 | 4998.25 | 30     |
/// | 3   | 4998.25 | 4999.00 | 4997.50 | 4998.75 | 40     |
/// | 4   | 4998.75 | 5001.00 | 4998.50 | 5000.25 | 50     |
///
/// open  = bar 0's open   = 5000.00
/// high  = max of column  = 5002.50 (bar 2)
/// low   = min of column  = 4997.50 (bar 3)
/// close = bar 4's close  = 5000.25
/// volume= 10+20+30+40+50 = 150
///
/// Every one of the five is a different bar, so no two fields can be
/// transposed without the test noticing.
#[test]
fn five_one_minute_bars_become_one_five_minute_bar() {
    let src = cols(&[
        (
            JAN3_OPEN,
            5_000_000_000_000,
            5_000_750_000_000,
            4_999_750_000_000,
            5_000_500_000_000,
            10,
        ),
        (
            JAN3_OPEN + MIN,
            5_000_500_000_000,
            5_002_250_000_000,
            5_000_250_000_000,
            5_002_000_000_000,
            20,
        ),
        (
            JAN3_OPEN + 2 * MIN,
            5_002_000_000_000,
            5_002_500_000_000,
            4_998_000_000_000,
            4_998_250_000_000,
            30,
        ),
        (
            JAN3_OPEN + 3 * MIN,
            4_998_250_000_000,
            4_999_000_000_000,
            4_997_500_000_000,
            4_998_750_000_000,
            40,
        ),
        (
            JAN3_OPEN + 4 * MIN,
            4_998_750_000_000,
            5_001_000_000_000,
            4_998_500_000_000,
            5_000_250_000_000,
            50,
        ),
    ]);

    let (out, report) =
        resample(&src, TimeFrame::M1, TimeFrame::M5, &plain()).expect("the fixture resamples");

    assert_eq!(out.len(), 1);
    assert_eq!(out.ts_open[0], JAN3_OPEN);
    assert_eq!(out.open[0], 5_000_000_000_000);
    assert_eq!(out.high[0], 5_002_500_000_000);
    assert_eq!(out.low[0], 4_997_500_000_000);
    assert_eq!(out.close[0], 5_000_250_000_000);
    assert_eq!(out.volume[0], 150);
    assert_eq!(report.source_bars, 5);
    assert_eq!(report.bars, 1);
    assert_eq!(report.trading_days, 1);
    assert_eq!(report.calendar_id, "test_resample");
    // The bucket opens exactly where the source does and closes exactly where
    // the fifth bar's interval does, so neither edge was cut.
    assert!(!report.first_bar_may_be_partial);
    assert!(!report.last_bar_may_be_partial);
}

/// The sixth minute opens the next bucket, and the first bucket keeps the
/// close it had — a bucket that leaked one bar forward would show 5001.00.
#[test]
fn a_bucket_closes_when_the_grid_advances() {
    let mut src = minute_bars(JAN3_OPEN, JAN3_OPEN + 5 * MIN, 5_000_000_000_000);
    src.ts_open.push(JAN3_OPEN + 5 * MIN);
    src.open.push(5_001_000_000_000);
    src.high.push(5_001_000_000_000);
    src.low.push(5_001_000_000_000);
    src.close.push(5_001_000_000_000);
    src.volume.push(7);

    let (out, _) = resample(&src, TimeFrame::M1, TimeFrame::M5, &plain()).expect("resamples");
    assert_eq!(out.ts_open, vec![JAN3_OPEN, JAN3_OPEN + 5 * MIN]);
    assert_eq!(out.close[0], 5_000_000_000_000);
    assert_eq!(out.volume, vec![5, 7]);
}

// -------------------------------------------------------- session boundaries

/// The headline. `JAN3_CLOSE` (22:00Z on the 3rd) and `JAN4_OPEN` (23:00Z on
/// the **same UTC date**) are one hour apart across the maintenance break, and
/// they belong to different trading days. A daily bar cut on UTC civil days
/// would hold both; one cut on sessions cannot.
#[test]
fn no_resampled_bar_spans_a_trading_day_boundary() {
    // Last two minutes of trading day 2024-01-03, then the first two of the
    // 4th's session, which opens an hour later on the same UTC date.
    let src = minute_bars(JAN3_CLOSE - 2 * MIN, JAN3_CLOSE, 5_000_000_000_000);
    let mut src = src;
    for bar in minute_bars(JAN4_OPEN, JAN4_OPEN + 2 * MIN, 5_100_000_000_000)
        .ts_open
        .iter()
        .copied()
        .enumerate()
    {
        let (_, ts) = bar;
        src.ts_open.push(ts);
        src.open.push(5_100_000_000_000);
        src.high.push(5_100_000_000_000);
        src.low.push(5_100_000_000_000);
        src.close.push(5_100_000_000_000);
        src.volume.push(1);
    }

    // The control that makes the assertion mean something: all four bars are
    // on the same UTC calendar date, so a UTC-day bucket would merge them.
    let utc_day = |ts: i64| ts.div_euclid(86_400 * SEC);
    assert_eq!(
        utc_day(src.ts_open[0]),
        utc_day(src.ts_open[3]),
        "the fixture must straddle a session boundary WITHOUT straddling a UTC day, \
         or it proves nothing"
    );

    let (out, report) = resample(&src, TimeFrame::M1, TimeFrame::D1, &plain()).expect("resamples");
    assert_eq!(out.len(), 2, "one daily bar per trading day, never one");
    assert_eq!(report.trading_days, 2);
    // Each daily bar opens when its own session opened.
    assert_eq!(out.ts_open, vec![JAN3_OPEN, JAN4_OPEN]);
    // And the prices did not cross the seam: the third's bar never saw 5100.
    assert_eq!(out.high[0], 5_000_000_000_000);
    assert_eq!(out.low[1], 5_100_000_000_000);
    assert_eq!(out.volume, vec![2, 2]);
}

/// A daily bar is a trading-day bar: it opens at the session open, not at
/// midnight and not at its first constituent.
#[test]
fn daily_bars_open_when_the_session_did() {
    // Start an hour into the session, so "opens at its first bar" would give a
    // visibly different answer from "opens at the session open".
    let src = minute_bars(
        JAN3_OPEN + 3600 * SEC,
        JAN3_OPEN + 3660 * SEC,
        5_000_000_000_000,
    );
    let (out, report) = resample(&src, TimeFrame::M1, TimeFrame::D1, &plain()).expect("resamples");
    assert_eq!(out.ts_open, vec![JAN3_OPEN]);
    assert!(
        report.first_bar_may_be_partial,
        "the bucket began an hour before the first source bar"
    );
}

// ------------------------------------------------------------- early closes

/// Trading day 2024-07-03 in the fixture closes early, at 12:15 CDT.
///
/// Opens 2024-07-02 17:00 CDT = 2024-07-02 22:00Z. `2024-07-02` is day 183 of
/// 2024 counting 2024-01-01 as day 0 (31+29+31+30+31+30 = 182 days to 1 July,
/// plus one), so the open is `EPOCH_2024 + 183 d + 22 h`.
///
/// Closes 2024-07-03 12:15 CDT = 2024-07-03 17:15Z = `open + 19 h 15 min`.
///
/// A one-hour resample therefore has 19 full buckets (k = 0..=18) and a
/// twentieth holding the 15 minutes from 12:00 to 12:15 CDT. With one unit of
/// volume per minute, the volumes read 60 × 19 then 15 — which is the bucket
/// boundary, the early close and the aggregation checked by one column.
#[test]
fn an_early_close_shortens_the_last_bucket() {
    const JUL3_OPEN: i64 = EPOCH_2024 + 183 * 86_400 * SEC + 22 * 3600 * SEC;
    const JUL3_CLOSE: i64 = JUL3_OPEN + (19 * 3600 + 15 * 60) * SEC;

    let cal = plain();
    assert_eq!(
        cal.session_close(CivilDate {
            year: 2024,
            month: 7,
            day: 3
        }),
        Ts(JUL3_CLOSE),
        "the fixture's one-off early close must be the 12:15 CDT one"
    );

    let src = minute_bars(JUL3_OPEN, JUL3_CLOSE, 5_000_000_000_000);
    assert_eq!(src.len(), (19 * 60) + 15);

    let (out, report) = resample(&src, TimeFrame::M1, TimeFrame::H1, &cal).expect("resamples");
    assert_eq!(out.len(), 20);
    assert_eq!(report.trading_days, 1);
    assert_eq!(out.ts_open[0], JUL3_OPEN);
    assert_eq!(out.ts_open[19], JUL3_OPEN + 19 * 3600 * SEC);
    let mut expected = vec![60u64; 19];
    expected.push(15);
    assert_eq!(out.volume, expected);

    // The last bucket runs past the close, but the *session* is what ended, not
    // the request — so this is not a truncation and must not be reported as
    // one. That distinction is the whole reason the flag is computed against
    // the session close rather than against the bucket alone.
    assert!(!report.last_bar_may_be_partial);
    assert!(!report.first_bar_may_be_partial);

    // The same day as one daily bar: every minute of it, opening at the open.
    let (daily, _) = resample(&src, TimeFrame::M1, TimeFrame::D1, &cal).expect("resamples");
    assert_eq!(daily.ts_open, vec![JUL3_OPEN]);
    assert_eq!(daily.volume, vec![(19 * 60) + 15]);
}

/// The other side of the same flag: when the source stops while the session is
/// still open, the last bar really was cut by the request.
#[test]
fn a_window_ending_mid_session_says_so() {
    let src = minute_bars(JAN3_OPEN, JAN3_OPEN + 90 * MIN, 5_000_000_000_000);
    let (out, report) = resample(&src, TimeFrame::M1, TimeFrame::H1, &plain()).expect("resamples");
    assert_eq!(out.len(), 2);
    assert_eq!(out.volume, vec![60, 30]);
    assert!(report.last_bar_may_be_partial);
    assert!(!report.first_bar_may_be_partial);
}

// ------------------------------------------------------------- §2.1 by hand

/// The invariant the whole bucket rule exists to keep: a resampled bar is never
/// knowable before any bar it was built from.
///
/// Checked over the early-close session, which is the one with a short bucket
/// and therefore the one where a boundary mistake would show.
#[test]
fn no_resampled_bar_is_available_before_its_constituents() {
    const JUL3_OPEN: i64 = EPOCH_2024 + 183 * 86_400 * SEC + 22 * 3600 * SEC;
    const JUL3_CLOSE: i64 = JUL3_OPEN + (19 * 3600 + 15 * 60) * SEC;
    let src = minute_bars(JUL3_OPEN, JUL3_CLOSE, 5_000_000_000_000);

    for target in [TimeFrame::M5, TimeFrame::M15, TimeFrame::H1, TimeFrame::D1] {
        let (out, _) = resample(&src, TimeFrame::M1, target, &plain()).expect("resamples");
        let mut i = 0usize;
        for b in 0..out.len() {
            let bucket_avail = out.ts_open[b] + target.duration_ns();
            let next_bucket = out.ts_open.get(b + 1).copied().unwrap_or(i64::MAX);
            while i < src.len() && src.ts_open[i] < next_bucket {
                assert!(
                    src.ts_open[i] + TimeFrame::M1.duration_ns() <= bucket_avail,
                    "{target}: constituent at {} outlives its bucket at {}",
                    src.ts_open[i],
                    out.ts_open[b]
                );
                i += 1;
            }
        }
        assert_eq!(
            i,
            src.len(),
            "{target}: every source bar must land somewhere"
        );
    }
}

// --------------------------------------------------------------- refusals

#[test]
fn a_calendar_declaring_a_halt_is_refused() {
    let halted = calendar_from(HALTED, "test_halted");
    let src = minute_bars(JAN3_OPEN, JAN3_OPEN + 5 * MIN, 5_000_000_000_000);
    let err = resample(&src, TimeFrame::M1, TimeFrame::M5, &halted)
        .expect_err("a halt is a session boundary this grid cannot see");
    assert!(
        matches!(err, ResampleError::CalendarDeclaresHalts { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("mix two sessions"));
}

#[test]
fn a_session_open_off_the_source_grid_is_refused() {
    let off = calendar_from(OFF_GRID, "test_off_grid");
    // 30 seconds past the hour, so the anchor is not a whole minute.
    let src = minute_bars(
        JAN3_OPEN + 60 * MIN,
        JAN3_OPEN + 65 * MIN,
        5_000_000_000_000,
    );
    let err = resample(&src, TimeFrame::M1, TimeFrame::M5, &off)
        .expect_err("bucket boundaries would fall inside source bars");
    assert!(
        matches!(err, ResampleError::SessionOpenOffGrid { .. }),
        "{err}"
    );
    // And the control: the same bars on a calendar that opens on the hour are
    // fine, so the refusal is the 30 seconds and not the fixture.
    assert!(resample(&src, TimeFrame::M1, TimeFrame::M5, &plain()).is_ok());
}

/// A source bar whose own interval ends exactly at the session open is
/// attributed to the day that is opening, but its content is from before it.
#[test]
fn a_bar_straddling_the_session_open_is_refused() {
    let src = minute_bars(JAN3_OPEN - MIN, JAN3_OPEN, 5_000_000_000_000);
    let err = resample(&src, TimeFrame::M1, TimeFrame::M5, &plain())
        .expect_err("the bar belongs to neither side of the boundary");
    assert!(
        matches!(err, ResampleError::BarPrecedesSessionOpen { .. }),
        "{err}"
    );
    // Control: one minute later, entirely inside the session, resamples.
    let ok = minute_bars(JAN3_OPEN, JAN3_OPEN + MIN, 5_000_000_000_000);
    assert!(resample(&ok, TimeFrame::M1, TimeFrame::M5, &plain()).is_ok());
}

#[test]
fn a_target_that_is_not_coarser_is_refused() {
    let src = minute_bars(JAN3_OPEN, JAN3_OPEN + 5 * MIN, 5_000_000_000_000);
    for target in [TimeFrame::M1, TimeFrame::S1] {
        let err = resample(&src, TimeFrame::M1, target, &plain())
            .expect_err("resampling only ever removes detail");
        assert!(matches!(err, ResampleError::NotCoarser { .. }), "{err}");
    }
}

/// [`ResampleError::Indivisible`] cannot fire for the timeframes this build
/// declares, and this is the test that says so out loud.
///
/// The availability proof in the module docs needs the target to be a whole
/// multiple of the source. Today every coarser pair divides, so the guard is
/// unreachable — which is exactly why it must be a guard and not an assumption:
/// the day someone adds a `4m` variant, this test fails and the refusal starts
/// firing, instead of a resampled bar quietly becoming visible one source
/// interval early.
#[test]
fn every_coarser_timeframe_pair_divides_today() {
    let all = [
        TimeFrame::S1,
        TimeFrame::M1,
        TimeFrame::M5,
        TimeFrame::M15,
        TimeFrame::H1,
        TimeFrame::D1,
    ];
    for source in all {
        for target in all {
            if target.duration_ns() > source.duration_ns() {
                assert_eq!(
                    target.duration_ns() % source.duration_ns(),
                    0,
                    "{target} is not a whole number of {source} intervals; \
                     ResampleError::Indivisible now has a case and needs its own test"
                );
            }
        }
    }
}

#[test]
fn out_of_order_source_bars_are_refused() {
    let mut src = minute_bars(JAN3_OPEN, JAN3_OPEN + 3 * MIN, 5_000_000_000_000);
    src.ts_open[2] = src.ts_open[1];
    let err = resample(&src, TimeFrame::M1, TimeFrame::M5, &plain())
        .expect_err("a repeated ts_open is a repeated bar");
    assert!(
        matches!(err, ResampleError::OutOfOrder { what: "source", .. }),
        "{err}"
    );
}

#[test]
fn volume_that_cannot_be_summed_is_refused() {
    let mut src = minute_bars(JAN3_OPEN, JAN3_OPEN + 2 * MIN, 5_000_000_000_000);
    src.volume = vec![u64::MAX, 1];
    let err = resample(&src, TimeFrame::M1, TimeFrame::M5, &plain())
        .expect_err("the source data is corrupt");
    assert!(matches!(err, ResampleError::VolumeOverflow { .. }), "{err}");
}

// ------------------------------------------------------------------- feed

fn source_for(instrument: &str) -> PartitionSource {
    PartitionSource {
        instrument: InstrumentId::new(instrument),
        tf: TimeFrame::M1,
        dataset: "GLBX.MDP3".to_owned(),
        vendor_schema: "ohlcv-1m".to_owned(),
        source_file_path: "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst".to_owned(),
        source_file_blake3: "2304cedfe99de7811ef2fa90cf9ff9f6a3bcfc0cca22ee134f4e031647e16ecc"
            .to_owned(),
    }
}

/// End to end through the curated store on the **bundled** CME calendar, which
/// opens at 17:00 CT exactly like the fixture — so the instants above are the
/// instants here.
#[test]
fn the_feed_aggregates_curated_partitions() {
    let dir = TempDir::new();
    let src = minute_bars(JAN3_OPEN, JAN3_OPEN + 15 * MIN, 5_000_000_000_000);
    write_partition(dir.path(), source_for("ESH2024"), "2024-01", &src)
        .expect("write")
        .expect("rows were written");

    let mut feed = ResampledBarFeed::open(
        dir.path(),
        &InstrumentId::new("ESH2024"),
        TimeFrame::M1,
        TimeFrame::M5,
        None,
    )
    .expect("open");

    assert_eq!(feed.len(), 3);
    assert_eq!(feed.tf(), TimeFrame::M5);
    assert_eq!(feed.report().calendar_id, "cme_globex_equity_index");
    assert_eq!(feed.report().source_bars, 15);
    // Provenance survives: a resampled bar's manifest id is its constituents'.
    assert_eq!(feed.sources().len(), 1);
    assert_eq!(
        feed.sources()[0].source_file_blake3,
        "2304cedfe99de7811ef2fa90cf9ff9f6a3bcfc0cca22ee134f4e031647e16ecc"
    );

    let events: Vec<MarketEvent> = std::iter::from_fn(|| feed.next_event()).collect();
    let MarketEvent::Bar(first) = &events[0];
    assert_eq!(first.tf, TimeFrame::M5);
    assert_eq!(first.ts_open, Ts(JAN3_OPEN));
    // avail_ts is derived and never stored: a 5m bar opening at t is knowable
    // at t + 300s (D-0003), which is when its fifth constituent was.
    assert_eq!(first.avail_ts(), Ts(JAN3_OPEN + 5 * MIN));
    assert_eq!(first.volume, 5);
    assert_eq!(first.signal_offset, Price::ZERO);
}

#[test]
fn an_instrument_no_calendar_governs_is_refused() {
    let dir = TempDir::new();
    let mut src = minute_bars(JAN3_OPEN, JAN3_OPEN + 5 * MIN, 5_000_000_000_000);
    src.volume = vec![1; 5];
    let mut synthetic = source_for("SYN:RW");
    synthetic.instrument = InstrumentId::new("SYN:RW");
    write_partition(dir.path(), synthetic, "2024-01", &src)
        .expect("write")
        .expect("rows were written");

    let err = ResampledBarFeed::open(
        dir.path(),
        &InstrumentId::new("SYN:RW"),
        TimeFrame::M1,
        TimeFrame::M5,
        None,
    )
    .expect_err("a synthetic feed has no exchange, so it has no sessions");
    assert!(matches!(err, ResampleError::NoCalendar { .. }), "{err}");
}
