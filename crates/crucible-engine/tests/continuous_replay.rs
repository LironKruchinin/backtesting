//! Replaying a stitched continuous series through the engine (D-0073).
//!
//! The one question this file exists to answer: **which of a continuous bar's
//! two price views reached which consumer?** Back-adjusted levels are for
//! signals; fills, marks and PnL use the tradeable price of the then-front
//! contract (D-0042). Nothing in the workspace can convert between them, so
//! what is left to prove is the *routing* — and routing is exactly the kind of
//! thing that looks right, passes every other test, and is wrong.
//!
//! The fixture is built so that a mistake in either direction is visible:
//!
//! ```text
//! ESH2024  days 0,1,2   every price 100      offset +20   adjusted 120
//!                       roll on day 2, gap = close(ESM2024) − close(ESH2024) = +20
//! ESM2024  days 3,4     every price 120      offset   0   adjusted 120
//! ```
//!
//! The adjusted series is flat at 120 across the roll and the tradeable one
//! steps 100 → 120. So an indicator fed the wrong series is not subtly off, it
//! is a different shape; and a PnL computed on the wrong series is off by
//! exactly the roll gap, which is the whole $1,000 of the round trip below.
//!
//! It also plants, and then measures, the thing this build does **not** model:
//! a position carried across a roll pays no spread and no fee, and books the
//! raw gap as PnL. See `a_position_carried_across_a_roll_books_the_raw_gap`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crucible_core::prelude::*;
use crucible_data::catalog::TsRange;
use crucible_data::continuous::{
    AdjustmentKind, ContinuousError, ContinuousFeed, ROLL_TABLE_SCHEMA_VERSION, RollRow, RollRule,
    RollTable,
};
use crucible_data::curated::{BarColumns, PartitionSource, write_partition};
use crucible_engine::{BacktestParams, FreeFills, run as run_backtest};
use crucible_strategies::indicators::Sma;

const MIN: i64 = 60_000_000_000;
const DAY: i64 = 86_400 * 1_000_000_000;
const NOON: i64 = 12 * 3_600_000_000_000;
const POINT: i64 = 1_000_000_000;

/// ES: quarter-point tick, $50 a point.
fn spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("ES.v.0"),
        tick: Price::from_nanos(250_000_000),
        point_value_usd: 50,
    }
}

// --------------------------------------------------------------- scaffolding

/// RAII temp dir, std-only (`tempfile` is not blessed — CLAUDE.md §6). Same
/// shape as `crucible-data`'s own, which is `pub(crate)` and so out of reach
/// from an integration test.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-contrep-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One flat bar per day at noon, so `avail_ts` (12:01) stays inside the same
/// UTC day and every OHLC field is the same number — nothing here depends on
/// intrabar path.
fn plant(dir: &TempDir, instrument: &str, start_day: i64, closes: &[i64]) {
    let mut bars = BarColumns::default();
    for (index, close) in closes.iter().enumerate() {
        #[expect(clippy::cast_possible_wrap, reason = "fixture indices are tiny")]
        let ts_open = (start_day + index as i64) * DAY + NOON;
        let nanos = close * POINT;
        bars.ts_open.push(ts_open);
        bars.open.push(nanos);
        bars.high.push(nanos);
        bars.low.push(nanos);
        bars.close.push(nanos);
        bars.volume.push(1);
    }
    write_partition(
        dir.path(),
        PartitionSource {
            instrument: InstrumentId::new(instrument),
            tf: TimeFrame::M1,
            dataset: "GLBX.MDP3".to_owned(),
            vendor_schema: "ohlcv-1m".to_owned(),
            source_file_path: format!("raw/GLBX.MDP3/ohlcv-1m/{instrument}/fixture.dbn.zst"),
            source_file_blake3: "00".repeat(32),
        },
        "fixture",
        &bars,
    )
    .expect("write partition")
    .expect("partition has rows");
}

/// `roll_ts` of the fixture's single roll: the availability of day 2's bars.
fn roll_ts() -> Ts {
    Ts(2 * DAY + NOON + MIN)
}

/// The fixture archive and its table. ESH2024 flat at 100 on days 0–2,
/// ESM2024 flat at 120 on days 2–4, one roll on day 2 with gap +20.
///
/// Both contracts trade on day 2, which is what makes the gap *observable* —
/// `close(ESM2024) − close(ESH2024) = 120 − 100 = +20` — and day 2's ESH2024
/// bar stays on the OLD contract, because the roll instant is the availability
/// of the session the decision was made from (D-0041).
fn fixture() -> (TempDir, RollTable) {
    let dir = TempDir::new();
    plant(&dir, "ESH2024", 0, &[100, 100, 100]);
    plant(&dir, "ESM2024", 2, &[120, 120, 120]);
    let table = RollTable {
        schema_version: ROLL_TABLE_SCHEMA_VERSION,
        root: "ES".to_owned(),
        tf: TimeFrame::M1,
        rule: RollRule::default(),
        decade_anchor: 2025,
        expiry_source: "none".to_owned(),
        first_ts_open: Ts(NOON),
        last_ts_open: Ts(4 * DAY + NOON),
        contracts: vec!["ESH2024".to_owned(), "ESM2024".to_owned()],
        sources: Vec::new(),
        rows: vec![RollRow {
            from_contract: "ESH2024".to_owned(),
            to_contract: "ESM2024".to_owned(),
            roll_ts: roll_ts(),
            adjustment: Price::from_points(20),
        }],
    };
    (dir, table)
}

