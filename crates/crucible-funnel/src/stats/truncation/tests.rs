//! Truncation invariance: the two-sided control, converse first.
//!
//! `a_causal_strategy_is_truncation_invariant` must pass before
//! `a_full_sample_fit_breaks_truncation_invariance` means anything — a harness
//! that reports a divergence for every strategy has found nothing (CLAUDE.md
//! §7).

use super::*;
use crucible_engine::{BacktestParams, FreeFills, run};
use crucible_strategies::SmaCross;
use crucible_strategies::controls::LeakyZScore;

const SEED: u64 = 29;
const BARS: usize = 3_000;

fn spec() -> ContractSpec {
    ContractSpec {
        instrument: InstrumentId::new("SYN:RW"),
        tick: Price::from_points_f64_lossy(0.25),
        point_value_usd: 50,
    }
}

fn params() -> BacktestParams {
    BacktestParams {
        initial_cash_nano_usd: 100_000_000_000_000,
        bars_per_year: 347_760.0,
    }
}

/// The project's own null harness, materialized (D-0011).
fn events() -> Vec<MarketEvent> {
    let mut feed = crucible_data::SyntheticFeed::random_walk(
        SEED,
        BARS,
        TimeFrame::M1,
        Price::from_points(5000),
        spec().tick,
        4,
    );
    std::iter::from_fn(|| feed.next_event()).collect()
}

struct SliceFeed<'a> {
    events: &'a [MarketEvent],
    at: usize,
}

impl Feed for SliceFeed<'_> {
    fn next_event(&mut self) -> Option<MarketEvent> {
        let e = self.events.get(self.at)?.clone();
        self.at += 1;
        Some(e)
    }
}

/// Replays a strategy over a slice and returns what it decided.
fn decisions_of<S: Strategy>(events: &[MarketEvent], strategy: S) -> Vec<Decision> {
    let mut rec = Recording::new(strategy);
    let mut feed = SliceFeed { events, at: 0 };
    let mut fills = FreeFills;
    run(&mut feed, &mut rec, &mut fills, &spec(), &params())
        .expect("the synthetic feed is ordered");
    rec.into_decisions()
}

fn closes(events: &[MarketEvent]) -> Vec<Price> {
    events
        .iter()
        .map(|ev| {
            let MarketEvent::Bar(b) = ev;
            b.close
        })
        .collect()
}

/// Cut points spread across the series, none at the very start (nothing has
/// warmed up) and none at the very end (nothing has been deleted).
const CUTS: [usize; 4] = [1_200, 1_800, 2_400, 2_800];

// ---------------------------------------------------------------------------
// CONVERSE CONTROL FIRST
// ---------------------------------------------------------------------------

#[test]
fn a_causal_strategy_is_truncation_invariant() {
    // CONVERSE CONTROL. SmaCross reads a trailing window and nothing else, so
    // deleting the future cannot change a single decision it already made.
    // If this fails, the harness is broken and the leak result below proves
    // nothing.
    let events = events();
    let report = truncation_invariance(&events, &CUTS, |prefix| {
        decisions_of(prefix, SmaCross::new(20, 50, Qty(1)))
    });

    eprintln!(
        "[converse] causal strategy: {} decisions compared across {} cuts, {} divergence(s)",
        report.compared,
        report.cuts.len(),
        report.divergences.len()
    );
    for d in &report.divergences {
        eprintln!("           {}", d.detail);
    }
    assert!(
        report.holds(),
        "a causal strategy MUST be truncation-invariant. A harness that flags one \
         has found nothing when it later flags a leak."
    );
    assert!(
        report.compared > 0,
        "a sweep that compared nothing does not hold — it is an absent measurement"
    );
}

// ---------------------------------------------------------------------------
// THE DETECTOR
// ---------------------------------------------------------------------------

