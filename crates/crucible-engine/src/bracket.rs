//! Protective brackets, and the one convention that decides what an OHLC bar
//! refuses to say.
//!
//! # `stop_first_intrabar`
//!
//! A bar records four prices and no order. When a live bracket's stop *and*
//! its target both sit inside `[low, high]`, the bar is equally consistent
//! with the stop having printed first and with the target having printed
//! first, and those two readings differ by the whole width of the bracket.
//! Every bar-based backtest in existence has to pick one. This is ours, it has
//! a name, and it is printed with any result it touched (D-0069):
//!
//! 1. **The opening print is known, and it wins.** The open is the first trade
//!    of the bar by definition. If the bar opened at or beyond a level, that
//!    level was reached before anything else in the bar could happen, so that
//!    leg fills — *at the opening print*, never at the level. A bar that
//!    opened three points through your stop never offered your stop; filling
//!    there manufactures a price. This resolves gaps in both directions: a
//!    gapped stop is worse than its level, a gapped target is better than its
//!    level, and both are simply what the market printed. It also means a bar
//!    that opens through the target fills the target even though the stop was
//!    touched later — claiming otherwise would require the price to have
//!    passed a resting limit without filling it.
//! 2. **Otherwise, if both legs were touched, the STOP fills**, at its level.
//!    This is the worst case for the strategy, and it is chosen for exactly
//!    that reason: the ambiguity is real, so the number a reader is given
//!    must be the pessimistic one. Never the target, never the nearer level,
//!    never a coin flip — a rule that resolved half the ambiguous bars
//!    favourably would let a strategy earn money from a fact the data does not
//!    contain.
//! 3. **Otherwise** whichever single leg was touched fills at its level, or
//!    neither does.
//!
//! Rule 2 is the *only* branch where the result depends on an unknowable
//! ordering, so it is the only branch that increments
//! [`Resolution::path_sensitive`] — and `BacktestResult::path_sensitive_bars`
//! after it. Counting gaps as ambiguous would drown the signal in bars whose
//! path the open already settled.
//!
//! # At the level exactly
//!
//! The two legs are deliberately **asymmetric** at their own price:
//!
//! | leg | touched when (long) | why |
//! |---|---|---|
//! | stop | `low <= stop` | a trade printing at the stop proves the market reached it, and a stop is a market order from that instant |
//! | target | `high > target` | a trade printing *at* a resting limit proves nothing: there was a queue in front of it |
//!
//! Both halves are the pessimistic reading, which is why they point in
//! opposite directions. The target's strictness is also the only defence this
//! layer has against the queue-position optimism that keeps `OrderKind::Limit`
//! out of the engine until M4's `queue_sim` can model it properly.
//!
//! # Where the levels come from
//!
//! From the price the parent order **actually filled at**, not from the price
//! the strategy saw when it placed it (see [`Bracket`]). One consequence is
//! worth stating because it looks like a bug: under `spread_cross`, a stop
//! closer than the half-spread lands at or beyond the opening print, so it
//! stops out on the very bar it was installed. That is the fill model being
//! honest — you bought the offer and put your stop on the bid.

use crucible_core::prelude::*;

/// The name this convention is reported under (CLAUDE.md §2.4: assumptions are
/// named, never implied).
pub const INTRABAR_CONVENTION: &str = "stop_first_intrabar";

/// The live protective levels of an open position: absolute prices, resolved
/// from the fill that opened it.
///
/// At most one exists per run, because the portfolio holds one position
/// (single-instrument until post-M4). It is one-cancels-the-other by
/// construction: the engine drops the whole thing the moment either leg fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveBracket {
    /// The order whose bracket this is. A protective exit reports this id
    /// rather than one of its own — it is that order's other half.
    pub order_id: u64,
    /// Inherited from the parent order. The engine only ever tests a bracket
    /// against events strictly after this instant, which is what keeps a
    /// resting order inside §2.1.
    pub placed_ts: Ts,
    /// The side that flattens the protected position.
    pub exit_side: Side,
    /// Absolute stop price, if the bracket has that leg.
    pub stop: Option<Price>,
    /// Absolute target price, if the bracket has that leg.
    pub target: Option<Price>,
}

