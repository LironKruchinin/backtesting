//! Bollinger Bands: SMA(period) ± k · population standard deviation.
//!
//! Population variance (ddof = 0) is the pinned convention — it matches the
//! common charting-package definition. Changing to sample variance would
//! silently shift every band-based strategy (docs/DECISIONS.md D-0009).

use std::collections::VecDeque;

use crucible_core::events::Bar;
use crucible_core::traits::Indicator;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BollingerBands {
    pub mid: f64,
    pub upper: f64,
    pub lower: f64,
}

#[derive(Debug, Clone)]
pub struct Bollinger {
    period: usize,
    k: f64,
    window: VecDeque<f64>,
    sum: f64,
    sumsq: f64,
}

impl Bollinger {
    /// # Panics
    /// Panics if `period == 0` or `k` is not finite/positive (config bug).
    #[must_use]
    pub fn new(period: usize, k: f64) -> Self {
        assert!(period > 0, "Bollinger period must be positive");
        assert!(k.is_finite() && k > 0.0, "Bollinger k must be positive");
        Bollinger {
            period,
            k,
            window: VecDeque::with_capacity(period + 1),
            sum: 0.0,
            sumsq: 0.0,
        }
    }
}

impl Indicator for Bollinger {
    type Out = BollingerBands;

    fn warmup(&self) -> usize {
        self.period
    }

    fn update(&mut self, bar: &Bar) -> Option<BollingerBands> {
        let x = bar.signal_close().as_points_f64();
        self.window.push_back(x);
        self.sum += x;
        self.sumsq += x * x;
        if self.window.len() > self.period
            && let Some(old) = self.window.pop_front()
        {
            self.sum -= old;
            self.sumsq -= old * old;
        }
        if self.window.len() < self.period {
            return None;
        }
        #[expect(clippy::cast_precision_loss, reason = "periods are small")]
        let n = self.period as f64;
        let mean = self.sum / n;
        // Population variance; clamp tiny negative values from float noise.
        let var = (self.sumsq / n - mean * mean).max(0.0);
        let sd = var.sqrt();
        Some(BollingerBands {
            mid: mean,
            upper: mean + self.k * sd,
            lower: mean - self.k * sd,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::prelude::*;

    fn bar(i: i64, close: f64) -> Bar {
        let p = Price::from_points_f64_lossy(close);
        Bar {
            instrument: InstrumentId::new("SYN:TEST"),
            tf: TimeFrame::M1,
            ts_open: Ts(i * 60_000_000_000),
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1,
            signal_offset: Price::ZERO,
        }
    }

    #[test]
    fn bollinger_hand_computed() {
        let mut bb = Bollinger::new(3, 2.0);
        assert!(bb.update(&bar(0, 1.0)).is_none());
        assert!(bb.update(&bar(1, 2.0)).is_none());
        let out = bb.update(&bar(2, 3.0)).expect("warm after 3 bars");
        // mean = 2, population var = 2/3, sd = 0.816496580927726
        let sd = (2.0_f64 / 3.0).sqrt();
        assert!((out.mid - 2.0).abs() < 1e-12);
        assert!((out.upper - (2.0 + 2.0 * sd)).abs() < 1e-12);
        assert!((out.lower - (2.0 - 2.0 * sd)).abs() < 1e-12);
    }
}
