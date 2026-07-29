//! `crucible theta-golden` — the golden-raw round-trip gate.
//!
//! Fetches one sample response per endpoint from the running Theta Terminal,
//! keeps the vendor's bytes exactly as delivered, transcodes them to Parquet,
//! reads the Parquet back, and compares it cell by cell against the CSV it came
//! from. It reports the compression ratio it measured.
//!
//! Two things this gate exists to produce, and they are different:
//!
//! 1. **Fidelity.** A round trip that compares only row counts proves almost
//!    nothing; this one re-parses every cell of the original CSV and asserts the
//!    Parquet holds the same value. `docs/THETADATA_PLAN.md` §6 keeps the raw
//!    responses permanently for a few GB precisely so this can be re-run against
//!    a future build and catch a transcode that drifted.
//! 2. **The measured CSV→Parquet ratio.** §7.3 records ~10× as an *assumption*
//!    and says so. T1's ~1.75 TB of CSV is sized against it, so the number
//!    wants measuring rather than believing. This prints what was measured, per
//!    endpoint, and the plan doc is updated from the output rather than from
//!    hope.
//!
//! Exit codes:
//!
//! | code | meaning |
//! |---|---|
//! | 0 | every endpoint round-tripped |
//! | 2 | the build lacks `--features thetadata` |
//! | 4 | the Terminal, validation, or a round trip failed |

/// Exit code when the feature is missing.
#[cfg(not(feature = "thetadata"))]
const EXIT_NO_FEATURE: i32 = 2;

/// Arguments for `theta-golden`.
#[derive(Debug, clap::Args)]
pub struct ThetaGoldenArgs {
    /// Root symbol to sample. VIX is the default because it is the smallest
    /// entitled chain, so the gate costs the least it can.
    #[arg(long, default_value = "VIX")]
    pub root: String,
    /// Sample date, `YYYY-MM-DD`.
    #[arg(long, default_value = "2024-01-02")]
    pub date: String,
    /// Base URL of the Theta Terminal.
    #[arg(long)]
    pub base_url: Option<String>,
    /// Where to keep the golden-raw responses and their Parquet.
    ///
    /// Defaults to `$CRUCIBLE_DATA_DIR/external/thetadata/golden_raw`.
    #[arg(long)]
    pub out_dir: Option<std::path::PathBuf>,
}

#[cfg(not(feature = "thetadata"))]
pub fn run(_args: &ThetaGoldenArgs) -> i32 {
    eprintln!(
        "error: `theta-golden` needs a build with the ThetaData client.\n\
         \n    cargo run -p crucible-cli --features thetadata -- theta-golden\n\
         \n\
         The feature is off by default so that the common path does not compile \
         an HTTP stack it never uses (D-0051)."
    );
    EXIT_NO_FEATURE
}

#[cfg(feature = "thetadata")]
pub use enabled::run;

/// Arguments for `theta-floors`.
#[derive(Debug, clap::Args)]
pub struct ThetaFloorsArgs {
    /// Roots to probe, comma separated.
    #[arg(long, default_value = "SPY,QQQ,IWM,SPX,SPXW,NDX,VIX,RUT,DIA")]
    pub roots: String,
    /// Earliest date the subscription could answer, `YYYY-MM-DD`.
    #[arg(long, default_value = "2012-06-01")]
    pub floor: String,
    /// Latest date to search up to, `YYYY-MM-DD`.
    #[arg(long, default_value = "2026-07-01")]
    pub ceiling: String,
    /// Base URL of the Theta Terminal.
    #[arg(long)]
    pub base_url: Option<String>,
}

#[cfg(not(feature = "thetadata"))]
pub fn run_floors(_args: &ThetaFloorsArgs) -> i32 {
    eprintln!(
        "error: `theta-floors` needs a build with the ThetaData client.\n\
         \n    cargo run -p crucible-cli --features thetadata -- theta-floors"
    );
    EXIT_NO_FEATURE
}

