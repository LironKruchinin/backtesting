//! # crucible-data
//!
//! Everything about getting market data and serving it as [`Feed`]s:
//! acquisition from Databento, the immutable local archive + manifest,
//! DBN→Parquet transcoding, session calendars, continuous contracts, and
//! synthetic data for tests/permutation harnesses.
//!
//! `synthetic`, `catalog`, and `ingest` are implemented. `calendar` and
//! `continuous` are still **specs encoded as module docs** — read them before
//! implementing them; they carry decisions made at design time that are easy
//! to get subtly wrong (roll conventions, timestamp rules).
//!
//! Crate-wide rules:
//! - This is the ONLY crate that touches the network or the filesystem for
//!   market data, and the only place `async`/tokio may appear — confined to
//!   `ingest::databento`, behind the non-default `databento` feature, which
//!   owns a private current-thread runtime behind a sync trait (D-0025).
//! - The public API is **sync**, always.
//! - Raw archive files are append-only and never mutated or deleted by code.
//! - Every timestamp handed to the engine is UTC nanoseconds. Session/
//!   timezone logic stays in `calendar`.
//!
//! [`Feed`]: crucible_core::traits::Feed

pub mod calendar;
pub mod catalog;
pub mod continuous;
pub mod ingest;
pub mod synthetic;

#[cfg(test)]
mod testutil;

pub use catalog::Catalog;
pub use synthetic::SyntheticFeed;
