//! Trial registry and results store — research memory.
//!
//! Two jobs, and the second is the one that matters: make every run
//! resumable and dedupable, and make the **trial count** — the denominator of
//! research honesty — automatic. `docs/PROJECT_PLAN.md` §7.7 is categorical
//! about it: *every run increments its hypothesis family in the registry;
//! deflated Sharpe reads the count from there, never from a human's memory of
//! "a few dozen tries".*
//!
//! # The contract (this is the part that is not allowed to change)
//!
//! 1. **Insert before running.** A run's row is written with status
//!    [`RunStatus::Running`] *before* the replay starts, so a crash leaves a
//!    visible corpse rather than a silent gap. A store that only records
//!    successes cannot tell "we never tried that" from "it blew up".
//! 2. **Dedupe on the run key** — `(config_hash, account_id, combo_index,
//!    fold, seed)`. Finished work is never recomputed; a grid resumes for
//!    free.
//! 3. **Trial counting is automatic** and per hypothesis family. Nothing asks
//!    a human how many things they tried.
//! 4. **Pre-registered criteria are stored on the row**, verbatim. A criterion
//!    that only exists in the config file at the moment of judging is not
//!    pre-registered, it is remembered.
//! 5. **The graveyard is a query, not a document**: [`Registry::verdicts`]
//!    replays every kill with its stage, its reasons and its date.
//!
//! ## What a trial is, precisely
//!
//! A **trial** is a distinct `(config_hash, account_id, combo_index)` — one
//! parameterization of one idea against one account. Its *folds* are runs
//! within that trial, not trials of their own: walking one combo forward
//! across eight folds is one thing tried once, and charging it eight trials
//! would deflate a Sharpe by an artifact of the fold layout. The account is in
//! the key because D-0067 puts it there: choosing the account size after
//! seeing results is a maximum over sixteen draws reported as an expectation,
//! and the only defence is to price it.
//!
//! # Why JSONL and not DuckDB (D-0074)
//!
//! CLAUDE.md §6 blesses `duckdb` for this crate, and it was tried first. On
//! this machine `duckdb 1.10505.0` with the `bundled` feature **fails to
//! build**: the vendored amalgamation's `catalog_entry/list.hpp` includes
//! `duckdb/catalog/catalog_entry/aggregate_function_catalog_entry.hpp`, which
//! is not in the shipped tree, and `cl.exe` exits 2 with `fatal error C1083`
//! after ~2 GB of object files. That is measured, not assumed.
//!
//! So the backend is an append-only JSONL log and the **contract above is
//! unchanged** — which is the point: the five rules are about ordering and
//! identity, not about SQL. It is also the shape this archive already uses and
//! already trusts (`manifest.jsonl`, D-0014/D-0017, and its second line kind
//! from D-0068): append-only, greppable, diffable, and readable by anything.
//! A finished run appends a second line rather than mutating the first, for
//! the same reason the manifest does.
//!
//! # No clocks in here (D-0015's device, again)
//!
//! `started_at` and `finished_at` are **caller-supplied strings**. This crate
//! reads no clock — not because §2.2 forbids it here (a registry timestamp is
//! metadata, not a result), but because a library that can read a clock will
//! eventually read one into a result. The binary that owns the process owns
//! the wall clock, exactly as `Catalog::append` takes an `acquired_ts`.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::stages::{Criteria, Verdict};

/// Where a run's numbers came from and what they are charged to.
///
/// Serialized as text on purpose: a registry line has to stay readable when
/// the code that wrote it is five years gone.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RunKey {
    /// blake3 of the config's canonical form (D-0012), lowercase hex.
    pub config_hash: String,
    /// The evaluated account (`configs/accounts/*.toml` stem), or `None` when
    /// the run is not being scored against one. Part of the identity because
    /// account selection is a research choice and D-0067 prices it.
    pub account_id: Option<String>,
    /// Index into the expanded grid.
    pub combo_index: usize,
    /// Fold index, or `None` for the pooled row that covers the whole replay.
    pub fold: Option<usize>,
    /// The derived seed for this run
    /// ([`crate::walkforward::derive_run_seed`]).
    ///
    /// In the key, not merely on the row: `config_hash` is blake3 over a
    /// `ComboSpec::canonical_form`, which deliberately does not cover
    /// `[run].seed` (D-0064). Without the seed here, two configs differing
    /// only in their declared seed would dedupe into each other and the second
    /// would never run.
    pub seed: u64,
}