impl ActiveBracket {
    /// Resolves `bracket`'s tick offsets against the price its parent filled
    /// at, for a position of sign `position_sign`.
    ///
    /// # Panics
    /// Panics if `position_sign` is zero — a bracket around a flat position is
    /// an engine bug, not a configuration one, and the caller (`replay`) drops
    /// brackets the moment the position closes.
    #[must_use]
    pub fn from_fill(
        order_id: u64,
        placed_ts: Ts,
        bracket: Bracket,
        fill_price: Price,
        position_sign: i64,
        tick: Price,
    ) -> ActiveBracket {
        assert!(
            position_sign != 0,
            "INVARIANT: a bracket needs a position to protect"
        );
        let long = position_sign > 0;
        let away = |ticks: i64| Price::from_nanos(ticks * tick.as_nanos());
        ActiveBracket {
            order_id,
            placed_ts,
            exit_side: if long { Side::Sell } else { Side::Buy },
            stop: bracket.stop_ticks().map(|t| {
                if long {
                    fill_price - away(t)
                } else {
                    fill_price + away(t)
                }
            }),
            target: bracket.target_ticks().map(|t| {
                if long {
                    fill_price + away(t)
                } else {
                    fill_price - away(t)
                }
            }),
        }
    }

    /// Does this bracket still protect a position of sign `position_sign`?
    ///
    /// False once the position has closed or flipped: levels computed for a
    /// long are on the wrong side of a short, and leaving them installed would
    /// exit the new position instantly and for the wrong reason.
    #[must_use]
    pub const fn protects(&self, position_sign: i64) -> bool {
        match self.exit_side {
            Side::Sell => position_sign > 0,
            Side::Buy => position_sign < 0,
        }
    }
}

/// Which leg (if any) a bar triggered, and at what price.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The bar traded through neither level.
    Untouched,
    /// A leg triggered.
    Touched {
        /// Which leg — and therefore how the fill model must cost it.
        leg: BracketLeg,
        /// The level, or the opening print when the bar opened beyond it.
        at: Price,
        /// True when `at` is the opening print rather than the level.
        gapped: bool,
    },
}

/// The verdict on one bar: what happened, and whether the bar's own path had
/// to be guessed to say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// Which leg the bar triggered, if any.
    pub outcome: Outcome,
    /// True only for rule 2 above: both legs were touched and the opening
    /// print settled neither, so the exit reported here rests on the
    /// convention rather than on the data. A result with many of these is a
    /// result to distrust, which is why the count is printed rather than
    /// absorbed.
    pub path_sensitive: bool,
}

/// The `stop_first_intrabar` convention (see the module docs, which are its
/// specification).
///
/// A type rather than a loose function so the convention has a name that can
/// be grepped, referenced from a report footer, and — the day a second
/// convention is defensible — sat beside a sibling instead of being edited in
/// place.
#[derive(Debug, Default, Clone, Copy)]
pub struct StopFirstIntrabar;

