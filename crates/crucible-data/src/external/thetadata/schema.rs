//! Pinned response headers, validated **by name and never by position**.
//!
//! Two measured vendor behaviours make this module load-bearing rather than
//! defensive:
//!
//! 1. **Unknown query parameters are ignored silently.** `greeks=true` on an
//!    endpoint without greeks returns HTTP 200 and the ordinary columns. The
//!    response header is the only evidence of what was actually asked for.
//! 2. **Column 5 is not the same field across endpoints.** `eod` has
//!    `created` (the vendor's post-close build-run time, D-0054) where
//!    `greeks/eod` has `timestamp` (per-contract last update), and `eod`
//!    carries an extra `last_trade` before the OHLC block that `greeks/eod`
//!    does not. A positional parser written against one silently mis-reads the
//!    other — dating every row by a build pass instead of a market event, or
//!    reading `open` out of `last_trade`.
//!
//! So a response whose header does not match its pin **refuses**, and decoding
//! looks columns up by name. This is `deny_unknown_fields` (CLAUDE.md §5.5)
//! applied to a wire format we do not control.
//!
//! The pins below are transcribed from live responses, not from the vendor's
//! documentation, which does not publish them.

use super::error::ThetaError;

/// A ThetaData endpoint whose response layout this build understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    /// `/option/history/eod` — OHLCV + closing NBBO. Duplicated pre-2022
    /// (D-0054).
    OptionEod,
    /// `/option/history/greeks/eod` — everything `eod` has except
    /// `last_trade`, plus all greeks, IV, and the underlying. Deduplicated.
    OptionGreeksEod,
    /// `/option/history/open_interest`.
    OptionOpenInterest,
    /// `/option/history/quote` — NBBO at the requested interval.
    OptionQuote,
    /// `/option/history/ohlc` — bars at the requested interval.
    OptionOhlc,
    /// `/option/history/greeks/first_order` — the cheapest carrier of
    /// `underlying_price` at 1-minute granularity.
    OptionGreeksFirstOrder,
    /// `/stock/history/ohlc`.
    StockOhlc,
    /// `/stock/history/quote`.
    StockQuote,
}

