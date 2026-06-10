//! `overlay/skill_overrides.json` 加载 + merge 的集成测试（真实仓库数据）。
//!
//! M0-W4a/W4b 搬迁不变式：`base/granted_effect_levels.json` /
//! `base/granted_effect_stat_sets.json` 还原为**纯 adapter 产物**（`.dat` 导出
//! 不含 critChance / attackSpeedMultiplier / baseMultiplier / statSet Speed MORE），
//! 这些 vendor 值改由 overlay 在加载期 merge——本测试锁定「纯 base + merge 后的
//! 生效值」与历史手补 base 的逐值等价（聚合行数 + 代表性技能点值）。

use pobr_gamedata::GameData;

fn repo_game_data() -> GameData {
    GameData::new(pobr_gamedata::repo_data_root().join("4.5.0.3.4"))
}

/// overlay 文档可加载，且至少覆盖四类 stat。
#[test]
fn loads_skill_overrides_overlay() {
    let overrides = repo_game_data()
        .skill_overrides()
        .expect("加载 skill_overrides overlay")
        .expect("仓库数据包应包含 overlay/skill_overrides.json");
    let has = |stat: &str| overrides.overrides.iter().any(|o| o.stat == stat);
    assert!(has("crit_chance"));
    assert!(has("attack_speed_multiplier"));
    assert!(has("base_multiplier"));
    assert!(has("skill_attack_speed_more"));
}

/// 等级域 merge 后的聚合行数——与历史手补 base 的覆盖规模逐值等价
/// （crit_chance 3911：原 3912 中 FireRuneFireDjinn L2 的 7.0 是历史填充伪影，
/// vendor 该级**无** critChance（导出器逐级写值、省略即无值），已修正）。
#[test]
fn merged_levels_match_historical_coverage() {
    let levels = repo_game_data()
        .granted_effect_levels()
        .expect("加载 + merge granted_effect_levels");

    let count = |f: fn(&pobr_data::catalog::SkillLevelDef) -> bool| {
        levels.values().flatten().filter(|r| f(r)).count()
    };
    assert_eq!(count(|r| r.crit_chance.is_some()), 3911);
    assert_eq!(count(|r| r.attack_speed_multiplier.is_some()), 3578);
    assert_eq!(count(|r| r.base_multiplier.is_some()), 6821);
}

/// 代表性点值（与 vendor PoB2 Lua 逐字对照）。
#[test]
fn merged_levels_spot_values() {
    let data = repo_game_data();
    let levels = data.granted_effect_levels().expect("加载等级域");

    // Flicker Strike：attackSpeedMultiplier -50（全等级同值）。
    let flicker = &levels["FlickerStrikePlayer"];
    assert!(
        flicker
            .iter()
            .all(|r| r.attack_speed_multiplier == Some(-50.0))
    );

    // Arc：critChance 9（全等级同值）。
    let arc = &levels["ArcPlayer"];
    assert!(arc.iter().all(|r| r.crit_chance == Some(9.0)));

    // RisenArbalestSnipe：vendor 仅 L1 有 baseMultiplier 2.65——per_level 明细
    // 不得把缺失等级误填（压缩条件修复的回归锚点）。
    let snipe = &levels["RisenArbalestSnipe"];
    assert_eq!(snipe[0].base_multiplier, Some(2.65));
    assert!(snipe[1..].iter().all(|r| r.base_multiplier.is_none()));

    // statSet 级：Flicker 固有攻击速度 MORE 285（PoB2 baseMods 常量）。
    let sets = data.skill_stat_sets().expect("加载 stat-set 域");
    let flicker_set = sets
        .iter()
        .find(|s| s.id == "FlickerStrikePlayer")
        .expect("FlickerStrikePlayer stat-set 存在");
    assert_eq!(flicker_set.skill_attack_speed_more, Some(285.0));
}

/// 纯 base（`.dat` 导出无这些列）不含任何 overlay 字段——确保值只来自 merge，
/// 不再有手补漂移（regen-check byte-diff 零的语义对应）。
#[test]
fn pure_base_has_no_overlay_fields() {
    // 构造无 overlay 的临时数据目录（base 软链到仓库文件，避免大文件复制）。
    let dir = std::env::temp_dir().join(format!(
        "pobr-gamedata-skill-overrides-pure-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("base")).unwrap();
    let repo_base = pobr_gamedata::repo_data_root().join("4.5.0.3.4/base");
    for f in [
        "granted_effect_levels.json",
        "granted_effect_stat_sets.json",
    ] {
        std::os::unix::fs::symlink(repo_base.join(f), dir.join("base").join(f)).unwrap();
    }

    let data = GameData::new(&dir);
    assert!(
        data.skill_overrides().unwrap().is_none(),
        "无 overlay 文件时应返回 None（行为 = 纯 base）"
    );
    let levels = data.granted_effect_levels().expect("纯 base 可加载");
    assert!(
        levels.values().flatten().all(|r| r.crit_chance.is_none()
            && r.attack_speed_multiplier.is_none()
            && r.base_multiplier.is_none()),
        "纯 base 不得含 overlay 字段（这些列在 .dat 导出中不存在）"
    );
    let sets = data.skill_stat_sets().expect("纯 base stat-set 可加载");
    assert!(sets.iter().all(|s| s.skill_attack_speed_more.is_none()));
}