impl RunKey {
    /// The stable identity string a [`RunKey`] hashes to.
    fn canonical(&self) -> String {
        format!(
            "{}|{}|{}|{}|{:016x}",
            self.config_hash,
            self.account_id.as_deref().unwrap_or("-"),
            self.combo_index,
            self.fold
                .map_or_else(|| "pooled".to_owned(), |f| f.to_string()),
            self.seed
        )
    }

    /// `blake3(canonical)`, truncated — a run id that is a *function* of the
    /// run rather than a counter, so two processes that insert the same run
    /// agree on its name without talking to each other.
    #[must_use]
    pub fn run_id(&self) -> String {
        blake3::hash(self.canonical().as_bytes()).to_hex()[..16].to_owned()
    }

    /// The trial this run is charged to: identity minus the fold and the seed.
    fn trial(&self) -> (String, Option<String>, usize) {
        (
            self.config_hash.clone(),
            self.account_id.clone(),
            self.combo_index,
        )
    }
}

/// How a run ended, or that it has not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Inserted, not yet finished. A row left in this state is a crash.
    Running,
    /// Replayed and scored.
    Done,
    /// Started and failed. Recorded rather than deleted — a failure that
    /// leaves no trace is indistinguishable from a run nobody attempted.
    Failed,
}

/// The numbers a finished run is remembered by.
///
/// Deliberately small: the registry is an index over results, not a copy of
/// them. Equity curves stay in the scorecard artifacts, which name this
/// `run_id`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    /// Final equity of the window this run covers.
    pub final_equity_nano_usd: i64,
    /// Total return over that window.
    pub return_pct: f64,
    /// Maximum drawdown over that window.
    pub max_dd_pct: f64,
    /// Naive per-bar Sharpe; `None` when there is no variance to divide by.
    pub sharpe_naive: Option<f64>,
    /// Round-trips closed inside the window (attributed by close, D-0063).
    pub round_trips: usize,
    /// Fraction of them that were profitable.
    pub win_rate: Option<f64>,
    /// Commission paid inside the window.
    pub fees_nano_usd: i64,
}

/// A run, as inserted before it runs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRow {
    /// Identity and dedupe key.
    pub key: RunKey,
    /// The family every trial is charged to (`meta.hypothesis_family`).
    pub hypothesis_family: String,
    /// Human-readable parameters, e.g. `fast(period=10) slow(period=50)`.
    pub params: String,
    /// The named execution assumption (§2.4). Never absent, never implied.
    pub fill_model: String,
    /// Repository revision, supplied by the caller (§2.5).
    pub git_sha: String,
    /// Manifest ids of every archived file the run read (§2.5). Empty for a
    /// synthetic feed, which is generated rather than archived.
    pub data_manifest_ids: Vec<String>,
    /// Wall clock, supplied by the caller. This crate reads no clock.
    pub started_at: String,
    /// The **pre-registered** criteria, stored verbatim on the row so a
    /// verdict can be audited against what was declared before the run rather
    /// than against whatever the config says today.
    pub criteria: Criteria,
}

/// A verdict, as decided after the runs it reads.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerdictRow {
    /// Config the verdict is about.
    pub config_hash: String,
    /// Account it was scored against, if any.
    pub account_id: Option<String>,
    /// Combo the verdict is about.
    pub combo_index: usize,
    /// Family it is charged to.
    pub hypothesis_family: String,
    /// The stage that decided it (`s0`/`s1`/`s2`/`s3`).
    pub decided_at: String,
    /// Kill / Iterate / Graduate.
    pub verdict: Verdict,
    /// Every criterion evaluated, in the order it was evaluated.
    pub reasons: Vec<String>,
    /// How many trials the family had been charged when this was decided.
    /// Stored because a deflated Sharpe computed later must use the count as
    /// of the decision, not as of the reading.
    pub trials_at_decision: usize,
    /// Wall clock, supplied by the caller.
    pub decided_on: String,
}

