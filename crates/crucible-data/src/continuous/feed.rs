//! Replaying a stitched series.
//!
//! [`ContinuousFeed::open`] does all the failing, for the reason
//! [`ParquetBarFeed`] does: [`Feed::next_event`] returns `Option`, not
//! `Result`, so a feed has no way to report an error mid-run and must have
//! none left to report. By the time the first bar is yielded the roll table
//! has been validated, every segment's contract has been found in the curated
//! store, the requested window has been checked against the span the table
//! was built over, and the concatenation has been verified strictly
//! increasing in `ts_open`.
//!
//! ## What the engine sees, and what M2 still owes
//!
//! [`Feed::next_event`] hands the engine a [`Bar`] whose four `Price` fields
//! are the **tradeable** ones — the then-front contract's real prices,
//! labelled with the continuous alias (`ES.v.0`) so a single-instrument
//! portfolio sees one instrument across rolls — and whose
//! [`signal_offset`](Bar::signal_offset) carries the segment's back-adjustment
//! (D-0073). Fills, marks and PnL read the prices; indicators read
//! `bar.signal_*()`, which yields an
//! [`AdjustedPrice`](crucible_core::types::AdjustedPrice) and has no way back
//! to `Price`. [`ContinuousFeed::next_continuous_bar`] still yields both views
//! as a [`ContinuousBar`], plus the contract behind them, for callers that
//! want the pair explicitly.
//!
//! That was the adjusted-price channel on `Bar` this file used to name as one
//! of two ways to remove the limitation; the other, a second [`MarketEvent`]
//! variant, was not taken, because everything in the engine that touches a bar
//! touches its tradeable prices and would have had to be revisited to keep
//! doing exactly what it already did.
//!
//! What is **not** here is the other half of the roll story: **a roll is a
//! position event, not a price event.** Rolling an open position pays the
//! spread and the fee twice, and the engine sees none of that — a position
//! held across a roll in this build is carried at no cost. That is M2's, it is
//! stated rather than papered over, and `crucible backtest` prints the roll
//! count with the result so a reader can size the omission.
//!
//! [`ParquetBarFeed`]: crate::curated::ParquetBarFeed
//! [`MarketEvent`]: crucible_core::events::MarketEvent
//! [`Bar`]: crucible_core::events::Bar

use std::path::Path;

use crucible_core::events::{Bar, MarketEvent};
use crucible_core::traits::Feed;
use crucible_core::types::{InstrumentId, Price, TimeFrame, Ts};

use crate::catalog::TsRange;
use crate::curated::{BarColumns, CuratedError, CuratedSource, ParquetBarFeed};

use super::adjust::{AdjustedOhlc, AdjustmentKind, ContinuousBar, back_adjust_offsets};
use super::error::ContinuousError;
use super::roll::RollTable;

/// One contract's stretch of a stitched series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuousSegment {
    /// The contract that was front, as the archive spells it.
    pub contract: InstrumentId,
    /// Additive offset applied to this stretch, nanopoints. Zero on the last
    /// segment under [`AdjustmentKind::BackAdjust`], and everywhere under
    /// [`AdjustmentKind::None`].
    pub offset: Price,
    /// Bars this stretch contributed.
    pub bars: usize,
}

/// Internal segment bookkeeping: where a stretch ends in the concatenation.
#[derive(Debug)]
struct Bounded {
    contract: InstrumentId,
    offset: Price,
    end_row: usize,
}

/// A [`Feed`] over a continuous series: one root, many contracts, stitched
/// on a [`RollTable`].
#[derive(Debug)]
pub struct ContinuousFeed {
    alias: InstrumentId,
    tf: TimeFrame,
    adjustment: AdjustmentKind,
    bars: BarColumns,
    segments: Vec<Bounded>,
    cursor: usize,
    segment_cursor: usize,
    sources: Vec<CuratedSource>,
}

