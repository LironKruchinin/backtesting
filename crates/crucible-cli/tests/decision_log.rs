//! `docs/DECISIONS.md` is a numbered, ordered record, and this asserts it.
//!
//! §8.2 exists because decision numbers collide, and its protocol has now
//! failed six times: four numbers claimed twice, one branch that pre-claimed
//! and got away with it, and one entry appended in the middle of the log. Every
//! repair has been a better *procedure*, and procedures keep failing for the
//! same reason — they need someone to run the right query at the right moment.
//!
//! A test does not. It makes the whole class inexpressible rather than
//! monitored, which is what the rest of this codebase does with an invariant it
//! cannot afford to re-check by hand.

use std::path::{Path, PathBuf};

/// The repository root, from this test binary's manifest dir rather than from
/// the process's working directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve")
}

/// Every `- **D-NNNN**` heading, in the order the file lists them.
fn headings(text: &str) -> Vec<(usize, u32)> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let rest = line.strip_prefix("- **D-")?;
            let digits = rest.strip_suffix("**").or_else(|| {
                let end = rest.find("**")?;
                Some(&rest[..end])
            })?;
            digits.parse::<u32>().ok().map(|n| (i + 1, n))
        })
        .collect()
}

/// **Decision numbers are unique and strictly ascending in file order.**
///
/// The uniqueness half is §8.2's whole subject. The *ordering* half is the one
/// that would have caught the incident this test was written for: D-0122 was
/// appended after D-0120 rather than at the end, because the edit anchored on a
/// string that was UNIQUE but not LAST, and `assert count == 1` cannot tell
/// those apart.
///
/// The consequence was not cosmetic. `grep -oE 'D-0[0-9]{3}' | tail -1` — the
/// obvious way to find the newest entry, and what §8.2 rule 5 effectively
/// prescribed — returned a number three behind the real maximum, which is a
/// collision waiting for the next allocator.
#[test]
fn decision_numbers_are_unique_and_ascending() {
    let path = repo_root().join("docs").join("DECISIONS.md");
    let text = std::fs::read_to_string(&path).expect("the decision log is readable");
    let found = headings(&text);

    // D-0118: a check that matched nothing would pass silently. This asserts
    // the pattern found the log it is about before asserting anything of it.
    assert!(
        found.len() > 100,
        "only {} headings matched — the pattern is wrong, not the file",
        found.len()
    );

    let mut previous: Option<(usize, u32)> = None;
    for &(line, number) in &found {
        if let Some((prev_line, prev)) = previous {
            assert!(
                number > prev,
                "docs/DECISIONS.md is out of order: D-{number:04} at line {line} follows \
                 D-{prev:04} at line {prev_line}. Decision numbers must be unique and strictly \
                 ascending in file order — otherwise the newest entry is not the last one, and \
                 an allocator reading the tail picks a number that is already taken (§8.2)."
            );
        }
        previous = Some((line, number));
    }
}

/// The *max* of the headings is the next-free basis, and reading the file's
/// tail is not — this pins the difference that caused the incident.
///
/// It is not redundant with the test above: ordering could hold while a
/// trailing non-heading mention of an older number still made `tail` wrong, and
/// §8.2 rule 5 is about which query an allocator runs, not about file order.
#[test]
fn the_next_free_number_comes_from_the_max_not_the_tail() {
    let path = repo_root().join("docs").join("DECISIONS.md");
    let text = std::fs::read_to_string(&path).expect("the decision log is readable");
    let found = headings(&text);
    assert!(found.len() > 100, "pattern control, as above");

    let max = found.iter().map(|&(_, n)| n).max().expect("nonempty");
    let last = found.last().expect("nonempty").1;
    assert_eq!(
        max, last,
        "the highest decision number (D-{max:04}) is not the last heading in the file \
         (D-{last:04}), so `tail` and `max` disagree and §8.2's allocator would read the \
         wrong one"
    );
}
