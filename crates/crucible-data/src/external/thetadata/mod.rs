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
//! Measured for all nine roots by `crucible theta-floors`, each verified
//! against its previous session and three samples above it (D-0057):
//!
//! | Root | `eod` | `greeks/eod` |
//! |---|---|---|
//! | SPY, IWM, SPX, SPXW, VIX, DIA | 2012-06-01 | **2017-01-03** |
//! | RUT | 2012-06-01 | **2018-12-03** |
//! | QQQ | 2012-06-01 | at or below **2012-06-01** |
//! | NDX | 2012-06-01 | **2026-05-08** |
//!
//! **Six roots share one date**, which makes this a vendor pipeline switching
//! on at the 2016→2017 turn rather than six independent data availabilities —
//! the same kind of boundary as D-0054's 2021→2022 dedup change, and equally
//! invisible to anything computed from the data rather than probed at the edge.
//!
//! A greeks request before a root's floor answers [`STATUS_NO_DATA`] with
//! "No data found for your request". That is an ordinary outcome to record,
//! not a failure to retry, and not a gap to re-attempt on the next run —
//! [`plan`] does not even issue it.
//!
//! ## Module map
//!
//! - [`client`] — the only async code; current-thread runtime behind a sync
//!   API, bounded concurrency, retry policy (D-0025's seam).
//! - [`pacer`] — the one global launch governor every request goes through:
//!   the Terminal's concurrency ceiling, a measured launch floor, capped
//!   backoff, `Retry-After`, and a circuit breaker that ends the run rather
//!   than hammering a Terminal that has stopped answering.
//! - [`schema`] — per-endpoint column pins, validated by name and in order,
//!   plus [`ContractKey`] and the zero-sentinel condition.
//! - [`validate`] — the merge-blocking clauses of `docs/THETADATA_PLAN.md` §4:
//!   dedup by contract key (D-0054), the zero-sentinel drop, the all-zero
//!   series gate, and the reconciliation edges. Every detector has a planted
//!   negative control (CLAUDE.md §7).
//! - [`transcode`] — the sync path from validated response bytes to
//!   `external/thetadata/{options|stocks}/{root}/{type}/{grain}/` parquet,
//!   reusing the curated writer's write-to-temp-then-rename placement.
//! - [`inventory`] — `external/thetadata/inventory.jsonl`: append-only, LF
//!   framed, one line per written file. Same lock-and-reload discipline as
//!   [`Catalog`](crate::Catalog), and **never merged into `manifest.jsonl`**.
//!   Resume is an inventory diff, never a directory listing, so a half-written
//!   file cannot look complete.
//! - [`error`] — failure modes, including the vendor's nonstandard 472.
//!
//! - [`plan`] — expands a tranche (roots × dates × endpoints) into
//!   [`Request`]s, subtracts what the inventory already holds, and refuses to
//!   start when free disk would fall below the floor the run reserves. Every
//!   tranche fires through its dry run first: plan, then execute (D-0024's
//!   pattern, which has now caught a mistake twice).
//!
//! [`Request`]: client::Request
//! [`STATUS_NO_DATA`]: error::STATUS_NO_DATA

pub mod client;
pub mod error;
pub mod inventory;
pub mod pacer;
pub mod plan;
pub mod schema;
pub mod transcode;
pub mod validate;

pub use client::{DEFAULT_BASE_URL, MAX_CONCURRENCY, Request, ThetaClient};
pub use error::{STATUS_NO_DATA, ThetaError};
pub use inventory::{INVENTORY_SCHEMA_VERSION, Inventory, InventoryRecord};
pub use pacer::Pacer;
pub use plan::{DryRunReport, PlannedRequest, TranchePlan, TrancheSpec};
pub use schema::{ColumnIndex, ContractKey, Endpoint, is_zero_sentinel};
pub use validate::{Reconciliation, ValidatedResponse, ValidationReport, reconcile, validate};