impl ContinuousFeed {
    /// Loads the stitched series for `table`, adjusted as asked, over
    /// `range` (half-open on `ts_open`; `None` means the table's whole span).
    ///
    /// # Errors
    /// [`ContinuousError::MalformedTable`] if the table contradicts itself,
    /// [`ContinuousError::RangeNotCovered`] if the window reaches outside the
    /// bars the table was built from, [`ContinuousError::SegmentMissing`] if
    /// a contract the table names has no curated bars,
    /// [`ContinuousError::OutOfOrderSegments`] if the segments do not
    /// concatenate in time, and [`ContinuousError::EmptySeries`] if the
    /// window holds no bars.
    pub fn open(
        data_dir: &Path,
        table: &RollTable,
        adjustment: AdjustmentKind,
        range: Option<TsRange>,
    ) -> Result<ContinuousFeed, ContinuousError> {
        table.validate()?;
        let tf_ns = table.tf.duration_ns();
        let alias = table.alias();

        // A window reaching outside the table's span is refused rather than
        // trimmed: outside it there is no roll information, so the series
        // would be missing whole contracts and look merely short of data.
        if let Some(requested) = range {
            let covered_end = table.last_ts_open.plus_ns(tf_ns);
            if requested.start_ts() < table.first_ts_open || requested.end_ts() > covered_end {
                return Err(ContinuousError::RangeNotCovered {
                    requested_start_ts: requested.start_ts(),
                    requested_end_ts: requested.end_ts(),
                    covered_first_ts: table.first_ts_open,
                    covered_last_ts: table.last_ts_open,
                });
            }
        }

        let offsets = match adjustment {
            AdjustmentKind::None => vec![Price::ZERO; table.contracts.len()],
            AdjustmentKind::BackAdjust => {
                let gaps: Vec<Price> = table.rows.iter().map(|row| row.adjustment).collect();
                back_adjust_offsets(&gaps)
            }
        };

        let mut bars = BarColumns::default();
        let mut segments: Vec<Bounded> = Vec::new();
        let mut sources: Vec<CuratedSource> = Vec::new();

        for (index, contract) in table.contracts.iter().enumerate() {
            // Segment `index` owns bars whose `avail_ts` is strictly after
            // the previous roll and at most the next one — the half-open
            // convention the roll instant is defined by (see `roll`).
            let lower_avail = index
                .checked_sub(1)
                .and_then(|i| table.rows.get(i))
                .map(|r| r.roll_ts);
            let upper_avail = table.rows.get(index).map(|r| r.roll_ts);
            // avail = ts_open + tf, so (lo, hi] on availability is
            // [lo - tf + 1, hi - tf + 1) on ts_open.
            let mut start = lower_avail.map_or(i64::MIN, |t| t.0 - tf_ns + 1);
            let mut end = upper_avail.map_or(i64::MAX, |t| t.0 - tf_ns + 1);
            if let Some(requested) = range {
                start = start.max(requested.start_ts().0);
                end = end.min(requested.end_ts().0);
            }
            if start >= end {
                continue;
            }
            let window =
                TsRange::new(Ts(start), Ts(end)).map_err(|_| ContinuousError::MalformedTable {
                    detail: format!("segment {index} spans no time"),
                })?;

            let instrument = InstrumentId::new(contract.as_str());
            let mut feed = match ParquetBarFeed::open(data_dir, &instrument, table.tf, Some(window))
            {
                Ok(feed) => feed,
                // Nothing in this slice of an otherwise-present contract:
                // a legitimate outcome at a window boundary.
                Err(CuratedError::EmptyRange { .. }) => continue,
                // The contract itself is absent. Replaying the rest would
                // silently drop this stretch of the series.
                Err(source) => {
                    return Err(ContinuousError::SegmentMissing {
                        contract: contract.clone(),
                        source,
                    });
                }
            };
            sources.extend_from_slice(feed.sources());

            let before = bars.len();
            while let Some(MarketEvent::Bar(bar)) = feed.next_event() {
                if let Some(&prev) = bars.ts_open.last()
                    && bar.ts_open.0 <= prev
                {
                    return Err(ContinuousError::OutOfOrderSegments {
                        contract: contract.clone(),
                        prev: Ts(prev),
                        next: bar.ts_open,
                    });
                }
                bars.ts_open.push(bar.ts_open.0);
                bars.open.push(bar.open.as_nanos());
                bars.high.push(bar.high.as_nanos());
                bars.low.push(bar.low.as_nanos());
                bars.close.push(bar.close.as_nanos());
                bars.volume.push(bar.volume);
            }
            if bars.len() == before {
                continue;
            }
            segments.push(Bounded {
                contract: instrument,
                offset: offsets.get(index).copied().unwrap_or(Price::ZERO),
                end_row: bars.len(),
            });
        }

        if bars.is_empty() {
            return Err(ContinuousError::EmptySeries {
                alias: alias.as_str().to_owned(),
                tf: table.tf,
            });
        }

        Ok(ContinuousFeed {
            alias,
            tf: table.tf,
            adjustment,
            bars,
            segments,
            cursor: 0,
            segment_cursor: 0,
            sources,
        })
    }

