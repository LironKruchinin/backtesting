//! Grid expansion and combo identity.
//!
//! Expansion is a mixed-radix odometer over a fixed axis order: slots in
//! sorted-name order, and within a slot its parameters in the kind's declared
//! order, with the **last axis varying fastest**. That order is the whole
//! contract — `combo_index` is meaningless without it, and a registry stores
//! combo indices for years — so it is pinned by test rather than by comment.
//!
//! Nothing here iterates a map, reads a clock, or depends on the order the
//! config file happened to be written in (CLAUDE.md §2.2).

use std::fmt;

use super::build::{ComboStrategy, SessionSeries};
use super::error::ComboError;
use super::spec::{ComboSpec, IndicatorParams};

/// The blake3 of a config's canonical form (D-0012).
///
/// This crate cannot compute one — blake3 is not a `crucible-strategies`
/// dependency and must not become one (D-0060). The caller hashes
/// [`ComboSpec::canonical_form`] and hands the digest back, exactly as
/// `acquired_ts` is handed to `crucible-data` so library code never reads a
/// clock (D-0015).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConfigHash([u8; 32]);

impl ConfigHash {
    /// Wraps a 32-byte digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> ConfigHash {
        ConfigHash(bytes)
    }

    /// The raw digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// First 12 hex characters, for report tables. Never use this as an
    /// identity — [`fmt::Display`] renders all 64.
    #[must_use]
    pub fn short(&self) -> String {
        self.to_string().chars().take(12).collect()
    }
}

impl fmt::Display for ConfigHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// What identifies one combo forever: which config, and which point in it.
///
/// `Ord` is derived so parallel results merge by sorting on run identity
/// rather than on completion order (CLAUDE.md §2.2). The fold and the seed
/// join this tuple in the funnel's `RunSpec`; they are not this crate's
/// business.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ComboId {
    /// Identity of the config that produced the grid.
    pub config: ConfigHash,
    /// Index into that grid.
    pub combo_index: usize,
}

impl fmt::Display for ComboId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.config, self.combo_index)
    }
}

/// One slot's resolved parameters at one grid point.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotParams {
    /// Slot name.
    pub name: String,
    /// The parameters this combo gives it.
    pub params: IndicatorParams,
}

/// One point in a grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Combo {
    /// Position in the grid; stable for a given canonical form.
    pub index: usize,
    /// Resolved slots, in sorted-name order.
    pub slots: Vec<SlotParams>,
}

impl Combo {
    /// Bars *this* combo needs before its own indicators are warm.
    ///
    /// Not what it is run with: §2.6 runs every combo on
    /// [`Grid::max_warmup_bars`] so a short-warmup combo gains no extra bars
    /// to trade. This exists so a report can show the difference.
    #[must_use]
    pub fn own_warmup_bars(&self) -> usize {
        self.slots
            .iter()
            .map(|s| s.params.warmup_bars())
            .max()
            .unwrap_or(0)
    }

