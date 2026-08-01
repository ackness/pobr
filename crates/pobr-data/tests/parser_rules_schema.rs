//! `mod_parser_rules/v1` schema round-trip unit tests.
//!
//! The fixture `tests/fixtures/mini_parser_rules.json` is a mini rule set
//! for the A→B contract: each section has ≥3 entries **taken byte-for-byte
//! from real extraction output** (covering both the closure-inferred
//! template shape and the handler-fallback shape), so the scan engine can
//! be developed before real extraction output lands.

use pobr_data::catalog::parser_rules::ModParserRulesDoc;
use pobr_data::catalog::stat_map::StatMapValue;

const MINI_FIXTURE: &str = include_str!("fixtures/mini_parser_rules.json");

fn load_mini() -> ModParserRulesDoc {
    serde_json::from_str(MINI_FIXTURE).expect("mini fixture should deserialize (_meta ignored)")
}

/// Deserialize → re-serialize → re-deserialize, structurally equivalent
/// (the serde shape is self-consistent; the `_meta` header is ignored per
/// the consumer's convention).
#[test]
fn roundtrips_mini_fixture() {
    let doc = load_mini();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let back: ModParserRulesDoc = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc);
}

/// Each section's entry count satisfies the contract (≥3; the two
/// unsupported sections each have 1).
#[test]
fn mini_fixture_section_sizes() {
    let doc = load_mini();
    assert!(doc.forms.len() >= 3);
    assert!(doc.name_map.len() >= 3);
    assert!(doc.flag_phrases.len() >= 3);
    assert!(doc.pre_flags.len() >= 3);
    assert!(doc.tag_phrases.len() >= 3);
    assert!(doc.suffix_types.len() >= 3);
    assert!(doc.damage_types.len() >= 3);
    assert!(doc.pen_types.len() >= 3);
    assert!(doc.regen_types.len() >= 3);
    assert!(doc.degen_types.len() >= 3);
    assert!(doc.cost_types_map.len() >= 3);
    assert!(doc.base_cost_types.len() >= 3);
    assert!(doc.flag_types.len() >= 3);
    assert_eq!(doc.unsupported, vec!["mirrored"]);
    assert_eq!(doc.unsupported_pobr_extra, vec!["split"]);
}

/// Pins the key shapes: tag templates (flattened fields) / closure-inferred
/// placeholders / handler fallback / flagTypes' two shapes.
#[test]
fn mini_fixture_shape_pins() {
    let doc = load_mini();

    // TagTemplate flatten: type extracted out, the rest of the fields go into a BTreeMap
    let mana_cost = doc
        .name_map
        .iter()
        .find(|e| e.phrase == "mana cost of attacks")
        .unwrap();
    let tag = &mana_cost.effects.tags[0];
    assert_eq!(tag.tag_type, "SkillType");
    assert_eq!(
        tag.fields.get("skill_type"),
        Some(&StatMapValue::Text("Attack".into()))
    );

    // A closure-inferred template: the placeholder lands in StatMapValue::Text
    let per_rage = doc
        .tag_phrases
        .iter()
        .find(|e| e.pattern == "per (%d+) rage")
        .unwrap();
    assert!(per_rage.inferred);
    assert_eq!(
        per_rage.effects.tags[0].fields.get("div"),
        Some(&StatMapValue::Text("$1".into()))
    );

    // A string-concatenation template
    let effect = doc
        .tag_phrases
        .iter()
        .find(|e| e.pattern == "per (%d+)%% (%a+) effect on enemy")
        .unwrap();
    assert_eq!(
        effect.effects.tags[0].fields.get("var"),
        Some(&StatMapValue::Text("$2:cap+Effect".into()))
    );

    // The handler-fallback shape: handler_id present and effects empty
    let rampage = doc
        .tag_phrases
        .iter()
        .find(|e| e.pattern == "per (%d+) rampage kills")
        .unwrap();
    assert!(
        rampage
            .handler_id
            .as_deref()
            .is_some_and(|id| id.starts_with("tag_phrase:"))
    );
    assert!(!rampage.inferred);
    assert!(rampage.effects.tags.is_empty());

    // pre_flags wrapping directive + modSuffix
    let take = doc
        .pre_flags
        .iter()
        .find(|e| e.pattern == "^take ")
        .unwrap();
    assert_eq!(take.effects.mod_suffix.as_deref(), Some("Taken"));
    let minions = doc
        .pre_flags
        .iter()
        .find(|e| e.pattern == "^minions [cthd][ae][ukva][sel]e? ")
        .unwrap();
    assert!(minions.effects.add_to_minion);

    // flagTypes' two shapes: a condition string vs. an embedded mod (hexproof)
    let phasing = doc
        .flag_types
        .iter()
        .find(|e| e.phrase == "phasing")
        .unwrap();
    assert_eq!(phasing.condition.as_deref(), Some("Condition:Phasing"));
    assert!(phasing.mod_def.is_none());
    let hexproof = doc
        .flag_types
        .iter()
        .find(|e| e.phrase == "hexproof")
        .unwrap();
    assert!(hexproof.condition.is_none());
    let mod_def = hexproof.mod_def.as_ref().unwrap();
    assert_eq!(
        (
            mod_def.name.as_str(),
            mod_def.mod_type.as_str(),
            mod_def.value
        ),
        ("CurseEffectOnSelf", "MORE", -100.0)
    );
}

/// Default fields (serde default): minimal JSON still deserializes.
#[test]
fn defaults_tolerate_minimal_entries() {
    let json = r#"{
        "forms": [{ "pattern": "^x", "form": "BASE" }],
        "name_map": [{ "phrase": "x", "names": ["X"] }],
        "flag_phrases": [{ "phrase": "x" }],
        "pre_flags": [{ "pattern": "^x" }],
        "tag_phrases": [{ "pattern": "x" }]
    }"#;
    let doc: ModParserRulesDoc = serde_json::from_str(json).unwrap();
    assert_eq!(doc.forms[0].literal, None);
    assert!(!doc.forms[0].anchored);
    assert!(doc.name_map[0].effects.tags.is_empty());
    assert!(!doc.pre_flags[0].inferred);
    assert!(doc.tag_phrases[0].handler_id.is_none());
    assert!(doc.suffix_types.is_empty());
    assert!(doc.unsupported.is_empty());
}
