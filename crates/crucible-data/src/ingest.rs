//! Databento acquisition — **M1 spec, not yet implemented.**
//!
//! ## Entitlement windows create DEADLINES (plan-dependent; on Standard as
//! of 2026-07: L0 bars 16+ years; L1 trades/quotes rolling 12 months;
//! L2 MBP-10 and L3 MBO rolling 1 month)
//!
//! The rolling windows slide forward monthly. Data not archived before it
//! exits the window is gone (or costs extra later). Therefore ingest is not
//! a one-off script but a **scheduled job**:
//!
//! - monthly: pull the newest month of `trades`, `tbbo`/`mbp-1`, `mbp-10`,
//!   `mbo` for the tracked symbol set into `raw/`, before the window slides.
//! - once (bootstrap): pull the full 16y of `ohlcv-1s`/`ohlcv-1m` +
//!   `definition` for the tracked symbols.
//! - after each pull: verify checksums, append manifest records, transcode
//!   to curated Parquet.
//!
//! ## Rules
//! - Batch API with `.dbn.zst` outputs (not streaming) for bulk pulls.
//! - `DATABENTO_API_KEY` comes from the **process environment**, read as
//!   late as possible and never stored. A bin target may populate that
//!   environment from a gitignored `.env` at startup (D-0022) — but the key
//!   never appears in a config struct, a CLI argument, a manifest record, or
//!   a log line, and library code never reads it from anywhere.
//! - tokio/async is confined to this crate's bin targets; library code stays
//!   sync so the engine's no-async rule holds trivially.
//! - Timestamps pass through as UTC nanoseconds untouched. The `ohlcv`
//!   `ts_event` marks the interval START — availability is `ts_event + tf`
//!   and is computed by `Bar::avail_ts`, never here.
//! - Symbols: raw contracts (e.g. `ESU6`) are ground truth in `raw/`;
//!   continuous series are *constructed* in `continuous`, never downloaded
//!   pre-stitched, so roll assumptions stay ours and stay explicit.

/// Placeholder for the M1 ingest implementation.
#[derive(Debug, Clone, Copy)]
pub struct IngestPlan;
