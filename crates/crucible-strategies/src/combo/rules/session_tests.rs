//! Session-operand tests (D-0078).
//!
//! Separate from `rules`'s own test module because these need a different
//! fixture — a `SessionPosition` rather than a bar — and because the numbers
//! are hand-derived against the CME session shape, which is worth keeping in
//! one readable block.
//!
//! The shape, once, so every case below can refer to it: a session opens at
//! 17:00 CT and closes at 16:00 CT the next day (1380 minutes), and the regular
//! session runs 08:30–15:00 CT, which is session minute 930 to session minute
//! 1320.

use super::*;
use crucible_core::prelude::*;

fn slots() -> Vec<SlotDecl> {
    vec![SlotDecl {
        name: "trend".to_owned(),
        kind: IndicatorKind::Ema,
    }]
}

fn parsed(text: &str) -> Expr {
    let mut crosses = 0;
    parse(text, &slots(), &mut crosses).expect("valid rule")
}

fn rejected(text: &str) -> RuleError {
    let mut crosses = 0;
    parse(text, &slots(), &mut crosses).expect_err("should have been refused")
}

fn rendered(text: &str) -> String {
    let mut out = String::new();
    parsed(text).render(&slots(), &mut out);
    out
}

fn bar() -> Bar {
    let p = Price::from_points(5000);
    Bar {
        instrument: InstrumentId::new("SYN:TEST"),
        tf: TimeFrame::M1,
        ts_open: Ts(0),
        open: p,
        high: p,
        low: p,
        close: p,
        volume: 1,
        signal_offset: Price::ZERO,
    }
}

/// A bar `since_open` minutes into a session with `to_close` minutes left, in
/// `phase`. The RTH distances follow from the shape in the module docs.
fn at(since_open: f64, to_close: f64, phase: SessionPhase) -> SessionPosition {
    SessionPosition {
        minutes_since_open: since_open,
        minutes_to_close: to_close,
        minutes_since_rth_open: since_open - 930.0,
        minutes_to_rth_close: 1320.0 - since_open,
        phase,
    }
}

fn fires(text: &str, session: Option<SessionPosition>) -> Option<bool> {
    let expr = parsed(text);
    let b = bar();
    let mut state = vec![None; 1];
    expr.eval(
        &EvalCtx {
            bar: &b,
            slots: &[Some(SlotOut::Scalar(5000.0))],
            session,
        },
        &mut state,
    )
}

// ------------------------------------------------------------------ grammar

/// Every operand parses, and its canonical rendering is the spelling that was
/// written — a config hash depends on it (D-0012).
#[test]
fn every_session_operand_parses_and_renders_as_itself() {
    for field in SessionField::all() {
        let name = field.name();
        assert_eq!(rendered(&format!("{name} > 0")), format!("({name} > 0.0)"));
    }
    assert_eq!(SessionField::all().len(), 7);
}

#[test]
fn session_operands_normalize_like_every_other_operand() {
    assert_eq!(
        rendered("  ( minutes_since_open  <  30 ) "),
        rendered("minutes_since_open < 30")
    );
    assert_ne!(
        rendered("minutes_since_open < 30"),
        rendered("minutes_since_rth_open < 30")
    );
}

#[test]
fn a_session_operand_has_no_fields_and_cannot_be_shadowed() {
    assert!(
        rejected("minutes_since_open.mid > 0")
            .message
            .contains("session clock reading")
    );
    // A slot named after one would shadow it and the rule would silently mean
    // something else.
    for field in SessionField::all() {
        assert!(
            RESERVED.contains(&field.name()),
            "{} is not reserved",
            field.name()
        );
    }
}

#[test]
fn uses_session_sees_operands_anywhere_in_the_ast() {
    let with = RuleSource {
        enter_long: Some("close > trend".to_owned()),
        exit_long: Some("not (close < trend and minutes_to_close > 5)".to_owned()),
        ..RuleSource::default()
    };
    assert!(RuleSet::new(&with, &slots()).expect("valid").uses_session());

    let without = RuleSource {
        enter_long: Some("close > trend".to_owned()),
        exit_long: Some("close < trend".to_owned()),
        ..RuleSource::default()
    };
    assert!(
        !RuleSet::new(&without, &slots())
            .expect("valid")
            .uses_session()
    );
}

// --------------------------------------------------------------- evaluation