    /// `fast(period=10) slow(period=50)` — for reports and log lines.
    #[must_use]
    pub fn label(&self) -> String {
        self.slots
            .iter()
            .map(|s| format!("{}({})", s.name, s.params.render()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// An expanded parameter grid.
#[derive(Clone, Debug)]
pub struct Grid {
    spec: ComboSpec,
    /// Points on each axis, slot-major in sorted-slot order. The odometer is
    /// read off this and nothing else.
    axis_lens: Vec<usize>,
    len: usize,
    max_warmup_bars: usize,
    /// One session-clock reading per bar of the series this grid will be
    /// replayed on, or `None` when nothing supplied one (D-0078).
    sessions: Option<SessionSeries>,
}

impl Grid {
    pub(crate) fn new(spec: ComboSpec) -> Result<Grid, ComboError> {
        let axis_lens: Vec<usize> = spec
            .slots()
            .iter()
            .flat_map(super::spec::Slot::axis_lens)
            .collect();
        let mut len = 1usize;
        for points in &axis_lens {
            len = len.checked_mul(*points).ok_or(ComboError::GridOverflow)?;
        }

        // Warmup is monotone in `period` for every kind, so the largest combo
        // is the one taking each slot's longest period — no need to walk the
        // product. `max_warmup_is_the_real_max` proves the shortcut.
        let longest = spec
            .slots()
            .iter()
            .map(super::spec::Slot::max_warmup_bars)
            .max()
            .unwrap_or(0);
        let max_warmup_bars = longest + usize::from(spec.rules().uses_crossover());

        Ok(Grid {
            spec,
            axis_lens,
            len,
            max_warmup_bars,
            sessions: None,
        })
    }

    /// The spec this grid came from.
    #[must_use]
    pub const fn spec(&self) -> &ComboSpec {
        &self.spec
    }

    /// Number of combos.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Always false — a validated [`ComboSpec`] has at least one slot and no
    /// empty axis, so a grid always holds at least one combo. Present because
    /// clippy asks for it next to `len`, and true to the invariant.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Bars every combo in this grid consumes before its orders count —
    /// the max across the grid, which is what §2.6 requires.
    #[must_use]
    pub const fn max_warmup_bars(&self) -> usize {
        self.max_warmup_bars
    }

    /// Resolves one combo.
    ///
    /// # Panics
    /// Panics if `index >= len()`. A grid index out of range is a caller bug,
    /// not a config error — every legitimate source of one is a loop bound or
    /// a registry row written by [`Grid::len`].
    #[must_use]
    pub fn combo(&self, index: usize) -> Combo {
        assert!(
            index < self.len,
            "combo index {index} is outside a grid of {} combos",
            self.len
        );
        // Mixed radix, last axis fastest.
        let mut digits = vec![0usize; self.axis_lens.len()];
        let mut rest = index;
        for (digit, points) in digits.iter_mut().zip(&self.axis_lens).rev() {
            *digit = rest % points;
            rest /= points;
        }

        let mut slots = Vec::with_capacity(self.spec.slots().len());
        let mut at = 0usize;
        for slot in self.spec.slots() {
            let width = slot.axis_lens().len();
            slots.push(SlotParams {
                name: slot.name.clone(),
                params: slot.resolve(&digits[at..at + width]),
            });
            at += width;
        }
        Combo { index, slots }
    }

    /// Every combo, in index order.
    pub fn iter(&self) -> impl Iterator<Item = Combo> + '_ {
        (0..self.len).map(|i| self.combo(i))
    }

    /// Builds the runnable strategy for one combo.
    ///
    /// The result trades on its **own** warmup. Wrap it in
    /// [`crate::align::Aligned`] with [`Grid::max_warmup_bars`] to get the
    /// §2.6 guarantee; [`Grid::aligned_strategy`] does both.
    ///
    /// # Panics
    /// Panics if `index >= len()`, as [`Grid::combo`] does.
    #[must_use]
    pub fn strategy(&self, index: usize) -> ComboStrategy {
        let strategy = ComboStrategy::build(&self.spec, &self.combo(index));
        match &self.sessions {
            Some(sessions) => strategy.with_sessions(sessions.clone()),
            None => strategy,
        }
    }

    /// Attaches the session series every combo in this grid will read (D-0078).
    ///
    /// Set once, after the bar series exists and before anything is replayed,
    /// so every combo scoring a given bar reads the same clock. That is the
    /// point of putting it on the *grid* rather than passing it per strategy:
    /// a per-call argument is a per-call opportunity to pass a different one.
    ///
    /// A grid with no session series builds strategies that have no clock, and
    /// a rule reading one is then silent — which the CLI refuses before it gets
    /// here, and which [`ComboStrategy::session_gaps`] counts if it happens.
    pub fn attach_sessions(&mut self, sessions: SessionSeries) {
        self.sessions = Some(sessions);
    }

    /// Whether a session series has been attached.
    #[must_use]
    pub fn has_sessions(&self) -> bool {
        self.sessions.is_some()
    }

    /// Builds the strategy for one combo, aligned to the grid's warmup so it
    /// is scored on the same bars as every other combo (§2.6).
    ///
    /// # Panics
    /// Panics if `index >= len()`, as [`Grid::combo`] does.
    #[must_use]
    pub fn aligned_strategy(&self, index: usize) -> crate::align::Aligned<ComboStrategy> {
        crate::align::Aligned::new(self.strategy(index), self.max_warmup_bars)
    }
}

impl<'a> IntoIterator for &'a Grid {
    type Item = Combo;
    type IntoIter = Box<dyn Iterator<Item = Combo> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combo::rules::RuleSource;
    use crate::combo::spec::{FloatAxis, IndicatorSpec, IntAxis};
    use crucible_core::types::Qty;

    fn grid() -> Grid {
        // Declared out of alphabetical order on purpose: `bb` must sort first.
        ComboSpec::new(
            vec![
                (
                    "trend".to_owned(),
                    IndicatorSpec::Ema {
                        period: IntAxis::List(vec![100, 200]),
                    },
                ),
                (
                    "bb".to_owned(),
                    IndicatorSpec::Bollinger {
                        period: IntAxis::Range {
                            start: 10,
                            end: 20,
                            step: 5,
                        },
                        k: FloatAxis::List(vec![1.5, 2.0]),
                    },
                ),
            ],
            &RuleSource {
                enter_long: Some("close < bb.lower and close > trend".to_owned()),
                exit_long: Some("close > bb.mid".to_owned()),
                ..RuleSource::default()
            },
            Qty(1),
        )
        .expect("valid spec")
        .expand()
        .expect("expands")
    }

    /// Hand-counted: bb.period has 3 points (10,15,20), bb.k has 2, and
    /// trend.period has 2 — so 3 × 2 × 2 = 12 combos.
    #[test]
    fn combo_count_is_the_cartesian_product() {
        assert_eq!(grid().len(), 12);
    }

    /// The odometer, written out. Axes in order are bb.period, bb.k,
    /// trend.period; the last varies fastest.
    ///
    /// | index | bb.period | bb.k | trend.period |
    /// |---|---|---|---|
    /// | 0 | 10 | 1.5 | 100 |
    /// | 1 | 10 | 1.5 | 200 |
    /// | 2 | 10 | 2.0 | 100 |
    /// | 4 | 15 | 1.5 | 100 |
    /// | 11 | 20 | 2.0 | 200 |
    #[test]
    fn expansion_order_is_pinned() {
        let g = grid();
        let at = |i: usize| g.combo(i).label();
        assert_eq!(at(0), "bb(period=10 k=1.5) trend(period=100)");
        assert_eq!(at(1), "bb(period=10 k=1.5) trend(period=200)");
        assert_eq!(at(2), "bb(period=10 k=2.0) trend(period=100)");
        assert_eq!(at(3), "bb(period=10 k=2.0) trend(period=200)");
        assert_eq!(at(4), "bb(period=15 k=1.5) trend(period=100)");
        assert_eq!(at(11), "bb(period=20 k=2.0) trend(period=200)");
    }

    #[test]
    fn every_combo_index_is_distinct() {
        let g = grid();
        let mut labels: Vec<String> = g.iter().map(|c| c.label()).collect();
        assert_eq!(labels.len(), g.len());
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), g.len());
    }

    /// The analytic shortcut in `Grid::new` must agree with walking the whole
    /// product. If a future indicator's warmup stops being monotone in
    /// `period`, this is what says so.
    #[test]
    fn max_warmup_is_the_real_max() {
        let g = grid();
        let walked = g
            .iter()
            .map(|c| c.own_warmup_bars())
            .max()
            .expect("non-empty");
        // No crossover in these rules, so the grid warmup is exactly the max.
        assert_eq!(walked, 200);
        assert_eq!(g.max_warmup_bars(), walked);
    }

    /// A crossover has no opinion until it has two readings, so it costs one
    /// bar on top of the longest indicator.
    #[test]
    fn a_crossover_costs_one_extra_warmup_bar() {
        let g = ComboSpec::new(
            vec![
                (
                    "fast".to_owned(),
                    IndicatorSpec::Sma {
                        period: IntAxis::Fixed(10),
                    },
                ),
                (
                    "slow".to_owned(),
                    IndicatorSpec::Sma {
                        period: IntAxis::Fixed(50),
                    },
                ),
            ],
            &RuleSource {
                enter_long: Some("fast crosses_above slow".to_owned()),
                ..RuleSource::default()
            },
            Qty(1),
        )
        .expect("valid")
        .expand()
        .expect("expands");
        assert_eq!(g.combo(0).own_warmup_bars(), 50);
        assert_eq!(g.max_warmup_bars(), 51);
    }

    #[test]
    fn combo_ids_render_and_sort_by_identity() {
        let config = ConfigHash::from_bytes([0xab; 32]);
        let a = ComboId {
            config,
            combo_index: 2,
        };
        let b = ComboId {
            config,
            combo_index: 10,
        };
        assert!(a < b, "identity ordering is by index, not by string");
        assert_eq!(config.short(), "ababababab".to_owned() + "ab");
        assert!(a.to_string().ends_with("#2"));
        assert_eq!(a.to_string().len(), 64 + 2);
    }

    #[test]
    #[should_panic(expected = "outside a grid")]
    fn an_out_of_range_index_is_a_caller_bug() {
        let _ = grid().combo(12);
    }
}
