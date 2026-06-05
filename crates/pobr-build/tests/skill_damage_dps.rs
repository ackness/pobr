//! 宝石数据通道 Phase 3 端到端验证：技能分等级**基础伤害** → DPS。
//!
//! 用真实入库数据（`data/4.5.0.3.4/granted_effect_stat_sets.json`）验证
//! 「`<Gem skillId>` + 等级 → stat-set 基础伤害 → `<Type>DamageMin/Max` BASE 词条
//! → 伤害分量 → DPS」整条通道。基准技能 Fireball（纯法术、基础伤害仅来自 stat-set，
//! 不依赖未接的武器伤害），其 L20 基础火焰伤害 224–336（与 PoB 自身
//! `Data/Skills/act_int.lua` 解析后逐字一致），平均击中 280。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn fireball_build(gem_level: u32) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/Fireball")
                .with_active_skill("FireballPlayer", gem_level),
        )
}

fn panel_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![],
    }
}

/// Fireball L20：基础火焰伤害 224–336（avg 280），DPS = avg × 行动速率 × 命中率 > 0。
#[test]
fn fireball_base_damage_drives_nonzero_dps() {
    let build_data = load_build_data();

    // 前置：stat-set 域确实载入了 Fireball 的分等级伤害（数据通道未断）。
    let resolved = build_data
        .resolve_skill_level("FireballPlayer", 20)
        .expect("FireballPlayer should resolve");
    assert!(
        resolved
            .base_damage
            .iter()
            .any(|d| d.stat == "spell_minimum_base_fire_damage" && d.value == 224.0),
        "expected L20 spell_minimum_base_fire_damage = 224, got {:?}",
        resolved.base_damage
    );

    let build = fireball_build(20);
    let out = calculate_with_data(&build, &build_data, &panel_opts())
        .expect("calculate_with_data should succeed for Fireball build");

    // 无任何 increased/more 来源 → 平均击中 = (224 + 336) / 2 = 280。
    assert!(
        (out.total_hit_avg - 280.0).abs() < 1.0,
        "Fireball L20 average hit should be ~280, got {}",
        out.total_hit_avg
    );
    // 行动速率来自 cast time 1.2s → 1/1.2 ≈ 0.833；命中率 > 0 → DPS > 0。
    assert!(
        out.dps > 0.0,
        "Fireball DPS should be > 0 once base damage is injected, got {}",
        out.dps
    );
    assert!(out.action_rate > 0.0, "action_rate should be > 0");
}

/// 等级缩放：L1 基础伤害（8–12，avg 10）远低于 L20（avg 280），且均 > 0。
#[test]
fn fireball_damage_scales_with_gem_level() {
    let build_data = load_build_data();
    let opts = panel_opts();

    let l1 = calculate_with_data(&fireball_build(1), &build_data, &opts).expect("L1 calc");
    let l20 = calculate_with_data(&fireball_build(20), &build_data, &opts).expect("L20 calc");

    assert!(
        (l1.total_hit_avg - 10.0).abs() < 1.0,
        "Fireball L1 avg hit should be ~10, got {}",
        l1.total_hit_avg
    );
    assert!(
        l20.total_hit_avg > l1.total_hit_avg * 10.0,
        "L20 hit ({}) should vastly exceed L1 ({})",
        l20.total_hit_avg,
        l1.total_hit_avg
    );
}
