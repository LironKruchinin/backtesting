//! Derived seeds: `(config_hash, root_seed, combo_index, fold) -> u64`.
//!
//! CLAUDE.md §2.2 pins where randomness may come from: explicit seeds carried
//! in configs, and derived seeds computed from `(config_hash, combo_index,
//! fold)` — never from a clock, a thread id, or an iteration order. This
//! module is that derivation, and it exists **before** anything consumes
//! randomness on purpose: the alternative is that the first randomized
//! component invents its own seeding, in a hurry, in the place where it is
//! least visible.
//!
//! The config's own `[run].seed` joins the tuple. §2.2's triple is a floor,
//! not a ceiling, and `config_hash` deliberately does not cover `[run].seed`:
//! it is blake3 over a [`ComboSpec::canonical_form`], which is slots, rules
//! and size. Leaving the root seed out would make two configs that differ
//! only in their declared seed derive identical per-fold seeds — the exact
//! thing a seed field exists to prevent.
//!
//! The mixer is hand-rolled (FNV-1a absorb, SplitMix64 finalize) for the same
//! reason `Fnv64` in the CLI is: `DefaultHasher` is not stable across Rust
//! releases, and a seed that changes when the toolchain does is not a seed.
//! It is not a cryptographic PRF and does not need to be — it needs to be
//! deterministic, stable, and to avalanche well enough that adjacent
//! `(combo, fold)` pairs do not produce correlated streams.
//!
//! [`ComboSpec::canonical_form`]: crucible_strategies::combo::ComboSpec::canonical_form

use crucible_strategies::combo::ConfigHash;

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The seed for one run: this config, this combo, this fold.
///
/// Pure: the same tuple gives the same seed on any machine, in any thread, in
/// any order. That is the whole requirement.
#[must_use]
pub fn derive_seed(
    config: &ConfigHash,
    root_seed: u64,
    combo_index: usize,
    fold_index: usize,
) -> u64 {
    let mut h = FNV_OFFSET;
    let mut absorb = |bytes: &[u8]| {
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(FNV_PRIME);
        }
    };
    absorb(config.as_bytes());
    absorb(&root_seed.to_le_bytes());
    absorb(&(combo_index as u64).to_le_bytes());
    absorb(&(fold_index as u64).to_le_bytes());
    split_mix64(h)
}

/// SplitMix64's finalizer. Turns the accumulator's low-order structure into
/// something that avalanches, so combo 3 fold 0 and combo 3 fold 1 do not
/// start correlated streams.
const fn split_mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> ConfigHash {
        ConfigHash::from_bytes([byte; 32])
    }

    /// The only property that matters for reproducibility.
    #[test]
    fn the_same_tuple_always_derives_the_same_seed() {
        assert_eq!(
            derive_seed(&hash(0xab), 42, 7, 3),
            derive_seed(&hash(0xab), 42, 7, 3)
        );
    }

    /// Every component of the tuple has to move the answer, or it is not part
    /// of the identity — including the config's own root seed, which the
    /// config hash does not cover.
    #[test]
    fn every_component_changes_the_seed() {
        let base = derive_seed(&hash(0xab), 42, 7, 3);
        assert_ne!(base, derive_seed(&hash(0xac), 42, 7, 3), "config hash");
        assert_ne!(base, derive_seed(&hash(0xab), 43, 7, 3), "root seed");
        assert_ne!(base, derive_seed(&hash(0xab), 42, 8, 3), "combo index");
        assert_ne!(base, derive_seed(&hash(0xab), 42, 7, 4), "fold index");
    }

    /// Adjacent tuples must not produce adjacent seeds: a resampler seeded
    /// with n and n+1 draws visibly related streams, and folds are adjacent by
    /// construction.
    #[test]
    fn adjacent_folds_are_not_adjacent_seeds() {
        let a = derive_seed(&hash(0x11), 1, 0, 0);
        let b = derive_seed(&hash(0x11), 1, 0, 1);
        assert!(a.abs_diff(b) > 1_000_000, "{a} and {b} are too close");
        // And the high bits move too, not just the low ones.
        assert_ne!(a >> 40, b >> 40);
    }

    /// A pinned value. If this changes, every seed in every stored result
    /// changed with it, and that is a decision-log event rather than a
    /// refactor.
    #[test]
    fn the_derivation_is_pinned() {
        assert_eq!(derive_seed(&hash(0x00), 0, 0, 0), 0x2c06_4a35_6a35_c973);
    }
}
