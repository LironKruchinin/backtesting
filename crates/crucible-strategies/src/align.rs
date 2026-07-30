//! Warmup alignment: making every combo in a grid trade the same bars.
//!
//! CLAUDE.md §2.6 says grid combos are scored on identical evaluation
//! windows, with warmup taken as the max across the grid. Without that, a
//! 10-bar combo starts trading 190 bars before a 200-bar one, and the two
//! numbers underneath a "best parameters" claim are not comparable — the
//! short one had a different, longer, and differently-timed sample.
//!
//! [`Aligned`] is how that rule is enforced (D-0061). It is a decorator, not
//! an engine feature, for two reasons:
//!
//! - The engine's loop enforces the *no-lookahead* rule and nothing else.
//!   Fairness is a property of a **set** of runs, which the engine
//!   deliberately cannot see.
//! - Every strategy gets it, not just config-built ones. A hand-coded
//!   `SmaCross` dropped into a grid is aligned by the same wrapper.
//!
//! **It drops orders; it does not withhold bars.** The inner strategy sees
//! every bar, so its indicators warm on exactly the data they otherwise
//! would; only the intents it emits before the window opens are discarded. A
//! wrapper that skipped `on_event` instead would leave the short-warmup combo
//! starting *late* rather than starting *aligned*, which is the opposite of
//! the guarantee.
//!
//! The cost, which is real and printed rather than absorbed: every combo's
//! equity curve carries a flat prefix of exactly `eval_start_bars` bars. It
//! is identical across the grid, so rankings are unaffected, but each naive
//! Sharpe carries the same `sqrt(n_eval / n_total)` factor against a run
//! started at its own warmup. Slicing the window out of the metrics belongs
//! to the walk-forward runner, which has to slice folds anyway.

use crucible_core::prelude::*;

/// Wraps a strategy so its orders only count once the evaluation window has
/// opened.
#[derive(Debug)]
pub struct Aligned<S> {
    inner: S,
    eval_start_bars: usize,
    bars_seen: usize,
    suppressed_intents: usize,
    /// Reused so a suppressed bar allocates at most once over a whole run.
    scratch: Actions,
}

impl<S: Strategy> Aligned<S> {
    /// Aligns `inner` to a window that opens after `eval_start_bars` bars.
    ///
    /// Pass the **grid's** max warmup, not the strategy's own — that is the
    /// entire point. [`crate::combo::Grid::aligned_strategy`] does it for you.
    #[must_use]
    pub fn new(inner: S, eval_start_bars: usize) -> Aligned<S> {
        Aligned {
            inner,
            eval_start_bars,
            bars_seen: 0,
            suppressed_intents: 0,
            scratch: Actions::new(),
        }
    }

    /// The bar index at which this strategy's orders start counting. Bars
    /// `0..eval_start_bars` are warmup for every combo in the grid.
    #[must_use]
    pub const fn eval_start_bars(&self) -> usize {
        self.eval_start_bars
    }

    /// Bars consumed so far.
    #[must_use]
    pub const fn bars_seen(&self) -> usize {
        self.bars_seen
    }

    /// Orders the inner strategy wanted to place before the window opened.
    ///
    /// This is the number that makes alignment visible instead of silent: it
    /// is exactly the edge a short-warmup combo would have had over the
    /// longest one in its grid.
    #[must_use]
    pub const fn suppressed_intents(&self) -> usize {
        self.suppressed_intents
    }

    /// The wrapped strategy, for reading counters it exposes.
    #[must_use]
    pub const fn inner(&self) -> &S {
        &self.inner
    }

    /// Unwraps.
    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Strategy> Strategy for Aligned<S> {
    fn warmup_bars(&self) -> usize {
        // The grid's warmup, not the inner strategy's: what this wrapper
        // reports is the window every combo shares.
        self.eval_start_bars
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        if self.bars_seen < self.eval_start_bars {
            self.bars_seen += 1;
            self.inner.on_event(ev, view, &mut self.scratch);
            self.suppressed_intents += self.scratch.take_intents().len();
            return;
        }
        self.bars_seen += 1;
        self.inner.on_event(ev, view, actions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SmaCross;

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
            signal_offset: Price::ZERO,
        })
    }

