//! Position and cash accounting, futures-style: realized PnL and fees settle
//! into cash; equity = cash + unrealized PnL of the open position. Margin is
//! not modeled in v0 (documented limitation, revisit at M2).
//!
//! All money is integer nano-USD. There is deliberately no `f64` anywhere in
//! this file (CLAUDE.md §2.3).

use crucible_core::prelude::*;

/// Single-instrument portfolio. The cross-sectional, multi-instrument
/// portfolio arrives with SSRN-style strategies (post-M4).
#[derive(Debug)]
pub struct Portfolio {
    spec: ContractSpec,
    /// Signed position in contracts: long > 0, short < 0.
    position: i64,
    /// Volume-weighted average entry; meaningful only while `position != 0`.
    avg_entry: Price,
    cash_nano_usd: NanoUsd,
    fees_nano_usd: NanoUsd,
    realized_nano_usd: NanoUsd,
    last_mark: Option<Price>,
    /// Net realized PnL (fees included) of the round-trip currently open.
    episode_net: NanoUsd,
    /// Net PnL of each completed round-trip (position returned to flat).
    closed_trades: Vec<NanoUsd>,
}

impl Portfolio {
    #[must_use]
    pub fn new(spec: ContractSpec, initial_cash_nano_usd: NanoUsd) -> Self {
        Portfolio {
            spec,
            position: 0,
            avg_entry: Price::ZERO,
            cash_nano_usd: initial_cash_nano_usd,
            fees_nano_usd: 0,
            realized_nano_usd: 0,
            last_mark: None,
            episode_net: 0,
            closed_trades: Vec::new(),
        }
    }

    /// Apply an execution. Handles increase, partial close, full close, and
    /// flip (close-then-reopen through zero) in one pass.
    pub fn apply_fill(&mut self, fill: &Fill) {
        let fill_signed = fill.side.sign() * fill.qty.abs().as_i64();
        debug_assert!(fill_signed != 0, "zero-qty fill");

        self.cash_nano_usd -= fill.fee_nano_usd;
        self.fees_nano_usd += fill.fee_nano_usd;
        self.episode_net -= fill.fee_nano_usd;

        let mut remaining = fill_signed;

        // Closing leg: fill direction opposes the open position.
        if self.position != 0 && self.position.signum() != fill_signed.signum() {
            let closable = self.position.abs().min(remaining.abs());
            let closed_signed = self.position.signum() * closable;
            let pnl = self
                .spec
                .pnl_nano_usd(fill.price - self.avg_entry, closed_signed);
            self.realized_nano_usd += pnl;
            self.cash_nano_usd += pnl;
            self.episode_net += pnl;
            self.position -= closed_signed;
            remaining += closed_signed; // consumed by the close

            if self.position == 0 {
                self.closed_trades.push(self.episode_net);
                self.episode_net = 0;
                self.avg_entry = Price::ZERO;
            }
        }

        // Opening/increasing leg.
        if remaining != 0 {
            if self.position == 0 {
                self.avg_entry = fill.price;
                self.position = remaining;
            } else {
                // Same direction: volume-weighted average entry, computed in
                // integer nanopoints.
                let old_abs = self.position.abs();
                let add_abs = remaining.abs();
                let total = old_abs + add_abs;
                let weighted =
                    self.avg_entry.as_nanos() * old_abs + fill.price.as_nanos() * add_abs;
                self.avg_entry = Price::from_nanos(weighted / total);
                self.position += remaining;
            }
        }
    }

    /// Mark the open position to `price` (typically the bar close).
    pub fn mark(&mut self, price: Price) {
        self.last_mark = Some(price);
    }

    #[must_use]
    pub fn unrealized_nano_usd(&self) -> NanoUsd {
        match (self.position, self.last_mark) {
            (0, _) | (_, None) => 0,
            (pos, Some(mark)) => self.spec.pnl_nano_usd(mark - self.avg_entry, pos),
        }
    }

    #[must_use]
    pub fn equity_nano_usd(&self) -> NanoUsd {
        self.cash_nano_usd + self.unrealized_nano_usd()
    }

    #[must_use]
    pub fn view(&self) -> PortfolioView {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "positions are small i64 by construction"
        )]
        PortfolioView {
            position: Qty(self.position as i32),
            avg_entry: (self.position != 0).then_some(self.avg_entry),
            cash_nano_usd: self.cash_nano_usd,
            equity_nano_usd: self.equity_nano_usd(),
        }
    }

    #[must_use]
    pub fn closed_trades(&self) -> &[NanoUsd] {
        &self.closed_trades
    }

    #[must_use]
    pub fn fees_nano_usd(&self) -> NanoUsd {
        self.fees_nano_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn es_spec() -> ContractSpec {
        ContractSpec {
            instrument: InstrumentId::new("SYN:ES"),
            tick: Price::from_points_f64_lossy(0.25),
            point_value_usd: 50,
        }
    }

    fn fill(side: Side, qty: i32, points: f64, fee_usd: i64) -> Fill {
        Fill {
            order_id: 0,
            ts: Ts(0),
            side,
            qty: Qty(qty),
            price: Price::from_points_f64_lossy(points),
            fee_nano_usd: fee_usd * 1_000_000_000,
        }
    }

    /// Long 1 @100 -> sell 2 @110 (flip) -> buy 1 @105 (close short).
    /// Round trip 1: +10 pts * $50 = $500. Round trip 2 (short 110->105):
    /// +5 pts * $50 = $250. Zero fees for arithmetic clarity.
    #[test]
    fn flip_accounting() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill(Side::Buy, 1, 100.0, 0));
        assert_eq!(p.view().position, Qty(1));

        p.apply_fill(&fill(Side::Sell, 2, 110.0, 0));
        let v = p.view();
        assert_eq!(v.position, Qty(-1));
        assert_eq!(v.avg_entry, Some(Price::from_points(110)));
        assert_eq!(p.closed_trades(), &[500_000_000_000]);

        p.apply_fill(&fill(Side::Buy, 1, 105.0, 0));
        let v = p.view();
        assert_eq!(v.position, Qty(0));
        assert_eq!(p.closed_trades(), &[500_000_000_000, 250_000_000_000]);
        assert_eq!(v.cash_nano_usd, 750_000_000_000); // $750 realized
        assert_eq!(v.equity_nano_usd, 750_000_000_000);
    }

    #[test]
    fn fees_hit_cash_and_episode() {
        let mut p = Portfolio::new(es_spec(), 100_000_000_000_000); // $100k
        p.apply_fill(&fill(Side::Buy, 1, 100.0, 2)); // $2 fee
        p.apply_fill(&fill(Side::Sell, 1, 101.0, 2)); // +1pt = $50, $2 fee
        assert_eq!(p.closed_trades(), &[46_000_000_000]); // 50 - 4
        assert_eq!(p.view().equity_nano_usd, 100_046_000_000_000);
        assert_eq!(p.fees_nano_usd(), 4_000_000_000);
    }

    #[test]
    fn unrealized_marks_to_market() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill(Side::Sell, 2, 200.0, 0)); // short 2 @200
        p.mark(Price::from_points(195)); // 5 pts in favor * 2 * $50 = $500
        assert_eq!(p.unrealized_nano_usd(), 500_000_000_000);
        p.mark(Price::from_points(205)); // 5 pts against
        assert_eq!(p.unrealized_nano_usd(), -500_000_000_000);
    }
}
