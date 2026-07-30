//! Rolling-statistic tests, and the planted control (D-0080).
//!
//! The hand-computed cases come first. The last three are the control this
//! module exists to pass, and they are the point: a rolling reading is
//! **truncation-invariant** — appending bars after bar *i* cannot change the
//! reading at bar *i* — and a full-sample reading is not. The difference
//! between the two is the lookahead §2.1 names, and it is measured here rather
//! than asserted.
//!
//! The full-sample computation below is written out **in the test**, on
//! purpose. There is no such function in the library and there must not be
//! one; the only way to compare against it is to build it where it cannot
//! escape.

use super::*;
use crucible_core::prelude::*;

fn bar(i: i64, close: f64, volume: u64) -> Bar {
    let p = Price::from_points_f64_lossy(close);
    Bar {
        instrument: InstrumentId::new("SYN:TEST"),
        tf: TimeFrame::M1,
        ts_open: Ts(i * 60_000_000_000),
        open: p,
        high: p,
        low: p,
        close: p,
        volume,
        signal_offset: Price::ZERO,
    }
}

/// A trending series with variation inside every window: a ramp of +1 a bar
/// plus a period-7 sawtooth, so no window is ever flat and the trend is strong
/// enough that a full-sample mean is nowhere near a local one.
fn trending(n: usize) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "small test indices")]
            let close = 100.0 + i as f64 + f64::from(u32::try_from(i % 7).unwrap_or(0));
            let volume = 100 + u64::try_from(i % 5).unwrap_or(0) * 10;
            #[expect(clippy::cast_possible_wrap, reason = "small test indices")]
            bar(i as i64, close, volume)
        })
        .collect()
}

/// Every reading a fresh indicator produces over `bars`, `None` included.
fn readings<I: Indicator<Out = f64>>(mut indicator: I, bars: &[Bar]) -> Vec<Option<f64>> {
    bars.iter().map(|b| indicator.update(b)).collect()
}

// ------------------------------------------------------------ hand-computed

/// Hand-computed. Closes [1, 2, 3] with `period = 3`:
///
/// mean = 2, population variance = ((1−2)² + (2−2)² + (3−2)²)/3 = 2/3,
/// sd = sqrt(2/3) = 0.816496580927726, and the reading standardizes the
/// **current** bar: z = (3 − 2) / sd = 1.224744871391589.
///
/// The two bars before it are `None`: the window is not full, and an
/// indicator with an incomplete window has no opinion rather than a partial
/// one.
#[test]
fn a_z_score_over_three_closes_is_hand_computable() {
    let bars = [bar(0, 1.0, 1), bar(1, 2.0, 1), bar(2, 3.0, 1)];
    let out = readings(RollingZScore::new(3, RollingSource::Close), &bars);
    assert_eq!(out[0], None);
    assert_eq!(out[1], None);
    let sd = (2.0_f64 / 3.0).sqrt();
    let z = out[2].expect("warm after three bars");
    assert!((z - 1.0 / sd).abs() < 1e-12, "{z}");

    // The same window's standard deviation, on its own.
    let sds = readings(RollingStdev::new(3, RollingSource::Close), &bars);
    assert!((sds[2].expect("warm") - sd).abs() < 1e-12);
}

/// The same three-bar arithmetic on volume, so the source is proven to be read
/// rather than defaulted: closes are held flat and the volumes are 1, 2, 3.
#[test]
fn the_source_selects_which_series_is_normalized() {
    let bars = [bar(0, 5000.0, 1), bar(1, 5000.0, 2), bar(2, 5000.0, 3)];
    let sd = (2.0_f64 / 3.0).sqrt();
    let z = readings(RollingZScore::new(3, RollingSource::Volume), &bars)[2]
        .expect("warm after three bars");
    assert!((z - 1.0 / sd).abs() < 1e-12, "{z}");

    // The control: the closes are flat, so a close z-score over the same bars
    // has no opinion at all. A source that was being ignored would give the
    // same answer twice.
    assert_eq!(
        readings(RollingZScore::new(3, RollingSource::Close), &bars)[2],
        None
    );
}

