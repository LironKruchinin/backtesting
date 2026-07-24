//! # crucible-core
//!
//! Domain types, market events, and the trait seams every other crate plugs
//! into. This crate is the vocabulary of the project: if a concept is defined
//! here, its meaning is pinned here and in `CLAUDE.md` §4 (glossary), and no
//! other crate may redefine it.
//!
//! Hard rules for this crate (see `CLAUDE.md`):
//! - Zero dependencies, zero I/O, zero threads. Pure data and traits.
//! - Prices and money are fixed-point integers ([`types::Price`], nano-USD).
//!   `f64` never touches accounting.
//! - Every market event knows its **availability time** (`avail_ts`) — the
//!   instant the information could first have been known. All replay ordering
//!   is by `avail_ts`, never by `ts_event`/`ts_open`.

pub mod events;
pub mod traits;
pub mod types;

pub mod prelude {
    //! Convenience re-exports: `use crucible_core::prelude::*;`
    pub use crate::events::{Bar, Fill, MarketEvent, Order, OrderIntent, OrderKind, Side};
    pub use crate::traits::{Actions, Feed, FillModel, Indicator, PortfolioView, Strategy};
    pub use crate::types::{
        ContractSpec, InstrumentId, NanoUsd, Price, Qty, TimeFrame, Ts, nano_usd_to_f64,
    };
}
