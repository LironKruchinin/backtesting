//! Reading the curated store into the shape [`build_roll_table`] wants.
//!
//! Everything here is I/O around the pure roll logic, kept separate so
//! [`roll`](super::roll) stays testable with hand-built fixtures and no
//! filesystem. The CLI calls this; the algorithm does not.
//!
//! Instruments that are not outright contracts of the requested root are
//! **skipped and reported**, not refused. A parent-symbology pull curates
//! every calendar spread that traded (D-0033), so `curated/bars/` legitimately
//! contains `ESH4-ESM4` beside `ESH4`, and `NQH4` beside both. Refusing on
//! them would make a roll table impossible to build from a real archive;
//! skipping them silently would hide a mistyped root. So they come back in
//! [`GatheredSeries::skipped`] and the caller prints them.
//!
//! [`build_roll_table`]: super::build_roll_table

use std::collections::BTreeMap;
use std::path::Path;

use crucible_core::types::{InstrumentId, TimeFrame};

use crate::calendar::Calendar;
use crate::curated::{ParquetBarFeed, list_instruments};

use super::error::ContinuousError;
use super::roll::RollSource;
use super::session::{ContractSeries, series_of};
use super::symbol::{ContractSymbol, DecadeAnchor};

/// What the curated store had to say about one root.
#[derive(Debug, Clone, Default)]
pub struct GatheredSeries {
    /// One series per contract that traded, keyed by delivery.
    pub series: BTreeMap<ContractSymbol, ContractSeries>,
    /// Provenance for every curated file read, in contract order.
    pub sources: Vec<RollSource>,
    /// Curated instruments that were not outright contracts of this root,
    /// with the reason, so a mistyped root is visible rather than silent.
    pub skipped: Vec<(String, String)>,
    /// Calendar whose trading day defined the volume buckets, if one claimed
    /// this root. `None` means buckets are UTC civil days — correct enough to
    /// use, but it weighs a one-hour Sunday sliver as a session (see
    /// `continuous::session`), so a roll table built that way says so.
    pub calendar: Option<String>,
}