    /// The next bar in both spaces at once.
    ///
    /// Shares its cursor with [`Feed::next_event`] — drive a feed with one or
    /// the other, never both.
    pub fn next_continuous_bar(&mut self) -> Option<ContinuousBar> {
        let row = self.cursor;
        let &ts_open = self.bars.ts_open.get(row)?;
        while self
            .segments
            .get(self.segment_cursor)
            .is_some_and(|s| row >= s.end_row)
        {
            self.segment_cursor += 1;
        }
        let segment = self.segments.get(self.segment_cursor)?;
        self.cursor += 1;

        // The two views ride on one bar: the four `Price` fields are the
        // then-front contract's real prices, and `signal_offset` is what
        // shifts them into signal space. Fills and marks read the former;
        // indicators read `signal_*()`, which cannot be converted back.
        let tradeable = Bar {
            instrument: self.alias.clone(),
            tf: self.tf,
            ts_open: Ts(ts_open),
            open: Price::from_nanos(self.bars.open[row]),
            high: Price::from_nanos(self.bars.high[row]),
            low: Price::from_nanos(self.bars.low[row]),
            close: Price::from_nanos(self.bars.close[row]),
            volume: self.bars.volume[row],
            signal_offset: segment.offset,
        };
        Some(ContinuousBar {
            adjusted: AdjustedOhlc::of(&tradeable),
            tradeable,
            contract: segment.contract.clone(),
            offset: segment.offset,
        })
    }

    /// The continuous alias every yielded bar is labelled with (`ES.v.0`).
    #[must_use]
    pub fn alias(&self) -> &InstrumentId {
        &self.alias
    }

    /// The bar interval.
    #[must_use]
    pub fn tf(&self) -> TimeFrame {
        self.tf
    }

    /// How the adjusted series was produced.
    #[must_use]
    pub fn adjustment(&self) -> AdjustmentKind {
        self.adjustment
    }

    /// Total bars loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// Whether the feed holds no bars. Never true for a feed that opened
    /// successfully — `open` refuses an empty result.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// `ts_open` of the first bar.
    #[must_use]
    pub fn first_ts_open(&self) -> Option<Ts> {
        self.bars.ts_open.first().copied().map(Ts)
    }

    /// `ts_open` of the last bar.
    #[must_use]
    pub fn last_ts_open(&self) -> Option<Ts> {
        self.bars.ts_open.last().copied().map(Ts)
    }

    /// The contracts this feed replays, in order, with the offset applied to
    /// each and the bars it contributed.
    #[must_use]
    pub fn segments(&self) -> Vec<ContinuousSegment> {
        let mut out = Vec::with_capacity(self.segments.len());
        let mut start = 0usize;
        for segment in &self.segments {
            out.push(ContinuousSegment {
                contract: segment.contract.clone(),
                offset: segment.offset,
                bars: segment.end_row - start,
            });
            start = segment.end_row;
        }
        out
    }

    /// The curated files behind this replay, with the manifest id of the raw
    /// bytes behind each (D-0013).
    #[must_use]
    pub fn sources(&self) -> &[CuratedSource] {
        &self.sources
    }
}