impl Endpoint {
    /// Path below the API base, e.g. `/option/history/greeks/eod`.
    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Endpoint::OptionEod => "/option/history/eod",
            Endpoint::OptionGreeksEod => "/option/history/greeks/eod",
            Endpoint::OptionOpenInterest => "/option/history/open_interest",
            Endpoint::OptionQuote => "/option/history/quote",
            Endpoint::OptionOhlc => "/option/history/ohlc",
            Endpoint::OptionGreeksFirstOrder => "/option/history/greeks/first_order",
            Endpoint::StockOhlc => "/stock/history/ohlc",
            Endpoint::StockQuote => "/stock/history/quote",
        }
    }

    /// The exact CSV header this build parses, in vendor order.
    #[must_use]
    pub fn pinned_header(self) -> &'static [&'static str] {
        match self {
            // Note `created` AND `last_trade` before the OHLC block: this is
            // the shape `greeks/eod` does NOT share (D-0054).
            Endpoint::OptionEod => &[
                "symbol",
                "expiration",
                "strike",
                "right",
                "created",
                "last_trade",
                "open",
                "high",
                "low",
                "close",
                "volume",
                "count",
                "bid_size",
                "bid_exchange",
                "bid",
                "bid_condition",
                "ask_size",
                "ask_exchange",
                "ask",
                "ask_condition",
            ],
            Endpoint::OptionGreeksEod => &[
                "symbol",
                "expiration",
                "strike",
                "right",
                "timestamp",
                "open",
                "high",
                "low",
                "close",
                "volume",
                "count",
                "bid_size",
                "bid_exchange",
                "bid",
                "bid_condition",
                "ask_size",
                "ask_exchange",
                "ask",
                "ask_condition",
                "delta",
                "theta",
                "vega",
                "rho",
                "epsilon",
                "lambda",
                "gamma",
                "vanna",
                "charm",
                "vomma",
                "veta",
                "vera",
                "speed",
                "zomma",
                "color",
                "ultima",
                "d1",
                "d2",
                "dual_delta",
                "dual_gamma",
                "implied_vol",
                "iv_error",
                "underlying_timestamp",
                "underlying_price",
            ],
            Endpoint::OptionOpenInterest => &[
                "symbol",
                "expiration",
                "strike",
                "right",
                "timestamp",
                "open_interest",
            ],
            Endpoint::OptionQuote => &[
                "symbol",
                "expiration",
                "strike",
                "right",
                "timestamp",
                "bid_size",
                "bid_exchange",
                "bid",
                "bid_condition",
                "ask_size",
                "ask_exchange",
                "ask",
                "ask_condition",
            ],
            Endpoint::OptionOhlc => &[
                "symbol",
                "expiration",
                "strike",
                "right",
                "timestamp",
                "open",
                "high",
                "low",
                "close",
                "volume",
                "count",
                "vwap",
            ],
            Endpoint::OptionGreeksFirstOrder => &[
                "symbol",
                "expiration",
                "strike",
                "right",
                "timestamp",
                "bid",
                "ask",
                "delta",
                "theta",
                "vega",
                "rho",
                "epsilon",
                "lambda",
                "implied_vol",
                "iv_error",
                "underlying_timestamp",
                "underlying_price",
            ],
            Endpoint::StockOhlc => &[
                "timestamp",
                "open",
                "high",
                "low",
                "close",
                "volume",
                "count",
                "vwap",
            ],
            Endpoint::StockQuote => &[
                "timestamp",
                "bid_size",
                "bid_exchange",
                "bid",
                "bid_condition",
                "ask_size",
                "ask_exchange",
                "ask",
                "ask_condition",
            ],
        }
    }

    /// True when responses from this endpoint identify a specific contract.
    ///
    /// Stock endpoints do not, which is why the contract-key reconciliations
    /// (D-0054) apply only to option responses.
    #[must_use]
    pub fn is_contract_scoped(self) -> bool {
        !matches!(self, Endpoint::StockOhlc | Endpoint::StockQuote)
    }
}

/// Column positions resolved **by name** from a validated header.
///
/// Built once per response; row decoding indexes through it rather than
/// through literals, so a vendor reordering that keeps every name is handled
/// rather than silently mis-parsed.
#[derive(Debug, Clone)]
pub struct ColumnIndex {
    endpoint: Endpoint,
    names: Vec<String>,
}

impl ColumnIndex {
    /// Validates a raw CSV header line against the endpoint's pin.
    ///
    /// # Errors
    /// [`ThetaError::UnexpectedColumns`] if the set or order of names differs
    /// from the pin in any way.
    pub fn validate(
        endpoint: Endpoint,
        header_line: &str,
        request_path: &str,
    ) -> Result<ColumnIndex, ThetaError> {
        let found: Vec<String> = header_line
            .trim()
            .trim_start_matches('\u{feff}')
            .split(',')
            .map(|c| c.trim().trim_matches('"').to_owned())
            .collect();
        let expected = endpoint.pinned_header();

        if found.len() != expected.len() || !found.iter().zip(expected).all(|(f, e)| f == e) {
            return Err(ThetaError::UnexpectedColumns {
                path: request_path.to_owned(),
                expected: expected.iter().map(|s| (*s).to_owned()).collect(),
                found,
            });
        }
        Ok(ColumnIndex {
            endpoint,
            names: found,
        })
    }

    /// The endpoint this index describes.
    #[must_use]
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint
    }

    /// Position of a named column, or `None` when the endpoint has no such
    /// column.
    #[must_use]
    pub fn position(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Number of columns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// True when the header carried no columns at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Identity of one option contract within a response: expiration, strike, and
/// right, exactly as the vendor wrote them.
///
/// This — never the raw row count — is the unit of completeness accounting
/// (D-0054). Kept as vendor text rather than parsed numbers so that the key
/// used for deduplication cannot itself introduce a rounding collision between
/// two genuinely different strikes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractKey {
    /// Expiration as written, e.g. `2024-01-19`.
    pub expiration: String,
    /// Strike as written, e.g. `470.000`.
    pub strike: String,
    /// Right as written, e.g. `CALL`.
    pub right: String,
}

