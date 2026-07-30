//! Raw DBN → curated Parquet.
//!
//! **Status: implemented** for bar schemas (`ohlcv-1s`, `ohlcv-1m`,
//! `ohlcv-1h`, `ohlcv-1d`). `trades`, `tbbo`, `mbo`, `definition`, and
//! `statistics` are recognised and skipped with a stated reason — they are
//! not bars, and inventing a bar-shaped reading of them here would be the
//! sort of quiet assumption this project exists to avoid.
//!
//! ## What it is driven by
//!
//! The manifest, not a directory walk. `manifest.jsonl` is the archive's
//! record of what was bought and verified (D-0017), so transcoding from it
//! means a file nobody paid for and nobody checksummed cannot become curated
//! bars by being dropped into `raw/`. It also gives every curated file the
//! `file_blake3` of its source for free, which is the manifest id (D-0014)
//! that a result has to carry (D-0013).
//!
//! ## One raw file, many instruments
//!
//! A parent-symbology pull (`ES.FUT`) resolves to every outright and calendar
//! spread that traded in the window — the January 2024 validation slice
//! contains 41 of them (D-0033). Records are interleaved by time and carry a
//! numeric `instrument_id`, not a symbol, because submissions pin
//! `stype_out=instrument_id` and `map_symbols=false`; the mapping lives in the
//! DBN header. So transcode holds one open [`PartitionWriter`] per
//! instrument, resolves each record through
//! [`Metadata::symbol_map`](databento::dbn::Metadata::symbol_map), and flushes
//! row groups as buffers fill. Live memory is bounded by (open instruments ×
//! [`ROW_GROUP_ROWS`](crate::curated::ROW_GROUP_ROWS)) regardless of how long
//! the window is — which is what makes a sixteen-year `ohlcv-1s` `Whole`
//! window transcodable at all.
//!
//! ## Every uncertainty is a refusal
//!
//! Unlike a pull, a transcode costs nothing to repeat: curated data is
//! derived, disposable, and rebuildable. So where `ingest` had to weigh a
//! refusal against stranding bytes that had already been paid for (D-0033),
//! this module simply stops. A record is refused if:
//!
//! - its `rtype` is not the one the file's schema promises (an `OhlcvMsg`
//!   decodes happily whether it is a 1-second or a 1-day bar, so the schema
//!   is the authority and the record has to agree with it);
//! - any of its OHLC fields is the vendor's `UNDEF_PRICE` sentinel — the one
//!   value that means "this field holds no price at all";
//! - its `ts_event` is not a whole multiple of the bar interval, because
//!   `avail_ts = ts_open + tf` is only meaningful for an aligned open (D-0003);
//! - its `instrument_id` has no symbol for that date, since bars nobody can
//!   name cannot be filed, replayed, or reported;
//! - it repeats or precedes the previous `ts_open` for its instrument.
//!
//! A refusal costs one re-run. A guess costs a research result nobody can
//! reproduce.
//!
//! ## A negative price is a price (D-0070)
//!
//! The list above deliberately does **not** refuse a zero or negative price,
//! and the validity predicate is `!= UNDEF_PRICE`, never `> 0`. Two
//! independent reasons, either of which is sufficient:
//!
//! - **Outrights go negative.** CL settled at **−$37.63 on 2020-04-20** — the
//!   most-studied day in the archive, and a day any serious study of crude
//!   needs. Refusing negatives refuses it.
//! - **Spread differentials are negative roughly half the time.** A calendar
//!   spread prices the *difference* between two contracts, and a market in
//!   contango prices it below zero. Over the whole archive, 103,201,649 of
//!   1,164,446,426 `ohlcv` records (8.9 %) carried a non-positive price, and
//!   every one of them was legitimate.
//!
//! Nothing downstream needs the old guarantee. [`Price`] is a signed i64 and
//! [`ContractSpec::pnl_nano_usd`] is linear in it, so accounting through a
//! negative price is the same arithmetic as through a positive one (proved by
//! `crucible-engine/tests/negative_prices.rs`, not assumed). The two places
//! that *could* have divided by a price both happen not to: `qa`'s spike
//! detector compares adjacent closes by **difference**, and `continuous`
//! back-adjusts **additively** (D-0042's reasoning, which rejected ratio
//! adjustment for unrelated determinism reasons and bought negative-price
//! safety for free).
//!
//! ## Spreads are a declared filter, not a refusal (D-0070)
//!
//! A parent-symbology window resolves to far more spreads than outrights:
//! `GC.FUT ohlcv-1m` resolves to 12,782 symbols of which 12,661 are spreads,
//! and `CL.FUT` to 7,120 of which 6,905 are. No strategy in this project
//! trades one yet, so writing them costs a curated file per spread per window
//! for data nothing reads.
//!
//! So [`TranscodeOptions::include_spreads`] excludes them — **by default, and
//! visibly**. Excluded records are counted and reported per source window
//! (`spread_records_skipped`), never silently dropped, and `raw/` keeps every
//! spread forever (D-0017). If calendar-spread research ever arrives, the flag
//! flips and the curated set is rebuilt; that is what "curated data is
//! disposable" is for.
//!
//! This is a **filter**, not a refusal. The refuse-the-whole-file rule above
//! stays reserved for genuine corruption — a record this build cannot read
//! with confidence — and a spread is not corrupt, it is uninteresting.
//!
//! ### The spread predicate, and how it was derived
//!
//! [`names_a_spread`]: **a symbol names a spread iff it contains `-`, `:`, or
//! a space.**
//!
//! Derived from the archive rather than guessed. Every symbol the manifest
//! carries — 27,099 distinct across all 41 lines, which after D-0068 is every
//! symbol the DBN headers declare — falls into exactly three buckets and no
//! others:
//!
//! | shape | count | example |
//! |---|---|---|
//! | contains `:` (and always a space too) | 5,434 | `CL:BF F0-G0-H0`, `UD:ZN: TL 0110987001` |
//! | contains `-`, and splits into exactly two alphanumeric legs | 21,044 | `RTYU7-RTYZ7`, `ESH4-ESM4` |
//! | plain `[A-Z0-9]+` | 614 | `ESH4`, `CLF10` |
//!
//! All 614 plain symbols are 4–5 characters and **all 614** match
//! `root + month code + 1–2 year digits` — the outright shape — so the marker
//! set and a positive outright test agree exactly on this archive. Zero
//! symbols fall outside the three buckets. (The seven `X.FUT` strings are
//! request keys, not resolved instruments: submissions pin
//! `stype_out=instrument_id`, so `get_for_rec` never returns one.)
//!
//! The marker set is used rather than the outright shape **because of which
//! way each one fails**. `include_spreads` defaults to false, so a symbol
//! wrongly called a spread has its bars silently omitted from the curated set
//! — the exact silent-gap failure this project exists to prevent — while a
//! symbol wrongly called an outright merely writes a partition nobody reads.
//! "Contains a marker no outright contains" errs toward writing; "fails to
//! match the outright pattern" errs toward dropping. An unrecognised future
//! shape therefore gets written, and is visible in `curated/bars/`.
//!
//! ### The better signal, and why it is not used
//!
//! `InstrumentDefMsg::instrument_class` (`InstrumentClass::FutureSpread`) is
//! the vendor's own authoritative answer, and the archive even holds the
//! `definition` schema for six of the seven roots. It is not used here because
//! it lives in a **different file**: joining it would make transcoding a bar
//! window depend on having also purchased `definition` for the same root and
//! span, and would add a cross-file join whose failure mode — a contract the
//! definition file does not mention — is silence. The symbol string travels
//! inside the very header being decoded. If `definition` coverage ever becomes
//! universal, that is the upgrade, and this doc is the note that it exists.
//!
//! [`Price`]: crucible_core::types::Price
//! [`ContractSpec::pnl_nano_usd`]: crucible_core::types::ContractSpec::pnl_nano_usd
//!
//! ## The partition key is a resolved contract, not a vendor string (D-0072)
//!
//! A curated partition is named after the contract whose bars it holds, spelled
//! with a **four-digit year**: `curated/bars/GCZ2014/1m/…`, never `GCZ4`.
//!
//! This is not cosmetic. A CME year code has one digit, so it repeats every ten
//! years, and **every bar window in this archive is sixteen years long**
//! (2010-06-06 → 2026-07-28). By pigeonhole, contracts alias — and they did:
//! `GC.FUT ohlcv-1m` wrote exactly **120 partitions = 12 month codes × 10 year
//! digits**, and `curated/bars/GCZ4/` held December-2014 gold and
//! December-2024 gold concatenated into one file spanning 14.5 years.
//!
//! **Both of this project's ordering checks were structurally blind to it.**
//! [`PartitionWriter::push`] enforces strictly increasing `ts_open` and passed,
//! because the two contracts trade in *disjoint sequential* periods and
//! concatenate in perfect order. `qa`'s gap detector reported "gaps inside
//! sessions none", because the ten-year hole falls *between* sessions, not
//! inside one. Neither could have caught it; both have controls saying so, on
//! the merged fixture, in this module's tests.
//!
//! So the key is resolved per record, against **the contract's own expiry**:
//! [`ContractCycles::resolve`] takes the vendor symbol and the bar's `ts_open`
//! and answers which cycle that bar printed in. The device is the one
//! [`expiries_from_definitions`] already used to separate the two `ESM0`s of a
//! sixteen-year `definition` file (D-0046) — one rule, now two consumers. A
//! [`DecadeAnchor`](crate::continuous::DecadeAnchor) constant is **not** used
//! and must never be: a constant has an answer for `GCZ4` and it is right for
//! half the bars.
//!
//! Expiries come from the archived `definition` file for the root, reached
//! through the catalog so the manifest vouches for the bytes
//! ([`Catalog::definition_file`]). Two- and four-digit years are absolute and
//! need no lookup — the vendor itself switches to two digits for far-dated
//! listings (`CLZ36`), and 16 such contracts trade in the CL window.
//!
//! ### The one place refuse-the-whole-file is right for a *meaning*
//!
//! A bar whose contract cannot be resolved **refuses the file**. No fallback to
//! a constant, no "best guess", no partition. That is deliberately unlike
//! D-0070's spread filter, which is a *declared filter with a count* precisely
//! because a spread is not corrupt — it is a record nothing replays yet. This
//! is different in kind: a bar filed under the wrong contract is corruption of
//! *meaning*, it looks exactly like correct data, and the silent path is what
//! produced the defect in the first place. A missing or incomplete `definition`
//! therefore fails loudly. Curated data is disposable, so a refusal costs one
//! re-run after the definition file is acquired.
//!
//! ## Timestamps
//!
//! Pass through untouched, as UTC nanoseconds. `ohlcv`'s `ts_event` marks the
//! interval START and is stored as `ts_open`; availability is `ts_open + tf`
//! and is computed by [`Bar::avail_ts`](crucible_core::events::Bar::avail_ts),
//! never here (`ingest`'s rules, CLAUDE.md §2.1).
//!
//! [`PartitionWriter`]: crate::curated::PartitionWriter
//! [`PartitionWriter::push`]: crate::curated::PartitionWriter::push
//! [`ContractCycles::resolve`]: crate::continuous::ContractCycles::resolve
//! [`expiries_from_definitions`]: crate::continuous::expiries_from_definitions
//! [`Catalog::definition_file`]: crate::catalog::Catalog::definition_file

