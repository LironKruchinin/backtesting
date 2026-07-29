//! Transcode fidelity: the parsers that turn vendor text into typed columns,
//! and the round trip that proves nothing was lost on the way.

use super::*;
use crate::testutil::TempDir;
use parquet::file::reader::{FileReader, SerializedFileReader};

const EOD_HEADER: &str = "symbol,expiration,strike,right,created,last_trade,open,high,low,close,volume,count,bid_size,bid_exchange,bid,bid_condition,ask_size,ask_exchange,ask,ask_condition";

/// A row in the shape the live Terminal actually returned on 2026-07-29 —
/// quoted strings, three-decimal strike, ISO-8601 Eastern stamps.
fn vix_row(strike: &str, close: &str) -> String {
    format!(
        "\"VIX\",\"2024-09-18\",{strike},\"CALL\",2024-01-02T17:25:19.675,\
         2024-01-02T15:12:32.213,1.96,1.96,1.96,{close},50,9,1063,5,1.84,50,702,5,1.96,50"
    )
}

fn body(rows: &[String]) -> Vec<u8> {
    let mut out = String::from(EOD_HEADER);
    for row in rows {
        out.push('\n');
        out.push_str(row);
    }
    out.push('\n');
    out.into_bytes()
}

// -------------------------------------------------------------------------
// Money: integers all the way in.
// -------------------------------------------------------------------------

#[test]
fn prices_parse_to_exact_nano_usd_without_touching_f64() {
    // 1.96 is not representable in binary floating point. Through f64 it is
    // 1.959999999999999964..., and a naive `(x * 1e9) as i64` yields
    // 1_959_999_999. The integer path gives the exact figure.
    assert_eq!(parse_nano_usd("1.96", "/x", 1).expect("ok"), 1_960_000_000);
    assert_eq!(parse_nano_usd("0.00", "/x", 1).expect("ok"), 0);
    assert_eq!(
        parse_nano_usd("95.000", "/x", 1).expect("ok"),
        95_000_000_000
    );
    assert_eq!(
        parse_nano_usd("4700.5", "/x", 1).expect("ok"),
        4_700_500_000_000
    );
    assert_eq!(parse_nano_usd("-0.25", "/x", 1).expect("ok"), -250_000_000);
    assert_eq!(parse_nano_usd(".5", "/x", 1).expect("ok"), 500_000_000);
}

#[test]
fn a_price_that_is_not_a_decimal_number_refuses_the_row() {
    for bad in ["", "1.2.3", "abc", "1,960", "1e9", "0x10"] {
        parse_nano_usd(bad, "/x", 7).expect_err(&format!("`{bad}` must not parse"));
    }
}

#[test]
fn a_price_carrying_more_than_nano_precision_refuses_rather_than_rounds() {
    // Silently dropping the tenth decimal is how a price series acquires an
    // error nobody can trace. There is no such value in this feed today.
    parse_nano_usd("1.0123456789", "/x", 1).expect_err("ten decimals");
}

// -------------------------------------------------------------------------
// Timestamps: Eastern wall clock, through real timezone rules (D-0052).
// -------------------------------------------------------------------------

#[test]
fn an_eastern_stamp_becomes_utc_nanoseconds() {
    // 2024-01-02 is EST (UTC-5), so 17:25:19.675 ET is 22:25:19.675 UTC.
    // Unix days to 2024-01-02 = 19724; 19724 * 86400 = 1_704_153_600 s at
    // midnight UTC, plus 22h25m19.675s = 80_719.675 s.
    let ts = parse_eastern_stamp("2024-01-02T17:25:19.675", "/x", 1).expect("ok");
    assert_eq!(ts, (1_704_153_600 + 80_719) * 1_000_000_000 + 675_000_000);
}

#[test]
fn a_summer_stamp_uses_edt_rather_than_a_fixed_offset() {
    // 2024-07-02 is EDT (UTC-4), so 12:00 ET is 16:00 UTC. A hardcoded -5
    // would put this an hour early — the bug chrono-tz exists to prevent.
    let ts = parse_eastern_stamp("2024-07-02T12:00:00.000", "/x", 1).expect("ok");
    let utc_midnight = 19_906i64 * 86_400;
    assert_eq!(ts, (utc_midnight + 16 * 3600) * 1_000_000_000);
}

