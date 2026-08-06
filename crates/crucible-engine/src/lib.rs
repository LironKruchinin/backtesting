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
//! 1. fills — pending orders placed *strictly before* this event may execute,
//!    then any live protective bracket is tested against this bar
//! 2. mark  — portfolio marks to the event's close; equity point recorded
//! 3. decide — the strategy sees the event and may emit new order intents
//!
//! Two named assumptions govern what a fill costs and when it happens, and
//! both are stated with every number they produce (CLAUDE.md §2.4): the
//! [`execution`] fill model, and — for stops and targets, where one bar can be
//! consistent with two opposite outcomes — the [`bracket`] intrabar ordering
//! convention.
//!
//! Step 2 is also where [`series`] captures the account-evaluation path
//! functionals — per-trading-day PnL, the intraday high-water, and per
//! round-trip excursions. They are captured *inside* the mark loop and never
//! rebuilt from bars afterwards, because a rebuild would re-open the intrabar
//! ordering the [`bracket`] convention already settled and measure a path the
//! account never took (`docs/ACCOUNT_EVAL_SPEC.md` §3.3, D-0071).

pub mod bracket;
pub mod execution;
pub mod metrics;
pub mod portfolio;
pub mod replay;
pub mod series;

pub use bracket::{ActiveBracket, INTRABAR_CONVENTION, Outcome, Resolution, StopFirstIntrabar};
pub use execution::{FreeFills, SpreadCrossFills};
pub use metrics::{ReturnShape, ReturnStats, Summary};
pub use portfolio::{ClosedTrade, FeeEvent, Portfolio};
pub use replay::{BacktestParams, BacktestResult, EngineError, run, run_capturing};
pub use series::{
    AccountCapture, AccountSeries, CaptureError, DayRecord, HighWaterState, PeakCrossing,
    WorstDayDistribution, approximate_day_count, intraday_peak_crossing,
};