impl StopFirstIntrabar {
    /// Resolves `bracket` against one bar.
    #[must_use]
    pub fn resolve(bracket: &ActiveBracket, bar: &Bar) -> Resolution {
        // Read every predicate from the exiting side's point of view. For a
        // long (exit_side = Sell) the stop is below and the target above; for a
        // short both flip, and so does every comparison.
        let long = matches!(bracket.exit_side, Side::Sell);
        let stop_reached = |p: Price, level: Price| if long { p <= level } else { p >= level };
        // Strict: a print *at* a resting limit does not prove it filled.
        let target_reached = |p: Price, level: Price| if long { p > level } else { p < level };
        let worst = if long { bar.low } else { bar.high };
        let best = if long { bar.high } else { bar.low };

        // 1. The opening print is the first trade of the bar, so a level the
        //    open already passed was reached before anything else could be.
        if let Some(stop) = bracket.stop
            && stop_reached(bar.open, stop)
        {
            return Resolution {
                outcome: Outcome::Touched {
                    leg: BracketLeg::Stop,
                    at: bar.open,
                    gapped: true,
                },
                path_sensitive: false,
            };
        }
        if let Some(target) = bracket.target
            && target_reached(bar.open, target)
        {
            return Resolution {
                outcome: Outcome::Touched {
                    leg: BracketLeg::Target,
                    at: bar.open,
                    gapped: true,
                },
                path_sensitive: false,
            };
        }

        let stop_hit = bracket.stop.filter(|&stop| stop_reached(worst, stop));
        let target_hit = bracket
            .target
            .filter(|&target| target_reached(best, target));

        // 2 and 3. Both touched inside the bar is the ambiguous case, and the
        //          stop takes it.
        match (stop_hit, target_hit) {
            (Some(stop), target) => Resolution {
                outcome: Outcome::Touched {
                    leg: BracketLeg::Stop,
                    at: stop,
                    gapped: false,
                },
                path_sensitive: target.is_some(),
            },
            (None, Some(target)) => Resolution {
                outcome: Outcome::Touched {
                    leg: BracketLeg::Target,
                    at: target,
                    gapped: false,
                },
                path_sensitive: false,
            },
            (None, None) => Resolution {
                outcome: Outcome::Untouched,
                path_sensitive: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ES-like: 0.25 tick.
    fn tick() -> Price {
        Price::from_points_f64_lossy(0.25)
    }

    fn bar(o: f64, h: f64, l: f64, c: f64) -> Bar {
        Bar {
            instrument: InstrumentId::new("SYN:ES"),
            tf: TimeFrame::M1,
            ts_open: Ts(0),
            open: Price::from_points_f64_lossy(o),
            high: Price::from_points_f64_lossy(h),
            low: Price::from_points_f64_lossy(l),
            close: Price::from_points_f64_lossy(c),
            volume: 10,
        }
    }

    /// Long 1 filled at 100.00 with an 8-tick stop and a 12-tick target.
    /// 8 × 0.25 = 2.00 below → stop 98.00; 12 × 0.25 = 3.00 above →
    /// target 103.00.
    fn long_bracket() -> ActiveBracket {
        ActiveBracket::from_fill(
            7,
            Ts(0),
            Bracket::new(Some(8), Some(12)),
            Price::from_points(100),
            1,
            tick(),
        )
    }

    /// Short 1 filled at 100.00 with the same offsets, mirrored: stop 102.00,
    /// target 97.00.
    fn short_bracket() -> ActiveBracket {
        ActiveBracket::from_fill(
            7,
            Ts(0),
            Bracket::new(Some(8), Some(12)),
            Price::from_points(100),
            -1,
            tick(),
        )
    }

    #[test]
    fn levels_are_ticks_away_from_the_fill_on_the_right_sides() {
        let long = long_bracket();
        assert_eq!(long.exit_side, Side::Sell);
        assert_eq!(long.stop, Some(Price::from_points(98)));
        assert_eq!(long.target, Some(Price::from_points(103)));

        let short = short_bracket();
        assert_eq!(short.exit_side, Side::Buy);
        assert_eq!(short.stop, Some(Price::from_points(102)));
        assert_eq!(short.target, Some(Price::from_points(97)));
    }

    #[test]
    fn a_bracket_stops_protecting_a_position_that_closed_or_flipped() {
        let long = long_bracket();
        assert!(long.protects(1));
        assert!(!long.protects(0));
        assert!(!long.protects(-1));
        let short = short_bracket();
        assert!(short.protects(-2));
        assert!(!short.protects(3));
    }

    /// THE ambiguous bar: opens at 100 (inside the bracket), then trades down
    /// to 97.50 — through the 98.00 stop — and up to 104.00 — through the
    /// 103.00 target. The bar does not say which came first. The stop wins,
    /// at its level, and the bar is counted.
    #[test]
    fn both_touched_gives_the_stop_and_is_path_sensitive() {
        let r = StopFirstIntrabar::resolve(&long_bracket(), &bar(100.0, 104.0, 97.5, 103.5));
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(98),
                gapped: false,
            }
        );
        assert!(r.path_sensitive);
    }

    /// Mirrored for a short: opens at 100, trades up through the 102.00 stop
    /// and down through the 97.00 target.
    #[test]
    fn both_touched_short_gives_the_stop_too() {
        let r = StopFirstIntrabar::resolve(&short_bracket(), &bar(100.0, 102.5, 96.5, 97.0));
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(102),
                gapped: false,
            }
        );
        assert!(r.path_sensitive);
    }

    /// A bar that opens *through* the stop fills at the opening print, which
    /// is worse than the level, and is NOT path-sensitive: the open is the
    /// first trade, so the ordering is known rather than guessed. The target
    /// being touched later in the bar changes nothing.
    #[test]
    fn a_gapped_stop_fills_at_the_open_and_is_not_ambiguous() {
        let r = StopFirstIntrabar::resolve(&long_bracket(), &bar(96.0, 104.0, 95.0, 103.5));
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(96),
                gapped: true,
            }
        );
        assert!(!r.path_sensitive);
    }

    /// The same rule the other way round, and the reason rule 1 is not "the
    /// stop always wins": this bar opened at 104.00, above the 103.00 target.
    /// A resting sell limit at 103.00 cannot have been passed unfilled, so the
    /// target filled first — at 104.00 — even though the low then reached the
    /// stop. Awarding the stop here would be pessimism achieved by asserting
    /// something impossible.
    #[test]
    fn a_gapped_target_fills_at_the_open_even_though_the_stop_was_touched() {
        let r = StopFirstIntrabar::resolve(&long_bracket(), &bar(104.0, 105.0, 97.0, 98.0));
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Target,
                at: Price::from_points(104),
                gapped: true,
            }
        );
        assert!(!r.path_sensitive);
    }

    #[test]
    fn a_gapped_target_fills_at_the_open_for_a_short_too() {
        let r = StopFirstIntrabar::resolve(&short_bracket(), &bar(96.0, 103.0, 95.0, 102.0));
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Target,
                at: Price::from_points(96),
                gapped: true,
            }
        );
        assert!(!r.path_sensitive);
    }

    /// Exactly at the level. A trade printing at the stop proves the market
    /// reached it; a trade printing at the target proves only that there was a
    /// queue. So `low == 98.00` stops out and `high == 103.00` does not take
    /// profit — the pessimistic reading of each leg, which is why they differ.
    #[test]
    fn at_the_level_the_stop_fills_and_the_target_does_not() {
        let stopped = StopFirstIntrabar::resolve(&long_bracket(), &bar(100.0, 100.5, 98.0, 99.0));
        assert_eq!(
            stopped.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(98),
                gapped: false,
            }
        );

        let not_filled =
            StopFirstIntrabar::resolve(&long_bracket(), &bar(100.0, 103.0, 99.0, 102.0));
        assert_eq!(not_filled.outcome, Outcome::Untouched);

        // One tick through it does fill.
        let filled = StopFirstIntrabar::resolve(&long_bracket(), &bar(100.0, 103.25, 99.0, 103.25));
        assert_eq!(
            filled.outcome,
            Outcome::Touched {
                leg: BracketLeg::Target,
                at: Price::from_points(103),
                gapped: false,
            }
        );
    }

    /// The same boundary, mirrored: a short's stop is *above* it, so touching
    /// 102.00 exactly stops out, and touching 97.00 exactly does not take
    /// profit.
    #[test]
    fn at_the_level_for_a_short_is_mirrored() {
        let stopped = StopFirstIntrabar::resolve(&short_bracket(), &bar(100.0, 102.0, 99.5, 101.0));
        assert_eq!(
            stopped.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(102),
                gapped: false,
            }
        );
        let not_filled =
            StopFirstIntrabar::resolve(&short_bracket(), &bar(100.0, 100.5, 97.0, 98.0));
        assert_eq!(not_filled.outcome, Outcome::Untouched);
    }

    /// An open sitting exactly *on* the stop is a gap: the stop's rule is
    /// "at or through", and the open is a trade at it.
    #[test]
    fn an_open_exactly_on_the_stop_gaps() {
        let r = StopFirstIntrabar::resolve(&long_bracket(), &bar(98.0, 99.0, 97.0, 98.5));
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(98),
                gapped: true,
            }
        );
    }

    /// An open sitting exactly on the target is not a gap, and not a fill:
    /// same queue argument, applied to the first print instead of the high.
    #[test]
    fn an_open_exactly_on_the_target_is_not_a_fill() {
        let r = StopFirstIntrabar::resolve(&long_bracket(), &bar(103.0, 103.0, 102.0, 102.5));
        assert_eq!(r.outcome, Outcome::Untouched);
    }

    #[test]
    fn a_quiet_bar_inside_both_levels_touches_nothing() {
        let r = StopFirstIntrabar::resolve(&long_bracket(), &bar(100.0, 101.0, 99.0, 100.5));
        assert_eq!(r.outcome, Outcome::Untouched);
        assert!(!r.path_sensitive);
    }

    /// A one-legged bracket can never be ambiguous: there is no second level
    /// for the bar to have reached in an unknown order.
    #[test]
    fn a_stop_only_bracket_is_never_path_sensitive() {
        let stop_only = ActiveBracket::from_fill(
            1,
            Ts(0),
            Bracket::new(Some(8), None),
            Price::from_points(100),
            1,
            tick(),
        );
        let r = StopFirstIntrabar::resolve(&stop_only, &bar(100.0, 110.0, 97.0, 109.0));
        assert!(!r.path_sensitive);
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Stop,
                at: Price::from_points(98),
                gapped: false,
            }
        );

        let target_only = ActiveBracket::from_fill(
            1,
            Ts(0),
            Bracket::new(None, Some(12)),
            Price::from_points(100),
            1,
            tick(),
        );
        let r = StopFirstIntrabar::resolve(&target_only, &bar(100.0, 110.0, 90.0, 109.0));
        assert!(!r.path_sensitive);
        assert_eq!(
            r.outcome,
            Outcome::Touched {
                leg: BracketLeg::Target,
                at: Price::from_points(103),
                gapped: false,
            }
        );
    }
}
