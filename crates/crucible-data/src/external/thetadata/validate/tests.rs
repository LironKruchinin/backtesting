//! §4.5's planted controls, plus the positive cases they are the negative of.
//!
//! Every detector in the parent module appears here twice: once accepting the
//! real vendor shape, and once with a bug deliberately planted in it. The pair
//! matters more than either half. A detector that only ever sees clean data has
//! never been observed to fire, and one that only ever sees broken data may be
//! refusing everything — CLAUDE.md §7 asks for the plant, and every fixture
//! below is derived from a body shape measured against the live Terminal on
//! 2026-07-29 rather than invented.

use super::*;

const EOD_HEADER: &str = "symbol,expiration,strike,right,created,last_trade,open,high,low,close,volume,count,bid_size,bid_exchange,bid,bid_condition,ask_size,ask_exchange,ask,ask_condition";

const GREEKS_EOD_HEADER: &str = "symbol,expiration,strike,right,timestamp,open,high,low,close,volume,count,bid_size,bid_exchange,bid,bid_condition,ask_size,ask_exchange,ask,ask_condition,delta,theta,vega,rho,epsilon,lambda,gamma,vanna,charm,vomma,veta,vera,speed,zomma,color,ultima,d1,d2,dual_delta,dual_gamma,implied_vol,iv_error,underlying_timestamp,underlying_price";

const OI_HEADER: &str = "symbol,expiration,strike,right,timestamp,open_interest";

const STOCK_OHLC_HEADER: &str = "timestamp,open,high,low,close,volume,count,vwap";

const QUOTE_HEADER: &str = "symbol,expiration,strike,right,timestamp,bid_size,bid_exchange,bid,bid_condition,ask_size,ask_exchange,ask,ask_condition";

/// One `eod` row. `created` is the build stamp D-0054 is about.
fn eod_row(strike: &str, right: &str, created: &str, close: &str) -> String {
    format!(
        "SPY,2024-01-19,{strike},{right},{created},2024-01-02T15:30:00.000,4.10,4.55,3.90,{close},1234,56,10,N,4.40,1,12,N,4.50,1"
    )
}

fn oi_row(strike: &str, right: &str, oi: &str) -> String {
    format!("SPY,2024-01-19,{strike},{right},2024-01-02T16:30:00.000,{oi}")
}

/// A `greeks/eod` row; `underlying_price` and `iv_error` carry the sentinel.
fn greeks_row(strike: &str, right: &str, iv_error: &str, underlying: &str) -> String {
    let greeks = "0.51,-0.02,0.11,0.03,0.01,1.2,0.04,0.001,0.002,0.003,0.004,0.005,0.006,0.007,0.008,0.009,0.31,0.29,0.12,0.05";
    format!(
        "SPY,2024-01-19,{strike},{right},2024-01-02T16:30:00.000,4.10,4.55,3.90,4.20,1234,56,10,N,4.40,1,12,N,4.50,1,{greeks},0.1832,{iv_error},2024-01-02T16:30:00.000,{underlying}"
    )
}

fn body(header: &str, rows: &[String]) -> Vec<u8> {
    let mut out = String::from(header);
    for row in rows {
        out.push('\n');
        out.push_str(row);
    }
    out.push('\n');
    out.into_bytes()
}

fn key(strike: &str, right: &str) -> ContractKey {
    ContractKey {
        expiration: "2024-01-19".to_owned(),
        strike: strike.to_owned(),
        right: right.to_owned(),
    }
}

// -------------------------------------------------------------------------
// §4.1 — pinned schemas. Header drift in each of four directions.
// -------------------------------------------------------------------------

// The control that matters most: an `eod` body parsed as `greeks/eod` and the
// reverse. Column 5 is `created` in one and `timestamp` in the other, and `eod`
// carries an extra `last_trade` before the OHLC block, so a positional parser
// would read `open` out of `last_trade` and date every row by a build pass.
// This is the pair that proves the pin is doing that work.
#[test]
fn planted_cross_parse_of_eod_as_greeks_and_the_reverse_is_refused() {
    let eod_body = body(
        EOD_HEADER,
        &[eod_row(
            "470.000",
            "CALL",
            "2024-01-02T18:00:00.000",
            "4.20",
        )],
    );
    let err = validate(Endpoint::OptionGreeksEod, &eod_body, "/x")
        .expect_err("an eod body must not validate as greeks/eod");
    assert!(matches!(err, ThetaError::UnexpectedColumns { .. }), "{err}");

    let greeks_body = body(
        GREEKS_EOD_HEADER,
        &[greeks_row("470.000", "CALL", "0.0021", "472.15")],
    );
    let err = validate(Endpoint::OptionEod, &greeks_body, "/x")
        .expect_err("a greeks/eod body must not validate as eod");
    assert!(matches!(err, ThetaError::UnexpectedColumns { .. }), "{err}");
}

