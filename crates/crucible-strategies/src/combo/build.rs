//! The factory: one grid point becomes one runnable [`Strategy`].
//!
//! A `ComboStrategy` is deliberately small — indicator state, a cloned rule
//! AST, one previous-reading pair per crossover node, and four counters — so
//! the funnel can hold thousands of them against one data pass. The AST is
//! cloned rather than shared behind an `Arc` because it is a handful of nodes
//! and an atomic refcount on the hot path buys nothing at this size.

use crucible_core::prelude::*;

use crate::indicators::{Bollinger, Ema, Sma};

use super::grid::Combo;
use super::rules::{EvalCtx, Expr, RuleSet, SlotOut};
use super::spec::{ComboSpec, IndicatorParams};

/// The indicator behind one slot.
#[derive(Debug, Clone)]
enum SlotState {
    Sma(Sma),
    Ema(Ema),
    Bollinger(Bollinger),
}

impl SlotState {
    fn build(params: IndicatorParams) -> SlotState {
        match params {
            IndicatorParams::Sma { period } => SlotState::Sma(Sma::new(period)),
            IndicatorParams::Ema { period } => SlotState::Ema(Ema::new(period)),
            IndicatorParams::Bollinger { period, k } => {
                SlotState::Bollinger(Bollinger::new(period, k))
            }
        }
    }

    fn update(&mut self, bar: &Bar) -> Option<SlotOut> {
        match self {
            SlotState::Sma(i) => i.update(bar).map(SlotOut::Scalar),
            SlotState::Ema(i) => i.update(bar).map(SlotOut::Scalar),
            SlotState::Bollinger(i) => i.update(bar).map(SlotOut::Bands),
        }
    }
}

/// A strategy assembled from a config instead of written in Rust.
#[derive(Debug, Clone)]
pub struct ComboStrategy {
    slots: Vec<SlotState>,
    values: Vec<Option<SlotOut>>,
    rules: RuleSet,
    cross_state: Vec<Option<(f64, f64)>>,
    qty: Qty,
    warmup_bars: usize,
    conflicting_signals: usize,
}

impl ComboStrategy {
    /// Builds the strategy for one grid point.
    ///
    /// Reached through [`crate::combo::Grid::strategy`] in normal use; public
    /// so a test can build one combo without an enclosing grid.
    ///
    /// # Panics
    /// Panics if `combo` did not come from `spec` (mismatched slot count).
    /// Both are produced together by [`ComboSpec::expand`], so a mismatch is a
    /// construction bug.
    #[must_use]
    pub fn build(spec: &ComboSpec, combo: &Combo) -> ComboStrategy {
        assert_eq!(
            combo.slots.len(),
            spec.slots().len(),
            "INVARIANT: a combo carries one entry per slot of the spec it came from"
        );
        let slots: Vec<SlotState> = combo
            .slots
            .iter()
            .map(|s| SlotState::build(s.params))
            .collect();
        let rules = spec.rules().clone();
        // A crossover needs two readings, so it costs one bar beyond the
        // longest indicator.
        let warmup_bars = combo.own_warmup_bars() + usize::from(rules.uses_crossover());
        ComboStrategy {
            values: vec![None; slots.len()],
            cross_state: vec![None; rules.cross_states()],
            slots,
            rules,
            qty: spec.qty(),
            warmup_bars,
            conflicting_signals: 0,
        }
    }

    /// Bars on which `enter_long` and `enter_short` were both true and the
    /// position was flat, so nothing was done.
    ///
    /// Expected to be zero. A nonzero count means the config's two entry
    /// conditions overlap — which is a config bug that only shows up at
    /// runtime, and this is how it becomes visible instead of becoming an
    /// arbitrary position (see the module docs).
    #[must_use]
    pub const fn conflicting_signals(&self) -> usize {
        self.conflicting_signals
    }
}

