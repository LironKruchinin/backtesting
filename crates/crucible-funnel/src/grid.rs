//! Parameter grids and config identity — **M3 spec, not yet implemented.**
//!
//! ## Expansion
//! Config files declare ranges (`period = { start = 10, end = 40, step = 5 }`)
//! or explicit lists (`k = [1.5, 2.0, 2.5]`). Expansion is the cartesian
//! product across all declared axes. Log the combo count *before* running;
//! warn loudly above ~10k combos and suggest coarse-to-fine instead — a
//! silent 400k-combo run is a footgun, not a feature.
//!
//! ## Config identity (reproducibility, CLAUDE.md §2.5)
//! `config_hash = blake3(canonical_form)` where canonical form is the parsed
//! config re-serialized with sorted keys and normalized numbers — NOT the
//! raw file bytes (whitespace/comments must not change identity), and NOT
//! `std::collections::hash_map::DefaultHasher` (not stable across Rust
//! releases; useless for a persistent registry).
//!
//! One expanded combo = one `RunSpec` = (config_hash, combo_index, fold,
//! seed). The registry dedupes on that tuple: re-running a config resumes
//! instead of recomputing finished work.

/// Placeholder for the M3 grid implementation.
#[derive(Debug, Clone, Copy)]
pub struct GridPlan;
