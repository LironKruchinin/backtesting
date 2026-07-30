//! Data quality assurance over the curated store.
//!
//! The archive is the asset, and an archive nobody has inspected is a liability
//! wearing the archive's clothes. This module answers, for one instrument and
//! timeframe: **is anything missing, is anything impossible, and does the
//! vendor agree?**
//!
//! Five questions, in the order they matter:
//!
//! 1. **Coverage.** How many bars does a year of sessions contain, how many are
//!    actually here, and where are the holes? A hole inside a session is a data
//!    problem. A hole outside one is the market being shut, which is why this
//!    report takes a [`Calendar`] and why the answer is much weaker without one.
//! 2. **Bars that should not exist.** Bars whose `ts_open` falls when the
//!    calendar says the exchange was closed. This is the check that runs
//!    *backwards*: real data is the ground truth, and a pile of these means the
//!    calendar is wrong, not the archive. Treat it as the calendar's negative
//!    control (CLAUDE.md §7).
//! 3. **Zero-volume runs.** An `ohlcv` bar with no volume is a bar the vendor
//!    synthesised or a market that did not trade. Either way a strategy that
//!    "fills" in one is trading against nobody.
//! 4. **Spikes and impossible bars.** Outlier returns by a robust (median
//!    absolute deviation) measure, plus OHLC self-consistency. Returns are only
//!    compared between *adjacent* bars — a jump across a session boundary or a
//!    gap is not a spike, and counting it as one buries the real ones.
//! 5. **Daylight-saving boundaries.** The two days a year the session moves an
//!    hour in UTC. Reported so a human can confirm the archive did what the
//!    calendar says it should have.
//!
//! Finally the vendor's own `condition.json` — delivered beside every batch
//! payload and kept under `delivery/` — is read back and any date the vendor
//! did not call `available` is surfaced. That file is the one piece of evidence
//! here that does not come from our own pipeline.
//!
//! ## Why this is plain Rust and not `polars`
//!
//! CLAUDE.md §6 pre-blessed `polars` for exactly this report and D-0037
//! narrowed it to this report. It is still not used, because everything above
//! is a single ordered pass over a bar series that [`ParquetBarFeed`] already
//! decodes, and `polars-core` depends on `rayon` non-optionally — adopting it
//! would start a work-stealing threadpool inside `crucible-data`, which §3
//! reserves for `crucible-funnel` alone. The DataFrame would buy nothing a
//! `for` loop does not, at the price of the one architectural rule this crate
//! exists to respect. See D-0040.
//!
//! [`ParquetBarFeed`]: crate::curated::ParquetBarFeed
//! [`Calendar`]: crate::calendar::Calendar

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crucible_core::events::{Bar, MarketEvent};
use crucible_core::traits::Feed;
#[cfg(test)]
use crucible_core::types::Price;
use crucible_core::types::{InstrumentId, TimeFrame, Ts};

use crate::calendar::{Calendar, CivilDate};
use crate::catalog::TsRange;
use crate::curated::{CuratedError, ParquetBarFeed};
use crate::ingest::window::{civil_from_days, date_of, days_from_civil};

/// Directory under the data dir holding one folder of support files per
/// delivered vendor job. Mirrors `ingest::execute`'s constant, which is
/// private to that module.
const DELIVERY_DIR: &str = "delivery";

/// Default robust-sigma multiple above which a return is called a spike.
///
/// 8 is deliberately loose. This report is a tripwire, not a filter: a list of
/// two hundred "spikes" gets skimmed and ignored, and the point is to surface
/// the handful that are actually impossible.
pub const DEFAULT_SPIKE_SIGMAS: f64 = 8.0;

/// Scale factor turning a median absolute deviation into a standard-deviation
/// equivalent for normally distributed data.
const MAD_TO_SIGMA: f64 = 1.482_602_218_505_602;

/// Why a QA run could not be produced.
#[derive(Debug)]
pub enum QaError {
    /// The curated store could not be read.
    Curated(CuratedError),
    /// A delivery support file could not be read.
    Io {
        /// Path being read.
        path: PathBuf,
        /// What was being attempted.
        during: &'static str,
        /// Underlying failure.
        source: std::io::Error,
    },
}

