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
