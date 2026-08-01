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
//! 3. **Trial counting is automatic** and per hypothesis family. One trial key
//!    binds to exactly one family: a later claim cannot move one trial between
//!    denominators. Nothing asks a human how many things they tried.
//! 4. **Pre-registered criteria are stored on the row**, verbatim. A criterion
//!    that only exists in the config file at the moment of judging is not
//!    pre-registered, it is remembered.
//! 5. **The graveyard is a query, not a document**: [`Registry::verdicts`]
//!    replays every kill with its stage, its reasons and its date.
//!
//! ## Results evolve reader-first
//!
//! Historical trading metrics keep their original untagged object shape, and
//! historical explicit `metrics: null` is indexed as named legacy absence. A
//! predictor result is a separately tagged [`crate::s0::S0RunMetrics`], never a
//! trading result filled with invented zeros. The reader refuses unknown,
//! extended, omitted, or claim-contradicting result shapes. Production writers
//! may emit a new typed result only after this reader contract is deployed.
//!
//! ## Correction is appended, never erased (D-0083)
//!
//! A row written in error is a fact: it happened. So the correction is a
//! `void` line naming the offending `run_id` and a reason, and
//! [`Registry::trials_for`] excludes voided runs **by construction** — a trial
//! counts while at least one run charged to it still stands. Deleting the row
//! instead would make the file claim the mistake never occurred, which is the
//! same class of dishonesty as editing a golden value to get green.
//!
//! A trial count is a denominator: it feeds deflated Sharpe and every other
//! multiple-testing correction. Wrong in the *large* direction it merely
//! costs power; wrong in the *small* direction it flatters every result that
//! reads it. That asymmetry is why an unknown line kind refuses rather than
//! skips, and it is the same reason voiding had to be a first-class record
//! rather than a hand-edit.
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
//! The family is owned by that trial's charge (D-0105), never copied onto the
//! void index; both increment and decrement therefore address the same family.
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

