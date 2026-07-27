//! Inspecting what the vendor actually delivered, as an injectable seam.
//!
//! Two questions are asked of a downloaded file, both before it is allowed
//! anywhere near `raw/`:
//!
//! 1. **Are these the bytes they sent?** The catalog is the checksum
//!    gatekeeper (D-0017), but it hashes whatever is on disk — so a truncated
//!    download would be blake3'd, recorded, and thereafter certified by the
//!    manifest as the January slice. The vendor publishes a SHA-256 per
//!    delivered file; checking it is the only thing standing between a short
//!    read and a permanently untruthful archive.
//! 2. **Which raw symbols are in here?** A pull keyed on `ES.FUT` must record
//!    the contracts that key expanded to, or `coverage("ESH4")` reports the
//!    range as missing and re-buys it forever (see the `ingest` module docs).
//!    Since 2026-02-17 Databento no longer ships symbology support files for
//!    DBN jobs — the mappings live in the DBN header — so this means decoding
//!    the file's metadata.
//!
//! Both answers need `sha2` and a DBN decoder, which live behind the
//! non-default `databento` feature. The trait is what lets the execute state
//! machine that *uses* those answers compile, and be tested, in a plain
//! `cargo test` with no vendor dependencies at all — including the negative
//! controls that prove a checksum mismatch actually stops a placement.

use std::path::{Path, PathBuf};

/// Why a delivered file could not be inspected or did not check out.
#[derive(Debug)]
pub enum DeliveryError {
    /// The file could not be read.
    Io {
        /// Path being read.
        path: PathBuf,
        /// Underlying failure.
        source: std::io::Error,
    },
    /// The file's SHA-256 does not match what the vendor published for it.
    ChecksumMismatch {
        /// Path that was hashed.
        path: PathBuf,
        /// Vendor-published digest, lowercase hex.
        expected: String,
        /// What the bytes on disk actually hash to, lowercase hex.
        actual: String,
    },
    /// The vendor published something that is not a SHA-256 digest.
    MalformedDigest {
        /// The offending text, as supplied.
        digest: String,
    },
    /// The file could not be decoded as DBN.
    Undecodable {
        /// Path being decoded.
        path: PathBuf,
        /// Explanation from the decoder.
        detail: String,
    },
}

