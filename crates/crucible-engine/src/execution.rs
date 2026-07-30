//! Fill models — named, explicit execution assumptions.
//!
//! Every scorecard states which fill model produced its numbers. There is no
//! anonymous "default" execution: optimism must be visible and greppable.
//!
//! | Model             | Fill price (market orders)         | Intended use        |
//! |-------------------|------------------------------------|---------------------|
//! | `FreeFills`       | next bar open, zero fees           | Stage 0–1 screening |
//! | `SpreadCrossFills`| next bar open ± half-spread, fees  | Stage 2+ defaults   |
//! | `QueueSimFills`   | (M4) MBO-calibrated queue position | Stage 3 / limits    |
//!
//! `FreeFills` existing at all is a deliberate decision (docs/DECISIONS.md
//! D-0006): if a strategy cannot make money with free fills it can be killed
//! immediately and cheaply. It is a screening tool, never a result.
//!
//! ## Protective exits cost differently per leg
//!
//! A triggered bracket ([`crate::bracket`]) arrives here already resolved: the
//! engine's `stop_first_intrabar` convention has decided which leg fired and
//! at what price. What is left is the cost, and the two legs are not the same
//! trade (D-0069):
//!
//! | Model | stop leg | target leg |
//! |---|---|---|
//! | `FreeFills` | level (or opening print), no fee | level (or opening print), no fee |
//! | `SpreadCrossFills` | ± half-spread **against** you, plus fee | level, **no** spread, plus fee |
//!
//! A stop becomes a market order the instant it is touched, so it crosses the
//! spread like any other market order. A target is a resting limit that the
//! market came to, so it does not — the price *is* the level. The optimism
//! that remains is queue position: under bar data there is no way to know
//! whether a limit at the level would actually have been filled, which is the
//! same reason `OrderKind::Limit` stays out of the engine until M4's
//! `queue_sim`. The bracket convention answers it with the strictest available
//! reading (a print *at* the target is not a fill; only a print through it
//! is), and that is a floor on the pessimism, not a proof.

use crucible_core::prelude::*;

/// Zero-cost fills at the next event's open. **Stage 0–1 screening only** —
/// numbers produced under `FreeFills` are upper bounds, not results.
#[derive(Debug, Default, Clone, Copy)]
pub struct FreeFills;

impl FillModel for FreeFills {
    fn fill(
        &mut self,
        order: &Order,
        next_event: &MarketEvent,
        _spec: &ContractSpec,
    ) -> Option<Fill> {
        let MarketEvent::Bar(bar) = next_event;
        Some(Fill {
            order_id: order.id,
            ts: next_event.avail_ts(),
            side: order.side,
            qty: order.qty.abs(),
            price: bar.open,
            fee_nano_usd: 0,
        })
    }

    /// Free means free on the way out too: the level the convention resolved,
    /// no spread on the stop leg, no fee on either. Anything else would make
    /// `FreeFills` a half-costed model, which is worse than a frankly
    /// uncosted one — the whole point is that it is an upper bound nobody can
    /// mistake for a result (D-0006).
    fn fill_protective_exit(&mut self, exit: &ProtectiveExit, spec: &ContractSpec) -> Fill {
        Fill {
            order_id: exit.order_id,
            ts: exit.ts,
            side: exit.side,
            qty: exit.qty.abs(),
            price: exit.touch_price.round_to_tick(spec.tick),
            fee_nano_usd: 0,
        }
    }
}

/// Market orders cross the spread: fill at next open shifted against you by
/// the modelled half-spread, plus a per-contract fee. Both are *inputs* today;
/// milestone M4 calibrates them from the L1/MBO archive instead of hand-picking
/// them.
///
/// # Why the half-spread is nanopoints and not ticks (D-0073)
///
/// §2.4 makes the cost-sensitivity sweep — **0 / 0.5 / 1 / 2 ticks** —
/// mandatory on every scorecard, and half a tick is not a tick count. Carrying
/// the field as an `i64` tick multiple made the most decision-relevant level in
/// that sweep the one the type could not express, so the field carries the
/// *distance* and [`SpreadCrossFills::from_tick_halves`] is where a sweep level
/// becomes one.
///
/// The consequence, stated because it is visible in a fill price: at a whole
/// number of ticks the fill lands on the tick grid, because a bar's open does;
/// at half a tick it deliberately does not. That is what a fractional
/// half-spread *is* — an average over sessions where the spread was one tick
/// and sessions where the trade printed at the touch — and rounding it back to
/// the grid would turn the sweep's middle level into either 0 or 1 and quietly
/// delete the row.
#[derive(Debug, Clone, Copy)]
pub struct SpreadCrossFills {
    /// Distance the fill price is shifted against the order, in nanopoints.
    pub half_spread_nano_points: i64,
    /// Commission per contract per side.
    pub fee_per_contract_nano_usd: NanoUsd,
}