fn open(dir: &TempDir, table: &RollTable) -> ContinuousFeed {
    ContinuousFeed::open(dir.path(), table, AdjustmentKind::BackAdjust, None).expect("open feed")
}

// ------------------------------------------------------------- probe strategy

/// What one bar looked like to the strategy, in both spaces at once.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Seen {
    avail_ts: Ts,
    tradeable_nanos: i64,
    signal_nanos: i64,
    sma2: Option<f64>,
}

/// Records every bar it is shown and trades on fixed bar indices.
///
/// Fixed indices rather than a signal, deliberately: this file is about which
/// prices reached whom, and a strategy whose *entries* depend on the prices
/// would make the PnL assertion depend on the routing twice over. The `Sma` is
/// carried along only so the indicator path is exercised through the real
/// trait, not reimplemented here.
struct Probe {
    seen: Vec<Seen>,
    sma: Sma,
    buy_on: usize,
    flatten_on: usize,
}

impl Probe {
    fn new(buy_on: usize, flatten_on: usize) -> Probe {
        Probe {
            seen: Vec::new(),
            sma: Sma::new(2),
            buy_on,
            flatten_on,
        }
    }
}

impl Strategy for Probe {
    fn warmup_bars(&self) -> usize {
        0
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        let MarketEvent::Bar(bar) = ev;
        let index = self.seen.len();
        self.seen.push(Seen {
            avail_ts: bar.avail_ts(),
            tradeable_nanos: bar.close.as_nanos(),
            signal_nanos: bar.signal_close().as_nanos(),
            sma2: self.sma.update(bar),
        });
        if index == self.buy_on {
            actions.target_position(view.position, Qty(1));
        } else if index == self.flatten_on {
            actions.target_position(view.position, Qty(0));
        }
    }
}

/// Runs `probe` over the fixture under `FreeFills` and returns it with the
/// result. `FreeFills` because this file is about price routing, not
/// execution cost: a half-spread on every fill would have to be subtracted
/// out of every hand-derived number below for no gain (D-0006 — screening
/// only, and a test fixture is the one place that is not a research claim).
fn replay(probe: Probe) -> (Probe, crucible_engine::BacktestResult) {
    let (dir, table) = fixture();
    let mut feed = open(&dir, &table);
    let mut probe = probe;
    let mut fills = FreeFills;
    let result = run_backtest(
        &mut feed,
        &mut probe,
        &mut fills,
        &spec(),
        &BacktestParams {
            initial_cash_nano_usd: 100_000 * 1_000_000_000,
            bars_per_year: 252.0,
        },
    )
    .expect("replay");
    (probe, result)
}

// ------------------------------------------------------------------ the tests

