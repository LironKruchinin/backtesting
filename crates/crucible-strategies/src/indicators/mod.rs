//! Streaming indicators, close-based in v0.
//!
//! Numerical note: rolling-sum implementations (SMA, Bollinger) accumulate
//! float drift over very long streams. Acceptable for the skeleton; milestone
//! M2 replaces the accumulators with periodically-rebased or Welford-style
//! updates. Do not "optimize" these back to naive `sum/period` recomputation
//! over the window — O(period) per bar breaks grid-search throughput.

mod bollinger;
mod ema;
mod sma;

pub use bollinger::{Bollinger, BollingerBands};
pub use ema::Ema;
pub use sma::Sma;

#[cfg(test)]
mod signal_space_tests {
    use super::{Bollinger, Ema, Sma};
    use crucible_core::prelude::*;

    /// A bar that traded at `close` and sits `offset` points higher in signal
    /// space — the shape a back-adjusted continuous bar has (D-0076).
    fn shifted_bar(close: f64, offset: i64) -> Bar {
        let p = Price::from_points_f64_lossy(close);
        Bar {
            instrument: InstrumentId::new("ES.v.0"),
            tf: TimeFrame::M1,
            ts_open: Ts(0),
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1,
            signal_offset: Price::from_points(offset),
        }
    }

    /// **Every** indicator consumes the signal view, not the tradeable one.
    ///
    /// One test over all three rather than one each: the rule is a property of
    /// indicator space, and three separate tests is three places to forget the
    /// fourth indicator. A bar that traded at 100 with a +20 offset must read
    /// 120 everywhere — reading 100 anywhere means that indicator is fed the
    /// raw front-month price and would see a step at every roll while its
    /// neighbours in the same rule do not.
    #[test]
    fn every_indicator_reads_the_signal_view() {
        let bar = shifted_bar(100.0, 20);

        let mut sma = Sma::new(1);
        assert_eq!(
            sma.update(&bar),
            Some(120.0),
            "Sma read the tradeable close"
        );

        let mut ema = Ema::new(1);
        assert_eq!(
            ema.update(&bar),
            Some(120.0),
            "Ema read the tradeable close"
        );

        // A 1-period Bollinger has zero variance, so the bands sit on the mid.
        let mut bb = Bollinger::new(1, 2.0);
        let bands = bb.update(&bar).expect("warm after one bar");
        assert!(
            (bands.mid - 120.0).abs() < 1e-9,
            "Bollinger read the tradeable close: {bands:?}"
        );

        // The control that makes the numbers above mean something: with no
        // offset the same bar reads 100, so 120 is the offset arriving and not
        // a constant baked into the fixture.
        let plain = shifted_bar(100.0, 0);
        assert_eq!(Sma::new(1).update(&plain), Some(100.0));
    }
}