impl ContractKey {
    /// Extracts the key from a split row using named positions.
    ///
    /// # Errors
    /// [`ThetaError::MalformedRow`] if any key column is missing.
    pub fn from_fields(
        index: &ColumnIndex,
        fields: &[&str],
        request_path: &str,
        row: u64,
    ) -> Result<ContractKey, ThetaError> {
        let get = |name: &str| -> Result<String, ThetaError> {
            index
                .position(name)
                .and_then(|p| fields.get(p))
                .map(|v| v.trim().trim_matches('"').to_owned())
                .ok_or_else(|| ThetaError::MalformedRow {
                    path: request_path.to_owned(),
                    row,
                    detail: format!("missing the {name} column"),
                })
        };
        Ok(ContractKey {
            expiration: get("expiration")?,
            strike: get("strike")?,
            right: get("right")?,
        })
    }
}

impl core::fmt::Display for ContractKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {} {}", self.expiration, self.strike, self.right)
    }
}

/// The threshold at or beyond which an implied-volatility fit is unusable.
///
/// The vendor reports a failed solve as exactly `iv_error = 100` alongside
/// `implied_vol = 0`, rather than as an absent value — so `== 100.0` would
/// catch the sentinel. The test is deliberately **`>=`**, not `==`, and that
/// must not be "simplified" back later: an IV fit whose error is at or beyond
/// 100% is worthless whether it is the vendor's sentinel or a genuine solve
/// failure that happened to land at 137. Keying on equality would admit the
/// genuinely-diverged fits while rejecting only the flagged ones, which is
/// precisely backwards.
///
/// The condition does not overfire on small errors: VIX rows carry
/// `iv_error = 0.0021` and are ordinary data. There is a test for that.
pub const IV_ERROR_SENTINEL: f64 = 100.0;

/// True when a row's underlying/IV fields carry the vendor's zero sentinel.
///
/// **A condition, not a row position.** The sentinel was first seen on the
/// 09:30 bar (`underlying_price = 0.0`, `iv_error = 100`), but keying on the
/// opening minute would mask the identical sentinel appearing mid-session,
/// which is the case that would quietly corrupt a feature. It is the same
/// family as the all-zero stock OHLC series (SPY 2016-01-04 answers HTTP 200
/// with 390 rows of zeros): this vendor spells "absent" as `0`, never as null.
#[must_use]
pub fn is_zero_sentinel(underlying_price: f64, iv_error: f64) -> bool {
    underlying_price == 0.0 || iv_error >= IV_ERROR_SENTINEL
}

#[cfg(test)]
mod tests {
    use super::*;

    const EOD_HEADER: &str = "symbol,expiration,strike,right,created,last_trade,open,high,low,close,volume,count,bid_size,bid_exchange,bid,bid_condition,ask_size,ask_exchange,ask,ask_condition";
    const GREEKS_EOD_HEADER: &str = "symbol,expiration,strike,right,timestamp,open,high,low,close,volume,count,bid_size,bid_exchange,bid,bid_condition,ask_size,ask_exchange,ask,ask_condition,delta,theta,vega,rho,epsilon,lambda,gamma,vanna,charm,vomma,veta,vera,speed,zomma,color,ultima,d1,d2,dual_delta,dual_gamma,implied_vol,iv_error,underlying_timestamp,underlying_price";

    // Both pins are transcribed from live responses; if either drifts, every
    // downstream column meaning drifts with it.
    #[test]
    fn the_live_headers_match_their_pins() {
        ColumnIndex::validate(Endpoint::OptionEod, EOD_HEADER, "/x").expect("eod pin");
        ColumnIndex::validate(Endpoint::OptionGreeksEod, GREEKS_EOD_HEADER, "/x")
            .expect("greeks/eod pin");
    }