use std::path::PathBuf;

use crucible_core::types::TimeFrame;

use crate::continuous::ContinuousError;
use crate::curated::CuratedError;

/// Vendor bar schemas this transcoder understands, and the interval each one
/// means.
///
/// Databento aggregates only at these four intervals; `5m` and `15m`
/// [`TimeFrame`]s exist for resampling that has not been built yet, and are
/// deliberately absent rather than silently mapped onto something else.
#[must_use]
pub fn timeframe_for_schema(schema: &str) -> Option<TimeFrame> {
    match schema {
        "ohlcv-1s" => Some(TimeFrame::S1),
        "ohlcv-1m" => Some(TimeFrame::M1),
        "ohlcv-1h" => Some(TimeFrame::H1),
        "ohlcv-1d" => Some(TimeFrame::D1),
        _ => None,
    }
}

/// Whether a vendor raw symbol names a **spread** — a multi-leg instrument
/// whose price is a differential — rather than a single outright contract.
///
/// The rule is one line: **a symbol names a spread iff it contains `-`, `:`,
/// or a space.** `ESH4` and `CLF10` are outrights; `RTYU7-RTYZ7`,
/// `ESH4-ESM4`, `CL:BF F0-G0-H0`, and `UD:ZN: TL 0110987001` are not.
///
/// The evidence behind the marker set, and the argument for testing for a
/// marker rather than for the outright shape, are in this module's docs. The
/// short version: this predicate gates an *exclusion*, so it is written to err
/// toward writing an unrecognised shape rather than toward dropping it.
#[must_use]
pub fn names_a_spread(symbol: &str) -> bool {
    symbol.contains(['-', ':', ' '])
}

/// What to transcode, and how insistently.
#[derive(Debug, Clone, Default)]
pub struct TranscodeOptions {
    /// Only write partitions for these instruments. `None` means every
    /// instrument the file resolves to, subject to `include_spreads`.
    pub symbols: Option<Vec<String>>,
    /// Only transcode manifest records with these ids (`file_blake3`).
    pub manifest_ids: Option<Vec<String>>,
    /// Rewrite partitions that already exist. Without this they are left
    /// alone, so re-running is cheap and idempotent.
    pub force: bool,
    /// Write partitions for spread instruments too ([`names_a_spread`]).
    ///
    /// **Defaults to false**, and the records it excludes are counted and
    /// reported rather than dropped quietly (D-0070). `raw/` keeps every
    /// spread forever and curated data is disposable, so the cost of the
    /// default being wrong is one rebuild with the flag on.
    pub include_spreads: bool,
}

/// One curated partition that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenPartition {
    /// Instrument the partition holds — the **resolved** contract key with a
    /// four-digit year (`GCZ2014`), not the vendor's one-digit spelling
    /// (D-0072).
    pub instrument: String,
    /// Where it landed.
    pub path: PathBuf,
    /// Bars written.
    pub rows: u64,
}

/// How much one source window's data a declared filter left out.
///
/// Carried per manifest record rather than per written partition because that
/// is the granularity at which it is *true*: a skipped spread produces no
/// partition to hang a number on, and the number belongs to the raw window
/// that contained it. A per-symbol table was considered and rejected — a
/// single `GC.FUT` window resolves to 12,661 spreads, so the table would be
/// the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpreadsExcluded {
    /// Bar records belonging to a spread instrument that were not written.
    pub spread_records_skipped: u64,
    /// Distinct spread instruments those records belonged to — the number of
    /// curated partitions that would have appeared with the flag on.
    pub spread_instruments_skipped: usize,
}

impl SpreadsExcluded {
    /// Whether anything was excluded at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spread_records_skipped == 0
    }
}

/// What happened to one manifest record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
    /// Bars were written.
    Transcoded {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Interval the schema resolved to.
        tf: TimeFrame,
        /// Partitions produced.
        partitions: Vec<WrittenPartition>,
        /// Partitions left alone because they already held these bytes.
        skipped: usize,
        /// Spread records the declared filter left out (D-0070).
        spreads: SpreadsExcluded,
    },
    /// Every partition already existed and `--force` was not given.
    AlreadyCurated {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// How many partitions were found in place.
        partitions: usize,
        /// Spread records the declared filter left out (D-0070). Known even
        /// here, because deciding "already curated" requires decoding the
        /// file anyway.
        spreads: SpreadsExcluded,
    },
    /// Not transcoded, and why.
    Skipped {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// The reason, phrased for an operator.
        reason: String,
    },
}

impl RecordOutcome {
    /// Archive-relative path of the record this outcome describes.
    #[must_use]
    pub fn source_file_path(&self) -> &str {
        match self {
            RecordOutcome::Transcoded {
                source_file_path, ..
            }
            | RecordOutcome::AlreadyCurated {
                source_file_path, ..
            }
            | RecordOutcome::Skipped {
                source_file_path, ..
            } => source_file_path,
        }
    }

    /// Spread records this record's decode left out. Zero for a source that
    /// was never decoded.
    #[must_use]
    pub fn spreads(&self) -> SpreadsExcluded {
        match self {
            RecordOutcome::Transcoded { spreads, .. }
            | RecordOutcome::AlreadyCurated { spreads, .. } => *spreads,
            RecordOutcome::Skipped { .. } => SpreadsExcluded::default(),
        }
    }
}

/// The result of a whole transcode run, in manifest order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscodeReport {
    /// One entry per manifest record considered.
    pub outcomes: Vec<RecordOutcome>,
    /// Whether spread instruments were written
    /// ([`TranscodeOptions::include_spreads`]).
    ///
    /// Carried on the report so its `Display` can state the mode on every
    /// run. Without it, "no spreads excluded" and "spreads not counted" print
    /// identically, and only one of them means the curated set is complete.
    pub include_spreads: bool,
}

impl TranscodeReport {
    /// Total bars written across every partition.
    #[must_use]
    pub fn rows_written(&self) -> u64 {
        self.outcomes
            .iter()
            .map(|outcome| match outcome {
                RecordOutcome::Transcoded { partitions, .. } => {
                    partitions.iter().map(|p| p.rows).sum()
                }
                _ => 0,
            })
            .sum()
    }

    /// Total partitions written.
    #[must_use]
    pub fn partitions_written(&self) -> usize {
        self.outcomes
            .iter()
            .map(|outcome| match outcome {
                RecordOutcome::Transcoded { partitions, .. } => partitions.len(),
                _ => 0,
            })
            .sum()
    }

    /// Total bar records excluded by the spread filter across every source
    /// window (D-0070).
    #[must_use]
    pub fn spread_records_skipped(&self) -> u64 {
        self.outcomes
            .iter()
            .map(|outcome| outcome.spreads().spread_records_skipped)
            .sum()
    }

    /// Total spread instruments excluded, summed per source window.
    ///
    /// A spread traded in two windows counts twice, exactly as a written
    /// partition would: one raw file fans out into one curated file per
    /// instrument (D-0036), so this is the number of partitions the flag
    /// would have added.
    #[must_use]
    pub fn spread_instruments_skipped(&self) -> usize {
        self.outcomes
            .iter()
            .map(|outcome| outcome.spreads().spread_instruments_skipped)
            .sum()
    }
}

impl core::fmt::Display for TranscodeReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.outcomes.is_empty() {
            return writeln!(f, "nothing in the manifest matched.");
        }
        // Never let a bounded run read as a complete one.
        let spreads_line = |f: &mut core::fmt::Formatter<'_>, spreads: &SpreadsExcluded| {
            if spreads.is_empty() {
                return Ok(());
            }
            writeln!(
                f,
                "       {} spread record(s) across {} instrument(s) excluded; \
                 --include-spreads to write them",
                spreads.spread_records_skipped, spreads.spread_instruments_skipped
            )
        };
        for outcome in &self.outcomes {
            match outcome {
                RecordOutcome::Transcoded {
                    source_file_path,
                    tf,
                    partitions,
                    skipped,
                    spreads,
                } => {
                    let rows: u64 = partitions.iter().map(|p| p.rows).sum();
                    writeln!(
                        f,
                        "  {source_file_path}\n    -> {} partition(s), {rows} bar(s) at {tf}",
                        partitions.len()
                    )?;
                    if *skipped > 0 {
                        writeln!(
                            f,
                            "       {skipped} partition(s) already held these bytes and were \
                             left alone; --force to rebuild"
                        )?;
                    }
                    spreads_line(f, spreads)?;
                }
                RecordOutcome::AlreadyCurated {
                    source_file_path,
                    partitions,
                    spreads,
                } => {
                    writeln!(
                        f,
                        "  {source_file_path}\n    already curated ({partitions} partition(s)); \
                         --force to rebuild"
                    )?;
                    spreads_line(f, spreads)?;
                }
                RecordOutcome::Skipped {
                    source_file_path,
                    reason,
                } => writeln!(f, "  {source_file_path}\n    skipped: {reason}")?,
            }
        }
        writeln!(
            f,
            "\n{} partition(s), {} bar(s) written.",
            self.partitions_written(),
            self.rows_written()
        )?;
        // Stated on every run, including when it is zero: "nothing excluded"
        // and "exclusions not counted" must not print the same way (D-0070).
        if self.include_spreads {
            writeln!(
                f,
                "spreads included (--include-spreads): nothing was excluded."
            )
        } else {
            writeln!(
                f,
                "spread filter on (the default): {} spread record(s) across {} \
                 instrument(s) excluded; raw/ keeps them, --include-spreads writes them.",
                self.spread_records_skipped(),
                self.spread_instruments_skipped()
            )
        }
    }
}

/// Why a transcode could not finish.
#[derive(Debug)]
pub enum TranscodeError {
    /// A curated read or write failed.
    Curated(CuratedError),
    /// The raw file could not be opened or decoded as DBN.
    Undecodable {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Explanation from the decoder.
        detail: String,
    },
    /// The DBN header carries no usable symbology, so records cannot be
    /// attributed to instruments.
    NoSymbolMap {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Explanation from the decoder.
        detail: String,
    },
    /// A record could not be trusted. Named precisely enough to find it.
    BadRecord {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Zero-based index of the record within the file.
        record_index: u64,
        /// What was wrong.
        reason: String,
    },
    /// The options ask for spread instruments by name while the spread filter
    /// is on, so the run would write nothing for them.
    ContradictorySpreadFilter {
        /// The requested symbols [`names_a_spread`] classifies as spreads.
        symbols: Vec<String>,
    },
    /// A record's contract could not be named without guessing which decade it
    /// belongs to (D-0072).
    ///
    /// The refusal that keeps two contract cycles out of one curated partition.
    /// Refusing the whole file is right here and nowhere near as broad as it
    /// sounds: curated data is derived and disposable, so it costs one re-run,
    /// while the alternative is a partition whose bars are two different
    /// contracts and whose every ordering check passes.
    UnresolvedContract {
        /// Archive-relative path of the source.
        source_file_path: String,
        /// Zero-based index of the record within the file.
        record_index: u64,
        /// Product root the definition file was looked up under.
        root: String,
        /// Why the symbol could not be resolved, from `continuous`.
        ///
        /// Boxed: this is the rarest variant of an error returned on every
        /// record path, and inlining it would widen every `Result` in the
        /// decode loop.
        source: Box<ContinuousError>,
    },
    /// A curated file for this window already exists but was produced from
    /// different raw bytes.
    SourceConflict {
        /// The curated file already in place.
        curated_path: PathBuf,
        /// Manifest id recorded in it.
        existing_blake3: String,
        /// Manifest id of the record being transcoded.
        incoming_blake3: String,
    },
}

