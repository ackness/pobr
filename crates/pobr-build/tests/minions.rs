//! 召唤物链路端到端集成测试（M5a Track A/B/C）。
//!
//! 覆盖：BuildData 召唤物查询 API（A5）、orchestrator 识别召唤宝石 →
//! `OutputTable.minions` 非空（B2）、createMinionSkills + 主技能喂 offence（C1/C2）、
//! modDB 装配补全（C3）。golden 数值取 PoB2 同参数面板（oracle 中间值注明）。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::build_minion_context_from_def;
use pobr_core::calc::minion::{
    AttributeInfusion, minion_level_from_gem_level, resolve_minion_level,
};
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(VERSION));
    BuildData::load(&data).expect("加载 4.5.0.3.4 数据")
}

fn default_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    }
}

/// 一个仅含 Raise Zombie（召唤宝石等级 N）主动技能的 Witch build。
fn zombie_build(gem_level: u32) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemRaiseZombie")
                .with_gem_skill("RaiseZombiePlayer", gem_level),
        )
}

// ---------------------------------------------------------------------------
// A5：BuildData 召唤物查询 API
// ---------------------------------------------------------------------------

#[test]
fn build_data_minion_def_zombie() {
    let data = load_data();
    let zombie = data.minion_def("RaisedZombie").expect("RaisedZombie 在库");
    assert_eq!(zombie.life, 0.7); // Minions.lua:12
    assert_eq!(zombie.damage, 0.75); // :18
    assert!(zombie.base_damage_ignores_attack_speed);
}

#[test]
fn build_data_minion_def_spectre_falls_back() {
    let data = load_data();
    // spectre key = 完整 metadata 路径（minions.json miss → spectres.json）
    let c = data
        .minion_def("Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre")
        .expect("Lightless Abomination 在库（落 spectres）");
    assert_eq!(c.life, 3.0); // Spectres.lua
    assert_eq!(c.armour, 0.4);
}

#[test]
fn build_data_effect_minion_list() {
    let data = load_data();
    assert_eq!(
        data.effect_minion_list("RaiseZombiePlayer"),
        ["RaisedZombie"]
    );
    assert_eq!(
        data.effect_minion_list("RagingSpiritsPlayer"),
        ["SummonedRagingSpirit"]
    );
    // 非召唤技能 → 空切片
    assert!(data.effect_minion_list("FireballPlayer").is_empty());
    assert!(data.effect_minion_list("NonexistentSkill").is_empty());
}

// ---------------------------------------------------------------------------
// B2：orchestrator 识别召唤宝石 → OutputTable.minions 非空
// ---------------------------------------------------------------------------

#[test]
fn summon_build_populates_output_minions() {
    let data = load_data();
    let out = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("calculate");
    // 召唤宝石被识别 → OutputTable.minions 非空（G4 关闭判据其一）。
    assert!(
        !out.minions.is_empty(),
        "Raise Zombie build 应产出召唤物快照"
    );
    let zombie = &out.minions[0];
    // 召唤物等级 = minionLevelTable[gem_level=20] = 40（CalcActiveSkill.lua:896 默认规则）。
    assert_eq!(zombie.level, minion_level_from_gem_level(20));
    assert_eq!(zombie.level, 40);
    // 生命 > 0（怪物等级基准表 × 0.7 归一化乘数派生）。
    assert!(zombie.life > 0.0, "召唤物生命应 > 0，实测 {}", zombie.life);
}

#[test]
fn non_summon_build_has_empty_minions() {
    let data = load_data();
    // 非召唤主技能（Fireball）→ minions 必须为空（gate 正确：零行为外溢）。
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemFireball")
                .with_gem_skill("FireballPlayer", 20),
        );
    let out = calculate_with_data(&build, &data, &default_opts()).expect("calculate");
    assert!(out.minions.is_empty(), "非召唤 build minions 应为空");
}

/// 召唤物生命 = 怪物盟友生命表(level) × life 归一化乘数（0.7）——与 core
/// `build_minion_context_from_def` 派生口径一致（B2 接线未引入额外缩放）。
#[test]
fn summon_zombie_life_matches_core_derivation() {
    let data = load_data();
    let out = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("calculate");
    let zombie_out = &out.minions[0];

    // 直接用 core 纯函数派生同一召唤物，取其防御管线后的生命做对照。
    let def = data.minion_def("RaisedZombie").expect("RaisedZombie 在库");
    let ctx = build_minion_context_from_def(def, 20, vec![], vec![], AttributeInfusion::default());
    let core_life = ctx.base.life;
    assert_eq!(zombie_out.level, resolve_minion_level(20));
    // OutputTable.minions 的 life 经 defence 管线（life pool）输出，与 base.life 同序；
    // B2 未加任何 minion 词条，二者应一致。
    assert!(
        (zombie_out.life - core_life).abs() < 1.0,
        "orchestrator minion life {} 应 ≈ core 派生 {}",
        zombie_out.life,
        core_life
    );
}