#[cfg(feature = "thetadata")]
pub use enabled::run_floors;

#[cfg(feature = "thetadata")]
mod enabled {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crucible_data::external::thetadata::{
        DEFAULT_BASE_URL, Endpoint, Request, ThetaClient,
        transcode::{ColumnKind, TranscodeSource, parse_eastern_stamp, parse_nano_usd},
        validate::validate,
    };
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use parquet::record::Field;

    use super::{ThetaFloorsArgs, ThetaGoldenArgs};

    const EXIT_OK: i32 = 0;
    const EXIT_FAILED: i32 = 4;

    /// One endpoint's result.
    struct Measured {
        endpoint: Endpoint,
        csv_bytes: u64,
        parquet_bytes: u64,
        rows: u64,
        distinct: u64,
        dup_rate: f64,
        cells_checked: u64,
    }

    pub fn run(args: &ThetaGoldenArgs) -> i32 {
        let data_dir = match crate::pull::data_dir() {
            Ok(dir) => dir,
            Err(message) => {
                eprintln!("error: {message}");
                return EXIT_FAILED;
            }
        };
        let out_dir = args.out_dir.clone().unwrap_or_else(|| {
            data_dir
                .join("external")
                .join("thetadata")
                .join("golden_raw")
        });
        let base_url = args
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());