impl core::fmt::Display for TranscodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TranscodeError::Curated(e) => write!(f, "{e}"),
            TranscodeError::Undecodable {
                source_file_path,
                detail,
            } => write!(f, "could not decode {source_file_path}: {detail}"),
            TranscodeError::NoSymbolMap {
                source_file_path,
                detail,
            } => write!(
                f,
                "{source_file_path} carries no instrument-id symbology ({detail}). \
                 Its records name instruments by number, so without the header's \
                 mapping there is no way to say whose bars these are"
            ),
            TranscodeError::BadRecord {
                source_file_path,
                record_index,
                reason,
            } => write!(
                f,
                "{source_file_path} record #{record_index}: {reason}. Refusing to \
                 curate a file this build cannot read with confidence — nothing \
                 was written, and re-running after the cause is understood costs \
                 nothing"
            ),
            TranscodeError::ContradictorySpreadFilter { symbols } => write!(
                f,
                "these requested symbols name spreads: {}. The spread filter is \
                 on (its default), so the run would decode every record and \
                 write nothing — an empty result in the shape of a finished \
                 one. Pass --include-spreads, or drop them from --symbols",
                symbols.join(", ")
            ),
            TranscodeError::UnresolvedContract {
                source_file_path,
                record_index,
                root,
                source,
            } => write!(
                f,
                "{source_file_path} record #{record_index}: {source}\n       \
                 A curated partition is named after the contract whose bars it \
                 holds, and a one-digit CME year code repeats every ten years \
                 while this window is sixteen years long — so a key that cannot \
                 be resolved would merge two contracts into one file that passes \
                 every ordering check (D-0072). Nothing was written. Check that \
                 the `definition` schema for {root} covers this window:\n       \
                 \x20     crucible coverage --schema definition"
            ),
            TranscodeError::SourceConflict {
                curated_path,
                existing_blake3,
                incoming_blake3,
            } => write!(
                f,
                "{} was transcoded from manifest id {existing_blake3} but this \
                 record is {incoming_blake3}. Two raw windows computing one \
                 curated path is a naming bug, not something to overwrite",
                curated_path.display()
            ),
        }
    }
}