#[test]
fn planted_header_drift_in_each_of_four_directions_is_refused() {
    let row = eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20");

    // 1. A column added.
    let added = format!("{EOD_HEADER},surprise");
    // 2. A column dropped.
    let dropped = EOD_HEADER.replace(",count", "");
    // 3. A column renamed — the silent-killer case, since the row still parses.
    let renamed = EOD_HEADER.replace("created", "build_time");
    // 4. Two columns reordered, with the set unchanged.
    let reordered = EOD_HEADER.replace("open,high", "high,open");

    for (label, header) in [
        ("added", added.as_str()),
        ("dropped", dropped.as_str()),
        ("renamed", renamed.as_str()),
        ("reordered", reordered.as_str()),
    ] {
        match validate(
            Endpoint::OptionEod,
            &body(header, std::slice::from_ref(&row)),
            "/x",
        ) {
            Err(ThetaError::UnexpectedColumns { .. }) => {}
            Err(other) => panic!("{label}: expected UnexpectedColumns, got {other}"),
            Ok(_) => panic!("{label}: header drift must refuse, never widen the pin"),
        }
    }

    // The pin itself still accepts the real header, so the four refusals above
    // are the detector working rather than the detector stuck.
    validate(Endpoint::OptionEod, &body(EOD_HEADER, &[row]), "/x")
        .expect("the live header must still validate");
}

// -------------------------------------------------------------------------
// §4.2 — dedup (D-0054).
// -------------------------------------------------------------------------

// The measured pre-2022 shape: every contract twice, one per build pass,
// market fields byte-identical, only `created` differing. Ratio 2.000.
#[test]
fn the_pre_2022_eod_shape_deduplicates_to_one_row_per_contract() {
    let rows = vec![
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
        eod_row("470.000", "CALL", "2024-01-02T21:00:00.000", "4.20"),
        eod_row("475.000", "PUT", "2024-01-02T18:00:00.000", "1.15"),
        eod_row("475.000", "PUT", "2024-01-02T21:00:00.000", "1.15"),
    ];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x").expect("valid");

    assert_eq!(out.report.raw_rows, 4);
    assert_eq!(out.report.distinct_rows, 2);
    assert!(
        (out.report.dup_rate() - 2.000).abs() < 1e-9,
        "era fingerprint"
    );
    assert_eq!(out.report.n_builds_distribution, BTreeMap::from([(2, 2)]));
    assert_eq!(out.report.identical_pairs, 2);
    assert_eq!(out.report.conflicting_pairs, 0);
    assert_eq!(out.rows.len(), 2);

    // Keep max(created): the final build, and the conservative availability
    // direction (D-0052). First-wins would date the row by whichever pass ran
    // first, asserting the value existed before the vendor settled on it.
    assert_eq!(
        out.rows[0].get(&out.index, "created"),
        Some("2024-01-02T21:00:00.000")
    );
}

// The counterexample that forbids hardcoding two passes: 2020-01-02 carries
// four distinct `created` values while every contract still appears exactly
// twice — contracts are split across passes. A check asserting "two builds per
// file" would refuse this good day; one asserting "two rows per contract"
// happens to pass here and would break elsewhere. Group by contract, count what
// you find, assume nothing.
#[test]
fn four_build_passes_with_two_rows_per_contract_is_accepted_and_recorded() {
    let rows = vec![
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
        eod_row("470.000", "CALL", "2024-01-02T21:00:00.000", "4.20"),
        eod_row("475.000", "PUT", "2024-01-02T19:00:00.000", "1.15"),
        eod_row("475.000", "PUT", "2024-01-02T22:00:00.000", "1.15"),
    ];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x").expect("valid");
    assert_eq!(out.report.distinct_rows, 2);
    assert_eq!(
        out.report.n_builds_distribution,
        BTreeMap::from([(2, 2)]),
        "two builds per contract, even though the file holds four stamps"
    );
}

// The clean post-2022 shape. Ratio 1.000, one build per contract.
#[test]
fn the_post_2022_eod_shape_is_already_distinct() {
    let rows = vec![
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
        eod_row("475.000", "PUT", "2024-01-02T18:00:00.000", "1.15"),
    ];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x").expect("valid");
    assert!((out.report.dup_rate() - 1.000).abs() < 1e-9);
    assert_eq!(out.report.n_builds_distribution, BTreeMap::from([(1, 2)]));
    assert_eq!(out.report.identical_pairs, 0);
}