impl Feed for ContinuousFeed {
    /// Yields the **tradeable** bar. See the module docs for why the adjusted
    /// series cannot travel this way yet.
    fn next_event(&mut self) -> Option<MarketEvent> {
        self.next_continuous_bar()
            .map(|bar| MarketEvent::Bar(bar.tradeable))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuous::roll::{ROLL_TABLE_SCHEMA_VERSION, RollRow, RollRule};
    use crate::curated::{PartitionSource, write_partition};
    use crate::testutil::TempDir;
    use crucible_core::types::ContractSpec;

    const MIN: i64 = 60_000_000_000;
    const DAY: i64 = 86_400 * 1_000_000_000;
    const NOON: i64 = 12 * 3_600_000_000_000;

    /// Writes one curated partition holding one flat bar per day, opening at
    /// noon so `avail_ts` (12:01) stays inside the same UTC day.
    fn plant(dir: &TempDir, instrument: &str, start_day: i64, closes: &[i64]) {
        let mut bars = BarColumns::default();
        for (index, close) in closes.iter().enumerate() {
            let ts_open = (start_day + index as i64) * DAY + NOON;
            let nanos = close * 1_000_000_000;
            bars.ts_open.push(ts_open);
            bars.open.push(nanos);
            bars.high.push(nanos);
            bars.low.push(nanos);
            bars.close.push(nanos);
            bars.volume.push(1);
        }
        write_partition(
            dir.path(),
            PartitionSource {
                instrument: InstrumentId::new(instrument),
                tf: TimeFrame::M1,
                dataset: "GLBX.MDP3".to_owned(),
                vendor_schema: "ohlcv-1m".to_owned(),
                source_file_path: format!("raw/GLBX.MDP3/ohlcv-1m/{instrument}/fixture.dbn.zst"),
                source_file_blake3: "00".repeat(32),
            },
            "fixture",
            &bars,
        )
        .expect("write")
        .expect("rows");
    }

    /// A table rolling ESH4 -> ESM4 -> ESU4 at the given days, with the given
    /// gaps in whole points.
    fn table(rolls: &[(&str, &str, i64, i64)], first_day: i64, last_day: i64) -> RollTable {
        let mut contracts = vec![rolls[0].0.to_owned()];
        let mut rows = Vec::new();
        for (from, to, day, gap) in rolls {
            contracts.push((*to).to_owned());
            rows.push(RollRow {
                from_contract: (*from).to_owned(),
                to_contract: (*to).to_owned(),
                roll_ts: Ts(day * DAY + NOON + MIN),
                adjustment: Price::from_points(*gap),
            });
        }
        RollTable {
            schema_version: ROLL_TABLE_SCHEMA_VERSION,
            root: "ES".to_owned(),
            tf: TimeFrame::M1,
            rule: RollRule::default(),
            decade_anchor: 2025,
            expiry_source: "none".to_owned(),
            first_ts_open: Ts(first_day * DAY + NOON),
            last_ts_open: Ts(last_day * DAY + NOON),
            contracts,
            sources: Vec::new(),
            rows,
        }
    }

    fn drain(feed: &mut ContinuousFeed) -> Vec<ContinuousBar> {
        std::iter::from_fn(|| feed.next_continuous_bar()).collect()
    }

    /// Fixture: ESH4 on days 0–2 at 100, ESM4 on days 1–4 at 105, ESU4 on
    /// days 3–5 at 102. Rolls on day 1 (gap +5) and day 3 (gap −3).
    fn three_contract_archive() -> (TempDir, RollTable) {
        let dir = TempDir::new();
        plant(&dir, "ESH4", 0, &[100, 100, 100]);
        plant(&dir, "ESM4", 1, &[105, 105, 105, 105]);
        plant(&dir, "ESU4", 3, &[102, 102, 102]);
        let table = table(&[("ESH4", "ESM4", 1, 5), ("ESM4", "ESU4", 3, -3)], 0, 5);
        (dir, table)
    }

    // The roll instant is a strict boundary: the bar available exactly at
    // roll_ts is the bar the decision was made from, and it belongs to the
    // OLD contract. Day 1's ESH4 bar is available at day 1 12:01, which is
    // roll_ts — so ESH4 owns days 0 and 1, and ESM4 starts on day 2.
    #[test]
    fn segments_split_strictly_after_the_roll_instant() {
        let (dir, table) = three_contract_archive();
        let mut feed = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, None)
            .expect("open");
        let bars = drain(&mut feed);
        let contracts: Vec<&str> = bars.iter().map(|b| b.contract.as_str()).collect();
        assert_eq!(
            contracts,
            vec!["ESH4", "ESH4", "ESM4", "ESM4", "ESU4", "ESU4"],
            "the deciding bar stays on the old contract"
        );
        let days: Vec<i64> = bars
            .iter()
            .map(|b| (b.tradeable.ts_open.0 - NOON) / DAY)
            .collect();
        assert_eq!(days, vec![0, 1, 2, 3, 4, 5]);
    }

