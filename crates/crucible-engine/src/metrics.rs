//! Run summary statistics.
//!
//! Unit rule (CLAUDE.md §2.3): accounting stays integer nano-USD; this module
//! is the boundary where results convert to `f64` for *statistics* — ratios,
//! drawdowns, Sharpe. Statistics never flow back into accounting.
//!
//! The per-run Sharpe here is deliberately naive (per-bar returns, flat
//! annualization). Research-grade inference — deflated Sharpe, PBO,
//! permutation p-values — lives in `crucible-funnel::stats` and operates
//! across runs, not inside one.

use crucible_core::prelude::*;

use crate::portfolio::ClosedTrade;

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub initial_equity_nano_usd: NanoUsd,
    pub final_equity_nano_usd: NanoUsd,
    pub total_return_pct: f64,
    pub max_drawdown_pct: f64,
    pub round_trips: usize,
    /// Fraction of round-trips with positive net PnL; `None` if no trades.
    pub win_rate: Option<f64>,
    /// Naive annualized Sharpe of per-bar equity returns; `None` if the
    /// return series is empty or has zero variance.
    pub sharpe_naive: Option<f64>,
    pub fees_nano_usd: NanoUsd,
}

impl Summary {
    /// `bars_per_year` sets Sharpe annualization (e.g. ≈347,760 for 1m bars
    /// on a 23h/252-day futures session).
    #[must_use]
    pub fn compute(
        equity: &[(Ts, NanoUsd)],
        closed_trades: &[ClosedTrade],
        fees_nano_usd: NanoUsd,
        bars_per_year: f64,
    ) -> Summary {
        let initial = equity.first().map_or(0, |&(_, e)| e);
        let final_ = equity.last().map_or(0, |&(_, e)| e);

        let total_return_pct = if initial != 0 {
            (nano_usd_to_f64(final_) / nano_usd_to_f64(initial) - 1.0) * 100.0
        } else {
            0.0
        };

        // Max drawdown over the equity curve.
        let mut peak = f64::MIN;
        let mut max_dd = 0.0_f64;
        for &(_, e) in equity {
            let e = nano_usd_to_f64(e);
            peak = peak.max(e);
            if peak > 0.0 {
                max_dd = max_dd.max((peak - e) / peak);
            }
        }

        // Per-bar simple returns for the naive Sharpe.
        let mut rets = Vec::with_capacity(equity.len().saturating_sub(1));
        for w in equity.windows(2) {
            let prev = nano_usd_to_f64(w[0].1);
            let curr = nano_usd_to_f64(w[1].1);
            if prev > 0.0 {
                rets.push(curr / prev - 1.0);
            }
        }
        let sharpe_naive = sharpe(&rets, bars_per_year);

        let wins = closed_trades.iter().filter(|t| t.net_nano_usd > 0).count();
        let win_rate = if closed_trades.is_empty() {
            None
        } else {
            #[expect(clippy::cast_precision_loss, reason = "trade counts are small")]
            Some(wins as f64 / closed_trades.len() as f64)
        };

        Summary {
            initial_equity_nano_usd: initial,
            final_equity_nano_usd: final_,
            total_return_pct,
            max_drawdown_pct: max_dd * 100.0,
            round_trips: closed_trades.len(),
            win_rate,
            sharpe_naive,
            fees_nano_usd,
        }
    }
}

fn sharpe(rets: &[f64], periods_per_year: f64) -> Option<f64> {
    if rets.len() < 2 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "bar counts fit f64 exactly at our scales"
    )]
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let sd = var.sqrt();
    if sd == 0.0 {
        return None;
    }
    Some(mean / sd * periods_per_year.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(points: &[i64]) -> Vec<(Ts, NanoUsd)> {
        points
            .iter()
            .enumerate()
            .map(|(i, &e)| (Ts(i as i64), e * 1_000_000_000))
            .collect()
    }

    fn trades(nets: &[NanoUsd]) -> Vec<ClosedTrade> {
        nets.iter()
            .enumerate()
            .map(|(i, &net)| ClosedTrade {
                closed_ts: Ts(i as i64),
                net_nano_usd: net,
                direction: Side::Buy,
                opened_ts: Ts(i as i64),
                mae_nano_usd: net.min(0),
                mfe_nano_usd: net.max(0),
            })
            .collect()
    }

    #[test]
    fn drawdown_known_series() {
        // 100 -> 120 -> 90 -> 130: max dd = (120-90)/120 = 25%
        let s = Summary::compute(&eq(&[100, 120, 90, 130]), &[], 0, 1.0);
        assert!((s.max_drawdown_pct - 25.0).abs() < 1e-9);
        assert!((s.total_return_pct - 30.0).abs() < 1e-9);
    }

    #[test]
    fn win_rate_counts_positive_net_trades() {
        let s = Summary::compute(&eq(&[100, 104]), &trades(&[5, -3, 2, 0]), 0, 1.0);
        assert_eq!(s.round_trips, 4);
        let wr = s.win_rate.expect("has trades");
        assert!((wr - 0.5).abs() < 1e-9); // 2 of 4 (zero is not a win)
    }

    #[test]
    fn sharpe_none_on_flat_curve() {
        let s = Summary::compute(&eq(&[100, 100, 100]), &[], 0, 252.0);
        assert!(s.sharpe_naive.is_none());
    }
}
