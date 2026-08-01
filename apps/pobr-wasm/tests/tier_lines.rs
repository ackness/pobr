//! Integration tests for affix tier annotation (real data).
//!
//! - The current data version (`data/CURRENT`, which has mods'
//!   `group`/`spawn_weights` plus the StatDescriptions overlay): a rare
//!   item's explicit lines should carry the `tier` field;
//! - The golden version (4.5.0.3.4, no pool data): the `tier` field is
//!   omitted entirely (backwards compatible, the contract shape doesn't change).

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

/// Graceful degradation: a data pack lacking the tier pool fields
/// (group/spawn_weights) doesn't get tiers labeled. Explicitly pinned to
/// 4.5.0.3.4 — this is a fixture pin for "an old-format pack predating the
/// tier data channel" (not a data-content-count pin, so it doesn't go into
/// the test_pins snapshot); the golden version (4.5.4.3 onward) already has
/// pool data and would label tiers, so this degradation test case can't
/// borrow `GOLDEN_PARITY_DATA_VERSION`. Skipped once the old pack gets cleaned up.
#[test]
fn golden_data_without_pool_fields_omits_tiers() {
    let dir = repo_data_root().join("4.5.0.3.4");
    if !dir.is_dir() {
        eprintln!("SKIP: 旧格式 fixture 数据包 4.5.0.3.4 不在仓库（已清理）");
        return;
    }
    let lines = classify(dir.to_str().unwrap());
    assert!(
        lines.iter().all(|l| l.get("tier").is_none()),
        "无池字段的数据包不应标 tier"
    );
}