// PLANTED CONTROL: the same contract with the same `created`. D-0054 explains
// a contract repeating across build passes; it does not explain one build
// emitting a contract twice. Refuse.
#[test]
fn planted_duplicate_contract_with_the_same_build_stamp_refuses_the_file() {
    let rows = vec![
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
    ];
    let err = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x")
        .expect_err("(contract, created) twice is unexplained");
    match err {
        ThetaError::DuplicateRow {
            discriminator: Some((column, ref value)),
            occurrences,
            ..
        } => {
            assert_eq!(column, "created");
            assert_eq!(value, "2024-01-02T18:00:00.000");
            assert_eq!(occurrences, 2);
        }
        other => panic!("expected DuplicateRow with a discriminator, got {other}"),
    }
}

// A revision between build passes: the vendor changed a printed value. Keep
// max(created) still applies, but this is QA signal an operator should see, not
// noise to swallow — so it is counted separately from the identical pairs.
#[test]
fn a_value_that_changed_between_builds_is_kept_late_and_counted_as_conflicting() {
    let rows = vec![
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
        eod_row("470.000", "CALL", "2024-01-02T21:00:00.000", "4.35"),
    ];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x").expect("valid");
    assert_eq!(out.report.conflicting_pairs, 1);
    assert_eq!(out.report.identical_pairs, 0);
    assert_eq!(out.rows[0].get(&out.index, "close"), Some("4.35"));
}

// PLANTED CONTROL: an interval endpoint has no build stamp, so the same
// (contract, minute) twice cannot be deduplicated — collapsing it would discard
// a real observation. This is the `(contract, minute)` distinct check §4.4 asks
// for on T1 golden sample days.
#[test]
fn planted_repeated_contract_minute_on_an_interval_endpoint_refuses_the_file() {
    let quote = |ts: &str| format!("SPY,2024-01-19,470.000,CALL,{ts},10,N,4.40,1,12,N,4.50,1");
    let clean = body(
        QUOTE_HEADER,
        &[quote("20240102093000"), quote("20240102093100")],
    );
    validate(Endpoint::OptionQuote, &clean, "/x").expect("distinct minutes are ordinary");

    let planted = body(
        QUOTE_HEADER,
        &[quote("20240102093000"), quote("20240102093000")],
    );
    let err = validate(Endpoint::OptionQuote, &planted, "/x")
        .expect_err("the same contract-minute twice is a bug");
    match err {
        ThetaError::DuplicateRow {
            discriminator: None,
            ..
        } => {}
        other => panic!("expected a discriminator-free DuplicateRow, got {other}"),
    }
}

// Open interest is NOT affected by the duplication (ratio 1.000 on 2014, 2017,
// 2019 and 2024), but the uniqueness gate applies anyway: the mechanism is a
// build-pipeline artefact and nothing guarantees where it appears next.
#[test]
fn open_interest_is_gated_for_uniqueness_even_though_it_was_never_duplicated() {
    let clean = body(OI_HEADER, &[oi_row("470.000", "CALL", "12345")]);
    let out = validate(Endpoint::OptionOpenInterest, &clean, "/x").expect("valid");
    assert!((out.report.dup_rate() - 1.000).abs() < 1e-9);

    let planted = body(
        OI_HEADER,
        &[
            oi_row("470.000", "CALL", "12345"),
            oi_row("470.000", "CALL", "12345"),
        ],
    );
    let err = validate(Endpoint::OptionOpenInterest, &planted, "/x")
        .expect_err("same contract, same timestamp");
    assert!(matches!(err, ThetaError::DuplicateRow { .. }), "{err}");
}

// -------------------------------------------------------------------------
// §4.3 — the zero sentinel, as a condition and never a row position.
// -------------------------------------------------------------------------

// PLANTED CONTROL: the sentinel mid-session. It was first seen on the 09:30
// row, and keying on the opening minute would mask exactly this — the case that
// would quietly corrupt a feature rather than obviously breaking one.
#[test]
fn planted_mid_day_zero_underlying_is_dropped_by_condition_not_position() {
    let rows = vec![
        greeks_row("470.000", "CALL", "0.0021", "472.15"),
        greeks_row("475.000", "CALL", "0.0018", "0.0"),
        greeks_row("480.000", "CALL", "0.0019", "473.02"),
    ];
    let out = validate(
        Endpoint::OptionGreeksEod,
        &body(GREEKS_EOD_HEADER, &rows),
        "/x",
    )
    .expect("two good rows remain");
    assert_eq!(out.report.sentinel_rows_dropped, 1);
    assert_eq!(out.rows.len(), 2);
    assert!(
        out.rows
            .iter()
            .all(|r| r.get(&out.index, "underlying_price") != Some("0.0")),
        "no zero-underlying row survives"
    );
}

