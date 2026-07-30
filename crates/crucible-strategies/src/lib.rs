//! # crucible-strategies
//!
//! Streaming indicators and the strategies built from them.
//!
//! Design rules (CLAUDE.md §3, §5):
//! - Indicators are **incremental**: O(1) state update per bar, `None` until
//!   warm. No indicator ever replays history internally — history replay is
//!   the engine's job, once, for everyone.
//! - Indicators compute in `f64` *points* (via `Price::as_points_f64`), never
//!   raw nanos (which exceed f64 integer precision when squared) and never
//!   in accounting space.
//! - Strategies own their indicators. Strategy structs are cheap and small so
//!   the funnel can run thousands of parameter variants per data pass.
//! - We write indicators by hand instead of pulling a TA crate: streaming
//!   semantics, testability against hand-computed values, and control over
//!   numerical behavior are the point (docs/DECISIONS.md D-0007).

pub mod align;
pub mod bracket;
pub mod combo;
pub mod indicators;
pub mod sma_cross;

pub use align::Aligned;
pub use bracket::Bracketed;
pub use combo::{ComboSpec, ComboStrategy, Grid};
pub use indicators::{Bollinger, BollingerBands, Ema, Sma};
pub use sma_cross::SmaCross;
