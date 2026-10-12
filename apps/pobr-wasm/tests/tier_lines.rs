//! Integration tests for affix tier annotation (real data).
//!
//! The active data version has mods' `group`/`spawn_weights` plus the
//! StatDescriptions overlay, so a rare item's explicit lines carry the `tier`
//! field.

use pobr_gamedata::repo_data_root;
use serde_json::Value;

const RARE_ITEM: &str = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Item Level: 81
+190 to maximum Life
+34% to Cold Resistance";

fn classify(version_dir: &str) -> Vec<Value> {
    pobr_wasm::init_data_from_dir(version_dir).expect("init data");
    let json = pobr_wasm::classify_item_lines_json(RARE_ITEM).expect("classify");
    serde_json::from_str(&json).expect("parse lines json")
}

#[test]
fn current_data_annotates_explicit_tiers() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    let lines = classify(dir.to_str().unwrap());
    let explicits: Vec<&Value> = lines.iter().filter(|l| l["kind"] == "explicit").collect();
    assert_eq!(explicits.len(), 2, "should be two explicit lines");
    let tiered: Vec<&&Value> = explicits
        .iter()
        .filter(|l| l.get("tier").is_some())
        .collect();
    assert!(
        !tiered.is_empty(),
        "the current data version should have tagged at least one explicit line with a tier: {explicits:?}"
    );
    for l in &tiered {
        let tier = l["tier"].as_u64().unwrap();
        let total = l["tier_total"].as_u64().unwrap();
        assert!(
            tier >= 1 && tier <= total,
            "tier {tier} should fall within 1..={total}"
        );
        let affix = l["affix"].as_str().unwrap();
        assert!(affix == "prefix" || affix == "suffix");
    }
}
