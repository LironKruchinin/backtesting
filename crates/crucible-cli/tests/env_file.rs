//! `.env` loading for the `crucible` binary (D-0022).
//!
//! These are *process-level* behaviours — what wins over what, and what a
//! broken file does — so they run the real binary rather than a function.
//! Every case gets a fresh working directory and an explicitly cleared
//! environment: a developer who already exports `DATABENTO_API_KEY` must not
//! be able to make this suite pass, or fail, by accident.
//!
//! The failure mode being defended against is not "the file doesn't load".
//! It is a file that loads *partially* or is silently overridden, leaving a
//! process that looks configured and is not — the environment-shaped version
//! of the config typo CLAUDE.md §5.5 refuses to tolerate.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// A fake key with a recognizable shape, so
/// [`api_key_value_is_never_printed`] can assert its absence from output.
const FAKE_KEY: &str = "db-THIS-VALUE-MUST-NEVER-BE-PRINTED";

/// RAII temp dir, mirroring `crucible-data`'s `testutil::TempDir`: pid plus a
/// process-wide counter for uniqueness without randomness or clocks, and
/// `create_dir` (not `_all`) so a leftover directory is skipped rather than
/// silently reused. Std-only — `tempfile` is not in the blessed set (§6).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-cli-env-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `.env` in this directory with exactly `content` (LF framing, no
    /// BOM — the bytes a text editor would produce).
    fn write_env_file(&self, content: &str) {
        std::fs::write(self.path.join(".env"), content).expect("write .env");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Runs `crucible env` with `dir` as the working directory. Both variables
/// are removed from the inherited environment first; `overrides` puts back
/// exactly what a case wants to test.
fn run_env(dir: &Path, overrides: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_crucible"));
    cmd.arg("env")
        .current_dir(dir)
        .env_remove("DATABENTO_API_KEY")
        .env_remove("CRUCIBLE_DATA_DIR");
    for (key, value) in overrides {
        cmd.env(key, value);
    }
    cmd.output().expect("run the crucible binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn missing_env_file_is_not_an_error() {
    // The environment alone is a complete configuration (it is how CI runs).
    // No file must never be treated as a problem.
    let dir = TempDir::new();
    let out = run_env(dir.path(), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("none found"), "{}", stdout(&out));
}

#[test]
fn env_file_supplies_variables() {
    let dir = TempDir::new();
    dir.write_env_file(&format!(
        "# a comment, and a blank line follow\n\nDATABENTO_API_KEY={FAKE_KEY}\nCRUCIBLE_DATA_DIR=E:/crucible-data\n"
    ));
    let out = run_env(dir.path(), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("DATABENTO_API_KEY  set"), "{text}");
    assert!(text.contains("E:/crucible-data"), "{text}");
}

#[test]
fn real_environment_beats_the_env_file() {
    // Precedence is the whole safety story: an exported variable (a CI
    // secret, a one-off `$env:`) must not be silently replaced by a stale
    // checked-out file.
    let dir = TempDir::new();
    dir.write_env_file("CRUCIBLE_DATA_DIR=E:/from-the-file\n");
    let out = run_env(
        dir.path(),
        &[("CRUCIBLE_DATA_DIR", "E:/from-the-environment")],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("E:/from-the-environment"), "{text}");
    assert!(!text.contains("E:/from-the-file"), "{text}");
}

#[test]
fn malformed_env_file_is_a_loud_hard_error() {
    // Negative control (§7): the file is applied line by line, so a bad line
    // leaves earlier ones already set. Exiting nonzero is what stops a
    // half-configured process from going on to spend money.
    let dir = TempDir::new();
    dir.write_env_file(&format!(
        "DATABENTO_API_KEY={FAKE_KEY}\nthis line is not KEY=value\n"
    ));
    let out = run_env(dir.path(), &[]);
    assert_eq!(out.status.code(), Some(2), "stdout: {}", stdout(&out));
    assert!(stderr(&out).contains(".env"), "{}", stderr(&out));
}

#[test]
fn windows_style_paths_need_quoting_or_forward_slashes() {
    // Pins the dotenv parsing contract the README documents: a backslash
    // starts an escape sequence, so an unquoted Windows path is a hard error
    // and a single-quoted one is taken literally. A dependency upgrade that
    // changes either half must fail here, not in a user's terminal.
    let unquoted = TempDir::new();
    unquoted.write_env_file("CRUCIBLE_DATA_DIR=E:\\crucible-data\n");
    let out = run_env(unquoted.path(), &[]);
    assert_eq!(out.status.code(), Some(2), "stdout: {}", stdout(&out));

    let quoted = TempDir::new();
    quoted.write_env_file("CRUCIBLE_DATA_DIR='E:\\crucible-data'\n");
    let out = run_env(quoted.path(), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("E:\\crucible-data"),
        "single quotes must preserve the path verbatim: {}",
        stdout(&out)
    );
}

#[test]
fn api_key_value_is_never_printed() {
    // CLAUDE.md §8: never log the key. `env` reports presence and length so
    // a truncated or quote-wrapped paste is diagnosable; the key itself must
    // appear on neither stream.
    let dir = TempDir::new();
    dir.write_env_file(&format!("DATABENTO_API_KEY={FAKE_KEY}\n"));
    let out = run_env(dir.path(), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(!stdout(&out).contains(FAKE_KEY), "key leaked to stdout");
    assert!(!stderr(&out).contains(FAKE_KEY), "key leaked to stderr");
    // The report is still useful: 35 characters, counted by hand from
    // FAKE_KEY ("db-" + "THIS-VALUE-MUST-NEVER-BE-PRINTED" = 3 + 32).
    assert!(stdout(&out).contains("set, 35 chars"), "{}", stdout(&out));
}
