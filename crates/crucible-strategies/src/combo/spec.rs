//! The declared shape of a combo: slots, parameter axes, rules, size.
//!
//! Everything here is plain data with no dependencies (D-0060). A config
//! parser hands [`ComboSpec::new`] a 1:1 transcription of what the file said;
//! this module does all the judging, so every refusal reads the same whether
//! it came from TOML, a test, or a future funnel.

use std::fmt::Write as _;

use crucible_core::types::Qty;

use super::error::ComboError;
use super::grid::Grid;
use super::rules::{RESERVED, RuleSet, RuleSource, SlotDecl, SlotField, canonical_f64};

/// Version tag on the canonical form. Bumping it deliberately invalidates
/// every stored config hash, which is the point: an identity scheme that can
/// change silently is not an identity scheme (D-0012).
pub const CANONICAL_FORM_VERSION: &str = "crucible-combo/1";

/// Largest number of points one axis may hold. An axis longer than this is a
/// typo in a `step`, not a research plan.
const MAX_AXIS_POINTS: usize = 1_000_000;

/// Which indicator fills a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndicatorKind {
    /// Simple moving average — one scalar output.
    Sma,
    /// Exponential moving average — one scalar output.
    Ema,
    /// Bollinger bands — `.mid`, `.upper`, `.lower`, and no scalar.
    Bollinger,
}

impl IndicatorKind {
    /// The config spelling (`kind = "bollinger"`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            IndicatorKind::Sma => "sma",
            IndicatorKind::Ema => "ema",
            IndicatorKind::Bollinger => "bollinger",
        }
    }

    /// What this kind produces, phrased for an error message.
    #[must_use]
    pub const fn fields_help(self) -> &'static str {
        match self {
            IndicatorKind::Sma | IndicatorKind::Ema => "a single value, referenced by name alone",
            IndicatorKind::Bollinger => "`.mid`, `.upper` and `.lower`",
        }
    }

    /// Resolves a rule's field reference (`None` for a bare slot name).
    /// Returns `None` when the kind does not produce that field — which the
    /// parser turns into a load-time error rather than a runtime absence.
    #[must_use]
    pub fn field(self, name: Option<&str>) -> Option<SlotField> {
        match (self, name) {
            (IndicatorKind::Sma | IndicatorKind::Ema, None) => Some(SlotField::Value),
            (IndicatorKind::Bollinger, Some("mid")) => Some(SlotField::Mid),
            (IndicatorKind::Bollinger, Some("upper")) => Some(SlotField::Upper),
            (IndicatorKind::Bollinger, Some("lower")) => Some(SlotField::Lower),
            _ => None,
        }
    }

    /// The parameter names this kind takes, in the order they expand.
    #[must_use]
    pub const fn param_names(self) -> &'static [&'static str] {
        match self {
            IndicatorKind::Sma | IndicatorKind::Ema => &["period"],
            IndicatorKind::Bollinger => &["period", "k"],
        }
    }
}

/// An integer parameter axis, as declared.
///
/// The three forms are equivalent by construction: `Fixed(20)`,
/// `List(vec![20])` and `Range { start: 20, end: 20, step: 1 }` all
/// materialize to `[20]` and therefore hash identically (D-0012 normalizes
/// numbers, and a range *is* a number list written shorter).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntAxis {
    /// One value: an axis of length 1.
    Fixed(usize),
    /// Values written out, in the order they will expand.
    List(Vec<usize>),
    /// `{ start, end, step }`, with an **inclusive** end. If `step` does not
    /// divide the span, the last point is the largest one `<= end`.
    Range {
        /// First value.
        start: usize,
        /// Inclusive upper bound.
        end: usize,
        /// Increment; must be positive.
        step: usize,
    },
}

/// A float parameter axis, as declared.
#[derive(Clone, Debug, PartialEq)]
pub enum FloatAxis {
    /// One value.
    Fixed(f64),
    /// Values written out.
    List(Vec<f64>),
    /// `{ start, end, step }` — **accepted by the type and refused by
    /// [`ComboSpec::new`]**, on purpose. A config that writes a stepped float
    /// axis deserves the explanation in [`ComboError::SteppedFloatAxis`],
    /// not a parser saying `unknown field`.
    Range {
        /// First value.
        start: f64,
        /// Inclusive upper bound.
        end: f64,
        /// Increment.
        step: f64,
    },
}

