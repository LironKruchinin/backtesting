//! Grid guardrails and run identity.
//!
//! ## Expansion moved out of this crate (D-0060)
//!
//! Cartesian expansion itself lives in `crucible-strategies::combo::grid`,
//! because §2.6 needs `max_warmup` across the grid at *build* time — the
//! funnel cannot align eval windows for combos it cannot yet construct — and
//! because expansion is a pure function over plain data with nothing to
//! parallelize. `crucible-cli::config` parses a TOML config into a `ComboSpec`
//! and expands it; this module inherits that seam rather than reimplementing
//! it.
//!
//! ## What is this crate's job, and what of it is done
//!
//! - **Combo-count guardrails.** [`check_size`] — implemented here, and it
//!   **refuses**. `crucible combo` warns above [`LOUD_COMBO_COUNT`] because
//!   expansion is free and a warning costs a reader nothing; the funnel
//!   refuses above [`REFUSE_COMBO_COUNT`] because by then a run costs hours
//!   and charges a trial per combo. See [`check_size`] for why the two numbers
//!   differ.
//! - **Config identity.** `config_hash = blake3(spec.canonical_form())` —
//!   computed by the caller (`crucible-cli::config`), never over raw file
//!   bytes and never with `DefaultHasher`. Done, D-0012/D-0060.
//! - **Run identity and dedupe.** `(config_hash, account_id, combo_index,
//!   fold, seed)`. Done, in [`crate::registry`].
//! - **Derived seeds.** Done, in [`crate::walkforward::seed`].

/// Combos above which a grid is more likely a mistyped `step` than a plan.
/// `crucible combo` warns here.
pub const LOUD_COMBO_COUNT: usize = 10_000;

/// Combos above which `crucible funnel` **refuses**.
pub const REFUSE_COMBO_COUNT: usize = 50_000;

/// The warn level is below the refuse level, checked at compile time: two
/// thresholds that crossed would make `combo` warn about grids `funnel` had
/// already refused, which is a confusing way to say nothing.
const _: () = assert!(LOUD_COMBO_COUNT < REFUSE_COMBO_COUNT);

/// A grid the funnel will not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TooManyCombos {
    /// Combos the config expands to.
    pub combos: usize,
    /// The family every one of them would be charged to.
    pub hypothesis_family: String,
}

impl std::fmt::Display for TooManyCombos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "this config expands to {} combos, past the {REFUSE_COMBO_COUNT} where `funnel` \
             refuses.\n       Every one of them is a trial charged to {}, and the trial count is \
             the denominator of\n       every deflated Sharpe that family will ever report — so \
             a grid this size does not\n       merely take hours, it permanently devalues the \
             result at the end of them.\n       Coarse-to-fine finds the same plateau for a \
             thousandth of the trial count.",
            self.combos, self.hypothesis_family
        )
    }
}

impl std::error::Error for TooManyCombos {}

/// Refuses a grid that is too large to run honestly.
///
/// # Why refuse where `combo` only warns
///
/// The two commands are asked different questions. `crucible combo` expands a
/// config and replays it: expansion spends nothing, a replay is minutes, and a
/// warning tells the researcher what they typed. `crucible funnel` runs six
/// replays per combo — the declared costs, the free-fill screen, four sweep
/// levels — plus sixteen control draws, and **charges a trial per combo to the
/// hypothesis family, permanently**. A 400,000-combo funnel run does not
/// merely take a long time; it writes 400,000 trials into the registry, and
/// every deflated Sharpe that family ever reports divides by that number. The
/// damage outlives the run, which is the difference between a footgun and a
/// warning.
///
/// # Errors
/// [`TooManyCombos`] naming the count, the family, and the alternative.
pub fn check_size(combos: usize, hypothesis_family: &str) -> Result<(), TooManyCombos> {
    if combos > REFUSE_COMBO_COUNT {
        return Err(TooManyCombos {
            combos,
            hypothesis_family: hypothesis_family.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boundary is a refusal, not a warning, and the message says what it
    /// costs rather than only that it is large.
    #[test]
    fn a_grid_past_the_limit_is_refused_and_the_message_names_the_family() {
        assert!(check_size(REFUSE_COMBO_COUNT, "es-meanrev").is_ok());
        let err = check_size(REFUSE_COMBO_COUNT + 1, "es-meanrev").expect_err("must refuse");
        assert_eq!(err.combos, REFUSE_COMBO_COUNT + 1);
        let text = err.to_string();
        assert!(text.contains("es-meanrev"), "{text}");
        assert!(text.contains("deflated Sharpe"), "{text}");
        assert!(text.contains("Coarse-to-fine"), "{text}");
    }

    /// A grid between the two thresholds runs, loudly — the warn level is not
    /// a second refusal.
    #[test]
    fn a_grid_between_the_two_thresholds_still_runs() {
        assert!(check_size(LOUD_COMBO_COUNT + 1, "f").is_ok());
    }
}
