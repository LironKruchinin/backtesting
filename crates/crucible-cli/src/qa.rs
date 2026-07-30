//! `crucible qa` — inspect the curated store for holes, impossibilities, and
//! anything the vendor already warned us about.
//!
//! `verify` answers "are the archived bytes the bytes we recorded". This
//! answers the question after that one: **the bytes are intact, but is the
//! data any good?** Checksums cannot see a missing Tuesday.
//!
//! Meant to be run after every `transcode`, and after every blitz tranche.
//!
//! ## Exit codes
//!
//! | code | meaning |
//! |---|---|
//! | 0 | the report is clean |
//! | 2 | usage error, or the curated data asked for does not exist |
//! | 4 | the report found something, or curated data could not be read |
//!
//! Findings exit 4 for the same reason `verify` does: a scheduled job that
//! reads "coverage 61 %" as success is worse than no job at all.

use crucible_core::types::{InstrumentId, TimeFrame};
use crucible_data::calendar::Calendar;
use crucible_data::catalog::TsRange;
use crucible_data::curated::CuratedError;
use crucible_data::ingest::range_from_dates;
use crucible_data::ingest::window::parse_civil_date;
use crucible_data::qa::{self, QaError, QaOptions};

use crate::pull::{EXIT_FAILED, EXIT_USAGE, data_dir};

/// Arguments to `crucible qa`.
#[derive(Debug, clap::Args)]
pub struct QaArgs {
    /// Instrument to inspect, e.g. ESH4. Must already be transcoded.
    #[arg(long)]
    pub instrument: String,
    /// Bar interval: 1s 1m 5m 15m 1h 1d.
    #[arg(long, default_value = "1m")]
    pub timeframe: String,
    /// Inclusive UTC start date, YYYY-MM-DD. Omit both dates to inspect
    /// everything held for this instrument.
    #[arg(long)]
    pub start: Option<String>,
    /// Exclusive UTC end date, YYYY-MM-DD.
    #[arg(long)]
    pub end: Option<String>,
    /// Which session calendar defines "expected": `auto` to match the
    /// instrument's root symbol, `none` to skip every calendar-dependent
    /// check, or an explicit calendar id.
    #[arg(long, default_value = "auto")]
    pub calendar: String,
    /// Robust-sigma multiple above which a bar-to-bar move is called a spike.
    #[arg(long, default_value_t = qa::DEFAULT_SPIKE_SIGMAS)]
    pub spike_sigmas: f64,
}

/// Runs the command, returning the process exit code.
pub fn run(args: &QaArgs) -> i32 {
    let tf: TimeFrame = match args.timeframe.parse() {
        Ok(tf) => tf,
        Err(e) => {
            eprintln!("error: --timeframe: {e}");
            return EXIT_USAGE;
        }
    };
    if !(args.spike_sigmas.is_finite() && args.spike_sigmas > 0.0) {
        eprintln!("error: --spike-sigmas must be a positive number");
        return EXIT_USAGE;
    }
    let range = match parse_range(args) {
        Ok(range) => range,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    let dir = match data_dir() {
        Ok(dir) => dir,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };

    let instrument = InstrumentId::new(&args.instrument);
    let calendar = match resolve_calendar(&args.calendar, &instrument) {
        Ok(calendar) => calendar,
        Err(msg) => {
            eprintln!("error: {msg}");
            return EXIT_USAGE;
        }
    };
    if calendar.is_none() && args.calendar != "none" {
        eprintln!(
            "warning: no bundled calendar claims {instrument}, so coverage, out-of-session\n\
             \x20        bars and daylight-saving boundaries cannot be checked. Pass\n\
             \x20        --calendar <id> to name one explicitly."
        );
    }

    let options = QaOptions {
        spike_sigmas: args.spike_sigmas,
        ..QaOptions::default()
    };
    let report = match qa::run(&dir, &instrument, tf, range, calendar.as_ref(), &options) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return match e {
                QaError::Curated(
                    CuratedError::NoCuratedData { .. } | CuratedError::EmptyRange { .. },
                ) => EXIT_USAGE,
                _ => EXIT_FAILED,
            };
        }
    };

    if let (Some(cal), Some(first), Some(last)) =
        (calendar.as_ref(), report.first_ts_open, report.last_ts_open)
    {
        warn_if_before_valid_from(cal, first);
        note_era_boundaries(cal, first, last);
    }
    print!("{report}");
    if report.is_clean() {
        println!("\n  clean.");
        0
    } else {
        println!(
            "\n  Findings above. A gap inside a session is missing data; a bar outside one\n\
             \x20 means the calendar is wrong. Neither is fixed by re-running this command."
        );
        EXIT_FAILED
    }
}