    /// A deterministic zig-zag that crosses often enough for a short-window
    /// crossover to trade early and a long-window one to trade late.
    fn closes(n: usize) -> Vec<f64> {
        #[expect(clippy::cast_precision_loss, reason = "small test indices")]
        (0..n)
            .map(|i: usize| 100.0 + ((i * 7) % 23) as f64 - 11.0)
            .collect()
    }

    /// Runs a strategy and returns the index of the first bar that emitted an
    /// order, plus how many orders it emitted in total.
    fn first_order<S: Strategy>(strategy: &mut S, closes: &[f64]) -> (Option<usize>, usize) {
        let mut position = Qty(0);
        let mut first = None;
        let mut count = 0usize;
        for (i, &c) in closes.iter().enumerate() {
            let view = PortfolioView {
                position,
                avg_entry: None,
                cash_nano_usd: 0,
                equity_nano_usd: 0,
            };
            let mut actions = Actions::new();
            #[expect(clippy::cast_possible_wrap, reason = "test index")]
            strategy.on_event(&bar(i as i64, c), &view, &mut actions);
            for intent in actions.take_intents() {
                count += 1;
                first.get_or_insert(i);
                let delta = intent.qty.0 * i32::try_from(intent.side.sign()).expect("±1");
                position = Qty(position.0 + delta);
            }
        }
        (first, count)
    }

    /// The §2.6 test: a short-warmup combo placed in a grid with a
    /// long-warmup one gains nothing. Both start on the same bar, and the
    /// short one is measurably *worse off* than it would be alone — which is
    /// the correct direction, and what makes the comparison fair.
    #[test]
    fn a_short_warmup_combo_gains_nothing_from_being_short() {
        let series = closes(400);
        // Own warmups: 3+1 = 4 bars and 200+1 = 201 bars.
        let short_alone = first_order(&mut SmaCross::new(2, 3, Qty(1)), &series);
        let long_alone = first_order(&mut SmaCross::new(50, 200, Qty(1)), &series);

        let grid_warmup = 201;
        let mut short_aligned = Aligned::new(SmaCross::new(2, 3, Qty(1)), grid_warmup);
        let mut long_aligned = Aligned::new(SmaCross::new(50, 200, Qty(1)), grid_warmup);
        let short = first_order(&mut short_aligned, &series);
        let long = first_order(&mut long_aligned, &series);

        // Alone, the short combo starts hundreds of bars earlier.
        assert!(
            short_alone.0.expect("trades") < long_alone.0.expect("trades"),
            "the fixture must actually show the unfair head start"
        );
        // In the grid, neither may act before the shared window opens.
        assert!(short.0.expect("trades") >= grid_warmup);
        assert!(long.0.expect("trades") >= grid_warmup);
        // And the head start is gone: the orders it would have placed are
        // counted, not quietly forgotten.
        assert!(short_aligned.suppressed_intents() > 0);
        assert_eq!(
            short.1 + short_aligned.suppressed_intents(),
            short_alone.1,
            "every suppressed order is one the unaligned run made"
        );
        assert_eq!(long_aligned.suppressed_intents(), 0);
    }

    /// Two different combos aligned to the same grid see the same window.
    #[test]
    fn every_combo_in_a_grid_reports_the_same_window() {
        let a = Aligned::new(SmaCross::new(2, 3, Qty(1)), 201);
        let b = Aligned::new(SmaCross::new(50, 200, Qty(1)), 201);
        assert_eq!(a.warmup_bars(), b.warmup_bars());
        assert_eq!(a.eval_start_bars(), 201);
    }

    /// Bars still reach the inner strategy during warmup — otherwise the
    /// indicators would not be warm when the window opens, and the combo
    /// would start late rather than aligned.
    #[test]
    fn the_inner_strategy_still_sees_the_warmup_bars() {
        let series = closes(60);
        let mut aligned = Aligned::new(SmaCross::new(2, 3, Qty(1)), 20);
        let (first, _) = first_order(&mut aligned, &series);
        assert_eq!(aligned.bars_seen(), 60);
        // Warm long before bar 20, so the first allowed bar can trade: it does
        // not need another 3 bars to spin up.
        assert_eq!(first, Some(20));
    }
}
