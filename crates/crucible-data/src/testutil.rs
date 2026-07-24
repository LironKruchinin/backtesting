//! Test-only filesystem helpers. Compiled under `#[cfg(test)]` only —
//! never part of the shipped library. Std-only by design: `tempfile` is
//! not in the blessed dependency set (CLAUDE.md §6).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII temp dir under `std::env::temp_dir()`, named
/// `crucible-test-{pid}-{counter}`. Removed (best effort) on `Drop`; the
/// OS temp cleaner is the backstop for panicked tests.
///
/// pid + process-wide atomic counter gives uniqueness across parallel test
/// threads and processes without randomness or clocks; `create_dir` (not
/// `_all`) guarantees a *fresh* dir — a leftover from a crashed earlier run
/// with a reused pid is skipped, never silently reused.
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-test-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
