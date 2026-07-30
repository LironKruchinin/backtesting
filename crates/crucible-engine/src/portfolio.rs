//! Position and cash accounting, futures-style: realized PnL and fees settle
//! into cash; equity = cash + unrealized PnL of the open position. Margin is
//! not modeled in v0 (documented limitation, revisit at M2).
//!
//! All money is integer nano-USD. There is deliberately no `f64` anywhere in
//! this file (CLAUDE.md §2.3).

use crucible_core::prelude::*;

/// A completed round-trip: the position left flat and returned to flat.
///
/// The timestamp is what makes a *windowed* report possible. A run summary
/// only ever needed the PnL, but a walk-forward fold that says "14 round
/// trips" while counting the whole run's trades is a number wearing another
/// window's label, which is precisely the failure the fold table exists to
/// prevent.
///
/// **Attribution is by close, not by open.** A round-trip opened inside a
/// training window and closed inside the test window counts as a test-window
/// trade, because `closed_ts` is when its PnL settles into cash. The
/// mark-to-market equity series already splits the *money* correctly across
/// the boundary (each window keeps the marks that happened inside it); this
/// field only decides which window gets the trade *count*. The excursions
/// below inherit that attribution unchanged, so a fold's excursion
/// distribution and its trade count describe the same trades.
///
/// **The excursions are series 3 of `docs/ACCOUNT_EVAL_SPEC.md` §3.4**: the
/// near-miss distribution that separates a strategy which survives an account
/// from one that survived a sample. They are measured on the episode's
/// realized-plus-unrealized PnL at every mark the position was open for, so
/// they are bar-close excursions — a *lower bound* on what the position
/// actually endured between two closes (§3.3.2, same caveat as every other
/// number sampled on a mark grid).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedTrade {
    /// `avail_ts` of the fill that returned the position to flat.
    pub closed_ts: Ts,
    /// Net PnL of the round-trip, fees included.
    pub net_nano_usd: NanoUsd,
    /// `avail_ts` of the fill that left flat and opened this round-trip.
    ///
    /// A flip through zero closes one episode and opens another at the same
    /// instant, so two round-trips can share a boundary timestamp. That is the
    /// same `apply_fill` boundary the PnL is cut at, not a second convention.
    pub opened_ts: Ts,
    /// Maximum **adverse** excursion: the lowest realized-plus-unrealized PnL
    /// the episode reached at any mark, in nano-USD. Never positive — the
    /// episode's own opening instant, at zero, is the seed, so a trade that
    /// never showed a loss at a mark reports `0`.
    pub mae_nano_usd: NanoUsd,
    /// Maximum **favourable** excursion, the mirror of `mae_nano_usd`. Never
    /// negative.
    pub mfe_nano_usd: NanoUsd,
}

