//! Which grain a curated replay reads, and how it reaches the one asked for.
//!
//! The archive holds what the vendor sent: `ohlcv-1s` and `ohlcv-1m`, because
//! the hourly and daily aggregates cost $190/GB and were deliberately not
//! bought (`docs/DATA_PLAN.md`). Everything coarser is
//! [`resample`](crucible_data::curated::resample)d on the exchange's own
//! sessions when it is read (D-0077).
//!
//! One place decides which of the two happens, so `backtest`, `combo`,
//! `walk-forward` and `funnel` cannot come to different answers about what
//! `--timeframe 5m` means. The rule is one line: **replay the stored grain if
//! the archive has it, otherwise aggregate 1-minute bars into it.** No fallback
//! is silent — [`CuratedGrain::resample_report`] is `Some` exactly when
//! aggregation happened, and every command that prints a header prints it.

use std::path::Path;

use crucible_core::events::MarketEvent;
use crucible_core::traits::Feed;
use crucible_core::types::{InstrumentId, TimeFrame, Ts};
use crucible_data::catalog::TsRange;
use crucible_data::curated::path::partition_dir;
use crucible_data::curated::resample::{ResampleError, ResampleReport, ResampledBarFeed};
use crucible_data::curated::{
    CuratedError, CuratedSource, ParquetBarFeed, Resolution, resolve_instrument,
};

/// The grain the archive is aggregated *from* when it does not hold the one
/// asked for.
///
/// 1-minute rather than 1-second, though both were bought: a sixteen-year 1s
/// window is two orders of magnitude more rows, and aggregating it to 5m gives
/// bar-for-bar the same answer because `ohlcv` sums are associative. Reading a
/// hundred times the bytes for an identical result is not a trade worth making.
const RESAMPLE_SOURCE: TimeFrame = TimeFrame::M1;

/// A curated replay at one grain, however that grain was obtained.
///
/// A wrapper rather than `Box<dyn Feed>` for `backtest::Replay`'s reason: the
/// accessors below are not on the `Feed` trait and should not be — the engine
/// has no business asking a feed how many bars it holds.
pub(crate) enum CuratedGrain {
    /// The archive holds this grain; it is replayed exactly as stored.
    Stored(ParquetBarFeed),
    /// The archive holds a finer grain, aggregated here at read time.
    Resampled(Box<ResampledBarFeed>),
}

impl CuratedGrain {
    /// Opens `instrument` at `tf`, resampling from
    /// [`RESAMPLE_SOURCE`] if the archive holds no partitions at that grain.
    ///
    /// # Errors
    /// [`ResampleError::Curated`] wrapping whatever the curated store refused
    /// — including [`CuratedError::NoCuratedData`] naming the **1-minute**
    /// partition directory when resampling was the path taken, which is the
    /// directory an operator has to fill — and the resampler's own refusals.
    ///
    /// [`CuratedError::NoCuratedData`]: crucible_data::curated::CuratedError::NoCuratedData
    pub(crate) fn open(
        data_dir: &Path,
        instrument: &InstrumentId,
        tf: TimeFrame,
        range: Option<TsRange>,
    ) -> Result<CuratedGrain, ResampleError> {
        let stored = partition_dir(data_dir, instrument, tf)
            .map(|dir| dir.is_dir())
            .unwrap_or(false);
        if stored || tf.duration_ns() <= RESAMPLE_SOURCE.duration_ns() {
            return Ok(CuratedGrain::Stored(ParquetBarFeed::open(
                data_dir, instrument, tf, range,
            )?));
        }
        Ok(CuratedGrain::Resampled(Box::new(ResampledBarFeed::open(
            data_dir,
            instrument,
            RESAMPLE_SOURCE,
            tf,
            range,
        )?)))
    }

    /// What the aggregation did, or `None` when the grain was read as stored.
    pub(crate) fn resample_report(&self) -> Option<&ResampleReport> {
        match self {
            CuratedGrain::Stored(_) => None,
            CuratedGrain::Resampled(f) => Some(f.report()),
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            CuratedGrain::Stored(f) => f.len(),
            CuratedGrain::Resampled(f) => f.len(),
        }
    }