/// What a run is charged to: `(config_hash, account_id, combo_index)`.
///
/// Named because four indices key on it and clippy is right that the bare
/// tuple had stopped being readable. Folds and seeds are deliberately absent —
/// that is the whole of rule 3.
type TrialKey = (String, Option<String>, usize);

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
    fn trial(&self) -> TrialKey {
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

/// The trading numbers a finished replay is remembered by.
///
/// Deliberately small: equity curves stay in the scorecard artifacts, which
/// name this `run_id`. Predictor runs have a different typed result because
/// forcing them into this shape would manufacture equity and trade zeros.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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

#[derive(Deserialize)]
#[serde(untagged)]
enum RequiredNullableF64 {
    Null(()),
    Value(f64),
}

impl RequiredNullableF64 {
    fn into_option(self) -> Option<f64> {
        match self {
            Self::Null(()) => None,
            Self::Value(value) => Some(value),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunMetricsReader {
    final_equity_nano_usd: i64,
    return_pct: f64,
    max_dd_pct: f64,
    sharpe_naive: RequiredNullableF64,
    round_trips: usize,
    win_rate: RequiredNullableF64,
    fees_nano_usd: i64,
}

impl<'de> Deserialize<'de> for RunMetrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = RunMetricsReader::deserialize(deserializer)?;
        Ok(Self {
            final_equity_nano_usd: wire.final_equity_nano_usd,
            return_pct: wire.return_pct,
            max_dd_pct: wire.max_dd_pct,
            sharpe_naive: wire.sharpe_naive.into_option(),
            round_trips: wire.round_trips,
            win_rate: wire.win_rate.into_option(),
            fees_nano_usd: wire.fees_nano_usd,
        })
    }
}

/// A finished run's result as understood by this reader.
///
/// Historical `metrics: null` rows remain visible as [`Self::LegacyAbsent`];
/// they are never reinterpreted as zero-valued trading results or as S0
/// evidence. New result families are closed and typed, so a reader refuses a
/// shape it does not know rather than silently discarding decision fields.
#[derive(Clone, Debug, PartialEq)]
pub enum PersistedRunResult {
    /// A historical finish row explicitly contained `metrics: null`.
    LegacyAbsent,
    /// The original trading/equity result shape.
    Trading(RunMetrics),
    /// A complete S0 measurement and its exact population scope.
    S0(crate::s0::S0RunMetrics),
}

/// On-disk result compatibility union.
///
/// `Trading` is untagged so every pre-existing non-null metrics object retains
/// its exact JSON shape. Typed successors carry an explicit discriminator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum MetricsWire {
    LegacyAbsent(()),
    Trading(RunMetrics),
    Typed(TypedMetricsWire),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "metric_kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum TypedMetricsWire {
    S0(crate::s0::S0RunMetrics),
}

impl From<MetricsWire> for PersistedRunResult {
    fn from(value: MetricsWire) -> Self {
        match value {
            MetricsWire::LegacyAbsent(()) => Self::LegacyAbsent,
            MetricsWire::Trading(metrics) => Self::Trading(metrics),
            MetricsWire::Typed(TypedMetricsWire::S0(metrics)) => Self::S0(metrics),
        }
    }
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
        metrics: MetricsWire,
        finished_at: String,
    },
    /// A verdict over a combo.
    Verdict(Box<VerdictRow>),
    /// A prior run, withdrawn from the record (D-0083).
    ///
    /// The store is append-only, so a row written in error is corrected by
    /// appending its withdrawal rather than by deleting it: what happened is
    /// that a row was written *and then* found not to be a trial, and a file
    /// that erased the first half would be claiming the mistake never occurred.
    /// A void names its target by [`RunKey::run_id`] — the same identity the
    /// row was inserted under — so the correction is checkable against the
    /// thing it corrects.
    Void {
        /// The `run_id` being withdrawn.
        run_id: String,
        /// Why, in words a reader five years from now can audit. Required: a
        /// void with no reason is indistinguishable from a deletion.
        reason: String,
        /// Wall clock, supplied by the caller.
        voided_at: String,
    },
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

#[derive(Clone, Debug)]
struct RunClaim {
    combo_index: usize,
    fold: Option<usize>,
    params: String,
    fill_model: String,
    declares_s0: bool,
    s0_min_abs_ic: Option<f64>,
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
    /// Typed result identity disagrees with the run row that claimed it.
    ResultDoesNotMatchClaim {
        /// The claimed run.
        run_id: String,
        /// The exact identity field that contradicted the claim.
        message: String,
    },
    /// A trial key was already charged to a different hypothesis family.
    TrialFamilyMismatch {
        /// Config identity in the conflicting trial key.
        config_hash: String,
        /// Account identity in the conflicting trial key.
        account_id: Option<String>,
        /// Combo identity in the conflicting trial key.
        combo_index: usize,
        /// Family that owns the existing charge.
        established_family: String,
        /// Family named by the refused claim.
        claimed_family: String,
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
            RegistryError::ResultDoesNotMatchClaim { run_id, message } => write!(
                f,
                "run {run_id} has a result that contradicts its claimed row: {message}"
            ),
            RegistryError::TrialFamilyMismatch {
                config_hash,
                account_id,
                combo_index,
                established_family,
                claimed_family,
            } => write!(
                f,
                "trial ({config_hash}, {}, {combo_index}) is already charged to hypothesis_family {established_family:?}; claim names {claimed_family:?}",
                account_id.as_deref().unwrap_or("-")
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
    /// `None` for an ephemeral registry, which honours the contract in memory
    /// and never touches the disk ([`Registry::ephemeral`]).
    sink: Option<File>,
    /// Every run seen, and how it ended.
    runs: BTreeMap<String, RunStatus>,
    /// Identity fields retained from each run's insert-before-run claim.
    claims: BTreeMap<String, RunClaim>,
    /// Every finished run's typed result, including explicit legacy absence.
    results: BTreeMap<String, PersistedRunResult>,
    /// Distinct trials charged, per family.
    trials: BTreeMap<String, usize>,
    /// Canonical family charged for each trial. Presence prevents a resumed
    /// fold from charging twice; the value is the one owner used by voids.
    charged: BTreeMap<TrialKey, String>,
    /// Every run_id ever charged to each trial, so withdrawing one can tell
    /// whether the trial still has a surviving run behind it (D-0083).
    trial_members: BTreeMap<TrialKey, Vec<String>>,
    /// Which trial each run belongs to, so a void can find the canonical
    /// charge from the `run_id` alone. Family is deliberately not duplicated.
    run_trial: BTreeMap<String, TrialKey>,
    /// Runs withdrawn from the record.
    voided: std::collections::BTreeSet<String>,
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
            sink: Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(io)?,
            ),
            runs: BTreeMap::new(),
            claims: BTreeMap::new(),
            results: BTreeMap::new(),
            trials: BTreeMap::new(),
            charged: BTreeMap::new(),
            trial_members: BTreeMap::new(),
            run_trial: BTreeMap::new(),
            voided: std::collections::BTreeSet::new(),
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

    /// An **ephemeral** registry: the contract of this module honoured entirely
    /// in memory, reading nothing and writing nothing (D-0083).
    ///
    /// This exists for the determinism gates. `--hash-only` asks "what number
    /// does this config produce", which is a question about the *code*, not a
    /// piece of research — so it must not charge a trial, and it must not
    /// deposit rows in the research memory that a later deflated Sharpe would
    /// divide by. A gate that costs trials makes the honesty machinery expensive
    /// to test, which is the wrong incentive to build into it.
    ///
    /// It deliberately does **not** read the existing store either. A gate whose
    /// output depended on how many times it had been run before would not be a
    /// determinism gate; starting from an empty index makes the hash a function
    /// of the config and the data alone.
    #[must_use]
    pub fn ephemeral(path: &Path) -> Registry {
        Registry {
            path: path.to_owned(),
            sink: None,
            runs: BTreeMap::new(),
            claims: BTreeMap::new(),
            results: BTreeMap::new(),
            trials: BTreeMap::new(),
            charged: BTreeMap::new(),
            trial_members: BTreeMap::new(),
            run_trial: BTreeMap::new(),
            voided: std::collections::BTreeSet::new(),
            verdicts: Vec::new(),
        }
    }

    /// Whether this registry only exists in memory.
    #[must_use]
    pub const fn is_ephemeral(&self) -> bool {
        self.sink.is_none()
    }

    fn validate_finished_result(
        &self,
        run_id: &str,
        metrics: &MetricsWire,
    ) -> Result<(), RegistryError> {
        let claim = self
            .claims
            .get(run_id)
            .ok_or_else(|| RegistryError::UnknownRun {
                run_id: run_id.to_owned(),
            })?;
        let is_s0_measurement =
            claim.fold.is_none() && claim.fill_model == crate::s0::S0_FILL_MODEL;
        let metrics = match metrics {
            MetricsWire::LegacyAbsent(()) => return Ok(()),
            MetricsWire::Trading(_) => {
                if is_s0_measurement {
                    return Err(RegistryError::ResultDoesNotMatchClaim {
                        run_id: run_id.to_owned(),
                        message: "a no-position S0 claim cannot contain trading metrics".to_owned(),
                    });
                }
                return Ok(());
            }
            MetricsWire::Typed(TypedMetricsWire::S0(metrics)) => metrics,
        };
        metrics
            .validate()
            .map_err(|message| RegistryError::ResultDoesNotMatchClaim {
                run_id: run_id.to_owned(),
                message,
            })?;
        if !claim.declares_s0 {
            return Err(RegistryError::ResultDoesNotMatchClaim {
                run_id: run_id.to_owned(),
                message: "typed S0 metrics require an S0 criterion on the claimed row".to_owned(),
            });
        }
        if !is_s0_measurement {
            return Err(RegistryError::ResultDoesNotMatchClaim {
                run_id: run_id.to_owned(),
                message: "typed S0 metrics require the no-position, non-fold S0 claim".to_owned(),
            });
        }
        if claim.combo_index != metrics.combo.combo_index {
            return Err(RegistryError::ResultDoesNotMatchClaim {
                run_id: run_id.to_owned(),
                message: format!(
                    "claimed combo index {} but metrics name {}",
                    claim.combo_index, metrics.combo.combo_index
                ),
            });
        }
        if claim.params != metrics.combo.label {
            return Err(RegistryError::ResultDoesNotMatchClaim {
                run_id: run_id.to_owned(),
                message: format!(
                    "claimed params {:?} but metrics label is {:?}",
                    claim.params, metrics.combo.label
                ),
            });
        }
        if claim.s0_min_abs_ic.map(f64::to_bits) != Some(metrics.combo.spec.min_abs_ic.to_bits()) {
            return Err(RegistryError::ResultDoesNotMatchClaim {
                run_id: run_id.to_owned(),
                message: "persisted S0 threshold differs from the pre-registered criterion"
                    .to_owned(),
            });
        }
        Ok(())
    }

    fn validate_trial_family(&self, row: &RunRow) -> Result<(), RegistryError> {
        let trial = row.key.trial();
        if let Some(established_family) = self.charged.get(&trial)
            && established_family != &row.hypothesis_family
        {
            return Err(RegistryError::TrialFamilyMismatch {
                config_hash: row.key.config_hash.clone(),
                account_id: row.key.account_id.clone(),
                combo_index: row.key.combo_index,
                established_family: established_family.clone(),
                claimed_family: row.hypothesis_family.clone(),
            });
        }
        Ok(())
    }

    /// Folds one record into the index. Shared by [`Registry::open`] and every
    /// append, so a fresh reader and a live writer cannot disagree about what
    /// the file means.
    fn apply(&mut self, record: Line) -> Result<(), RegistryError> {
        match record {
            Line::Run(row) => {
                self.validate_trial_family(&row)?;
                let id = row.key.run_id();
                let trial = row.key.trial();
                if self.runs.insert(id.clone(), RunStatus::Running).is_none() {
                    self.claims.insert(
                        id.clone(),
                        RunClaim {
                            combo_index: row.key.combo_index,
                            fold: row.key.fold,
                            params: row.params.clone(),
                            fill_model: row.fill_model.clone(),
                            declares_s0: row.criteria.stages.contains(&crate::stages::Stage::S0),
                            s0_min_abs_ic: row.criteria.s0_min_abs_ic,
                        },
                    );
                    self.run_trial.insert(id.clone(), trial.clone());
                    self.trial_members
                        .entry(trial.clone())
                        .or_default()
                        .push(id.clone());
                    if self
                        .charged
                        .insert(trial, row.hypothesis_family.clone())
                        .is_none()
                    {
                        *self
                            .trials
                            .entry(row.hypothesis_family.clone())
                            .or_insert(0) += 1;
                    }
                }
            }
            Line::RunFinished {
                run_id,
                status,
                metrics,
                ..
            } => {
                if !self.runs.contains_key(&run_id) {
                    return Err(RegistryError::UnknownRun { run_id });
                }
                self.validate_finished_result(&run_id, &metrics)?;
                let entry = self.runs.get_mut(&run_id).expect("checked above");
                *entry = status;
                self.results
                    .insert(run_id, PersistedRunResult::from(metrics));
            }
            Line::Verdict(row) => self.verdicts.push(*row),
            Line::Void { run_id, .. } => {
                if !self.runs.contains_key(&run_id) {
                    return Err(RegistryError::UnknownRun { run_id });
                }
                if self.voided.insert(run_id.clone())
                    && let Some(trial) = self.run_trial.get(&run_id).cloned()
                {
                    // The trial survives while ANY run charged to it is still
                    // standing: folds of one combo are one trial (rule 3), so
                    // withdrawing one of them must not withdraw the combo.
                    let survives = self
                        .trial_members
                        .get(&trial)
                        .is_some_and(|m| m.iter().any(|r| !self.voided.contains(r)));
                    if !survives
                        && let Some(family) = self.charged.get(&trial)
                        && let Some(n) = self.trials.get_mut(family)
                    {
                        *n = n.saturating_sub(1);
                    }
                }
            }
        }
        Ok(())
    }

    fn append(&mut self, record: &Line) -> Result<(), RegistryError> {
        let Some(sink) = self.sink.as_mut() else {
            // Ephemeral: the contract is honoured in memory and the disk is
            // never touched. See [`Registry::ephemeral`].
            return Ok(());
        };
        let mut line = serde_json::to_string(record).map_err(|e| RegistryError::Io {
            path: self.path.clone(),
            source: std::io::Error::other(e),
        })?;
        line.push('\n');
        let path = self.path.clone();
        let io = |source| RegistryError::Io {
            path: path.clone(),
            source,
        };
        sink.write_all(line.as_bytes()).map_err(io)?;
        sink.flush().map_err(io)
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
    /// [`RegistryError::TrialFamilyMismatch`] if the trial was already bound
    /// to another family, or [`RegistryError::Io`] if the row cannot be
    /// appended. A run whose claim could not be written is not started: a
    /// result with no row is a number nobody can reproduce (§2.5).
    pub fn insert_running(&mut self, row: &RunRow) -> Result<Inserted, RegistryError> {
        // Validate before the status shortcut and, critically, before append:
        // an AlreadyDone spelling with another family is still a contradictory
        // claim about the same trial, not a successful resume.
        self.validate_trial_family(row)?;
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
        let run_id = key.run_id();
        let metrics = metrics.map_or(MetricsWire::LegacyAbsent(()), MetricsWire::Trading);
        if self.runs.contains_key(&run_id) {
            self.validate_finished_result(&run_id, &metrics)?;
        }
        let record = Line::RunFinished {
            run_id,
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

    /// Withdraws a previously-recorded run from the record (D-0083).
    ///
    /// Appends a `void` line rather than editing anything: the store is
    /// append-only, and a row written in error is a fact about what happened.
    /// After this, the run is excluded from [`Registry::trials_for`] — unless
    /// another run charged to the same trial is still standing, because folds
    /// of one combo are one trial (rule 3).
    ///
    /// # Errors
    /// [`RegistryError::UnknownRun`] if no such run was ever inserted — a void
    /// that names nothing would be an assertion about a row that does not
    /// exist. [`RegistryError::Io`] if the line cannot be appended.
    pub fn void(
        &mut self,
        run_id: &str,
        reason: &str,
        voided_at: &str,
    ) -> Result<(), RegistryError> {
        if !self.runs.contains_key(run_id) {
            return Err(RegistryError::UnknownRun {
                run_id: run_id.to_owned(),
            });
        }
        let record = Line::Void {
            run_id: run_id.to_owned(),
            reason: reason.to_owned(),
            voided_at: voided_at.to_owned(),
        };
        self.append(&record)?;
        self.apply(record)
    }

    /// Whether this run has been withdrawn from the record.
    #[must_use]
    pub fn is_voided(&self, run_id: &str) -> bool {
        self.voided.contains(run_id)
    }

    /// Every withdrawn run, in id order. A query over the corrections, so a
    /// reader can audit what was withdrawn as easily as what was recorded.
    #[must_use]
    pub fn voided(&self) -> Vec<&str> {
        self.voided.iter().map(String::as_str).collect()
    }

    /// How many distinct trials this family has been charged — the number a
    /// deflated Sharpe divides by, read from here and never from memory.
    ///
    /// **Voided runs are excluded by construction** (D-0083): a trial counts
    /// while at least one run charged to it is still standing. That is what
    /// makes the correction real rather than cosmetic — the number that feeds
    /// the multiple-testing correction is the one that moves.
    #[must_use]
    pub fn trials_for(&self, family: &str) -> usize {
        self.trials.get(family).copied().unwrap_or(0)
    }

    /// Status of one run, if it is known.
    #[must_use]
    pub fn status_of(&self, key: &RunKey) -> Option<RunStatus> {
        self.runs.get(&key.run_id()).copied()
    }

    /// Persisted result of one finished run, if a finish row exists.
    ///
    /// A historical null is returned as [`PersistedRunResult::LegacyAbsent`],
    /// distinct from a run that has never finished and from every measured
    /// value.
    #[must_use]
    pub fn result_of(&self, key: &RunKey) -> Option<&PersistedRunResult> {
        self.results.get(&key.run_id())
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
    ///
    /// This returns **everything**, including verdicts whose runs were later
    /// voided, because the log is the record of what happened. Statistics want
    /// [`Registry::verdicts_standing`].
    #[must_use]
    pub fn verdicts(&self) -> &[VerdictRow] {
        &self.verdicts
    }

    /// Whether every run charged to this trial has been withdrawn (D-0083).
    ///
    /// A verdict has no `run_id` of its own — it is decided over a *combo*, not
    /// a run — so its voided-ness is inherited from the trial it names, which is
    /// exactly the `(config_hash, account_id, combo_index)` triple it carries.
    #[must_use]
    pub fn is_trial_voided(
        &self,
        config_hash: &str,
        account_id: Option<&str>,
        combo_index: usize,
    ) -> bool {
        let trial = (
            config_hash.to_owned(),
            account_id.map(str::to_owned),
            combo_index,
        );
        match self.trial_members.get(&trial) {
            // A trial nobody ever ran is not "voided", it is absent.
            None => false,
            Some(members) => members.iter().all(|r| self.voided.contains(r)),
        }
    }

    /// The graveyard as **statistics** should read it: verdicts whose trials
    /// still stand (D-0083).
    ///
    /// The distinction matters because a verdict is evidence about an idea, and
    /// a verdict produced by a determinism gate is evidence about nothing. It is
    /// a separate method rather than a filter inside [`Registry::verdicts`] so
    /// that "what does the log say" and "what counts" stay askable separately —
    /// collapsing them would make the correction invisible, which is the failure
    /// voiding exists to avoid.
    #[must_use]
    pub fn verdicts_standing(&self) -> Vec<&VerdictRow> {
        self.verdicts
            .iter()
            .filter(|v| {
                !self.is_trial_voided(&v.config_hash, v.account_id.as_deref(), v.combo_index)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s0::{
        Availability, Bucket, BucketSet, HorizonEvidence, Interval, S0ComboReport, S0EvidenceScope,
        S0RunMetrics, S0Spec, UnavailableReason,
    };
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

    fn s0_row(key: RunKey, family: &str) -> RunRow {
        let mut row = row(key, family);
        row.params = "zscore(period=20)".to_owned();
        row.fill_model = crate::s0::S0_FILL_MODEL.to_owned();
        row.criteria.stages.push(Stage::S0);
        row.criteria.s0_min_abs_ic = Some(0.5);
        row
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("crucible-registry-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(format!("{name}.jsonl"));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn measured_s0_metrics() -> S0RunMetrics {
        S0RunMetrics {
            evidence_scope: S0EvidenceScope::EqualCountScoreBuckets,
            combo: S0ComboReport {
                score_identity: "z = zscore(close, period=20)".to_owned(),
                tick_size_nanopoints: 250_000_000,
                spec: S0Spec {
                    score_slot: "z".to_owned(),
                    horizons_ns: vec![60_000_000_000, 120_000_000_000],
                    buckets: 2,
                    bootstrap_draws: 200,
                    min_abs_ic: 0.5,
                },
                combo_index: 0,
                label: "zscore(period=20)".to_owned(),
                warmup_bars: 19,
                scores: 4,
                horizons: vec![
                    HorizonEvidence {
                        horizon_ns: 60_000_000_000,
                        n_pairs: 4,
                        dropped_no_partner: 0,
                        dropped_invalid_price: 0,
                        ic: Availability::Available { value: -1.0 },
                        buckets: Availability::Available {
                            value: BucketSet::from_nonempty(vec![
                                Bucket {
                                    score_lo: -2.0,
                                    score_hi: -1.0,
                                    n: 2,
                                    mean_return: 0.002,
                                    mean_move_ticks: 2.0,
                                },
                                Bucket {
                                    score_lo: 1.0,
                                    score_hi: 2.0,
                                    n: 2,
                                    mean_return: -0.002,
                                    mean_move_ticks: -2.0,
                                },
                            ])
                            .expect("ordered buckets"),
                        },
                        unconditional_mean_interval: Availability::Available {
                            value: Interval {
                                point: 0.0,
                                lo: -0.001,
                                hi: 0.001,
                                draws: 200,
                            },
                        },
                    },
                    HorizonEvidence {
                        horizon_ns: 120_000_000_000,
                        n_pairs: 1,
                        dropped_no_partner: 2,
                        dropped_invalid_price: 1,
                        ic: Availability::Unavailable {
                            reason: UnavailableReason::TooFewObservations {
                                observed: 1,
                                required: 2,
                            },
                        },
                        buckets: Availability::Unavailable {
                            reason: UnavailableReason::TooFewObservations {
                                observed: 1,
                                required: 2,
                            },
                        },
                        unconditional_mean_interval: Availability::Unavailable {
                            reason: UnavailableReason::DegenerateBootstrap,
                        },
                    },
                ],
            },
        }
    }

    fn s0_float_bits(metrics: &S0RunMetrics) -> Vec<u64> {
        let mut bits = vec![metrics.combo.spec.min_abs_ic.to_bits()];
        for horizon in &metrics.combo.horizons {
            if let Availability::Available { value } = &horizon.ic {
                bits.push(value.to_bits());
            }
            if let Availability::Available { value } = &horizon.buckets {
                for bucket in value.as_slice() {
                    bits.extend([
                        bucket.score_lo.to_bits(),
                        bucket.score_hi.to_bits(),
                        bucket.mean_return.to_bits(),
                        bucket.mean_move_ticks.to_bits(),
                    ]);
                }
            }
            if let Availability::Available { value } = &horizon.unconditional_mean_interval {
                bits.extend([
                    value.point.to_bits(),
                    value.lo.to_bits(),
                    value.hi.to_bits(),
                ]);
            }
        }
        bits
    }

    fn append_json_line(path: &Path, value: &serde_json::Value) {
        let mut file = OpenOptions::new().append(true).open(path).expect("append");
        serde_json::to_writer(&mut file, value).expect("serialize fixture");
        writeln!(file).expect("terminate fixture line");
    }

    fn append_finished_json(path: &Path, key: &RunKey, metrics: serde_json::Value) {
        append_json_line(
            path,
            &serde_json::json!({
                "kind": "run_finished",
                "run_id": key.run_id(),
                "status": "done",
                "metrics": metrics,
                "finished_at": "2026-08-01T00:00:00Z"
            }),
        );
    }

    #[test]
    fn legacy_trading_metrics_keep_their_wire_shape_and_round_trip() {
        let path = tmp("legacy-trading-metrics");
        let key = key(0, None, 7);
        let metrics = RunMetrics {
            final_equity_nano_usd: 123,
            return_pct: 0.25,
            max_dd_pct: 0.125,
            sharpe_naive: Some(1.5),
            round_trips: 9,
            win_rate: Some(0.5),
            fees_nano_usd: 17,
        };
        {
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&row(key.clone(), "fam"))
                .expect("insert");
            registry
                .finish(&key, RunStatus::Done, Some(metrics), "finished")
                .expect("finish");
        }

        let text = std::fs::read_to_string(&path).expect("read registry");
        let finish = text.lines().nth(1).expect("finish line");
        let expected = format!(
            "{{'kind':'run_finished','run_id':'{}','status':'done','metrics':{{'final_equity_nano_usd':123,'return_pct':0.25,'max_dd_pct':0.125,'sharpe_naive':1.5,'round_trips':9,'win_rate':0.5,'fees_nano_usd':17}},'finished_at':'finished'}}",
            key.run_id()
        )
        .replace('\'', &char::from(34).to_string());
        assert_eq!(finish, expected, "the old writer's bytes must not move");

        let reread = Registry::open(&path).expect("reopens");
        assert_eq!(
            reread.result_of(&key),
            Some(&PersistedRunResult::Trading(metrics))
        );
    }

    #[test]
    fn explicit_legacy_nullable_metrics_remain_required_and_byte_exact() {
        let path = tmp("legacy-nullable-fields");
        let key = key(0, None, 7);
        let metrics = RunMetrics {
            final_equity_nano_usd: 123,
            return_pct: 0.25,
            max_dd_pct: 0.125,
            sharpe_naive: None,
            round_trips: 0,
            win_rate: None,
            fees_nano_usd: 17,
        };
        {
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&row(key.clone(), "fam"))
                .expect("insert");
            registry
                .finish(&key, RunStatus::Done, Some(metrics), "finished")
                .expect("finish");
        }

        let text = std::fs::read_to_string(&path).expect("read registry");
        let finish = text.lines().nth(1).expect("finish line");
        let expected = format!(
            "{{'kind':'run_finished','run_id':'{}','status':'done','metrics':{{'final_equity_nano_usd':123,'return_pct':0.25,'max_dd_pct':0.125,'sharpe_naive':null,'round_trips':0,'win_rate':null,'fees_nano_usd':17}},'finished_at':'finished'}}",
            key.run_id()
        )
        .replace('\'', &char::from(34).to_string());
        assert_eq!(finish, expected, "explicit null fields must remain on wire");
        let reread = Registry::open(&path).expect("reopens");
        assert_eq!(
            reread.result_of(&key),
            Some(&PersistedRunResult::Trading(metrics))
        );
    }

    #[test]
    fn historical_null_metrics_are_explicit_legacy_absence() {
        let path = tmp("legacy-null-metrics");
        let key = key(0, None, 7);
        {
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&row(key.clone(), "fam"))
                .expect("insert");
            registry
                .finish(&key, RunStatus::Done, None, "finished")
                .expect("finish");
            assert_eq!(
                registry.result_of(&key),
                Some(&PersistedRunResult::LegacyAbsent)
            );
        }

        let text = std::fs::read_to_string(&path).expect("read registry");
        let finish = text.lines().nth(1).expect("finish line");
        let expected = format!(
            "{{'kind':'run_finished','run_id':'{}','status':'done','metrics':null,'finished_at':'finished'}}",
            key.run_id()
        )
        .replace('\'', &char::from(34).to_string());
        assert_eq!(finish, expected, "explicit legacy null bytes must not move");
        let reread = Registry::open(&path).expect("reopens");
        assert_eq!(reread.status_of(&key), Some(RunStatus::Done));
        assert_eq!(
            reread.result_of(&key),
            Some(&PersistedRunResult::LegacyAbsent),
            "a historical S0 null must not become zero or passed evidence"
        );
    }

    #[test]
    fn real_reader_round_trips_every_typed_s0_decision_field_bit_for_bit() {
        let path = tmp("typed-s0-reader");
        let key = key(0, None, 7);
        let expected = measured_s0_metrics();
        expected.validate().expect("valid S0 fixture");
        {
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&s0_row(key.clone(), "fam"))
                .expect("insert");
        }
        append_finished_json(
            &path,
            &key,
            serde_json::json!({
                "metric_kind": "s0",
                "value": expected.clone()
            }),
        );

        let reread = Registry::open(&path).expect("real reader accepts typed S0");
        let Some(PersistedRunResult::S0(actual)) = reread.result_of(&key) else {
            panic!("finished S0 row must expose typed S0 metrics");
        };
        assert_eq!(&expected, actual, "every non-float decision field");
        assert_eq!(
            s0_float_bits(&expected),
            s0_float_bits(actual),
            "every persisted floating-point decision field must retain its bits"
        );
        assert_eq!(
            serde_json::to_vec(&expected).expect("serialize expected"),
            serde_json::to_vec(actual).expect("serialize actual"),
            "scope, identity, order, absence, and derived criterion all round-trip"
        );
    }

    #[test]
    fn unknown_or_extended_metric_shapes_are_refused() {
        let unknown_kind = serde_json::json!({
            "metric_kind": "future_metric",
            "value": {}
        });
        let mut extended_s0 = serde_json::to_value(measured_s0_metrics()).expect("S0 JSON");
        extended_s0
            .as_object_mut()
            .expect("S0 object")
            .insert("future_decision".to_owned(), serde_json::json!(1));
        let legacy = serde_json::json!({
            "final_equity_nano_usd": 123,
            "return_pct": 0.25,
            "max_dd_pct": 0.125,
            "sharpe_naive": null,
            "round_trips": 0,
            "win_rate": null,
            "fees_nano_usd": 17
        });
        let mut extended_legacy = legacy.clone();
        extended_legacy
            .as_object_mut()
            .expect("legacy object")
            .insert("future_decision".to_owned(), serde_json::json!(1));
        let mut hybrid = legacy.clone();
        hybrid
            .as_object_mut()
            .expect("legacy object")
            .insert("metric_kind".to_owned(), serde_json::json!("s0"));
        hybrid
            .as_object_mut()
            .expect("legacy object")
            .insert("value".to_owned(), serde_json::json!(measured_s0_metrics()));
        let mut missing_sharpe = legacy.clone();
        missing_sharpe
            .as_object_mut()
            .expect("legacy object")
            .remove("sharpe_naive");
        let mut missing_win_rate = legacy;
        missing_win_rate
            .as_object_mut()
            .expect("legacy object")
            .remove("win_rate");

        for (name, metrics) in [
            ("unknown-metric-kind", unknown_kind),
            (
                "extended-s0-metrics",
                serde_json::json!({
                    "metric_kind": "s0",
                    "value": extended_s0
                }),
            ),
            ("extended-legacy-metrics", extended_legacy),
            ("hybrid-legacy-typed-metrics", hybrid),
            ("missing-legacy-sharpe", missing_sharpe),
            ("missing-legacy-win-rate", missing_win_rate),
        ] {
            let path = tmp(name);
            let key = key(0, None, 7);
            {
                let mut registry = Registry::open(&path).expect("opens");
                registry
                    .insert_running(&row(key.clone(), "fam"))
                    .expect("insert");
            }
            append_finished_json(&path, &key, metrics);
            let error = Registry::open(&path).expect_err("new shape must refuse");
            assert!(
                matches!(error, RegistryError::Corrupt { line: 2, .. }),
                "{name}: {error:?}"
            );
        }

        let path = tmp("missing-metrics-member");
        let key = key(0, None, 7);
        {
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&row(key.clone(), "fam"))
                .expect("insert");
        }
        append_json_line(
            &path,
            &serde_json::json!({
                "kind": "run_finished",
                "run_id": key.run_id(),
                "status": "done",
                "finished_at": "2026-08-01T00:00:00Z"
            }),
        );
        let error = Registry::open(&path).expect_err("omitted metrics must refuse");
        assert!(
            matches!(error, RegistryError::Corrupt { line: 2, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn typed_s0_identity_must_match_the_claimed_run_row() {
        for (name, mutate) in [
            ("s0-combo-mismatch", 0usize),
            ("s0-label-mismatch", 1usize),
            ("s0-threshold-mismatch", 2usize),
            ("s0-fold-mismatch", 3usize),
            ("s0-fill-model-mismatch", 4usize),
        ] {
            let path = tmp(name);
            let key = key(0, (mutate == 3).then_some(0), 7);
            let mut metrics = measured_s0_metrics();
            match mutate {
                0 => metrics.combo.combo_index = 1,
                1 => metrics.combo.label = "another-combo".to_owned(),
                2 => metrics.combo.spec.min_abs_ic = 0.75,
                3 | 4 => {}
                _ => unreachable!(),
            }
            let mut claim = s0_row(key.clone(), "fam");
            if mutate == 4 {
                claim.fill_model = "spread_cross".to_owned();
            }
            {
                let mut registry = Registry::open(&path).expect("opens");
                registry.insert_running(&claim).expect("insert");
            }
            append_finished_json(
                &path,
                &key,
                serde_json::json!({
                    "metric_kind": "s0",
                    "value": metrics
                }),
            );
            let error = Registry::open(&path).expect_err("identity mismatch must refuse");
            assert!(
                matches!(error, RegistryError::ResultDoesNotMatchClaim { .. }),
                "{name}: {error:?}"
            );
        }
    }

    #[test]
    fn predictor_claims_refuse_trading_metrics_but_keep_legacy_null_readable() {
        let metrics = RunMetrics {
            final_equity_nano_usd: 0,
            return_pct: 0.0,
            max_dd_pct: 0.0,
            sharpe_naive: None,
            round_trips: 0,
            win_rate: None,
            fees_nano_usd: 0,
        };
        let path = tmp("s0-refuses-trading-metrics");
        let s0_key = key(0, None, 7);
        {
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&s0_row(s0_key.clone(), "fam"))
                .expect("insert");
            let error = registry
                .finish(&s0_key, RunStatus::Done, Some(metrics), "finished")
                .expect_err("an S0 result is not a trading result full of zeros");
            assert!(
                matches!(error, RegistryError::ResultDoesNotMatchClaim { .. }),
                "{error:?}"
            );
            assert_eq!(
                std::fs::read_to_string(&path)
                    .expect("read")
                    .lines()
                    .count(),
                1,
                "claim/result disagreement must be refused before append"
            );
            assert_eq!(registry.status_of(&s0_key), Some(RunStatus::Running));
            assert_eq!(registry.result_of(&s0_key), None);

            registry
                .finish(&s0_key, RunStatus::Done, None, "finished")
                .expect("historical explicit null remains readable");
            assert_eq!(
                registry.result_of(&s0_key),
                Some(&PersistedRunResult::LegacyAbsent)
            );
        }
        assert!(matches!(
            Registry::open(&path).expect("reopens").result_of(&s0_key),
            Some(PersistedRunResult::LegacyAbsent)
        ));

        let path = tmp("s0-enabled-trading-fold");
        let fold_key = key(0, Some(0), 7);
        let mut claim = row(fold_key.clone(), "fam");
        claim.criteria.stages.push(Stage::S0);
        claim.criteria.s0_min_abs_ic = Some(0.5);
        let mut registry = Registry::open(&path).expect("opens");
        registry.insert_running(&claim).expect("insert");
        registry
            .finish(&fold_key, RunStatus::Done, Some(metrics), "finished")
            .expect("an S0-enabled config still has ordinary trading folds");
        assert_eq!(
            registry.result_of(&fold_key),
            Some(&PersistedRunResult::Trading(metrics))
        );
    }

    /// Rule 3 with a correction applied: a voided run stops counting as a
    /// trial (D-0083). The trial count is a DENOMINATOR — wrong small, it
    /// flatters every result that reads it — so the exclusion has to be real
    /// rather than cosmetic.
    #[test]
    fn a_voided_row_is_excluded_from_the_trial_count() {
        let path = tmp("void-excludes");
        let mut r = Registry::open(&path).expect("opens");

        let a = key(0, Some(0), 7);
        let b = key(1, Some(0), 7);
        r.insert_running(&row(a.clone(), "fam")).expect("insert a");
        r.insert_running(&row(b.clone(), "fam")).expect("insert b");
        assert_eq!(r.trials_for("fam"), 2, "two combos, two trials");

        r.void(
            &a.run_id(),
            "written by a determinism gate",
            "2026-07-30T00:00:00Z",
        )
        .expect("void");
        assert_eq!(r.trials_for("fam"), 1, "the voided combo stops counting");
        assert!(r.is_voided(&a.run_id()));
        assert!(!r.is_voided(&b.run_id()));

        // Append-only: the row it corrects is still in the file, and so is the
        // correction. Nothing was erased.
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text.lines().count(), 3, "2 runs + 1 void, nothing deleted");
        assert!(text.contains(r#""kind":"void""#), "{text}");
        assert!(text.contains("written by a determinism gate"), "{text}");

        // And a fresh reader reaches the same count from the file alone —
        // otherwise the exclusion would live only in this process.
        let reread = Registry::open(&path).expect("reopens");
        assert_eq!(reread.trials_for("fam"), 1);
        assert!(reread.is_voided(&a.run_id()));
        assert_eq!(reread.voided(), vec![a.run_id().as_str()]);
    }

    /// A verdict inherits its voided-ness from the trial it names, because it
    /// has no run_id of its own (D-0083). Two-sided: the log still shows it,
    /// the statistics view does not.
    #[test]
    fn a_voided_trials_verdict_is_still_logged_but_no_longer_counts() {
        let path = tmp("void-verdict");
        let mut r = Registry::open(&path).expect("opens");
        let k = key(0, Some(0), 7);
        r.insert_running(&row(k.clone(), "fam")).expect("insert");
        r.record_verdict(&VerdictRow {
            config_hash: k.config_hash.clone(),
            account_id: None,
            combo_index: 0,
            hypothesis_family: "fam".to_owned(),
            decided_at: Stage::S1.to_string(),
            verdict: Verdict::Kill,
            reasons: vec!["dead cost-free".to_owned()],
            trials_at_decision: 1,
            decided_on: "2026-07-30T00:00:00Z".to_owned(),
        })
        .expect("verdict");

        assert_eq!(r.verdicts().len(), 1);
        assert_eq!(r.verdicts_standing().len(), 1, "standing before the void");

        r.void(&k.run_id(), "gate", "2026-07-30T00:00:00Z")
            .expect("void");

        assert_eq!(r.verdicts().len(), 1, "the log still records what happened");
        assert!(
            r.verdicts_standing().is_empty(),
            "but statistics must not read a gate's verdict as evidence"
        );
        assert!(r.is_trial_voided(&k.config_hash, None, 0));
    }

    /// Folds of one combo are ONE trial (rule 3), so withdrawing one fold must
    /// not withdraw the combo. The third case that makes the two above agree:
    /// it says the exclusion keys on the trial, not on the row.
    #[test]
    fn voiding_one_fold_does_not_withdraw_a_combo_another_fold_still_holds() {
        let path = tmp("void-one-fold");
        let mut r = Registry::open(&path).expect("opens");

        let f0 = key(0, Some(0), 7);
        let f1 = key(0, Some(1), 7);
        r.insert_running(&row(f0.clone(), "fam"))
            .expect("insert f0");
        r.insert_running(&row(f1.clone(), "fam"))
            .expect("insert f1");
        assert_eq!(
            r.trials_for("fam"),
            1,
            "two folds of one combo are one trial"
        );

        r.void(&f0.run_id(), "gate", "2026-07-30T00:00:00Z")
            .expect("void f0");
        assert_eq!(
            r.trials_for("fam"),
            1,
            "fold 1 still stands, so the combo is still a trial"
        );

        r.void(&f1.run_id(), "gate", "2026-07-30T00:00:00Z")
            .expect("void f1");
        assert_eq!(r.trials_for("fam"), 0, "now nothing is left standing");
    }

    /// A void naming a run that was never inserted is refused. A correction
    /// that points at nothing is an assertion about a row that does not exist.
    #[test]
    fn a_void_for_an_unknown_run_is_refused() {
        let path = tmp("void-unknown");
        let mut r = Registry::open(&path).expect("opens");
        let e = r.void("deadbeefdeadbeef", "gate", "2026-07-30T00:00:00Z");
        assert!(matches!(e, Err(RegistryError::UnknownRun { .. })), "{e:?}");
    }

    /// An ephemeral registry honours the contract and never touches the disk.
    #[test]
    fn an_ephemeral_registry_writes_no_file_at_all() {
        let path = tmp("ephemeral");
        let mut r = Registry::ephemeral(&path);
        assert!(r.is_ephemeral());
        let k = key(0, Some(0), 7);
        assert_eq!(
            r.insert_running(&row(k.clone(), "fam")).expect("insert"),
            Inserted::New,
            "insert-before-run still holds in memory"
        );
        r.finish(&k, RunStatus::Done, None, "2026-07-30T00:00:00Z")
            .expect("finish");
        assert_eq!(r.trials_for("fam"), 1, "the contract is honoured in memory");
        assert!(
            !path.exists(),
            "an ephemeral registry must not create its file"
        );
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
    }

    #[test]
    fn a_trial_key_refuses_a_different_family_before_append() {
        let path = tmp("trial-family-mismatch");
        let mut registry = Registry::open(&path).expect("opens");
        let charged = key(0, Some(0), 100);
        let conflicting_fold = key(0, Some(1), 101);

        registry
            .insert_running(&row(charged, "charged-family"))
            .expect("first family owns the trial");
        let before = std::fs::read(&path).expect("read before mismatch");

        let error = registry
            .insert_running(&row(conflicting_fold.clone(), "claimed-family"))
            .expect_err("one TrialKey cannot move between families");
        assert!(
            matches!(
                error,
                RegistryError::TrialFamilyMismatch {
                    ref established_family,
                    ref claimed_family,
                    ..
                } if established_family == "charged-family" && claimed_family == "claimed-family"
            ),
            "{error:?}"
        );
        assert_eq!(
            std::fs::read(&path).expect("read after mismatch"),
            before,
            "the contradictory claim must be refused before append"
        );
        assert_eq!(registry.status_of(&conflicting_fold), None);
        assert_eq!(registry.trials_for("charged-family"), 1);
        assert_eq!(registry.trials_for("claimed-family"), 0);

        for (name, status) in [
            ("already-running", None),
            ("already-failed", Some(RunStatus::Failed)),
            ("already-done", Some(RunStatus::Done)),
        ] {
            let path = tmp(name);
            let exact_key = key(0, Some(0), 100);
            let mut registry = Registry::open(&path).expect("opens");
            registry
                .insert_running(&row(exact_key.clone(), "charged-family"))
                .expect("first family owns exact run");
            if let Some(status) = status {
                registry
                    .finish(&exact_key, status, None, "finished")
                    .expect("set shortcut status");
            }
            let before = std::fs::read(&path).expect("read before shortcut mismatch");
            assert!(
                matches!(
                    registry.insert_running(&row(exact_key, "claimed-family")),
                    Err(RegistryError::TrialFamilyMismatch { .. })
                ),
                "{name}: family validation must precede the status shortcut"
            );
            assert_eq!(
                std::fs::read(&path).expect("read after shortcut mismatch"),
                before,
                "{name}: contradictory exact-run claim must not append"
            );
        }

        let mut reread = Registry::open(&path).expect("real reader replays the binding");
        assert!(matches!(
            reread.insert_running(&row(conflicting_fold, "claimed-family")),
            Err(RegistryError::TrialFamilyMismatch { .. })
        ));

        let poisoned = tmp("historical-trial-family-mismatch");
        let mut writer = Registry::open(&poisoned).expect("opens");
        let first = key(0, Some(0), 100);
        let second = key(0, Some(1), 101);
        writer
            .insert_running(&row(first, "charged-family"))
            .expect("write first family");
        drop(writer);
        append_json_line(
            &poisoned,
            &serde_json::to_value(Line::Run(Box::new(row(second, "claimed-family"))))
                .expect("serialize conflicting historical row"),
        );
        assert!(
            matches!(
                Registry::open(&poisoned),
                Err(RegistryError::TrialFamilyMismatch { .. })
            ),
            "the real replay path must refuse a conflicting persisted row"
        );
    }

    #[test]
    fn void_decrements_the_family_that_owns_the_trial_charge() {
        let path = tmp("void-canonical-trial-family");
        let mut registry = Registry::open(&path).expect("opens");
        let fold_zero = key(0, Some(0), 100);
        let fold_one = key(0, Some(1), 101);
        let trial = fold_zero.trial();

        registry
            .insert_running(&row(fold_zero.clone(), "charged-family"))
            .expect("insert fold zero");
        registry
            .insert_running(&row(fold_one.clone(), "charged-family"))
            .expect("insert fold one");
        assert_eq!(
            registry.charged.get(&trial).map(String::as_str),
            Some("charged-family"),
            "the charge owns the one canonical family"
        );

        registry
            .void(&fold_zero.run_id(), "control", "2026-08-01T00:00:00Z")
            .expect("void fold zero");
        assert_eq!(registry.trials_for("charged-family"), 1);
        registry
            .void(&fold_one.run_id(), "control", "2026-08-01T00:00:01Z")
            .expect("void fold one");
        assert_eq!(registry.trials_for("charged-family"), 0);
        assert_eq!(registry.trials_for("some-other-family"), 0);

        let reread = Registry::open(&path).expect("reopens");
        assert_eq!(reread.trials_for("charged-family"), 0);
        assert_eq!(
            reread.charged.get(&trial).map(String::as_str),
            Some("charged-family"),
            "voiding does not erase or reassign the trial's family binding"
        );
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
                // An adequacy kill, so it is decided at `admission` — not at
                // `s0`, which is a gate no combo in this build can be decided
                // at because it is refused at load (D-0084).
                decided_at: Stage::Admission.to_string(),
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
