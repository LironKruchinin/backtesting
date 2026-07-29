//! Everything a combo config can be wrong about, in one enum.
//!
//! Every variant here is a *load-time* refusal. That is the whole point: a
//! config typo must be a hard error before a run starts (CLAUDE.md §5.5), not
//! a rule that silently never fires. Nothing in this module is recoverable —
//! the caller's only sane response is to print it and stop.

use std::fmt;

use super::rules::RuleError;

/// A combo config that cannot be turned into a runnable grid.
#[derive(Clone, Debug, PartialEq)]
pub enum ComboError {
    /// No indicator slots were declared.
    NoSlots,
    /// Two slots share a name.
    DuplicateSlot {
        /// The repeated name.
        name: String,
    },
    /// A slot is named after a price field, a keyword, or an operator word.
    ReservedSlotName {
        /// The offending name.
        name: String,
    },
    /// A slot name is not a bare identifier, so a rule could not reference it.
    InvalidSlotName {
        /// The offending name.
        name: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// A parameter axis has no values, so the grid would be empty.
    EmptyAxis {
        /// Slot the axis belongs to.
        slot: String,
        /// Parameter name.
        param: &'static str,
    },
    /// An axis lists the same value twice, which would double-count a trial.
    DuplicateAxisValue {
        /// Slot the axis belongs to.
        slot: String,
        /// Parameter name.
        param: &'static str,
        /// The repeated value, rendered.
        value: String,
    },
    /// A float axis was written as `{ start, end, step }`.
    SteppedFloatAxis {
        /// Slot the axis belongs to.
        slot: String,
        /// Parameter name.
        param: &'static str,
    },
    /// An integer range is not traversable.
    BadRange {
        /// Slot the axis belongs to.
        slot: String,
        /// Parameter name.
        param: &'static str,
        /// What is wrong with it.
        reason: String,
    },
    /// An axis value is outside the indicator's domain.
    AxisValueOutOfDomain {
        /// Slot the axis belongs to.
        slot: String,
        /// Parameter name.
        param: &'static str,
        /// The offending value, rendered.
        value: String,
        /// The domain it violated.
        reason: &'static str,
    },
    /// Every rule was omitted, so the strategy can never take a position.
    NoRules,
    /// A rule failed to parse or referenced something that does not exist.
    Rule {
        /// Which rule (`enter_long`, …).
        rule: &'static str,
        /// The parse or resolution failure.
        source: RuleError,
    },
    /// Position size is not a positive contract count.
    NonPositiveQty {
        /// What was given.
        qty: i32,
    },
    /// The cartesian product overflowed `usize`. Not a realistic grid — a
    /// realistic *typo*, and one that must not wrap into a small number.
    GridOverflow,
}

impl fmt::Display for ComboError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComboError::NoSlots => write!(
                f,
                "no indicator slots declared; a combo needs at least one [indicators.<name>]"
            ),
            ComboError::DuplicateSlot { name } => {
                write!(f, "indicator slot {name:?} is declared twice")
            }
            ComboError::ReservedSlotName { name } => write!(
                f,
                "indicator slot {name:?} uses a reserved word; open/high/low/close, \
                 and/or/not, and crosses_above/crosses_below already mean something \
                 in a rule"
            ),
            ComboError::InvalidSlotName { name, reason } => {
                write!(
                    f,
                    "indicator slot {name:?} is not usable in a rule: {reason}"
                )
            }
            ComboError::EmptyAxis { slot, param } => write!(
                f,
                "{slot}.{param} has no values; an empty axis makes the whole grid empty"
            ),
            ComboError::DuplicateAxisValue { slot, param, value } => write!(
                f,
                "{slot}.{param} lists {value} more than once. Two identical combos are \
                 two trials charged to the hypothesis family for one search, which \
                 deflates the deflated Sharpe against work nobody did"
            ),
            ComboError::SteppedFloatAxis { slot, param } => write!(
                f,
                "{slot}.{param} is a float axis and must be an explicit list. A stepped \
                 float axis has a floating-point-dependent length — adding 0.1 five \
                 times lands on 0.5000000000000001 — so the combo count, every combo \
                 index, and every trial count would depend on the FPU (D-0060). Write \
                 the values out"
            ),
            ComboError::BadRange {
                slot,
                param,
                reason,
            } => write!(f, "{slot}.{param} range is not traversable: {reason}"),
            ComboError::AxisValueOutOfDomain {
                slot,
                param,
                value,
                reason,
            } => write!(f, "{slot}.{param} = {value} is invalid: {reason}"),
            ComboError::NoRules => write!(
                f,
                "no rules declared; a combo with no entry rule can never take a position"
            ),
            ComboError::Rule { rule, source } => write!(f, "rule {rule}: {source}"),
            ComboError::NonPositiveQty { qty } => write!(
                f,
                "qty_contracts = {qty}; position size is a positive magnitude and \
                 direction comes from the rules"
            ),
            ComboError::GridOverflow => write!(
                f,
                "the parameter grid does not fit in memory address space, which means \
                 an axis is far larger than intended"
            ),
        }
    }
}

impl std::error::Error for ComboError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ComboError::Rule { source, .. } => Some(source),
            _ => None,
        }
    }
}