// PLANTED CONTROL: a failed IV solve mid-session. `>=` and not `==`: a fit at
// or beyond 100% error is unusable whether it is the vendor's flag or a genuine
// divergence that landed at 137. Keying on equality would admit the diverged
// fits and reject only the flagged ones, which is precisely backwards.
#[test]
fn planted_iv_error_at_and_beyond_the_sentinel_is_dropped() {
    for planted in ["100", "137.4"] {
        let rows = vec![
            greeks_row("470.000", "CALL", "0.0021", "472.15"),
            greeks_row("475.000", "CALL", planted, "472.15"),
        ];
        let out = validate(
            Endpoint::OptionGreeksEod,
            &body(GREEKS_EOD_HEADER, &rows),
            "/x",
        )
        .expect("one good row remains");
        assert_eq!(
            out.report.sentinel_rows_dropped, 1,
            "iv_error={planted} must be dropped"
        );
    }
}

// The condition must not overfire. VIX rows carry `iv_error = 0.0021` and are
// ordinary data; a detector that ate them would silently delete a whole root.
#[test]
fn small_iv_errors_are_ordinary_data_and_survive() {
    let rows = vec![greeks_row("20.000", "CALL", "0.0021", "18.44")];
    let out = validate(
        Endpoint::OptionGreeksEod,
        &body(GREEKS_EOD_HEADER, &rows),
        "/x",
    )
    .expect("valid");
    assert_eq!(out.report.sentinel_rows_dropped, 0);
    assert_eq!(out.rows.len(), 1);
}

// PLANTED CONTROL: the SPY 2016-01-04 shape — HTTP 200, 390 rows, every field
// zero, while QQQ on the same date returns real prices. Per symbol, so it
// cannot be inferred from the date; refused rather than archived as a quiet day.
#[test]
fn planted_all_zero_ohlc_series_is_refused_rather_than_archived() {
    let zeros: Vec<String> = (0..5)
        .map(|i| format!("2024010209{i:02}00,0.0,0.0,0.0,0.0,0,0,0.0"))
        .collect();
    let err = validate(Endpoint::StockOhlc, &body(STOCK_OHLC_HEADER, &zeros), "/x")
        .expect_err("an all-zero series is the vendor saying \"absent\"");
    match err {
        ThetaError::AllZeroSeries { rows, .. } => assert_eq!(rows, 5),
        other => panic!("expected AllZeroSeries, got {other}"),
    }
}

// REGRESSION, and the reason the gate is scoped rather than global. An option
// `eod` row for a contract that did not trade legitimately carries
// `0.00,0.00,0.00,0.00` with zero volume and a real NBBO beside it — that is
// most of a chain on most days. Measured on the live Terminal: VIX 2024-01-02
// returns 1,058 contracts, 672 of them with zero close, and 614 of those with
// a real bid. An all-zero-OHLC gate applied to `eod` would have refused a good
// day for nine roots at once, so it applies only where OHLC is the whole
// payload and there is no quote to carry the information instead.
#[test]
fn zero_ohlc_on_option_eod_means_did_not_trade_and_is_kept() {
    let untraded = |strike: &str| {
        format!(
            "VIX,2024-09-18,{strike},CALL,2024-01-02T17:25:19.675,\
             2024-01-02T00:00:00.000,0.00,0.00,0.00,0.00,0,0,2435,5,0.25,50,2563,5,1.34,50"
        )
    };
    let rows = vec![untraded("95.000"), untraded("40.000")];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x")
        .expect("an untraded chain is ordinary data, not an absent one");
    assert_eq!(out.rows.len(), 2);
    assert!(!Endpoint::OptionEod.ohlc_is_the_whole_payload());
    assert!(Endpoint::StockOhlc.ohlc_is_the_whole_payload());
}

// A genuinely quiet-but-real series must survive: one nonzero row is enough to
// prove the tape was reaching us. A gate that refused this would delete real
// data from thin roots, which is the opposite failure.
#[test]
fn a_series_with_any_real_print_is_not_the_all_zero_shape() {
    let mut rows: Vec<String> = (0..4)
        .map(|i| format!("2024010209{i:02}00,0.0,0.0,0.0,0.0,0,0,0.0"))
        .collect();
    rows.push("20240102090400,4.10,4.55,3.90,4.20,12,1,4.11".to_owned());
    validate(Endpoint::StockOhlc, &body(STOCK_OHLC_HEADER, &rows), "/x")
        .expect("one real print means the series is real");
}

// A test whose failure mode produces the desired answer is not evidence
// (THETADATA_PLAN §0.4). An empty body must not read as "clean": it has no
// duplication rate to report, and claiming 1.000 would bank a measurement
// nobody made.
#[test]
fn an_empty_body_reports_unmeasured_rather_than_clean() {
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &[]), "/x").expect("valid, empty");
    assert_eq!(out.report.raw_rows, 0);
    assert_eq!(out.report.distinct_rows, 0);
    assert_eq!(
        out.report.dup_rate(),
        0.0,
        "not 1.000 — nothing was measured"
    );
    assert!(out.report.n_builds_distribution.is_empty());
}