// Fractional seconds are left-aligned. `.5` is half a second, not 5ns — and
// getting this backwards would shift a stamp by 499_999_995 nanoseconds while
// still parsing cleanly.
#[test]
fn fractional_seconds_are_left_aligned() {
    let with_millis = parse_eastern_stamp("2024-01-02T12:00:00.5", "/x", 1).expect("ok");
    let whole = parse_eastern_stamp("2024-01-02T12:00:00", "/x", 1).expect("ok");
    assert_eq!(with_millis - whole, 500_000_000);
}

// D-0052: the spring-forward gap does not exist and the fall-back hour is
// ambiguous. Both refuse. Resolving an ambiguous stamp to the earlier candidate
// asserts information existed an hour before it may have — lookahead
// manufactured in the module whose job is preventing it.
#[test]
fn planted_nonexistent_and_ambiguous_eastern_stamps_are_refused() {
    // 2024-03-10 02:30 ET never happened.
    let err = parse_eastern_stamp("2024-03-10T02:30:00.000", "/x", 3)
        .expect_err("the spring-forward gap");
    assert!(matches!(err, ThetaError::Timestamp { .. }), "{err}");

    // 2024-11-03 01:30 ET happened twice.
    let err =
        parse_eastern_stamp("2024-11-03T01:30:00.000", "/x", 4).expect_err("the fall-back hour");
    assert!(matches!(err, ThetaError::Timestamp { .. }), "{err}");
}

#[test]
fn a_malformed_stamp_refuses_the_row_rather_than_defaulting_to_epoch() {
    for bad in [
        "2024-01-02",
        "2024-01-02 17:25:19",
        "not-a-time",
        "2024-13-02T00:00:00.000",
        "2024-01-02T25:00:00.000",
    ] {
        parse_eastern_stamp(bad, "/x", 1).expect_err(&format!("`{bad}` must not parse"));
    }
}

// -------------------------------------------------------------------------
// Column typing.
// -------------------------------------------------------------------------

// Every column of every pinned header must be classified. An unclassified one
// would otherwise have to default, and defaulting to `Statistic` would put a
// price into f64 — the exact boundary §2.3 draws.
#[test]
fn every_pinned_column_of_every_endpoint_is_classified() {
    for endpoint in [
        Endpoint::OptionEod,
        Endpoint::OptionGreeksEod,
        Endpoint::OptionOpenInterest,
        Endpoint::OptionQuote,
        Endpoint::OptionOhlc,
        Endpoint::OptionGreeksFirstOrder,
        Endpoint::StockOhlc,
        Endpoint::StockQuote,
    ] {
        for column in endpoint.pinned_header() {
            assert!(
                ColumnKind::of(column).is_some(),
                "{}: column `{column}` has no ColumnKind",
                endpoint.path()
            );
        }
        message_type(endpoint).expect("a schema for every endpoint");
    }
}

// The typing that matters most, stated as an assertion rather than a comment:
// money never lands in `f64`, and statistics never land in the integer path.
#[test]
fn money_is_integer_and_greeks_are_float() {
    for money in ["strike", "open", "close", "bid", "ask", "underlying_price"] {
        assert_eq!(ColumnKind::of(money), Some(ColumnKind::Money), "{money}");
        assert_eq!(
            ColumnKind::of(money).expect("classified").parquet_type(),
            "INT64"
        );
    }
    for stat in ["delta", "gamma", "implied_vol", "iv_error"] {
        assert_eq!(ColumnKind::of(stat), Some(ColumnKind::Statistic), "{stat}");
        assert_eq!(
            ColumnKind::of(stat).expect("classified").parquet_type(),
            "DOUBLE"
        );
    }
    // `created` and `timestamp` are both stamps, which is what makes reading
    // one as the other a type-safe mistake rather than a silent one.
    assert_eq!(ColumnKind::of("created"), Some(ColumnKind::Timestamp));
    assert_eq!(ColumnKind::of("timestamp"), Some(ColumnKind::Timestamp));
    assert_eq!(ColumnKind::of("last_trade"), Some(ColumnKind::Timestamp));
    // And an unknown column is unclassified rather than guessed.
    assert_eq!(ColumnKind::of("surprise"), None);
}

