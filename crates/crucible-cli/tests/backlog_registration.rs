//! The registration lint: every config block in `research/backlog/` must be a
//! config this build can actually run.
//!
//! A hypothesis file's embedded TOML is a **pre-registration**. It states, in
//! advance and in public, what will be run and what will kill it — and the
//! whole value of writing it before the run is that it cannot be adjusted
//! afterwards. That guarantee is worth nothing if the block was never runnable
//! in the first place, because then the file that gets run is a *different*
//! file, written later, by someone who has already seen the data.
//!
//! Two registrations sat in that state and nobody noticed:
//!
//! - **H-007** declared `stages = ["s1", "s2"]` and carried no `[walk_forward]`
//!   section. S2 *is* a walk-forward under costs, so the funnel refuses the
//!   config outright — the fold geometry the run needed was never registered.
//! - **H-008** declared `stages = ["s1", "s2"]` while its own Gate 0 and Gate
//!   0b are S0 measurements, and carried no `[s0]` block. Its comment said `s0`
//!   was refused at load, which had been true and stopped being true when the
//!   predictor seam landed (D-0085) — a registration describing a build that no
//!   longer existed.
//!
//! Both were invisible because nothing ever asked a machine. This file asks, on
//! every `cargo test`.
//!
//! ## What it checks, and where the requirements come from
//!
//! The stage-to-section requirements are **not listed here**. They are read out
//! of the build by running `crucible funnel --check-config`, which is the
//! funnel's own pre-flight — the same function the real run calls, refusing for
//! the same reasons in the same order (`crucible-cli/src/funnel.rs`). A lint
//! carrying its own copy of "s2 needs `[walk_forward]`, s0 needs `[s0]`" would
//! be a second source of truth, and the day a third requirement landed the copy
//! would be the one that did not learn about it.
//!
//! Three assertions per file:
//!
//! 1. A block declaring `schema_version` is a **registration**: it must pass
//!    `--check-config`.
//! 2. A registration must name the file it was extracted to, and be
//!    **byte-identical** to it. The registration and the runnable config are
//!    then the same bytes, so they cannot drift — which is the failure this
//!    file exists to make impossible, not merely to detect.
//! 3. A block *not* declaring `schema_version` is an illustrative **fragment**
//!    (H-001 and H-012 write bare rule lines with `<feature 1>` placeholders in
//!    them). A fragment must not carry `[meta]` or `[funnel]`, so a whole
//!    config cannot escape assertion 1 by dropping one line.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// The repository root, from this test binary's manifest dir rather than from
/// the process's working directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve")
}

/// RAII temp dir — same shape as the other CLI tests: pid plus a process-wide
/// counter for uniqueness without randomness or clocks.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("crucible-backlog-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return TempDir { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create test temp dir {}: {e}", path.display()),
            }
        }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One fenced ```` ```toml ```` block, with the line it started on so a failure
/// can be navigated to.
struct Block {
    /// Backlog file it came from, repo-relative.
    file: String,
    /// 1-based line of the opening fence.
    line: usize,
    /// The block's contents, fences excluded.
    body: String,
}

/// Every fenced `toml` block in a markdown source.
///
/// Deliberately literal: the opening fence must be exactly ```` ```toml ```` at
/// the start of a line, which is what every backlog file writes. A looser
/// matcher would start interpreting prose.
fn toml_blocks(file: &str, text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current: Option<(usize, Vec<&str>)> = None;
    for (i, line) in text.lines().enumerate() {
        match &mut current {
            None => {
                if line.trim_end() == "```toml" {
                    current = Some((i + 1, Vec::new()));
                }
            }
            Some((start, body)) => {
                if line.trim_end() == "```" {
                    out.push(Block {
                        file: file.to_owned(),
                        line: *start,
                        body: format!("{}\n", body.join("\n")),
                    });
                    current = None;
                } else {
                    body.push(line);
                }
            }
        }
    }
    assert!(
        current.is_none(),
        "{file}: a ```toml fence is never closed; the lint cannot tell where the config ends"
    );
    out
}

/// Does this block claim to be a whole config?
///
/// `schema_version` is the discriminator because it is the field the loader
/// itself reads first, before anything else is interpreted (`VersionProbe`),
/// and §5.5 requires it on every config. A block that declares it is asserting
/// "this is loadable"; assertion 3 stops that claim from being dodged.
fn is_registration(body: &str) -> bool {
    body.lines()
        .any(|l| l.trim_start().starts_with("schema_version"))
}

/// The `# EXTRACTED:` line's payload, if the block carries one.
fn extracted_path(body: &str) -> Option<String> {
    body.lines().find_map(|l| {
        l.trim()
            .strip_prefix("# EXTRACTED:")
            .map(|p| p.trim().to_owned())
    })
}