impl core::fmt::Display for QaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            QaError::Curated(e) => write!(f, "{e}"),
            QaError::Io {
                path,
                during,
                source,
            } => write!(f, "{during} {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for QaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            QaError::Curated(e) => Some(e),
            QaError::Io { source, .. } => Some(source),
        }
    }
}

impl From<CuratedError> for QaError {
    fn from(e: CuratedError) -> Self {
        QaError::Curated(e)
    }
}

/// Knobs. Defaults are tuned for "print this and read it", not for machine
/// consumption.
#[derive(Debug, Clone, PartialEq)]
pub struct QaOptions {
    /// Robust-sigma multiple above which a return is reported as a spike.
    pub spike_sigmas: f64,
    /// How many of the largest gaps to list individually.
    pub max_gaps_listed: usize,
    /// How many of the largest spikes to list individually.
    pub max_spikes_listed: usize,
    /// How many out-of-session bars to list individually.
    pub max_unexpected_listed: usize,
}

impl Default for QaOptions {
    fn default() -> Self {
        QaOptions {
            spike_sigmas: DEFAULT_SPIKE_SIGMAS,
            max_gaps_listed: 20,
            max_spikes_listed: 20,
            max_unexpected_listed: 10,
        }
    }
}

/// A run of consecutive missing bars inside a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// `ts_open` of the first missing bar.
    pub from_ts: Ts,
    /// `ts_open` of the first present bar after the gap, or the session end.
    pub to_ts: Ts,
    /// How many bar intervals are missing.
    pub missing_bars: i64,
}

/// A bar whose return is an outlier by the robust measure.
#[derive(Debug, Clone, PartialEq)]
pub struct Spike {
    /// `ts_open` of the bar.
    pub ts_open: Ts,
    /// Close-to-close move, in points.
    pub move_points: f64,
    /// That move in robust sigmas.
    pub sigmas: f64,
}

/// A bar that contradicts itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OhlcViolation {
    /// `ts_open` of the bar.
    pub ts_open: Ts,
    /// Which rule it broke.
    pub rule: &'static str,
}

/// A run of consecutive bars with no volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZeroVolumeRun {
    /// `ts_open` of the first bar in the run.
    pub from_ts: Ts,
    /// Number of consecutive zero-volume bars.
    pub bars: usize,
}

/// A trading day on which the session's UTC offset changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DstBoundary {
    /// The trading day.
    pub date: CivilDate,
    /// UTC hour the session opened on the previous trading day.
    pub previous_open_hour_utc: i64,
    /// UTC hour the session opened on this one.
    pub open_hour_utc: i64,
    /// Bars observed on this trading day.
    pub bars: usize,
    /// Bars the calendar expects on this trading day.
    pub expected_bars: i64,
}

/// One date's condition as the vendor reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionEntry {
    /// The date, as written by the vendor.
    pub date: String,
    /// The vendor's word for it: `available`, `degraded`, `pending`, `missing`.
    pub condition: String,
    /// Which delivery folder it was read from.
    pub job_id: String,
}