// -------------------------------------------------------------------------
// §4.4 — the reconciliation edges.
// -------------------------------------------------------------------------

// Exact parity, the measured case: 4,588/4,588, 7,020/7,020, 2,840/2,840.
#[test]
fn eod_and_greeks_at_exact_parity_reconcile_to_zero_delta() {
    let eod = BTreeSet::from([key("470.000", "CALL"), key("475.000", "PUT")]);
    let greeks = eod.clone();
    let out = reconcile(&eod, Some(&greeks), None, "SPY 2017-06-15").expect("parity");
    assert_eq!(out.eod_and_greeks, 2);
    assert_eq!(out.eod_without_greeks, 0);
}

// A positive delta is coverage asymmetry, NOT a failure: contracts `eod` has
// and greeks lack are covered by the computed surface (D-0053), and this is the
// expected shape below a root's greeks floor.
#[test]
fn contracts_without_greeks_are_coverage_asymmetry_not_a_failure() {
    let eod = BTreeSet::from([key("470.000", "CALL"), key("475.000", "PUT")]);
    let greeks = BTreeSet::from([key("470.000", "CALL")]);
    let out = reconcile(&eod, Some(&greeks), None, "SPY 2013-07-15").expect("asymmetry is fine");
    assert_eq!(out.eod_without_greeks, 1);
}

// PLANTED CONTROL: greeks holding a contract `eod` does not. Impossible under
// the established mechanism — greeks/eod is derived from the same chain — so
// the day is refused rather than recorded with a discrepancy.
#[test]
fn planted_negative_eod_to_greeks_delta_refuses_the_day() {
    let eod = BTreeSet::from([key("470.000", "CALL")]);
    let greeks = BTreeSet::from([key("470.000", "CALL"), key("999.000", "PUT")]);
    let err = reconcile(&eod, Some(&greeks), None, "SPY 2019-01-02")
        .expect_err("greeks cannot exceed eod");
    match err {
        ThetaError::ReconciliationInverted { edge, orphans, .. } => {
            assert_eq!(edge, "greeks/eod ⊆ eod");
            assert_eq!(orphans, 1);
        }
        other => panic!("expected ReconciliationInverted, got {other}"),
    }
}

// `OI ⊆ eod` on every sampled day (SPY 2014-07-02: 2,221 OI against 2,956
// distinct eod). The subset is expected — OI rows exist only where interest
// does — and the coverage fraction is recorded rather than asserted.
#[test]
fn open_interest_is_a_recorded_subset_of_eod() {
    let eod = BTreeSet::from([
        key("470.000", "CALL"),
        key("475.000", "PUT"),
        key("480.000", "CALL"),
        key("485.000", "PUT"),
    ]);
    let oi = BTreeSet::from([key("470.000", "CALL"), key("475.000", "PUT")]);
    let out = reconcile(&eod, None, Some(&oi), "SPY 2014-07-02").expect("subset");
    assert_eq!(out.oi_in_eod, 2);
    assert_eq!(out.eod_without_oi, 2);
    assert_eq!(out.oi_coverage(), Some(0.5));
}

// PLANTED CONTROL: a contract with open interest and no `eod` row inverts the
// mechanism.
#[test]
fn planted_oi_key_absent_from_eod_refuses_the_day() {
    let eod = BTreeSet::from([key("470.000", "CALL")]);
    let oi = BTreeSet::from([key("470.000", "CALL"), key("999.000", "PUT")]);
    let err = reconcile(&eod, None, Some(&oi), "SPY 2014-07-02").expect_err("OI cannot exceed eod");
    match err {
        ThetaError::ReconciliationInverted { edge, example, .. } => {
            assert_eq!(edge, "open_interest ⊆ eod");
            assert!(example.contains("999.000"), "{example}");
        }
        other => panic!("expected ReconciliationInverted, got {other}"),
    }
}

// The same "empty is not clean" trap as above, on the reconciliation side: a
// coverage fraction over an empty chain is unmeasured, not 100%.
#[test]
fn oi_coverage_over_an_empty_chain_is_unmeasured_rather_than_perfect() {
    let empty = BTreeSet::new();
    let out = reconcile(&empty, None, Some(&empty), "SPY 2012-06-01").expect("vacuous");
    assert_eq!(out.oi_coverage(), None, "not Some(1.0)");
}

