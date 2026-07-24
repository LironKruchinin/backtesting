//! The event loop. Read this file before touching anything else in the
//! engine — the ordering rules here ARE the no-lookahead guarantee.
//!
//! Per-event processing order (never reorder these steps):
//! 1. **Fills.** Pending orders whose `placed_ts` is *strictly before* this
//!    event's `avail_ts` are offered to the fill model. An order can never
//!    fill against the event that triggered it.
//! 2. **Mark.** The portfolio marks to this bar's close; one equity point is
//!    recorded per bar.
//! 3. **Decide.** The strategy sees the event and may emit intents, which
//!    become orders stamped `placed_ts = this event's avail_ts`.
//!
//! The loop also enforces the Feed contract (nondecreasing `avail_ts`) and
//! aborts the run on violation — a misordered feed corrupts every number
//! downstream, so it is an error, not a warning.

use std::collections::VecDeque;

use crucible_core::prelude::*;

use crate::metrics::Summary;
use crate::portfolio::Portfolio;

#[derive(Debug, Clone)]
pub struct BacktestParams {
    pub initial_cash_nano_usd: NanoUsd,
    /// Sharpe annualization factor for the naive per-run Sharpe.
    pub bars_per_year: f64,
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    /// One `(avail_ts, equity)` point per bar, in nano-USD.
    pub equity: Vec<(Ts, NanoUsd)>,
    pub summary: Summary,
    pub n_fills: usize,
    /// Orders still pending when the feed ended (cancelled, never filled).
    pub cancelled_at_eof: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The feed emitted events with decreasing availability time.
    OutOfOrderFeed { prev: Ts, next: Ts },
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::OutOfOrderFeed { prev, next } => write!(
                f,
                "feed violated ordering contract: {next} after {prev} (events must be nondecreasing in avail_ts)"
            ),
        }
    }
}

impl std::error::Error for EngineError {}

/// Run one backtest to completion. Deterministic: identical inputs produce a
/// bit-identical `BacktestResult`.
pub fn run<F, S, M>(
    feed: &mut F,
    strategy: &mut S,
    fill_model: &mut M,
    spec: &ContractSpec,
    params: &BacktestParams,
) -> Result<BacktestResult, EngineError>
where
    F: Feed,
    S: Strategy,
    M: FillModel,
{
    let mut portfolio = Portfolio::new(spec.clone(), params.initial_cash_nano_usd);
    let mut pending: VecDeque<Order> = VecDeque::new();
    let mut actions = Actions::new();
    let mut equity: Vec<(Ts, NanoUsd)> = Vec::new();
    let mut last_avail: Option<Ts> = None;
    let mut next_order_id: u64 = 1;
    let mut n_fills = 0usize;

    while let Some(ev) = feed.next_event() {
        let ts = ev.avail_ts();
        if let Some(prev) = last_avail
            && ts < prev
        {
            return Err(EngineError::OutOfOrderFeed { prev, next: ts });
        }
        last_avail = Some(ts);

        // 1. Fills — only orders placed strictly before this event.
        let mut still_pending = VecDeque::with_capacity(pending.len());
        while let Some(order) = pending.pop_front() {
            if order.placed_ts < ts {
                match fill_model.fill(&order, &ev, spec) {
                    Some(fill) => {
                        debug_assert_eq!(fill.order_id, order.id);
                        portfolio.apply_fill(&fill);
                        n_fills += 1;
                    }
                    None => still_pending.push_back(order),
                }
            } else {
                still_pending.push_back(order);
            }
        }
        pending = still_pending;

        // 2. Mark to this bar's close and record equity.
        let MarketEvent::Bar(bar) = &ev;
        portfolio.mark(bar.close);
        equity.push((ts, portfolio.equity_nano_usd()));

        // 3. Strategy decides.
        let view = portfolio.view();
        strategy.on_event(&ev, &view, &mut actions);
        for intent in actions.take_intents() {
            pending.push_back(Order {
                id: next_order_id,
                instrument: ev.instrument().clone(),
                side: intent.side,
                qty: intent.qty.abs(),
                kind: intent.kind,
                placed_ts: ts,
            });
            next_order_id += 1;
        }
    }

    let summary = Summary::compute(
        &equity,
        portfolio.closed_trades(),
        portfolio.fees_nano_usd(),
        params.bars_per_year,
    );

    Ok(BacktestResult {
        equity,
        summary,
        n_fills,
        cancelled_at_eof: pending.len(),
    })
}
