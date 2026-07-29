//! Failure modes of the ThetaData acquisition path.
//!
//! Hand-rolled per CLAUDE.md §5.1. The variants that matter operationally are
//! [`ThetaError::NotEntitled`] and [`ThetaError::NoData`]: the vendor reports
//! both with status codes that are easy to mistake for success or for a bug,
//! and confusing them would either buy nothing or record a gap that is not
//! real.

use std::fmt;
use std::path::PathBuf;

/// The nonstandard status the Theta Terminal returns for "your request was
/// valid, the subscription covers it, and there is simply nothing there".
///
/// Not an error in the HTTP sense and not a failure of ours: an option root
/// has no chain before it was listed, and greeks exist only from the date the
/// vendor's underlying feed for that root begins — which is **per root**, and
/// as late as 2017 for SPY and 2026 for NDX. Callers record the empty result
/// and move on rather than retrying.
pub const STATUS_NO_DATA: u16 = 472;

/// Why a ThetaData request or its transcode failed.
#[derive(Debug)]
pub enum ThetaError {
    /// The Theta Terminal is not reachable on the configured address.
    ///
    /// Almost always "the JAR is not running": it is a local process that must
    /// be started by hand and stays up for the life of a pull.
    TerminalUnreachable {
        /// Base URL that was tried.
        base_url: String,
        /// What the transport said.
        detail: String,
    },
    /// The transport failed after the configured number of attempts.
    Transport {
        /// The request path, for locating the failure.
        path: String,
        /// How many attempts were made.
        attempts: u32,
        /// What the transport said on the last attempt.
        detail: String,
    },
    /// The subscription does not cover this request (HTTP 403).
    ///
    /// Carries the vendor's own wording, which names the tier required — the
    /// only trustworthy statement of entitlement, since the published tables
    /// and the running terminal have been observed to disagree.
    NotEntitled {
        /// The request path.
        path: String,
        /// The vendor's message verbatim.
        message: String,
    },
    /// The vendor has no data for this request ([`STATUS_NO_DATA`]).
    NoData {
        /// The request path.
        path: String,
        /// The vendor's message verbatim.
        message: String,
    },
    /// The endpoint does not exist (HTTP 404), or the API version is retired
    /// (HTTP 410).
    ///
    /// Kept distinct from [`ThetaError::NoData`] because it means the *client*
    /// is wrong, not the archive: v2 paths answer 410, and a misspelled v3
    /// path answers 404 with an HTML body rather than a diagnostic.
    UnknownEndpoint {
        /// The request path.
        path: String,
        /// Status observed.
        status: u16,
    },
    /// The request was malformed (HTTP 400).
    BadRequest {
        /// The request path.
        path: String,
        /// The vendor's message verbatim.
        message: String,
    },
    /// Any other status.
    UnexpectedStatus {
        /// The request path.
        path: String,
        /// Status observed.
        status: u16,
        /// First part of the body, for diagnosis.
        body_head: String,
    },
    /// The CSV header did not match what this build expects to parse.
    ///
    /// This is a merge-blocking class of bug rather than a runtime hiccup. The
    /// Terminal **silently ignores unknown query parameters** — `greeks=true`
    /// on an endpoint that has no greeks returns the ordinary columns with no
    /// complaint — so the returned header is the only evidence of what was
    /// actually asked for. Pinning it is the same instinct as
    /// `deny_unknown_fields` on a config (CLAUDE.md §5.5).
    UnexpectedColumns {
        /// The request path.
        path: String,
        /// Columns this build expected, in order.
        expected: Vec<String>,
        /// Columns the vendor sent, in order.
        found: Vec<String>,
    },
    /// A row could not be parsed into the expected types.
    MalformedRow {
        /// The request path.
        path: String,
        /// 1-based row number within the body, excluding the header.
        row: u64,
        /// What was wrong.
        detail: String,
    },
    /// A timestamp could not be converted from Eastern wall clock to UTC.
    Timestamp {
        /// The request path.
        path: String,
        /// 1-based row number within the body, excluding the header.
        row: u64,
        /// The conversion failure.
        source: crate::calendar::EasternTimeError,
    },
    /// Filesystem failure while writing curated output or the inventory.
    Io {
        /// Path being operated on.
        path: PathBuf,
        /// What was being attempted.
        during: &'static str,
        /// The underlying error.
        source: std::io::Error,
    },
    /// Parquet encoding failure.
    Parquet {
        /// File being written.
        path: PathBuf,
        /// What was being attempted.
        during: &'static str,
        /// What the Parquet layer said.
        detail: String,
    },
}

impl fmt::Display for ThetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThetaError::TerminalUnreachable { base_url, detail } => write!(
                f,
                "the Theta Terminal is not answering at {base_url} ({detail}). It is a \
                 local process: start it with `java -jar ThetaTerminalv3.jar` and wait \
                 for it to log \"Starting server\""
            ),
            ThetaError::Transport {
                path,
                attempts,
                detail,
            } => write!(f, "GET {path} failed after {attempts} attempts: {detail}"),
            ThetaError::NotEntitled { path, message } => write!(
                f,
                "GET {path} is not covered by this subscription: {message}"
            ),
            ThetaError::NoData { path, message } => {
                write!(f, "GET {path} returned no data: {message}")
            }
            ThetaError::UnknownEndpoint { path, status } => write!(
                f,
                "GET {path} answered {status}: this build is asking for an endpoint the \
                 Terminal does not serve. v2 paths answer 410 (the API was retired); a \
                 misspelled v3 path answers 404"
            ),
            ThetaError::BadRequest { path, message } => {
                write!(f, "GET {path} was rejected as malformed: {message}")
            }
            ThetaError::UnexpectedStatus {
                path,
                status,
                body_head,
            } => write!(f, "GET {path} answered {status}: {body_head}"),
            ThetaError::UnexpectedColumns {
                path,
                expected,
                found,
            } => write!(
                f,
                "GET {path} returned columns this build does not recognise.\n  \
                 expected: {}\n  found:    {}\nThe Terminal ignores unknown query \
                 parameters silently, so a changed header is the only signal that the \
                 request meant something other than intended. Refusing rather than \
                 storing mislabelled columns",
                expected.join(","),
                found.join(",")
            ),
            ThetaError::MalformedRow { path, row, detail } => {
                write!(f, "GET {path} row {row}: {detail}")
            }
            ThetaError::Timestamp { path, row, source } => {
                write!(f, "GET {path} row {row}: {source}")
            }
            ThetaError::Io {
                path,
                during,
                source,
            } => write!(f, "{during} {} failed: {source}", path.display()),
            ThetaError::Parquet {
                path,
                during,
                detail,
            } => write!(f, "{during} {} failed: {detail}", path.display()),
        }
    }
}

impl std::error::Error for ThetaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ThetaError::Io { source, .. } => Some(source),
            ThetaError::Timestamp { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl ThetaError {
    /// True when the vendor answered "nothing here" rather than failing.
    ///
    /// Callers treat this as a recorded empty result, not a retry.
    #[must_use]
    pub fn is_no_data(&self) -> bool {
        matches!(self, ThetaError::NoData { .. })
    }
}