/// THE routing test, hand-derived.
///
/// Back-adjustment arithmetic (`continuous::adjust`): one roll with gap +20,
/// so there are two segments and
///
/// ```text
/// offset(ESM2024) = 0                    (the newest segment is untouched)
/// offset(ESH2024) = 0 + 20 = +20
/// ```
///
/// **Signal side.** ESH2024 traded at 100, so its signal close is
/// `100 + 20 = 120`; ESM2024 traded at 120 with offset 0, so its signal close
/// is `120`. The signal series is therefore flat at 120 across the roll, and
/// `Sma(2)` over it reads 120 on every bar from the second onwards. Over the
/// *tradeable* series the same `Sma(2)` would read 100, 100, 110, 120 — so if
/// the indicator were fed the wrong series this assertion fails on bar 3 by
/// ten points, not by a rounding error.
///
/// **Money side.** The probe buys on bar 1 and flattens on bar 3; under
/// `FreeFills` an order placed on bar *i* fills at bar *i+1*'s open, and an
/// order can never fill against the bar that triggered it (§2.1). So:
///
/// ```text
/// entry  fill at bar 2's open = 100 points   (ESH2024, the then-front contract)
/// exit   fill at bar 4's open = 120 points   (ESM2024, the then-front contract)
/// PnL    (120 − 100) points × 1 contract × $50/point = $1,000
/// equity $100,000 + $1,000 = $101,000
/// ```
///
/// Both fills are prices that really printed, on the contract that was really
/// front. The same round trip priced on the *adjusted* series would be
/// `120 − 120 = 0` points and $0 — a difference of exactly the roll gap, which
/// is asserted below so that a fixture which stopped exercising the difference
/// would fail rather than pass quietly.
#[test]
fn back_adjustment_reaches_the_indicator_and_never_the_pnl() {
    let (probe, result) = replay(Probe::new(1, 3));

    let tradeable: Vec<i64> = probe.seen.iter().map(|s| s.tradeable_nanos).collect();
    let signal: Vec<i64> = probe.seen.iter().map(|s| s.signal_nanos).collect();
    assert_eq!(
        tradeable,
        vec![
            100 * POINT,
            100 * POINT,
            100 * POINT,
            120 * POINT,
            120 * POINT
        ],
        "the strategy must see the prices that really printed"
    );
    assert_eq!(
        signal,
        vec![120 * POINT; 5],
        "back-adjustment must flatten the roll gap in signal space"
    );

    // The indicator consumed the signal series, so it never sees the step.
    let sma: Vec<Option<f64>> = probe.seen.iter().map(|s| s.sma2).collect();
    assert_eq!(sma[0], None, "a 2-bar SMA is not warm on bar 0");
    for (index, reading) in sma.iter().enumerate().skip(1) {
        let value = reading.expect("warm from bar 1");
        assert!(
            (value - 120.0).abs() < 1e-9,
            "bar {index}: SMA read {value}, so the indicator was fed the tradeable series"
        );
    }

    // And the money used the tradeable series.
    assert_eq!(result.summary.round_trips, 1);
    assert_eq!(
        result.summary.final_equity_nano_usd,
        101_000 * 1_000_000_000,
        "PnL must be the raw front-contract arithmetic: 120 − 100 points, $50/point"
    );
    assert_eq!(result.summary.fees_nano_usd, 0, "FreeFills charges nothing");

    // The companion that proves the fixture still exercises the difference:
    // the same trade priced on adjusted levels is worth nothing. Reaching that
    // number at all takes two explicit `as_nanos()` calls and a hand-built
    // `Price` — there is no conversion, and the loudness is the point (D-0042).
    let adjusted_delta = Price::from_nanos(signal[4] - signal[1]);
    assert_eq!(
        spec().pnl_nano_usd(adjusted_delta, 1),
        0,
        "if this ever equals the realized PnL the fixture has stopped testing anything"
    );
}

/// The planted defect, measured rather than described (CLAUDE.md §7).
///
/// Every ESH2024 bar in the fixture is 100 and every ESM2024 bar is 120, so
/// **no price moved inside either contract**. A round trip that opens before
/// the roll and closes after it should therefore be worth nothing, and it is
/// worth $1,000 — the roll gap, exactly.
///
/// That is not a bug in the routing above; it is the half of the roll story
/// this build does not model. A real roll is a *position* event: close the old
/// contract, open the new one, pay the spread and the fee twice. This engine
/// sees one instrument whose price stepped, so the step lands in PnL. The
/// omission is bounded — `Σ |gap| × point_value × qty` over the rolls a run
/// spans — and `crucible backtest` prints that bound with the result.
///
/// The test is here so the bound has something behind it: a number nobody has
/// watched appear is a comment.
#[test]
fn a_position_carried_across_a_roll_books_the_raw_gap() {
    let (_, result) = replay(Probe::new(1, 3));
    let gap_pnl = spec().pnl_nano_usd(Price::from_points(20), 1);
    assert_eq!(gap_pnl, 1_000 * 1_000_000_000);
    assert_eq!(
        result.summary.final_equity_nano_usd - 100_000 * 1_000_000_000,
        gap_pnl,
        "the whole round trip is the roll gap; nothing else moved"
    );

    // The control that names it: the same round trip entirely inside the OLD
    // contract (buy on bar 0, flatten on bar 1 — fills on bars 1 and 2, both
    // ESH2024 at 100) is worth zero, because a flat contract is flat.
    let (_, inside) = replay(Probe::new(0, 1));
    assert_eq!(inside.summary.round_trips, 1);
    assert_eq!(
        inside.summary.final_equity_nano_usd,
        100_000 * 1_000_000_000,
        "a round trip inside one contract of a flat series must be worth nothing"
    );
}