/// Everything one QA pass found.
#[derive(Debug, Clone)]
pub struct QaReport {
    /// Instrument inspected.
    pub instrument: String,
    /// Timeframe inspected.
    pub tf: TimeFrame,
    /// Calendar consulted, if any.
    pub calendar_id: Option<String>,
    /// Bars read.
    pub bars: usize,
    /// First `ts_open` held.
    pub first_ts_open: Option<Ts>,
    /// Last `ts_open` held.
    pub last_ts_open: Option<Ts>,
    /// Bars the calendar expects across the inspected span. `None` without a
    /// calendar — nothing else can say what "expected" means.
    pub expected_bars: Option<i64>,
    /// Missing runs inside sessions, largest first.
    pub gaps: Vec<Gap>,
    /// Total bars missing inside sessions.
    pub missing_bars: i64,
    /// Bars whose `ts_open` falls outside every session the calendar knows.
    pub unexpected_bars: Vec<Ts>,
    /// Total count of the above, which may exceed the listed sample.
    pub unexpected_bar_count: usize,
    /// Zero-volume runs, longest first.
    pub zero_volume_runs: Vec<ZeroVolumeRun>,
    /// Total zero-volume bars.
    pub zero_volume_bars: usize,
    /// Outlier returns, largest first — truncated to `max_spikes_listed`.
    pub spikes: Vec<Spike>,
    /// How many returns exceeded the threshold in total, before truncation.
    pub spike_count: usize,
    /// The robust-sigma threshold this run actually used.
    pub spike_sigmas: f64,
    /// Robust sigma of adjacent-bar returns, in points.
    pub robust_sigma_points: Option<f64>,
    /// Self-contradictory bars.
    pub ohlc_violations: Vec<OhlcViolation>,
    /// Daylight-saving transitions inside the span.
    pub dst_boundaries: Vec<DstBoundary>,
    /// Vendor-reported conditions that are not `available`.
    ///
    /// **Archive-wide, not filtered to this instrument or span.** A delivery
    /// folder records the job, not the contract, and a job covers a parent key
    /// over a window; attributing its dates to one contract would be a guess.
    /// Read these as "the vendor flagged these dates somewhere in this archive".
    pub conditions: Vec<ConditionEntry>,
    /// Delivery folders that carried a `condition.json`.
    pub condition_sources: Vec<String>,
}

impl QaReport {
    /// True when nothing worth acting on was found.
    ///
    /// **Spikes are deliberately not counted.** Real index futures move several
    /// robust sigmas at every RTH open, so a gate treating outlier returns as
    /// findings would never pass on real data — and a check that always fails
    /// is a check nobody reads. What is counted is completeness (missing bars),
    /// impossibility (bars outside a session, self-contradictory OHLC),
    /// emptiness (zero volume), and the vendor's own warnings. Spikes are
    /// printed for a human to judge.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.missing_bars == 0
            && self.unexpected_bar_count == 0
            && self.zero_volume_bars == 0
            && self.ohlc_violations.is_empty()
            && self.conditions.is_empty()
    }

    /// Bars present as a fraction of bars expected, when a calendar said what
    /// to expect.
    #[must_use]
    pub fn coverage_pct(&self) -> Option<f64> {
        let expected = self.expected_bars?;
        if expected <= 0 {
            return None;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a coverage percentage is statistics space (§2.3), not accounting"
        )]
        let present = (expected - self.missing_bars) as f64;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a coverage percentage is statistics space (§2.3), not accounting"
        )]
        let total = expected as f64;
        Some(present / total * 100.0)
    }
}

/// Runs every check over one instrument and timeframe.
///
/// `calendar` is optional but strongly recommended: without it there is no
/// definition of "expected", so coverage, out-of-session bars and daylight
/// boundaries are all skipped and only the checks a bar series can answer
/// about itself are run.
///
/// # Errors
/// [`QaError`] if the curated store cannot be read, or a delivery support file
/// exists but cannot be opened.
pub fn run(
    data_dir: &Path,
    instrument: &InstrumentId,
    tf: TimeFrame,
    range: Option<TsRange>,
    calendar: Option<&Calendar>,
    options: &QaOptions,
) -> Result<QaReport, QaError> {
    let mut feed = ParquetBarFeed::open(data_dir, instrument, tf, range)?;
    let mut bars: Vec<Bar> = Vec::with_capacity(feed.len());
    while let Some(MarketEvent::Bar(bar)) = feed.next_event() {
        bars.push(bar);
    }

    let mut report = QaReport {
        instrument: instrument.as_str().to_owned(),
        tf,
        calendar_id: calendar.map(|c| c.id().to_owned()),
        bars: bars.len(),
        first_ts_open: bars.first().map(|b| b.ts_open),
        last_ts_open: bars.last().map(|b| b.ts_open),
        expected_bars: None,
        gaps: Vec::new(),
        missing_bars: 0,
        unexpected_bars: Vec::new(),
        unexpected_bar_count: 0,
        zero_volume_runs: Vec::new(),
        zero_volume_bars: 0,
        spikes: Vec::new(),
        spike_count: 0,
        spike_sigmas: options.spike_sigmas,
        robust_sigma_points: None,
        ohlc_violations: Vec::new(),
        dst_boundaries: Vec::new(),
        conditions: Vec::new(),
        condition_sources: Vec::new(),
    };

    check_ohlc(&bars, &mut report);
    check_zero_volume(&bars, options, &mut report);
    check_spikes(&bars, tf, options, &mut report);
    if let Some(cal) = calendar {
        check_against_calendar(&bars, tf, cal, options, &mut report);
    }
    read_conditions(data_dir, &mut report)?;
    Ok(report)
}