impl SpreadCrossFills {
    /// A whole number of ticks of half-spread — what a config's
    /// `execution.half_spread_ticks` means.
    #[must_use]
    pub const fn from_ticks(ticks: i64, tick: Price) -> SpreadCrossFills {
        SpreadCrossFills {
            half_spread_nano_points: ticks * tick.as_nanos(),
            fee_per_contract_nano_usd: 0,
        }
    }

    /// A half-spread in **half-ticks**: the cost sweep's 0 / 0.5 / 1 / 2 levels
    /// are `0 / 1 / 2 / 4`.
    ///
    /// Half-ticks rather than an `f64` tick count on purpose: the sweep's
    /// levels are a fixed, config-declared list, and an axis whose points are
    /// reached by floating-point arithmetic is the thing D-0060 refused for
    /// parameter axes. Integer half-ticks are exact for every level §2.4 names.
    #[must_use]
    pub const fn from_tick_halves(half_ticks: i64, tick: Price) -> SpreadCrossFills {
        SpreadCrossFills {
            half_spread_nano_points: half_ticks * tick.as_nanos() / 2,
            fee_per_contract_nano_usd: 0,
        }
    }

    /// The same model, charging `fee` per contract per side.
    #[must_use]
    pub const fn with_fee(self, fee_per_contract_nano_usd: NanoUsd) -> SpreadCrossFills {
        SpreadCrossFills {
            fee_per_contract_nano_usd,
            ..self
        }
    }
}

impl FillModel for SpreadCrossFills {
    fn fill(
        &mut self,
        order: &Order,
        next_event: &MarketEvent,
        _spec: &ContractSpec,
    ) -> Option<Fill> {
        let MarketEvent::Bar(bar) = next_event;
        let slip = Price::from_nanos(self.half_spread_nano_points);
        let price = match order.side {
            Side::Buy => bar.open + slip,
            Side::Sell => bar.open - slip,
        };
        Some(Fill {
            order_id: order.id,
            ts: next_event.avail_ts(),
            side: order.side,
            qty: order.qty.abs(),
            price,
            fee_nano_usd: self.fee_per_contract_nano_usd * order.qty.abs().as_i64(),
        })
    }