/// One line of the store. `kind` is the discriminator, and a reader that meets
/// a kind it does not know **refuses** rather than skipping — an unknown line
/// means the file was written by a newer build, and quietly ignoring it would
/// under-count trials, which is the one number here that must never be too
/// small (CLAUDE.md §8, reader-first).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Line {
    /// A run, inserted before it ran.
    Run(Box<RunRow>),
    /// The same run, finished.
    RunFinished {
        run_id: String,
        status: RunStatus,
        metrics: Option<RunMetrics>,
        finished_at: String,
    },
    /// A verdict over a combo.
    Verdict(Box<VerdictRow>),
}

/// What [`Registry::insert_running`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inserted {
    /// New work: the row was appended and the trial charged.
    New,
    /// This exact run is already finished. Skip it — that is the resume path.
    AlreadyDone,
    /// This exact run was claimed before and never finished: a corpse. It is
    /// re-run, and it is **not** charged a second trial.
    Retrying,
}

/// Anything that stops the registry from being read or written.
#[derive(Debug)]
pub enum RegistryError {
    /// The store could not be opened, read, or appended to.
    Io {
        /// Which file.
        path: PathBuf,
        /// Why not.
        source: std::io::Error,
    },
    /// A line does not parse. Never skipped: see [`Line`].
    Corrupt {
        /// Which file.
        path: PathBuf,
        /// 1-based line number.
        line: usize,
        /// The parser's message.
        message: String,
    },
    /// A finish arrived for a run that was never inserted, which means
    /// something wrote a finish without the insert-before-run step.
    UnknownRun {
        /// The id that was finished.
        run_id: String,
    },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Io { path, source } => {
                write!(f, "registry {}: {source}", path.display())
            }
            RegistryError::Corrupt {
                path,
                line,
                message,
            } => write!(
                f,
                "{}:{line} is not a registry record: {message}\n\
                 A line this reader does not understand is not skipped, because skipping it \
                 would under-count the trials of whatever family wrote it — and an under-counted \
                 trial count flatters every deflated Sharpe that reads it.",
                path.display()
            ),
            RegistryError::UnknownRun { run_id } => write!(
                f,
                "run {run_id} was finished but never inserted; the insert-before-run rule exists \
                 so that a crash leaves a corpse rather than a gap, and a finish with no insert \
                 means something wrote results without claiming them first"
            ),
        }
    }
}

impl std::error::Error for RegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RegistryError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// The append-only trial registry.
///
/// The write handle is held open for the life of the value rather than
/// reopened per append. On Windows a reader that opens the file with
/// `FileShare.Read` — which `Get-Content` and `System.IO.StreamReader` both do
/// — denies *new* writers, so a progress check taken with the wrong tool would
/// otherwise end a run (D-0065 records that happening). An already-open handle
/// is unaffected.
#[derive(Debug)]
pub struct Registry {
    path: PathBuf,
    sink: File,
    /// Every run seen, and how it ended.
    runs: BTreeMap<String, RunStatus>,
    /// Distinct trials charged, per family.
    trials: BTreeMap<String, usize>,
    /// Which trials have already been charged, so a resumed fold does not
    /// charge its combo twice.
    charged: BTreeMap<(String, Option<String>, usize), String>,
    verdicts: Vec<VerdictRow>,
}