impl std::error::Error for TranscodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TranscodeError::Curated(e) => Some(e),
            TranscodeError::UnresolvedContract { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<CuratedError> for TranscodeError {
    fn from(e: CuratedError) -> TranscodeError {
        TranscodeError::Curated(e)
    }
}

#[cfg(feature = "databento")]
pub use imp::transcode;

#[cfg(feature = "databento")]
mod imp {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};
    use databento::dbn::decode::{DbnMetadata, DecodeRecord, dbn::Decoder};
    use databento::dbn::{RType, Record, SymbolIndex, UNDEF_PRICE, record::OhlcvMsg};

    use crate::catalog::{Catalog, ManifestRecord};
    use crate::continuous::{
        ContractCycles, ContractSymbol, DecadeAnchor, contract_cycles_from_definitions, parse_parts,
    };
    use crate::curated::path::window_stem_of;
    use crate::curated::{CuratedMeta, PartitionSource, PartitionWriter, read_meta};

    use super::{
        RecordOutcome, SpreadsExcluded, TranscodeError, TranscodeOptions, TranscodeReport,
        WrittenPartition, names_a_spread, timeframe_for_schema,
    };

    /// A vendor spelling's resolved partition key, and the window over which
    /// that answer holds.
    ///
    /// A sixteen-year window resolves the same handful of spellings tens of
    /// millions of times, and only at a decade boundary does the answer change.
    /// Remembering the window rather than the answer alone is what makes the
    /// reuse *provably* safe: the cached key is used only for a timestamp the
    /// resolver itself said it covers.
    struct ResolvedKey {
        /// Canonical four-digit contract key.
        key: String,
        /// Exclusive lower bound of the window this key holds over.
        opens: Ts,
        /// Inclusive upper bound.
        closes: Ts,
    }

    /// Contract cycles per product root, loaded once per run.
    ///
    /// A `definition` file for a sixteen-year parent key is tens of thousands
    /// of records, and every bar window of that root needs the same answer from
    /// it, so it is read once and shared. `BTreeMap` rather than `HashMap` for
    /// CLAUDE.md §2.2 — nothing here reaches a result through iteration order
    /// today, and an unordered container on a result path is a habit worth not
    /// having.
    #[derive(Default)]
    struct CycleCache {
        by_root: BTreeMap<String, ContractCycles>,
    }

    impl CycleCache {
        /// The cycles for `root`, reading the archive's `definition` file the
        /// first time it is asked.
        ///
        /// A root with no archived `definition` gets an **empty** resolver
        /// rather than an error: an empty one answers every two- and four-digit
        /// spelling correctly and refuses every one-digit one, which is exactly
        /// the truth of the situation. Failing here instead would refuse files
        /// whose symbols never needed a lookup.
        fn cycles(
            &mut self,
            catalog: &Catalog,
            root: &str,
        ) -> Result<&ContractCycles, crate::continuous::ContinuousError> {
            if !self.by_root.contains_key(root) {
                let cycles = match catalog.definition_file(root) {
                    Some(path) => {
                        contract_cycles_from_definitions(&path, root, DecadeAnchor::DEFAULT)?
                    }
                    None => ContractCycles::default(),
                };
                self.by_root.insert(root.to_owned(), cycles);
            }
            Ok(self
                .by_root
                .get(root)
                .expect("INVARIANT: just inserted under this key"))
        }
    }

    /// The DBN record type an interval's bars must carry.
    fn rtype_for(tf: TimeFrame) -> Option<RType> {
        match tf {
            TimeFrame::S1 => Some(RType::Ohlcv1S),
            TimeFrame::M1 => Some(RType::Ohlcv1M),
            TimeFrame::H1 => Some(RType::Ohlcv1H),
            TimeFrame::D1 => Some(RType::Ohlcv1D),
            TimeFrame::M5 | TimeFrame::M15 => None,
        }
    }

    /// Makes sure `keys` holds a partition key covering `ts` for `symbol`.
    ///
    /// The one place a vendor spelling becomes a curated partition key
    /// (D-0072), and the one place that can refuse a file for not knowing which
    /// contract a bar belongs to.
    fn ensure_key(
        catalog: &Catalog,
        cycles: &mut CycleCache,
        keys: &mut BTreeMap<String, ResolvedKey>,
        record: &ManifestRecord,
        symbol: &str,
        ts: Ts,
        index: u64,
    ) -> Result<(), TranscodeError> {
        if let Some(cached) = keys.get(symbol)
            && ts > cached.opens
            && ts <= cached.closes
        {
            return Ok(());
        }
        // A symbol that is not an outright contract keeps the vendor's
        // spelling, and that is D-0070's rule unchanged: this predicate gates a
        // *rename*, not an exclusion, so an unrecognised shape must still be
        // written and visible rather than refused. It costs nothing here —
        // aliasing is a property of the one-digit CME year code, and a string
        // that does not parse as a contract does not have one. Every spread
        // lands here (`ESH4-ESM4`, `CL:BF F0-G0-H0`), which is also why a
        // spread partition is still named the vendor's way; nothing replays one
        // (D-0070), and making them replayable means resolving their legs
        // first.
        let Ok(root) = parse_parts(symbol).map(|parts| parts.root.to_owned()) else {
            keys.insert(
                symbol.to_owned(),
                ResolvedKey {
                    key: symbol.to_owned(),
                    opens: Ts(i64::MIN),
                    closes: Ts(i64::MAX),
                },
            );
            return Ok(());
        };
        let unresolved = |source| TranscodeError::UnresolvedContract {
            source_file_path: record.file_path.clone(),
            record_index: index,
            root: root.clone(),
            source: Box::new(source),
        };
        let cycles = cycles.cycles(catalog, &root).map_err(unresolved)?;
        let (resolved, opens, closes) = cycles.resolve_window(symbol, ts).map_err(unresolved)?;
        keys.insert(
            symbol.to_owned(),
            ResolvedKey {
                key: contract_key(&resolved),
                opens,
                closes,
            },
        );
        Ok(())
    }

    /// The curated partition key of a resolved contract: its canonical
    /// four-digit spelling (D-0072).
    fn contract_key(symbol: &ContractSymbol) -> String {
        symbol.to_string()
    }

    /// Transcodes every manifest record matching `opts` into curated Parquet.
    ///
    /// # Errors
    /// [`TranscodeError`] on an undecodable file, a record that cannot be
    /// trusted, or a curated write failure. Nothing partial is published: a
    /// failed record leaves its `.parquet.tmp` files removed and its previous
    /// curated output untouched.
    pub fn transcode(
        catalog: &Catalog,
        opts: &TranscodeOptions,
    ) -> Result<TranscodeReport, TranscodeError> {
        // Refused up front rather than answered with an empty report: asking
        // for a spread by name while the filter that excludes it is on is a
        // contradiction, and running it decodes the whole archive to write
        // nothing.
        if !opts.include_spreads
            && let Some(requested) = &opts.symbols
        {
            let spreads: Vec<String> = requested
                .iter()
                .filter(|s| names_a_spread(s))
                .cloned()
                .collect();
            if !spreads.is_empty() {
                return Err(TranscodeError::ContradictorySpreadFilter { symbols: spreads });
            }
        }

        let mut report = TranscodeReport {
            outcomes: Vec::new(),
            include_spreads: opts.include_spreads,
        };
        let mut cycles = CycleCache::default();
        for record in catalog.records() {
            if let Some(ids) = &opts.manifest_ids
                && !ids.contains(&record.file_blake3)
            {
                continue;
            }
            let Some(tf) = timeframe_for_schema(&record.schema) else {
                report.outcomes.push(RecordOutcome::Skipped {
                    source_file_path: record.file_path.clone(),
                    reason: format!(
                        "schema `{}` is not a bar schema; transcode handles \
                         ohlcv-1s/1m/1h/1d",
                        record.schema
                    ),
                });
                continue;
            };
            report
                .outcomes
                .push(transcode_record(catalog, &mut cycles, record, tf, opts)?);
        }
        Ok(report)
    }

    /// Transcodes one raw file into one curated partition per contract.
    fn transcode_record(
        catalog: &Catalog,
        cycles: &mut CycleCache,
        record: &ManifestRecord,
        tf: TimeFrame,
        opts: &TranscodeOptions,
    ) -> Result<RecordOutcome, TranscodeError> {
        let data_dir = catalog.data_dir();
        let Some(window_stem) = window_stem_of(&record.file_path) else {
            return Ok(RecordOutcome::Skipped {
                source_file_path: record.file_path.clone(),
                reason: "the archive path has no window stem to name a partition after".to_owned(),
            });
        };
        let raw_path = data_dir.join(&record.file_path);
        let undecodable = |detail: String| TranscodeError::Undecodable {
            source_file_path: record.file_path.clone(),
            detail,
        };

        let mut decoder =
            Decoder::from_zstd_file(&raw_path).map_err(|e| undecodable(e.to_string()))?;
        let symbol_map =
            decoder
                .metadata()
                .symbol_map()
                .map_err(|e| TranscodeError::NoSymbolMap {
                    source_file_path: record.file_path.clone(),
                    detail: e.to_string(),
                })?;

        let expected_rtype = rtype_for(tf).ok_or_else(|| TranscodeError::Undecodable {
            source_file_path: record.file_path.clone(),
            detail: format!("{tf} bars are not a vendor schema"),
        })?;
        let interval_ns = tf.duration_ns();

        // BTreeMap, not HashMap: the report is part of the output, and
        // iteration order must not depend on hashing (CLAUDE.md §2.2).
        let mut writers: BTreeMap<String, PartitionWriter> = BTreeMap::new();
        let mut index: u64 = 0;
        let bad = |reason: String, index: u64| TranscodeError::BadRecord {
            source_file_path: record.file_path.clone(),
            record_index: index,
            reason,
        };

        // Which instruments are already curated is decided per partition, at
        // the moment the first bar for one shows up. It cannot be decided up
        // front from the header: the symbology maps every contract the parent
        // key *resolves to*, and most of them never trade a bar in the window
        // — the January 2024 ES.FUT slice maps 41 and produces 16. Treating
        // the mapped set as the expected set makes a completed transcode look
        // permanently unfinished.
        //
        // A `BTreeSet`, not a `Vec`, because this is consulted once per bar
        // record and a big window holds hundreds of millions of them: the
        // linear scan it replaces cost O(|already|) per record, so a *no-op
        // re-run* — the case where `already` is at its fullest — was slower
        // than the transcode it was skipping (D-0070). Ordered rather than
        // hashed for CLAUDE.md §2.2; only its length reaches the report, but
        // an unordered container in a result path is a habit worth not having.
        let mut already: BTreeSet<String> = BTreeSet::new();

        // Spread records are excluded by a declared filter, and counted so the
        // exclusion is visible (D-0070). Counted independently of `--symbols`,
        // so the number describes the source window rather than the request.
        let mut spread_records_skipped: u64 = 0;
        let mut spread_instruments: BTreeSet<String> = BTreeSet::new();

        // Vendor spelling -> resolved partition key, with the window the answer
        // holds over (D-0072). Populated on first sight of each contract.
        let mut keys: BTreeMap<String, ResolvedKey> = BTreeMap::new();

        while let Some(msg) = decoder
            .decode_record::<OhlcvMsg>()
            .map_err(|e| undecodable(e.to_string()))?
        {
            let header = msg.header();
            if header.rtype != expected_rtype as u8 {
                return Err(bad(
                    format!(
                        "rtype {} is not the {} this file's schema promises",
                        header.rtype, expected_rtype as u8
                    ),
                    index,
                ));
            }
            let ts_event = i64::try_from(header.ts_event)
                .map_err(|_| bad(format!("ts_event {} exceeds i64", header.ts_event), index))?;
            if ts_event % interval_ns != 0 {
                return Err(bad(
                    format!(
                        "ts_event {ts_event} is not a whole multiple of the {tf} interval, \
                         so `avail_ts = ts_open + tf` would not describe when this bar \
                         became knowable"
                    ),
                    index,
                ));
            }
            // The validity predicate is `!= UNDEF_PRICE`, and deliberately not
            // `> 0`: CL settled at −$37.63 on 2020-04-20 as an outright, and a
            // calendar spread's differential is negative whenever the market
            // is in contango. Refusing those refused 8.9 % of the archive
            // (D-0070). `UNDEF_PRICE` is the only value that means "no price".
            for (name, value) in [
                ("open", msg.open),
                ("high", msg.high),
                ("low", msg.low),
                ("close", msg.close),
            ] {
                if value == UNDEF_PRICE {
                    return Err(bad(format!("{name} is the UNDEF_PRICE sentinel"), index));
                }
            }
            let symbol = symbol_map.get_for_rec(msg).ok_or_else(|| {
                bad(
                    format!(
                        "instrument_id {} has no symbol on this date in the file's own \
                         header mappings",
                        header.instrument_id
                    ),
                    index,
                )
            })?;

            if names_a_spread(symbol.as_str()) && !opts.include_spreads {
                spread_records_skipped += 1;
                if !spread_instruments.contains(symbol.as_str()) {
                    spread_instruments.insert(symbol.clone());
                }
            } else {
                // The partition key is the CONTRACT, resolved against its own
                // expiry — not the vendor's one-digit spelling, which repeats
                // every ten years inside a sixteen-year window (D-0072).
                // Resolved once per (symbol, key) pair and then remembered:
                // a symbol's cycle can change within one file, but only where
                // the file spans a decade boundary, so the cache is keyed on
                // both the spelling and the resolved answer.
                ensure_key(
                    catalog,
                    cycles,
                    &mut keys,
                    record,
                    symbol.as_str(),
                    Ts(ts_event),
                    index,
                )?;
                let key = &keys
                    .get(symbol.as_str())
                    .expect("INVARIANT: ensure_key inserted it or returned Err")
                    .key;
                let wanted = opts.symbols.as_ref().is_none_or(|filter| {
                    // Either spelling selects the contract: `--symbols ESH4`
                    // and `--symbols ESH2024` both name it. The first is what
                    // an operator reads off the vendor's window and may select
                    // more than one contract; the report names every partition
                    // written, so the breadth is visible rather than assumed.
                    filter
                        .iter()
                        .any(|s| s.as_str() == symbol.as_str() || s.as_str() == key.as_str())
                });
                if wanted && !already.contains(key.as_str()) {
                    let writer = match writers.get_mut(key.as_str()) {
                        Some(writer) => Some(writer),
                        None => {
                            let source = PartitionSource {
                                instrument: InstrumentId::new(key.as_str()),
                                tf,
                                dataset: record.dataset.clone(),
                                vendor_schema: record.schema.clone(),
                                source_file_path: record.file_path.clone(),
                                source_file_blake3: record.file_blake3.clone(),
                            };
                            // Existing output from *these* bytes is left alone
                            // unless asked; from different bytes it is a refusal.
                            match existing_partition(data_dir, &source, window_stem, record)? {
                                Some(()) if !opts.force => {
                                    already.insert(key.clone());
                                    None
                                }
                                _ => Some(writers.entry(key.clone()).or_insert(
                                    PartitionWriter::create(data_dir, source, window_stem)?,
                                )),
                            }
                        }
                    };
                    if let Some(writer) = writer {
                        writer.push(
                            Ts(ts_event),
                            Price::from_nanos(msg.open),
                            Price::from_nanos(msg.high),
                            Price::from_nanos(msg.low),
                            Price::from_nanos(msg.close),
                            msg.volume,
                        )?;
                    }
                }
            }
            index += 1;
        }

        let mut partitions = Vec::with_capacity(writers.len());
        for (instrument, mut writer) in writers {
            if let Some(meta) = writer.finish()? {
                partitions.push(WrittenPartition {
                    instrument,
                    path: writer.final_path().to_path_buf(),
                    rows: meta.row_count,
                });
            }
        }

        let spreads = SpreadsExcluded {
            spread_records_skipped,
            spread_instruments_skipped: spread_instruments.len(),
        };
        if partitions.is_empty() && !already.is_empty() {
            return Ok(RecordOutcome::AlreadyCurated {
                source_file_path: record.file_path.clone(),
                partitions: already.len(),
                spreads,
            });
        }
        Ok(RecordOutcome::Transcoded {
            source_file_path: record.file_path.clone(),
            tf,
            partitions,
            skipped: already.len(),
            spreads,
        })
    }

    /// Reads an existing partition, if any, and refuses when it came from a
    /// different raw file.
    fn existing_meta(
        data_dir: &Path,
        instrument: &str,
        tf: TimeFrame,
        window_stem: &str,
    ) -> Result<Option<CuratedMeta>, TranscodeError> {
        let path = crate::curated::path::partition_file(
            data_dir,
            &InstrumentId::new(instrument),
            tf,
            window_stem,
        )?;
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(read_meta(&path)?))
    }

    /// Reports whether a partition for this instrument and window already
    /// exists, refusing when the one on disk came from different raw bytes.
    fn existing_partition(
        data_dir: &Path,
        source: &PartitionSource,
        window_stem: &str,
        record: &ManifestRecord,
    ) -> Result<Option<()>, TranscodeError> {
        let Some(meta) =
            existing_meta(data_dir, source.instrument.as_str(), source.tf, window_stem)?
        else {
            return Ok(None);
        };
        if meta.source.source_file_blake3 != record.file_blake3 {
            let curated_path = crate::curated::path::partition_file(
                data_dir,
                &source.instrument,
                source.tf,
                window_stem,
            )?;
            return Err(TranscodeError::SourceConflict {
                curated_path,
                existing_blake3: meta.source.source_file_blake3,
                incoming_blake3: record.file_blake3.clone(),
            });
        }
        Ok(Some(()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use std::fs::File;
        use std::num::NonZeroU64;

        use crate::SyntheticFeed;
        use crate::catalog::{Acquisition, TsRange};
        use crate::curated::ParquetBarFeed;
        use crate::testutil::TempDir;
        use crucible_core::events::MarketEvent;
        use crucible_core::traits::Feed;
        use databento::dbn::encode::EncodeRecord;
        use databento::dbn::encode::dbn::Encoder;
        use databento::dbn::{
            MappingInterval, Metadata, RecordHeader, SType, Schema, SymbolMapping,
        };

        /// 2024-01-01T00:00:00Z, and minute-aligned — which every `ohlcv-1m`
        /// `ts_event` from the vendor is, and which transcode insists on.
        const JAN1: i64 = 1_704_067_200_000_000_000;
        const FEB1: i64 = 1_706_745_600_000_000_000;
        /// 2020-04-20T00:00:00Z — the session WTI settled below zero on.
        /// 1_587_340_800 = 1_577_836_800 (2020-01-01) + 110 whole days
        /// (31 January + 29 February + 31 March + 19 April), × 1e9 for nanos.
        const APR20: i64 = 1_587_340_800_000_000_000;
        const APR21: i64 = 1_587_427_200_000_000_000;
        const MIN: i64 = 60_000_000_000;
        const RAW_PATH: &str = "raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst";

        /// One bar to encode: instrument id, ts_event, open/high/low/close, volume.
        #[derive(Clone, Copy)]
        struct Row(u32, i64, i64, i64, i64, i64, u64);

        /// The span a fixture file covers: DBN metadata endpoints plus the
        /// mapping interval its symbology is valid over. `get_for_rec` resolves
        /// by the record's own date, so a record outside the interval has no
        /// symbol — which is a different refusal than the one under test.
        #[derive(Clone, Copy)]
        struct Window {
            start_ns: i64,
            end_ns: i64,
            first_day: (i32, time::Month, u8),
            last_day: (i32, time::Month, u8),
        }

        const JAN_2024: Window = Window {
            start_ns: JAN1,
            end_ns: FEB1,
            first_day: (2024, time::Month::January, 1),
            last_day: (2024, time::Month::January, 31),
        };
        const APR_2020: Window = Window {
            start_ns: APR20,
            end_ns: APR21,
            first_day: (2020, time::Month::April, 20),
            last_day: (2020, time::Month::April, 21),
        };
        /// The span a `definition` record covers — the real archive's, so the
        /// fixtures exercise the shape the join actually has.
        /// 2010-06-06 is epoch day 14,766; 2026-07-28 is 20,663.
        const DEFINITION_SPAN: Window = Window {
            start_ns: 14_766 * 86_400_000_000_000,
            end_ns: 20_663 * 86_400_000_000_000,
            first_day: (2010, time::Month::June, 6),
            last_day: (2026, time::Month::July, 28),
        };

        fn date_of(day: (i32, time::Month, u8)) -> time::Date {
            time::Date::from_calendar_date(day.0, day.1, day.2).expect("a real calendar date")
        }

        /// Writes a `.dbn.zst` over [`JAN_2024`].
        fn write_dbn(path: &Path, symbols: &[(u32, &str)], rows: &[Row], rtype: u8) {
            write_dbn_in(path, symbols, rows, rtype, JAN_2024);
        }

        /// Writes a `.dbn.zst` whose header maps ids to symbols exactly the way
        /// a real parent-symbology delivery does: `stype_out=instrument_id`, so
        /// `raw_symbol` is the contract and the interval's `symbol` is the id.
        #[expect(
            clippy::cast_sign_loss,
            reason = "fixture timestamps are positive by construction"
        )]
        fn write_dbn_in(
            path: &Path,
            symbols: &[(u32, &str)],
            rows: &[Row],
            rtype: u8,
            window: Window,
        ) {
            std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
            let mappings: Vec<SymbolMapping> = symbols
                .iter()
                .map(|(id, symbol)| SymbolMapping {
                    raw_symbol: (*symbol).to_owned(),
                    intervals: vec![MappingInterval {
                        start_date: date_of(window.first_day),
                        end_date: date_of(window.last_day),
                        symbol: id.to_string(),
                    }],
                })
                .collect();
            let metadata = Metadata::builder()
                .dataset("GLBX.MDP3".to_owned())
                .schema(Some(Schema::Ohlcv1M))
                .start(window.start_ns as u64)
                .end(NonZeroU64::new(window.end_ns as u64))
                .stype_in(Some(SType::Parent))
                .stype_out(SType::InstrumentId)
                .symbols(vec!["ES.FUT".to_owned()])
                .mappings(mappings)
                .build();

            let file = File::create(path).expect("create dbn");
            let mut encoder = Encoder::with_zstd(file, &metadata).expect("encoder");
            for row in rows {
                let msg = OhlcvMsg {
                    hd: RecordHeader::new::<OhlcvMsg>(rtype, 1, row.0, row.1 as u64),
                    open: row.2,
                    high: row.3,
                    low: row.4,
                    close: row.5,
                    volume: row.6,
                };
                encoder.encode_record(&msg).expect("encode");
            }
            drop(encoder);
        }

        /// A catalog holding exactly one record for the file just written.
        fn catalog_for(dir: &TempDir, schema: &str, rel_path: &str) -> Catalog {
            catalog_for_in(dir, schema, rel_path, JAN_2024)
        }

        fn catalog_for_in(dir: &TempDir, schema: &str, rel_path: &str, window: Window) -> Catalog {
            let mut catalog = Catalog::open(dir.path()).expect("open catalog");
            append_record(&mut catalog, schema, rel_path, window, "ES.FUT");
            catalog
        }

        fn append_record(
            catalog: &mut Catalog,
            schema: &str,
            rel_path: &str,
            window: Window,
            parent: &str,
        ) {
            catalog
                .append(Acquisition {
                    dataset: "GLBX.MDP3".to_owned(),
                    schema: schema.to_owned(),
                    symbols: vec![parent.to_owned()],
                    range: TsRange::new(Ts(window.start_ns), Ts(window.end_ns))
                        .expect("valid range"),
                    acquired_ts: Ts(window.start_ns),
                    databento_job_id: format!("TEST-JOB-{schema}-{parent}"),
                    file_path: rel_path.to_owned(),
                })
                .expect("append");
        }

        /// Plants the `definition` file a real archive holds for every root the
        /// fixture's outright symbols name, and records it in the manifest.
        ///
        /// Not scaffolding: without it no bar window transcodes at all, because
        /// a one-digit CME year code cannot say which decade it means (D-0072).
        /// Expiries are the nominal third Friday of the delivery month — the
        /// fallback [`nominal_expiry`](crate::continuous::nominal_expiry) exists
        /// for exactly this, and every fixture bar here sits well inside the
        /// resulting cycle.
        fn plant_definitions(dir: &TempDir, catalog: &mut Catalog, symbols: &[(u32, &str)]) {
            use crate::continuous::expiry::imp::tests::write_definitions;
            use crate::continuous::{ContractSymbol, nominal_expiry};
            use databento::dbn::InstrumentClass;

            let mut by_root: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
            for (_, symbol) in symbols {
                let Ok(parsed) = ContractSymbol::parse(symbol) else {
                    continue; // a spread: it carries no outright expiry
                };
                let expiry = u64::try_from(nominal_expiry(&parsed).0).expect("after the epoch");
                by_root
                    .entry(parsed.root().to_owned())
                    .or_default()
                    .push(((*symbol).to_owned(), expiry));
            }
            for (root, contracts) in by_root {
                let parent = format!("{root}.FUT");
                let rel =
                    format!("raw/GLBX.MDP3/definition/{parent}/2010-06-06--2026-07-28.dbn.zst");
                let rows: Vec<(&str, InstrumentClass, u64)> = contracts
                    .iter()
                    .map(|(symbol, expiry)| (symbol.as_str(), InstrumentClass::Future, *expiry))
                    .collect();
                write_definitions(&dir.path().join(&rel), &rows);
                append_record(catalog, "definition", &rel, DEFINITION_SPAN, &parent);
            }
        }

        fn ohlcv_1m_rtype() -> u8 {
            RType::Ohlcv1M as u8
        }

        fn one_bar(id: u32, minute: i64) -> Row {
            #[expect(
                clippy::cast_sign_loss,
                reason = "minute indices in fixtures are small and non-negative"
            )]
            let volume = 100 + minute as u64;
            Row(
                id,
                JAN1 + minute * MIN,
                5_000_000_000_000,
                5_001_000_000_000,
                4_999_000_000_000,
                5_000_500_000_000,
                volume,
            )
        }

        fn setup(symbols: &[(u32, &str)], rows: &[Row], rtype: u8) -> (TempDir, Catalog) {
            let dir = TempDir::new();
            write_dbn(&dir.path().join(RAW_PATH), symbols, rows, rtype);
            let mut catalog = Catalog::open(dir.path()).expect("open catalog");
            plant_definitions(&dir, &mut catalog, symbols);
            append_record(&mut catalog, "ohlcv-1m", RAW_PATH, JAN_2024, "ES.FUT");
            (dir, catalog)
        }

        /// The bar-window outcomes of a report, with the `definition` records'
        /// "not a bar schema" skips filtered out.
        ///
        /// Every fixture archive now holds a `definition` file (it has to —
        /// D-0072), and `transcode` reports one skip per non-bar record. That
        /// is correct and uninteresting here.
        fn bar_outcomes(report: &TranscodeReport) -> Vec<&RecordOutcome> {
            report
                .outcomes
                .iter()
                .filter(|outcome| !outcome.source_file_path().contains("/definition/"))
                .collect()
        }

        fn bars_of(dir: &TempDir, symbol: &str) -> Vec<MarketEvent> {
            let mut feed =
                ParquetBarFeed::open(dir.path(), &InstrumentId::new(symbol), TimeFrame::M1, None)
                    .expect("open the curated partition");
            std::iter::from_fn(|| feed.next_event()).collect()
        }

        /// No curated `.parquet` anywhere under the data dir.
        fn nothing_published(dir: &TempDir) -> bool {
            fn walk(path: &Path) -> bool {
                let Ok(entries) = std::fs::read_dir(path) else {
                    return true;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if !walk(&path) {
                            return false;
                        }
                    } else if path.extension().and_then(|e| e.to_str()) == Some("parquet") {
                        return false;
                    }
                }
                true
            }
            walk(&dir.path().join("curated"))
        }

        // -------------------------------------------------------- the golden

        // Synthetic bars through a real DBN encode, a real transcode, and a
        // real Parquet read must come back byte-identical. No vendor data is
        // involved, so this runs in CI (testdata/README.md rule 1).
        //
        // Timestamps are re-stamped onto a minute-aligned grid: SyntheticFeed's
        // fixed base is 2023-11-14T22:13:20Z, twenty seconds off a minute
        // boundary, and transcode refuses an unaligned `ts_event` because
        // `avail_ts = ts_open + tf` would not describe when such a bar became
        // knowable. The prices — the part Parquet has to carry losslessly —
        // are the generator's own.
        #[test]
        fn synthetic_bars_survive_dbn_transcode_and_parquet_bit_identically() {
            let mut generator = SyntheticFeed::random_walk(
                42,
                240,
                TimeFrame::M1,
                Price::from_points(5000),
                Price::from_points_f64_lossy(0.25),
                4,
            );
            let generated: Vec<MarketEvent> =
                std::iter::from_fn(|| generator.next_event()).collect();

            let rows: Vec<Row> = generated
                .iter()
                .enumerate()
                .map(|(i, MarketEvent::Bar(bar))| {
                    Row(
                        7001,
                        JAN1 + i as i64 * MIN,
                        bar.open.as_nanos(),
                        bar.high.as_nanos(),
                        bar.low.as_nanos(),
                        bar.close.as_nanos(),
                        bar.volume,
                    )
                })
                .collect();
            let expected: Vec<MarketEvent> = rows
                .iter()
                .map(|row| {
                    MarketEvent::Bar(crucible_core::events::Bar {
                        instrument: InstrumentId::new("ESH2024"),
                        tf: TimeFrame::M1,
                        ts_open: Ts(row.1),
                        open: Price::from_nanos(row.2),
                        high: Price::from_nanos(row.3),
                        low: Price::from_nanos(row.4),
                        close: Price::from_nanos(row.5),
                        volume: row.6,
                        signal_offset: Price::ZERO,
                    })
                })
                .collect();

            let (dir, catalog) = setup(&[(7001, "ESH4")], &rows, ohlcv_1m_rtype());
            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.partitions_written(), 1);
            assert_eq!(report.rows_written(), 240);
            assert_eq!(bars_of(&dir, "ESH2024"), expected);
        }

        #[test]
        fn one_raw_file_fans_out_into_one_partition_per_instrument() {
            let rows = vec![
                one_bar(1, 0),
                one_bar(2, 0),
                one_bar(1, 1),
                one_bar(2, 1),
                one_bar(2, 2),
            ];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESM4")], &rows, ohlcv_1m_rtype());
            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.partitions_written(), 2);
            assert_eq!(report.rows_written(), 5);
            assert_eq!(bars_of(&dir, "ESH2024").len(), 2);
            assert_eq!(bars_of(&dir, "ESM2024").len(), 3);
        }

        #[test]
        fn a_symbol_filter_writes_only_what_was_asked_for() {
            let rows = vec![one_bar(1, 0), one_bar(2, 0)];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESM4")], &rows, ohlcv_1m_rtype());
            let opts = TranscodeOptions {
                symbols: Some(vec!["ESH4".to_owned()]),
                ..TranscodeOptions::default()
            };
            let report = transcode(&catalog, &opts).expect("transcode");
            assert_eq!(report.partitions_written(), 1);
            assert_eq!(bars_of(&dir, "ESH2024").len(), 1);
            assert!(
                ParquetBarFeed::open(
                    dir.path(),
                    &InstrumentId::new("ESM2024"),
                    TimeFrame::M1,
                    None
                )
                .is_err()
            );
        }

        // ------------------------------------------- negative prices (D-0070)

        /// The CL 2020-04-20 fixture: one positive minute, one that touches
        /// zero, and the minute holding the session's negative settle. Values
        /// are exact nanopoints, so a Parquet round trip either reproduces
        /// them bit-for-bit or fails.
        ///
        /// Instrument `CLK0` is the May-2020 WTI contract, which is the one
        /// that settled at −$37.63. Prices are in points, where one point is
        /// one dollar per barrel, so −37.63 is −37_630_000_000 nanopoints.
        fn cl_2020_04_20_rows() -> Vec<Row> {
            vec![
                // 00:00 — still positive, the day before the collapse finished.
                Row(
                    1,
                    APR20,
                    11_000_000_000,
                    11_000_000_000,
                    10_980_000_000,
                    10_990_000_000,
                    812,
                ),
                // 00:01 — through zero. A close of exactly 0 is a legal price,
                // and the old `value <= 0` rule refused the whole file for it.
                Row(
                    1,
                    APR20 + MIN,
                    10_000_000,
                    10_000_000,
                    -10_000_000,
                    0,
                    1_337,
                ),
                // 00:02 — the low and the settle. −40.32 and −37.63.
                Row(
                    1,
                    APR20 + 2 * MIN,
                    -1_430_000_000,
                    1_000_000_000,
                    -40_320_000_000,
                    -37_630_000_000,
                    4_051,
                ),
            ]
        }

        /// (a) of the planted fixture: a negative-price window survives
        /// transcode and reads back **bit-identically** from Parquet.
        ///
        /// Under the predicate this commit replaces, this file was refused at
        /// record #1 and nothing was published at all.
        #[test]
        fn the_cl_2020_04_20_negative_price_window_round_trips_bit_identically() {
            let dir = TempDir::new();
            let rel = "raw/GLBX.MDP3/ohlcv-1m/CL.FUT/2020-04.dbn.zst";
            let rows = cl_2020_04_20_rows();
            write_dbn_in(
                &dir.path().join(rel),
                &[(1, "CLK0")],
                &rows,
                ohlcv_1m_rtype(),
                APR_2020,
            );
            let mut catalog = Catalog::open(dir.path()).expect("open catalog");
            plant_definitions(&dir, &mut catalog, &[(1, "CLK0")]);
            append_record(&mut catalog, "ohlcv-1m", rel, APR_2020, "CL.FUT");

            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.partitions_written(), 1);
            assert_eq!(report.rows_written(), 3);

            let expected: Vec<MarketEvent> = rows
                .iter()
                .map(|row| {
                    MarketEvent::Bar(crucible_core::events::Bar {
                        instrument: InstrumentId::new("CLK2020"),
                        tf: TimeFrame::M1,
                        ts_open: Ts(row.1),
                        open: Price::from_nanos(row.2),
                        high: Price::from_nanos(row.3),
                        low: Price::from_nanos(row.4),
                        close: Price::from_nanos(row.5),
                        volume: row.6,
                        signal_offset: Price::ZERO,
                    })
                })
                .collect();
            assert_eq!(bars_of(&dir, "CLK2020"), expected);

            // Spelled out, so a future edit to the fixture cannot quietly
            // stop testing the thing this test is named after.
            let MarketEvent::Bar(settle) = &bars_of(&dir, "CLK2020")[2];
            assert_eq!(settle.close, Price::from_nanos(-37_630_000_000));
            assert_eq!(settle.close.to_string(), "-37.63");
            assert_eq!(settle.low, Price::from_nanos(-40_320_000_000));
            let MarketEvent::Bar(crossing) = &bars_of(&dir, "CLK2020")[1];
            assert_eq!(crossing.close, Price::ZERO);
        }

        #[test]
        fn a_non_bar_schema_is_skipped_with_a_reason() {
            let dir = TempDir::new();
            let rel = "raw/GLBX.MDP3/trades/ES.FUT/2024-01.dbn.zst";
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(&path, b"not really dbn").expect("write");
            let catalog = catalog_for(&dir, "trades", rel);

            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.rows_written(), 0);
            assert!(matches!(
                report.outcomes.as_slice(),
                [RecordOutcome::Skipped { reason, .. }] if reason.contains("not a bar schema")
            ));
        }

        // ------------------------------------------------- negative controls

        fn refusal_reason(rows: &[Row], symbols: &[(u32, &str)], rtype: u8) -> String {
            let (dir, catalog) = setup(symbols, rows, rtype);
            let err = transcode(&catalog, &TranscodeOptions::default())
                .expect_err("must refuse this file");
            assert!(
                nothing_published(&dir),
                "a refused record must publish nothing: {err}"
            );
            err.to_string()
        }

        // The control for D-0070's loosened predicate: the validity test moved
        // from `!= UNDEF_PRICE && > 0` to `!= UNDEF_PRICE`, and this asserts
        // the half that stayed. Deleting the real check would pass every other
        // test in this file.
        #[test]
        fn an_undefined_price_is_refused_and_nothing_is_published() {
            let mut bad = one_bar(1, 1);
            bad.3 = UNDEF_PRICE;
            let reason = refusal_reason(&[one_bar(1, 0), bad], &[(1, "ESH4")], ohlcv_1m_rtype());
            assert!(reason.contains("UNDEF_PRICE"), "{reason}");
        }

        // `UNDEF_PRICE` is `i64::MAX`, so a *negative* sentinel-shaped value is
        // not a sentinel: only the exact constant means "no price". The
        // arithmetic negation of it is an ordinary, if absurd, price.
        #[test]
        fn only_the_exact_undef_sentinel_is_refused() {
            let mut bar = one_bar(1, 0);
            bar.4 = -UNDEF_PRICE;
            let (dir, catalog) = setup(&[(1, "ESH4")], &[bar], ohlcv_1m_rtype());
            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.rows_written(), 1);
            let MarketEvent::Bar(written) = &bars_of(&dir, "ESH2024")[0];
            assert_eq!(written.low, Price::from_nanos(-UNDEF_PRICE));
        }

        // Replaces `a_nonpositive_price_is_refused`, whose assertion was the
        // bug (D-0070): zero and negative prices are legal for every record
        // type, and refusing them refused 8.9 % of the archive.
        #[test]
        fn a_zero_or_negative_price_is_written_not_refused() {
            let mut zero = one_bar(1, 0);
            zero.4 = 0; // low
            let mut negative = one_bar(1, 1);
            negative.2 = -1_500_000_000; // open
            negative.4 = -2_000_000_000; // low
            let (dir, catalog) = setup(&[(1, "ESH4")], &[zero, negative], ohlcv_1m_rtype());

            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(report.rows_written(), 2);
            let bars = bars_of(&dir, "ESH2024");
            let MarketEvent::Bar(first) = &bars[0];
            assert_eq!(first.low, Price::ZERO);
            let MarketEvent::Bar(second) = &bars[1];
            assert_eq!(second.open, Price::from_nanos(-1_500_000_000));
            assert_eq!(second.low, Price::from_nanos(-2_000_000_000));
        }

        // ------------------------------------------- the spread filter (D-0070)

        // The control for the filter: a spread record is *skipped and counted*,
        // never refused, and the outright beside it is written as usual.
        #[test]
        fn a_spread_is_skipped_and_counted_by_default() {
            let rows = vec![
                one_bar(1, 0),
                one_bar(2, 0),
                one_bar(1, 1),
                one_bar(2, 1),
                one_bar(2, 2),
            ];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESH4-ESM4")], &rows, ohlcv_1m_rtype());
            let report = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");

            assert_eq!(report.partitions_written(), 1);
            assert_eq!(report.rows_written(), 2);
            assert_eq!(bars_of(&dir, "ESH2024").len(), 2);
            assert!(
                ParquetBarFeed::open(
                    dir.path(),
                    &InstrumentId::new("ESH4-ESM4"),
                    TimeFrame::M1,
                    None
                )
                .is_err()
            );

            // Counted, not absorbed.
            assert_eq!(report.spread_records_skipped(), 3);
            assert_eq!(report.spread_instruments_skipped(), 1);
            let text = report.to_string();
            assert!(
                text.contains("3 spread record(s) across 1 instrument(s)"),
                "{text}"
            );
            assert!(text.contains("--include-spreads"), "{text}");
        }

        #[test]
        fn a_spread_is_written_when_the_flag_asks_for_it() {
            let rows = vec![
                one_bar(1, 0),
                one_bar(2, 0),
                one_bar(1, 1),
                one_bar(2, 1),
                one_bar(2, 2),
            ];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESH4-ESM4")], &rows, ohlcv_1m_rtype());
            let opts = TranscodeOptions {
                include_spreads: true,
                ..TranscodeOptions::default()
            };
            let report = transcode(&catalog, &opts).expect("transcode");

            assert_eq!(report.partitions_written(), 2);
            assert_eq!(report.rows_written(), 5);
            assert_eq!(bars_of(&dir, "ESH4-ESM4").len(), 3);
            assert_eq!(report.spread_records_skipped(), 0);
            assert!(report.to_string().contains("spreads included"), "{report}");
        }

        // A spread's differential is negative in contango, which is why the
        // two halves of D-0070 arrived together: the old predicate refused
        // `GC.FUT ohlcv-1m` at record #0 over exactly this.
        #[test]
        fn a_spread_with_a_negative_differential_is_written_under_the_flag() {
            let mut spread = one_bar(2, 0);
            spread.2 = -250_000_000; // open  −0.25
            spread.3 = -250_000_000; // high  −0.25
            spread.4 = -1_000_000_000; // low   −1.00
            spread.5 = -750_000_000; // close −0.75
            let (dir, catalog) = setup(
                &[(1, "ESH4"), (2, "ESH4-ESM4")],
                &[one_bar(1, 0), spread],
                ohlcv_1m_rtype(),
            );
            let opts = TranscodeOptions {
                include_spreads: true,
                ..TranscodeOptions::default()
            };
            transcode(&catalog, &opts).expect("transcode");
            let MarketEvent::Bar(bar) = &bars_of(&dir, "ESH4-ESM4")[0];
            assert_eq!(bar.close, Price::from_nanos(-750_000_000));
        }

        // Asking for a spread by name while the filter that excludes it is on
        // would decode the archive and write nothing. An empty result in the
        // shape of a finished one is worse than a refusal that costs a flag.
        #[test]
        fn naming_a_spread_while_the_filter_is_on_is_refused() {
            let (dir, catalog) = setup(&[(1, "ESH4")], &[one_bar(1, 0)], ohlcv_1m_rtype());
            let opts = TranscodeOptions {
                symbols: Some(vec!["ESH4".to_owned(), "ESH4-ESM4".to_owned()]),
                ..TranscodeOptions::default()
            };
            let err = transcode(&catalog, &opts).expect_err("must refuse the contradiction");
            assert!(
                matches!(err, TranscodeError::ContradictorySpreadFilter { .. }),
                "{err}"
            );
            assert!(err.to_string().contains("ESH4-ESM4"), "{err}");
            assert!(nothing_published(&dir), "nothing may be written: {err}");

            // The same request with the flag on is not a contradiction.
            let opts = TranscodeOptions {
                symbols: Some(vec!["ESH4".to_owned(), "ESH4-ESM4".to_owned()]),
                include_spreads: true,
                ..TranscodeOptions::default()
            };
            transcode(&catalog, &opts).expect("no contradiction with the flag on");
        }

        // The count describes the source window, not the request: a
        // `--symbols` filter that would have excluded the spread anyway must
        // not change how many spread records the window is reported to hold.
        #[test]
        fn the_spread_count_does_not_depend_on_the_symbol_filter() {
            let rows = vec![one_bar(1, 0), one_bar(2, 0), one_bar(2, 1)];
            let (_dir, catalog) = setup(&[(1, "ESH4"), (2, "ESH4-ESM4")], &rows, ohlcv_1m_rtype());
            let opts = TranscodeOptions {
                symbols: Some(vec!["ESH4".to_owned()]),
                ..TranscodeOptions::default()
            };
            let report = transcode(&catalog, &opts).expect("transcode");
            assert_eq!(report.spread_records_skipped(), 2);
            assert_eq!(report.spread_instruments_skipped(), 1);
        }

        // An OhlcvMsg decodes whether it is a 1-second or a 1-day bar, so the
        // file's schema is the authority and the record has to agree.
        #[test]
        fn a_record_from_the_wrong_interval_is_refused() {
            let reason = refusal_reason(&[one_bar(1, 0)], &[(1, "ESH4")], RType::Ohlcv1S as u8);
            assert!(reason.contains("rtype"), "{reason}");
        }

        #[test]
        fn an_unaligned_timestamp_is_refused() {
            let mut bad = one_bar(1, 0);
            bad.1 += 20_000_000_000; // twenty seconds into the minute
            let reason = refusal_reason(&[bad], &[(1, "ESH4")], ohlcv_1m_rtype());
            assert!(reason.contains("whole multiple"), "{reason}");
        }

        #[test]
        fn an_instrument_id_with_no_symbol_is_refused() {
            // Id 9 trades, but the header maps only id 1.
            let reason = refusal_reason(
                &[one_bar(1, 0), one_bar(9, 1)],
                &[(1, "ESH4")],
                ohlcv_1m_rtype(),
            );
            assert!(reason.contains("no symbol"), "{reason}");
        }

        #[test]
        fn a_repeated_timestamp_for_one_instrument_is_refused() {
            let reason = refusal_reason(
                &[one_bar(1, 0), one_bar(1, 0)],
                &[(1, "ESH4")],
                ohlcv_1m_rtype(),
            );
            assert!(reason.contains("ts_open"), "{reason}");
        }

        // ------------------------------------------------------- idempotency

        // The regression this guards: a parent key's symbology maps every
        // contract it *resolves to*, and most of them never trade a bar in the
        // window — the real January 2024 ES.FUT slice maps 41 and produces 16.
        // Deciding "already curated" from the mapped set therefore made every
        // completed transcode look unfinished, and re-ran forever. Here id 2 is
        // mapped and never trades.
        #[test]
        fn a_second_run_writes_nothing_and_force_rebuilds() {
            let rows = vec![one_bar(1, 0), one_bar(1, 1)];
            let (dir, catalog) = setup(&[(1, "ESH4"), (2, "ESM4")], &rows, ohlcv_1m_rtype());

            let first = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(first.partitions_written(), 1);

            let again = transcode(&catalog, &TranscodeOptions::default()).expect("transcode");
            assert_eq!(again.partitions_written(), 0);
            assert!(matches!(
                bar_outcomes(&again).as_slice(),
                [RecordOutcome::AlreadyCurated { partitions: 1, .. }]
            ));
            assert_eq!(again.spread_records_skipped(), 0);

            let forced = transcode(
                &catalog,
                &TranscodeOptions {
                    force: true,
                    ..TranscodeOptions::default()
                },
            )
            .expect("transcode");
            assert_eq!(forced.partitions_written(), 1);
            assert_eq!(bars_of(&dir, "ESH2024").len(), 2);
        }

        // Two raw windows computing one curated path is a naming bug. Silently
        // overwriting would leave a partition whose provenance metadata names
        // bytes it does not contain.
        #[test]
        fn a_partition_from_different_raw_bytes_is_refused() {
            let dir = TempDir::new();
            write_dbn(
                &dir.path().join(RAW_PATH),
                &[(1, "ESH4")],
                &[one_bar(1, 0)],
                ohlcv_1m_rtype(),
            );
            let mut catalog = Catalog::open(dir.path()).expect("open catalog");
            plant_definitions(&dir, &mut catalog, &[(1, "ESH4")]);
            append_record(&mut catalog, "ohlcv-1m", RAW_PATH, JAN_2024, "ES.FUT");
            transcode(&catalog, &TranscodeOptions::default()).expect("first transcode");

            // Same window, different bytes, recorded under a second path so the
            // manifest accepts it (raw is immutable, D-0017).
            let other_rel = "raw/GLBX.MDP3/ohlcv-1m/ES.FUT2/2024-01.dbn.zst";
            write_dbn(
                &dir.path().join(other_rel),
                &[(1, "ESH4")],
                &[one_bar(1, 0), one_bar(1, 1)],
                ohlcv_1m_rtype(),
            );
            let mut catalog = Catalog::open(dir.path()).expect("reopen");
            catalog
                .append(Acquisition {
                    dataset: "GLBX.MDP3".to_owned(),
                    schema: "ohlcv-1m".to_owned(),
                    symbols: vec!["ES.FUT".to_owned()],
                    range: TsRange::new(Ts(JAN1), Ts(FEB1)).expect("range"),
                    acquired_ts: Ts(JAN1),
                    databento_job_id: "TEST-JOB-2".to_owned(),
                    file_path: other_rel.to_owned(),
                })
                .expect("append");

            let err = transcode(
                &catalog,
                &TranscodeOptions {
                    force: true,
                    ..TranscodeOptions::default()
                },
            )
            .expect_err("must refuse a foreign source");
            assert!(
                matches!(err, TranscodeError::SourceConflict { .. }),
                "{err}"
            );
        }
    }
}