#[test]
fn a_full_sample_fit_breaks_truncation_invariance() {
    // THE DETECTOR. LeakyZScore standardizes against the whole series it is
    // handed, so a shorter series gives it a different mean and standard
    // deviation — and therefore different decisions on bars it had already
    // decided. That is information flowing backwards in time, made visible.
    let events = events();
    let report = truncation_invariance(&events, &CUTS, |prefix| {
        let leaky = LeakyZScore::fit_on_everything(&closes(prefix), 0.5, Qty(1));
        decisions_of(prefix, leaky)
    });

    eprintln!(
        "[detector] full-sample fit: {} decisions compared, {} divergence(s)",
        report.compared,
        report.divergences.len()
    );
    if let Some(d) = report.divergences.first() {
        eprintln!(
            "           first at cut {} / avail_ts {}",
            d.cut, d.avail_ts
        );
        eprintln!("           {}", d.detail);
    }
    assert!(
        !report.holds(),
        "the full-sample fit MUST break truncation invariance — its decisions depend \
         on data that had not happened when it made them"
    );
    assert!(
        !report.divergences.is_empty(),
        "and the harness must name where, not merely report a boolean"
    );
}

/// The third case: the difference above is the *fit*, not the strategy family.
/// A z-score of the same shape computed on a **trailing** window is causal and
/// passes, so what the detector caught is the full-sample fit and nothing else
/// (§7's "add the third case that makes them agree").
#[test]
fn the_same_strategy_shape_fitted_causally_passes() {
    let events = events();
    let report = truncation_invariance(&events, &CUTS, |prefix| {
        // The causal counterpart: refit on a fixed-length trailing window that
        // ends well before the earliest cut, so every run sees an IDENTICAL
        // fit and only the future differs.
        let window = &closes(prefix)[..1_000];
        decisions_of(prefix, LeakyZScore::fit_on_everything(window, 0.5, Qty(1)))
    });
    eprintln!(
        "[third]    identical fit, future deleted: {} compared, {} divergence(s)",
        report.compared,
        report.divergences.len()
    );
    assert!(
        report.holds(),
        "with the fit held fixed, deleting the future changes nothing — so the \
         detector's finding was the fit, not the strategy"
    );
}

/// A **content-only** leak: it decides on exactly the same bars whatever series
/// it is given, but the SIZE it trades is a function of the whole series.
///
/// This exists because a mutation found a hole. Dropping `bytes` from the
/// comparison — leaving only the timestamps — left every test above green,
/// which meant the detector was firing on *when* the leaky strategy decided and
/// never on *what* it decided. A leak that changed size or side without
/// changing timing would have walked through. This fixture closes that.
struct SizeFromFullSample {
    /// Quantity derived from the length of the series it was built for — the
    /// leak, in its smallest honest form.
    qty: Qty,
    seen: usize,
}

impl SizeFromFullSample {
    fn new(series_len: usize) -> SizeFromFullSample {
        // 1 or 2 contracts depending on the whole series. Nothing about bar
        // 600 should depend on whether bar 2,900 exists.
        let qty = if series_len > 2_000 { 2 } else { 1 };
        SizeFromFullSample {
            qty: Qty(qty),
            seen: 0,
        }
    }
}

impl Strategy for SizeFromFullSample {
    fn warmup_bars(&self) -> usize {
        0
    }

    fn on_event(&mut self, _ev: &MarketEvent, _view: &PortfolioView, actions: &mut Actions) {
        self.seen += 1;
        // A fixed schedule, so the TIMESTAMPS are identical in every run and
        // only the contents can differ.
        if self.seen.is_multiple_of(500) {
            actions.buy(self.qty);
        }
    }
}