impl Registry {
    /// Opens (creating if absent) the store at `path`, replaying it into an
    /// index.
    ///
    /// The whole file is read. It is one line per run, so a year of daily
    /// grids is megabytes — and the alternative, trusting a cached count, is
    /// how a trial count becomes a rumor.
    ///
    /// # Errors
    /// [`RegistryError`] if the file cannot be created or read, or if any line
    /// in it is not a record this build understands.
    pub fn open(path: &Path) -> Result<Registry, RegistryError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| RegistryError::Io {
                path: parent.to_owned(),
                source,
            })?;
        }
        let io = |source| RegistryError::Io {
            path: path.to_owned(),
            source,
        };

        let mut registry = Registry {
            path: path.to_owned(),
            sink: OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(io)?,
            runs: BTreeMap::new(),
            trials: BTreeMap::new(),
            charged: BTreeMap::new(),
            verdicts: Vec::new(),
        };

        let file = File::open(path).map_err(io)?;
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(io)?;
            if line.trim().is_empty() {
                continue;
            }
            let record: Line = serde_json::from_str(&line).map_err(|e| RegistryError::Corrupt {
                path: path.to_owned(),
                line: i + 1,
                message: e.to_string(),
            })?;
            registry.apply(record)?;
        }
        Ok(registry)
    }

    /// Folds one record into the index. Shared by [`Registry::open`] and every
    /// append, so a fresh reader and a live writer cannot disagree about what
    /// the file means.
    fn apply(&mut self, record: Line) -> Result<(), RegistryError> {
        match record {
            Line::Run(row) => {
                let id = row.key.run_id();
                if self.runs.insert(id.clone(), RunStatus::Running).is_none()
                    && self.charged.insert(row.key.trial(), id).is_none()
                {
                    *self
                        .trials
                        .entry(row.hypothesis_family.clone())
                        .or_insert(0) += 1;
                }
            }
            Line::RunFinished { run_id, status, .. } => {
                let entry = self
                    .runs
                    .get_mut(&run_id)
                    .ok_or(RegistryError::UnknownRun { run_id })?;
                *entry = status;
            }
            Line::Verdict(row) => self.verdicts.push(*row),
        }
        Ok(())
    }

    fn append(&mut self, record: &Line) -> Result<(), RegistryError> {
        let mut line = serde_json::to_string(record).map_err(|e| RegistryError::Io {
            path: self.path.clone(),
            source: std::io::Error::other(e),
        })?;
        line.push('\n');
        let io = |source| RegistryError::Io {
            path: self.path.clone(),
            source,
        };
        self.sink.write_all(line.as_bytes()).map_err(io)?;
        self.sink.flush().map_err(io)
    }

    /// Where the store lives.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Claims a run **before** it executes.
    ///
    /// Returns [`Inserted::AlreadyDone`] without writing anything when this
    /// exact run has finished before — that is the whole resume mechanism, and
    /// it is why re-running a 4,000-combo grid after a crash costs only the
    /// combos that did not finish.
    ///
    /// # Errors
    /// [`RegistryError::Io`] if the row cannot be appended. A run whose claim
    /// could not be written is not started: a result with no row is a number
    /// nobody can reproduce (§2.5).
    pub fn insert_running(&mut self, row: &RunRow) -> Result<Inserted, RegistryError> {
        let id = row.key.run_id();
        match self.runs.get(&id) {
            Some(RunStatus::Done) => return Ok(Inserted::AlreadyDone),
            Some(RunStatus::Running | RunStatus::Failed) => {
                self.runs.insert(id, RunStatus::Running);
                return Ok(Inserted::Retrying);
            }
            None => {}
        }
        let record = Line::Run(Box::new(row.clone()));
        self.append(&record)?;
        self.apply(record)?;
        Ok(Inserted::New)
    }

    /// Records how a claimed run ended.
    ///
    /// # Errors
    /// [`RegistryError::UnknownRun`] if the run was never claimed, or
    /// [`RegistryError::Io`] if the line cannot be appended.
    pub fn finish(
        &mut self,
        key: &RunKey,
        status: RunStatus,
        metrics: Option<RunMetrics>,
        finished_at: &str,
    ) -> Result<(), RegistryError> {
        let record = Line::RunFinished {
            run_id: key.run_id(),
            status,
            metrics,
            finished_at: finished_at.to_owned(),
        };
        self.append(&record)?;
        self.apply(record)
    }

    /// Records a verdict.
    ///
    /// # Errors
    /// [`RegistryError::Io`] if the line cannot be appended.
    pub fn record_verdict(&mut self, row: &VerdictRow) -> Result<(), RegistryError> {
        let record = Line::Verdict(Box::new(row.clone()));
        self.append(&record)?;
        self.apply(record)
    }

    /// How many distinct trials this family has been charged — the number a
    /// deflated Sharpe divides by, read from here and never from memory.
    #[must_use]
    pub fn trials_for(&self, family: &str) -> usize {
        self.trials.get(family).copied().unwrap_or(0)
    }

    /// Status of one run, if it is known.
    #[must_use]
    pub fn status_of(&self, key: &RunKey) -> Option<RunStatus> {
        self.runs.get(&key.run_id()).copied()
    }

    /// Runs claimed and never finished — the corpses insert-before-run exists
    /// to leave behind.
    #[must_use]
    pub fn unfinished(&self) -> Vec<&str> {
        self.runs
            .iter()
            .filter(|&(_, &s)| s == RunStatus::Running)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// The strategy graveyard: every verdict ever recorded, in the order it
    /// was decided. A query, not a document — month three of research must not
    /// re-litigate month one.
    #[must_use]
    pub fn verdicts(&self) -> &[VerdictRow] {
        &self.verdicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::Stage;

    fn key(combo: usize, fold: Option<usize>, seed: u64) -> RunKey {
        RunKey {
            config_hash: "aa".repeat(32),
            account_id: None,
            combo_index: combo,
            fold,
            seed,
        }
    }

    fn row(key: RunKey, family: &str) -> RunRow {
        RunRow {
            key,
            hypothesis_family: family.to_owned(),
            params: "fast(period=10)".to_owned(),
            fill_model: "spread_cross".to_owned(),
            git_sha: "0123456".to_owned(),
            data_manifest_ids: vec![],
            started_at: "2026-07-30T00:00:00Z".to_owned(),
            criteria: Criteria::for_tests(),
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("crucible-registry-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(format!("{name}.jsonl"));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// Rule 1: the row exists before the run does, and it says `running`.
    #[test]
    fn a_claimed_run_is_visible_before_it_finishes() {
        let path = tmp("claim");
        let mut r = Registry::open(&path).expect("opens");
        let k = key(0, Some(0), 7);
        assert_eq!(
            r.insert_running(&row(k.clone(), "fam")).expect("insert"),
            Inserted::New
        );
        assert_eq!(r.status_of(&k), Some(RunStatus::Running));
        assert_eq!(r.unfinished(), vec![k.run_id()]);

        r.finish(&k, RunStatus::Done, None, "2026-07-30T00:00:01Z")
            .expect("finishes");
        assert_eq!(r.status_of(&k), Some(RunStatus::Done));
        assert!(r.unfinished().is_empty());
    }

    /// Rule 2, and the resume path: finished work is never recomputed, and a
    /// corpse is retried without being charged a second trial.
    #[test]
    fn finished_work_dedupes_and_a_corpse_is_retried() {
        let path = tmp("dedupe");
        let done = key(0, Some(0), 7);
        let crashed = key(1, Some(0), 8);
        {
            let mut r = Registry::open(&path).expect("opens");
            r.insert_running(&row(done.clone(), "fam")).expect("insert");
            r.finish(&done, RunStatus::Done, None, "t").expect("finish");
            r.insert_running(&row(crashed.clone(), "fam"))
                .expect("insert");
        }

        // Reopened from disk, because that is what a resumed run does.
        let mut r = Registry::open(&path).expect("reopens");
        assert_eq!(r.trials_for("fam"), 2);
        assert_eq!(
            r.insert_running(&row(done, "fam")).expect("dedupes"),
            Inserted::AlreadyDone
        );
        assert_eq!(
            r.insert_running(&row(crashed, "fam")).expect("retries"),
            Inserted::Retrying
        );
        assert_eq!(r.trials_for("fam"), 2, "a retry is not a new trial");
    }

    /// Rule 3, and the part that decides a deflated Sharpe: folds of one combo
    /// are ONE trial, and a second account is a new one (D-0067).
    #[test]
    fn trials_count_combos_and_accounts_not_folds() {
        let path = tmp("trials");
        let mut r = Registry::open(&path).expect("opens");
        for fold in 0..8u64 {
            r.insert_running(&row(
                key(0, Some(usize::try_from(fold).expect("small")), 100 + fold),
                "fam",
            ))
            .expect("insert");
        }
        assert_eq!(r.trials_for("fam"), 1, "eight folds of one combo");

        r.insert_running(&row(key(1, Some(0), 1), "fam"))
            .expect("insert");
        assert_eq!(r.trials_for("fam"), 2, "a second combo");

        let mut with_account = key(0, Some(0), 100);
        with_account.account_id = Some("apex_50k".to_owned());
        r.insert_running(&row(with_account, "fam")).expect("insert");
        assert_eq!(r.trials_for("fam"), 3, "the same combo, a second account");

        r.insert_running(&row(key(0, Some(0), 100), "other"))
            .expect("insert");
        assert_eq!(r.trials_for("other"), 0, "already charged to `fam`");
    }

    /// A config that differs only in its declared seed is a different run.
    /// Without the seed in the key it would dedupe into the first and never
    /// execute — the exact hole D-0064 predicted.
    #[test]
    fn two_configs_differing_only_in_seed_do_not_dedupe_into_each_other() {
        let path = tmp("seeds");
        let mut r = Registry::open(&path).expect("opens");
        r.insert_running(&row(key(0, Some(0), 111), "fam"))
            .expect("insert");
        assert_eq!(
            r.insert_running(&row(key(0, Some(0), 222), "fam"))
                .expect("insert"),
            Inserted::New
        );
    }

    /// Rule 5: the graveyard survives a reopen, because it is on disk rather
    /// than in someone's head.
    #[test]
    fn verdicts_are_a_query_over_the_store() {
        let path = tmp("graveyard");
        {
            let mut r = Registry::open(&path).expect("opens");
            r.insert_running(&row(key(0, Some(0), 1), "fam"))
                .expect("insert");
            r.record_verdict(&VerdictRow {
                config_hash: "aa".repeat(32),
                account_id: None,
                combo_index: 0,
                hypothesis_family: "fam".to_owned(),
                decided_at: Stage::S0.to_string(),
                verdict: Verdict::Kill,
                reasons: vec!["3 OOS trades < 30".to_owned()],
                trials_at_decision: 1,
                decided_on: "2026-07-30T00:00:00Z".to_owned(),
            })
            .expect("records");
        }

        let r = Registry::open(&path).expect("reopens");
        assert_eq!(r.verdicts().len(), 1);
        assert_eq!(r.verdicts()[0].verdict, Verdict::Kill);
        assert_eq!(r.verdicts()[0].trials_at_decision, 1);
    }

    /// A line this build cannot read is a refusal, not a skip. Skipping would
    /// under-count trials, and an under-counted trial count is the one error
    /// here that makes every downstream number look better than it is.
    #[test]
    fn an_unreadable_line_refuses_rather_than_being_skipped() {
        let path = tmp("corrupt");
        {
            let mut r = Registry::open(&path).expect("opens");
            r.insert_running(&row(key(0, Some(0), 1), "fam"))
                .expect("insert");
        }
        let mut f = OpenOptions::new().append(true).open(&path).expect("append");
        writeln!(f, r#"{{"kind":"something_from_the_future","n":1}}"#).expect("write");
        drop(f);

        let err = Registry::open(&path).expect_err("must refuse");
        assert!(
            matches!(err, RegistryError::Corrupt { line: 2, .. }),
            "{err:?}"
        );
    }

    /// A finish with no claim means someone wrote results without the
    /// insert-before-run step, which is the rule that makes crashes visible.
    #[test]
    fn finishing_a_run_that_was_never_claimed_refuses() {
        let path = tmp("orphan-finish");
        {
            let mut r = Registry::open(&path).expect("opens");
            r.finish(&key(0, Some(0), 1), RunStatus::Done, None, "t")
                .expect_err("must refuse");
        }
        // And the refusal is durable: the orphan line reached disk before the
        // index rejected it, so reopening reports it rather than accepting it.
        let err = Registry::open(&path).expect_err("must refuse on reopen");
        assert!(matches!(err, RegistryError::UnknownRun { .. }), "{err:?}");
    }
}
