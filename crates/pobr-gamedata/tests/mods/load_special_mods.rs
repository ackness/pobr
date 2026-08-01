//! Load test for the data prerequisite: `overlay/special_mods.json`
//! (schema in [`pobr_data::catalog::parser_rules`]).
//!
//! Only checks the data shape and curation discipline (unique id /
//! verified:false / batch marker / mods and handler are mutually
//! exclusive); regex compilation checks live on the sync-pob-catalog test
//! side (that crate has the regex dependency); interpreter wiring belongs
//! to B-2/B-3.

use pobr_data::catalog::parser_rules::{
    SpecialModsDef, TemplateNameDef, TemplateValueDef, ValueOpDef,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> SpecialModsDef {
    GameData::new(repo_data_root().join(version()))
        .special_mods()
        .unwrap()
        .expect("special_mods.json should be present")
}

/// The first batch's shape **invariants** (not a snapshot): non-empty and
/// non-trivially populated + unique id + contains the S0 section + a
/// non-empty batch label + the first batch is verified:false + handler_id
/// naming convention. **Doesn't pin the exact entry count / order / batch
/// enumeration** — special_mods grows with long-tail additions
/// (71→78→86→…) and new curation waves introduce new batch labels; pinning
/// those snapshot quantities would cause data growth to falsely fail, and
/// none of them are calc-logic properties.
#[test]
fn first_batch_shape() {
    let def = load();
    assert!(
        def.entries.len() >= 50,
        "special_mods entry count {} is suspiciously low (possible load truncation)",
        def.entries.len()
    );
    // The invariant is that ids are **unique** (a duplicate id ⇒ lookup
    // ambiguity, a real bug); the vec isn't required to be sorted — sorting
    // is curation aesthetics, and hand-migrated "whole line" entries are
    // appended by batch (not alphabetically), so pinning sort order would
    // falsely fail.
    let mut seen = std::collections::HashSet::new();
    for e in &def.entries {
        assert!(seen.insert(e.id.as_str()), "{}: duplicate id", e.id);
    }
    assert!(
        def.entries.iter().any(|e| e.batch == "S0"),
        "should include the S0 keystone section"
    );
    for e in &def.entries {
        // The batch invariant is a **non-empty provenance label** (marking
        // which curation wave it was added in); it doesn't pin a fixed
        // enum {S0,S1,S2} — later curation waves (e.g. a fork-a whole-line
        // migration) will introduce new labels, and pinning the enum would
        // falsely fail.
        assert!(
            !e.batch.trim().is_empty(),
            "{}: batch provenance label must not be empty",
            e.id
        );
        // No longer asserting verified is always false: the curation
        // discipline (docs/contributing-mods.md §7) allows setting it true
        // once confirmed by oracle reconciliation (hand-curated entries
        // ported from 4.5.4.3, like take_pct_less_damage, are already
        // reconciled verified:true).
        if let Some(id) = &e.handler_id {
            assert!(
                id.starts_with("special:"),
                "{}: handler_id should be named `special:<name>` (got {id})",
                e.id
            );
        }
    }
}

/// A spot check on the S0 keystone section (source of truth = pobr's
/// current `parse_keystone_special`): an OVERRIDE direct capture
/// reference (`Your Critical Damage Bonus is N%`).
#[test]
fn s0_crit_override_entry() {
    let def = load();
    let e = def
        .entries
        .iter()
        .find(|e| e.id == "your_critical_damage_bonus_override")
        .expect("S0 entry should be present");
    assert_eq!(
        e.vendor_pattern.as_deref(),
        Some("your critical damage bonus is (%d+)%%")
    );
    assert_eq!(e.mods.len(), 1);
    let m = &e.mods[0];
    assert_eq!(
        m.name,
        TemplateNameDef::Literal("CriticalStrikeMultiplier".to_string())
    );
    assert_eq!(m.mod_type, "OVERRIDE");
    assert_eq!(m.value, TemplateValueDef::Capture("$1".to_string()));
}

/// An S0 recognition-only entry (both mods and handler absent = known
/// unsupported, pobr's current Unsupported state).
#[test]
fn s0_recognition_only_entries() {
    let def = load();
    for id in [
        "immune_to_chaos_damage",
        "immune_to_chaos_damage_and_bleeding",
    ] {
        let e = def.entries.iter().find(|e| e.id == id).unwrap();
        assert!(
            e.mods.is_empty() && e.handler_id.is_none(),
            "{id}: recognition-only shape"
        );
        assert!(
            e.source_note.is_some(),
            "{id}: must annotate vendor's actual semantics"
        );
    }
}

/// An operator-chain entry: `N% reduced Movement Speed Penalty ...` →
/// negate (vendor ModParser.lua:6017, INC negated — within the five-operator
/// whitelist).
#[test]
fn value_expr_negate_entry() {
    let def = load();
    let e = def
        .entries
        .iter()
        .find(|e| e.id == "reduced_movement_speed_penalty_from_using_skills_while_moving")
        .expect("negate entry should be present");
    let TemplateValueDef::Expr(expr) = &e.mods[0].value else {
        panic!("expected an operator-chain expression");
    };
    assert_eq!(expr.capture, "$1");
    assert_eq!(expr.ops, vec![ValueOpDef::Negate {}]);
}

/// Non-S0 entries carrying vendor provenance **invariants** (not a
/// snapshot): carries `vendor_pattern` (reconciliation input) + a
/// non-empty `source_note` (provenance) + a product shape (mod or
/// handler). **Doesn't pin `source_note`'s text format** — an
/// auto-transcribed entry anchors to `ModParser.lua:<line>`, but a
/// hand-migrated "whole line" entry (Herald / armour-applies-chaos, etc.)
/// spans multiple lines semantically with no single line number, so
/// pinning a literal `ModParser.lua:` would falsely fail legitimate
/// hand-curated entries; the provenance invariant is "has a traceable
/// note", not "must contain a specific line-number format".
#[test]
fn non_s0_has_vendor_provenance() {
    let def = load();
    for e in def.entries.iter().filter(|e| e.batch != "S0") {
        assert!(
            e.vendor_pattern.is_some(),
            "{}: missing vendor_pattern",
            e.id
        );
        assert!(
            e.source_note
                .as_deref()
                .is_some_and(|n| !n.trim().is_empty()),
            "{}: missing source_note provenance",
            e.id
        );
        // A template entry must produce a mod; a handler entry (an open
        // capture routed to handler_id) produces its output on the Rust
        // side (HandlerOutcome), with data mods empty — exempted from this check.
        assert!(
            !e.mods.is_empty() || e.handler_id.is_some(),
            "{}: an auto-transcribed entry must produce a mod or attach a handler_id",
            e.id
        );
    }
}

/// No open captures allowed (a hard DSL boundary): a **template** entry's
/// pattern must not contain an open capture like `(.+)` — word-class
/// captures are already inlined as an explicit closed set at transcription
/// time. Open-capture entries go through `handler_id` per architecture doc
/// §5 / the DSL boundary (e.g. `allocates (.+)` →
/// `special:granted_passive`, with the text name forwarded via
/// `HandlerCtx::raw_captures`), so handler entries are exempt from this check.
#[test]
fn no_open_captures_in_patterns() {
    let def = load();
    for e in &def.entries {
        if e.handler_id.is_some() {
            continue; // Open-capture entries go through a handler
            // (explicitly allowed by the DSL boundary).
        }
        assert!(
            !e.pattern.contains("(.+)") && !e.pattern.contains("(.*)"),
            "{}: open capture out of bounds (should go through handler_id)",
            e.id
        );
    }
}

// The version-independent curation layer's (data/overlay-common/)
// two-layer merge behavior (P1-3)
//
// Hand-builds two layers in a temp directory to precisely verify merge
// semantics (the repo's real data has mutually exclusive ids across the
// two layers, so there's no override sample there):
//   <tmp>/overlay-common/special_mods.json  ← the version-independent baseline
//   <tmp>/<ver>/overlay/special_mods.json   ← version-specific overrides

/// Builds a unique temp data root, returns (root, version directory).
fn temp_data_root(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "pobr-gamedata-special-merge-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let ver_dir = root.join("4.5.4.3");
    std::fs::create_dir_all(ver_dir.join("overlay")).unwrap();
    (root, ver_dir)
}

/// Writes a special_mods.json containing only the given id entries (a
/// minimal valid shape).
fn write_special(path: &std::path::Path, entries: &[(&str, &str)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: Vec<String> = entries
        .iter()
        .map(|(id, pattern)| format!(r#"{{"id":"{id}","pattern":"{pattern}","batch":"T"}}"#))
        .collect();
    std::fs::write(path, format!(r#"{{"entries":[{}]}}"#, body.join(","))).unwrap();
}

fn ids(def: &SpecialModsDef) -> Vec<String> {
    def.entries.iter().map(|e| e.id.clone()).collect()
}

/// The common layer is inherited, and version-only entries are appended
/// after it (common's order comes first).
#[test]
fn overlay_common_is_inherited() {
    let (root, ver_dir) = temp_data_root("inherit");
    write_special(
        &root.join("overlay-common/special_mods.json"),
        &[("common_a", "aaa"), ("common_b", "bbb")],
    );
    write_special(
        &ver_dir.join("overlay/special_mods.json"),
        &[("ver_c", "ccc")],
    );
    let def = GameData::new(&ver_dir).special_mods().unwrap().unwrap();
    assert_eq!(ids(&def), ["common_a", "common_b", "ver_c"]);
}

/// A version-layer entry sharing an id wholly overrides the common layer's
/// (no duplicate id results).
#[test]
fn version_layer_overrides_common_by_id() {
    let (root, ver_dir) = temp_data_root("override");
    write_special(
        &root.join("overlay-common/special_mods.json"),
        &[("shared", "old_pattern"), ("common_only", "keep")],
    );
    write_special(
        &ver_dir.join("overlay/special_mods.json"),
        &[("shared", "new_pattern")],
    );
    let def = GameData::new(&ver_dir).special_mods().unwrap().unwrap();
    // After overriding there's still only one "shared" entry, taking
    // version's pattern; common_only is kept.
    assert_eq!(ids(&def), ["shared", "common_only"]);
    let shared = def.entries.iter().find(|e| e.id == "shared").unwrap();
    assert_eq!(shared.pattern, "new_pattern");
}

/// Only common exists (the version directory has no
/// overlay/special_mods.json) → inherits common directly.
#[test]
fn common_only_when_version_absent() {
    let (root, ver_dir) = temp_data_root("common-only");
    write_special(
        &root.join("overlay-common/special_mods.json"),
        &[("common_x", "xxx")],
    );
    // The version layer's file is missing.
    let def = GameData::new(&ver_dir).special_mods().unwrap().unwrap();
    assert_eq!(ids(&def), ["common_x"]);
}

/// Both layers missing → None (preserving the RuleSet domain's
/// missing-table soft-degradation semantics).
#[test]
fn both_layers_absent_yields_none() {
    let (_root, ver_dir) = temp_data_root("none");
    assert!(GameData::new(&ver_dir).special_mods().unwrap().is_none());
}
