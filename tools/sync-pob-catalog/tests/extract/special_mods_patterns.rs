//! Pattern-compile validation for `overlay/special_mods.json`.
//!
//! Schema contract: pattern is a subset of Rust regex syntax, and a compile
//! failure at load time means fail-fast. This crate already depends on
//! regex, so the compile check lives here; the interpreter (the full error
//! path of `SpecialModRules::compile`) belongs to B-2.

use std::path::Path;

use regex::Regex;

/// The two special_mods layers (tools/sync-pob-catalog/ -> up two levels to
/// the repo root): the version-independent curation layer
/// `data/overlay-common/` (P1-3, the bulk with 133 entries) plus the version
/// layer `data/<ver>/overlay/`. Their union covers every curated pattern.
fn special_mods_paths() -> Vec<std::path::PathBuf> {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    vec![
        data.join("overlay-common/special_mods.json"),
        data.join(pobr_data::data_version())
            .join("overlay/special_mods.json"),
    ]
}

/// The union of entries from both layers (missing files are skipped; at least one layer must exist).
fn load_entries() -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    for path in special_mods_paths() {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => panic!("{}: {e}", path.display()),
        };
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        entries.extend(
            doc["entries"]
                .as_array()
                .expect("entries array")
                .iter()
                .cloned(),
        );
    }
    entries
}

/// Every pattern must compile under the regex crate (the engine uniformly
/// wraps patterns in `^...$` for whole-line anchoring, so this compiles the
/// same way); the capture-group count must be >= the largest `$n` referenced within mods.
#[test]
fn all_patterns_compile_and_captures_cover_refs() {
    let entries = load_entries();
    assert!(!entries.is_empty());
    for entry in &entries {
        let id = entry["id"].as_str().unwrap();
        let pattern = entry["pattern"].as_str().unwrap();
        let re = Regex::new(&format!("^{pattern}$"))
            .unwrap_or_else(|e| panic!("{id}: pattern failed to compile: {e}"));
        // The largest $n appearing in the mods JSON text must not exceed the capture-group count
        let mods_text = entry["mods"].to_string();
        let max_ref = (1..=9)
            .rev()
            .find(|n| {
                mods_text.contains(&format!("\"${n}\"")) || mods_text.contains(&format!("${n}\""))
            })
            .unwrap_or(0);
        assert!(
            re.captures_len() > max_ref,
            "{id}: {} capture group(s), but ${max_ref} is referenced",
            re.captures_len() - 1
        );
    }
}

/// Numeric captures use a fixed set of shapes: `(\d+)` / `(\d+(?:\.\d+)?)` /
/// `([+-]\d+)` / `(\+?\d+)`. Any capture outside that set must be an
/// explicit closed set (no `\d`) — the DSL's hard boundary of "no open captures".
#[test]
fn captures_are_numeric_or_closed_sets() {
    const NUMERIC_FORMS: [&str; 4] = [r"(\d+)", r"(\d+(?:\.\d+)?)", r"([+-]\d+)", r"(\+?\d+)"];
    for entry in &load_entries() {
        let id = entry["id"].as_str().unwrap();
        let pattern = entry["pattern"].as_str().unwrap();
        // Scan capture group by capture group (top-level parens, ignoring (?: non-capturing groups)
        let bytes = pattern.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'(' && !pattern[i..].starts_with("(?:") {
                // Find the balanced closing paren
                let mut depth = 1;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'(' {
                        depth += 1;
                    } else if bytes[j] == b')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let group = &pattern[i..j];
                let is_numeric = NUMERIC_FORMS.contains(&group);
                assert!(
                    is_numeric || !group.contains("\\d"),
                    "{id}: capture group {group:?} is neither a standard numeric form nor a closed set"
                );
                i = j;
            } else {
                i += 1;
            }
        }
    }
}