// Completeness accounting keys on distinct contracts, never raw rows. This is
// the arithmetic that would silently double a GEX-style aggregate for a decade
// of data, and it is worst below the greeks floor where `eod` is the only
// source and has nothing to reconcile against.
#[test]
fn contract_keys_come_from_deduplicated_rows_never_raw_ones() {
    let rows = vec![
        eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20"),
        eod_row("470.000", "CALL", "2024-01-02T21:00:00.000", "4.20"),
        eod_row("475.000", "PUT", "2024-01-02T18:00:00.000", "1.15"),
        eod_row("475.000", "PUT", "2024-01-02T21:00:00.000", "1.15"),
    ];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x").expect("valid");
    assert_eq!(out.report.raw_rows, 4, "the tempting number");
    assert_eq!(out.contract_keys().len(), 2, "the true one");
}

// Stock endpoints have no contract dimension, so contract-key reconciliation
// does not apply to them. Returning an empty set rather than a bogus one keeps
// a caller from reconciling a stock series against an option chain and
// concluding the chain is empty.
#[test]
fn stock_responses_expose_no_contract_keys() {
    let rows = vec!["20240102093000,4.10,4.55,3.90,4.20,12,1,4.11".to_owned()];
    let out = validate(Endpoint::StockOhlc, &body(STOCK_OHLC_HEADER, &rows), "/x").expect("valid");
    assert!(out.contract_keys().is_empty());
}

// A short row is a parse failure, not a row to pad. Padding would put the
// vendor's `volume` into our `count` for every column after the gap.
#[test]
fn a_row_with_the_wrong_field_count_refuses_the_file() {
    let err = validate(
        Endpoint::OptionOpenInterest,
        &body(OI_HEADER, &["SPY,2024-01-19,470.000,CALL".to_owned()]),
        "/x",
    )
    .expect_err("four fields against a six-column pin");
    assert!(matches!(err, ThetaError::MalformedRow { .. }), "{err}");
}

// -------------------------------------------------------------------------
// §4.4 edge 3 — coverage against the session calendar (D-0058).
// -------------------------------------------------------------------------

fn day(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
}

fn us_calendar() -> crate::calendar::TradingDayCalendar {
    crate::calendar::Calendar::by_id("us_equity_options")
        .expect("bundled")
        .into_trading_days()
}

// One ordinary week: five sessions, all held, nothing missing.
#[test]
fn a_complete_week_reports_full_coverage() {
    let held: BTreeSet<CivilDate> = (2..=5).map(|d| day(2024, 1, d)).collect();
    let out = coverage_vs_calendar(&us_calendar(), day(2024, 1, 1), day(2024, 1, 7), &held);
    // 1 Jan 2024 was New Year's Day (Monday) — a holiday, so four sessions.
    assert_eq!(out.expected_sessions, 4);
    assert_eq!(out.present_sessions, 4);
    assert!(out.is_clean());
    assert_eq!(out.coverage(), Some(1.0));
}

// PLANTED CONTROL: a session the vendor did not serve. This is the whole point
// of the edge — `verify` re-hashes bytes and `layout-check` reads paths, and
// neither can see a missing Tuesday.
#[test]
fn planted_missing_session_is_found_and_named() {
    let mut held: BTreeSet<CivilDate> = (2..=5).map(|d| day(2024, 1, d)).collect();
    held.remove(&day(2024, 1, 4));
    let out = coverage_vs_calendar(&us_calendar(), day(2024, 1, 1), day(2024, 1, 7), &held);
    assert_eq!(out.missing, vec![day(2024, 1, 4)]);
    assert_eq!(out.present_sessions, 3);
    assert!(!out.is_clean());
    assert_eq!(out.coverage(), Some(0.75));
}

// The check runs backwards too. Real data is evidence and a calendar is a
// claim, so data on a day the calendar calls closed indicts the calendar —
// exactly how D-0040 falsified CME's published 15:15 CT halt.
#[test]
fn planted_data_on_a_closed_day_is_reported_as_unexpected() {
    let mut held: BTreeSet<CivilDate> = (2..=5).map(|d| day(2024, 1, d)).collect();
    held.insert(day(2024, 1, 1));
    let out = coverage_vs_calendar(&us_calendar(), day(2024, 1, 1), day(2024, 1, 7), &held);
    assert_eq!(out.unexpected, vec![day(2024, 1, 1)]);
    assert!(!out.is_clean(), "a calendar this contradicts needs a look");
}

// Would have fired on the Hurricane Sandy week had the CME calendar been
// borrowed: Globex traded 29-30 October 2012 and the NYSE did not, so those
// two dates must NOT be expected of an equity vendor.
#[test]
fn the_sandy_closure_is_not_expected_of_an_equity_vendor() {
    let held: BTreeSet<CivilDate> = [day(2012, 10, 31), day(2012, 11, 1), day(2012, 11, 2)]
        .into_iter()
        .collect();
    let out = coverage_vs_calendar(&us_calendar(), day(2012, 10, 29), day(2012, 11, 3), &held);
    assert_eq!(out.expected_sessions, 3, "Wed-Fri only");
    assert!(
        out.is_clean(),
        "borrowing the CME calendar would have called Mon+Tue missing: {:?}",
        out.missing
    );
}

