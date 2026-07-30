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
//! ## Two more dimensions, added by M3 without moving the pinned value
//!
//! D-0067 puts `account_id` in the run identity and in the seed lineage: an
//! account is chosen, and choosing after seeing results is a maximum over
//! sixteen draws reported as an expectation. And the funnel's controls draw
//! random numbers — the matched random-entry benchmark is the first thing in
//! this project that actually consumes a seed — so they need a stream that
//! cannot collide with a fold's.
//!
//! Both arrive through [`derive_run_seed`] and [`derive_control_seed`] rather
//! than by changing [`derive_seed`], which stays **bit-identical** to what
//! D-0064 pinned. `derive_run_seed(.., None, combo, fold)` absorbs exactly the
//! bytes `derive_seed(.., combo, fold)` does and is asserted equal to it; an
//! account absorbs its name, and a control absorbs a marker byte the fold path
//! never writes. Changing the pinned value would change every seed in every
//! stored result, which is a decision-log event rather than a refactor — so
//! the way to add a dimension is to add bytes on the paths that need it, not
//! to renumber the one that does not.
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
    derive_run_seed(config, root_seed, None, combo_index, fold_index)
}

/// The seed for one run when an **account** is part of its identity (D-0067).
///
/// `account_id: None` is the no-account case and reproduces [`derive_seed`]
/// byte for byte — asserted by
/// `a_run_without_an_account_derives_the_pinned_fold_seed`.
#[must_use]
pub fn derive_run_seed(
    config: &ConfigHash,
    root_seed: u64,
    account_id: Option<&str>,
    combo_index: usize,
    fold_index: usize,
) -> u64 {
    let mut h = Absorb::new();
    h.identity(config, root_seed, account_id, combo_index);
    h.bytes(&(fold_index as u64).to_le_bytes());
    h.finish()
}

/// The seed for a named **control** stream: the matched random-entry
/// benchmark, and anything else that resamples per combo rather than per fold.
///
/// A control is not a fold and must not draw a fold's numbers, so the name is
/// absorbed after a marker byte the fold path never writes. Naming the stream
/// rather than numbering it is deliberate: a reader of a stored seed can grep
/// for what drew it.
#[must_use]
pub fn derive_control_seed(
    config: &ConfigHash,
    root_seed: u64,
    account_id: Option<&str>,
    combo_index: usize,
    control: &str,
) -> u64 {
    let mut h = Absorb::new();
    h.identity(config, root_seed, account_id, combo_index);
    h.bytes(&[CONTROL_MARKER]);
    h.bytes(control.as_bytes());
    h.finish()
}

/// Written only on control streams, so a control seed can never be a fold
/// seed: the fold path absorbs eight length-fixed bytes where this writes one
/// byte that is not part of any `u64`'s little-endian encoding of a plausible
/// fold index.
const CONTROL_MARKER: u8 = 0xff;

/// The FNV-1a accumulator, factored out so every stream absorbs the identity
/// prefix in exactly one order. Two derivations that agree on the prefix by
/// copy-paste is how they stop agreeing.
struct Absorb {
    h: u64,
}

impl Absorb {
    const fn new() -> Absorb {
        Absorb { h: FNV_OFFSET }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.h ^= u64::from(b);
            self.h = self.h.wrapping_mul(FNV_PRIME);
        }
    }

    /// Config, root seed, account (absorbing nothing at all when there is
    /// none — that is what keeps D-0064's value pinned), combo index.
    fn identity(
        &mut self,
        config: &ConfigHash,
        root_seed: u64,
        account_id: Option<&str>,
        combo_index: usize,
    ) {
        self.bytes(config.as_bytes());
        self.bytes(&root_seed.to_le_bytes());
        if let Some(account) = account_id {
            self.bytes(account.as_bytes());
        }
        self.bytes(&(combo_index as u64).to_le_bytes());
    }

    const fn finish(&self) -> u64 {
        split_mix64(self.h)
    }
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

    /// The whole point of adding the account dimension the way it was added:
    /// a run with no account absorbs exactly the bytes D-0064 pinned, so no
    /// stored seed moved when the dimension arrived.
    #[test]
    fn a_run_without_an_account_derives_the_pinned_fold_seed() {
        assert_eq!(
            derive_run_seed(&hash(0x00), 0, None, 0, 0),
            0x2c06_4a35_6a35_c973
        );
        assert_eq!(
            derive_run_seed(&hash(0xab), 42, None, 7, 3),
            derive_seed(&hash(0xab), 42, 7, 3)
        );
    }

    /// D-0067: the account is part of the run identity, so it must be part of
    /// the seed. Two accounts sharing a stream would make "we tried sixteen
    /// accounts" and "we tried one" indistinguishable downstream.
    #[test]
    fn the_account_changes_the_seed_and_two_accounts_differ() {
        let none = derive_run_seed(&hash(0xab), 42, None, 7, 3);
        let apex = derive_run_seed(&hash(0xab), 42, Some("apex_50k"), 7, 3);
        let topstep = derive_run_seed(&hash(0xab), 42, Some("topstep_50k"), 7, 3);
        assert_ne!(none, apex);
        assert_ne!(apex, topstep);
    }

    /// A control stream must not be able to land on a fold's numbers, and two
    /// named controls must not share one.
    #[test]
    fn a_control_stream_is_disjoint_from_the_fold_streams() {
        let control = derive_control_seed(&hash(0xab), 42, None, 7, "random_entry");
        for fold in 0..64 {
            assert_ne!(
                control,
                derive_seed(&hash(0xab), 42, 7, fold),
                "fold {fold}"
            );
        }
        assert_ne!(
            control,
            derive_control_seed(&hash(0xab), 42, None, 7, "buy_and_hold")
        );
    }
}
