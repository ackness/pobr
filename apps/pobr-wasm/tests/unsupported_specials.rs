//! 药剂/护符使用效果类词条经 special 表解析为正确机制语义，不再进未支持报表：
//! 免疫 → *Immune flag、瞬时回魔/魔力溢出/附身 → 结构化 mod 登记（vendor 对照
//! 与来源见 overlay source_note）。
//!
//! 例外：`Also grants N Guard` 曾被策展条目 also_grants_guard 建模为承受层吸收，
//! 但 vendor ModParser 根本不解析它——对 PoB2 golden 是幻影 Guard 池（存量 #7
//! 摘除，ritualist EHP 1.10x→1.00x）。与 PoB2 对齐后它回到「未建模 → 响亮进
//! unsupported 报表、不影响 hit pool」的口径。

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
    ] {
        assert!(
            !unsupported.iter().any(|u| u.contains(line)),
            "`{line}` 仍在未支持列表: {unsupported:?}"
        );
    }
    // guard 行未建模（vendor 同样不解析）——必须响亮上报而非静默丢弃。
    assert!(
        unsupported
            .iter()
            .any(|u| u.contains("Also grants 481 Guard")),
        "`Also grants 481 Guard` 应进未支持列表（#7 摘除幻影建模后与 PoB2 口径一致）: {unsupported:?}"
    );
}

#[test]
fn guard_line_does_not_extend_hit_pool() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let without = calculate(false);
    let with = calculate(true);
    // #7 摘除 also_grants_guard 幻影建模后，guard 行不得再扩大 hit pool
    // （vendor 不解析该行；此前的 +481 是 PoBR 单方面多建模的幻影池）。
    let delta = stat(&with, "PhysicalMaxHit") - stat(&without, "PhysicalMaxHit");
    assert!(
        delta.abs() < 1.0,
        "guard 行不应改变物理 max hit（实际增量 {delta}）"
    );
}
