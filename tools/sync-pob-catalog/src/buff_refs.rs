//! `check-buff-refs`: reconciles `overlay/buff_definitions.json` against
//! vendor line ranges (the drift guard for the manually-curated-exception channel).
//!
//! `buff_definitions.json` is curated by hand from the `CalcPerform.lua
//! doActorMisc` if-chain (procedural code can't be serialized by luajit),
//! and each entry carries a `vendor_ref` (file + line range + `fnv1a64` hash
//! of that range). This module:
//! - `check`: recomputes each line range's hash and compares it against the
//!   recorded value — a drifted range after a vendor upgrade raises a
//!   warning (a signal that the curation may no longer be faithful);
//! - `--write`: after manual review, writes back the fresh hashes (a
//!   mechanical step; the curated content itself is still a human's responsibility).
//!
//! Hashing uses FNV-1a 64 (self-contained, non-cryptographic — this is only for drift detection).

use std::fs;
use std::io;
use std::path::Path;

use pobr_data::catalog::buffs::BuffDef;
use serde::{Deserialize, Serialize};

/// The full document (production/reconciliation side; `_meta` is passed through order-preserving).
#[derive(Debug, Serialize, Deserialize)]
pub struct BuffDefinitionsDoc {
    /// Header metadata (a hand-curated table: vendor commit + maintenance notes).
    #[serde(rename = "_meta")]
    pub meta: serde_json::Value,
    /// The list of buff definitions.
    pub buffs: Vec<BuffDef>,
}

/// FNV-1a 64-bit hash.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Compute the recorded hash for lines `[line_start, line_end]` (1-based,
/// inclusive) of a file. Lines are rejoined with `\n` (platform-line-ending
/// independent); returns `None` when the range is out of bounds.
pub fn segment_hash(file_text: &str, line_start: u32, line_end: u32) -> Option<String> {
    if line_start == 0 || line_end < line_start {
        return None;
    }
    let lines: Vec<&str> = file_text.lines().collect();
    let start = (line_start - 1) as usize;
    let end = line_end as usize;
    if end > lines.len() {
        return None;
    }
    let segment = lines[start..end].join("\n");
    Some(format!("fnv1a64:{:016x}", fnv1a64(segment.as_bytes())))
}

/// A single reconciliation result.
#[derive(Debug)]
pub struct RefDrift {
    /// The buff id.
    pub id: String,
    /// The recorded hash.
    pub recorded: String,
    /// The freshly computed hash (`None` when the line range is out of bounds).
    pub actual: Option<String>,
}

/// Reconcile: returns the drift list (empty means everything matches). When
/// `write = true`, writes back the freshly computed hashes and re-serializes
/// the defs file (a mechanical refresh, meant to be used after manual review).
pub fn run_check_buff_refs(
    vendor_root: &Path,
    defs_path: &Path,
    write: bool,
) -> io::Result<Vec<RefDrift>> {
    let defs_text = fs::read_to_string(defs_path)?;
    let mut doc: BuffDefinitionsDoc = serde_json::from_str(&defs_text)
        .map_err(|error| io::Error::other(format!("buff_definitions failed to parse: {error}")))?;

    let mut drifts = Vec::new();
    for buff in &mut doc.buffs {
        let vendor_file = vendor_root.join(&buff.vendor_ref.file);
        let file_text = fs::read_to_string(&vendor_file).map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "failed to read vendor file {}: {error}",
                    vendor_file.display()
                ),
            )
        })?;
        let actual = segment_hash(
            &file_text,
            buff.vendor_ref.line_start,
            buff.vendor_ref.line_end,
        );
        if actual.as_deref() != Some(buff.vendor_ref.segment_hash.as_str()) {
            drifts.push(RefDrift {
                id: buff.id.clone(),
                recorded: buff.vendor_ref.segment_hash.clone(),
                actual: actual.clone(),
            });
            if write && let Some(actual) = actual {
                buff.vendor_ref.segment_hash = actual;
            }
        }
    }

    if write && !drifts.is_empty() {
        let mut json = serde_json::to_string_pretty(&doc)
            .expect("buff definitions document serialization should not fail");
        json.push('\n');
        fs::write(defs_path, json)?;
    }
    Ok(drifts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a 64 known-answer test (standard test vectors).
    #[test]
    fn fnv1a64_known_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    /// Line-range hash: 1-based inclusive; out-of-bounds returns None; independent of line endings.
    #[test]
    fn segment_hash_lines_and_bounds() {
        let text = "line1\nline2\nline3\n";
        let h12 = segment_hash(text, 1, 2).unwrap();
        assert_eq!(h12, format!("fnv1a64:{:016x}", fnv1a64(b"line1\nline2")));
        // CRLF gives the same value
        let crlf = "line1\r\nline2\r\nline3\r\n";
        assert_eq!(segment_hash(crlf, 1, 2).unwrap(), h12);
        assert!(segment_hash(text, 0, 1).is_none());
        assert!(segment_hash(text, 2, 1).is_none());
        assert!(segment_hash(text, 1, 4).is_none());
    }
}