/// One indicator slot as declared, with axes in place of values.
#[derive(Clone, Debug, PartialEq)]
pub enum IndicatorSpec {
    /// `kind = "sma"`.
    Sma {
        /// Window length in bars.
        period: IntAxis,
    },
    /// `kind = "ema"`.
    Ema {
        /// Smoothing length in bars (α = 2/(period+1), D-0008).
        period: IntAxis,
    },
    /// `kind = "bollinger"`.
    Bollinger {
        /// Window length in bars.
        period: IntAxis,
        /// Band width in population standard deviations (D-0009).
        k: FloatAxis,
    },
}

impl IndicatorSpec {
    /// Which indicator this is.
    #[must_use]
    pub const fn kind(&self) -> IndicatorKind {
        match self {
            IndicatorSpec::Sma { .. } => IndicatorKind::Sma,
            IndicatorSpec::Ema { .. } => IndicatorKind::Ema,
            IndicatorSpec::Bollinger { .. } => IndicatorKind::Bollinger,
        }
    }
}

/// One resolved point on one parameter axis of one slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IndicatorParams {
    /// SMA over `period` bars.
    Sma {
        /// Window length.
        period: usize,
    },
    /// EMA over `period` bars.
    Ema {
        /// Smoothing length.
        period: usize,
    },
    /// Bollinger bands.
    Bollinger {
        /// Window length.
        period: usize,
        /// Band width in standard deviations.
        k: f64,
    },
}

impl IndicatorParams {
    /// Bars this indicator consumes before it produces a reading.
    #[must_use]
    pub const fn warmup_bars(self) -> usize {
        match self {
            IndicatorParams::Sma { period }
            | IndicatorParams::Ema { period }
            | IndicatorParams::Bollinger { period, .. } => period,
        }
    }

    /// Which indicator this is.
    #[must_use]
    pub const fn kind(self) -> IndicatorKind {
        match self {
            IndicatorParams::Sma { .. } => IndicatorKind::Sma,
            IndicatorParams::Ema { .. } => IndicatorKind::Ema,
            IndicatorParams::Bollinger { .. } => IndicatorKind::Bollinger,
        }
    }

    /// `period=20 k=2.0` — used in reports and in combo labels.
    #[must_use]
    pub fn render(self) -> String {
        match self {
            IndicatorParams::Sma { period } | IndicatorParams::Ema { period } => {
                format!("period={period}")
            }
            IndicatorParams::Bollinger { period, k } => {
                format!("period={period} k={}", canonical_f64(k))
            }
        }
    }
}

/// A validated parameter axis: values, materialized, in expansion order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResolvedAxis {
    Int(Vec<usize>),
    Float(Vec<f64>),
}

impl ResolvedAxis {
    pub(crate) fn len(&self) -> usize {
        match self {
            ResolvedAxis::Int(v) => v.len(),
            ResolvedAxis::Float(v) => v.len(),
        }
    }

    fn render(&self, out: &mut String) {
        out.push('[');
        match self {
            ResolvedAxis::Int(values) => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "{v}");
                }
            }
            ResolvedAxis::Float(values) => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&canonical_f64(*v));
                }
            }
        }
        out.push(']');
    }
}

/// A validated slot: a name a rule can reference, a kind, and its axes.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// Slot name as written in the config.
    pub name: String,
    /// Which indicator fills it.
    pub kind: IndicatorKind,
    pub(crate) axes: Vec<ResolvedAxis>,
}

impl Slot {
    /// Number of points on each of this slot's axes, in expansion order.
    #[must_use]
    pub fn axis_lens(&self) -> Vec<usize> {
        self.axes.iter().map(ResolvedAxis::len).collect()
    }

    /// The largest warmup any point on this slot's axes implies.
    ///
    /// Warmup is monotone in `period` for every kind, so this is the max of
    /// the period axis — no need to walk the cartesian product.
    pub(crate) fn max_warmup_bars(&self) -> usize {
        match &self.axes[0] {
            ResolvedAxis::Int(periods) => periods.iter().copied().max().unwrap_or(0),
            ResolvedAxis::Float(_) => {
                unreachable!("INVARIANT: axis 0 of every kind is the integer period")
            }
        }
    }

