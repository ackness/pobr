//! M1-T5.2 多 statSet 入库验收（蓝图 T5 验收项）：
//! - IceNova effect 含 ≥2 个**附加** set（`IceNovaPlayerOnFrostbolt` /
//!   `IceNovaColdInfusedPlayer`，`.dat` `GrantedEffects.AdditionalStatSets` 列）；
//! - vendor 导出的 set 带非空 label（`overlay/stat_set_labels.json` merge）；
//! - `GrantedEffectDef::additional_stat_set_ids` 外键与 stat-set 域对齐。

use pobr_gamedata::{GameData, repo_data_root};

fn game_data() -> GameData {
    GameData::new(repo_data_root().join("4.5.0.3.4"))
}

#[test]
fn ice_nova_has_additional_sets_with_labels() {
    let sets = game_data().skill_stat_sets().expect("load stat sets");
    let ice = sets
        .iter()
        .find(|s| s.effect_id == "IceNovaPlayer")
        .expect("IceNovaPlayer present");

    // 主 set + ≥2 附加 set（蓝图验收：IceNova ≥2 附加 set）。
    assert!(
        ice.sets.len() >= 3,
        "IceNova 应含主 set + ≥2 附加 set，实得 {}",
        ice.sets.len()
    );
    assert_eq!(ice.sets[0].set_id, "IceNovaPlayer", "sets[0] 恒为主 set");
    let set_ids: Vec<&str> = ice.sets.iter().map(|s| s.set_id.as_str()).collect();
    assert!(set_ids.contains(&"IceNovaPlayerOnFrostbolt"));
    assert!(set_ids.contains(&"IceNovaColdInfusedPlayer"));

    // label 非空（vendor 导出的 set；OnFrostbolt 被 vendor 模板策展跳过 → 无 label，
    // 忠实转录 PoB2 行为）。
    assert_eq!(ice.sets[0].label.as_deref(), Some("Ice Nova"));
    assert_eq!(ice.sets[0].vendor_set_index, Some(1));
    let cold = ice
        .sets
        .iter()
        .find(|s| s.set_id == "IceNovaColdInfusedPlayer")
        .expect("Cold-Infused set");
    assert_eq!(cold.label.as_deref(), Some("Cold-Infused"));
    assert_eq!(
        cold.vendor_set_index,
        Some(2),
        "vendor 模板序号 = PoB2 statSetIndex 语义（OnFrostbolt 被策展跳过）"
    );
    let frostbolt = ice
        .sets
        .iter()
        .find(|s| s.set_id == "IceNovaPlayerOnFrostbolt")
        .expect("OnFrostbolt set");
    assert!(
        frostbolt.vendor_set_index.is_none(),
        "vendor 未导出的 set 不可被 statSetIndex 选中"
    );

    // 附加 set 已做 vendor base-merge：常量 = 主 set 常量 ++ 本 set 常量。
    assert!(
        cold.constant_stats.len() >= ice.sets[0].constant_stats.len(),
        "附加 set 常量应含主 set 拼接"
    );
    assert!(
        cold.constant_stats
            .iter()
            .any(|c| c.stat == "base_skill_effect_duration"),
        "主 set 常量拼接进附加 set（skills.lua:502-504）"
    );
}

#[test]
fn granted_effect_additional_stat_set_ids_align() {
    let data = game_data();
    let effects = data.granted_effects().expect("load effects");
    let ice = effects
        .iter()
        .find(|e| e.id == "IceNovaPlayer")
        .expect("IceNovaPlayer effect");
    assert_eq!(
        ice.additional_stat_set_ids,
        ["IceNovaPlayerOnFrostbolt", "IceNovaColdInfusedPlayer"],
        "AdditionalStatSets 外键解析为稳定 id（列序保留）"
    );
}