        let compact = args.date.replace('-', "");
        let client = match ThetaClient::open(&base_url, 8) {
            Ok(client) => client,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_FAILED;
            }
        };

        println!("golden-raw round trip");
        println!("  terminal   {base_url}");
        println!("  root       {}", args.root);
        println!("  date       {}", args.date);
        println!("  out        {}", out_dir.display());
        println!("  pacer      {:?}", client.pacer());
        println!();

        // The endpoints T0 actually fetches, plus the two T1 samples so the
        // ratio is measured on the shape that dominates the storage estimate
        // rather than only on the small one.
        //
        // `interval=1m` is the v3 spelling, established by probing: the
        // millisecond form every v2-era example uses (`interval=60000`) answers
        // `400 Invalid interval`, as do `60` and `1min`. Nothing in the vendor's
        // v3 material states this, and §3.3 is why it had to be probed rather
        // than assumed — an unknown *parameter* is ignored silently, so only a
        // bad *value* announces itself.
        //
        // `greeks/first_order` refuses `expiration=*` outright ("Cannot specify
        // '*' for the date"), so it is sampled against one real expiration
        // discovered from the `eod` response rather than a guessed one.
        let endpoints = [
            (Endpoint::OptionEod, "eod", false, Vec::new()),
            (Endpoint::OptionGreeksEod, "greeks_eod", false, Vec::new()),
            (
                Endpoint::OptionOpenInterest,
                "open_interest",
                false,
                Vec::new(),
            ),
            (
                Endpoint::OptionQuote,
                "quote_1m",
                false,
                vec![("interval".to_owned(), "1m".to_owned())],
            ),
            (
                Endpoint::OptionGreeksFirstOrder,
                "greeks_first_order_1m",
                true,
                vec![("interval".to_owned(), "1m".to_owned())],
            ),
        ];

        let mut measured: Vec<Measured> = Vec::new();
        let mut failures = 0u32;
        let mut an_expiration: Option<String> = None;

        for (endpoint, label, needs_concrete_expiration, extra) in endpoints {
            let expiration = if needs_concrete_expiration {
                match an_expiration.clone() {
                    Some(e) => e,
                    None => {
                        println!("{label:<24} skipped — no expiration discovered from eod");
                        continue;
                    }
                }
            } else {
                "*".to_owned()
            };
            let mut request = Request::new(endpoint.path())
                .with("symbol", args.root.clone())
                .with("expiration", expiration)
                .with("start_date", compact.clone())
                .with("end_date", compact.clone());
            for (k, v) in extra {
                request = request.with(k, v);
            }

            print!("{label:<24} ");
            let body = match client.fetch(&request) {
                Ok(body) => body,
                Err(e) if e.is_no_data() => {
                    println!("472 no data — recorded, not retried");
                    continue;
                }
                Err(e) => {
                    println!("FAILED");
                    eprintln!("  {e}");
                    failures += 1;
                    continue;
                }
            };

            // Cheap and honest: take an expiration from the body we already
            // have rather than guessing one or spending a request to list them.
            if endpoint == Endpoint::OptionEod && an_expiration.is_none() {
                an_expiration = first_expiration(&body);
            }

            match round_trip(
                endpoint, &body, &request, &out_dir, label, &args.root, &args.date,
            ) {
                Ok(m) => {
                    println!(
                        "{:>10} CSV  {:>10} parquet  {:>6.2}x  {} rows / {} distinct  dup {:.3}  {} cells verified",
                        human(m.csv_bytes),
                        human(m.parquet_bytes),
                        ratio(m.csv_bytes, m.parquet_bytes),
                        m.rows,
                        m.distinct,
                        m.dup_rate,
                        m.cells_checked
                    );
                    measured.push(m);
                }
                Err(message) => {
                    println!("FAILED");
                    eprintln!("  {message}");
                    failures += 1;
                }
            }
        }

        println!();
        if measured.is_empty() {
            eprintln!("error: nothing was measured; the gate proves nothing");
            return EXIT_FAILED;
        }

        let csv_total: u64 = measured.iter().map(|m| m.csv_bytes).sum();
        let parquet_total: u64 = measured.iter().map(|m| m.parquet_bytes).sum();
        println!("MEASURED CSV->parquet ratio");
        for m in &measured {
            println!(
                "  {:<38} {:>6.2}x",
                m.endpoint.path(),
                ratio(m.csv_bytes, m.parquet_bytes)
            );
        }
        println!(
            "  {:<38} {:>6.2}x   ({} CSV -> {} parquet)",
            "weighted over everything measured",
            ratio(csv_total, parquet_total),
            human(csv_total),
            human(parquet_total)
        );
        println!("\nRecord this in docs/THETADATA_PLAN.md §7.3, replacing the ~10x assumption.");

        if failures > 0 {
            eprintln!("\n{failures} endpoint(s) failed the round trip");
            return EXIT_FAILED;
        }
        EXIT_OK
    }

    /// Fetch bytes -> validate -> parquet -> read back -> compare every cell.
    fn round_trip(
        endpoint: Endpoint,
        body: &[u8],
        request: &Request,
        out_dir: &std::path::Path,
        label: &str,
        root: &str,
        date: &str,
    ) -> Result<Measured, String> {
        let rendered = request.render();
        let response = validate(endpoint, body, &rendered).map_err(|e| e.to_string())?;

        // Keep the vendor's bytes exactly as delivered. This is the artifact
        // that makes the round trip re-runnable against a future build.
        let raw_dir = out_dir.join(root).join(label);
        std::fs::create_dir_all(&raw_dir)
            .map_err(|e| format!("creating {}: {e}", raw_dir.display()))?;
        let raw_path = raw_dir.join(format!("{date}.csv"));
        std::fs::write(&raw_path, body)
            .map_err(|e| format!("writing {}: {e}", raw_path.display()))?;

        let source = TranscodeSource {
            request: rendered.clone(),
            response_blake3: blake3::hash(body).to_hex().to_string(),
        };
        let parquet_path = raw_dir.join(format!("{date}.parquet"));
        let parquet_bytes = crucible_data::external::thetadata::transcode::write_parquet(
            &response,
            &source,
            &parquet_path,
            &rendered,
        )
        .map_err(|e| e.to_string())?;

        let cells_checked = compare(endpoint, &response, &parquet_path, &rendered)?;

        Ok(Measured {
            endpoint,
            csv_bytes: body.len() as u64,
            parquet_bytes,
            rows: response.report.raw_rows,
            distinct: response.report.distinct_rows,
            dup_rate: response.report.dup_rate(),
            cells_checked,
        })
    }

    /// Reads the Parquet back and asserts every cell equals the CSV's value.
    ///
    /// The comparison re-derives the expected value from the original text
    /// through the same parsers the writer used. That is deliberate: it proves
    /// the *file* holds what the parser produced, which is the half that can
    /// break silently. The parsers themselves are proven separately, against
    /// hand-derived constants, in `transcode`'s unit tests — a round trip that
    /// checked only itself would agree with any consistent bug.
    fn compare(
        endpoint: Endpoint,
        response: &crucible_data::external::thetadata::validate::ValidatedResponse,
        parquet_path: &std::path::Path,
        rendered: &str,
    ) -> Result<u64, String> {
        let file = std::fs::File::open(parquet_path)
            .map_err(|e| format!("opening {}: {e}", parquet_path.display()))?;
        let reader = SerializedFileReader::new(file).map_err(|e| e.to_string())?;
        let mut checked = 0u64;
        let mut rows = reader.get_row_iter(None).map_err(|e| e.to_string())?;

        for (i, expected_row) in response.rows.iter().enumerate() {
            let actual = rows
                .next()
                .ok_or_else(|| format!("parquet ran out of rows at {i}"))?
                .map_err(|e| e.to_string())?;
            let actual: BTreeMap<String, Field> = actual
                .get_column_iter()
                .map(|(name, field)| (name.clone(), field.clone()))
                .collect();

            for column in endpoint.pinned_header() {
                let raw = expected_row
                    .get(&response.index, column)
                    .unwrap_or_default()
                    .trim();
                let field = actual
                    .get(*column)
                    .ok_or_else(|| format!("parquet is missing column {column}"))?;
                if raw.is_empty() {
                    if !matches!(field, Field::Null) {
                        return Err(format!("row {i} column {column}: blank became {field}"));
                    }
                    checked += 1;
                    continue;
                }
                let kind = ColumnKind::of(column)
                    .ok_or_else(|| format!("column {column} has no ColumnKind"))?;
                let ok = match (kind, field) {
                    (ColumnKind::Text, Field::Str(got)) => got == raw,
                    (ColumnKind::Money, Field::Long(got)) => {
                        *got == parse_nano_usd(raw, rendered, expected_row.row)
                            .map_err(|e| e.to_string())?
                    }
                    (ColumnKind::Timestamp, Field::Long(got)) => {
                        *got == parse_eastern_stamp(raw, rendered, expected_row.row)
                            .map_err(|e| e.to_string())?
                    }
                    (ColumnKind::Count, Field::Long(got)) => {
                        *got == raw.parse::<i64>().map_err(|e| e.to_string())?
                    }
                    (ColumnKind::Statistic, Field::Double(got)) => {
                        let want = raw.parse::<f64>().map_err(|e| e.to_string())?;
                        got.to_bits() == want.to_bits()
                    }
                    (kind, field) => {
                        return Err(format!(
                            "row {i} column {column}: {kind:?} was stored as {field}"
                        ));
                    }
                };
                if !ok {
                    return Err(format!(
                        "row {i} column {column}: CSV said `{raw}`, parquet said {field}"
                    ));
                }
                checked += 1;
            }
        }
        if rows.next().is_some() {
            return Err("parquet holds more rows than the response did".to_owned());
        }
        Ok(checked)
    }

    /// First `expiration` value in a response body, as vendor text.
    fn first_expiration(body: &[u8]) -> Option<String> {
        let text = std::str::from_utf8(body).ok()?;
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header = lines.next()?;
        let position = header
            .split(',')
            .position(|c| c.trim().trim_matches('"') == "expiration")?;
        let field = lines
            .next()?
            .split(',')
            .nth(position)?
            .trim()
            .trim_matches('"')
            .to_owned();
        (!field.is_empty()).then_some(field)
    }

    fn ratio(csv: u64, parquet: u64) -> f64 {
        if parquet == 0 {
            return 0.0;
        }
        csv as f64 / parquet as f64
    }

    fn human(bytes: u64) -> String {
        #[expect(
            clippy::cast_precision_loss,
            reason = "display only; a file size that loses precision in f64 is far \
                      beyond anything this vendor serves"
        )]
        let b = bytes as f64;
        if b >= 1_048_576.0 {
            format!("{:.2} MB", b / 1_048_576.0)
        } else if b >= 1024.0 {
            format!("{:.1} kB", b / 1024.0)
        } else {
            format!("{bytes} B")
        }
    }

    /// Bisects each root's `greeks/eod` history floor **over sessions**.
    ///
    /// A request below a root's floor answers HTTP 472 ("No data found for your
    /// request"), an ordinary outcome to record rather than a failure to retry.
    /// That makes the floor a monotone predicate and binary search finds it in
    /// ~14 requests per root instead of the thousands a linear scan needs.
    ///
    /// **It is monotone over sessions and NOT over calendar days**, which the
    /// first version of this got wrong and paid for. A weekend answers 472
    /// exactly like a date below the floor, so a midpoint landing on a Sunday
    /// reads as "nothing here" and drags the lower bound past years of real
    /// history. It reported SPY's floor as 2021-03-22 — a Monday whose
    /// preceding Sunday appeared to confirm it — when SPY in fact returns 4,818
    /// contracts on 2017-01-03. The search therefore indexes into the sessions
    /// the `us_equity_options` calendar lists (D-0058) and never probes a day
    /// the exchange was shut.
    ///
    /// **The answer is verified rather than trusted.** Monotonicity still
    /// assumes no interior holes, so three sessions sampled above the boundary
    /// must also carry data. A root that fails that is reported `UNCERTAIN` and
    /// its number is not banked (§0.4) — a floor nobody checked is exactly the
    /// kind of plausible figure that turns a real gap into an expected 472.
    pub fn run_floors(args: &ThetaFloorsArgs) -> i32 {
        let base_url = args
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        let client = match ThetaClient::open(&base_url, 8) {
            Ok(client) => client,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_FAILED;
            }
        };
        let (Some(low), Some(high)) = (parse_day(&args.floor), parse_day(&args.ceiling)) else {
            eprintln!("error: --floor and --ceiling must be YYYY-MM-DD");
            return EXIT_FAILED;
        };
        if low >= high {
            eprintln!("error: --floor must precede --ceiling");
            return EXIT_FAILED;
        }

        let calendar = match crucible_data::calendar::Calendar::by_id("us_equity_options") {
            Ok(calendar) => calendar,
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_FAILED;
            }
        };
        // Every session in the window, in order. Bisection indexes into this,
        // so a probe can never land on a day the exchange was closed.
        let sessions: Vec<i64> = (low..high)
            .filter(|d| calendar.is_trading_day(crucible_data::ingest::window::civil_from_days(*d)))
            .collect();
        if sessions.len() < 8 {
            eprintln!("error: the window holds too few sessions to search");
            return EXIT_FAILED;
        }

        println!("greeks/eod history floors, bisected over sessions");
        println!("  terminal   {base_url}");
        println!(
            "  window     {} .. {}  ({} sessions)",
            args.floor,
            args.ceiling,
            sessions.len()
        );
        println!();
        println!("  {:<6} {:<12} {:>9}  verdict", "root", "floor", "requests");

        let mut requests_total = 0u32;
        let mut failed = 0u32;
        for root in args
            .roots
            .split(',')
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            let mut requests = 0u32;
            let mut probe = |index: usize, requests: &mut u32| -> Option<bool> {
                *requests += 1;
                requests_total += 1;
                match has_greeks(&client, root, sessions[index]) {
                    Ok(answer) => Some(answer),
                    Err(e) => {
                        eprintln!("  {root}: {e}");
                        None
                    }
                }
            };

            let last = sessions.len() - 1;
            // The search itself lives in `crucible-data` so it can be tested
            // against the planted weekend-eve geometry that broke the first
            // version (D-0057); this loop only supplies the predicate and the
            // reporting. Logic in the CLI is a smell (CLAUDE.md §3).
            let outcome = crucible_data::external::thetadata::validate::first_session_with_data(
                sessions.len(),
                |index| probe(index, &mut requests).ok_or(()),
            );
            let hi = match outcome {
                Err(()) => {
                    failed += 1;
                    continue;
                }
                Ok(None) => {
                    println!(
                        "  {root:<6} {:<12} {requests:>9}  no greeks anywhere in the window",
                        "none"
                    );
                    continue;
                }
                Ok(Some(0)) => {
                    println!(
                        "  {root:<6} {:<12} {requests:>9}  at or below the subscription floor",
                        render_day(sessions[0])
                    );
                    continue;
                }
                Ok(Some(index)) => index,
            };
            let lo = hi - 1;
            let mut broke = false;

            // Verify rather than trust. The loop's invariant already proves
            // `hi` has data and `lo` does not, but it proves nothing about
            // interior holes — and a hole is what would make bisection's whole
            // premise false. Sample three sessions above the boundary.
            let mut holes = Vec::new();
            for quarter in 1..=3usize {
                let at = hi + (last - hi) * quarter / 4;
                if at <= hi || at > last {
                    continue;
                }
                match probe(at, &mut requests) {
                    Some(true) => {}
                    Some(false) => holes.push(render_day(sessions[at])),
                    None => {
                        broke = true;
                        break;
                    }
                }
            }
            if broke {
                failed += 1;
                continue;
            }
            if holes.is_empty() {
                println!(
                    "  {root:<6} {:<12} {requests:>9}  verified; previous session {} answered 472",
                    render_day(sessions[hi]),
                    render_day(sessions[lo])
                );
            } else {
                println!(
                    "  {root:<6} {:<12} {requests:>9}  UNCERTAIN — interior holes at {}; not banked",
                    render_day(sessions[hi]),
                    holes.join(", ")
                );
                failed += 1;
            }
        }

        println!();
        println!("{requests_total} requests total.");
        println!(
            "Record these in docs/THETADATA_PLAN.md §3.4 — a floor is what makes \
             a 472 an expected outcome rather than a gap."
        );
        if failed > 0 {
            eprintln!("{failed} root(s) could not be resolved");
            return EXIT_FAILED;
        }
        EXIT_OK
    }

    /// True when `greeks/eod` returns data for this root on this day.
    ///
    /// 472 is "no data", which is the answer this search is looking for and
    /// not an error. Weekends and holidays also answer 472, so the caller's
    /// bisection lands on *a* date with data rather than necessarily the very
    /// first session — which is why the printed line says "on or before".
    fn has_greeks(client: &ThetaClient, root: &str, epoch_day: i64) -> Result<bool, String> {
        let compact = render_day(epoch_day).replace('-', "");
        let request = Request::new(Endpoint::OptionGreeksEod.path())
            .with("symbol", root)
            .with("expiration", "*")
            .with("start_date", compact.clone())
            .with("end_date", compact);
        match client.fetch(&request) {
            Ok(body) => Ok(body.len() > 64),
            Err(e) if e.is_no_data() => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }

    /// `YYYY-MM-DD` to days since the Unix epoch.
    fn parse_day(text: &str) -> Option<i64> {
        let mut parts = text.split('-');
        let year: i64 = parts.next()?.parse().ok()?;
        let month: u32 = parts.next()?.parse().ok()?;
        let day: u32 = parts.next()?.parse().ok()?;
        Some(crucible_data::ingest::window::days_from_civil(
            crucible_data::calendar::CivilDate { year, month, day },
        ))
    }

    /// Days since the Unix epoch back to `YYYY-MM-DD`.
    fn render_day(epoch_day: i64) -> String {
        let date = crucible_data::ingest::window::civil_from_days(epoch_day);
        format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
    }
    /// Keeps `PathBuf` in scope for the argument type without an unused import.
    const _: Option<PathBuf> = None;
}
