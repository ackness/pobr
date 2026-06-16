//! pre-M5a 数据前置的加载测试：`overlay/minions.json` / `overlay/spectres.json` /
//! `overlay/granted_effect_minions.json` / `overlay/mirage_configs.json`。
//!
//! 抽样断言全部注明 vendor 行号来源（commit `2df5a74`，见
//! `vendor/.pob2-version.txt`）；搬迁不变式校验对照
//! `pobr_data::minion::minion_def_*` 手抄常量（M5a 蓝图 A2 测试 1）。

use pobr_data::catalog::actors::{LuaValueDef, MinionEntryDef, MinionsDef};
use pobr_data::minion::{
    MinionDef, minion_def_raging_spirit, minion_def_skeletal_storm_mage,
    minion_def_skeletal_warrior, minion_def_zombie,
};
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
}

fn find<'a>(def: &'a MinionsDef, id: &str) -> &'a MinionEntryDef {
    def.minions
        .iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("条目 {id} 缺失"))
}

/// 抽取条目（overlay）与手抄常量（`pobr_data::minion`，2025-06 手抄）的
/// **数值字段**逐值比对——搬迁不变式的逐值校验（M5a 蓝图 A2 测试 1）。
///
/// 已知差异（以 vendor 为准，记录于此）：
/// - 手抄 `monster_tags` 是缩减子集（如 zombie 手抄 4 条 vs vendor
///   Minions.lua:11 全量 10 条）——非数值口径，A6 删手抄时随 schema v2 收齐；
/// - 手抄无 `attack_range`/`accuracy`/`base_movement_speed`/`weapon_type1`
///   （schema v2 新增列），不在比对面。
fn assert_matches_handwritten(entry: &MinionEntryDef, hand: &MinionDef) {
    assert_eq!(entry.name, hand.name, "{}: name", hand.id);
    assert_eq!(entry.life, hand.life, "{}: life", hand.id);
    assert_eq!(entry.damage, hand.damage, "{}: damage", hand.id);
    assert_eq!(
        entry.damage_spread, hand.damage_spread,
        "{}: damage_spread",
        hand.id
    );
    assert_eq!(
        entry.attack_time, hand.attack_time,
        "{}: attack_time",
        hand.id
    );
    assert_eq!(
        entry.crit_chance, hand.crit_chance,
        "{}: crit_chance",
        hand.id
    );
    // 可选字段缺省语义：armour/evasion 缺失 = 1.0，energyShield 缺失 = 0.0
    assert_eq!(
        entry.armour.unwrap_or(1.0),
        hand.armour,
        "{}: armour",
        hand.id
    );
    assert_eq!(
        entry.evasion.unwrap_or(1.0),
        hand.evasion,
        "{}: evasion",
        hand.id
    );
    assert_eq!(
        entry.energy_shield.unwrap_or(0.0),
        hand.energy_shield,
        "{}: energy_shield",
        hand.id
    );
    assert_eq!(entry.fire_resist, hand.fire_resist, "{}: fire", hand.id);
    assert_eq!(entry.cold_resist, hand.cold_resist, "{}: cold", hand.id);
    assert_eq!(
        entry.lightning_resist, hand.lightning_resist,
        "{}: lightning",
        hand.id
    );
    assert_eq!(entry.chaos_resist, hand.chaos_resist, "{}: chaos", hand.id);
    assert_eq!(
        entry.base_damage_ignores_attack_speed, hand.base_damage_ignores_attack_speed,
        "{}: base_damage_ignores_attack_speed",
        hand.id
    );
    assert_eq!(
        entry.limit.as_deref().unwrap_or(""),
        hand.limit.to_pob2_str(),
        "{}: limit",
        hand.id
    );
    assert_eq!(
        entry.spectre_reservation, hand.spectre_reservation,
        "{}: spectre_reservation",
        hand.id
    );
    assert_eq!(
        entry.companion_reservation, hand.companion_reservation,
        "{}: companion_reservation",
        hand.id
    );
    assert_eq!(entry.skill_list, hand.skill_list, "{}: skill_list", hand.id);
}

/// minions.json：32 条（vendor Data/Minions.lua 全量），id 升序。
#[test]
fn minions_count_and_order() {
    let def = game_data().minions().unwrap().expect("minions.json 在库");
    assert_eq!(def.minions.len(), 32, "Minions.lua 条目数");
    let ids: Vec<&str> = def.minions.iter().map(|m| m.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "id 升序");
}

