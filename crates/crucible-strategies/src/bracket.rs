//! Attaching a fixed protective bracket to any strategy.
//!
//! [`Bracketed`] is a decorator for the same reason [`crate::align::Aligned`]
//! is one: the stop distance is a property of how a *position* is managed, not
//! of the signal that opened it, so bolting it onto every strategy struct would
//! duplicate the same three lines and let each copy drift. A hand-written
//! `SmaCross` and a config-built `ComboStrategy` get identical protective
//! behaviour from one implementation.
//!
//! **It rewrites intents; it does not place orders of its own.** The inner
//! strategy decides *what* to trade and this wrapper decides only what rides
//! along with the entry, so a strategy that never trades is never bracketed,
//! and the order stream is otherwise byte-identical to the unwrapped one.
//!
//! What the levels then mean, and which leg wins a bar that touched both, is
//! not decided here — it is `crucible-engine::bracket`'s single named
//! convention (`stop_first_intrabar`), so that no strategy can quietly choose
//! a more flattering intrabar assumption than its neighbour in the same grid.

use crucible_core::prelude::*;

/// Wraps a strategy so every position it opens carries the same [`Bracket`].
#[derive(Debug)]
pub struct Bracketed<S> {
    inner: S,
    bracket: Bracket,
    bracketed_intents: usize,
    /// Reused so a bar allocates at most once over a whole run.
    scratch: Actions,
}

impl<S: Strategy> Bracketed<S> {
    /// Brackets every position `inner` opens with `bracket`, measured in ticks
    /// from the price each entry actually fills at.
    #[must_use]
    pub fn new(inner: S, bracket: Bracket) -> Bracketed<S> {
        Bracketed {
            inner,
            bracket,
            bracketed_intents: 0,
            scratch: Actions::new(),
        }
    }

    /// The bracket every entry carries.
    #[must_use]
    pub const fn bracket(&self) -> Bracket {
        self.bracket
    }

    /// Orders that went out carrying a bracket.
    ///
    /// Fewer than the strategy's total: a pure exit gets none, because there is
    /// no resulting position to protect.
    #[must_use]
    pub const fn bracketed_intents(&self) -> usize {
        self.bracketed_intents
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

impl<S: Strategy> Strategy for Bracketed<S> {
    fn warmup_bars(&self) -> usize {
        self.inner.warmup_bars()
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        self.inner.on_event(ev, view, &mut self.scratch);
        // Walk the position forward through the intents rather than looking at
        // each one alone: whether an order opens something or closes something
        // depends on what the orders before it in the same bar did.
        let mut position = view.position.0;
        for mut intent in self.scratch.take_intents() {
            let signed = match intent.side {
                Side::Buy => intent.qty.0,
                Side::Sell => -intent.qty.0,
            };
            position += signed;
            // A fill that lands flat is a pure exit — nothing to protect. A
            // flip closes and reopens, and the reopened half does need levels.
            if position != 0 {
                intent.bracket = Some(self.bracket);
                self.bracketed_intents += 1;
            }
            actions.push_intent(intent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emits a scripted sequence of `target_position` calls, one per bar.
    struct Scripted {
        targets: Vec<i32>,
        bar: usize,
    }

    impl Strategy for Scripted {
        fn warmup_bars(&self) -> usize {
            0
        }
        fn on_event(&mut self, _ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
            if let Some(&target) = self.targets.get(self.bar) {
                actions.target_position(view.position, Qty(target));
            }
            self.bar += 1;
        }
    }

    fn bar() -> MarketEvent {
        let p = Price::from_points(100);
        MarketEvent::Bar(Bar {
            instrument: InstrumentId::new("SYN:TEST"),
            tf: TimeFrame::M1,
            ts_open: Ts(0),
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1,
            signal_offset: Price::ZERO,
        })
    }

    fn view(position: i32) -> PortfolioView {
        PortfolioView {
            position: Qty(position),
            avg_entry: None,
            cash_nano_usd: 0,
            equity_nano_usd: 0,
        }
    }

    /// Flat → long 1 → flat → short 1. The entry and the flip-to-short carry
    /// the bracket; the exit to flat does not.
    #[test]
    fn entries_are_bracketed_and_exits_are_not() {
        let bracket = Bracket::new(Some(8), Some(12));
        let mut s = Bracketed::new(
            Scripted {
                targets: vec![1, 0, -1],
                bar: 0,
            },
            bracket,
        );
        let positions = [0, 1, 0];
        let mut brackets = Vec::new();
        for (i, position) in positions.into_iter().enumerate() {
            let mut actions = Actions::new();
            s.on_event(&bar(), &view(position), &mut actions);
            let intents = actions.take_intents();
            assert_eq!(intents.len(), 1, "bar {i} should emit one order");
            brackets.push(intents[0].bracket);
        }
        assert_eq!(brackets, vec![Some(bracket), None, Some(bracket)]);
        assert_eq!(s.bracketed_intents(), 2);
    }

    /// The wrapper is transparent to everything except the bracket field: same
    /// sides, same sizes, same bars, same warmup.
    #[test]
    fn the_order_stream_is_otherwise_unchanged() {
        let script = || Scripted {
            targets: vec![1, 1, -1, 0],
            bar: 0,
        };
        let mut plain = script();
        let mut wrapped = Bracketed::new(script(), Bracket::new(Some(4), None));
        assert_eq!(wrapped.warmup_bars(), plain.warmup_bars());

        let positions = [0, 1, 1, -1];
        for position in positions {
            let mut a = Actions::new();
            let mut b = Actions::new();
            plain.on_event(&bar(), &view(position), &mut a);
            wrapped.on_event(&bar(), &view(position), &mut b);
            let (a, b) = (a.take_intents(), b.take_intents());
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                assert_eq!(x.side, y.side);
                assert_eq!(x.qty, y.qty);
                assert_eq!(x.kind, y.kind);
            }
        }
    }
}