    // The D-0054 trap, as a negative control: the two endpoints disagree at
    // column 5, and `eod` carries an extra `last_trade`. Parsing one response
    // with the other's layout must be impossible, not merely discouraged.
    #[test]
    fn an_eod_body_cannot_be_read_as_greeks_eod_or_the_reverse() {
        let err = ColumnIndex::validate(Endpoint::OptionGreeksEod, EOD_HEADER, "/x")
            .expect_err("eod header is not the greeks/eod layout");
        assert!(matches!(err, ThetaError::UnexpectedColumns { .. }), "{err}");

        let err = ColumnIndex::validate(Endpoint::OptionEod, GREEKS_EOD_HEADER, "/x")
            .expect_err("greeks/eod header is not the eod layout");
        assert!(matches!(err, ThetaError::UnexpectedColumns { .. }), "{err}");
    }

    // Column 5 differs by NAME between the two, which is the whole reason
    // lookups go through `position` rather than a literal 4.
    #[test]
    fn column_five_is_a_different_field_in_each_endpoint() {
        let eod = ColumnIndex::validate(Endpoint::OptionEod, EOD_HEADER, "/x").expect("eod");
        let greeks =
            ColumnIndex::validate(Endpoint::OptionGreeksEod, GREEKS_EOD_HEADER, "/x").expect("gk");

        assert_eq!(eod.position("created"), Some(4));
        assert_eq!(eod.position("timestamp"), None);
        assert_eq!(greeks.position("timestamp"), Some(4));
        assert_eq!(greeks.position("created"), None);

        // The consequence that would otherwise be silent: `open` sits at a
        // different index in each, so a positional parse reads `last_trade`.
        assert_eq!(eod.position("open"), Some(6));
        assert_eq!(greeks.position("open"), Some(5));
    }

    // A vendor adding, removing, or renaming a column must stop the run rather
    // than shift every subsequent column's meaning by one.
    #[test]
    fn any_header_drift_refuses() {
        let drifted = [
            "symbol,expiration,strike,right,timestamp,open_interest,extra",
            "symbol,expiration,strike,right,timestamp",
            "symbol,expiration,strike,side,timestamp,open_interest",
            "expiration,symbol,strike,right,timestamp,open_interest",
        ];
        for header in drifted {
            let err = ColumnIndex::validate(Endpoint::OptionOpenInterest, header, "/x")
                .expect_err("drifted header must refuse");
            assert!(matches!(err, ThetaError::UnexpectedColumns { .. }), "{err}");
        }
    }

    #[test]
    fn quoted_and_bom_prefixed_headers_still_validate() {
        let quoted = "\u{feff}\"symbol\",\"expiration\",\"strike\",\"right\",\"timestamp\",\"open_interest\"";
        ColumnIndex::validate(Endpoint::OptionOpenInterest, quoted, "/x").expect("quoted pin");
    }

    #[test]
    fn the_contract_key_comes_from_named_columns() {
        let index = ColumnIndex::validate(Endpoint::OptionEod, EOD_HEADER, "/x").expect("eod");
        let row = r#""SPY","2017-12-15",194.000,"PUT",2017-06-15T18:00:39.750,2017-06-15T15:34:53.505,1.13,1.13,1.09,1.09,6,3,2216,9,1.08,50,4026,9,1.11,50"#;
        let fields: Vec<&str> = row.split(',').collect();
        let key = ContractKey::from_fields(&index, &fields, "/x", 1).expect("key");
        assert_eq!(key.expiration, "2017-12-15");
        assert_eq!(key.strike, "194.000");
        assert_eq!(key.right, "PUT");
        assert_eq!(key.to_string(), "2017-12-15 194.000 PUT");
    }

    // The sentinel is a condition on values, never on row position: the
    // mid-session case is the one that would corrupt a feature silently.
    #[test]
    fn the_zero_sentinel_is_detected_wherever_it_appears() {
        assert!(is_zero_sentinel(0.0, 100.0), "the 09:30 case");
        assert!(is_zero_sentinel(0.0, 0.0), "underlying absent mid-session");
        assert!(
            is_zero_sentinel(4742.83, 100.0),
            "IV solve failed mid-session"
        );
        assert!(!is_zero_sentinel(4742.83, 0.0), "an ordinary row");
        assert!(!is_zero_sentinel(13.20, 0.0021), "small iv_error is fine");
    }
}
