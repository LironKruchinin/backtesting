//! Computing a statistic on the window it claims.
//!
//! This is the module D-0061 deferred to. `crucible combo` reports one
//! [`Summary`] for a whole replay, so every combo's equity carries a flat
//! prefix as long as the grid's warmup and every naive Sharpe is scaled by
//! the same `sqrt(n_eval / n_total)`. A fold table cannot absorb that: its
//! whole claim is that the number under "fold 2 OOS" was computed on fold 2's
//! out-of-sample bars and nothing else.
//!
//! ## Three conventions, all of which change numbers
//!
//! **The anchor bar.** A window `[start, end)` is measured from the equity
//! point at `start - 1`, not at `start`. The move *into* the window's first
//! bar is the window's first return; starting at `start` would silently drop
//! it. Fold 0's anchor always exists because folds are laid out after the
//! grid's warmup — except when there is no warmup at all, where the anchor
//! degenerates to bar 0 and the window is short one return, which is correct
//! rather than convenient: nothing happened before bar 0.
//!
//! **Rebasing to declared capital (D-0063).** A window's curve is rebased so
//! it opens at the config's `initial_cash_usd`, carrying the window's own
//! per-bar equity *deltas*. Positions here are a fixed contract count
//! (`run.qty_contracts`), so a window's dollar PnL does not scale with the
//! account: expressing fold 7 as a return on whatever equity had drifted to
//! by fold 7 would make two identical windows report different percentages
//! purely because of what happened before them. Rebasing is additive integer
//! arithmetic on nano-USD — no float ever re-enters accounting (§2.3).
//!
//! **Pooling by deltas, not by levels.** The headline out-of-sample curve is
//! the concatenation of every test window's per-bar deltas. Concatenating
//! equity *levels* across the training windows between them would invent a
//! jump at each seam and hand `max_drawdown_pct` a drawdown nobody could have
//! suffered. The pooled curve is what an account would have done if it held
//! the strategy during out-of-sample windows and sat flat otherwise.

use std::ops::Range;

use crucible_core::prelude::*;
use crucible_engine::{ClosedTrade, FeeEvent, Summary};

/// One replay, indexed so any window can be cut out of it.
///
/// Trades and fees arrive stamped with an instant; a window is a range of bar
/// indices. The join between the two happens once, here, rather than once per
/// window per fold per combo.
#[derive(Clone, Debug)]
pub struct RunTrace<'a> {
    equity: &'a [(Ts, NanoUsd)],
    /// `(bar index, round-trip)` in close order.
    ///
    /// The whole [`ClosedTrade`] rather than its net PnL, because it now
    /// carries the MAE/MFE of the round-trip (`docs/ACCOUNT_EVAL_SPEC.md`
    /// §3.4) and a fold's excursion distribution has to describe the same
    /// trades its trade count does. Attribution is unchanged: by the closing
    /// fill, per D-0063.
    trades: Vec<(usize, ClosedTrade)>,
    /// `(bar index, fee)` per costed fill, in fill order.
    fees: Vec<(usize, NanoUsd)>,
}

