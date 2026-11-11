//! Gate tests for special_mods.
//!
//! Reads the repo's `data/overlay-common/special_mods.json` (the version-independent
//! curation layer, P1-3) + `data/<ver>/{overlay/special_mods.json, generated/special_derived.json}`,
//! and asserts:
//! 1. [`SpecialModRules::compile`] succeeds fully (pattern is valid / mod_type is known /
//!    enum references are in range / ids are unique);
//! 2. every `handler_id` is registered (reported together before compilation);
//! 3. `registry.len() < 100` (the architecture §5 monitoring line);
//! 4. handler entry count / total special entries < 10% (approaching it counts as a split failure);
//! 5. ids are unique + compiled patterns are unique (two equivalent pattern strings count as a conflict);
//! 6. the `verified:false` count is printed (a report, not an assertion).
//!
//! Optional missing layers are skipped; other I/O errors fail the gate.
//! Raw curated layers and their capture-shape policy are checked separately by
//! sync-pob-catalog, so shadowed entries retain independent validation.

use std::collections::BTreeMap;

use pobr_core::rules::{HandlerRegistry, SpecialModRules};
use pobr_data::catalog::parser_rules::{SpecialModsDef, SpecialTemplateDef};

fn overlay_common_special_mods_path() -> std::path::PathBuf {
    crate::support::test_data_dir().join("../overlay-common/special_mods.json")
}
fn special_mods_path() -> std::path::PathBuf {
    crate::support::test_data_dir().join("overlay/special_mods.json")
}
fn special_derived_path() -> std::path::PathBuf {
    crate::support::test_data_dir().join("generated/special_derived.json")
}
fn special_vendor_path() -> std::path::PathBuf {
    crate::support::test_data_dir().join("generated/special_vendor.json")
}

fn read_layer(path: std::path::PathBuf) -> Option<SpecialModsDef> {
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => panic!("{}: {e}", path.display()),
    };
    Some(serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

/// Match runtime precedence: common entries, version overrides by id, then
/// generated derived/vendor rules. Optional layers may be absent.
fn load_entries() -> Vec<SpecialTemplateDef> {
    let mut entries = read_layer(overlay_common_special_mods_path())
        .map(|doc| doc.entries)
        .unwrap_or_default();
    let version = read_layer(special_mods_path()).expect("read prepared special_mods.json");
    for entry in version.entries {
        match entries.iter_mut().find(|e| e.id == entry.id) {
            Some(slot) => *slot = entry,
            None => entries.push(entry),
        }
    }
    for path in [special_derived_path(), special_vendor_path()] {
        if let Some(doc) = read_layer(path) {
            entries.extend(doc.entries);
        }
    }
    entries
}

/// All registered special handlers (`register_special_handlers`).
/// Shared by the compilation and handler-count checks.
fn special_registry() -> HandlerRegistry {
    crate::support::install_test_skill_types();
    let mut registry = HandlerRegistry::new();
    pobr_core::rules::register_special_handlers(&mut registry)
        .expect("special handler registration should not conflict");
    registry
}

#[test]
fn special_mods_compile_clean() {
    let entries = load_entries();
    report_verified_distribution(&entries);
    let registry = special_registry();
    assert!(
        !entries.is_empty(),
        "effective special rules must not be empty"
    );
    let unmapped: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.handler_id.as_deref())
        .filter(|id| registry.get(id).is_none())
        .collect();
    assert!(
        unmapped.is_empty(),
        "unmapped handler_id (needs registering in register_special_handlers): {unmapped:?}"
    );
    let rules = SpecialModRules::compile(&entries, &registry)
        .expect("all repo special entries should compile cleanly (pattern/mod_type/enums/id gate)");
    assert_eq!(
        rules.len(),
        entries.len(),
        "compiled entry count should equal the input entry count"
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
fn report_verified_distribution(entries: &[SpecialTemplateDef]) {
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
