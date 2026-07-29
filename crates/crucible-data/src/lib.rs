//! # crucible-data
//!
//! Everything about getting market data and serving it as [`Feed`]s:
//! acquisition from Databento, the immutable local archive + manifest,
//! DBN→Parquet transcoding, session calendars, continuous contracts, and
//! synthetic data for tests/permutation harnesses.
//!
//! `synthetic`, `catalog`, `ingest`, `curated`, and `transcode` are
//! implemented. `calendar` and `continuous` are still **specs encoded as
//! module docs** — read them before implementing them; they carry decisions
//! made at design time that are easy to get subtly wrong (roll conventions,
//! timestamp rules).
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
//!                                 ParquetBarFeed
//!                                       ▼
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
pub mod synthetic;
pub mod transcode;

#[cfg(test)]
mod testutil;

pub use catalog::Catalog;
pub use continuous::{AdjustedPrice, ContinuousBar, ContinuousFeed, RollTable};
pub use curated::ParquetBarFeed;
pub use synthetic::SyntheticFeed;