impl Strategy for ComboStrategy {
    fn warmup_bars(&self) -> usize {
        self.warmup_bars
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        let MarketEvent::Bar(bar) = ev;

        // Split the borrow: indicators are updated mutably, then read
        // immutably alongside the crossover slab.
        let ComboStrategy {
            slots,
            values,
            rules,
            cross_state,
            qty,
            conflicting_signals,
            ..
        } = self;

        for (state, out) in slots.iter_mut().zip(values.iter_mut()) {
            *out = state.update(bar);
        }

        let ctx = EvalCtx { bar, slots: values };
        // Every rule, every bar, unconditionally: a crossover node that misses
        // a bar compares against a stale reading and fires late. See
        // `rules`'s module docs.
        let fires = |rule: &Option<Expr>, state: &mut Vec<Option<(f64, f64)>>| -> bool {
            rule.as_ref()
                .and_then(|e| e.eval(&ctx, state))
                .unwrap_or(false)
        };
        let enter_long = fires(&rules.enter_long, cross_state);
        let exit_long = fires(&rules.exit_long, cross_state);
        let enter_short = fires(&rules.enter_short, cross_state);
        let exit_short = fires(&rules.exit_short, cross_state);

        let position = view.position;
        let mut target = position.0;
        if target > 0 && exit_long {
            target = 0;
        }
        if target < 0 && exit_short {
            target = 0;
        }
        if target == 0 {
            match (enter_long, enter_short) {
                (true, false) => target = qty.0,
                (false, true) => target = -qty.0,
                (true, true) => *conflicting_signals += 1,
                (false, false) => {}
            }
        }
        actions.target_position(position, Qty(target));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SmaCross;
    use crate::combo::rules::RuleSource;
    use crate::combo::spec::{FloatAxis, IndicatorSpec, IntAxis};
    use crate::combo::{ComboSpec, Grid};

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

    fn flat() -> PortfolioView {
        PortfolioView {
            position: Qty(0),
            avg_entry: None,
            cash_nano_usd: 0,
            equity_nano_usd: 0,
        }
    }

    fn build(slots: Vec<(String, IndicatorSpec)>, rules: RuleSource) -> Grid {
        ComboSpec::new(slots, &rules, Qty(1))
            .expect("valid spec")
            .expand()
            .expect("expands")
    }

    fn sma(name: &str, period: usize) -> (String, IndicatorSpec) {
        (
            name.to_owned(),
            IndicatorSpec::Sma {
                period: IntAxis::Fixed(period),
            },
        )
    }

    /// Feeds a series through a strategy, tracking the position the way the
    /// engine would if every order filled on the next bar. Returns
    /// `(bar_index, target_position)` for every bar that emitted an order.
    fn drive<S: Strategy>(strategy: &mut S, closes: &[f64]) -> Vec<(usize, i32)> {
        let mut position = Qty(0);
        let mut fired = Vec::new();
        for (i, &c) in closes.iter().enumerate() {
            let view = PortfolioView { position, ..flat() };
            let mut actions = Actions::new();
            #[expect(clippy::cast_possible_wrap, reason = "test index")]
            strategy.on_event(&bar(i as i64, c), &view, &mut actions);
            let intents = actions.take_intents();
            if !intents.is_empty() {
                assert_eq!(intents.len(), 1, "target_position emits one order");
                let delta = intents[0].qty.0 * i32::try_from(intents[0].side.sign()).expect("±1");
                position = Qty(position.0 + delta);
                fired.push((i, position.0));
            }
        }
        fired
    }

    /// Hand-computed. SMA(3) of closes [10, 12, 14, 9, 6]:
    ///
    /// | bar | close | SMA(3)          | close < sma3 |
    /// |---|---|---|---|
    /// | 0 | 10 | —               | — (unwarm)   |
    /// | 1 | 12 | —               | — (unwarm)   |
    /// | 2 | 14 | (10+12+14)/3=12 | false        |
    /// | 3 |  9 | (12+14+9)/3≈11.67 | **true**   |
    /// | 4 |  6 | (14+9+6)/3≈9.67 | true         |
    ///
    /// `enter_long = "close < mid"` therefore first fires on bar 3, and bar 4
    /// re-issues nothing because the position is already there.
    #[test]
    fn a_threshold_rule_fires_where_hand_arithmetic_says() {
        let grid = build(
            vec![sma("mid", 3)],
            RuleSource {
                enter_long: Some("close < mid".to_owned()),
                ..RuleSource::default()
            },
        );
        let mut strategy = grid.strategy(0);
        let fired = drive(&mut strategy, &[10.0, 12.0, 14.0, 9.0, 6.0]);
        assert_eq!(fired, vec![(3, 1)]);
        assert_eq!(strategy.warmup_bars(), 3);
    }

    /// The warmup boundary, from the other side: with SMA(3) the rule cannot
    /// have an opinion before bar index 2, so nothing fires on bars 0 or 1 no
    /// matter what the prices do.
    #[test]
    fn nothing_fires_before_the_indicator_is_warm() {
        let grid = build(
            vec![sma("mid", 4)],
            RuleSource {
                // Always true once warm: an SMA of a falling series is above
                // its own last close.
                enter_long: Some("close < mid".to_owned()),
                ..RuleSource::default()
            },
        );
        let mut strategy = grid.strategy(0);
        let fired = drive(&mut strategy, &[10.0, 9.0, 8.0, 7.0, 6.0, 5.0]);
        // SMA(4) is warm on bar index 3: (10+9+8+7)/4 = 8.5 > close 7.
        assert_eq!(fired, vec![(3, 1)]);
    }

    /// The equivalence that makes the whole layer credible: `SmaCross`, the
    /// reference strategy every golden test uses, written as four lines of
    /// TOML rules, emits exactly the same orders on the same bars.
    ///
    /// The series is a deterministic zig-zag rather than random data so the
    /// test cannot pass by both strategies doing nothing.
    #[test]
    fn a_config_reproduces_the_hand_coded_sma_cross() {
        #[expect(clippy::cast_precision_loss, reason = "small test indices")]
        let closes: Vec<f64> = (0..200)
            .map(|i: usize| 100.0 + ((i * 7) % 23) as f64 - 11.0)
            .collect();

        let grid = build(
            vec![sma("fast", 2), sma("slow", 3)],
            RuleSource {
                enter_long: Some("fast crosses_above slow".to_owned()),
                exit_long: Some("fast crosses_below slow".to_owned()),
                enter_short: Some("fast crosses_below slow".to_owned()),
                exit_short: Some("fast crosses_above slow".to_owned()),
            },
        );
        let mut from_config = grid.strategy(0);
        let mut hand_coded = SmaCross::new(2, 3, Qty(1));

        assert_eq!(from_config.warmup_bars(), hand_coded.warmup_bars());
        let a = drive(&mut from_config, &closes);
        let b = drive(&mut hand_coded, &closes);
        assert!(a.len() > 10, "the fixture must actually trade: {}", a.len());
        assert_eq!(a, b);
        assert_eq!(from_config.conflicting_signals(), 0);
    }

    /// Both entry rules true at once takes no position and is counted, rather
    /// than picking one arbitrarily.
    #[test]
    fn conflicting_entries_are_counted_not_guessed() {
        let grid = build(
            vec![sma("mid", 2)],
            RuleSource {
                enter_long: Some("close > 0".to_owned()),
                enter_short: Some("close > 0".to_owned()),
                ..RuleSource::default()
            },
        );
        let mut strategy = grid.strategy(0);
        let fired = drive(&mut strategy, &[10.0, 11.0, 12.0]);
        assert!(fired.is_empty(), "no position may be taken on a conflict");
        assert_eq!(strategy.conflicting_signals(), 3);
    }

    /// Stop-and-reverse: one crossover both closes the long and opens the
    /// short, so the emitted order is for twice the size.
    #[test]
    fn exit_and_enter_on_one_bar_is_a_single_reversing_order() {
        let grid = build(
            vec![sma("fast", 2), sma("slow", 3)],
            RuleSource {
                enter_long: Some("fast crosses_above slow".to_owned()),
                exit_long: Some("fast crosses_below slow".to_owned()),
                enter_short: Some("fast crosses_below slow".to_owned()),
                exit_short: Some("fast crosses_above slow".to_owned()),
            },
        );
        let mut strategy = grid.strategy(0);
        let mut position = Qty(0);
        let mut sizes = Vec::new();
        for (i, &c) in [10.0, 9.0, 8.0, 7.0, 12.0, 15.0, 2.0, 1.0]
            .iter()
            .enumerate()
        {
            let view = PortfolioView { position, ..flat() };
            let mut actions = Actions::new();
            #[expect(clippy::cast_possible_wrap, reason = "test index")]
            strategy.on_event(&bar(i as i64, c), &view, &mut actions);
            for intent in actions.take_intents() {
                sizes.push(intent.qty.0);
                let delta = intent.qty.0 * i32::try_from(intent.side.sign()).expect("±1");
                position = Qty(position.0 + delta);
            }
        }
        assert_eq!(sizes, vec![1, 2], "flat->long is 1, long->short is 2");
        assert_eq!(position, Qty(-1));
    }

    /// A Bollinger slot exercises the multi-output path and the float axis.
    /// Hand-computed: closes [1,2,3] give mean 2 and population sd
    /// sqrt(2/3) ≈ 0.8165 (D-0009), so with k = 2 the lower band is
    /// 2 − 1.633 = 0.367 — above the close of 3? No: close 3 > lower, so
    /// `close < bb.lower` is false, and `close > bb.upper` (2 + 1.633 =
    /// 3.633) is also false. Bar 4 closes at 0.1, below the lower band of
    /// window [2,3,0.1]: mean 1.7, sd ≈ 1.2437, lower ≈ −0.787 — still not
    /// below. The rule that does fire is `close < bb.mid`.
    #[test]
    fn bollinger_slots_resolve_their_fields() {
        let grid = build(
            vec![(
                "bb".to_owned(),
                IndicatorSpec::Bollinger {
                    period: IntAxis::Fixed(3),
                    k: FloatAxis::Fixed(2.0),
                },
            )],
            RuleSource {
                enter_long: Some("close < bb.mid".to_owned()),
                exit_long: Some("close > bb.upper".to_owned()),
                ..RuleSource::default()
            },
        );
        let mut strategy = grid.strategy(0);
        let fired = drive(&mut strategy, &[1.0, 2.0, 3.0, 0.1]);
        // Bar 2: mid = 2, close 3 — no. Bar 3: window [2,3,0.1], mid = 1.7,
        // close 0.1 < 1.7 — enter long.
        assert_eq!(fired, vec![(3, 1)]);
    }
}
