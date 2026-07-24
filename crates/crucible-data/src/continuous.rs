//! Continuous futures construction — **M1 spec, not yet implemented.**
//!
//! Individual contracts expire; research runs on stitched "continuous"
//! series. How you stitch changes results, so the stitch is explicit,
//! versioned, and stored as a *roll table*, never baked into the archive.
//!
//! ## Plan
//! - Roll rule variants: volume-crossover (default; matches Databento `.v`),
//!   calendar-days-before-expiry (`.c`), open-interest (`.n`).
//! - The roll table (symbol, from_contract, to_contract, roll_ts, rule,
//!   adjustment) is computed from archived data + the `definition` schema
//!   and written to `curated/`; backtests reference its version.
//! - Price adjustment at load time: `none` | `back_adjust` (additive) |
//!   `ratio`. Back-adjusted prices are for *signals*; PnL accounting must use
//!   tradeable (unadjusted) prices of the then-front contract — mixing these
//!   up is a classic silent-corruption bug. Keep the two series distinct in
//!   types/names.
//! - A roll is a position event, not a price event: rolling a position pays
//!   spread + fees twice. The engine sees a roll as close-old/open-new fills
//!   (M2 wiring).

/// Placeholder for the M1 continuous-contract implementation.
#[derive(Debug, Clone, Copy)]
pub struct ContinuousPlan;
