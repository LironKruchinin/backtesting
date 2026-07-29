//! Config-driven indicator combos — the layer that turns "test an idea" from
//! writing Rust into writing TOML.
//!
//! **Spec, ahead of the implementation.** This module doc is the design; the
//! code lands next, module by module, against it (CLAUDE.md §5.3 — an
//! unimplemented module keeps its full spec in `//!` docs, and the spec is
//! done only when the module is).
//!
//! A combo config names **indicator slots**, writes **rules** that reference
//! those slots by name, and declares **parameter axes** instead of parameter
//! values. Expanding the axes gives a grid; one grid point plus the rules
//! builds one runnable [`Strategy`](crucible_core::traits::Strategy).
//!
//! ```text
//!   TOML  ──parse──▶  ComboSpec  ──expand──▶  Grid  ──index──▶  Combo
//!  (cli)              (this module)                              │
//!                                                          build │
//!                                                                ▼
//!                                                        ComboStrategy
//! ```
//!
//! # Where the pieces are allowed to live (D-0060)
//!
//! This crate depends on `crucible-core` and nothing else, forever (CLAUDE.md
//! §3), and §6 blesses `serde`/`toml` for `funnel`, `data` and `cli` — **not**
//! for `strategies`. So the split is:
//!
//! | Concern | Home | Why |
//! |---|---|---|
//! | spec types, axes, rule AST, expansion, factory | here | pure data + pure functions; no I/O, no deps |
//! | TOML text → [`ComboSpec`] | `crucible-cli::config` (later `funnel`) | serde lives where §6 put it |
//! | `blake3(canonical_form)` → config hash | the caller | blake3 is not a `strategies` dependency |
//!
//! [`ComboSpec::canonical_form`] is this crate's half of D-0012: a
//! deterministic, dependency-free rendering of the *parsed and normalized*
//! spec. Whoever owns blake3 hashes that string. `strategies` never computes
//! an identity, exactly as `crucible-data` never reads a clock (D-0015) —
//! the value is supplied, so the crate cannot acquire a dependency by
//! accident.
//!
//! # The config surface
//!
//! Indicator slots are **tagged** by `kind` and carry axes rather than
//! scalars:
//!
//! ```toml
//! [indicators.bb]
//! kind = "bollinger"
//! period = { start = 10, end = 40, step = 5 }   # inclusive range
//! k = [1.5, 2.0, 2.5]                           # explicit list
//!
//! [indicators.trend]
//! kind = "ema"
//! period = 200                                  # a scalar is an axis of one
//!
//! [rules]
//! enter_long  = "close < bb.lower and close > trend"
//! exit_long   = "close > bb.mid"
//! ```
//!
//! Slot names are sorted before anything else happens, so the grid does not
//! depend on the order TOML tables happen to appear in (§2.2 bans letting map
//! iteration order reach results — sorting is how this module obeys it).
//!
//! ## Axes
//!
//! - Integer axes accept a scalar, a list, or `{ start, end, step }` with an
//!   **inclusive** end. If `step` does not divide the span, the last value is
//!   the largest one `<= end`; that is arithmetic, not a typo, so it is
//!   allowed and documented rather than refused.
//! - **Float axes accept lists only.** `{ start = 1.5, end = 2.5, step = 0.1 }`
//!   is refused, naming the alternative: repeated addition of 0.1 lands on
//!   2.0000000000000004, so the number of points on the axis — and therefore
//!   the combo count, every combo index, and every trial charged to the
//!   hypothesis family — would depend on floating-point accumulation. A grid
//!   whose *size* is a rounding artifact is not reproducible (§2.5).
//! - **Duplicate values in an axis are a hard error.** Two identical combos
//!   are two trials of one hypothesis (§4, "trial"), and a trial count that
//!   overstates the search deflates the deflated Sharpe against a search that
//!   never happened.
//!
//! ## The rule language
//!
//! Rules are boolean expressions parsed once into an [`Expr`](rules::Expr) AST at
//! config-load time and evaluated per bar with no allocation and no string
//! handling in the loop.
//!
//! ```text
//! expr       := or
//! or         := and ("or" and)*
//! and        := unary ("and" unary)*
//! unary      := "not" unary | "(" expr ")" | comparison
//! comparison := operand ("<" | "<=" | ">" | ">=" | "crosses_above"
//!                        | "crosses_below") operand
//! operand    := number | price_field | slot ("." field)?
//! price_field := "open" | "high" | "low" | "close"
//! ```
//!
//! - `a crosses_above b` is true on the bar where `a` goes from `<= b` to
//!   `> b` — the same two-reading rule [`SmaCross`](crate::SmaCross) implements by
//!   hand, which is what lets a TOML config reproduce it bit for bit (there
//!   is a test asserting exactly that).
//! - `==` is **not** in the grammar. Two `f64`s in indicator space are equal
//!   by coincidence, never by intent.
//! - Arithmetic on operands (`close > trend * 1.001`) is **not** in the
//!   grammar either, on purpose: every operator added here is something the
//!   evaluator must compute identically forever. Operators arrive when a
//!   strategy needs one, with their own tests, not speculatively.
//! - `open`/`high`/`low`/`close`, the keywords, and the operator words are
//!   reserved: a slot may not be named one of them.
//! - A slot reference must name a field the slot's kind actually produces.
//!   `bb` alone (Bollinger has three outputs and no scalar) and `trend.upper`
//!   (an EMA has no bands) are both load-time errors, not runtime `None`.
//!
//! ## Rules to position
//!
//! One target position is computed per bar, from the position the portfolio
//! actually holds — never from a position the strategy hopes it has:
//!
//! ```text
//! target := view.position
//! if target > 0 and exit_long  fires  -> target = 0
//! if target < 0 and exit_short fires  -> target = 0
//! if target == 0:
//!     enter_long only   -> +qty
//!     enter_short only  -> -qty
//!     both              -> stay flat, and count it
//! ```
//!
//! Exit-then-enter within one bar is deliberate: it is what makes a
//! stop-and-reverse config (the same condition as `exit_long` and
//! `enter_short`) emit the single order `SmaCross` emits.
//!
//! **Both entries firing at once takes no position.** A config where long and
//! short can be simultaneously true is a bug in the config, but it is only
//! discoverable at runtime, and a strategy has no error channel — so picking
//! one arbitrarily would be a silently-wrong result, the failure mode this
//! project exists to prevent. Instead the bar is skipped and
//! [`ComboStrategy::conflicting_signals`] counts it, so the caller can print
//! a number that is never supposed to be nonzero.
//!
//! # Fair comparison (§2.6)
//!
//! Warmup is a property of the **grid**, not of a combo:
//! [`Grid::max_warmup_bars`] is the max across every combo, and
//! [`Aligned`](crate::align::Aligned) wraps any strategy so it consumes bars — warming
//! its indicators — while its orders are discarded until that bar count is
//! reached. Every combo in a grid therefore places its first order on the
//! same bar index, and a combo with a shorter warmup gains no extra bars to
//! trade. The equity curve carries an identical flat prefix for every combo,
//! so each naive Sharpe carries the same `sqrt(n_eval / n_total)` factor;
//! slicing the eval window out of the *metrics* is the walk-forward runner's
//! job (engine side, next M2 task), not this module's.
//!
//! # Determinism (§2.2)
//!
//! - Slots are sorted by name; axes keep the order they were declared in.
//! - Expansion is mixed-radix over that fixed axis order, last axis varying
//!   fastest, so `combo_index` is a pure function of the canonical form.
//! - A combo's identity is [`ComboId`] = (config
//!   hash, combo index) — stable enough to sort parallel results on (§2.2
//!   forbids merging by completion order) and to dedupe a registry.
//! - Nothing here reads a clock, a map iterator, or an address.
//!
//! # Not here yet
//!
//! Stops and targets in ticks — spec'd in this module's earlier draft — are a
//! separate M2 item, because a stop is resolved *inside* a bar and OHLC does
//! not say whether the high or the low came first. That is an execution
//! assumption (§2.4) and belongs to `crucible-engine::execution` with the
//! worst-case ordering convention and a path-sensitivity flag on the
//! scorecard, not to a rule that only ever sees completed bars.

pub mod build;
pub mod error;
pub mod grid;
pub mod rules;
pub mod spec;

pub use build::ComboStrategy;
pub use error::ComboError;
pub use grid::{Combo, ComboId, ConfigHash, Grid, SlotParams};
pub use rules::{RuleSet, RuleSource};
pub use spec::{
    CANONICAL_FORM_VERSION, ComboSpec, FloatAxis, IndicatorKind, IndicatorParams, IndicatorSpec,
    IntAxis, Slot,
};