// An empty window is unmeasured, not perfect (§0.4 again).
#[test]
fn an_empty_window_reports_unmeasured_coverage() {
    let held = BTreeSet::new();
    let out = coverage_vs_calendar(&us_calendar(), day(2024, 1, 6), day(2024, 1, 8), &held);
    assert_eq!(out.expected_sessions, 0, "a weekend");
    assert_eq!(out.coverage(), None, "not Some(1.0)");
}

// -------------------------------------------------------------------------
// Floor bisection, and the planted control for the geometry that fooled v1.
// -------------------------------------------------------------------------

/// Days since the Unix epoch, so a fixture can talk in real dates.
fn epoch_day(year: i64, month: u32, d: u32) -> i64 {
    crate::ingest::window::days_from_civil(day(year, month, d))
}

/// The sessions a calendar lists over a range — what the search must index.
fn sessions_between(start: CivilDate, end: CivilDate) -> Vec<i64> {
    let cal = us_calendar();
    (crate::ingest::window::days_from_civil(start)..crate::ingest::window::days_from_civil(end))
        .filter(|d| cal.is_trading_day(crate::ingest::window::civil_from_days(*d)))
        .collect()
}

#[test]
fn bisection_finds_the_first_session_with_data() {
    let found = first_session_with_data::<()>(100, |i| Ok(i >= 37)).expect("no failure");
    assert_eq!(found, Some(37));
}

#[test]
fn a_range_entirely_covered_reports_its_first_session() {
    assert_eq!(
        first_session_with_data::<()>(50, |_| Ok(true)).expect("ok"),
        Some(0)
    );
}

#[test]
fn a_range_with_no_data_anywhere_reports_none() {
    assert_eq!(
        first_session_with_data::<()>(50, |_| Ok(false)).expect("ok"),
        None
    );
    assert_eq!(
        first_session_with_data::<()>(0, |_| Ok(true)).expect("ok"),
        None
    );
}

// THE PLANTED CONTROL (D-0057). A synthetic floor whose eve falls on a
// weekend — the exact geometry that made the first implementation confidently
// wrong. The planted floor is Monday 2021-03-22; the day before it is Sunday
// 2021-03-21, which any vendor answers 472 for whatever the floor is.
//
// Searching over CALENDAR DAYS finds a boundary at that Monday no matter where
// the real floor sits, because every weekend looks like the floor. Searching
// over SESSIONS finds the true floor. Both are run here against the same
// planted data, and the assertion is that they disagree — which is what makes
// this a control rather than a restatement.
#[test]
fn planted_floor_whose_eve_is_a_weekend_is_found_only_by_session_bisection() {
    let start = day(2016, 1, 1);
    let end = day(2022, 1, 1);
    // The truth we plant: data begins Tuesday 2017-01-03, exactly where SPY's
    // really does. Everything from that session onward has data.
    let true_floor = epoch_day(2017, 1, 3);

    // --- the correct search: over sessions ---
    let sessions = sessions_between(start, end);
    let found = first_session_with_data::<()>(sessions.len(), |i| Ok(sessions[i] >= true_floor))
        .expect("ok")
        .expect("data exists");
    assert_eq!(
        sessions[found], true_floor,
        "session bisection must land exactly on the planted floor"
    );

    // --- the broken search: over calendar days, weekends answering 472 ---
    let cal = us_calendar();
    let all_days: Vec<i64> = (crate::ingest::window::days_from_civil(start)
        ..crate::ingest::window::days_from_civil(end))
        .collect();
    let found_by_day = first_session_with_data::<()>(all_days.len(), |i| {
        let d = all_days[i];
        // A non-session answers "no data" indistinguishably from below-floor.
        Ok(d >= true_floor && cal.is_trading_day(crate::ingest::window::civil_from_days(d)))
    })
    .expect("ok")
    .expect("data exists");

    assert_ne!(
        all_days[found_by_day], true_floor,
        "if this ever matches, the weekend hazard is gone and this control is \
         no longer testing anything — check why before deleting it"
    );
    // And it is wrong in the specific way observed: it lands on a session whose
    // previous calendar day is a weekend, which is what made the bad answer
    // look self-confirming.
    let landed = crate::ingest::window::civil_from_days(all_days[found_by_day]);
    let eve = crate::ingest::window::civil_from_days(all_days[found_by_day] - 1);
    assert!(cal.is_trading_day(landed), "it lands on a real session");
    assert!(
        !cal.is_trading_day(eve),
        "whose eve is not — the false confirmation"
    );
}