#[test]
fn a_content_only_leak_is_caught_because_decisions_are_compared_byte_for_byte() {
    // The decisions land on identical bars in every run; only the quantity
    // moves. Timestamp comparison alone cannot see this, which is precisely
    // why the comparison is bit-identity of the encoded intents.
    let events = events();
    let report = truncation_invariance(&events, &CUTS, |prefix| {
        decisions_of(prefix, SizeFromFullSample::new(prefix.len()))
    });

    eprintln!(
        "[content]  same bars, different size: {} compared, {} divergence(s)",
        report.compared,
        report.divergences.len()
    );
    if let Some(d) = report.divergences.first() {
        eprintln!("           {}", d.detail);
    }
    assert!(
        !report.holds(),
        "a leak that changes WHAT it trades without changing WHEN must still be          caught — otherwise the byte comparison is decoration and only timestamps          are doing work"
    );
    // And the divergence must be at a timestamp BOTH runs decided on, which is
    // what makes this a content finding rather than a timing one.
    let d = &report.divergences[0];
    assert!(
        d.detail.contains("differs"),
        "the finding must be a content difference, not a missing decision: {}",
        d.detail
    );
}

// ---------------------------------------------------------------------------
// Mechanics
// ---------------------------------------------------------------------------

#[test]
fn the_encoding_is_exact_and_order_sensitive() {
    let buy = OrderIntent {
        side: Side::Buy,
        qty: Qty(1),
        kind: OrderKind::Market,
        bracket: None,
    };
    let sell = OrderIntent {
        side: Side::Sell,
        qty: Qty(1),
        kind: OrderKind::Market,
        bracket: None,
    };
    assert_ne!(
        Decision::encode(&[buy.clone(), sell.clone()]),
        Decision::encode(&[sell, buy.clone()]),
        "order is part of the decision"
    );
    let bigger = OrderIntent {
        qty: Qty(2),
        ..buy.clone()
    };
    assert_ne!(
        Decision::encode(&[buy]),
        Decision::encode(&[bigger]),
        "quantity is part of the decision"
    );
}

#[test]
#[should_panic(expected = "every cut point lies inside the series")]
fn a_cut_outside_the_series_is_refused_rather_than_clamped() {
    let events = events();
    let _ = truncation_invariance(&events, &[BARS + 1], |p| {
        decisions_of(p, SmaCross::new(20, 50, Qty(1)))
    });
}

#[test]
fn a_sweep_that_compared_nothing_does_not_hold() {
    let report = TruncationReport {
        cuts: vec![10],
        divergences: Vec::new(),
        compared: 0,
    };
    assert!(
        !report.holds(),
        "an absent measurement is not a cleared bar (D-0075)"
    );
}

/// The harness's **pinned determinism hash** (D-0088). Assigned directly
/// rather than as a `D-TBD(...)` placeholder because this lands on `main`
/// through the primary session, which is what §8.2 allows.
///
/// Inputs, so it can be re-derived: the seeded random walk `SyntheticFeed::
/// random_walk(seed 29, 3000 bars, 1m, start 5000.00 pt, tick 0.25, vol 4)`,
/// `SmaCross(20, 50, qty 1)` under `FreeFills`, cuts `[1200, 1800, 2400, 2800]`.
#[test]
fn the_truncation_sweep_is_pinned() {
    let events = events();
    let report = truncation_invariance(&events, &CUTS, |prefix| {
        decisions_of(prefix, SmaCross::new(20, 50, Qty(1)))
    });

    let mut h = blake3::Hasher::new();
    h.update(&(report.compared as u64).to_le_bytes());
    h.update(&(report.divergences.len() as u64).to_le_bytes());
    for c in &report.cuts {
        h.update(&(*c as u64).to_le_bytes());
    }
    // The decision stream itself, so a change in what the strategy did fails
    // here even when the count of comparisons happens to match.
    for d in decisions_of(&events, SmaCross::new(20, 50, Qty(1))) {
        h.update(&d.avail_ts.to_le_bytes());
        h.update(&d.bytes);
    }
    let digest = h.finalize().to_hex()[..16].to_owned();

    eprintln!("[pinned] truncation hash = {digest}");
    assert_eq!(
        digest, "91b9ff5b9bbcdb25",
        "the truncation sweep moved. Same seed and same series must give the same \
         decisions; if this changed on purpose, re-derive it and say so in a decision entry."
    );
}
