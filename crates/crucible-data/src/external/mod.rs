//! Market data that did not come through `ingest` — different vendor,
//! different licence, different trust.
//!
//! `raw/` and its `manifest.jsonl` make one narrow promise: *this tool fetched
//! these bytes from Databento and hashed them at the moment of placement*
//! (D-0017). Everything under `external/` fails that promise in at least one
//! way, so it gets its own tree, its own per-vendor inventory, and is never
//! merged into the manifest (D-0049, `docs/DATA_LAYOUT.md`). Collapsing the two
//! would quietly downgrade the manifest's guarantee to the inventory's for
//! everything already in it.
//!
//! Each vendor brings its own layout rather than being reshaped into a common
//! one on the way in; reshaping someone else's export is how provenance is
//! lost. `layout-check` knows `external/` is not judged by the archive's rules.

#[cfg(feature = "thetadata")]
pub mod thetadata;