/// Loads every curated contract of `root` at `tf`.
///
/// # Errors
/// [`ContinuousError::Curated`] if the curated store cannot be listed or a
/// partition cannot be trusted, and
/// [`ContinuousError::OutOfOrderSegments`] if a partition's bars are not in
/// availability order (which [`ParquetBarFeed`] already refuses, so this is
/// belt and braces).
pub fn gather_series(
    data_dir: &Path,
    root: &str,
    tf: TimeFrame,
    anchor: DecadeAnchor,
) -> Result<GatheredSeries, ContinuousError> {
    let mut out = GatheredSeries::default();
    // Volume buckets are trading days where a calendar claims the root, and
    // UTC civil days otherwise. A CME week has five sessions but six UTC days
    // carrying bars, and the sixth is a one-hour Sunday sliver that a
    // crossover rule would otherwise weigh as a full session (see `session`).
    let calendar = Calendar::for_instrument(&InstrumentId::new(root)).unwrap_or(None);
    out.calendar = calendar.as_ref().map(|c| c.id().to_owned());
    // `list_instruments` returns a sorted Vec, so this walk is deterministic
    // without any further sorting (CLAUDE.md §2.2).
    for name in list_instruments(data_dir, tf)? {
        let symbol = match ContractSymbol::parse_with_anchor(&name, anchor) {
            Ok(symbol) => symbol,
            Err(e) => {
                out.skipped.push((name, e.to_string()));
                continue;
            }
        };
        if symbol.root() != root {
            out.skipped
                .push((name, format!("root {} is not {root}", symbol.root())));
            continue;
        }
        let instrument = InstrumentId::new(&name);
        let mut feed = ParquetBarFeed::open(data_dir, &instrument, tf, None)?;
        for source in feed.sources() {
            out.sources.push(RollSource {
                instrument: name.clone(),
                source_file_path: source.source_file_path.clone(),
                source_file_blake3: source.source_file_blake3.clone(),
                transcoder_version: source.transcoder_version,
                bars: source.rows as u64,
            });
        }
        if let Some(series) = series_of(&mut feed, instrument, tf, calendar.as_ref())? {
            out.series.insert(symbol, series);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuous::adjust::{AdjustedPrice, AdjustmentKind};
    use crate::continuous::feed::ContinuousFeed;
    use crate::continuous::roll::{RollRule, build_roll_table, read_roll_table, write_roll_table};
    use crate::continuous::{NO_EXPIRY_SOURCE, RollTableInput};
    use crate::curated::{BarColumns, PartitionSource, write_partition};
    use crate::testutil::TempDir;
    use crucible_core::types::Price;

    const DAY: i64 = 86_400 * 1_000_000_000;
    const NOON: i64 = 12 * 3_600_000_000_000;

    fn plant(dir: &TempDir, instrument: &str, days: i64) {
        let bars: Vec<(i64, i64, u64)> = (0..days).map(|day| (day, 5000, 10)).collect();
        plant_bars(dir, instrument, &bars);
    }

    /// One flat 1-minute bar per entry: `(utc_day, close_points, volume)`.
    /// Bars open at noon so `avail_ts` stays inside the same UTC day.
    fn plant_bars(dir: &TempDir, instrument: &str, rows: &[(i64, i64, u64)]) {
        let mut bars = BarColumns::default();
        for (day, close, volume) in rows {
            bars.ts_open.push(day * DAY + NOON);
            for column in [
                &mut bars.open,
                &mut bars.high,
                &mut bars.low,
                &mut bars.close,
            ] {
                column.push(close * 1_000_000_000);
            }
            bars.volume.push(*volume);
        }
        write_partition(
            dir.path(),
            PartitionSource {
                instrument: InstrumentId::new(instrument),
                tf: TimeFrame::M1,
                dataset: "GLBX.MDP3".to_owned(),
                vendor_schema: "ohlcv-1m".to_owned(),
                source_file_path: format!("raw/GLBX.MDP3/ohlcv-1m/{instrument}/2024-01.dbn.zst"),
                source_file_blake3: "00".repeat(32),
            },
            "2024-01",
            &bars,
        )
        .expect("write")
        .expect("rows");
    }

    #[test]
    fn only_outright_contracts_of_the_root_are_gathered() {
        let dir = TempDir::new();
        plant(&dir, "ESH4", 2);
        plant(&dir, "ESM4", 3);
        plant(&dir, "ESH4-ESM4", 1);
        plant(&dir, "NQH4", 1);

        let gathered =
            gather_series(dir.path(), "ES", TimeFrame::M1, DecadeAnchor::DEFAULT).expect("gather");
        let names: Vec<String> = gathered.series.keys().map(ToString::to_string).collect();
        assert_eq!(names, vec!["ESH24", "ESM24"]);
        // The archive spelling survives, because it is what names a partition.
        let first = gathered
            .series
            .values()
            .next()
            .expect("at least one series");
        assert_eq!(first.instrument.as_str(), "ESH4");
        assert_eq!(first.sessions.len(), 2);

        let skipped: Vec<&str> = gathered
            .skipped
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(skipped, vec!["ESH4-ESM4", "NQH4"]);
        assert_eq!(gathered.sources.len(), 2);
        assert_eq!(gathered.sources[0].bars, 2);
    }

    #[test]
    fn an_empty_curated_store_gathers_nothing_rather_than_failing() {
        let dir = TempDir::new();
        let gathered =
            gather_series(dir.path(), "ES", TimeFrame::M1, DecadeAnchor::DEFAULT).expect("gather");
        assert!(gathered.series.is_empty());
        assert!(gathered.skipped.is_empty());
    }

    // The whole leg, on a synthetic three-contract archive: curated bars ->
    // gather -> build -> write JSON -> read it back -> replay. Nothing is
    // hand-fed; every number below comes out of the pipeline.
    //
    // The fixture, one bar per UTC day at noon:
    //   ESH4  days 0-3   close 100   volume 100 throughout
    //   ESM4  days 2-6   close 105   volume 50,150,150,150,20
    //   ESU4  days 5-9   close 102   volume 10,200,200,200,200
    //
    // Roll 1 (volume crossover, confirm 1): day 2 has 50 < 100, day 3 has
    //   150 > 100 -> roll on day 3, gap 105 - 100 = +5.
    // Roll 2: ESM4's remaining days are 4,5,6. Day 4 ESU4 has no bar (0),
    //   day 5 has 10 < 150, day 6 has 200 > 20 -> roll on day 6,
    //   gap 102 - 105 = -3.
    // Offsets back-adjusted: ESU4 0, ESM4 -3, ESH4 +2, so the adjusted
    // series is flat at 102 for all ten bars while the tradeable one steps
    // 100 -> 105 -> 102.
    #[test]
    fn a_synthetic_archive_becomes_a_table_on_disk_and_then_a_replay() {
        let dir = TempDir::new();
        plant_bars(
            &dir,
            "ESH4",
            &[(0, 100, 100), (1, 100, 100), (2, 100, 100), (3, 100, 100)],
        );
        plant_bars(
            &dir,
            "ESM4",
            &[
                (2, 105, 50),
                (3, 105, 150),
                (4, 105, 150),
                (5, 105, 150),
                (6, 105, 20),
            ],
        );
        plant_bars(
            &dir,
            "ESU4",
            &[
                (5, 102, 10),
                (6, 102, 200),
                (7, 102, 200),
                (8, 102, 200),
                (9, 102, 200),
            ],
        );

        let gathered =
            gather_series(dir.path(), "ES", TimeFrame::M1, DecadeAnchor::DEFAULT).expect("gather");
        assert_eq!(gathered.series.len(), 3);
        let expiries = BTreeMap::new();
        let table = build_roll_table(&RollTableInput {
            root: "ES",
            tf: TimeFrame::M1,
            rule: RollRule::default(),
            decade_anchor: DecadeAnchor::DEFAULT,
            expiries: &expiries,
            expiry_source: NO_EXPIRY_SOURCE,
            series: &gathered.series,
            sources: &gathered.sources,
        })
        .expect("build");

        assert_eq!(table.contracts, vec!["ESH4", "ESM4", "ESU4"]);
        assert_eq!(table.rows[0].roll_ts.0, 3 * DAY + NOON + 60_000_000_000);
        assert_eq!(table.rows[0].adjustment, Price::from_points(5));
        assert_eq!(table.rows[1].roll_ts.0, 6 * DAY + NOON + 60_000_000_000);
        assert_eq!(table.rows[1].adjustment, Price::from_points(-3));
        assert_eq!(table.sources.len(), 3);

        let path = write_roll_table(dir.path(), &table).expect("write");
        let reloaded = read_roll_table(&path).expect("read");
        assert_eq!(reloaded, table);

        let mut feed =
            ContinuousFeed::open(dir.path(), &reloaded, AdjustmentKind::BackAdjust, None)
                .expect("open");
        let mut contracts = Vec::new();
        let mut adjusted = Vec::new();
        let mut tradeable = Vec::new();
        while let Some(bar) = feed.next_continuous_bar() {
            contracts.push(bar.contract.as_str().to_owned());
            adjusted.push(bar.adjusted.close);
            tradeable.push(bar.tradeable.close);
        }
        assert_eq!(
            contracts,
            vec![
                "ESH4", "ESH4", "ESH4", "ESH4", "ESM4", "ESM4", "ESM4", "ESU4", "ESU4", "ESU4"
            ]
        );
        assert!(
            adjusted
                .iter()
                .all(|p| *p == AdjustedPrice::from_nanos(102_000_000_000)),
            "back-adjustment must remove both roll gaps: {adjusted:?}"
        );
        assert_eq!(
            tradeable.iter().map(|p| p.as_nanos()).collect::<Vec<i64>>(),
            vec![
                100_000_000_000,
                100_000_000_000,
                100_000_000_000,
                100_000_000_000,
                105_000_000_000,
                105_000_000_000,
                105_000_000_000,
                102_000_000_000,
                102_000_000_000,
                102_000_000_000
            ]
        );
    }
}