/// 4 条手抄常量逐值校验（搬迁不变式）。
#[test]
fn minions_match_handwritten_constants() {
    let def = game_data().minions().unwrap().unwrap();
    assert_matches_handwritten(find(&def, "RaisedZombie"), &minion_def_zombie());
    assert_matches_handwritten(
        find(&def, "SummonedRagingSpirit"),
        &minion_def_raging_spirit(),
    );
    assert_matches_handwritten(
        find(&def, "RaisedSkeletonWarriors"),
        &minion_def_skeletal_warrior(),
    );
    assert_matches_handwritten(
        find(&def, "RaisedSkeletonStormMage"),
        &minion_def_skeletal_storm_mage(),
    );
}

/// RaisedZombie 逐字段抽查（vendor Minions.lua:9-21 + weaponType1 列）。
#[test]
fn zombie_fields_from_vendor() {
    let def = game_data().minions().unwrap().unwrap();
    let z = find(&def, "RaisedZombie");
    assert_eq!(z.life, 0.7); // Minions.lua:12
    assert_eq!(z.damage, 0.75); // :18
    assert_eq!(z.attack_time, 1.25); // :21
    assert!(z.base_damage_ignores_attack_speed); // :13
    assert_eq!(z.monster_tags.len(), 10, "vendor 全量 10 个 tag（:11）");
    assert_eq!(z.weapon_type1.as_deref(), Some("One Hand Axe"));
}

/// SummonedRagingSpirit 的 modList 完整序列化（R3：mod() 构造全部入参；
/// vendor Minions.lua:68 `mod("Speed", "MORE", 40, 1, 0)`）。
#[test]
fn raging_spirit_mod_list_full_args() {
    let def = game_data().minions().unwrap().unwrap();
    let r = find(&def, "SummonedRagingSpirit");
    assert_eq!(r.mod_list.len(), 1);
    let m = &r.mod_list[0];
    assert_eq!(m.name, "Speed");
    assert_eq!(m.mod_type, "MORE");
    assert_eq!(m.value, LuaValueDef::Number(40.0));
    assert_eq!(m.flags, Some(LuaValueDef::Number(1.0)));
    assert_eq!(m.keyword_flags, Some(LuaValueDef::Number(0.0)));
    assert!(m.tags.is_empty());
}

/// spectres.json：591 条 distinct。
///
/// 注：vendor Data/Spectres.lua 有 593 个赋值块，其中 2 个 key 重复
/// （`Metadata/Monsters/Cenobite/CenobiteBloater/CenobiteBloater` /
/// `Metadata/Monsters/GoreCharger/GoreCharger`）——Lua 表语义后写覆盖，
/// PoB2 运行时同样只见 591 条；本表忠实于运行时语义。
#[test]
fn spectres_count() {
    let def = game_data().spectres().unwrap().expect("spectres.json 在库");
    assert_eq!(def.minions.len(), 591);
}

/// Lightless Abomination 抽查（vendor Spectres.lua:10-30 + :49 modList）：
/// life=3 / armour=0.4 / fireResist=75 / StunDuration OVERRIDE 3。
#[test]
fn spectre_lightless_abomination() {
    let def = game_data().spectres().unwrap().unwrap();
    let c = find(
        &def,
        "Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre",
    );
    assert_eq!(c.name, "Lightless Abomination");
    assert_eq!(c.life, 3.0);
    assert_eq!(c.armour, Some(0.4));
    assert_eq!(c.fire_resist, 75.0);
    assert_eq!(c.spectre_reservation, 99.0);
    assert_eq!(c.monster_category.as_deref(), Some("Demon"));
    let m = &c.mod_list[0];
    assert_eq!(
        (m.name.as_str(), m.mod_type.as_str()),
        ("StunDuration", "OVERRIDE")
    );
    assert_eq!(m.value, LuaValueDef::Number(3.0));
}