impl core::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeliveryError::Io { path, source } => {
                write!(f, "reading {}: {source}", path.display())
            }
            DeliveryError::ChecksumMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "{} does not match the vendor checksum: expected sha256 \
                 {expected}, got {actual}. The download is incomplete or \
                 corrupt and was NOT placed in the archive",
                path.display()
            ),
            DeliveryError::MalformedDigest { digest } => write!(
                f,
                "vendor supplied {digest:?} where a 64-character hex sha256 \
                 was expected; refusing to treat an unverifiable download as \
                 verified"
            ),
            DeliveryError::Undecodable { path, detail } => write!(
                f,
                "could not read DBN metadata from {}: {detail}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DeliveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DeliveryError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Questions asked of a delivered file before it is archived.
pub trait DeliveryInspector {
    /// Verifies the file's SHA-256 against the vendor-published digest.
    ///
    /// `expected_hex` is bare lowercase hex; providers normalise any
    /// `sha256:` prefix away before this is called.
    ///
    /// # Errors
    /// [`DeliveryError::ChecksumMismatch`] when the bytes disagree,
    /// [`DeliveryError::MalformedDigest`] when the vendor digest is not a
    /// digest, or [`DeliveryError::Io`] when the file cannot be read.
    fn verify_sha256(&self, path: &Path, expected_hex: &str) -> Result<(), DeliveryError>;

    /// Raw symbols observed in the file's DBN metadata.
    ///
    /// These are unioned with the requested key to form
    /// [`ManifestRecord::symbols`](crate::catalog::ManifestRecord). An empty
    /// list is legal — it means the delivered window resolved to no contracts
    /// — and is reported rather than treated as an error, because the
    /// requested key alone still makes coverage truthful for that key.
    ///
    /// # Errors
    /// [`DeliveryError::Undecodable`] or [`DeliveryError::Io`].
    fn observed_symbols(&self, path: &Path) -> Result<Vec<String>, DeliveryError>;
}

/// True if `text` is a bare lowercase 64-character hex SHA-256 digest.
#[must_use]
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The real inspector: SHA-256 from `sha2`, symbols from the DBN header.
///
/// Both dependencies arrive with the `databento` feature — `dbn` through the
/// client's own re-export, so the decoder can never drift from the client
/// that produced the file.
#[cfg(feature = "databento")]
pub struct DbnDelivery;

#[cfg(feature = "databento")]
impl DeliveryInspector for DbnDelivery {
    fn verify_sha256(&self, path: &Path, expected_hex: &str) -> Result<(), DeliveryError> {
        use sha2::{Digest, Sha256};

        if !is_sha256_hex(expected_hex) {
            return Err(DeliveryError::MalformedDigest {
                digest: expected_hex.to_string(),
            });
        }
        let file = std::fs::File::open(path).map_err(|source| DeliveryError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut reader = std::io::BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let read =
                std::io::Read::read(&mut reader, &mut buf).map_err(|source| DeliveryError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        let actual = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if actual == expected_hex {
            Ok(())
        } else {
            Err(DeliveryError::ChecksumMismatch {
                path: path.to_path_buf(),
                expected: expected_hex.to_string(),
                actual,
            })
        }
    }

    fn observed_symbols(&self, path: &Path) -> Result<Vec<String>, DeliveryError> {
        use databento::dbn::decode::{DbnMetadata, dbn::Decoder};

        let decoder = Decoder::from_zstd_file(path).map_err(|e| DeliveryError::Undecodable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
        let metadata = decoder.metadata();

        // Two sources, unioned. `mappings` is the authoritative expansion of
        // a parent key into the contracts that actually traded in the window;
        // `symbols` echoes back what was requested and is included so a
        // raw-symbol pull still records what it asked for.
        let mut out: Vec<String> = metadata.symbols.clone();
        for mapping in &metadata.mappings {
            out.push(mapping.raw_symbol.clone());
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A scripted [`DeliveryInspector`] for tests.
    //!
    //! Both answers are scripted per file *name* rather than per path, so a
    //! test can pin them before it knows which temp directory the staging
    //! area will land in.

    use super::{DeliveryError, DeliveryInspector};
    use std::collections::BTreeMap;
    use std::path::Path;

    pub(crate) struct FakeInspector {
        /// File name -> symbols reported from "DBN metadata".
        pub symbols: BTreeMap<String, Vec<String>>,
        /// File names whose checksum verification must fail.
        pub bad_checksums: Vec<String>,
        /// File names whose DBN metadata must fail to decode.
        pub undecodable: Vec<String>,
    }

    impl FakeInspector {
        pub(crate) fn new() -> FakeInspector {
            FakeInspector {
                symbols: BTreeMap::new(),
                bad_checksums: Vec::new(),
                undecodable: Vec::new(),
            }
        }

        /// Scripts the raw symbols a given delivered file "contains".
        pub(crate) fn with_symbols(mut self, file_name: &str, symbols: &[&str]) -> FakeInspector {
            self.symbols.insert(
                file_name.to_string(),
                symbols.iter().map(|s| (*s).to_string()).collect(),
            );
            self
        }

        /// Makes checksum verification fail for a given delivered file.
        pub(crate) fn with_bad_checksum(mut self, file_name: &str) -> FakeInspector {
            self.bad_checksums.push(file_name.to_string());
            self
        }

        fn file_name(path: &Path) -> String {
            path.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        }
    }

    impl DeliveryInspector for FakeInspector {
        fn verify_sha256(&self, path: &Path, expected_hex: &str) -> Result<(), DeliveryError> {
            let name = FakeInspector::file_name(path);
            if self.bad_checksums.contains(&name) {
                return Err(DeliveryError::ChecksumMismatch {
                    path: path.to_path_buf(),
                    expected: expected_hex.to_string(),
                    actual: "0".repeat(64),
                });
            }
            Ok(())
        }

        fn observed_symbols(&self, path: &Path) -> Result<Vec<String>, DeliveryError> {
            let name = FakeInspector::file_name(path);
            if self.undecodable.contains(&name) {
                return Err(DeliveryError::Undecodable {
                    path: path.to_path_buf(),
                    detail: "scripted decode failure".to_string(),
                });
            }
            Ok(self.symbols.get(&name).cloned().unwrap_or_default())
        }
    }
}
