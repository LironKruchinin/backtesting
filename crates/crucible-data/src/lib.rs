//! # crucible-data
//!
//! Everything about getting market data and serving it as [`Feed`]s:
//! acquisition from Databento, the immutable local archive + manifest,
//! DBN→Parquet transcoding, session calendars, continuous contracts, and
//! synthetic data for tests/permutation harnesses.
//!
//! Every module is implemented as of M1. `calendar` says when the exchange was
//! open, `continuous` stitches expiring contracts into one series, and `qa`
//! reads the others back and asks whether the archive is any good — start there
//! if you want to know what the data actually looks like rather than how it got
//! here.
//!
//! The path a bar takes, and where to read about each leg:
//!
//! ```text
//! Databento batch API ──ingest──► raw/*.dbn.zst  +  manifest.jsonl
//!                                       │
//!                                   transcode
//!                                       ▼
//!                     curated/bars/{instrument}/{tf}/{window}.parquet
//!                                       │
//!                        ┌──────────────┼──────────────┐
//!                  ParquetBarFeed       │         continuous
//!                        │              │              │
//!                        │             qa   curated/rolls/{root}/{tf}/*.json
//!                        │        (+ calendar)         │
//!                        │                       ContinuousFeed
//!                        ▼                             ▼
//!                              crucible-engine::run
//! ```
//!
//! Crate-wide rules:
//! - This is the ONLY crate that touches the network or the filesystem for
//!   market data, and the only place `async`/tokio may appear — confined to
//!   `ingest::databento`, behind the non-default `databento` feature, which
//!   owns a private current-thread runtime behind a sync trait (D-0025).
//! - The public API is **sync**, always.
//! - No thread ever starts here. Parallelism is `crucible-funnel`'s alone
//!   (CLAUDE.md §3), which is why the Parquet layer is `parquet` rather than
//!   a DataFrame library carrying a work-stealing pool (D-0037).
//! - Raw archive files are append-only and never mutated or deleted by code.
//!   Curated files are derived, disposable, and freely replaced.
//! - Every timestamp handed to the engine is UTC nanoseconds. Session/
//!   timezone logic stays in `calendar`.
//!
//! [`Feed`]: crucible_core::traits::Feed

pub mod calendar;
pub mod catalog;
pub mod continuous;
pub mod curated;
pub mod external;
pub mod ingest;
pub mod layout;
pub mod qa;
pub mod synthetic;
pub mod transcode;

#[cfg(test)]
mod testutil;

pub use catalog::Catalog;
pub use continuous::{AdjustedPrice, ContinuousBar, ContinuousFeed, RollTable};
pub use curated::ParquetBarFeed;
pub use synthetic::SyntheticFeed;
