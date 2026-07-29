//! # crucible-engine
//!
//! The replay core. One backtest run = one feed + one strategy + one fill
//! model + one contract spec, executed **single-threaded and synchronously**.
//!
//! Why single-threaded? A run is a deterministic fold over a time-ordered
//! event stream; threading inside it buys nothing and risks nondeterminism.
//! Throughput comes from running *many* runs in parallel — that scheduling
//! belongs to `crucible-funnel`, never here. Do not introduce `async`, `std::
//! thread`, or wall-clock time into this crate (CLAUDE.md §2.2, §3).
//!
//! Event-loop order within one event (see `replay.rs`, do not reorder):
//! 1. fills — pending orders placed *strictly before* this event may execute
//! 2. mark  — portfolio marks to the event's close; equity point recorded
//! 3. decide — the strategy sees the event and may emit new order intents

pub mod execution;
pub mod metrics;
pub mod portfolio;
pub mod replay;

pub use execution::{FreeFills, SpreadCrossFills};
pub use metrics::Summary;
pub use portfolio::{ClosedTrade, FeeEvent, Portfolio};
pub use replay::{BacktestParams, BacktestResult, EngineError, run};
