//! ThetaData acquisition: US equity and index **options**, and US **equities**.
//!
//! Complements Databento rather than overlapping it. Databento supplies CME
//! futures (`raw/` + `manifest.jsonl`, D-0017); ThetaData supplies the options
//! chains and their underlyings, lands under `external/thetadata/`, and is
//! inventoried separately because the trust statement is different (D-0049).
//!
//! ## What the subscription actually covers
//!
//! Established by asking the running Terminal, not by reading the published
//! tables — the two disagree, and the Terminal is the one that serves the
//! bytes. It logs the answer at startup:
//!
//! ```text
//! Subscriptions: Stock: STANDARD  Options: PROFESSIONAL  Index: FREE  Rate: FREE
//! Max concurrent requests: 8
//! ```
//!
//! - **Options (PROFESSIONAL)** — tick granularity, history from 2012-06-01,
//!   every endpoint including third-order greeks and the `expiration=*`
//!   whole-chain wildcard.
//! - **Stocks (STANDARD)** — 1-minute granularity (tick is PRO-only), history
//!   from 2016-01-01.
//! - **Index (FREE)** — *no access*. `/index/history/price` answers 403. This
//!   matters less than it sounds: the greeks endpoints carry an
//!   `underlying_price` column, so the SPX/VIX/RUT levels arrive with the
//!   option data anyway.
//! - **Concurrency is 8 globally**, not per asset class.
//!
//! ## Three vendor behaviours that would corrupt research silently
//!
//! 1. **Unknown query parameters are ignored without complaint.** `greeks=true`
//!    on an endpoint with no greeks returns the ordinary columns and HTTP 200.
//!    The response header is therefore the only proof of what was asked for,
//!    and [`schema`] pins it per endpoint — the same instinct as
//!    `deny_unknown_fields` (CLAUDE.md §5.5).
//! 2. **Absent data can arrive as zeros, not as an error.** A 1-minute stock
//!    OHLC request for SPY on 2016-01-04 answers HTTP 200 with 390 rows of
//!    `0.0,0.0,0.0,0.0,0,0`. QQQ on the same date returns real prices, so it
//!    is per-symbol. Validation must reject an all-zero series rather than
//!    archive it as a quiet day.
//! 3. **Timestamps are Eastern wall clock with no offset.** Converted through
//!    real timezone rules by [`crate::calendar::eastern_wall_clock_to_ts`],
//!    which is in `calendar` because CLAUDE.md §4 puts every timezone there.
//!
//! ## History floors are per root and per endpoint, and must be probed
//!
//! Quotes and `eod` reach back to the subscription floor of 2012-06-01. Greeks
//! do not, and the gap is not uniform:
//!
//! | Root | `eod` | `greeks/eod` |
//! |---|---|---|
//! | SPY | 2012-06-01 | **~2017** — no greeks on any 2012–2016 date tested |
//! | QQQ | 2012-06-01 | at least 2013-07 |
//! | NDX | 2012-06-01 | **~2026-06** — the vendor's NDX underlying feed starts 2026-05 |
//!
//! A greeks request before a root's floor answers [`STATUS_NO_DATA`] with
//! "No data found for your request". That is an ordinary outcome to record,
//! not a failure to retry, and not a gap to re-attempt on the next run.
//!
//! ## Module map
//!
//! - [`client`] — the only async code; current-thread runtime behind a sync
//!   API, bounded concurrency, retry policy (D-0025's seam).
//! - [`error`] — failure modes, including the vendor's nonstandard 472.
//!
//! ### Still to implement
//!
//! - `schema` — per-endpoint column pins and CSV row decoding into typed
//!   columns. Prices become `i64` nano-USD, sizes and open interest `i64`,
//!   greeks and implied volatility `f64` (they are statistics, never
//!   accounting, CLAUDE.md §2.3). The pinned header is compared byte-for-byte
//!   before any row is read.
//! - `inventory` — `external/thetadata/inventory.jsonl`: append-only, LF
//!   framed, one line per written file, carrying the request that produced it,
//!   the row count, the blake3 of the parquet, and the vendor floor observed.
//!   Same lock-and-reload discipline as [`Catalog`](crate::Catalog), and
//!   **never merged into `manifest.jsonl`**. Resume is an inventory diff:
//!   never a directory listing, so a half-written file cannot look complete.
//! - `plan` — expands a tranche (roots × dates × endpoints) into [`Request`]s,
//!   subtracts what the inventory already holds, and refuses to start when
//!   free disk would fall below the floor the run reserves.
//! - `transcode` — the sync path from response bytes to
//!   `external/thetadata/{options|stocks}/{root}/{type}/{grain}/` parquet,
//!   reusing the curated writer's write-to-temp-then-rename placement.
//!
//! [`Request`]: client::Request
//! [`STATUS_NO_DATA`]: error::STATUS_NO_DATA

pub mod client;
pub mod error;
pub mod schema;

pub use client::{DEFAULT_BASE_URL, MAX_CONCURRENCY, Request, ThetaClient};
pub use error::{STATUS_NO_DATA, ThetaError};
pub use schema::{ColumnIndex, ContractKey, Endpoint, is_zero_sentinel};