/// A bar that contradicts itself is corrupt, whatever the vendor says.
fn check_ohlc(bars: &[Bar], report: &mut QaReport) {
    for bar in bars {
        let rules: [(&'static str, bool); 3] = [
            ("high < low", bar.high < bar.low),
            (
                "high below open or close",
                bar.high < bar.open || bar.high < bar.close,
            ),
            (
                "low above open or close",
                bar.low > bar.open || bar.low > bar.close,
            ),
        ];
        for (rule, broken) in rules {
            if broken {
                report.ohlc_violations.push(OhlcViolation {
                    ts_open: bar.ts_open,
                    rule,
                });
            }
        }
    }
}

fn check_zero_volume(bars: &[Bar], options: &QaOptions, report: &mut QaReport) {
    let mut run: Option<ZeroVolumeRun> = None;
    for bar in bars {
        if bar.volume == 0 {
            report.zero_volume_bars += 1;
            match &mut run {
                Some(current) => current.bars += 1,
                None => {
                    run = Some(ZeroVolumeRun {
                        from_ts: bar.ts_open,
                        bars: 1,
                    });
                }
            }
        } else if let Some(finished) = run.take() {
            report.zero_volume_runs.push(finished);
        }
    }
    if let Some(finished) = run.take() {
        report.zero_volume_runs.push(finished);
    }
    report
        .zero_volume_runs
        .sort_by(|a, b| b.bars.cmp(&a.bars).then(a.from_ts.cmp(&b.from_ts)));
    report.zero_volume_runs.truncate(options.max_gaps_listed);
}

/// Outlier detection on adjacent-bar close-to-close moves.
///
/// Only bars exactly one interval apart are compared. A move across a session
/// boundary or a gap is a different animal — it is the overnight move, not a
/// spike — and mixing the two inflates the scale estimate until nothing looks
/// unusual any more.
fn check_spikes(bars: &[Bar], tf: TimeFrame, options: &QaOptions, report: &mut QaReport) {
    let interval = tf.duration_ns();
    let mut moves: Vec<(Ts, f64)> = Vec::new();
    for pair in bars.windows(2) {
        if pair[1].ts_open.0 - pair[0].ts_open.0 != interval {
            continue;
        }
        moves.push((
            pair[1].ts_open,
            pair[1].close.as_points_f64() - pair[0].close.as_points_f64(),
        ));
    }
    if moves.len() < 3 {
        return;
    }

    let mut magnitudes: Vec<f64> = moves.iter().map(|(_, m)| m.abs()).collect();
    magnitudes.sort_by(f64::total_cmp);
    let mad = magnitudes[magnitudes.len() / 2];
    // An all-zero median means more than half the bars did not move at all —
    // common on a thin contract. There is no scale to measure against, so no
    // spike claim can be made honestly.
    if mad <= 0.0 {
        return;
    }
    let sigma = mad * MAD_TO_SIGMA;
    report.robust_sigma_points = Some(sigma);

    let mut spikes: Vec<Spike> = moves
        .into_iter()
        .filter_map(|(ts_open, move_points)| {
            let sigmas = move_points.abs() / sigma;
            (sigmas > options.spike_sigmas).then_some(Spike {
                ts_open,
                move_points,
                sigmas,
            })
        })
        .collect();
    spikes.sort_by(|a, b| {
        b.sigmas
            .total_cmp(&a.sigmas)
            .then(a.ts_open.cmp(&b.ts_open))
    });
    // The total is recorded before truncation: a report that says "20 spikes"
    // when it means "the 20 worst of 340" understates the problem it exists to
    // surface.
    report.spike_count = spikes.len();
    spikes.truncate(options.max_spikes_listed);
    report.spikes = spikes;
}

/// Coverage, out-of-session bars, and daylight boundaries — everything that
/// needs an outside opinion on when the exchange was open.
fn check_against_calendar(
    bars: &[Bar],
    tf: TimeFrame,
    calendar: &Calendar,
    options: &QaOptions,
    report: &mut QaReport,
) {
    let (Some(first), Some(last)) = (report.first_ts_open, report.last_ts_open) else {
        return;
    };
    let interval = tf.duration_ns();

    // Coverage is claimed only over the span actually held: from the first bar
    // to the end of the last one. Counting whole sessions that merely overlap
    // the span would report a complete January as 8 % of a year.
    let window_start = first.0;
    let window_end = last.0 + interval;

    // Bars are ordered, so one cursor walks them alongside the sessions.
    let mut cursor = 0usize;
    let mut expected_total: i64 = 0;
    let mut gaps: Vec<Gap> = Vec::new();
    let mut previous_open_hour: Option<i64> = None;

    // Trading days are attributed from the session open, so the first bar may
    // belong to a session that opened the previous evening. The lookback is
    // four days rather than one so that the previous *trading* day is always
    // in hand across a weekend — without it, a daylight-saving transition on
    // the first Monday of a span has nothing to be compared against. Days
    // before the first bar contribute no expected bars, because every interval
    // is clamped to the window above.
    let first_day = date_of(first);
    let last_day = date_of(last);
    for offset in (days_from_civil(first_day) - 4)..=(days_from_civil(last_day) + 1) {
        let date = civil_from_days(offset);
        let open_hour = match calendar.open_intervals(date).first() {
            Some((start, _)) => start.0.div_euclid(3_600_000_000_000).rem_euclid(24),
            None => continue,
        };

        let mut bars_today = 0usize;
        let mut expected_today: i64 = 0;
        for (start, end) in calendar.open_intervals(date) {
            let start = start.0.max(window_start);
            let end = end.0.min(window_end);
            if end <= start {
                continue;
            }
            // Bar opens are multiples of the interval, so the first one at or
            // after `start` is `ceil(start / interval)`.
            let first_open =
                start.div_euclid(interval) + i64::from(start.rem_euclid(interval) != 0);
            let after_last = end.div_euclid(interval) + i64::from(end.rem_euclid(interval) != 0);
            if after_last <= first_open {
                continue;
            }
            expected_today += after_last - first_open;

            // Skip bars before this interval — they are out of session, and
            // out-of-session bars are found in the separate pass below.
            while cursor < bars.len() && bars[cursor].ts_open.0 < start {
                cursor += 1;
            }
            let mut want = first_open;
            while cursor < bars.len() && bars[cursor].ts_open.0 < end {
                let have = bars[cursor].ts_open.0 / interval;
                if have > want {
                    gaps.push(Gap {
                        from_ts: Ts(want * interval),
                        to_ts: Ts(have * interval),
                        missing_bars: have - want,
                    });
                }
                want = have + 1;
                bars_today += 1;
                cursor += 1;
            }
            if want < after_last {
                gaps.push(Gap {
                    from_ts: Ts(want * interval),
                    to_ts: Ts(after_last * interval),
                    missing_bars: after_last - want,
                });
            }
        }
        expected_total += expected_today;

        if let Some(previous) = previous_open_hour
            && previous != open_hour
            && expected_today > 0
        {
            report.dst_boundaries.push(DstBoundary {
                date,
                previous_open_hour_utc: previous,
                open_hour_utc: open_hour,
                bars: bars_today,
                expected_bars: expected_today,
            });
        }
        previous_open_hour = Some(open_hour);
    }

    report.missing_bars = gaps.iter().map(|g| g.missing_bars).sum();
    // Only the span actually inspected is claimed, so a partial month does not
    // read as 3% coverage of a year.
    report.expected_bars = Some(expected_total);
    gaps.sort_by(|a, b| {
        b.missing_bars
            .cmp(&a.missing_bars)
            .then(a.from_ts.cmp(&b.from_ts))
    });
    gaps.truncate(options.max_gaps_listed);
    report.gaps = gaps;

    // The check that runs backwards: bars the calendar says cannot exist.
    for bar in bars {
        if !calendar.is_open(bar.ts_open) {
            report.unexpected_bar_count += 1;
            if report.unexpected_bars.len() < options.max_unexpected_listed {
                report.unexpected_bars.push(bar.ts_open);
            }
        }
    }
}

/// The vendor's own opinion, read back from every delivery folder.
///
/// Parsed permissively on purpose. CLAUDE.md §5.5 requires
/// `deny_unknown_fields` on *our* configs, where a typo must be fatal; this is
/// someone else's file, and refusing to read it because Databento added a
/// field would turn their release note into our outage.
fn read_conditions(data_dir: &Path, report: &mut QaReport) -> Result<(), QaError> {
    let root = data_dir.join(DELIVERY_DIR);
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(QaError::Io {
                path: root,
                during: "listing delivery support files",
                source,
            });
        }
    };

    // Sorted, so the report is byte-identical across filesystems (§2.2).
    let mut jobs: BTreeMap<String, PathBuf> = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path().join("condition.json");
        if path.is_file() {
            jobs.insert(entry.file_name().to_string_lossy().into_owned(), path);
        }
    }

    for (job_id, path) in jobs {
        let text = std::fs::read_to_string(&path).map_err(|source| QaError::Io {
            path: path.clone(),
            during: "reading a delivery condition file",
            source,
        })?;
        let Ok(parsed) = serde_json::from_str::<Vec<VendorCondition>>(&text) else {
            // A condition file we cannot parse is worth saying so about, but
            // it is not worth failing a QA run over: it is a courtesy file, and
            // the bars are the evidence.
            report
                .condition_sources
                .push(format!("{job_id} (unreadable condition.json)"));
            continue;
        };
        report.condition_sources.push(job_id.clone());
        for entry in parsed {
            if entry.condition != "available" {
                report.conditions.push(ConditionEntry {
                    date: entry.date,
                    condition: entry.condition,
                    job_id: job_id.clone(),
                });
            }
        }
    }
    Ok(())
}