/// §2.1, at the seam a roll creates.
///
/// The new contract's prices must not be visible to the strategy until the
/// roll instant has passed — and "passed" is strict: the bars whose
/// availability *is* `roll_ts` are the bars the roll was decided from, so they
/// belong to the old contract (D-0041). Day 2's ESM2024 bar exists in the
/// archive and traded at 120; if the boundary were `>=` instead of `>`, the
/// strategy would see 120 one bar early, which on an adjusted chart is
/// invisible — the adjusted series is flat at 120 either way.
///
/// So the assertion is made on the **tradeable** series, where the step is
/// still there. That is the only place this particular lookahead is visible at
/// all, which is why the control is written against it.
#[test]
fn no_post_roll_price_is_visible_before_the_roll_instant() {
    let (probe, _) = replay(Probe::new(1, 3));
    let roll = roll_ts();

    for seen in &probe.seen {
        if seen.avail_ts <= roll {
            assert_eq!(
                seen.tradeable_nanos,
                100 * POINT,
                "a bar available at or before the roll instant ({roll}) showed the \
                 NEXT contract's price: {seen:?}"
            );
        } else {
            assert_eq!(
                seen.tradeable_nanos,
                120 * POINT,
                "a bar available after the roll instant still showed the old \
                 contract's price: {seen:?}"
            );
        }
    }

    // Stated the other way round, so a fixture where the boundary never gets
    // tested would fail: there IS a bar on each side of the instant, and the
    // first sight of the new contract's price is strictly after it.
    let first_new = probe
        .seen
        .iter()
        .find(|s| s.tradeable_nanos == 120 * POINT)
        .expect("the fixture must cross the roll");
    let last_old = probe
        .seen
        .iter()
        .rfind(|s| s.tradeable_nanos == 100 * POINT)
        .expect("the fixture must have pre-roll bars");
    assert!(first_new.avail_ts > roll);
    assert_eq!(
        last_old.avail_ts, roll,
        "the deciding session's own bar must stay on the old contract — that is \
         the bar the roll was computed from"
    );
}

/// D-0045's refusal, still made, now that something replays through it.
///
/// A window reaching outside the span the table was built from is refused
/// rather than trimmed: outside it the table has no front-contract answer, so
/// the series would be missing whole contracts and would look merely short of
/// data. `crucible backtest` narrows a *date* request to the covered span
/// before it gets here and prints that it did — this is the library check that
/// still fires whatever a caller hands it.
#[test]
fn a_window_outside_the_tables_span_is_still_refused() {
    let (dir, table) = fixture();
    let early = TsRange::new(Ts(-5 * DAY), Ts(3 * DAY + NOON)).expect("range");
    let err = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, Some(early))
        .expect_err("a window starting before the table's span must be refused");
    assert!(
        matches!(err, ContinuousError::RangeNotCovered { .. }),
        "{err}"
    );

    let late = TsRange::new(Ts(NOON), Ts(9 * DAY)).expect("range");
    let err = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, Some(late))
        .expect_err("a window ending after the table's span must be refused");
    assert!(
        matches!(err, ContinuousError::RangeNotCovered { .. }),
        "{err}"
    );

    // And the window that sits exactly inside is accepted, so the test above
    // is about coverage and not about the range machinery refusing everything.
    let inside = TsRange::new(Ts(NOON), Ts(4 * DAY + NOON + MIN)).expect("range");
    let feed = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, Some(inside))
        .expect("the table's own span must be replayable");
    assert_eq!(feed.len(), 5);
}

/// The narrowing `crucible backtest` does before it calls `open`, and its
/// limits.
///
/// `--start 2010-06-06` is a *date*; the archive's first ES bar opens at 22:00
/// that day. Refusing that is a refusal about the difference between a date
/// and an instant, not about missing contracts — so the CLI narrows the
/// request to what the table covers and prints that it did. What it must never
/// do is invent coverage: a request with no overlap at all still refuses.
#[test]
fn narrowing_clamps_an_overhang_and_refuses_a_disjoint_window() {
    let (_dir, table) = fixture();
    let covered = table.covered_range().expect("covered");
    assert_eq!(covered.start_ts(), Ts(NOON));
    assert_eq!(
        covered.end_ts(),
        Ts(4 * DAY + NOON + MIN),
        "the covered end is the last bar's availability, so a request for the \
         whole last bar is covered exactly"
    );

    // Overhanging on both sides: narrowed to the covered span.
    let asked = TsRange::new(Ts(-3 * DAY), Ts(20 * DAY)).expect("range");
    let narrowed = table
        .narrow(asked)
        .expect("valid table")
        .expect("overlaps the span");
    assert_eq!(narrowed, covered);

    // Entirely before the span: no overlap, and no answer.
    let disjoint = TsRange::new(Ts(-10 * DAY), Ts(-9 * DAY)).expect("range");
    assert!(
        table.narrow(disjoint).expect("valid table").is_none(),
        "a window that does not touch the table must not be narrowed into one that does"
    );

    // A window already inside is returned unchanged — narrowing must not move
    // a request that needed no moving.
    let inside = TsRange::new(Ts(DAY + NOON), Ts(3 * DAY + NOON)).expect("range");
    assert_eq!(
        table.narrow(inside).expect("valid table"),
        Some(inside),
        "narrowing must be the identity on a covered window"
    );
}