/// A return source costs one extra bar of warmup, and the arithmetic is the
/// simple bar-over-bar return.
///
/// Closes [100, 110, 121, 121] give returns [—, 0.1, 0.1, 0.0]. With
/// `period = 2` the window is full on bar index 2 (two returns: 0.1 and 0.1),
/// which has no spread — so the z-score is `None` and the standard deviation is
/// 0. On bar 3 the window is [0.1, 0.0]: mean 0.05, population sd 0.05, and
/// z = (0.0 − 0.05) / 0.05 = −1.
#[test]
fn a_return_source_costs_one_extra_warmup_bar() {
    let bars = [
        bar(0, 100.0, 1),
        bar(1, 110.0, 1),
        bar(2, 121.0, 1),
        bar(3, 121.0, 1),
    ];
    let z = RollingZScore::new(2, RollingSource::Return);
    assert_eq!(z.warmup(), 3, "two returns need three closes");
    let out = readings(z, &bars);
    assert_eq!(out[0], None);
    assert_eq!(out[1], None);
    assert_eq!(out[2], None, "a window of two equal returns has no spread");
    assert!((out[3].expect("warm") + 1.0).abs() < 1e-12, "{:?}", out[3]);

    let sds = readings(RollingStdev::new(2, RollingSource::Return), &bars);
    assert!((sds[2].expect("warm") - 0.0).abs() < 1e-12);
    assert!((sds[3].expect("warm") - 0.05).abs() < 1e-12);
}

/// Warmup is declared, and it is what §2.6 aligns a grid on.
#[test]
fn warmup_is_the_period_plus_what_the_source_needs() {
    assert_eq!(RollingZScore::new(20, RollingSource::Close).warmup(), 20);
    assert_eq!(RollingZScore::new(20, RollingSource::Volume).warmup(), 20);
    assert_eq!(RollingZScore::new(20, RollingSource::Return).warmup(), 21);
    assert_eq!(RollingStdev::new(5, RollingSource::Return).warmup(), 6);
}

/// A window of one is a config bug, not a runtime condition: every reading over
/// it would be 0/0 forever.
#[test]
#[should_panic(expected = "at least 2 bars")]
fn a_one_bar_window_is_a_config_bug() {
    let _ = RollingZScore::new(1, RollingSource::Close);
}

/// A flat window is silent, not zero, and its standard deviation is zero, not
/// silent. The pair is the distinction: "no opinion" and "no variation" are
/// different facts.
#[test]
fn a_flat_window_has_no_z_score_but_does_have_a_deviation() {
    let bars: Vec<Bar> = (0..5).map(|i| bar(i, 100.0, 7)).collect();
    assert_eq!(
        readings(RollingZScore::new(3, RollingSource::Close), &bars)[4],
        None
    );
    assert_eq!(
        readings(RollingStdev::new(3, RollingSource::Close), &bars)[4],
        Some(0.0)
    );
}

/// A z-score is shift-invariant, so it is safe on a back-adjusted series where
/// `close > 4500` is not (D-0076): the offset cancels between the value and the
/// window's mean.
///
/// **Within a tolerance, not bit-exactly**, and the distinction is worth
/// stating. Shift-invariance is exact in real arithmetic; this implementation
/// carries a rolling sum, and summing values around 350 rounds differently from
/// summing values around 100, so the two runs agree to about twelve digits
/// rather than to the last bit. That is the accumulator drift `indicators`'
/// module docs already accept until the M2 Welford/rebase task, showing up
/// where it is easiest to see. It is a numerics gap, not a semantics one — a
/// 1e-12 difference cannot move a comparison against a threshold anyone writes.
///
/// Contrast `a_rolling_reading_is_truncation_invariant_and_a_full_sample_one_is_not`,
/// which *is* asserted bit-exactly: there the accumulator sees identical values
/// in identical order, so anything less than equality would mean state leaked
/// between the two runs.
#[test]
fn a_z_score_is_shift_invariant() {
    let plain = trending(60);
    let shifted: Vec<Bar> = plain
        .iter()
        .map(|b| Bar {
            signal_offset: Price::from_points(250),
            ..b.clone()
        })
        .collect();
    let a = readings(RollingZScore::new(20, RollingSource::Close), &plain);
    let b = readings(RollingZScore::new(20, RollingSource::Close), &shifted);
    assert_eq!(a.len(), b.len());
    let mut compared = 0usize;
    for (x, y) in a.iter().zip(&b) {
        match (x, y) {
            (Some(x), Some(y)) => {
                compared += 1;
                assert!((x - y).abs() < 1e-9, "{x} vs {y}");
            }
            (None, None) => {}
            _ => panic!("the offset changed which bars had readings: {x:?} vs {y:?}"),
        }
    }
    assert!(compared >= 40, "only {compared} readings were compared");
    // The control: the level itself did move, so the fixture really is shifted.
    assert!(
        (shifted[0].signal_close().as_points_f64()
            - plain[0].signal_close().as_points_f64()
            - 250.0)
            .abs()
            < 1e-9
    );
}

