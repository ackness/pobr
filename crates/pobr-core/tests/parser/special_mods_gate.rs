//! Gate tests for special_mods.
//!
//! Reads the repo's `data/overlay-common/special_mods.json` (the version-independent
//! curation layer, P1-3) + `data/<ver>/{overlay/special_mods.json, generated/special_derived.json}`,
//! and asserts:
//! 1. [`SpecialModRules::compile`] succeeds fully (pattern is valid / mod_type is known /
//!    enum references are in range / ids are unique);
//! 2. every `handler_id` is registered (unregistered = test failure + a printed list of
//!    unmapped ids — turning "unmapped" warnings into a hard gate);
//! 3. `registry.len() < 100` (the architecture §5 monitoring line);
//! 4. handler entry count / total special entries < 10% (approaching it counts as a split failure);
//! 5. ids are unique + compiled patterns are unique (two equivalent pattern strings count as a conflict);
//! 6. the `verified:false` count is printed (a report, not an assertion).
//!
//! When special_derived.json is missing, its concatenation step is skipped (will be included
//! once it lands).

use std::collections::BTreeMap;

use pobr_core::rules::{HandlerRegistry, SpecialModRules};
use pobr_data::catalog::parser_rules::{SpecialModsDef, SpecialTemplateDef};

fn overlay_common_special_mods_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join("overlay-common/special_mods.json")
}
fn special_mods_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
        .join("overlay/special_mods.json")
}
fn special_derived_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
        .join("generated/special_derived.json")
}
fn special_vendor_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
        .join("generated/special_vendor.json")
}

/// Load the repo's special entries (overlay-common version-independent layer + version
/// overlay + optional generated derived/vendor batches, concatenated — same order as
/// pobr-gamedata's `load_ruleset`). The overlay-common layer (P1-3) forms the base by id;
/// the version layer overrides / appends on top.
fn load_entries() -> Vec<SpecialTemplateDef> {
    let mut entries: Vec<SpecialTemplateDef> = Vec::new();
    if let Ok(raw) = std::fs::read_to_string(overlay_common_special_mods_path()) {
        let doc: SpecialModsDef =
            serde_json::from_str(&raw).expect("overlay-common/special_mods.json should parse");
        entries = doc.entries;
    }
    let raw =
        std::fs::read_to_string(special_mods_path()).expect("special_mods.json should be readable");
    let doc: SpecialModsDef = serde_json::from_str(&raw).expect("special_mods.json should parse");
    for v in doc.entries {
        match entries.iter_mut().find(|e| e.id == v.id) {
            Some(slot) => *slot = v,
            None => entries.push(v),
        }
    }
    if let Ok(raw) = std::fs::read_to_string(special_derived_path()) {
        let derived: SpecialModsDef =
            serde_json::from_str(&raw).expect("special_derived.json should parse");
        entries.extend(derived.entries);
    }
    if let Ok(raw) = std::fs::read_to_string(special_vendor_path()) {
        let vendor: SpecialModsDef =
            serde_json::from_str(&raw).expect("special_vendor.json should parse");
        entries.extend(vendor.entries);
    }
    entries
}

/// All registered special handlers (`register_special_handlers`).
/// Used by the `all_handler_ids_registered` gate to check every `handler_id` entry is registered.
fn special_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    pobr_core::rules::register_special_handlers(&mut registry)
        .expect("special handler registration should not conflict");
    registry
}

#[test]
fn special_mods_compile_clean() {
    let entries = load_entries();
    let registry = special_registry();
    let rules = SpecialModRules::compile(&entries, &registry)
        .expect("all repo special entries should compile cleanly (pattern/mod_type/enums/id gate)");
    assert_eq!(
        rules.len(),
        entries.len(),
        "compiled entry count should equal the input entry count"
    );
}

#[test]
fn all_handler_ids_registered() {
    let entries = load_entries();
    let registry = special_registry();
    let unmapped: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.handler_id.as_deref())
        .filter(|id| registry.get(id).is_none())
        .collect();
    assert!(
        unmapped.is_empty(),
        "unmapped handler_id (needs registering in register_special_handlers): {unmapped:?}"
    );
}

#[test]
fn handler_registry_under_monitoring_line() {
    let registry = special_registry();
    assert!(
        registry.len() < 100,
        "handler entry count {} should be < 100 (architecture §5 monitoring line)",
        registry.len()
    );
}

#[test]
fn handler_ratio_under_ten_percent() {
    let entries = load_entries();
    let handler_count = entries.iter().filter(|e| e.handler_id.is_some()).count();
    let total = entries.len().max(1);
    let ratio = handler_count as f64 / total as f64;
    assert!(
        ratio < 0.10,
        "handler ratio {ratio:.3} ({handler_count}/{total}) should be < 10% (approaching it counts as a split failure)"
    );
}

#[test]
fn ids_and_patterns_unique() {
    let entries = load_entries();
    let mut ids = BTreeMap::new();
    let mut patterns = BTreeMap::new();
    for e in &entries {
        *ids.entry(e.id.clone()).or_insert(0usize) += 1;
        *patterns.entry(e.pattern.clone()).or_insert(0usize) += 1;
    }
    let dup_ids: Vec<_> = ids
        .iter()
        .filter(|(_, c)| **c > 1)
        .map(|(k, _)| k)
        .collect();
    let dup_patterns: Vec<_> = patterns
        .iter()
        .filter(|(_, c)| **c > 1)
        .map(|(k, _)| k)
        .collect::<Vec<_>>();
    assert!(dup_ids.is_empty(), "duplicate id: {dup_ids:?}");
    assert!(
        dup_patterns.is_empty(),
        "duplicate pattern: {dup_patterns:?}"
    );
}

/// A report of the `verified:false` count (not an assertion — acceptance is judged by the
/// trend/spot-checks, not a hard percentage threshold).
#[test]
fn report_verified_distribution() {
    let entries = load_entries();
    let total = entries.len();
    let verified = entries.iter().filter(|e| e.verified).count();
    let unverified = total - verified;
    let handler = entries.iter().filter(|e| e.handler_id.is_some()).count();
    let template = entries.iter().filter(|e| !e.mods.is_empty()).count();
    let pure_recognise = entries
        .iter()
        .filter(|e| e.mods.is_empty() && e.handler_id.is_none())
        .count();
    println!(
        "[special_mods] total={total} verified={verified} unverified={unverified} \
         template={template} handler={handler} pure_recognise={pure_recognise}"
    );
}