// -------------------------------------------------------------------------
// The zero-OHLC rate: a fingerprint, never a gate (D-0055).
// -------------------------------------------------------------------------

// The measured VIX shape. A high rate is ordinary — most of a chain does not
// trade on most days — and recording it is the only way a *change* in the rate
// ever becomes visible.
#[test]
fn the_zero_ohlc_rate_is_recorded_and_does_not_refuse() {
    let traded = eod_row("470.000", "CALL", "2024-01-02T18:00:00.000", "4.20");
    let untraded = "VIX,2024-09-18,95.000,CALL,2024-01-02T17:25:19.675,\
         2024-01-02T00:00:00.000,0.00,0.00,0.00,0.00,0,0,2435,5,0.25,50,2563,5,1.34,50"
        .to_owned();
    let rows = vec![traded, untraded];
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &rows), "/x")
        .expect("an untraded contract is ordinary data");
    assert_eq!(out.report.zero_ohlc_rows, 1);
    assert_eq!(out.report.zero_ohlc_rate(), Some(0.5));
}

// Endpoints with no OHLC cannot have a rate, and reporting 0.0 would imply one
// was measured.
#[test]
fn an_endpoint_without_ohlc_has_no_zero_ohlc_rate_to_report() {
    let out = validate(
        Endpoint::OptionOpenInterest,
        &body(OI_HEADER, &[oi_row("470.000", "CALL", "12345")]),
        "/x",
    )
    .expect("valid");
    assert_eq!(out.report.zero_ohlc_rows, 0);
    assert_eq!(out.report.zero_ohlc_rate(), Some(0.0));
}

#[test]
fn an_empty_response_has_an_unmeasured_zero_ohlc_rate() {
    let out = validate(Endpoint::OptionEod, &body(EOD_HEADER, &[]), "/x").expect("valid");
    assert_eq!(out.report.zero_ohlc_rate(), None, "not Some(0.0)");
}

// -------------------------------------------------------------------------
// Day-level coverage for the five index roots (D-0059).
// -------------------------------------------------------------------------

// The whole point of the day-level split: SPX has no hours this project can
// state, and its dates are still answerable. Before this, edge 3 simply could
// not be computed for five of the nine roots.
#[test]
fn every_thetadata_root_has_a_day_level_calendar() {
    for root in [
        "SPY", "QQQ", "IWM", "DIA", "SPX", "SPXW", "NDX", "VIX", "RUT",
    ] {
        let found = crate::calendar::Calendar::trading_days_for(
            &crucible_core::types::InstrumentId::new(root),
        )
        .expect("parses")
        .unwrap_or_else(|| panic!("{root} must have a day-level calendar"));
        assert_eq!(found.id(), "us_equity_options", "{root}");
    }
}

// And the barrier still holds: the five index roots get dates and nothing else.
// `for_instrument` is the hour-level door and it stays shut for them, so
// `is_open` and `bars_per_year` remain unreachable rather than wrong.
#[test]
fn index_roots_get_dates_but_never_hours() {
    for root in ["SPX", "SPXW", "NDX", "VIX", "RUT"] {
        assert!(
            crate::calendar::Calendar::for_instrument(&crucible_core::types::InstrumentId::new(
                root
            ))
            .expect("parses")
            .is_none(),
            "{root} must not get an hour-level calendar"
        );
        assert!(
            crate::calendar::Calendar::trading_days_for(&crucible_core::types::InstrumentId::new(
                root
            ))
            .expect("parses")
            .is_some(),
            "{root} must get a day-level one"
        );
    }
}

// A day-level root reconciles against the same session set as an ETF root,
// which is the claim the Cboe citation supports and the reason edge 3 can be
// authoritative rather than provisional.
#[test]
fn an_index_root_and_an_etf_root_expect_the_same_sessions() {
    let spx = crate::calendar::Calendar::trading_days_for(
        &crucible_core::types::InstrumentId::new("SPX"),
    )
    .expect("parses")
    .expect("claimed");
    let spy = crate::calendar::Calendar::trading_days_for(
        &crucible_core::types::InstrumentId::new("SPY"),
    )
    .expect("parses")
    .expect("claimed");

    let held: BTreeSet<CivilDate> = (2..=5).map(|d| day(2024, 1, d)).collect();
    let a = coverage_vs_calendar(&spx, day(2024, 1, 1), day(2024, 1, 7), &held);
    let b = coverage_vs_calendar(&spy, day(2024, 1, 1), day(2024, 1, 7), &held);
    assert_eq!(a, b);
    assert!(a.is_clean());
}