// ------------------------------------------------------- the planted control

/// The full-sample z-score of the closes of `bars` — **the defect**, written
/// here and only here.
///
/// This is the computation §2.1 names by name and `controls::LeakyZScore`
/// implements as a strategy. It exists in this file so the property below can
/// be measured against something, and it exists nowhere else in the workspace:
/// no `IndicatorKind` names it, so no config can reach it.
fn full_sample_z(bars: &[Bar]) -> Vec<f64> {
    let values: Vec<f64> = bars
        .iter()
        .map(|b| b.signal_close().as_points_f64())
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "small test fixtures")]
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    let sd = variance.sqrt();
    values.iter().map(|x| (x - mean) / sd).collect()
}

/// **The control.** A rolling reading at bar *i* does not move when bars after
/// *i* are appended. A full-sample reading does, and by a lot.
///
/// Both halves are asserted, because only the pair is a diagnosis (§7): the
/// first alone says "these numbers are stable" and the second alone says
/// "those numbers are unstable". Together they say the rolling statistic
/// **excludes** something the full-sample one uses, and that something is the
/// future.
///
/// The rolling agreement is asserted **bit-exact**, not within a tolerance:
/// the accumulator sees the same values in the same order in both runs, so
/// anything less than equality would mean state was leaking between them.
#[test]
fn a_rolling_reading_is_truncation_invariant_and_a_full_sample_one_is_not() {
    let all = trending(200);
    let prefix = &all[..100];

    let rolling_all = readings(RollingZScore::new(20, RollingSource::Close), &all);
    let rolling_prefix = readings(RollingZScore::new(20, RollingSource::Close), prefix);
    assert_eq!(
        rolling_all[..100],
        rolling_prefix[..],
        "a rolling reading changed when the future was appended"
    );
    // Not vacuous: the fixture produces real readings over that prefix.
    assert!(
        rolling_prefix.iter().filter(|r| r.is_some()).count() >= 80,
        "the fixture must actually produce readings"
    );

    let leaky_all = full_sample_z(&all);
    let leaky_prefix = full_sample_z(prefix);
    let moved = leaky_all[..100]
        .iter()
        .zip(&leaky_prefix)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        moved > 1.0,
        "the full-sample control must be seen moving, and moved only {moved}"
    );
}

/// The size of what is being excluded, on a trending fixture.
///
/// At the same bar the two readings answer different questions: the rolling one
/// asks "where is this bar against its own recent range", the full-sample one
/// asks "where is this bar against a range that includes bars it precedes". On
/// a trending series the second is monotone by construction — every early bar
/// is far below the eventual mean — and the gap is enormous.
///
/// The assertion is the honest form of the claim in §2.1: this is not a
/// rounding difference that a careful implementation could avoid. It is a
/// different number, and a strategy trading on it is trading on the future.
#[test]
fn the_difference_from_a_full_sample_z_score_is_the_lookahead() {
    let bars = trending(200);
    let rolling = readings(RollingZScore::new(20, RollingSource::Close), &bars);
    let leaky = full_sample_z(&bars);

    let mut compared = 0usize;
    let mut worst = 0.0_f64;
    for (r, l) in rolling.iter().zip(&leaky) {
        if let Some(r) = r {
            compared += 1;
            worst = worst.max((r - l).abs());
        }
    }
    assert!(compared >= 180, "only {compared} bars were compared");
    assert!(
        worst > 2.0,
        "the two statistics differ by only {worst} sigma; on a trending fixture \
         they must differ by far more, or this control is not measuring the leak"
    );

    // The direction, named: the full-sample reading of the first warm bar is
    // deeply negative because it "knows" the series climbs for 180 more bars,
    // while the rolling one is a local reading of ordinary size.
    let first_warm = rolling.iter().position(Option::is_some).expect("some warm");
    assert!(leaky[first_warm] < -1.0, "{}", leaky[first_warm]);
    assert!(
        rolling[first_warm].expect("warm").abs() < 2.5,
        "{:?}",
        rolling[first_warm]
    );
}
