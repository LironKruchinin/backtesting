//! SMA crossover — the reference strategy.
//!
//! Not here because it makes money (it almost certainly doesn't); it's the
//! simplest strategy with entries, exits, longs and shorts, which makes it
//! the standard fixture for golden tests, determinism checks, and the demo.

use crucible_core::prelude::*;

use crate::indicators::Sma;

pub struct SmaCross {
    fast: Sma,
    slow: Sma,
    slow_period: usize,
    qty: Qty,
    prev_diff: Option<f64>,
}

impl SmaCross {
    /// # Panics
    /// Panics unless `0 < fast_period < slow_period` (config bug).
    #[must_use]
    pub fn new(fast_period: usize, slow_period: usize, qty: Qty) -> Self {
        assert!(
            fast_period > 0 && fast_period < slow_period,
            "require 0 < fast < slow"
        );
        SmaCross {
            fast: Sma::new(fast_period),
            slow: Sma::new(slow_period),
            slow_period,
            qty: qty.abs(),
            prev_diff: None,
        }
    }
}

impl Strategy for SmaCross {
    fn warmup_bars(&self) -> usize {
        // Cross detection needs two consecutive readings of both SMAs.
        self.slow_period + 1
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        let MarketEvent::Bar(bar) = ev;
        let fast = self.fast.update(bar);
        let slow = self.slow.update(bar);
        let (Some(fast), Some(slow)) = (fast, slow) else {
            return;
        };
        let diff = fast - slow;
        if let Some(prev) = self.prev_diff {
            if prev <= 0.0 && diff > 0.0 {
                actions.target_position(view.position, self.qty);
            } else if prev >= 0.0 && diff < 0.0 {
                actions.target_position(view.position, Qty(-self.qty.0));
            }
        }
        self.prev_diff = Some(diff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(i: i64, close: f64) -> MarketEvent {
        let p = Price::from_points_f64_lossy(close);
        MarketEvent::Bar(Bar {
            instrument: InstrumentId::new("SYN:TEST"),
            tf: TimeFrame::M1,
            ts_open: Ts(i * 60_000_000_000),
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1,
        })
    }

    /// Closes [10,9,8,7,12,15] with fast=2, slow=3:
    /// diffs run -0.5, -0.5, +0.5, +2.17 → exactly one cross-up, at bar 4.
    #[test]
    fn fires_exactly_on_cross_up() {
        let mut strat = SmaCross::new(2, 3, Qty(1));
        let flat = PortfolioView {
            position: Qty(0),
            avg_entry: None,
            cash_nano_usd: 0,
            equity_nano_usd: 0,
        };
        let closes = [10.0, 9.0, 8.0, 7.0, 12.0, 15.0];
        let mut fired_at = Vec::new();
        for (i, &c) in closes.iter().enumerate() {
            let mut actions = Actions::new();
            #[expect(clippy::cast_possible_wrap, reason = "test index")]
            strat.on_event(&bar(i as i64, c), &flat, &mut actions);
            let intents = actions.take_intents();
            if !intents.is_empty() {
                assert_eq!(intents.len(), 1);
                assert_eq!(intents[0].side, Side::Buy);
                assert_eq!(intents[0].qty, Qty(1));
                fired_at.push(i);
            }
        }
        assert_eq!(fired_at, vec![4]);
    }
}