/// The two fields of a vendor condition record this report uses. Everything
/// else the vendor writes is ignored rather than refused.
#[derive(serde::Deserialize)]
struct VendorCondition {
    date: String,
    condition: String,
}

fn utc_string(ts: Ts) -> String {
    let date = date_of(ts);
    let secs = (ts.0 - days_from_civil(date) * 86_400_000_000_000) / 1_000_000_000;
    format!(
        "{date}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

impl core::fmt::Display for QaReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "data QA — {} {}", self.instrument, self.tf)?;
        match (self.first_ts_open, self.last_ts_open) {
            (Some(first), Some(last)) => writeln!(
                f,
                "  span           {} .. {}   ({} bars)",
                utc_string(first),
                utc_string(last),
                self.bars
            )?,
            _ => writeln!(f, "  span           (no bars)")?,
        }
        match &self.calendar_id {
            Some(id) => writeln!(f, "  calendar       {id}")?,
            None => writeln!(
                f,
                "  calendar       none — coverage, out-of-session bars and DST\n\
                 \x20                boundaries are NOT checked"
            )?,
        }

        if let (Some(expected), Some(pct)) = (self.expected_bars, self.coverage_pct()) {
            writeln!(
                f,
                "  coverage       {pct:.3} %   ({} of {expected} expected bars)",
                expected - self.missing_bars
            )?;
        }

        section(f, "gaps inside sessions", self.gaps.len(), || {
            format!("{} bar(s) missing", self.missing_bars)
        })?;
        for gap in &self.gaps {
            writeln!(
                f,
                "    {} .. {}   {} bar(s)",
                utc_string(gap.from_ts),
                utc_string(gap.to_ts),
                gap.missing_bars
            )?;
        }

        if self.unexpected_bar_count > 0 {
            writeln!(
                f,
                "  BARS OUTSIDE ANY SESSION: {} — the calendar and the archive\n\
                 \x20   disagree, and the archive is the evidence. Suspect the calendar first.",
                self.unexpected_bar_count
            )?;
            for ts in &self.unexpected_bars {
                writeln!(f, "    {}", utc_string(*ts))?;
            }
        }

        section(f, "zero-volume runs", self.zero_volume_runs.len(), || {
            format!("{} bar(s) total", self.zero_volume_bars)
        })?;
        for run in &self.zero_volume_runs {
            writeln!(f, "    {}   {} bar(s)", utc_string(run.from_ts), run.bars)?;
        }

        if let Some(sigma) = self.robust_sigma_points {
            writeln!(
                f,
                "  spikes         {} above {:.0} robust sigma (sigma = {sigma:.4} pt){}",
                self.spike_count,
                self.spike_sigmas,
                if self.spike_count > self.spikes.len() {
                    format!(", {} listed", self.spikes.len())
                } else {
                    String::new()
                }
            )?;
        }
        for spike in &self.spikes {
            writeln!(
                f,
                "    {}   {:+.4} pt   {:.1} sigma",
                utc_string(spike.ts_open),
                spike.move_points,
                spike.sigmas
            )?;
        }

        if !self.ohlc_violations.is_empty() {
            writeln!(
                f,
                "  IMPOSSIBLE BARS: {} — these should have been refused at\n\
                 \x20   transcode; their presence is an engine bug, not a data issue",
                self.ohlc_violations.len()
            )?;
            for violation in self.ohlc_violations.iter().take(10) {
                writeln!(
                    f,
                    "    {}   {}",
                    utc_string(violation.ts_open),
                    violation.rule
                )?;
            }
        }

        if !self.dst_boundaries.is_empty() {
            writeln!(f, "  daylight-saving boundaries")?;
            for boundary in &self.dst_boundaries {
                writeln!(
                    f,
                    "    {}   session open moved {:02}:00 -> {:02}:00 UTC   {} of {} bars",
                    boundary.date,
                    boundary.previous_open_hour_utc,
                    boundary.open_hour_utc,
                    boundary.bars,
                    boundary.expected_bars
                )?;
            }
        }

        if self.condition_sources.is_empty() {
            writeln!(
                f,
                "  vendor condition: no condition.json under delivery/ — either\n\
                 \x20   nothing was pulled by this tool, or the support files were lost"
            )?;
        } else if self.conditions.is_empty() {
            writeln!(
                f,
                "  vendor condition: every date reported available ({} job(s))",
                self.condition_sources.len()
            )?;
        } else {
            writeln!(
                f,
                "  VENDOR REPORTED {} DATE(S) NOT AVAILABLE:",
                self.conditions.len()
            )?;
            for entry in self.conditions.iter().take(20) {
                writeln!(
                    f,
                    "    {}   {}   (job {})",
                    entry.date, entry.condition, entry.job_id
                )?;
            }
        }
        Ok(())
    }
}

fn section(
    f: &mut core::fmt::Formatter<'_>,
    label: &str,
    count: usize,
    detail: impl Fn() -> String,
) -> core::fmt::Result {
    if count == 0 {
        writeln!(f, "  {label:<14} none")
    } else {
        writeln!(f, "  {label:<14} {count} listed, {}", detail())
    }
}

/// Exposed for tests and for callers assembling bars by other means.
#[cfg(test)]
fn bar(ts_open: i64, tf: TimeFrame, close: i64, volume: u64) -> Bar {
    Bar {
        instrument: InstrumentId::new("TEST"),
        tf,
        ts_open: Ts(ts_open),
        open: Price::from_points(close),
        high: Price::from_points(close),
        low: Price::from_points(close),
        close: Price::from_points(close),
        volume,
        signal_offset: Price::ZERO,
    }
}

#[cfg(test)]
mod tests;