    /// A stop crosses the spread; a target does not.
    ///
    /// The stop leg *is* a market order from the instant it is touched, so it
    /// pays the same half-spread as any other market order — including on a
    /// gapped stop, where the opening print is already bad and the spread
    /// makes it worse, which is what happens when everyone's stop triggers at
    /// once. The target leg was a resting limit the market came to, so its
    /// price is the level itself. Both pay the commission: the exchange does
    /// not care which of your orders it filled.
    fn fill_protective_exit(&mut self, exit: &ProtectiveExit, _spec: &ContractSpec) -> Fill {
        let price = match exit.leg {
            BracketLeg::Stop => {
                let slip = Price::from_nanos(self.half_spread_nano_points);
                match exit.side {
                    Side::Buy => exit.touch_price + slip,
                    Side::Sell => exit.touch_price - slip,
                }
            }
            BracketLeg::Target => exit.touch_price,
        };
        Fill {
            order_id: exit.order_id,
            ts: exit.ts,
            side: exit.side,
            qty: exit.qty.abs(),
            price,
            fee_nano_usd: self.fee_per_contract_nano_usd * exit.qty.abs().as_i64(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (ContractSpec, Order, MarketEvent) {
        let spec = ContractSpec {
            instrument: InstrumentId::new("SYN:ES"),
            tick: Price::from_points_f64_lossy(0.25),
            point_value_usd: 50,
        };
        let order = Order {
            id: 7,
            instrument: spec.instrument.clone(),
            side: Side::Buy,
            qty: Qty(2),
            kind: OrderKind::Market,
            placed_ts: Ts(0),
            bracket: None,
        };
        let bar = Bar {
            instrument: spec.instrument.clone(),
            tf: TimeFrame::M1,
            ts_open: Ts(60_000_000_000),
            open: Price::from_points_f64_lossy(5000.00),
            high: Price::from_points_f64_lossy(5001.00),
            low: Price::from_points_f64_lossy(4999.00),
            close: Price::from_points_f64_lossy(5000.50),
            volume: 100,
        };
        (spec, order, MarketEvent::Bar(bar))
    }

    #[test]
    fn free_fills_at_open_no_fee() {
        let (spec, order, ev) = setup();
        let f = FreeFills
            .fill(&order, &ev, &spec)
            .expect("market order fills");
        assert_eq!(f.price, Price::from_points(5000));
        assert_eq!(f.fee_nano_usd, 0);
    }

    #[test]
    fn spread_cross_charges_half_spread_and_fees() {
        let (spec, order, ev) = setup();
        let mut m = SpreadCrossFills::from_ticks(1, spec.tick).with_fee(1_250_000_000); // $1.25
        let f = m.fill(&order, &ev, &spec).expect("market order fills");
        assert_eq!(f.price, Price::from_points_f64_lossy(5000.25)); // buy pays up
        assert_eq!(f.fee_nano_usd, 2_500_000_000); // 2 contracts
    }

    /// The cost sweep's middle level, which an `i64` tick count could not
    /// express (D-0073). Tick 0.25 pt ⇒ half a tick is 0.125 pt, so a buy at a
    /// 5000.00 open fills at 5000.125 — deliberately **off** the tick grid,
    /// because half a tick is an average cost and no single print pays it.
    #[test]
    fn a_half_tick_half_spread_is_exact_and_off_the_tick_grid() {
        let (spec, order, ev) = setup();
        let mut m = SpreadCrossFills::from_tick_halves(1, spec.tick);
        let f = m.fill(&order, &ev, &spec).expect("market order fills");
        assert_eq!(f.price.as_nanos(), 5_000_125_000_000);
        assert_ne!(f.price, f.price.round_to_tick(spec.tick));
        // And the four sweep levels §2.4 mandates are 0 / 1 / 2 / 4 half-ticks.
        let nanos = |h| SpreadCrossFills::from_tick_halves(h, spec.tick).half_spread_nano_points;
        assert_eq!(
            [nanos(0), nanos(1), nanos(2), nanos(4)],
            [0, 125_000_000, 250_000_000, 500_000_000]
        );
    }

    /// A whole number of ticks lands back on the grid, so nothing that existed
    /// before the field changed shape moves. Bar opens are tick-aligned, and
    /// aligned ± k·tick is aligned.
    #[test]
    fn a_whole_tick_half_spread_stays_on_the_tick_grid() {
        let (spec, order, ev) = setup();
        for ticks in [0, 1, 2] {
            let mut m = SpreadCrossFills::from_ticks(ticks, spec.tick);
            let f = m.fill(&order, &ev, &spec).expect("market order fills");
            assert_eq!(f.price, f.price.round_to_tick(spec.tick), "{ticks} ticks");
        }
    }

    /// A long's protective exit: `side` is Sell, because selling is what
    /// flattens a long.
    fn exit(leg: BracketLeg, touch_points: f64) -> ProtectiveExit {
        ProtectiveExit {
            order_id: 7,
            ts: Ts(60_000_000_000),
            side: Side::Sell,
            qty: Qty(2),
            leg,
            touch_price: Price::from_points_f64_lossy(touch_points),
            gapped: false,
        }
    }

    #[test]
    fn free_fills_prices_both_legs_at_the_level_with_no_fee() {
        let (spec, _, _) = setup();
        let stop = FreeFills.fill_protective_exit(&exit(BracketLeg::Stop, 4998.00), &spec);
        assert_eq!(stop.price, Price::from_points(4998));
        assert_eq!(stop.fee_nano_usd, 0);
        let target = FreeFills.fill_protective_exit(&exit(BracketLeg::Target, 5003.00), &spec);
        assert_eq!(target.price, Price::from_points(5003));
        assert_eq!(target.fee_nano_usd, 0);
    }

    /// Hand arithmetic, tick 0.25, half-spread 1 tick, $1.25/contract/side,
    /// 2 contracts:
    /// - stop at 4998.00, exiting a long by selling → 4998.00 − 0.25 = 4997.75
    /// - target at 5003.00 → 5003.00, no spread (it was the resting side)
    /// - fee either way → 2 × $1.25 = $2.50 = 2_500_000_000 nano
    #[test]
    fn spread_cross_charges_the_spread_on_the_stop_and_not_on_the_target() {
        let (spec, _, _) = setup();
        let mut m = SpreadCrossFills::from_ticks(1, spec.tick).with_fee(1_250_000_000);
        let stop = m.fill_protective_exit(&exit(BracketLeg::Stop, 4998.00), &spec);
        assert_eq!(stop.price, Price::from_points_f64_lossy(4997.75));
        assert_eq!(stop.fee_nano_usd, 2_500_000_000);

        let target = m.fill_protective_exit(&exit(BracketLeg::Target, 5003.00), &spec);
        assert_eq!(target.price, Price::from_points(5003));
        assert_eq!(target.fee_nano_usd, 2_500_000_000);
    }

    /// A short's stop exit buys, so the half-spread is paid the other way.
    #[test]
    fn a_shorts_stop_pays_the_spread_upwards() {
        let (spec, _, _) = setup();
        let mut m = SpreadCrossFills::from_ticks(1, spec.tick);
        let mut e = exit(BracketLeg::Stop, 5002.00);
        e.side = Side::Buy;
        let f = m.fill_protective_exit(&e, &spec);
        assert_eq!(f.price, Price::from_points_f64_lossy(5002.25));
    }
}
