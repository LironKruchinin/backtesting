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
}

/// Market orders cross the spread: fill at next open shifted against you by
/// `half_spread_ticks`, plus a per-contract fee. The half-spread and fee are
/// *inputs* today; milestone M4 calibrates them from the L1/MBO archive
/// instead of hand-picking them.
#[derive(Debug, Clone, Copy)]
pub struct SpreadCrossFills {
    pub half_spread_ticks: i64,
    pub fee_per_contract_nano_usd: NanoUsd,
}

impl FillModel for SpreadCrossFills {
    fn fill(
        &mut self,
        order: &Order,
        next_event: &MarketEvent,
        spec: &ContractSpec,
    ) -> Option<Fill> {
        let MarketEvent::Bar(bar) = next_event;
        let slip = Price::from_nanos(self.half_spread_ticks * spec.tick.as_nanos());
        let price = match order.side {
            Side::Buy => bar.open + slip,
            Side::Sell => bar.open - slip,
        };
        Some(Fill {
            order_id: order.id,
            ts: next_event.avail_ts(),
            side: order.side,
            qty: order.qty.abs(),
            price: price.round_to_tick(spec.tick),
            fee_nano_usd: self.fee_per_contract_nano_usd * order.qty.abs().as_i64(),
        })
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
        let mut m = SpreadCrossFills {
            half_spread_ticks: 1,
            fee_per_contract_nano_usd: 1_250_000_000, // $1.25
        };
        let f = m.fill(&order, &ev, &spec).expect("market order fills");
        assert_eq!(f.price, Price::from_points_f64_lossy(5000.25)); // buy pays up
        assert_eq!(f.fee_nano_usd, 2_500_000_000); // 2 contracts
    }
}
