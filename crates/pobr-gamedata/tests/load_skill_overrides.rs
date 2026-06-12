//! `overlay/skill_overrides.json` 加载 + merge 的集成测试（真实仓库数据）。
//!
//! M0-W4a/W4b 搬迁不变式：`base/granted_effect_levels.json` /
//! `base/granted_effect_stat_sets.json` 为**纯 adapter 产物**，`.dat` 拿不到的
//! vendor 值由 overlay 在加载期 merge——本测试锁定「纯 base + merge 后的生效值」
//! 与历史手补 base 的逐值等价（聚合行数 + 代表性技能点值）。
//!
//! M1-T4.3 通道收窄：critChance / attackSpeedMultiplier 改为 `.dat` 表列直读落
//! base/（GrantedEffectStatSetsPerLevel 暴击两列 / GrantedEffectsPerLevel
//! `AttackSpeedMultiplier`），历史 overlay 值 3911 + 3578 条逐值一致验收后切换；
//! overlay 边车仅余 base_multiplier（分等级）+ skill_attack_speed_more（statSet
//! 常量）。表列直读在 vendor 覆盖之外**新增**怪物/非 vendor 技能的值（crit
//! +2180 行、attspd +678 行）——vendor 覆盖技能上零新增、零漂移（T4.3 校验）。

use pobr_gamedata::GameData;

fn repo_game_data() -> GameData {
    GameData::new(pobr_gamedata::repo_data_root().join("4.5.0.3.4"))
}

/// overlay 文档可加载，且已收窄为两类 stat（M1-T4.3：crit/attspd 改表列直读，
/// 不得再出现在边车里——出现即说明 extract 脚本回退）。
#[test]
fn loads_skill_overrides_overlay() {
    let overrides = repo_game_data()
        .skill_overrides()
        .expect("加载 skill_overrides overlay")
        .expect("仓库数据包应包含 overlay/skill_overrides.json");
    let has = |stat: &str| overrides.overrides.iter().any(|o| o.stat == stat);
    assert!(has("base_multiplier"));
    assert!(has("skill_attack_speed_more"));
    assert!(has("dot_is_area"), "M4-T4 W-D1 dotIs* 抽取段不得回退");
    assert!(!has("crit_chance"), "crit 已表列直读，不得回流 overlay");
    assert!(
        !has("attack_speed_multiplier"),
        "attspd 已表列直读，不得回流 overlay"
    );
}