/// Every registration block in the backlog passes the funnel's own pre-flight.
///
/// This is the assertion that would have caught both H-007 and H-008 on the day
/// they were written. It runs the real binary, so what it enforces is whatever
/// the build enforces — including requirements added after this file was
/// written.
#[test]
fn every_backlog_registration_is_a_config_this_build_can_run() {
    let root = repo_root();
    let dir = root.join("research").join("backlog");
    let temp = TempDir::new();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("research/backlog must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let name = path
            .file_name()
            .expect("a file has a name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("readable backlog file");

        for block in toml_blocks(&name, &text) {
            if !is_registration(&block.body) {
                continue;
            }
            let tmp = temp
                .path
                .join(format!("{}-{}.toml", block.file, block.line));
            std::fs::write(&tmp, &block.body).expect("write extracted block");
            let out = Command::new(env!("CARGO_BIN_EXE_crucible"))
                .args([
                    "funnel",
                    "--config",
                    &tmp.to_string_lossy(),
                    "--check-config",
                ])
                .env_remove("DATABENTO_API_KEY")
                .env_remove("CRUCIBLE_DATA_DIR")
                .output()
                .expect("failed to run the crucible binary");
            if !out.status.success() {
                bad.push(format!(
                    "{}:{}\n{}",
                    block.file,
                    block.line,
                    String::from_utf8_lossy(&out.stderr).trim_end()
                ));
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 2,
        "found {checked} registration block(s); H-007 and H-008 alone are two, so the extractor \
         has stopped seeing blocks it used to see"
    );
    // Every offender, never the first one. A lint that stops at the first
    // failure makes a backlog with two broken registrations look like a backlog
    // with one — the same argument D-0090 makes about naming every contract in
    // a refusal rather than returning on the earliest.
    assert!(
        bad.is_empty(),
        "{} of {checked} backlog registration(s) declare a config this build refuses. A \
         pre-registration that cannot be run is not a pre-registration — whatever gets run \
         instead is a different file, written later, by someone who has already seen the \
         data.\n\n{}",
        bad.len(),
        bad.join("\n\n")
    );
}

/// A registration and the config that actually runs are the same bytes.
///
/// Byte-identity rather than "both parse": two configs can each be valid and
/// still describe different experiments, and the difference between the
/// registered grid and the run grid is precisely the thing pre-registration
/// exists to pin. Making them one artifact is what removes the failure mode;
/// this test is what keeps them one.
#[test]
fn every_backlog_registration_is_byte_identical_to_the_config_it_names() {
    let root = repo_root();
    let dir = root.join("research").join("backlog");
    let mut bad: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("research/backlog must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let name = path
            .file_name()
            .expect("a file has a name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("readable backlog file");

        for block in toml_blocks(&name, &text) {
            if !is_registration(&block.body) {
                continue;
            }
            let Some(rel) = extracted_path(&block.body) else {
                bad.push(format!(
                    "{}:{} — no `# EXTRACTED:` line. A registration must name the config file \
                     it is extracted to, or nothing stops the two from drifting apart",
                    block.file, block.line
                ));
                continue;
            };
            match std::fs::read_to_string(root.join(&rel)) {
                Err(e) => bad.push(format!(
                    "{}:{} — names `{rel}`, which cannot be read: {e}",
                    block.file, block.line
                )),
                Ok(on_disk)
                    if on_disk.replace("\r\n", "\n") != block.body.replace("\r\n", "\n") =>
                {
                    bad.push(format!(
                        "{}:{} — has drifted from `{rel}`",
                        block.file, block.line
                    ));
                }
                Ok(_) => {}
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} backlog registration(s) are not the same bytes as the config that runs. The \
         registered grid and the run grid differing is precisely what pre-registration exists \
         to prevent.\n\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// A block that is not a registration is genuinely a fragment.
///
/// Without this, assertion 1 is optional: any config could opt out of being
/// checked by deleting its `schema_version` line, and the file would still read
/// to a human as a complete registration.
#[test]
fn a_block_without_a_schema_version_carries_no_config_sections() {
    let root = repo_root();
    let dir = root.join("research").join("backlog");

    for entry in std::fs::read_dir(&dir).expect("research/backlog must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let name = path
            .file_name()
            .expect("a file has a name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("readable backlog file");

        for block in toml_blocks(&name, &text) {
            if is_registration(&block.body) {
                continue;
            }
            for section in ["[meta]", "[funnel]", "[universe]", "[run]"] {
                assert!(
                    !block.body.contains(section),
                    "{}:{} carries {section} but declares no schema_version, so the registration \
                     lint skips it. Either it is a config — give it a schema_version and let it \
                     be checked — or it is an illustration and should not carry {section}",
                    block.file,
                    block.line
                );
            }
        }
    }
}