impl<'a> RunTrace<'a> {
    /// Indexes one replay's records against its equity curve.
    ///
    /// Both record streams are already in nondecreasing timestamp order and
    /// every fill lands on an event that also produced an equity point, so
    /// this is a merge, not a search.
    #[must_use]
    pub fn new(
        equity: &'a [(Ts, NanoUsd)],
        closed_trades: &[ClosedTrade],
        fee_events: &[FeeEvent],
    ) -> RunTrace<'a> {
        RunTrace {
            equity,
            trades: index_by_bar(equity, closed_trades.iter().map(|t| (t.closed_ts, *t))),
            fees: index_by_bar(equity, fee_events.iter().map(|f| (f.ts, f.fee_nano_usd))),
        }
    }

    /// The whole run, unsliced — the number `crucible combo` prints, kept so a
    /// report can show what the warmup prefix was costing.
    #[must_use]
    pub const fn equity(&self) -> &'a [(Ts, NanoUsd)] {
        self.equity
    }

    /// [`Summary`] of one window, on the window's own bars.
    ///
    /// # Panics
    /// Panics if `bars` is outside the equity curve. Every range reaching
    /// this function comes from a [`FoldPlan`] built against the same series,
    /// so an out-of-range window is a wiring bug, not a data condition.
    ///
    /// [`FoldPlan`]: super::FoldPlan
    #[must_use]
    pub fn window(
        &self,
        bars: Range<usize>,
        initial_cash_nano_usd: NanoUsd,
        bars_per_year: f64,
    ) -> Summary {
        self.pooled(&[bars], initial_cash_nano_usd, bars_per_year)
    }

    /// [`Summary`] over several windows stitched together by their per-bar
    /// deltas — the headline out-of-sample number.
    ///
    /// # Panics
    /// Panics if any range is outside the equity curve, or if the ranges are
    /// not in increasing order. Both are wiring bugs (see [`Self::window`]).
    #[must_use]
    pub fn pooled(
        &self,
        windows: &[Range<usize>],
        initial_cash_nano_usd: NanoUsd,
        bars_per_year: f64,
    ) -> Summary {
        let mut curve: Vec<(Ts, NanoUsd)> = Vec::new();
        let mut level = initial_cash_nano_usd;
        let mut trades: Vec<ClosedTrade> = Vec::new();
        let mut fees_nano_usd: NanoUsd = 0;
        let mut prev_end = 0usize;

        for w in windows {
            assert!(
                w.start >= prev_end && w.end <= self.equity.len(),
                "window {w:?} is out of order or outside a {}-bar run",
                self.equity.len()
            );
            // Windows that tile (the `step_days == test_days` layout) share a
            // seam: this window's anchor is the last point the previous one
            // already contributed. Pushing it again would add a zero return
            // per seam and bias every pooled Sharpe downward by an artifact
            // of the fold count.
            let continues_previous = !curve.is_empty() && w.start == prev_end;
            prev_end = w.end;
            if w.is_empty() {
                continue;
            }
            // When the window opens at bar 0 there is nothing before it, so
            // bar 0 IS the anchor and contributes no return of its own —
            // rather than a duplicated point contributing a spurious zero.
            let anchor = w.start.saturating_sub(1);
            if !continues_previous {
                curve.push((self.equity[anchor].0, level));
            }
            for i in w.start.max(1)..w.end {
                level += self.equity[i].1 - self.equity[i - 1].1;
                curve.push((self.equity[i].0, level));
            }
            for &(bar, trade) in &self.trades {
                if w.contains(&bar) {
                    trades.push(ClosedTrade {
                        closed_ts: self.equity[bar].0,
                        ..trade
                    });
                }
            }
            for &(bar, fee) in &self.fees {
                if w.contains(&bar) {
                    fees_nano_usd += fee;
                }
            }
        }

        Summary::compute(&curve, &trades, fees_nano_usd, bars_per_year)
    }
}