/// 等级域 merge 后的聚合行数。
///
/// crit / attspd（M1-T4.3 表列直读）：vendor 覆盖的历史值 3911 / 3578 条逐值保留
/// （原 3912 中 FireRuneFireDjinn L2 的 7.0 是历史填充伪影，M0-W4b 已修正），表列
/// 直读额外带来怪物/非 vendor 技能的值 → 总数 6091 / 4256；base_multiplier 仍走
/// overlay merge（6821 不变）。M1-T4.2 新增的等级字段族覆盖数取自 W0 下载报告
/// （CostMultiplier≠100 = 542 / Reservation≠0 = 3166 / EffectOnPlayer≠100 = 169 /
/// StoredUses≠0 = 6721）。
#[test]
fn merged_levels_match_historical_coverage() {
    let levels = repo_game_data()
        .granted_effect_levels()
        .expect("加载 + merge granted_effect_levels");

    let count = |f: fn(&pobr_data::catalog::SkillLevelDef) -> bool| {
        levels.values().flatten().filter(|r| f(r)).count()
    };
    assert_eq!(count(|r| r.crit_chance.is_some()), 6091);
    assert_eq!(count(|r| r.attack_speed_multiplier.is_some()), 4256);
    assert_eq!(count(|r| r.base_multiplier.is_some()), 6821);
    // M1-T4.2 等级字段族（adapter 直读，平凡值归一化为 None）。
    assert_eq!(count(|r| r.mana_multiplier.is_some()), 542);
    assert_eq!(count(|r| r.spirit_reservation_flat.is_some()), 3166);
    assert_eq!(count(|r| r.reservation_multiplier.is_some()), 169);
    assert_eq!(count(|r| r.stored_uses.is_some()), 6721);
    // level_requirement：PoE2 `.dat` 无列（真源表不可下载），M1 恒 None、M5a 落库。
    assert_eq!(count(|r| r.level_requirement.is_some()), 0);
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

    // M1-T4.2 等级字段族点值（与 vendor Data/Skills/*.lua 逐字对照）：
    // Acrimony support：manaMultiplier 10（sup_int.lua `manaMultiplier = 10`）。
    let acrimony = &levels["SupportAcrimonyPlayer"];
    assert_eq!(acrimony[0].mana_multiplier, Some(10.0));
    // Alchemist's Boon（持续型）：spiritReservationFlat 30。
    let boon = &levels["AlchemistsBoonPlayer"];
    assert_eq!(boon[0].spirit_reservation_flat, Some(30.0));
    // Impurity：manaMultiplier -100 + reservationMultiplier -100（双倍率归一化保号）。
    let impurity = &levels["ImpurityPlayer"];
    assert_eq!(impurity[0].mana_multiplier, Some(-100.0));
    assert_eq!(impurity[0].reservation_multiplier, Some(-100.0));

    // statSet 级：Flicker 固有攻击速度 MORE 285（PoB2 baseMods 常量）。
    let sets = data.skill_stat_sets().expect("加载 stat-set 域");
    let flicker_set = sets
        .iter()
        .find(|s| s.effect_id == "FlickerStrikePlayer")
        .and_then(|def| def.sets.first())
        .expect("FlickerStrikePlayer 主 stat-set 存在");
    assert_eq!(flicker_set.skill_attack_speed_more, Some(285.0));

    // M4-T4 W-D1：dotIs* 布尔——vendor 全量唯一条目 TornadoShotPlayer
    // statSets[2]（"Tornado"）的 dotIsArea，按 vendor 序号命中
    // TornadoShotNovaPlayer set；主 set 保持保守默认（未核验全 false）。
    let tornado = sets
        .iter()
        .find(|s| s.effect_id == "TornadoShotPlayer")
        .expect("TornadoShotPlayer stat-set 存在");
    let nova = tornado
        .sets
        .iter()
        .find(|s| s.set_id == "TornadoShotNovaPlayer")
        .expect("Tornado 形态 set 存在");
    assert!(nova.dot_flags.area, "dotIsArea 应命中 vendor statSets[2]");
    assert!(nova.dot_flags.verified);
    assert!(
        tornado.sets[0].dot_flags.is_default(),
        "主 set（Impact）未核验，保守默认"
    );
}

/// 纯 base 不含 overlay 专属字段——确保这些值只来自 merge，不再有手补漂移
/// （regen-check byte-diff 零的语义对应）。M1-T4.3 后 overlay 专属字段收窄为
/// base_multiplier（等级域）+ skill_attack_speed_more（statSet 域）；crit /
/// attspd 已是表列直读的 base 字段，不在断言范围。
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
        levels
            .values()
            .flatten()
            .all(|r| r.base_multiplier.is_none()),
        "纯 base 不得含 base_multiplier（该列在 .dat 导出中不存在，值只来自 overlay merge）"
    );
    // 表列直读字段在纯 base 中**应当**存在（与 overlay 无关）——锚定通道切换不可回退。
    assert!(
        levels.values().flatten().any(|r| r.crit_chance.is_some()),
        "crit_chance 是表列直读的 base 字段（M1-T4.3），纯 base 不得为空"
    );
    assert!(
        levels
            .values()
            .flatten()
            .any(|r| r.attack_speed_multiplier.is_some()),
        "attack_speed_multiplier 是表列直读的 base 字段（M1-T4.3），纯 base 不得为空"
    );
    let sets = data.skill_stat_sets().expect("纯 base stat-set 可加载");
    assert!(
        sets.iter()
            .flat_map(|s| &s.sets)
            .all(|s| s.skill_attack_speed_more.is_none())
    );
}
