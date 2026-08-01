//! `overlay/curse_priority.json` 加载测试（-C）。
//!
//! 抽样断言对照 vendor `Modules/Data.lua:274-300` 的 `data.cursePriority`
//! 表（commit `2df5a74`）；缺表容忍（消费侧回退）。

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// 四段分类齐全 + vendor 数值抽查（Data.lua:275-300）。
#[test]
fn curse_priority_sections_and_samples() {
    let def = game_data()
        .curse_priority()
        .expect("curse_priority 可加载")
        .expect("curse_priority.json 在库");

    // per-curse 基值：撰写时 13 条（Temporal Chains=1 … Poacher's Mark=13）
    assert_eq!(def.curse_base.len(), 13);
    assert_eq!(def.curse_base["Temporal Chains"], 1);
    assert_eq!(def.curse_base["Despair"], 8);
    assert_eq!(def.curse_base["Poacher's Mark"], 13);

    // socket 序权重单价
    assert_eq!(def.socket_priority_base, 100);

    // 装备槽名权重：10 槽（Weapon 1=1000 … Ring 3=10000）
    assert_eq!(def.slot_weights.len(), 10);
    assert_eq!(def.slot_weights["Weapon 1"], 1000);
    assert_eq!(def.slot_weights["Body Armour"], 5000);
    assert_eq!(def.slot_weights["Ring 3"], 10000);

    // 来源权重：aura 恒为最高一段
    assert_eq!(def.curse_from_equipment, 11000);
    assert_eq!(def.curse_from_aura, 20000);
    assert!(def.curse_from_aura > def.curse_from_equipment);
}

/// 缺表容忍（缺表容忍）：版本目录无 overlay 表时返回 `Ok(None)` 而非报错。
#[test]
fn curse_priority_tolerates_missing_table() {
    let missing = GameData::new(repo_data_root().join("no-such-version"));
    assert!(
        missing
            .curse_priority()
            .expect("缺表应容忍为 Ok(None)")
            .is_none()
    );
}