    /// Resolves one point from per-axis digits.
    ///
    /// # Panics
    /// Panics if `digits` does not match this slot's axis shape. Callers are
    /// inside this crate and the shape comes from [`Slot::axis_lens`], so a
    /// mismatch is a construction bug, not a config error.
    pub(crate) fn resolve(&self, digits: &[usize]) -> IndicatorParams {
        let period = match &self.axes[0] {
            ResolvedAxis::Int(values) => values[digits[0]],
            ResolvedAxis::Float(_) => {
                unreachable!("INVARIANT: axis 0 of every kind is the integer period")
            }
        };
        match self.kind {
            IndicatorKind::Sma => IndicatorParams::Sma { period },
            IndicatorKind::Ema => IndicatorParams::Ema { period },
            IndicatorKind::Bollinger => {
                let k = match &self.axes[1] {
                    ResolvedAxis::Float(values) => values[digits[1]],
                    ResolvedAxis::Int(_) => {
                        unreachable!("INVARIANT: bollinger axis 1 is the float k")
                    }
                };
                IndicatorParams::Bollinger { period, k }
            }
        }
    }
}

/// A validated combo config: what to compute, when to trade, and how big.
///
/// Construct with [`ComboSpec::new`] and expand with [`ComboSpec::expand`].
/// There is no way to build one that a grid cannot be made from.
#[derive(Clone, Debug, PartialEq)]
pub struct ComboSpec {
    slots: Vec<Slot>,
    decls: Vec<SlotDecl>,
    rules: RuleSet,
    qty: Qty,
}

impl ComboSpec {
    /// Validates a transcribed config into something runnable.
    ///
    /// Slots are **sorted by name** here and stay in that order everywhere
    /// afterwards, so neither the grid nor a combo index can depend on the
    /// order a TOML table happened to iterate in (CLAUDE.md §2.2).
    ///
    /// # Errors
    /// [`ComboError`] for anything wrong with the config: a duplicate or
    /// reserved slot name, an empty or duplicated axis, a stepped float axis,
    /// a value outside an indicator's domain, an unparseable rule, a rule
    /// naming a slot or field that does not exist, or a non-positive size.
    pub fn new(
        slots: Vec<(String, IndicatorSpec)>,
        rules: &RuleSource,
        qty: Qty,
    ) -> Result<ComboSpec, ComboError> {
        if slots.is_empty() {
            return Err(ComboError::NoSlots);
        }
        if qty.0 <= 0 {
            return Err(ComboError::NonPositiveQty { qty: qty.0 });
        }
        if rules.is_empty() {
            return Err(ComboError::NoRules);
        }

        let mut slots = slots;
        slots.sort_by(|a, b| a.0.cmp(&b.0));
        for pair in slots.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(ComboError::DuplicateSlot {
                    name: pair[0].0.clone(),
                });
            }
        }

        let mut resolved = Vec::with_capacity(slots.len());
        for (name, spec) in slots {
            check_slot_name(&name)?;
            let kind = spec.kind();
            let axes = materialize(&name, &spec)?;
            resolved.push(Slot { name, kind, axes });
        }

        let decls: Vec<SlotDecl> = resolved
            .iter()
            .map(|s| SlotDecl {
                name: s.name.clone(),
                kind: s.kind,
            })
            .collect();
        let rules = RuleSet::new(rules, &decls)
            .map_err(|(rule, source)| ComboError::Rule { rule, source })?;

        Ok(ComboSpec {
            slots: resolved,
            decls,
            rules,
            qty,
        })
    }

    /// The slots, sorted by name.
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// The parsed rules.
    #[must_use]
    pub const fn rules(&self) -> &RuleSet {
        &self.rules
    }

    /// Position size in contracts (a positive magnitude; direction comes from
    /// the rules).
    #[must_use]
    pub const fn qty(&self) -> Qty {
        self.qty
    }

    /// The deterministic rendering that a config hash is taken over (D-0012,
    /// D-0060).
    ///
    /// It is built from the *parsed and normalized* spec, never from the file
    /// bytes: slots sorted, ranges materialized into their value lists, rules
    /// re-rendered from the AST. So comments, whitespace, table order, and
    /// redundant parentheses cannot change a config's identity, while any
    /// change to what the config *means* does.
    ///
    /// This crate renders it and never hashes it — blake3 belongs to the
    /// caller (CLAUDE.md §3 keeps `crucible-strategies` dependency-free).
    #[must_use]
    pub fn canonical_form(&self) -> String {
        let mut out = String::new();
        out.push_str(CANONICAL_FORM_VERSION);
        out.push('\n');
        let _ = writeln!(out, "qty {}", self.qty.0);
        for slot in &self.slots {
            let _ = write!(out, "slot {} {}", slot.name, slot.kind.name());
            for (name, axis) in slot.kind.param_names().iter().zip(&slot.axes) {
                let _ = write!(out, " {name}=");
                axis.render(&mut out);
            }
            out.push('\n');
        }
        self.rules.render(&self.decls, &mut out);
        out
    }

    /// Expands the axes into a grid.
    ///
    /// # Errors
    /// [`ComboError::GridOverflow`] if the cartesian product does not fit in
    /// a `usize` — which is not a realistic grid, but is a realistic typo.
    pub fn expand(self) -> Result<Grid, ComboError> {
        Grid::new(self)
    }
}

