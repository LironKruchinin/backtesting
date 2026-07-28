//! Turning an instrument and a timeframe into a partition path, reversibly.
//!
//! `InstrumentId` admits strings a filename does not. `SYN:RW` is the
//! instrument every synthetic feed emits and a colon is illegal in a Windows
//! filename; `ES.v.0` is fine; a hypothetical symbol containing `/` would
//! silently create a directory level. So the instrument is **percent-encoded**
//! into its path component: every byte outside `[A-Za-z0-9._-]` becomes
//! `%XX`, uppercase hex.
//!
//! Encoding rather than sanitising, because the mapping has to be injective.
//! Replacing `:` with `_` would file `SYN:RW` and `SYN_RW` in the same
//! directory, and the first backtest that mixed them would be wrong in a way
//! no test would catch.
//!
//! The one wart: a component that percent-encodes to exactly `.` or `..`
//! would be a path traversal, so a leading `.` in that case is escaped too.
//! `ES.v.0` is unaffected — only a whole component of dots can trigger it.

use std::path::{Path, PathBuf};

use crucible_core::types::{InstrumentId, TimeFrame};

use super::{CURATED_BARS_DIR, CuratedError};

/// Bytes that survive encoding unescaped: safe on every filesystem we target
/// and readable in a directory listing.
fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')
}