// -------------------------------------------------------------------------
// The round trip.
// -------------------------------------------------------------------------

#[test]
fn a_response_round_trips_to_parquet_with_its_provenance_in_the_footer() {
    let dir = TempDir::new();
    let raw = body(&[vix_row("95.000", "1.96"), vix_row("40.000", "0.71")]);
    let response = crate::external::thetadata::validate::validate(
        Endpoint::OptionEod,
        &raw,
        "/option/history/eod",
    )
    .expect("valid");

    let source = TranscodeSource {
        request: "/option/history/eod?symbol=VIX&expiration=*&start_date=20240102".to_owned(),
        response_blake3: blake3::hash(&raw).to_hex().to_string(),
    };
    let destination = dir.path().join("vix").join("2024-01-02.parquet");
    let bytes =
        write_parquet(&response, &source, &destination, "/option/history/eod").expect("write");

    assert!(bytes > 0);
    assert!(destination.exists(), "placed at its real name");
    assert!(
        !destination
            .with_file_name(".2024-01-02.parquet.partial")
            .exists(),
        "the temporary is renamed away, never left behind"
    );

    let file = std::fs::File::open(&destination).expect("open");
    let reader = SerializedFileReader::new(file).expect("read");
    let metadata = reader.metadata().file_metadata();
    assert_eq!(metadata.num_rows(), 2, "two contracts in, two rows out");

    let footer: std::collections::BTreeMap<String, String> = metadata
        .key_value_metadata()
        .expect("footer")
        .iter()
        .filter_map(|kv| kv.value.clone().map(|v| (kv.key.clone(), v)))
        .collect();
    assert_eq!(footer["endpoint"], "/option/history/eod");
    assert_eq!(footer["request"], source.request);
    assert_eq!(footer["response_blake3"], source.response_blake3);
    assert_eq!(footer["raw_rows"], "2");
    assert_eq!(footer["distinct_rows"], "2");
    assert_eq!(footer["dup_rate"], "1.000");
    assert_eq!(
        footer["theta_schema_version"],
        THETA_CURATED_SCHEMA_VERSION.to_string()
    );
}

// A file must never be observable at its real path while half-written: a reader
// that opened one would get a truncated footer and a confusing error, and the
// inventory would have no idea. Write to a sibling, then rename.
#[test]
fn the_written_file_is_placed_by_rename_from_a_sibling() {
    let destination = Path::new("/data/external/thetadata/options/SPY/eod/2024-01-02.parquet");
    let temp = temp_sibling(destination);
    assert_eq!(temp.parent(), destination.parent(), "same directory");
    assert_ne!(temp, destination);
    assert!(
        temp.file_name()
            .expect("named")
            .to_string_lossy()
            .ends_with(".partial")
    );
}

// Absent is a null, never a zero. This vendor has already proved it does not
// distinguish them (§4.3), so ours must — a blank `last_trade` for a contract
// that never traded must not read back as 1970-01-01.
#[test]
fn a_blank_field_is_written_as_null_rather_than_a_zero() {
    let dir = TempDir::new();
    let blank_last_trade = "\"VIX\",\"2024-09-18\",95.000,\"CALL\",\
         2024-01-02T17:25:19.675,,0.00,0.00,0.00,0.00,0,0,2435,5,0.25,50,2563,5,1.34,50";
    let raw = body(&[blank_last_trade.to_owned()]);
    let response = crate::external::thetadata::validate::validate(
        Endpoint::OptionEod,
        &raw,
        "/option/history/eod",
    )
    .expect("valid");
    let source = TranscodeSource {
        request: "/option/history/eod?symbol=VIX".to_owned(),
        response_blake3: "0".to_owned(),
    };
    let destination = dir.path().join("blank.parquet");
    write_parquet(&response, &source, &destination, "/option/history/eod").expect("write");

    let file = std::fs::File::open(&destination).expect("open");
    let reader = SerializedFileReader::new(file).expect("read");
    let row = reader
        .get_row_iter(None)
        .expect("rows")
        .next()
        .expect("one row")
        .expect("readable");
    let last_trade = row
        .get_column_iter()
        .find(|(name, _)| name.as_str() == "last_trade")
        .map(|(_, field)| field.to_string())
        .expect("column present");
    assert_eq!(last_trade, "null", "blank must be null, not an epoch");
}