/// Slot names have to be bare identifiers, because a rule references them by
/// writing them.
fn check_slot_name(name: &str) -> Result<(), ComboError> {
    let invalid = |reason: &'static str| {
        Err(ComboError::InvalidSlotName {
            name: name.to_owned(),
            reason,
        })
    };
    if name.is_empty() {
        return invalid("it is empty");
    }
    if !name.is_ascii() {
        return invalid("it is not ASCII, and rules are written in ASCII");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap_or('\0');
    if !(first.is_ascii_alphabetic() || first == '_') {
        return invalid("it must start with a letter or underscore");
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return invalid("it may only contain letters, digits and underscores");
    }
    if RESERVED.contains(&name) {
        return Err(ComboError::ReservedSlotName {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// Turns declared axes into validated value lists, in the kind's parameter
/// order.
fn materialize(slot: &str, spec: &IndicatorSpec) -> Result<Vec<ResolvedAxis>, ComboError> {
    match spec {
        IndicatorSpec::Sma { period } | IndicatorSpec::Ema { period } => {
            Ok(vec![periods(slot, period)?])
        }
        IndicatorSpec::Bollinger { period, k } => Ok(vec![periods(slot, period)?, ks(slot, k)?]),
    }
}

fn periods(slot: &str, axis: &IntAxis) -> Result<ResolvedAxis, ComboError> {
    let values = match axis {
        IntAxis::Fixed(v) => vec![*v],
        IntAxis::List(v) => v.clone(),
        IntAxis::Range { start, end, step } => range_values(slot, "period", *start, *end, *step)?,
    };
    if values.is_empty() {
        return Err(ComboError::EmptyAxis {
            slot: slot.to_owned(),
            param: "period",
        });
    }
    for (i, v) in values.iter().enumerate() {
        if *v == 0 {
            return Err(ComboError::AxisValueOutOfDomain {
                slot: slot.to_owned(),
                param: "period",
                value: "0".to_owned(),
                reason: "a window of zero bars has no mean",
            });
        }
        if values[..i].contains(v) {
            return Err(ComboError::DuplicateAxisValue {
                slot: slot.to_owned(),
                param: "period",
                value: v.to_string(),
            });
        }
    }
    Ok(ResolvedAxis::Int(values))
}

fn ks(slot: &str, axis: &FloatAxis) -> Result<ResolvedAxis, ComboError> {
    let values = match axis {
        FloatAxis::Fixed(v) => vec![*v],
        FloatAxis::List(v) => v.clone(),
        FloatAxis::Range { .. } => {
            return Err(ComboError::SteppedFloatAxis {
                slot: slot.to_owned(),
                param: "k",
            });
        }
    };
    if values.is_empty() {
        return Err(ComboError::EmptyAxis {
            slot: slot.to_owned(),
            param: "k",
        });
    }
    for (i, v) in values.iter().enumerate() {
        if !v.is_finite() || *v <= 0.0 {
            return Err(ComboError::AxisValueOutOfDomain {
                slot: slot.to_owned(),
                param: "k",
                value: canonical_f64(*v),
                reason: "band width is a positive, finite number of standard deviations",
            });
        }
        if values[..i].contains(v) {
            return Err(ComboError::DuplicateAxisValue {
                slot: slot.to_owned(),
                param: "k",
                value: canonical_f64(*v),
            });
        }
    }
    Ok(ResolvedAxis::Float(values))
}

fn range_values(
    slot: &str,
    param: &'static str,
    start: usize,
    end: usize,
    step: usize,
) -> Result<Vec<usize>, ComboError> {
    let bad = |reason: String| ComboError::BadRange {
        slot: slot.to_owned(),
        param,
        reason,
    };
    if step == 0 {
        return Err(bad(
            "step must be positive; a step of 0 never reaches end".to_owned()
        ));
    }
    if start > end {
        return Err(bad(format!(
            "start {start} is past end {end}; the range is written low to high and end is inclusive"
        )));
    }
    let points = (end - start) / step + 1;
    if points > MAX_AXIS_POINTS {
        return Err(bad(format!(
            "{points} points from {start}..={end} step {step}; more than {MAX_AXIS_POINTS} on one \
             axis is a mistyped step, not a search"
        )));
    }
    let mut values = Vec::with_capacity(points);
    let mut v = start;
    for _ in 0..points {
        values.push(v);
        v += step;
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> RuleSource {
        RuleSource {
            enter_long: Some("close < fast".to_owned()),
            ..RuleSource::default()
        }
    }

    fn spec_with(slots: Vec<(String, IndicatorSpec)>) -> Result<ComboSpec, ComboError> {
        ComboSpec::new(slots, &rules(), Qty(1))
    }

    fn sma(axis: IntAxis) -> IndicatorSpec {
        IndicatorSpec::Sma { period: axis }
    }

    /// Hand-listed: 10..=40 step 5 is 10,15,20,25,30,35,40 — seven points,
    /// and 10..=40 step 7 stops at 38 because 45 is past the inclusive end.
    #[test]
    fn integer_ranges_materialize_by_hand() {
        assert_eq!(
            range_values("s", "period", 10, 40, 5),
            Ok(vec![10, 15, 20, 25, 30, 35, 40])
        );
        assert_eq!(
            range_values("s", "period", 10, 40, 7),
            Ok(vec![10, 17, 24, 31, 38])
        );
        assert_eq!(range_values("s", "period", 20, 20, 1), Ok(vec![20]));
    }

    #[test]
    fn broken_ranges_are_refused() {
        assert!(range_values("s", "period", 10, 40, 0).is_err());
        assert!(range_values("s", "period", 40, 10, 5).is_err());
        assert!(range_values("s", "period", 1, 10_000_000, 1).is_err());
    }

    /// A range and the equivalent list must be the same config, or the
    /// registry would hold two rows for one search (D-0012).
    #[test]
    fn a_range_and_its_list_are_the_same_config() {
        let from_range = spec_with(vec![(
            "fast".to_owned(),
            sma(IntAxis::Range {
                start: 10,
                end: 20,
                step: 5,
            }),
        )])
        .expect("valid");
        let from_list = spec_with(vec![(
            "fast".to_owned(),
            sma(IntAxis::List(vec![10, 15, 20])),
        )])
        .expect("valid");
        assert_eq!(from_range.canonical_form(), from_list.canonical_form());
        assert_eq!(
            from_range.canonical_form(),
            "crucible-combo/1\nqty 1\nslot fast sma period=[10,15,20]\n\
             rule enter_long (close < fast)\n"
        );
    }

    /// Declaration order of a slot table must not reach the canonical form,
    /// because a TOML map has no order worth depending on (§2.2).
    #[test]
    fn slot_declaration_order_does_not_matter() {
        let a = ComboSpec::new(
            vec![
                ("slow".to_owned(), sma(IntAxis::Fixed(50))),
                ("fast".to_owned(), sma(IntAxis::Fixed(10))),
            ],
            &rules(),
            Qty(1),
        )
        .expect("valid");
        let b = ComboSpec::new(
            vec![
                ("fast".to_owned(), sma(IntAxis::Fixed(10))),
                ("slow".to_owned(), sma(IntAxis::Fixed(50))),
            ],
            &rules(),
            Qty(1),
        )
        .expect("valid");
        assert_eq!(a.canonical_form(), b.canonical_form());
        assert_eq!(a.slots()[0].name, "fast");
    }

    /// Axis *value* order, by contrast, is content: it decides which combo
    /// index a parameter lands on, so it must change identity.
    #[test]
    fn axis_value_order_is_content() {
        let a =
            spec_with(vec![("fast".to_owned(), sma(IntAxis::List(vec![10, 20])))]).expect("valid");
        let b =
            spec_with(vec![("fast".to_owned(), sma(IntAxis::List(vec![20, 10])))]).expect("valid");
        assert_ne!(a.canonical_form(), b.canonical_form());
    }

    #[test]
    fn duplicate_axis_values_are_refused() {
        let err = spec_with(vec![(
            "fast".to_owned(),
            sma(IntAxis::List(vec![10, 20, 10])),
        )])
        .expect_err("duplicate");
        assert!(matches!(err, ComboError::DuplicateAxisValue { .. }));
        assert!(err.to_string().contains("deflated Sharpe"));
    }

    #[test]
    fn a_stepped_float_axis_explains_itself() {
        let err = ComboSpec::new(
            vec![(
                "bb".to_owned(),
                IndicatorSpec::Bollinger {
                    period: IntAxis::Fixed(20),
                    k: FloatAxis::Range {
                        start: 1.5,
                        end: 2.5,
                        step: 0.5,
                    },
                },
            )],
            &RuleSource {
                enter_long: Some("close < bb.lower".to_owned()),
                ..RuleSource::default()
            },
            Qty(1),
        )
        .expect_err("stepped float");
        assert!(matches!(err, ComboError::SteppedFloatAxis { .. }));
        assert!(err.to_string().contains("Write the values out"));
    }

    #[test]
    fn bad_slot_names_and_domains_are_refused() {
        assert!(matches!(
            spec_with(vec![("close".to_owned(), sma(IntAxis::Fixed(10)))]),
            Err(ComboError::ReservedSlotName { .. })
        ));
        assert!(matches!(
            spec_with(vec![("2fast".to_owned(), sma(IntAxis::Fixed(10)))]),
            Err(ComboError::InvalidSlotName { .. })
        ));
        assert!(matches!(
            spec_with(vec![("fast".to_owned(), sma(IntAxis::Fixed(0)))]),
            Err(ComboError::AxisValueOutOfDomain { .. })
        ));
        assert!(matches!(
            spec_with(vec![("fast".to_owned(), sma(IntAxis::List(vec![])))]),
            Err(ComboError::EmptyAxis { .. })
        ));
        assert!(matches!(
            ComboSpec::new(vec![], &rules(), Qty(1)),
            Err(ComboError::NoSlots)
        ));
        assert!(matches!(
            ComboSpec::new(
                vec![("fast".to_owned(), sma(IntAxis::Fixed(10)))],
                &RuleSource::default(),
                Qty(1)
            ),
            Err(ComboError::NoRules)
        ));
        assert!(matches!(
            ComboSpec::new(
                vec![("fast".to_owned(), sma(IntAxis::Fixed(10)))],
                &rules(),
                Qty(0)
            ),
            Err(ComboError::NonPositiveQty { .. })
        ));
    }

    #[test]
    fn a_rule_naming_an_absent_slot_fails_at_load() {
        let err = ComboSpec::new(
            vec![("fast".to_owned(), sma(IntAxis::Fixed(10)))],
            &RuleSource {
                enter_long: Some("close < slow".to_owned()),
                ..RuleSource::default()
            },
            Qty(1),
        )
        .expect_err("unknown slot");
        assert!(matches!(
            err,
            ComboError::Rule {
                rule: "enter_long",
                ..
            }
        ));
        assert!(err.to_string().contains("unknown indicator slot"));
    }
}
