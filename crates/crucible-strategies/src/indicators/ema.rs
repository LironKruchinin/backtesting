//! Exponential moving average of bar closes.
//!
//! Seeding: the first output is the SMA of the first `period` closes, then
//! `ema' = α·x + (1−α)·ema` with `α = 2/(period+1)`. Seeding choice is a
//! pinned decision (docs/DECISIONS.md D-0008) — changing it silently changes
//! every EMA-based backtest, so don't.

use crucible_core::events::Bar;
use crucible_core::traits::Indicator;

#[derive(Debug, Clone)]
pub struct Ema {
    period: usize,
    alpha: f64,
    seed_sum: f64,
    seen: usize,
    value: Option<f64>,
}

impl Ema {
    /// # Panics
    /// Panics if `period == 0` (config bug — fail at construction).
    #[must_use]
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "EMA period must be positive");
        #[expect(clippy::cast_precision_loss, reason = "periods are small")]
        Ema {
            period,
            alpha: 2.0 / (period as f64 + 1.0),
            seed_sum: 0.0,
            seen: 0,
            value: None,
        }
    }
}

impl Indicator for Ema {
    type Out = f64;

    fn warmup(&self) -> usize {
        self.period
    }

    fn update(&mut self, bar: &Bar) -> Option<f64> {
        let x = bar.signal_close().as_points_f64();
        match self.value {
            Some(prev) => {
                let next = self.alpha * x + (1.0 - self.alpha) * prev;
                self.value = Some(next);
                Some(next)
            }
            None => {
                self.seed_sum += x;
                self.seen += 1;
                #[expect(clippy::cast_precision_loss, reason = "periods are small")]
                if self.seen == self.period {
                    let seed = self.seed_sum / self.period as f64;
                    self.value = Some(seed);
                    Some(seed)
                } else {
                    None
                }
            }
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
            signal_offset: Price::ZERO,
        }
    }

    #[test]
    fn ema_seeds_with_sma_then_smooths() {
        let mut ema = Ema::new(3); // alpha = 0.5
        assert_eq!(ema.update(&bar(0, 1.0)), None);
        assert_eq!(ema.update(&bar(1, 2.0)), None);
        assert_eq!(ema.update(&bar(2, 3.0)), Some(2.0)); // SMA seed
        assert_eq!(ema.update(&bar(3, 4.0)), Some(3.0)); // 0.5*4 + 0.5*2
    }
}