    pub(crate) fn first_ts_open(&self) -> Option<Ts> {
        match self {
            CuratedGrain::Stored(f) => f.first_ts_open(),
            CuratedGrain::Resampled(f) => f.first_ts_open(),
        }
    }

    pub(crate) fn last_ts_open(&self) -> Option<Ts> {
        match self {
            CuratedGrain::Stored(f) => f.last_ts_open(),
            CuratedGrain::Resampled(f) => f.last_ts_open(),
        }
    }

    /// The curated files behind the bars, with the manifest id of the raw bytes
    /// behind each. Identical either way: a resampled bar's provenance is its
    /// constituents' provenance (D-0013).
    pub(crate) fn sources(&self) -> &[CuratedSource] {
        match self {
            CuratedGrain::Stored(f) => f.sources(),
            CuratedGrain::Resampled(f) => f.sources(),
        }
    }
}

impl Feed for CuratedGrain {
    fn next_event(&mut self) -> Option<MarketEvent> {
        match self {
            CuratedGrain::Stored(f) => f.next_event(),
            CuratedGrain::Resampled(f) => f.next_event(),
        }
    }
}

/// One line naming the grain, for a report header.
///
/// Printed on every curated run, including the ones that resampled nothing —
/// "these are the bars the vendor sent" and "these were aggregated here" are
/// different claims, and a reader who cannot tell them apart does not know
/// which assumptions the numbers rest on (§2.4's argument, applied to data).
pub(crate) fn grain_line(grain: &CuratedGrain, tf: TimeFrame) -> String {
    match grain.resample_report() {
        None => format!("{tf} as stored — the grain the vendor delivered"),
        Some(r) => format!(
            "{} resampled to {tf} on {} sessions — {} source bar(s) over {} trading day(s), \
             resampler v{}",
            r.source_tf,
            r.calendar_id,
            r.source_bars,
            r.trading_days,
            crucible_data::curated::RESAMPLER_VERSION,
        ),
    }
}

/// Resolves a possibly-shorthand instrument name against whichever grain holds
/// bars for it.
///
/// `curated/bars/ESH2024/5m` never exists — 5-minute bars are aggregated on
/// read (D-0077) — so resolving `ESH4` at 5m would find nothing and refuse a
/// contract that is right there. The requested grain is tried first anyway,
/// because 1s and 1m *are* stored and an operator asking for one of them
/// should be answered from it.
///
/// The D-0072 refusal is unaffected: an ambiguous shorthand is still ambiguous
/// at whichever grain answered.
///
/// # Errors
/// Whatever [`resolve_instrument`](crucible_data::curated::resolve_instrument)
/// returns — a `curated/bars` that cannot be listed, or a directory under it
/// that is not a percent-encoded instrument.
pub(crate) fn resolve_at_any_grain(
    data_dir: &Path,
    tf: TimeFrame,
    requested: &str,
) -> Result<Resolution, CuratedError> {
    let at_requested = resolve_instrument(data_dir, tf, requested)?;
    if tf == RESAMPLE_SOURCE || !matches!(at_requested, Resolution::Missing(_)) {
        return Ok(at_requested);
    }
    resolve_instrument(data_dir, RESAMPLE_SOURCE, requested)
}

/// Prints a series' caveats, or nothing when there are none.
///
/// Shared so `combo`, `walk-forward` and `funnel` cannot come to differ about
/// whether a caveat is worth showing.
pub(crate) fn print_caveats(caveats: &[String]) {
    for caveat in caveats {
        println!("  NOTE           {caveat}");
    }
    if !caveats.is_empty() {
        println!();
    }
}

/// The caveats a resampled series carries, if any, one per line.
pub(crate) fn grain_caveats(grain: &CuratedGrain) -> Vec<String> {
    let Some(r) = grain.resample_report() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if r.first_bar_may_be_partial {
        out.push(format!(
            "the first {} bar's bucket opened before the first source bar, so it may be \
             built from part of its interval",
            r.target_tf
        ));
    }
    if r.last_bar_may_be_partial {
        out.push(format!(
            "the last {} bar's bucket runs past the last source bar while the session was \
             still open, so the requested window cut it short",
            r.target_tf
        ));
    }
    out
}