/// A commission charged at an instant.
///
/// Recorded per fill, and only when nonzero — under `FreeFills` there are no
/// fee events at all, which is the honest representation of an execution
/// assumption that charges nothing (D-0006). Costs are visible per window
/// (CLAUDE.md §2.4), not as one whole-run total reported under every fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeEvent {
    /// `avail_ts` of the fill that incurred it.
    pub ts: Ts,
    /// Commission and exchange fees for that fill.
    pub fee_nano_usd: NanoUsd,
}

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
    /// `avail_ts` of the fill that opened the round-trip currently open.
    episode_opened_ts: Option<Ts>,
    /// Running extremes of `episode_net + unrealized` over the marks the
    /// current round-trip has been open for. Seeded at zero, which is the
    /// account's position the instant before the opening fill.
    episode_mae_nano_usd: NanoUsd,
    episode_mfe_nano_usd: NanoUsd,
    /// Each completed round-trip (position returned to flat), in close order.
    closed_trades: Vec<ClosedTrade>,
    /// Each nonzero commission, in fill order.
    fee_events: Vec<FeeEvent>,
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
            episode_opened_ts: None,
            episode_mae_nano_usd: 0,
            episode_mfe_nano_usd: 0,
            closed_trades: Vec::new(),
            fee_events: Vec::new(),
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
        if fill.fee_nano_usd != 0 {
            self.fee_events.push(FeeEvent {
                ts: fill.ts,
                fee_nano_usd: fill.fee_nano_usd,
            });
        }

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
                self.closed_trades.push(ClosedTrade {
                    closed_ts: fill.ts,
                    net_nano_usd: self.episode_net,
                    opened_ts: self
                        .episode_opened_ts
                        .expect("INVARIANT: a position cannot close without having opened"),
                    mae_nano_usd: self.episode_mae_nano_usd,
                    mfe_nano_usd: self.episode_mfe_nano_usd,
                });
                self.episode_net = 0;
                self.episode_opened_ts = None;
                self.episode_mae_nano_usd = 0;
                self.episode_mfe_nano_usd = 0;
                self.avg_entry = Price::ZERO;
            }
        }

        // Opening/increasing leg.
        if remaining != 0 {
            if self.position == 0 {
                self.episode_opened_ts = Some(fill.ts);
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
    ///
    /// This is also where series 3 of the account-evaluation capture is
    /// sampled (`docs/ACCOUNT_EVAL_SPEC.md` §3.4): the round-trip's adverse
    /// and favourable excursions are two more running extremes on the episode
    /// lifecycle `episode_net` already tracks, updated here because this is
    /// the one place in the workspace where unrealized PnL is evaluated
    /// against a price. Guarded on an open position — a flat account has no
    /// excursion to have.
    ///
    /// The excursion is measured on `episode_net + unrealized`, i.e. realized
    /// PnL banked *within this episode* (a partial close, and every fee paid
    /// so far) plus the mark-to-market of what is still open. Anything less
    /// would report a scale-out's remaining leg as if the money already taken
    /// off the table had never existed.
    pub fn mark(&mut self, price: Price) {
        self.last_mark = Some(price);
        if self.position != 0 {
            let excursion = self.episode_net + self.unrealized_nano_usd();
            if excursion < self.episode_mae_nano_usd {
                self.episode_mae_nano_usd = excursion;
            }
            if excursion > self.episode_mfe_nano_usd {
                self.episode_mfe_nano_usd = excursion;
            }
        }
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

    /// The signed position: long > 0, short < 0.
    ///
    /// Separate from [`Portfolio::view`] because the replay loop asks this
    /// question after every fill — to decide whether a protective bracket
    /// still has anything to protect — and does not need equity marked to
    /// answer it.
    #[must_use]
    pub fn position(&self) -> Qty {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "positions are small i64 by construction"
        )]
        Qty(self.position as i32)
    }

    #[must_use]
    pub fn view(&self) -> PortfolioView {
        PortfolioView {
            position: self.position(),
            avg_entry: (self.position != 0).then_some(self.avg_entry),
            cash_nano_usd: self.cash_nano_usd,
            equity_nano_usd: self.equity_nano_usd(),
        }
    }

    #[must_use]
    pub fn closed_trades(&self) -> &[ClosedTrade] {
        &self.closed_trades
    }

    #[must_use]
    pub fn fee_events(&self) -> &[FeeEvent] {
        &self.fee_events
    }

    #[must_use]
    pub fn fees_nano_usd(&self) -> NanoUsd {
        self.fees_nano_usd
    }

    /// Consumes the portfolio, yielding its per-event records.
    ///
    /// The run is over by the time a caller wants these, and moving beats
    /// cloning a vector per run when the funnel is doing it once per combo.
    #[must_use]
    pub fn into_records(self) -> (Vec<ClosedTrade>, Vec<FeeEvent>) {
        (self.closed_trades, self.fee_events)
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
        fill_at(Ts(0), side, qty, points, fee_usd)
    }

    fn fill_at(ts: Ts, side: Side, qty: i32, points: f64, fee_usd: i64) -> Fill {
        Fill {
            order_id: 0,
            ts,
            side,
            qty: Qty(qty),
            price: Price::from_points_f64_lossy(points),
            fee_nano_usd: fee_usd * 1_000_000_000,
        }
    }

    /// Net PnL of each round-trip, dropping the timestamps — the shape the
    /// arithmetic assertions below care about.
    fn nets(p: &Portfolio) -> Vec<NanoUsd> {
        p.closed_trades().iter().map(|t| t.net_nano_usd).collect()
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
        assert_eq!(nets(&p), vec![500_000_000_000]);

        p.apply_fill(&fill(Side::Buy, 1, 105.0, 0));
        let v = p.view();
        assert_eq!(v.position, Qty(0));
        assert_eq!(nets(&p), vec![500_000_000_000, 250_000_000_000]);
        assert_eq!(v.cash_nano_usd, 750_000_000_000); // $750 realized
        assert_eq!(v.equity_nano_usd, 750_000_000_000);
    }

    #[test]
    fn fees_hit_cash_and_episode() {
        let mut p = Portfolio::new(es_spec(), 100_000_000_000_000); // $100k
        p.apply_fill(&fill(Side::Buy, 1, 100.0, 2)); // $2 fee
        p.apply_fill(&fill(Side::Sell, 1, 101.0, 2)); // +1pt = $50, $2 fee
        assert_eq!(nets(&p), vec![46_000_000_000]); // 50 - 4
        assert_eq!(p.view().equity_nano_usd, 100_046_000_000_000);
        assert_eq!(p.fees_nano_usd(), 4_000_000_000);
    }

    /// A round-trip is stamped with the fill that flattened it, and a fee with
    /// the fill that incurred it. Both are what lets a fold report the trades
    /// and costs that happened *inside* the window it names.
    #[test]
    fn records_carry_the_instant_they_happened() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill_at(Ts(10), Side::Buy, 1, 100.0, 2));
        p.apply_fill(&fill_at(Ts(40), Side::Sell, 1, 101.0, 2));
        assert_eq!(
            p.closed_trades(),
            &[ClosedTrade {
                closed_ts: Ts(40), // the CLOSING fill, not the opening one
                net_nano_usd: 46_000_000_000,
                opened_ts: Ts(10),
                // No mark happened while the position was open, so the
                // excursions are the seed. Not the round-trip's PnL: MAE/MFE
                // are sampled on marks, and this fixture has none.
                mae_nano_usd: 0,
                mfe_nano_usd: 0,
            }]
        );
        assert_eq!(
            p.fee_events(),
            &[
                FeeEvent {
                    ts: Ts(10),
                    fee_nano_usd: 2_000_000_000
                },
                FeeEvent {
                    ts: Ts(40),
                    fee_nano_usd: 2_000_000_000
                },
            ]
        );
    }

    /// `FreeFills` charges nothing, so there is nothing to record. An empty
    /// vector is the honest shape — not a run of zero-valued entries that a
    /// windowed cost report would have to filter out.
    #[test]
    fn a_costless_fill_records_no_fee_event() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill_at(Ts(10), Side::Buy, 1, 100.0, 0));
        assert!(p.fee_events().is_empty());
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

    /// MAE / MFE, hand-derived (`docs/ACCOUNT_EVAL_SPEC.md` §3.4).
    ///
    /// ES: $50/point, 1 contract, $2 fee per side. Buy 1 @ 100.00 at Ts(10),
    /// then mark at 103, 96, 101, then sell 1 @ 99.00 at Ts(50).
    ///
    /// The excursion at a mark is `episode_net + unrealized`, and
    /// `episode_net` is −$2 from the entry fee for every mark below:
    ///
    /// | mark | unrealized | excursion |
    /// |---|---|---|
    /// | 103 | +3 × 50 = +$150 | −2 + 150 = **+$148** |
    /// |  96 | −4 × 50 = −$200 | −2 − 200 = **−$202** |
    /// | 101 | +1 × 50 =  +$50 | −2 +  50 =  +$48 |
    ///
    /// so MFE = +$148 and MAE = −$202. Net PnL is a different number:
    /// (99 − 100) × 50 − 2 − 2 = −$54. The trade lost $54 while having been
    /// $148 up and $202 down — which is the whole reason the excursions are
    /// captured rather than inferred from the net.
    #[test]
    fn excursions_are_the_extremes_of_the_open_episode() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill_at(Ts(10), Side::Buy, 1, 100.0, 2));
        p.mark(Price::from_points(103));
        p.mark(Price::from_points(96));
        p.mark(Price::from_points(101));
        p.apply_fill(&fill_at(Ts(50), Side::Sell, 1, 99.0, 2));

        assert_eq!(
            p.closed_trades(),
            &[ClosedTrade {
                closed_ts: Ts(50),
                net_nano_usd: -54_000_000_000,
                opened_ts: Ts(10),
                mae_nano_usd: -202_000_000_000,
                mfe_nano_usd: 148_000_000_000,
            }]
        );
    }

    /// A trade that never showed a loss at any mark reports `mae = 0`, not its
    /// smallest gain, and vice versa. The seed is the account's position the
    /// instant before the opening fill, so the signs in the field docs hold by
    /// construction rather than by clamping afterwards.
    ///
    /// Buy 1 @ 100, no fee, marks at 101 and 104: excursions +$50 and +$200.
    #[test]
    fn a_one_sided_trade_reports_a_zero_on_the_side_it_never_visited() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill_at(Ts(1), Side::Buy, 1, 100.0, 0));
        p.mark(Price::from_points(101));
        p.mark(Price::from_points(104));
        p.apply_fill(&fill_at(Ts(9), Side::Sell, 1, 104.0, 0));
        let t = p.closed_trades()[0];
        assert_eq!(t.mae_nano_usd, 0);
        assert_eq!(t.mfe_nano_usd, 200_000_000_000);
        assert_eq!(t.net_nano_usd, 200_000_000_000);
    }

    /// A flip through zero closes one episode and opens another at the same
    /// instant, and the excursions reset with it — the second round-trip must
    /// not inherit the first one's drawdown.
    ///
    /// Long 1 @ 100, mark 90 (−$500), then sell 2 @ 110 (flip to short 1).
    /// Round-trip 1: net +$500, MAE −$500, MFE 0 — it was never up at a mark.
    /// Then mark 130: short 1 from 110 is −20 × 50 = −$1,000. Buy 1 @ 105
    /// closes: net (110 − 105) × 50 = +$250, MAE −$1,000, MFE 0.
    #[test]
    fn a_flip_resets_the_excursions_with_the_episode() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill_at(Ts(1), Side::Buy, 1, 100.0, 0));
        p.mark(Price::from_points(90));
        p.apply_fill(&fill_at(Ts(2), Side::Sell, 2, 110.0, 0));
        p.mark(Price::from_points(130));
        p.apply_fill(&fill_at(Ts(3), Side::Buy, 1, 105.0, 0));

        let t = p.closed_trades();
        assert_eq!(t[0].net_nano_usd, 500_000_000_000);
        assert_eq!(t[0].mae_nano_usd, -500_000_000_000);
        assert_eq!(t[0].mfe_nano_usd, 0);
        assert_eq!(t[0].opened_ts, Ts(1));

        assert_eq!(t[1].net_nano_usd, 250_000_000_000);
        assert_eq!(t[1].mae_nano_usd, -1_000_000_000_000);
        assert_eq!(t[1].mfe_nano_usd, 0);
        // The flipping fill both closed the first episode and opened the
        // second, so the two share an instant.
        assert_eq!(t[1].opened_ts, Ts(2));
        assert_eq!(t[0].closed_ts, Ts(2));
    }

    /// A scale-out banks money mid-episode, and the excursion has to see it.
    ///
    /// Buy 2 @ 100, no fees. Sell 1 @ 110: +10 × 50 = +$500 realized, still
    /// long 1. Mark at 100: unrealized on the remaining contract is 0, so the
    /// excursion is 500 + 0 = **+$500** — not zero. Mark at 95: unrealized
    /// −$250, excursion +$250. Sell 1 @ 95 closes: net 500 − 250 = +$250.
    ///
    /// Measuring the excursion on unrealized alone would report MFE = 0 for a
    /// trade that was $500 up, which is the bug this fixture exists to catch.
    #[test]
    fn an_excursion_includes_money_already_banked_in_the_episode() {
        let mut p = Portfolio::new(es_spec(), 0);
        p.apply_fill(&fill_at(Ts(1), Side::Buy, 2, 100.0, 0));
        p.apply_fill(&fill_at(Ts(2), Side::Sell, 1, 110.0, 0));
        p.mark(Price::from_points(100));
        p.mark(Price::from_points(95));
        p.apply_fill(&fill_at(Ts(4), Side::Sell, 1, 95.0, 0));

        let t = p.closed_trades()[0];
        assert_eq!(t.net_nano_usd, 250_000_000_000);
        assert_eq!(t.mfe_nano_usd, 500_000_000_000);
        assert_eq!(t.mae_nano_usd, 0);
        assert_eq!(t.opened_ts, Ts(1));
    }

    /// 40 bytes per round-trip is what makes "retain in full at every grain"
    /// affordable: 50,000 round-trips over 16 years is 2 MB.
    #[test]
    fn a_closed_trade_is_forty_bytes() {
        // Ts + NanoUsd + Ts + NanoUsd + NanoUsd = 5 × 8.
        assert_eq!(size_of::<ClosedTrade>(), 40);
    }
}