/// Maps each `(ts, value)` record onto the index of the bar it happened on.
///
/// The bar is the first equity point whose timestamp is not before the
/// record's — fills are stamped with the `avail_ts` of the event they filled
/// against, and that event produced an equity point, so the match is exact.
/// A record past the end of the curve cannot happen (the run ended when the
/// curve did) and is dropped rather than panicking: losing one trade from a
/// count is a smaller wrong than aborting a research run over an impossible
/// branch.
fn index_by_bar<T>(
    equity: &[(Ts, NanoUsd)],
    records: impl Iterator<Item = (Ts, T)>,
) -> Vec<(usize, T)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for (ts, value) in records {
        while at < equity.len() && equity[at].0 < ts {
            at += 1;
        }
        if at < equity.len() {
            out.push((at, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Equity in whole dollars at bar `i`, one nanosecond apart so the ts of
    /// bar `i` is `Ts(i)`.
    fn curve(dollars: &[i64]) -> Vec<(Ts, NanoUsd)> {
        dollars
            .iter()
            .enumerate()
            .map(|(i, &d)| {
                (
                    Ts(i64::try_from(i).expect("small test index")),
                    d * 1_000_000_000,
                )
            })
            .collect()
    }

    /// The anchor is the whole point. Equity 100, 100, 130, 120 with the
    /// window `[2, 4)`: the window's PnL is 120 − 100 = +20 (the move INTO
    /// bar 2 is the window's first return), not 120 − 130 = −10.
    ///
    /// Rebased on $1,000 of declared capital: 1000 -> 1030 -> 1020, so
    /// total_return_pct = 1020/1000 − 1 = +2.00 %, and max drawdown is
    /// (1030 − 1020)/1030 = 0.970873…%.
    #[test]
    fn a_window_is_measured_from_the_bar_before_it() {
        let eq = curve(&[100, 100, 130, 120]);
        let trace = RunTrace::new(&eq, &[], &[]);
        let s = trace.window(2..4, 1_000_000_000_000, 252.0);
        assert_eq!(s.initial_equity_nano_usd, 1_000_000_000_000);
        assert_eq!(s.final_equity_nano_usd, 1_020_000_000_000);
        assert!((s.total_return_pct - 2.0).abs() < 1e-9);
        assert!((s.max_drawdown_pct - 100.0 * 10.0 / 1030.0).abs() < 1e-9);
    }

    /// The §2.6-shaped guarantee restated as arithmetic: a window's numbers
    /// do not depend on what the equity level happened to be when it opened.
    /// Two runs with identical PnL inside the window but different histories
    /// before it report the identical window summary.
    #[test]
    fn rebasing_makes_two_windows_with_the_same_pnl_report_the_same() {
        let poor = curve(&[100, 50, 60, 55]);
        let rich = curve(&[100, 900, 910, 905]);
        let cash = 1_000_000_000_000;
        let a = RunTrace::new(&poor, &[], &[]).window(2..4, cash, 252.0);
        let b = RunTrace::new(&rich, &[], &[]).window(2..4, cash, 252.0);
        assert_eq!(a.final_equity_nano_usd, b.final_equity_nano_usd);
        assert!((a.total_return_pct - b.total_return_pct).abs() < 1e-12);
        assert!((a.max_drawdown_pct - b.max_drawdown_pct).abs() < 1e-12);
    }

    /// Two windows that tile share a seam, and the seam must not become an
    /// extra return. Equity 100, 101, 103, 106 pooled as `[1,2)` + `[2,4)` is
    /// the same series as `[1,4)`: three returns, not four.
    #[test]
    fn tiled_windows_do_not_invent_a_return_at_the_seam() {
        let eq = curve(&[100, 101, 103, 106]);
        let trace = RunTrace::new(&eq, &[], &[]);
        let cash = 1_000_000_000_000;
        let split = trace.pooled(&[1..2, 2..4], cash, 252.0);
        let whole = trace.window(1..4, cash, 252.0);
        assert_eq!(split, whole);
    }

    /// Pooling stitches deltas, so the training window between two test
    /// windows contributes nothing — not even a drawdown.
    ///
    /// Equity 100, 100, 110, 10, 20, 25 with test windows `[2,3)` and
    /// `[4,6)`. Window 1 delta: +10. The collapse to 10 at bar 3 is inside
    /// the training window and is skipped. Window 2 deltas: +10 then +5.
    /// Pooled on $1,000: 1000 -> 1010 -> (anchor) -> 1020 -> 1025. No
    /// drawdown at all, and +2.5 % total.
    #[test]
    fn pooling_ignores_what_happened_between_the_windows() {
        let eq = curve(&[100, 100, 110, 10, 20, 25]);
        let trace = RunTrace::new(&eq, &[], &[]);
        let s = trace.pooled(&[2..3, 4..6], 1_000_000_000_000, 252.0);
        assert_eq!(s.final_equity_nano_usd, 1_025_000_000_000);
        assert!((s.total_return_pct - 2.5).abs() < 1e-9);
        assert!(
            s.max_drawdown_pct.abs() < 1e-12,
            "the −100 was out of sample"
        );
    }

    /// Trades and fees are counted in the window that contains the bar they
    /// landed on, and nowhere else.
    #[test]
    fn trades_and_fees_belong_to_one_window_each() {
        let eq = curve(&[100, 101, 102, 103, 104, 105]);
        let trades = [
            ClosedTrade {
                closed_ts: Ts(1),
                net_nano_usd: 5_000_000_000,
                opened_ts: Ts(0),
                mae_nano_usd: -1_000_000_000,
                mfe_nano_usd: 6_000_000_000,
            },
            ClosedTrade {
                closed_ts: Ts(4),
                net_nano_usd: -2_000_000_000,
                opened_ts: Ts(3),
                mae_nano_usd: -3_000_000_000,
                mfe_nano_usd: 0,
            },
        ];
        let fees = [
            FeeEvent {
                ts: Ts(1),
                fee_nano_usd: 1_000_000_000,
            },
            FeeEvent {
                ts: Ts(4),
                fee_nano_usd: 1_000_000_000,
            },
        ];
        let trace = RunTrace::new(&eq, &trades, &fees);
        let cash = 1_000_000_000_000;

        let early = trace.window(0..3, cash, 252.0);
        assert_eq!(early.round_trips, 1);
        assert_eq!(early.fees_nano_usd, 1_000_000_000);
        assert_eq!(early.win_rate, Some(1.0));

        let late = trace.window(3..6, cash, 252.0);
        assert_eq!(late.round_trips, 1);
        assert_eq!(late.fees_nano_usd, 1_000_000_000);
        assert_eq!(late.win_rate, Some(0.0));

        // And the two windows together account for the whole run.
        let all = trace.pooled(&[0..3, 3..6], cash, 252.0);
        assert_eq!(all.round_trips, 2);
        assert_eq!(all.fees_nano_usd, 2_000_000_000);
    }

    /// A window at the very start has no anchor to reach back to, and must
    /// not reach back to bar `usize::MAX`.
    #[test]
    fn the_first_window_of_a_run_needs_no_anchor() {
        let eq = curve(&[100, 110, 120]);
        let s = RunTrace::new(&eq, &[], &[]).window(0..3, 1_000_000_000_000, 252.0);
        assert_eq!(s.final_equity_nano_usd, 1_020_000_000_000);
    }
}
