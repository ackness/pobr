//! PoB Build Code codec: URL-safe Base64 + zlib (byte-compatible with PathOfBuilding / pobb.in).
//!
//! PoB's share code flow is: Build XML → `zlib.compress` (with zlib header `0x78 0x9c`) →
//! standard Base64 → replace `+`/`/` with `-`/`_` (URL-safe), strip `=` padding.
//!
//! This module replicates that flow, and is **tolerant on the decode side**:
//! - tolerates `=` padding being present or absent (PoB strips it, pobb.in sometimes keeps it);
//! - tolerates both the URL-safe and standard alphabets (tries URL-safe first, falls back to standard);
//! - strips any ASCII whitespace / newlines (common pollution from copy-paste).
//!
//! Also includes **zip-bomb protection**: caps on both the compressed input length and the
//! decompressed output length.

use std::io::Read;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::Write;

use crate::error::BuildCodeError;

/// Upper bound on decompressed output length (PoB Build XML is usually < 1 MiB; this
/// leaves plenty of headroom while still guarding against zip-bombs).
pub const MAX_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

/// Upper bound on compressed input length (encode side; normal Build XML is far smaller).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Decodes a PoB Build Code into a Build XML string.
///
/// Tolerant of whitespace, missing padding, and both URL-safe and standard alphabets.
/// Guards against zip-bombs by erroring if the decompressed output exceeds [`MAX_DECOMPRESSED_BYTES`].
pub fn decode_pob_code(code: &str) -> Result<String, BuildCodeError> {
    let cleaned = strip_ascii_whitespace(code);
    if cleaned.is_empty() {
        return Err(BuildCodeError::Empty);
    }

    let compressed = decode_base64_tolerant(&cleaned)?;
    let xml_bytes = inflate_bounded(&compressed, MAX_DECOMPRESSED_BYTES)?;

    String::from_utf8(xml_bytes).map_err(|e| BuildCodeError::Utf8(e.to_string()))
}

/// Encodes a Build XML string into a PoB Build Code (URL-safe Base64, no padding, matches PoB).
pub fn encode_pob_code(xml: &str) -> Result<String, BuildCodeError> {
    let bytes = xml.as_bytes();
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(BuildCodeError::InputTooLarge {
            len: bytes.len(),
            limit: MAX_INPUT_BYTES,
        });
    }

    let compressed = deflate(bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(compressed))
}

/// Strips all ASCII whitespace characters (space / tab / newline / carriage return).
fn strip_ascii_whitespace(input: &str) -> String {
    input.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

/// Tolerant Base64 decode: tries URL-safe (no-pad / padded) then standard (no-pad / padded), in order.
fn decode_base64_tolerant(cleaned: &str) -> Result<Vec<u8>, BuildCodeError> {
    // URL-safe first (PoB's default); padding tolerance tries NO_PAD before the padded variant.
    let attempts: [&base64::engine::GeneralPurpose; 4] =
        [&URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD_NO_PAD, &STANDARD];

    let mut last_err = String::new();
    for engine in attempts {
        match engine.decode(cleaned.as_bytes()) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => last_err = e.to_string(),
        }
    }

    Err(BuildCodeError::Base64(last_err))
}

/// zlib compression.
fn deflate(bytes: &[u8]) -> Result<Vec<u8>, BuildCodeError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|e| BuildCodeError::Deflate(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| BuildCodeError::Deflate(e.to_string()))
}

/// zlib decompression with an output length cap (zip-bomb protection).
fn inflate_bounded(compressed: &[u8], limit: usize) -> Result<Vec<u8>, BuildCodeError> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut out = Vec::new();
    // Read one extra byte to detect overflow: take(limit + 1).
    let mut limited = (&mut decoder).take((limit as u64) + 1);
    limited
        .read_to_end(&mut out)
        .map_err(|e| BuildCodeError::Inflate(e.to_string()))?;

    if out.len() > limit {
        return Err(BuildCodeError::DecompressedTooLarge { limit });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encode_decode() {
        let xml = "<PathOfBuilding2><Build level=\"1\"/></PathOfBuilding2>";
        let code = encode_pob_code(xml).expect("encode");
        // URL-safe / no padding: should not contain +, /, =.
        assert!(!code.contains('+'));
        assert!(!code.contains('/'));
        assert!(!code.contains('='));
        let decoded = decode_pob_code(&code).expect("decode");
        assert_eq!(decoded, xml);
    }

    #[test]
    fn decode_tolerates_whitespace_and_newlines() {
        let xml = "<PathOfBuilding2/>";
        let code = encode_pob_code(xml).expect("encode");
        let mut polluted = String::new();
        for (i, c) in code.chars().enumerate() {
            polluted.push(c);
            if i % 4 == 0 {
                polluted.push('\n');
            }
        }
        polluted.push_str("  \t");
        let decoded = decode_pob_code(&polluted).expect("decode polluted");
        assert_eq!(decoded, xml);
    }

    #[test]
    fn decode_tolerates_standard_alphabet_with_padding() {
        // Simulate a code from a non-PoB source using standard base64 + padding.
        let xml = "<PathOfBuilding2>x</PathOfBuilding2>";
        let compressed = deflate(xml.as_bytes()).unwrap();
        let std_code = STANDARD.encode(&compressed);
        let decoded = decode_pob_code(&std_code).expect("decode std");
        assert_eq!(decoded, xml);
    }

    #[test]
    fn empty_code_errors() {
        assert!(matches!(
            decode_pob_code("   \n\t "),
            Err(BuildCodeError::Empty)
        ));
    }

    #[test]
    fn garbage_base64_errors() {
        assert!(matches!(
            decode_pob_code("!!!not base64!!!"),
            Err(BuildCodeError::Base64(_))
        ));
    }

    #[test]
    fn non_zlib_payload_errors() {
        // Valid base64 but not a zlib stream.
        let code = URL_SAFE_NO_PAD.encode(b"hello world not zlib");
        assert!(matches!(
            decode_pob_code(&code),
            Err(BuildCodeError::Inflate(_))
        ));
    }
}