/// granted_effect_minions.json：外键边车抽样（M5a 蓝图 A3 测试，≥5 条）。
#[test]
fn granted_effect_minions_samples() {
    let def = game_data()
        .granted_effect_minions()
        .unwrap()
        .expect("granted_effect_minions.json 在库");
    assert!(def.entries.len() >= 25, "外键边车条目数（实测 31）");
    let ids: Vec<&str> = def.entries.iter().map(|e| e.effect_id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "effect_id 升序");
    let find = |id: &str| {
        def.entries
            .iter()
            .find(|e| e.effect_id == id)
            .unwrap_or_else(|| panic!("{id} 缺失"))
    };
    // act_int.lua:16180-16186 RagingSpiritsPlayer.minionList
    assert_eq!(
        find("RagingSpiritsPlayer").minion_list,
        ["SummonedRagingSpirit"]
    );
    // sup_int.lua:6250-6258 TriggeredLivingLightningPlayer.minionList
    assert_eq!(
        find("TriggeredLivingLightningPlayer").minion_list,
        ["LivingLightning"]
    );
    // other.lua:10523/10590-10592 Manifest Weapon：minionList + 借武器槽 + item set
    let manifest = find("ManifestWeaponPlayer");
    assert_eq!(manifest.minion_list, ["ManifestWeapon"]);
    assert_eq!(manifest.minion_uses, ["Weapon 1"]);
    assert!(manifest.minion_has_item_set);
    // 召唤系主动技能至少含骷髅/僵尸两类
    assert!(
        def.entries
            .iter()
            .any(|e| e.minion_list.iter().any(|m| m == "RaisedZombie")),
        "RaisedZombie 外键存在"
    );
    assert!(
        def.entries.iter().any(|e| e
            .minion_list
            .iter()
            .any(|m| m.starts_with("RaisedSkeleton"))),
        "骷髅系外键存在"
    );
}

/// A3 merge：`granted_effects()` 加载期把 `granted_effect_minions.json` 边车
/// 拼进 `GrantedEffectDef.minion_list` 等字段（M5a-A3）。
#[test]
fn granted_effects_merge_minion_list() {
    let effects = game_data().granted_effects().unwrap();
    let by_id: std::collections::HashMap<&str, &_> =
        effects.iter().map(|e| (e.id.as_str(), e)).collect();
    // RaiseZombiePlayer → [RaisedZombie]
    let zombie = by_id
        .get("RaiseZombiePlayer")
        .expect("RaiseZombiePlayer 在库");
    assert_eq!(zombie.minion_list, ["RaisedZombie"]);
    // RagingSpiritsPlayer → [SummonedRagingSpirit]
    assert_eq!(
        by_id
            .get("RagingSpiritsPlayer")
            .expect("RagingSpiritsPlayer 在库")
            .minion_list,
        ["SummonedRagingSpirit"]
    );
    // Manifest Weapon：minion_uses + item set 也 merge 进
    let manifest = by_id
        .get("ManifestWeaponPlayer")
        .expect("ManifestWeaponPlayer 在库");
    assert_eq!(manifest.minion_uses, ["Weapon 1"]);
    assert!(manifest.minion_has_item_set);
    // 非召唤技能（如 Fireball）的 minion_list 应为空（向后兼容）
    if let Some(fb) = by_id.get("FireballPlayer") {
        assert!(fb.minion_list.is_empty(), "非召唤技能 minion_list 空");
    }
}

/// mirage_configs.json：5 条配置（vendor CalcMirages.lua 五分支），
/// mirage_archer 的 stat 名抽查（:74-76）。
#[test]
fn mirage_configs_five_branches() {
    let def = game_data()
        .mirage_configs()
        .unwrap()
        .expect("mirage_configs.json 在库");
    assert_eq!(def.configs.len(), 5);
    let ids: Vec<&str> = def.configs.iter().map(|c| c.mirage_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "generals_cry",
            "mirage_archer",
            "sacred_wisps",
            "saviour_mirage_warriors",
            "tawhoas_chosen"
        ]
    );
    let archer = &def.configs[1];
    assert_eq!(
        archer.trigger.skill_data_flag.as_deref(),
        Some("triggeredByMirageArcher")
    );
    assert_eq!(archer.count_stat.as_deref(), Some("MirageArcherMaxCount"));
    assert_eq!(
        archer.less_damage_stat.as_deref(),
        Some("MirageArcherLessDamage")
    );
    assert_eq!(
        archer.less_attack_speed_stat.as_deref(),
        Some("MirageArcherLessAttackSpeed")
    );
    assert_eq!(
        archer.source_skill_filter.weapon_type.as_deref(),
        Some("Bow")
    );
    assert!(archer.calc_main_skill_offence);
    assert!(archer.handler_id.is_none());
}

/// 缺表容忍（R7）：空目录下全部新域返回 Ok(None) 不 panic。
#[test]
fn missing_overlay_files_yield_none() {
    let dir = std::env::temp_dir().join(format!("pobr-pre-m5a-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data = GameData::new(&dir);
    assert!(data.minions().unwrap().is_none());
    assert!(data.spectres().unwrap().is_none());
    assert!(data.granted_effect_minions().unwrap().is_none());
    assert!(data.mirage_configs().unwrap().is_none());
}
