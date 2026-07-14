//! 药剂/护符使用效果类词条经 special 表识别，不再进未支持报表
//! （两条免疫产 flag，四条纯识别零 mod；vendor 对照见 overlay source_note）。

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

#[test]
fn charm_flask_use_effect_lines_are_recognized() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let charm = "Rarity: UNIQUE\nRite of Passage\nGolden Charm\nImplicits: 1\nUsed when you kill a Rare or Unique enemy\nPossessed by Spirit Of The Stag for 19 seconds on use\nImmune to Ignite\nRecover 295 Mana when Used\nMana Recovery from Flasks can Overflow maximum Mana during Effect\nImmune to Freeze\nAlso grants 481 Guard";
    let request = json!({
        "character": { "level": 90, "class_name": "Monk", "ascendancy_name": "" },
        "allocated_nodes": [],
        "socket_groups": [],
        "items": [],
        "flasks": [{ "slot": "Charm 1", "text": charm }],
        "config_inputs": {},
    });
    let out = pobr_wasm::calculate_build_json(&request.to_string()).expect("calculate");
    let response: Value = serde_json::from_str(&out).expect("parse");
    let unsupported: Vec<&str> = response["unsupported_modifiers"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    for line in [
        "Immune to Ignite",
        "Immune to Freeze",
        "Recover 295 Mana when Used",
        "Mana Recovery from Flasks can Overflow maximum Mana during Effect",
        "Possessed by Spirit Of The Stag for 19 seconds on use",
        "Also grants 481 Guard",
    ] {
        assert!(
            !unsupported.iter().any(|u| u.contains(line)),
            "`{line}` 仍在未支持列表: {unsupported:?}"
        );
    }
}
