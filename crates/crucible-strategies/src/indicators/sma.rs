//! Simple moving average of bar closes.

use std::collections::VecDeque;

use crucible_core::events::Bar;
use crucible_core::traits::Indicator;

#[derive(Debug, Clone)]
pub struct Sma {
    period: usize,
    window: VecDeque<f64>,
    sum: f64,
}

impl Sma {
    /// # Panics
    /// Panics if `period == 0` — a zero-period indicator is a config bug and
    /// must fail loudly at construction, not produce NaNs at runtime.
    #[must_use]
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "SMA period must be positive");
        Sma {
            period,
            window: VecDeque::with_capacity(period + 1),
            sum: 0.0,
        }
    }
}

impl Indicator for Sma {
    type Out = f64;

    fn warmup(&self) -> usize {
        self.period
    }

    fn update(&mut self, bar: &Bar) -> Option<f64> {
        let x = bar.close.as_points_f64();
        self.window.push_back(x);
        self.sum += x;
        if self.window.len() > self.period
            && let Some(old) = self.window.pop_front()
        {
            self.sum -= old;
        }
        #[expect(clippy::cast_precision_loss, reason = "periods are small")]
        if self.window.len() == self.period {
            Some(self.sum / self.period as f64)
        } else {
            None
        }
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
        }
    }

    #[test]
    fn sma_hand_computed() {
        let mut sma = Sma::new(3);
        assert_eq!(sma.update(&bar(0, 1.0)), None);
        assert_eq!(sma.update(&bar(1, 2.0)), None);
        assert_eq!(sma.update(&bar(2, 3.0)), Some(2.0));
        assert_eq!(sma.update(&bar(3, 4.0)), Some(3.0));
        assert_eq!(sma.update(&bar(4, 10.0)), Some(17.0 / 3.0));
    }
}
