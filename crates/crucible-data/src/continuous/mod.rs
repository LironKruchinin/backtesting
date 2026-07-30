//! Continuous futures construction — **M1, implemented.**
//!
//! Individual contracts expire; research runs on stitched "continuous"
//! series. How you stitch changes results, so the stitch is explicit,
//! versioned, and stored as a *roll table* — never baked into the archive,
//! and never into the curated bars either.
//!
//! ```text
//! curated/bars/{contract}/{tf}/*.parquet ──┐
//!                                          ├─ gather ─► ContractSeries (per contract)
//! definition (expiries, optional) ─────────┘                    │
//!                                                       build_roll_table
//!                                                               ▼
//!                             curated/rolls/{root}/{tf}/{rule}.json   (RollTable)
//!                                                               │
//!                                                    ContinuousFeed::open
//!                                                               ▼
//!                                            ContinuousBar { tradeable, adjusted }
//! ```
//!
//! Read the submodules in that order — each carries the reasoning for its own
//! leg:
//!
//! - [`symbol`] — parsing `ESH4` far enough to know what follows what, and
//!   the pinned rule for the single-digit-year ambiguity.
//! - [`session`] — bars folded into day buckets keyed by **availability**.
//! - [`roll`] — the rules, the roll instant, the table, and its storage path.
//! - [`adjust`] — the two price series, and the type barrier between them.
//! - [`feed`] — replaying the result.
//! - [`expiry`] — where expiries come from (and why the core does not care).
//! - [`gather`] — the I/O that turns the curated store into roll-rule input.
//!
//! ## The three things this module exists to get right
//!
//! **1. The roll instant carries no lookahead.** A roll decided from day D's
//! volume takes effect at `roll_ts = max(front.avail_ts(D), next.avail_ts(D))`
//! — the instant both contracts' D-buckets were complete — and the new
//! contract is front only for bars available *strictly after* that. Same
//! comparison the engine's replay loop makes when it fills orders, for the
//! same reason. See [`roll`].
//!
//! **2. Adjusted prices cannot become money.** Back-adjusted prices are for
//! *signals*; PnL must use the tradeable prices of the then-front contract.
//! Mixing them is a silent-corruption bug that leaves a smooth-looking equity
//! curve, so the two are **different types**: [`AdjustedPrice`] has no
//! conversion to [`Price`](crucible_core::types::Price), and
//! `ContractSpec::pnl_nano_usd` — the workspace's only price→money
//! conversion — takes `Price`. See [`adjust`], which carries a `compile_fail`
//! doctest proving the call does not compile.
//!
//! **3. Adjustment happens at load.** The table stores the per-roll gap; the
//! loader accumulates it. Storing adjusted bars would bake a research choice
//! into the archive and would have to be rewritten on every new roll.
//!
//! ## What is deliberately not here
//!
//! - **Open interest (`.n`) is not a variant of [`RollRule`]**, not merely
//!   unimplemented: it needs the `statistics` schema, which M1 has not
//!   curated, and a name that parses but means something else is worse than
//!   one that does not parse.
//! - **Ratio adjustment is not a variant of [`AdjustmentKind`]**, for the
//!   arithmetic reason spelled out in [`adjust`]: it cannot be made exact in
//!   integer nanopoints without a scheme that is its own design, and an `f64`
//!   in the price path is not on offer (§2.3).
//! - **The engine still sees one price series.** [`MarketEvent`] has one
//!   `Bar` variant, so [`ContinuousFeed`]'s [`Feed`] impl hands over the
//!   *tradeable* bar and the adjusted series is reachable only through
//!   [`ContinuousFeed::next_continuous_bar`]. Adding a variant is a
//!   deliberate breaking change requiring an engine review and a decision-log
//!   entry (`crucible-core::events` says so), and it belongs with the other
//!   half of the roll story: **a roll is a position event, not a price
//!   event** — rolling a position pays spread and fees twice, and the engine
//!   has to see close-old/open-new fills. Both are M2.
//!
//! [`Feed`]: crucible_core::traits::Feed
//! [`MarketEvent`]: crucible_core::events::MarketEvent

pub mod adjust;
pub mod error;
pub mod expiry;
pub mod feed;
pub mod gather;
pub mod roll;
pub mod session;
pub mod symbol;

pub use adjust::{
    AdjustedOhlc, AdjustedPrice, AdjustmentKind, ContinuousBar, back_adjust_offsets,
    unadjusted_offsets,
};
pub use error::ContinuousError;
pub use expiry::{
    CONTRACT_CYCLE_DAYS, ContractCycles, NO_EXPIRY_SOURCE, NOMINAL_EXPIRY_SOURCE, nominal_expiries,
    nominal_expiry,
};
pub use feed::{ContinuousFeed, ContinuousSegment};
pub use gather::{GatheredSeries, gather_series};
pub use roll::{
    CURATED_ROLLS_DIR, ROLL_TABLE_SCHEMA_VERSION, RollRow, RollRule, RollSource, RollTable,
    RollTableInput, build_roll_table, read_roll_table, roll_table_path, write_roll_table,
};
pub use session::{ContractSeries, SessionAccumulator, SessionBar, series_of};
pub use symbol::{
    ContractSymbol, DEFAULT_ANCHOR_YEAR, DecadeAnchor, MonthCode, SymbolParts, parse_parts,
};

#[cfg(feature = "databento")]
pub use expiry::{contract_cycles_from_definitions, expiries_from_definitions};
