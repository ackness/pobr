//! End-to-end verification of the corpse-explosion base damage channel
//! (vendor `CalcOffence.lua:2211-2217`).
//!
//! Detonate Dead's hit base = `monsterLifeTable[enemyLevel] ×
//! corpseExplosionLifeMultiplier` (per-level gem stat
//! `corpse_explosion_monster_life_permillage_physical`, statmap div=1000,
//! `SkillStatMap.lua:313-316`); gated by the statSet baseMods `explodeCorpse`
//! (act_int.lua:5287, via the skill_overrides overlay).
//!
//! Walks the full chain with real ingested data: gem -> statmap skill_data ->
//! orchestration injects a `PhysicalDamageMin/Max` BASE -> damage component.
//! On a bare build (no gear/passives), the physical component should equal
//! the corpse base damage **exactly** (no inc/more interference).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_data::prelude::DamageType;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn detonate_dead_build() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemDetonateDead")
                .with_active_skill("DetonateDeadPlayer", 20),
        )
}

fn opts(enemy_level: u32) -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

fn physical_component(build_data: &BuildData, enemy_level: u32) -> (f64, f64) {
    let out = calculate_with_data(&detonate_dead_build(), build_data, &opts(enemy_level))
        .expect("calculate");
    let phys = out
        .damage_components
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .expect("physical component present");
    (phys.min, phys.max)
}

/// DD L20: permillage 107 -> multiplier 0.107.
/// - Default enemy level = min(MaxEnemyLevel 85, character 90) = 85 -> monsterLife 36012
///   -> corpse base damage 3853.284 (min = max, vendor BonusMin/Max share the same value);
/// - Explicit enemy level 50 -> monsterLife 2556 -> 273.492 (proves the level pipeline is wired up).
#[test]
fn detonate_dead_physical_base_is_corpse_life_times_multiplier() {
    let build_data = load_build_data();

    let (min, max) = physical_component(&build_data, 0);
    let expected = 36012.0 * 0.107;
    assert!(
        (min - expected).abs() < 1e-6 && (max - expected).abs() < 1e-6,
        "lv85 corpse base: got {min}..{max}, want {expected}"
    );

    let (min50, max50) = physical_component(&build_data, 50);
    let expected50 = 2556.0 * 0.107;
    assert!(
        (min50 - expected50).abs() < 1e-6 && (max50 - expected50).abs() < 1e-6,
        "lv50 corpse base: got {min50}..{max50}, want {expected50}"
    );
}

/// A non-corpse skill (Fireball) is unaffected: the physical component is zero (explodeCorpse gate defaults to false).
#[test]
fn non_corpse_skill_gets_no_physical_injection() {
    let build_data = load_build_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/Fireball")
                .with_active_skill("FireballPlayer", 20),
        );
    let out = calculate_with_data(&build, &build_data, &opts(0)).expect("calculate");
    let phys_avg: f64 = out
        .damage_components
        .iter()
        .filter(|c| c.damage_type == DamageType::Physical)
        .map(|c| (c.min + c.max) / 2.0)
        .sum();
    assert_eq!(phys_avg, 0.0, "Fireball 不应获得尸体物理基伤");
}
