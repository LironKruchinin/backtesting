//! `crucible layout-check` — is the archive tree the shape it claims to be?
//!
//! `verify` answers "are these the bytes we recorded" by re-hashing every
//! file, which on a 99 GiB archive takes minutes. This answers "is everything
//! where the layout says it goes", which takes milliseconds because it only
//! reads directory entries. Different questions, different costs, so different
//! commands — but neither implies the other, and the blitz runs both after
//! every tranche.
//!
//! `docs/DATA_LAYOUT.md` is the specification this enforces.
//!
//! **This command never moves anything.** Manifest `file_path`s are
//! load-bearing (D-0013/D-0014), so a violation under `raw/` is reported for a
//! human to correct by *acquiring a new file at a correct path*, never by
//! renaming the old one. There is deliberately no `--fix`.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the tree matches the layout |
//! | 2 | nothing could be checked — `CRUCIBLE_DATA_DIR` unset, the archive root missing, or `manifest.jsonl` unreadable |
//! | 4 | layout violations were found |
//!
//! 2 and 4 are deliberately different: "I could not look" and "I looked and it
//! is wrong" are not the same answer, and a blitz script that conflates them
//! would treat a misconfigured environment as a clean archive.

use crucible_data::Catalog;
use crucible_data::layout;

use crate::pull::{EXIT_FAILED, EXIT_USAGE, data_dir};

/// Runs the command, returning the process exit code.
pub fn run() -> i32 {
    let dir = match data_dir() {
        Ok(dir) => dir,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };

    // An unreadable manifest is a usage-level problem, not a layout finding:
    // without it there is nothing to check the recorded paths against, and
    // reporting "0 records, all fine" would be a lie of omission.
    let catalog = match Catalog::open(&dir) {
        Ok(catalog) => catalog,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    let report = match layout::check(&dir, catalog.records()) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_USAGE;
        }
    };

    print!("{report}");
    if report.is_clean() {
        0
    } else {
        println!(
            "\n  The layout is a contract, not a habit: no directory holds two\n\
             \x20 instruments' data, and none holds two kinds. Nothing here is fixed by\n\
             \x20 moving a file under raw/ — manifest paths name the bytes a result read,\n\
             \x20 so a correction is a new acquisition at a correct path (D-0017).\n\
             \x20 Curated data is disposable: delete it and re-run transcode."
        );
        EXIT_FAILED
    }
}
