//! Streaming indicators, close-based in v0.
//!
//! Numerical note: rolling-sum implementations (SMA, Bollinger) accumulate
//! float drift over very long streams. Acceptable for the skeleton; milestone
//! M2 replaces the accumulators with periodically-rebased or Welford-style
//! updates. Do not "optimize" these back to naive `sum/period` recomputation
//! over the window — O(period) per bar breaks grid-search throughput.

mod bollinger;
mod ema;
mod sma;

pub use bollinger::{Bollinger, BollingerBands};
pub use ema::Ema;
pub use sma::Sma;
