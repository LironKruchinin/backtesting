# testdata/ — golden fixtures policy

Golden fixtures are small, versioned inputs with hand-derived expected
outputs. They pin engine semantics: if a change shifts a golden number, the
change altered execution behavior, and that must be a decision, not a side
effect.

Rules (enforced socially + in review; see CLAUDE.md §7):

1. Fixtures are tiny (KBs) and synthetic or hand-written — never vendor data
   (licensing + size). Real-data cross-checks run locally against the
   archive and are not committed.
2. Every expected value in a golden test must be derivable by hand; the
   derivation lives in a comment next to the expectation (see
   `crucible-engine/tests/golden_smoke.rs` for the format).
3. Changing a golden expectation requires: the hand arithmetic re-done in
   the commit message, and a docs/DECISIONS.md entry if semantics changed.
   "Updated goldens to match new output" with no arithmetic is a rejected
   change by definition.
4. Claude Code sessions: you may ADD fixtures freely; you may not MODIFY
   existing expected values without an explicit human instruction in the
   session.