/// Percent-encodes an instrument identifier into one path component.
///
/// # Errors
/// [`CuratedError::InvalidInstrument`] if the identifier is empty or not
/// ASCII. ASCII-only mirrors the archive's rule (D-0021): macOS normalises
/// filenames to NFD while Linux stores the bytes given, so a non-ASCII
/// component can be byte-different per host while looking identical.
pub fn encode_instrument(instrument: &str) -> Result<String, CuratedError> {
    if instrument.is_empty() {
        return Err(CuratedError::InvalidInstrument {
            instrument: instrument.to_owned(),
            reason: "must not be empty",
        });
    }
    if !instrument.is_ascii() {
        return Err(CuratedError::InvalidInstrument {
            instrument: instrument.to_owned(),
            reason: "must be ASCII (a non-ASCII path is not byte-portable across hosts)",
        });
    }
    let mut out = String::with_capacity(instrument.len());
    for &b in instrument.as_bytes() {
        if is_unreserved(b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    // A component of nothing but dots is a traversal, not a name.
    if out == "." || out == ".." {
        out.replace_range(0..1, "%2E");
    }
    Ok(out)
}

/// Reverses [`encode_instrument`].
///
/// Returns `None` for anything that is not a well-formed encoding, which is
/// how a stray directory under `curated/bars/` is caught rather than guessed
/// at.
#[must_use]
pub fn decode_instrument(component: &str) -> Option<String> {
    let bytes = component.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = component.get(i + 1..i + 3)?;
            let byte = u8::from_str_radix(hex, 16).ok()?;
            // Only ever escape what encode_instrument would have escaped, so
            // the mapping stays one-to-one in both directions.
            if is_unreserved(byte) && !(i == 0 && byte == b'.') {
                return None;
            }
            out.push(byte);
            i += 3;
        } else {
            if !is_unreserved(bytes[i]) {
                return None;
            }
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok().filter(|s| !s.is_empty())
}

/// Directory holding every curated partition file for one instrument and
/// timeframe: `{data_dir}/curated/bars/{instrument}/{tf}`.
///
/// # Errors
/// [`CuratedError::InvalidInstrument`] via [`encode_instrument`].
pub fn partition_dir(
    data_dir: &Path,
    instrument: &InstrumentId,
    tf: TimeFrame,
) -> Result<PathBuf, CuratedError> {
    let encoded = encode_instrument(instrument.as_str())?;
    Ok(data_dir
        .join(CURATED_BARS_DIR)
        .join(encoded)
        .join(tf.to_string()))
}

/// Full path of one partition file, named after the raw window it came from.
///
/// # Errors
/// [`CuratedError::InvalidInstrument`] via [`encode_instrument`].
pub fn partition_file(
    data_dir: &Path,
    instrument: &InstrumentId,
    tf: TimeFrame,
    window_stem: &str,
) -> Result<PathBuf, CuratedError> {
    Ok(partition_dir(data_dir, instrument, tf)?.join(format!("{window_stem}.parquet")))
}

/// The window stem a raw archive path implies: the file name with the
/// `.dbn.zst` (or any) extensions removed.
///
/// `raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst` → `2024-01`, and
/// `.../2010-06-06--2010-07-01.dbn.zst` → `2010-06-06--2010-07-01`
/// (D-0026's two window shapes, carried through unchanged so a curated file
/// names the raw window it came from at a glance).
#[must_use]
pub fn window_stem_of(raw_file_path: &str) -> Option<&str> {
    let file_name = raw_file_path.rsplit('/').next()?;
    let stem = file_name.strip_suffix(".dbn.zst").unwrap_or(file_name);
    if stem.is_empty() { None } else { Some(stem) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_symbols_encode_to_themselves() {
        for sym in ["ESH4", "ES.v.0", "ESH4-ESM4", "ES.c.0", "6EU6", "ZN"] {
            let encoded = encode_instrument(sym).expect("valid symbol");
            assert_eq!(encoded, sym, "{sym} should need no escaping");
            assert_eq!(decode_instrument(&encoded).as_deref(), Some(sym));
        }
    }

    // SYN:RW is what every synthetic feed emits, and ':' is illegal in a
    // Windows filename. 0x3A is ':'.
    #[test]
    fn synthetic_instruments_encode_their_colon() {
        let encoded = encode_instrument("SYN:RW").expect("valid symbol");
        assert_eq!(encoded, "SYN%3ARW");
        assert_eq!(decode_instrument(&encoded).as_deref(), Some("SYN:RW"));
    }

    #[test]
    fn separators_and_spaces_are_escaped() {
        // '/' is 0x2F, '\\' is 0x5C, ' ' is 0x20.
        assert_eq!(encode_instrument("a/b").expect("ok"), "a%2Fb");
        assert_eq!(encode_instrument("a\\b").expect("ok"), "a%5Cb");
        assert_eq!(encode_instrument("a b").expect("ok"), "a%20b");
        for sym in ["a/b", "a\\b", "a b"] {
            let encoded = encode_instrument(sym).expect("ok");
            assert_eq!(decode_instrument(&encoded).as_deref(), Some(sym));
        }
    }

    #[test]
    fn dot_components_cannot_traverse() {
        assert_eq!(encode_instrument(".").expect("ok"), "%2E");
        assert_eq!(encode_instrument("..").expect("ok"), "%2E.");
        assert_eq!(decode_instrument("%2E").as_deref(), Some("."));
        assert_eq!(decode_instrument("%2E.").as_deref(), Some(".."));
    }

    #[test]
    fn empty_and_non_ascii_are_refused() {
        assert!(matches!(
            encode_instrument(""),
            Err(CuratedError::InvalidInstrument { .. })
        ));
        assert!(matches!(
            encode_instrument("ESü4"),
            Err(CuratedError::InvalidInstrument { .. })
        ));
    }

    // A stray directory must not decode to something plausible: the mapping
    // is one-to-one, so an over-escaped or malformed name has no preimage.
    #[test]
    fn malformed_directory_names_do_not_decode() {
        for bad in ["", "%", "%2", "%ZZ", "%41", "a:b", "a b"] {
            assert_eq!(decode_instrument(bad), None, "{bad:?} should not decode");
        }
    }

    #[test]
    fn partition_paths_are_built_from_the_pieces() {
        let dir = partition_dir(
            Path::new("/data"),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
        )
        .expect("ok");
        assert!(dir.ends_with("curated/bars/ESH4/1m"), "{}", dir.display());

        let file = partition_file(
            Path::new("/data"),
            &InstrumentId::new("ESH4"),
            TimeFrame::M1,
            "2024-01",
        )
        .expect("ok");
        assert!(
            file.ends_with("curated/bars/ESH4/1m/2024-01.parquet"),
            "{}",
            file.display()
        );
    }

    #[test]
    fn window_stems_survive_both_naming_shapes() {
        assert_eq!(
            window_stem_of("raw/GLBX.MDP3/ohlcv-1m/ES.FUT/2024-01.dbn.zst"),
            Some("2024-01")
        );
        assert_eq!(
            window_stem_of("raw/GLBX.MDP3/ohlcv-1s/ES.FUT/2010-06-06--2010-07-01.dbn.zst"),
            Some("2010-06-06--2010-07-01")
        );
        assert_eq!(window_stem_of(""), None);
    }
}