    // Hand-derived from the module arithmetic in `adjust`:
    //   gaps +5 then -3
    //   offset(ESU4) = 0, offset(ESM4) = -3, offset(ESH4) = +2
    // so ESH4's 100 becomes 102, ESM4's 105 becomes 102, ESU4's 102 stays.
    // The adjusted series is flat at 102 across both rolls; the tradeable one
    // jumps, which is exactly the difference the two types exist to keep.
    #[test]
    fn back_adjustment_flattens_the_roll_gaps_and_leaves_the_last_segment_alone() {
        let (dir, table) = three_contract_archive();
        let mut feed = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, None)
            .expect("open");
        let bars = drain(&mut feed);
        for bar in &bars {
            assert_eq!(
                bar.adjusted.close,
                crate::continuous::AdjustedPrice::from_nanos(102_000_000_000),
                "{} at {}",
                bar.contract,
                bar.tradeable.ts_open
            );
        }
        let tradeable: Vec<i64> = bars.iter().map(|b| b.tradeable.close.as_nanos()).collect();
        assert_eq!(
            tradeable,
            vec![
                100_000_000_000,
                100_000_000_000,
                105_000_000_000,
                105_000_000_000,
                102_000_000_000,
                102_000_000_000
            ]
        );
        assert_eq!(
            bars[5].offset,
            Price::ZERO,
            "the newest segment is untouched"
        );
        assert_eq!(bars[0].offset, Price::from_points(2));
    }

    #[test]
    fn adjustment_none_leaves_every_segment_at_the_traded_price() {
        let (dir, table) = three_contract_archive();
        let mut feed =
            ContinuousFeed::open(dir.path(), &table, AdjustmentKind::None, None).expect("open");
        for bar in drain(&mut feed) {
            assert_eq!(bar.offset, Price::ZERO);
            assert_eq!(
                bar.adjusted.close.as_nanos(),
                bar.tradeable.close.as_nanos()
            );
        }
    }

    // THE negative control (CLAUDE.md §7). The classic bug is PnL computed on
    // back-adjusted prices. The compile_fail doctest on `ContinuousBar` proves
    // the type system refuses the call; this proves the bug would have
    // mattered — that the two series really do disagree by the cumulative
    // roll gap, so a detector that never fires is not what is being shipped.
    #[test]
    fn pnl_on_adjusted_prices_would_be_wrong_by_the_cumulative_roll_gap() {
        let (dir, table) = three_contract_archive();
        let mut feed = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, None)
            .expect("open");
        let bars = drain(&mut feed);
        let spec = ContractSpec {
            instrument: InstrumentId::new("ES.v.0"),
            tick: Price::from_nanos(250_000_000),
            point_value_usd: 50,
        };

        // Buy on the first bar, sell on the last, one contract. The truth:
        // bought at 100, sold at 102, +2 points, $100.
        let truthful = spec.pnl_nano_usd(bars[5].tradeable.close - bars[0].tradeable.close, 1);
        assert_eq!(truthful, 100_000_000_000);

        // What the bug computes: adjusted 102 minus adjusted 102 = nothing.
        // Reaching the number at all takes an explicit `as_nanos()` on each
        // side and a hand-built `Price` — the loudness is the point.
        let adjusted_delta = Price::from_nanos(
            bars[5].adjusted.close.as_nanos() - bars[0].adjusted.close.as_nanos(),
        );
        let corrupted = spec.pnl_nano_usd(adjusted_delta, 1);
        assert_eq!(corrupted, 0);
        assert_ne!(
            corrupted, truthful,
            "if these ever agree the fixture has stopped exercising the bug"
        );
    }

    // The engine seam: one instrument for the whole run, tradeable prices,
    // nondecreasing availability.
    #[test]
    fn the_feed_yields_tradeable_bars_under_one_alias() {
        let (dir, table) = three_contract_archive();
        let mut feed = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, None)
            .expect("open");
        let mut previous: Option<Ts> = None;
        let mut count = 0;
        while let Some(MarketEvent::Bar(bar)) = feed.next_event() {
            assert_eq!(bar.instrument, InstrumentId::new("ES.v.0"));
            if let Some(prev) = previous {
                assert!(bar.avail_ts() > prev, "availability must increase");
            }
            previous = Some(bar.avail_ts());
            count += 1;
        }
        assert_eq!(count, 6);
    }

    #[test]
    fn segments_report_their_contract_offset_and_bar_count() {
        let (dir, table) = three_contract_archive();
        let feed = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, None)
            .expect("open");
        let segments = feed.segments();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].contract.as_str(), "ESH4");
        assert_eq!(segments[0].offset, Price::from_points(2));
        assert_eq!(segments[0].bars, 2);
        assert_eq!(segments[2].offset, Price::ZERO);
        assert_eq!(feed.sources().len(), 3);
    }

    #[test]
    fn a_window_inside_the_table_replays_only_that_window() {
        let (dir, table) = three_contract_archive();
        let range = TsRange::new(Ts(2 * DAY + NOON), Ts(4 * DAY + NOON)).expect("range");
        let mut feed =
            ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, Some(range))
                .expect("open");
        let bars = drain(&mut feed);
        let contracts: Vec<&str> = bars.iter().map(|b| b.contract.as_str()).collect();
        assert_eq!(contracts, vec!["ESM4", "ESM4"]);
    }

    // ------------------------------------------------- negative controls

    #[test]
    fn a_window_outside_the_tables_span_refuses_rather_than_going_quiet() {
        let (dir, table) = three_contract_archive();
        let range = TsRange::new(Ts(-10 * DAY), Ts(4 * DAY + NOON)).expect("range");
        let err = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, Some(range))
            .expect_err("must refuse a window the table cannot describe");
        assert!(
            matches!(err, ContinuousError::RangeNotCovered { .. }),
            "{err}"
        );
    }

    #[test]
    fn a_contract_the_table_names_but_the_archive_lacks_refuses() {
        let dir = TempDir::new();
        plant(&dir, "ESH4", 0, &[100, 100]);
        // ESM4 is named by the table and never transcoded.
        let table = table(&[("ESH4", "ESM4", 0, 5)], 0, 3);
        let err = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, None)
            .expect_err("must refuse to drop a segment");
        assert!(
            matches!(&err, ContinuousError::SegmentMissing { contract, .. } if contract == "ESM4"),
            "{err}"
        );
    }

    #[test]
    fn a_table_whose_contracts_hold_no_bars_in_range_refuses() {
        let dir = TempDir::new();
        plant(&dir, "ESH4", 0, &[100]);
        let mut table = table(&[("ESH4", "ESM4", 0, 5)], 0, 0);
        table.contracts.pop();
        table.rows.clear();
        // The table's span is exactly the one bar at NOON; this sliver sits
        // inside that span and still holds no bar.
        let range = TsRange::new(Ts(NOON + 1), Ts(NOON + 2)).expect("range");
        let err = ContinuousFeed::open(dir.path(), &table, AdjustmentKind::BackAdjust, Some(range))
            .expect_err("must refuse an empty replay");
        assert!(
            matches!(
                err,
                ContinuousError::EmptySeries { .. } | ContinuousError::RangeNotCovered { .. }
            ),
            "{err}"
        );
    }
}
