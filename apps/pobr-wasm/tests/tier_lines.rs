//! 词缀 tier 标注集成测试（真实数据）。
//!
//! - 当前数据版本（`data/CURRENT`，含 mods `group`/`spawn_weights` 与
//!   StatDescriptions overlay）：rare 物品 explicit 行应携带 tier 字段；
//! - golden 版本（4.5.0.3.4，无池数据）：tier 字段整体省略（向后兼容，
//!   契约形状不变）。

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
    assert_eq!(explicits.len(), 2, "两条 explicit 行");
    let tiered: Vec<&&Value> = explicits
        .iter()
        .filter(|l| l.get("tier").is_some())
        .collect();
    assert!(
        !tiered.is_empty(),
        "当前数据版本应至少给一条 explicit 行标出 tier：{explicits:?}"
    );
    for l in &tiered {
        let tier = l["tier"].as_u64().unwrap();
        let total = l["tier_total"].as_u64().unwrap();
        assert!(tier >= 1 && tier <= total, "tier {tier} 应落在 1..={total}");
        let affix = l["affix"].as_str().unwrap();
        assert!(affix == "prefix" || affix == "suffix");
    }
}

#[test]
fn golden_data_without_pool_fields_omits_tiers() {
    let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
    let lines = classify(dir.to_str().unwrap());
    assert!(
        lines.iter().all(|l| l.get("tier").is_none()),
        "旧数据包（无 group/spawn_weights）不应标 tier"
    );
}
