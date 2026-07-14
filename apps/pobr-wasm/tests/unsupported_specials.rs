//! 药剂/护符使用效果类词条经 special 表解析为正确机制语义，不再进未支持报表：
//! 免疫 → *Immune flag、Guard → GuardAbsorbRate/Limit（承受层真实吸收）、
//! 瞬时回魔/魔力溢出/附身 → 结构化 mod 登记（vendor 对照与来源见 overlay source_note）。

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

const CHARM: &str = "Rarity: UNIQUE\nRite of Passage\nGolden Charm\nImplicits: 1\nUsed when you kill a Rare or Unique enemy\nPossessed by Spirit Of The Stag for 19 seconds on use\nImmune to Ignite\nRecover 295 Mana when Used\nMana Recovery from Flasks can Overflow maximum Mana during Effect\nImmune to Freeze\nAlso grants 481 Guard";
/// charm 并入需要 CharmLimit 预算（无腰带 charm 槽时预算 0、charm 全不生效）。
const BELT: &str = "Rarity: NORMAL\nHeavy Belt\nHas 1 Charm Slot";

fn calculate(with_charm: bool) -> Value {
    let flasks = if with_charm {
        json!([{ "slot": "Charm 1", "text": CHARM }])
    } else {
        json!([])
    };
    let request = json!({
        "character": { "level": 90, "class_name": "Monk", "ascendancy_name": "" },
        "allocated_nodes": [],
        "socket_groups": [],
        "items": [{ "slot": "belt", "text": BELT }],
        "flasks": flasks,
        "config_inputs": {},
    });
    let out = pobr_wasm::calculate_build_json(&request.to_string()).expect("calculate");
    serde_json::from_str(&out).expect("parse")
}

fn stat(resp: &Value, id: &str) -> f64 {
    resp["stats"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["id"] == id)
        .and_then(|s| s["value"].as_f64())
        .unwrap_or(0.0)
}

#[test]
fn charm_flask_use_effect_lines_are_recognized() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let response = calculate(true);
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

#[test]
fn guard_line_extends_hit_pool() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let without = calculate(false);
    let with = calculate(true);
    // Guard 是生命/ES 前的全额吸收池（agent-docs/active-defences.md §2）：
    // 物理 max hit 应至少提高 Guard 池大小（481）。
    let delta = stat(&with, "PhysicalMaxHit") - stat(&without, "PhysicalMaxHit");
    assert!(
        delta >= 481.0,
        "Guard 481 应扩大物理 max hit（实际增量 {delta}）"
    );
}
