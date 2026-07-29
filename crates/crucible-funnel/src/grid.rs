//! Grid scheduling and config identity — **M3 spec, not yet implemented.**
//!
//! ## Expansion moved out of this crate (D-0060)
//! Cartesian expansion itself now lives in `crucible-strategies::combo::grid`,
//! because §2.6 needs `max_warmup` across the grid at *build* time — the
//! funnel cannot align eval windows for combos it cannot yet construct — and
//! because expansion is a pure function over plain data with nothing to
//! parallelize. `crucible-cli::config` already parses a TOML config into a
//! `ComboSpec` and expands it; this module inherits that seam in M3 rather
//! than reimplementing it.
//!
//! What is still this crate's job:
//!
//! - **Combo-count guardrails.** Log the count *before* running; warn loudly
//!   above ~10k combos and suggest coarse-to-fine instead — a silent
//!   400k-combo run is a footgun, not a feature. (`crucible combo` warns
//!   today; the funnel must refuse rather than warn once a run costs hours.)
//! - **Config identity.** `config_hash = blake3(spec.canonical_form())` —
//!   NOT the raw file bytes (whitespace/comments must not change identity),
//!   and NOT `std::collections::hash_map::DefaultHasher` (not stable across
//!   Rust releases; useless for a persistent registry). `crucible-strategies`
//!   renders the canonical form and never hashes it, which is what keeps
//!   blake3 out of a crate that must stay dependency-free (CLAUDE.md §3).
//! - **Run identity and dedupe.** One expanded combo = one `RunSpec` =
//!   (config_hash, combo_index, fold, seed), built on
//!   `crucible_core::identity::ComboId`. The registry dedupes on that tuple:
//!   re-running a config resumes instead of recomputing finished work.
//! - **Derived seeds.** From (config_hash, combo_index, fold), never from a
//!   clock or a thread id (§2.2).

/// Placeholder for the M3 grid implementation.
#[derive(Debug, Clone, Copy)]
pub struct GridPlan;
