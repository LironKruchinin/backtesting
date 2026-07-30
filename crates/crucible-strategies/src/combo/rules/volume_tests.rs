//! Volume-operand tests (D-0079).
//!
//! `Bar::volume` has reached the engine since M0; what was missing was a way to
//! name it in a rule. These check that naming it reads the bar's own figure,
//! that it renders as itself in the canonical form, and — the one that matters
//! — that it is **not** touched by the `signal_offset` a stitched series
//! applies to every price field (D-0076).

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

/// A bar that traded `close` on `volume` contracts, sitting `offset` points
/// higher in signal space.
fn bar(close: f64, volume: u64, offset: i64) -> Bar {
    let p = Price::from_points_f64_lossy(close);
    Bar {
        instrument: InstrumentId::new("ES.v.0"),
        tf: TimeFrame::M1,
        ts_open: Ts(0),
        open: p,
        high: p,
        low: p,
        close: p,
        volume,
        signal_offset: Price::from_points(offset),
    }
}

fn fires(text: &str, b: &Bar) -> Option<bool> {
    let expr = parsed(text);
    let mut state = vec![None; 1];
    expr.eval(
        &EvalCtx {
            bar: b,
            slots: &[Some(SlotOut::Scalar(0.0))],
            session: None,
        },
        &mut state,
    )
}

/// Hand-checked: the operand is the bar's own contract count, and the
/// comparison is the ordinary one.
#[test]
fn volume_reads_the_bars_own_contract_count() {
    let b = bar(5000.0, 1500, 0);
    assert_eq!(fires("volume > 1000", &b), Some(true));
    assert_eq!(fires("volume > 1500", &b), Some(false));
    assert_eq!(fires("volume >= 1500", &b), Some(true));
    assert_eq!(fires("volume < 1501", &b), Some(true));
    // Zero is a real reading, not an absent one: an `ohlcv` bar with no trades
    // does not exist, but a synthesized or aggregated one can be zero.
    assert_eq!(fires("volume > 0", &bar(5000.0, 0, 0)), Some(false));
}

/// **The one that matters.** `signal_offset` shifts every price field into
/// signal space (D-0076) and must not touch volume — there is no signal space
/// for a contract count, and an offset that reached it would be adding points
/// to contracts.
///
/// Two-sided: the same bar's `close` moves by the offset and its `volume` does
/// not, so the fixture is proven to carry an offset at all.
#[test]
fn volume_is_not_in_signal_space() {
    let shifted = bar(100.0, 137, 20);
    assert_eq!(fires("volume > 136", &shifted), Some(true));
    assert_eq!(fires("volume > 137", &shifted), Some(false));
    // The control: the price field on the same bar reads 120, not 100.
    assert_eq!(fires("close > 119", &shifted), Some(true));
    assert_eq!(fires("close > 121", &shifted), Some(false));
    // And with no offset the close reads 100, so 120 above was the offset
    // arriving rather than the fixture.
    let plain = bar(100.0, 137, 0);
    assert_eq!(fires("close > 99", &plain), Some(true));
    assert_eq!(fires("close > 101", &plain), Some(false));
    assert_eq!(fires("volume > 136", &plain), Some(true));
}

/// Volume can be crossed like anything else, and the crossover still needs two
/// readings before it has an opinion (the module's rule 2).
#[test]
fn volume_crosses_like_any_other_operand() {
    let expr = parsed("volume crosses_above 100");
    let mut state = vec![None; 1];
    let fired: Vec<_> = [50u64, 90, 150, 200, 10]
        .into_iter()
        .map(|v| {
            let b = bar(5000.0, v, 0);
            expr.eval(
                &EvalCtx {
                    bar: &b,
                    slots: &[],
                    session: None,
                },
                &mut state,
            )
        })
        .collect();
    assert_eq!(
        fired,
        vec![None, Some(false), Some(true), Some(false), Some(false)]
    );
}

/// The canonical form is what gets hashed (D-0012), so the operand renders as
/// itself and is not confusable with anything else.
#[test]
fn volume_renders_as_itself_and_is_reserved() {
    assert_eq!(rendered("volume > 1000"), "(volume > 1000.0)");
    assert_eq!(
        rendered("  ( volume  >  1000 )  "),
        rendered("volume > 1000")
    );
    assert_ne!(rendered("volume > 1000"), rendered("close > 1000"));
    assert!(RESERVED.contains(&"volume"));
    assert!(
        rejected("volume.mid > 0")
            .message
            .contains("is a bar field")
    );
}

/// A slot may not be named `volume`, or a rule mentioning it would silently
/// mean the indicator instead of the bar's traded size.
#[test]
fn a_slot_named_volume_is_refused() {
    use crate::combo::spec::{ComboSpec, IndicatorSpec, IntAxis};
    use crate::combo::{ComboError, RuleSource};

    let err = ComboSpec::new(
        vec![(
            "volume".to_owned(),
            IndicatorSpec::Sma {
                period: IntAxis::Fixed(10),
            },
        )],
        &RuleSource {
            enter_long: Some("close < volume".to_owned()),
            ..RuleSource::default()
        },
        Qty(1),
    )
    .expect_err("a slot may not shadow a bar field");
    assert!(matches!(err, ComboError::ReservedSlotName { .. }), "{err}");
}

/// Warmup boundary: a volume operand is warm on bar 0, so it adds nothing to
/// the grid's warmup — the only cost is the one extra bar a crossover always
/// pays for its second reading.
///
/// This matters for §2.6: if reading `volume` silently lengthened warmup, every
/// combo in the grid would start later and the eval window would shrink for a
/// reason nobody wrote down.
#[test]
fn a_volume_operand_adds_no_warmup() {
    use crate::combo::RuleSource;
    use crate::combo::spec::{ComboSpec, IndicatorSpec, IntAxis};

    let grid = |rule: &str| {
        ComboSpec::new(
            vec![(
                "mid".to_owned(),
                IndicatorSpec::Sma {
                    period: IntAxis::Fixed(5),
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

    // The SMA's five bars and nothing more.
    assert_eq!(grid("volume > 1000").max_warmup_bars(), 5);
    assert_eq!(grid("close < mid").max_warmup_bars(), 5);
    // ...and the crossover's extra bar is the crossover's, not volume's.
    assert_eq!(grid("volume crosses_above 1000").max_warmup_bars(), 6);
    assert_eq!(grid("close crosses_above mid").max_warmup_bars(), 6);
}
