//! Databento acquisition.
//!
//! **Status: implemented, end to end.** [`plan()`] and [`quote()`] cost
//! nothing and spend nothing; [`execute()`](execute::execute) is the only
//! thing in the workspace that can spend money, and it runs only after
//! [`quote::gate`] has authorised a total against a cap the operator typed.
//!
//! Read [`execute`] before changing anything here: it documents the four
//! guards that stand between a dropped connection and a second purchase, and
//! every one of them is load-bearing.
//!
//! ## Entitlement windows create DEADLINES
//!
//! Plan-dependent. On Standard (verified 2026-07-24): L0 bars 16+ years;
//! L1 `trades`/`tbbo`/`bbo`/`mbp-1` rolling 12 months; L2 `mbp-10` and L3
//! `mbo` rolling 1 month.
//!
//! The rolling windows slide forward monthly. Data not archived before it
//! exits the window is gone (or costs extra later). Therefore ingest is not
//! a one-off script but a **scheduled job**:
//!
//! - monthly: pull the newest month of `trades`, `tbbo`, and `mbo` for the
//!   tracked symbol set into `raw/`, before the window slides.
//! - once (bootstrap): pull the full history of `ohlcv-1s`/`ohlcv-1m` +
//!   `definition` + `statistics` for the tracked symbols.
//! - after each pull: verify checksums, append manifest records, transcode
//!   to curated Parquet.
//!
//! `mbp-10` is deliberately NOT archived (D-0023). `mbo` is the strictly
//! richer L3 book and is what M4's queue model calibrates against, at
//! 16.5 GiB/month against `mbp-10`'s 86.5 — five times the disk for a view
//! derivable from bytes we already hold.
//!
//! ## What the data actually costs
//!
//! Verified 2026-07-24 against `GLBX.MDP3` through the free metadata
//! endpoints. These are quotes, not estimates; re-derive them with a dry run
//! rather than trusting this table to stay current.
//!
//! **Availability.** Every schema starts `2010-06-06` EXCEPT `mbo`, which
//! starts `2017-05-21`. Requested ranges are clamped to these — asking for
//! 16y of `mbo` is asking for data that does not exist. [`plan()`] fetches the
//! live range rather than hardcoding those dates, so they cannot rot.
//!
//! **Unit prices**, USD per billable GB (`historical` and
//! `historical-streaming` price identically, so batching is a throughput
//! choice, not a cost one):
//!
//! | $/GB | schemas |
//! |---|---|
//! | 0.50 | `mbp-10` |
//! | 1.00 | `statistics` |
//! | 1.70 | `definition` |
//! | 1.80 | `mbo`, `mbp-1` |
//! | 18.00 | `bbo-1s`, `bbo-1m` |
//! | 28.00 | `trades`, `tbbo` |
//! | 70.00 | `ohlcv-1s`, `ohlcv-1m` |
//! | 190.00 | `ohlcv-1h`, `ohlcv-1d` |
//!
//! The counter-intuitive consequence: **aggregates cost the most per byte**,
//! so the bill tracks volume, not tier depth. `ohlcv-1s` and `ohlcv-1m` are
//! the same $70/GB and differ only in size — 16y of `ohlcv-1s` for
//! ES+NQ+RTY is 19.67 GiB, i.e. **$1,377** metered, more than every L1/L2/L3
//! tier on the bootstrap list combined. One-second bars, not the order book,
//! are the expensive thing. Do not reason about this from tier names.
//!
//! Standard's $199/month *includes* unlimited historical inside those
//! windows rather than metering on top, so the entire bootstrap list
//! (~53 GiB, ~$1,901 metered) is acquired inside one subscription month,
//! after which the recurring monthly job costs ~$63 metered (D-0023). That
//! makes the subscription a **30-day clock against a large batch run**,
//! which is precisely why the execute path must be finished and
//! unattended-capable before it starts.
//!
//! ## Rules
//! - **Quote before you buy** (D-0024). Pricing is a dry run by default: it
//!   reports `get_cost` / `get_billable_size` per window and stops. Spending
//!   requires an explicit opt-in *and* a `--max-cost-usd` cap, which
//!   [`quote::gate`] enforces with integer comparison. A fat-fingered date
//!   range must not silently become a four-figure charge.
//! - **Quote only what we do not already own.** [`plan()`] subtracts
//!   [`Catalog::coverage`](crate::catalog::Catalog::coverage) from the
//!   request, so a re-run after a partial pull quotes only the remainder.
//!   That also makes coverage subtraction testable end to end for free,
//!   before it is trusted to guard four figures of entitlement.
//! - **Every uncertainty is a refusal.** An unobtainable cost quote, an
//!   unreachable dataset range, and a plan whose windows fail their tiling
//!   check all stop the run. A false refusal costs a re-run; a false proceed
//!   costs money.
//! - Batch API with `.dbn.zst` outputs (not streaming) for bulk pulls.
//! - `DATABENTO_API_KEY` comes from the **process environment**, read as
//!   late as possible and never stored. A bin target may populate that
//!   environment from a gitignored `.env` at startup (D-0022) — but the key
//!   never appears in a config struct, a CLI argument, a manifest record, or
//!   a log line, and library code never reads it from anywhere.
//! - The public API of this crate is **sync**. The Databento client is
//!   async, so the single module that will call it owns a private
//!   current-thread tokio runtime behind [`provider::BatchProvider`]
//!   (D-0025); `async fn` and `.await` appear nowhere else.
//! - Timestamps pass through as UTC nanoseconds untouched. The `ohlcv`
//!   `ts_event` marks the interval START — availability is `ts_event + tf`
//!   and is computed by `Bar::avail_ts`, never here.
//! - Symbols: raw contracts (e.g. `ESU6`) are ground truth in `raw/`;
//!   continuous series are *constructed* in `continuous`, never downloaded
//!   pre-stitched, so roll assumptions stay ours and stay explicit.
//!   Pre-stitched continuous symbology is also ~10% cheaper to download
//!   (measured 2026-07-24) — declined anyway, because a roll rule we did
//!   not choose is a research assumption we cannot defend.
//!
//! ## The execute path, as built
//!
//! Submit → poll → download to a staging dir → verify size and SHA-256 →
//! `rename` into `raw/` → [`Catalog::append`](crate::catalog::Catalog::append)
//! → journal it. Nothing is ever downloaded straight into `raw/`; raw is
//! immutable.
//!
//! - **One job = one window = one archive file = one manifest row** (D-0028).
//!   Submissions pin `split_duration=none`, because the vendor's default of
//!   `day` would deliver ~31 files for one month.
//! - **The journal records the intent before the submission** and every run
//!   reconciles against the vendor's own job list, so a lost journal or a
//!   crash inside `submit` cannot cause a second purchase (D-0029). One pull
//!   at a time, enforced by an OS lock (D-0030).
//! - **`ManifestRecord.symbols` is the requested key plus every raw symbol
//!   observed in the delivered file's DBN metadata** (D-0033). Recording only
//!   the key would make `coverage("ESU6")` report the full range as missing
//!   and re-buy it forever. Only the *key* has to survive the path-component
//!   rule; the rest are coverage data and may carry the spaces and colons CME
//!   puts in spread names (D-0066). [`supplement`] repairs records written
//!   before that distinction existed.
//! - **A delivery is verified before it is placed** (D-0031): the catalog
//!   hashes whatever is on disk, so an unchecked truncated download would be
//!   recorded and thereafter certified by the manifest as the real thing.
//!
//! The whole state machine is tested offline against faked seams — including
//! every crash-resume path and every refusal — because the vendor, the
//! delivered bytes, and the clock all arrive through traits (D-0032).

pub mod clock;
#[cfg(feature = "databento")]
pub mod databento;
pub mod delivery;
pub mod error;
pub mod execute;
pub mod journal;
pub mod money;
pub mod plan;
pub mod provider;
pub mod quote;
pub mod supplement;
pub mod window;

pub use clock::Clock;
pub use delivery::{DeliveryError, DeliveryInspector};
pub use error::IngestError;
pub use execute::{AcquiredWindow, Deps, ExecuteOptions, ExecuteReport, execute};
pub use journal::{IntentState, Journal, JournalEntry, JournalEvent, intent_id};
pub use plan::{PlannedJob, PullPlan, PullRequest, WindowSpan, plan, range_from_dates};
pub use provider::{BatchProvider, JobState, ProviderError, QuoteQuery, StypeIn};
pub use quote::{QuoteReport, gate, quote};
