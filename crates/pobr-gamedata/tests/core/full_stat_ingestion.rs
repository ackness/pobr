//! M1-T5.3 全量 stat 入库验收：`granted_effect_stat_sets.json` 不再受
//! adapter 白名单（原后缀谓词，T2.4 已随消费侧兜底一并删除）过滤——曾被过滤的非伤害 stat
//! （范围/持续时间/弹道数等）必须出现在入库 JSON 中（蓝图 T5 验收项）。
//!
//! 搬迁不变式的另一半（消费侧 legacy 过滤保证 ninja 逐值不变）由
//! `pobr-build::legacy_stat_filter` 的单测 + ninja_parity 回归门禁锁定。

use pobr_gamedata::{GameData, repo_data_root};

#[test]
fn previously_filtered_stats_are_now_ingested() {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    let sets = data.skill_stat_sets().expect("load stat sets");

    // IceNova 主 set 的 constantStats 含 `base_skill_effect_duration`（8000）与
    // `active_skill_base_area_of_effect_radius`（32）——均为旧白名单排除的
    // 非伤害 stat（vendor Data/Skills/act_int.lua IceNovaPlayer statSets[1]）。
    let ice = sets
        .iter()
        .find(|s| s.effect_id == "IceNovaPlayer")
        .and_then(|s| s.sets.first())
        .expect("IceNovaPlayer 主 stat set present");
    let has_const = |stat: &str| ice.constant_stats.iter().any(|c| c.stat == stat);
    assert!(
        has_const("base_skill_effect_duration"),
        "曾被过滤的持续时间 stat 应入库"
    );
    assert!(
        has_const("active_skill_base_area_of_effect_radius"),
        "曾被过滤的范围 stat 应入库"
    );

    // 分等级行的非伤害 stat 同样入库（IceNova 每级含 freeze/chill multiplier，
    // 其中 `active_skill_chill_effect_+%_final` 旧白名单不命中）。
    let level1 = ice
        .levels
        .iter()
        .find(|l| l.gem_level == 1)
        .expect("IceNova L1 present");
    assert!(
        level1
            .stats
            .iter()
            .any(|s| s.stat == "active_skill_chill_effect_+%_final"),
        "曾被过滤的分等级 stat 应入库"
    );
    // 既有伤害 stat 不受影响（防回归锚点）。
    assert!(
        level1
            .stats
            .iter()
            .any(|s| s.stat == "spell_minimum_base_cold_damage" && s.value == 6.0),
        "既有伤害 stat 保持不变"
    );
}