/// Why nothing already in this codebase could have caught D-0072.
///
/// Not decoration and not a regression guard: these two tests assert that the
/// project's existing ordering and coverage checks **pass** on the merged
/// partition the bug produced. A defect that every detector waves through is
/// one you only find by looking, so the record of *which* detectors were blind,
/// and structurally why, is part of the fix.
#[cfg(test)]
mod blind_detector_controls {
    use crucible_core::traits::Feed;
    use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};

    use crate::calendar::Calendar;
    use crate::curated::{BarColumns, ParquetBarFeed, PartitionSource, write_partition};
    use crate::qa;
    use crate::testutil::TempDir;

    const DAY_NS: i64 = 86_400_000_000_000;
    const MIN_NS: i64 = 60_000_000_000;
    /// 2014-11-03, epoch day 16,377 — inside the December-2014 gold contract's life.
    const NOV_2014: i64 = 16_377;
    /// 2024-11-04, epoch day 20,031 — inside the December-2024 one's, ten years on.
    const NOV_2024: i64 = 20_031;

    /// Plants the partition the bug produced: one key holding two contracts
    /// that traded a decade apart, concatenated.
    ///
    /// Bars open at 15:00 UTC (09:00 Central), which is inside the CME session
    /// on both days, so nothing here is out of session on either side of the
    /// hole.
    fn plant_merged(dir: &TempDir, instrument: &str, tf: TimeFrame) {
        let mut bars = BarColumns::default();
        for day in [NOV_2014, NOV_2024] {
            for minute in 0..60 {
                bars.ts_open
                    .push(day * DAY_NS + 15 * 3_600_000_000_000 + minute * MIN_NS);
                for column in [
                    &mut bars.open,
                    &mut bars.high,
                    &mut bars.low,
                    &mut bars.close,
                ] {
                    column.push(1_200_000_000_000);
                }
                bars.volume.push(10);
            }
        }
        write_partition(
            dir.path(),
            PartitionSource {
                instrument: InstrumentId::new(instrument),
                tf,
                dataset: "GLBX.MDP3".to_owned(),
                vendor_schema: "ohlcv-1m".to_owned(),
                source_file_path: "raw/GLBX.MDP3/ohlcv-1m/GC.FUT/2010-06-06--2026-07-28.dbn.zst"
                    .to_owned(),
                source_file_blake3: "00".repeat(32),
            },
            "2010-06-06--2026-07-28",
            &bars,
        )
        .expect("write")
        .expect("rows");
    }

    // BLIND DETECTOR 1 — strictly increasing `ts_open`.
    //
    // `PartitionWriter::push` refuses a repeated or out-of-order `ts_open`, and
    // `ParquetBarFeed::open` re-checks it. Both pass here, and they had to: the
    // two contracts trade in *disjoint sequential* periods, so concatenating
    // them produces a perfectly ordered series. Order is a statement about
    // neighbours; aliasing is a statement about identity, and no ordering check
    // can see it.
    #[test]
    fn the_ts_open_monotonicity_check_passes_on_the_merged_partition() {
        let dir = TempDir::new();
        plant_merged(&dir, "GCZ4", TimeFrame::M1);

        let mut feed =
            ParquetBarFeed::open(dir.path(), &InstrumentId::new("GCZ4"), TimeFrame::M1, None)
                .expect("the writer accepted every row and the reader accepts them back");
        let mut previous: Option<Ts> = None;
        let mut count = 0usize;
        while let Some(crucible_core::events::MarketEvent::Bar(bar)) = feed.next_event() {
            if let Some(prev) = previous {
                assert!(bar.ts_open > prev, "the merged series really is ordered");
            }
            previous = Some(bar.ts_open);
            count += 1;
        }
        assert_eq!(count, 120, "both contracts' bars are present, in one file");

        // And the span the check was happy with is 3,654 days — the thing a
        // human would have spotted instantly and no assertion was looking at.
        let span_days = (previous.expect("bars").0 - (NOV_2014 * DAY_NS)) / DAY_NS;
        assert_eq!(span_days, NOV_2024 - NOV_2014);
        assert!(
            span_days > 3_600,
            "one gold contract does not live ten years"
        );
    }

    // BLIND DETECTOR 2 — gaps inside sessions.
    //
    // Two independent reasons it was silent, and D-0086 removed the first:
    //
    // 1. **No bundled calendar claimed GC.** `Calendar::for_instrument` answered
    //    only for the equity-index roots, so `qa` had no definition of
    //    "expected" for gold at all: coverage, gaps and out-of-session bars were
    //    every one of them skipped, and the report said `calendar none`. That is
    //    the run in the bug report, and it printed "gaps inside sessions none"
    //    because it never looked. `cme_globex_metals` now governs GC, so this
    //    reason is gone.
    // 2. **The hole falls between sessions.** The check is defined as missing
    //    bars *inside* a session; a ten-year absence is almost entirely made of
    //    sessions that are wholly absent, which is a coverage number.
    //
    // So the test has two halves. Called the way the bug report called it — no
    // calendar — the detector still reports nothing, which is the historical
    // fact and the reason the partition key had to be the fix. Called with the
    // calendar gold now has, coverage collapses and the missing-bar count runs
    // to millions. That is a **side effect of building a metals session table**,
    // not the "strengthening" CLAUDE.md §9 refuses: the check was not touched,
    // and a `qa` run nobody schedules still catches nothing.
    #[test]
    fn the_gap_inside_sessions_check_passes_on_the_merged_partition() {
        let dir = TempDir::new();
        plant_merged(&dir, "GCZ4", TimeFrame::M1);

        let report = qa::run(
            dir.path(),
            &InstrumentId::new("GCZ4"),
            TimeFrame::M1,
            None,
            None,
            &qa::QaOptions::default(),
        )
        .expect("qa runs");

        assert!(
            report.gaps.is_empty() && report.missing_bars == 0,
            "the detector reports nothing: {report}"
        );
        assert_eq!(
            report.expected_bars, None,
            "there is no `expected` without a calendar"
        );
        assert_eq!(report.bars, 120);
        // The one number that was visible all along, and that nothing compared
        // against anything: the span.
        assert_eq!(
            report.last_ts_open.expect("bars").0 - report.first_ts_open.expect("bars").0,
            (NOV_2024 - NOV_2014) * DAY_NS + 59 * MIN_NS
        );
    }

    // The half that D-0086 made possible: reason (1) is gone, and the same
    // planted merge is now loud.
    //
    // Ten years of gold sessions against 120 bars. The floor asserted below is
    // hand-derived and deliberately far under the true figure: 3,654 days is
    // more than 2,600 weekdays, each a session of at least 12 hours after
    // holidays and early closes, so at least 2,600 x 720 = 1,872,000 one-minute
    // intervals are expected and all but 120 of them are missing.
    #[test]
    fn the_metals_calendar_makes_the_same_merged_partition_loud() {
        let dir = TempDir::new();
        plant_merged(&dir, "GCZ4", TimeFrame::M1);

        let calendar = Calendar::for_instrument(&InstrumentId::new("GCZ4"))
            .expect("lookup")
            .expect("cme_globex_metals governs GC as of D-0086");
        assert_eq!(calendar.id(), "cme_globex_metals");

        let report = qa::run(
            dir.path(),
            &InstrumentId::new("GCZ4"),
            TimeFrame::M1,
            None,
            Some(&calendar),
            &qa::QaOptions::default(),
        )
        .expect("qa runs");

        assert!(
            report.missing_bars > 1_872_000,
            "ten years of absent sessions, not a gap: {} missing",
            report.missing_bars
        );
        assert!(
            report.coverage_pct().expect("a calendar gives coverage") < 0.1,
            "coverage should be a rounding error, not a percentage"
        );
        assert!(!report.is_clean(), "exit 4 is the right answer here");
    }

    // The positive half: the same two contracts under *resolved* keys are two
    // partitions, each spanning one hour of one day, and neither spans a decade.
    // Same bars, same order, same everything the detectors could see — the only
    // difference is the key (D-0072).
    #[test]
    fn resolved_keys_split_the_same_bars_into_two_partitions() {
        let dir = TempDir::new();
        let mut spans = Vec::new();
        for (key, day) in [("GCZ2014", NOV_2014), ("GCZ2024", NOV_2024)] {
            let mut bars = BarColumns::default();
            for minute in 0..60 {
                bars.ts_open
                    .push(day * DAY_NS + 15 * 3_600_000_000_000 + minute * MIN_NS);
                for column in [
                    &mut bars.open,
                    &mut bars.high,
                    &mut bars.low,
                    &mut bars.close,
                ] {
                    column.push(Price::from_points(1200).as_nanos());
                }
                bars.volume.push(10);
            }
            write_partition(
                dir.path(),
                PartitionSource {
                    instrument: InstrumentId::new(key),
                    tf: TimeFrame::M1,
                    dataset: "GLBX.MDP3".to_owned(),
                    vendor_schema: "ohlcv-1m".to_owned(),
                    source_file_path:
                        "raw/GLBX.MDP3/ohlcv-1m/GC.FUT/2010-06-06--2026-07-28.dbn.zst".to_owned(),
                    source_file_blake3: "00".repeat(32),
                },
                "2010-06-06--2026-07-28",
                &bars,
            )
            .expect("write")
            .expect("rows");

            let feed =
                ParquetBarFeed::open(dir.path(), &InstrumentId::new(key), TimeFrame::M1, None)
                    .expect("open");
            spans.push((
                key,
                feed.len(),
                (feed.last_ts_open().expect("bars").0 - feed.first_ts_open().expect("bars").0)
                    / DAY_NS,
            ));
        }
        assert_eq!(spans, vec![("GCZ2014", 60, 0), ("GCZ2024", 60, 0)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_vendor_bar_schemas_map_to_a_timeframe() {
        assert_eq!(timeframe_for_schema("ohlcv-1s"), Some(TimeFrame::S1));
        assert_eq!(timeframe_for_schema("ohlcv-1m"), Some(TimeFrame::M1));
        assert_eq!(timeframe_for_schema("ohlcv-1h"), Some(TimeFrame::H1));
        assert_eq!(timeframe_for_schema("ohlcv-1d"), Some(TimeFrame::D1));
        // Not bars. Mapping any of these onto a timeframe would be inventing
        // an aggregation nobody chose.
        for schema in [
            "trades",
            "tbbo",
            "mbo",
            "mbp-10",
            "definition",
            "statistics",
        ] {
            assert_eq!(timeframe_for_schema(schema), None, "{schema}");
        }
        // Databento does not aggregate at these; the TimeFrame variants exist
        // for resampling that has not been built.
        for schema in ["ohlcv-5m", "ohlcv-15m", ""] {
            assert_eq!(timeframe_for_schema(schema), None, "{schema}");
        }
    }

    #[test]
    fn an_empty_report_says_so_rather_than_printing_nothing() {
        let report = TranscodeReport::default();
        assert!(
            report
                .to_string()
                .contains("nothing in the manifest matched")
        );
        assert_eq!(report.rows_written(), 0);
        assert_eq!(report.partitions_written(), 0);
    }

    #[test]
    fn a_report_totals_partitions_and_rows() {
        let report = TranscodeReport {
            outcomes: vec![
                RecordOutcome::Transcoded {
                    source_file_path: "raw/a.dbn.zst".to_owned(),
                    tf: TimeFrame::M1,
                    partitions: vec![
                        WrittenPartition {
                            instrument: "ESH4".to_owned(),
                            path: PathBuf::from("curated/bars/ESH2024/1m/2024-01.parquet"),
                            rows: 7,
                        },
                        WrittenPartition {
                            instrument: "ESM4".to_owned(),
                            path: PathBuf::from("curated/bars/ESM4/1m/2024-01.parquet"),
                            rows: 3,
                        },
                    ],
                    skipped: 5,
                    spreads: SpreadsExcluded {
                        spread_records_skipped: 1_234,
                        spread_instruments_skipped: 20,
                    },
                },
                RecordOutcome::Skipped {
                    source_file_path: "raw/b.dbn.zst".to_owned(),
                    reason: "schema `trades` is not a bar schema".to_owned(),
                },
            ],
            include_spreads: false,
        };
        assert_eq!(report.partitions_written(), 2);
        assert_eq!(report.rows_written(), 10);
        assert_eq!(report.spread_records_skipped(), 1_234);
        assert_eq!(report.spread_instruments_skipped(), 20);
        let text = report.to_string();
        assert!(text.contains("raw/a.dbn.zst"), "{text}");
        assert!(text.contains("2 partition(s), 10 bar(s) written"), "{text}");
        assert!(text.contains("not a bar schema"), "{text}");
        // A bounded run must never read as a complete one.
        assert!(
            text.contains("5 partition(s) already held these bytes"),
            "{text}"
        );
        assert!(
            text.contains("1234 spread record(s) across 20 instrument(s)"),
            "{text}"
        );
    }

    // "Nothing was excluded" and "exclusions were not counted" must not print
    // the same way — the same argument that makes `combo` print an
    // intrabar-convention line saying the count is zero (D-0069, D-0070).
    #[test]
    fn a_report_states_the_spread_mode_even_when_nothing_was_excluded() {
        // A run that matched nothing short-circuits: it decoded no file, so
        // there is no exclusion to report and saying "0 excluded" would imply
        // one was looked for.
        assert!(
            !TranscodeReport::default()
                .to_string()
                .contains("spread filter")
        );

        let nothing_excluded = TranscodeReport {
            outcomes: vec![RecordOutcome::Skipped {
                source_file_path: "raw/b.dbn.zst".to_owned(),
                reason: "not a bar schema".to_owned(),
            }],
            include_spreads: false,
        };
        assert!(
            nothing_excluded
                .to_string()
                .contains("spread filter on (the default): 0 spread record(s)"),
            "{nothing_excluded}"
        );

        let including = TranscodeReport {
            outcomes: vec![RecordOutcome::Skipped {
                source_file_path: "raw/b.dbn.zst".to_owned(),
                reason: "not a bar schema".to_owned(),
            }],
            include_spreads: true,
        };
        assert!(
            including.to_string().contains("spreads included"),
            "{including}"
        );
    }

    // The predicate, against the shapes the archive actually contains.
    // Outrights: 614 distinct, all plain `[A-Z0-9]+`. Spreads: 21,044 with a
    // dash and 5,434 with a colon-and-space. No symbol falls outside those.
    #[test]
    fn the_spread_predicate_separates_the_shapes_the_archive_contains() {
        for outright in [
            "ESH4", "CLF10", "CLK0", "RTYU7", "RTYZ7", "6EF0", "GCZ9", "ZNH4", "CLZ36",
        ] {
            assert!(!names_a_spread(outright), "{outright} is an outright");
        }
        for spread in [
            "RTYU7-RTYZ7",
            "ESH4-ESM4",
            "6EF0-6EU9",
            "CL:BF F0-G0-H0",
            "UD:ZN: TL 0110987001",
            "ZN:CF 0110987001",
            "WS:XS 0110987001",
        ] {
            assert!(names_a_spread(spread), "{spread} is a spread");
        }
    }

    // The predicate gates an exclusion whose default is on, so the direction
    // it fails in is the whole design: an unrecognised shape must be *written*
    // (visible clutter) rather than dropped (a silent gap). A synthetic
    // instrument and a continuous alias both carry no marker, so both would be
    // written if one ever reached this path.
    #[test]
    fn an_unrecognised_shape_is_not_called_a_spread() {
        for other in ["SYN:RW", "ES.v.0", "ES.c.0", "MES", "", "X"] {
            // `SYN:RW` is the one exception and it is deliberate: it carries a
            // colon, and no vendor record ever resolves to it — the synthetic
            // feed never passes through transcode.
            if other == "SYN:RW" {
                assert!(names_a_spread(other));
                continue;
            }
            assert!(
                !names_a_spread(other),
                "{other} must be written, not dropped"
            );
        }
    }
}