/// A session template describes one era of an exchange. Answering about an
/// earlier instant is not an error — the calendar is total — but it is this
/// era's answer to a different era's question, and a silent one would show up
/// as thousands of phantom gaps rather than as a mistake.
fn warn_if_before_valid_from(calendar: &Calendar, first: crucible_core::types::Ts) {
    let valid_from = calendar.valid_from();
    let first_date = crucible_data::ingest::window::date_of(first);
    if crucible_data::ingest::window::days_from_civil(first_date)
        < crucible_data::ingest::window::days_from_civil(valid_from)
    {
        eprintln!(
            "warning: this span starts {first_date}, before calendar {} describes the\n\
             \x20        exchange ({valid_from}). Sessions before that date had different\n\
             \x20        hours, so the coverage and gaps below are measured against the\n\
             \x20        wrong template.",
            calendar.id()
        );
    }
}

/// Names every session era the inspected span crosses.
///
/// Not a warning: crossing one is legal and the coverage numbers are right on
/// both sides of it. It is printed because a *single* coverage percentage over
/// a span containing two different exchanges is one number describing two
/// things, and a reader who cannot see the boundary has no way to know that
/// (D-0086).
fn note_era_boundaries(
    calendar: &Calendar,
    first: crucible_core::types::Ts,
    last: crucible_core::types::Ts,
) {
    use crucible_data::ingest::window::{date_of, days_from_civil};
    let (from, to) = (
        days_from_civil(date_of(first)),
        days_from_civil(date_of(last)),
    );
    let crossed: Vec<String> = calendar
        .era_starts()
        .into_iter()
        .filter(|start| {
            let day = days_from_civil(*start);
            day > from && day <= to
        })
        .map(|start| start.to_string())
        .collect();
    if !crossed.is_empty() {
        println!(
            "  note: this span crosses {} session era boundary/-ies ({}). The coverage\n\
             \x20       figure below pools sessions of different lengths.",
            crossed.len(),
            crossed.join(", ")
        );
    }
}

fn parse_range(args: &QaArgs) -> Result<Option<TsRange>, String> {
    match (&args.start, &args.end) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) => {
            let s = parse_civil_date(start)
                .ok_or_else(|| format!("--start {start:?} is not a YYYY-MM-DD calendar date"))?;
            let e = parse_civil_date(end)
                .ok_or_else(|| format!("--end {end:?} is not a YYYY-MM-DD calendar date"))?;
            Ok(Some(
                range_from_dates((s.year, s.month, s.day), (e.year, e.month, e.day))
                    .map_err(|err| err.to_string())?,
            ))
        }
        _ => Err(
            "--start and --end go together; give both to bound the inspection, or neither \
             to use everything held"
                .to_owned(),
        ),
    }
}

fn resolve_calendar(choice: &str, instrument: &InstrumentId) -> Result<Option<Calendar>, String> {
    match choice {
        "none" => Ok(None),
        "auto" => Calendar::for_instrument(instrument)
            .map_err(|e| format!("bundled calendar tables are broken: {e}")),
        id => Calendar::by_id(id)
            .map(Some)
            .map_err(|e| format!("--calendar: {e}")),
    }
}