/// Hand-derived. "The first half hour of the regular session" is
/// `minutes_since_rth_open > 0 and minutes_since_rth_open <= 30`, which is
/// session minutes 931 through 960.
///
/// The bar at minute 930 is the one whose interval *ends* at the opening bell,
/// so it is not in the first half hour — it is the last bar before it. That
/// boundary is what the `> 0` is there to get right, and it is the difference
/// between "the first half-hour return" and "the first half-hour return plus
/// the overnight bar that preceded it".
#[test]
fn the_first_half_hour_of_the_regular_session_is_expressible() {
    let rule = "minutes_since_rth_open > 0 and minutes_since_rth_open <= 30";
    for (minute, expected) in [
        (929.0, false),
        (930.0, false),
        (931.0, true),
        (960.0, true),
        (961.0, false),
    ] {
        let session = at(minute, 1380.0 - minute, SessionPhase::Regular);
        assert_eq!(
            fires(rule, Some(session)),
            Some(expected),
            "session minute {minute}"
        );
    }
}

/// The early-close case, which is the whole reason this reads a calendar
/// rather than subtracting from a constant.
///
/// "Flatten in the last 30 minutes" is `minutes_to_close <= 30`. An ordinary
/// CME session is 23 h = 1380 minutes, so the rule starts firing at session
/// minute 1350. A 12:15 CT early close makes the session 19 h 15 min = 1155
/// minutes, so it starts firing at minute 1125 — 225 minutes earlier, which is
/// exactly how much the exchange moved its close by. A rule written against a
/// fixed 16:00 close would have tried to flatten three and three-quarter hours
/// after the market shut.
#[test]
fn flattening_before_the_close_follows_an_early_close() {
    let rule = "minutes_to_close <= 30";
    let ordinary = |minute: f64| at(minute, 1380.0 - minute, SessionPhase::PostRegular);
    assert_eq!(fires(rule, Some(ordinary(1349.0))), Some(false));
    assert_eq!(fires(rule, Some(ordinary(1350.0))), Some(true));

    let early = |minute: f64| at(minute, 1155.0 - minute, SessionPhase::Regular);
    assert_eq!(fires(rule, Some(early(1124.0))), Some(false));
    assert_eq!(fires(rule, Some(early(1125.0))), Some(true));

    // The third case that turns the difference into a diagnosis (§7): at the
    // same session minute the ordinary day is still 255 minutes from its close
    // and says nothing, so what moved the rule is the early close and not the
    // clock.
    assert_eq!(fires(rule, Some(ordinary(1125.0))), Some(false));
}

#[test]
fn the_phase_flags_are_one_hot() {
    for (phase, rth, overnight, post) in [
        (SessionPhase::Regular, true, false, false),
        (SessionPhase::Overnight, false, true, false),
        (SessionPhase::PostRegular, false, false, true),
        (SessionPhase::Closed, false, false, false),
    ] {
        let s = Some(at(100.0, 100.0, phase));
        assert_eq!(fires("is_rth > 0", s), Some(rth), "{phase:?}");
        assert_eq!(fires("is_overnight > 0", s), Some(overnight), "{phase:?}");
        assert_eq!(fires("is_post_rth > 0", s), Some(post), "{phase:?}");
    }
}

/// No clock is **`None`**, not false — the rule this module's docs give for an
/// unwarm slot, and for the same reason.
///
/// `not minutes_since_open < 30` would otherwise read as *true* on every bar of
/// a feed that has no exchange, which is a position taken on the absence of a
/// calendar. The CLI refuses such a config outright; this is what the evaluator
/// does if one ever reaches it anyway.
#[test]
fn a_missing_session_is_silent_rather_than_false() {
    assert_eq!(fires("not minutes_since_open < 30", None), None);
    assert_eq!(fires("minutes_since_open < 30", None), None);
    // The control: with a clock, the same rules have opinions.
    let s = Some(at(10.0, 1370.0, SessionPhase::Overnight));
    assert_eq!(fires("not minutes_since_open < 30", s), Some(false));
    assert_eq!(fires("minutes_since_open < 30", s), Some(true));
}

/// Warmup boundary: a session operand is warm on bar 0, so it adds nothing to
/// the grid's warmup (§2.6). The one extra bar belongs to the crossover.
#[test]
fn a_session_operand_adds_no_warmup() {
    use crate::combo::spec::{ComboSpec, IndicatorSpec, IntAxis};

    let grid = |rule: &str| {
        ComboSpec::new(
            vec![(
                "trend".to_owned(),
                IndicatorSpec::Ema {
                    period: IntAxis::Fixed(7),
                },
            )],
            &RuleSource {
                enter_long: Some(rule.to_owned()),
                ..RuleSource::default()
            },
            Qty(1),
        )
        .expect("valid spec")
        .expand()
        .expect("expands")
    };

    assert_eq!(grid("minutes_since_open > 30").max_warmup_bars(), 7);
    assert_eq!(grid("is_rth > 0").max_warmup_bars(), 7);
    assert_eq!(grid("close < trend").max_warmup_bars(), 7);
    assert_eq!(
        grid("minutes_since_open crosses_above 30").max_warmup_bars(),
        8
    );
}
