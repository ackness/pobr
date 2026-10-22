//! A corpse contributes the share of its maximum Life named by the skill:
//! Detonate Dead's `corpse_explosion_monster_life_permillage_physical`
//! is 107 at level 20 (10.7% Physical damage), while Decompose's
//! `corpse_explosion_monster_life_permillage_chaos` is 100 (poisons as
//! though dealing Chaos damage equal to 10% of the corpse's maximum Life).
//! The corpse's Life comes from the configured level's monster life table.

use pobr_build::{
    Build, BuildConfig, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::monster::EnemyTier;
use pobr_data::prelude::DamageType;

use crate::test_data as load_build_data;

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
/// - Default normal enemy level = min(MaxEnemyLevel 85, character 90) = 85 -> monsterLife 36012
///   -> corpse base damage 3853.284 (min = max);
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

#[test]
fn explicit_corpse_life_overrides_the_monster_table_for_each_corpse_skill() {
    let data = load_build_data();
    for (skill_id, damage_type, share) in [
        ("DetonateDeadPlayer", DamageType::Physical, 0.107),
        ("CorpseCloudPlayer", DamageType::Chaos, 0.1),
    ] {
        let mut build =
            Build::new().add_socket_group(SocketGroup::new().with_active_skill(skill_id, 20));
        for (value, life) in [
            (ConfigInputValue::Number(10000.0), 10000.0),
            (ConfigInputValue::Number(0.0), 0.0),
            (ConfigInputValue::Number(-1.0), 2556.0),
            (ConfigInputValue::Number(f64::NAN), 2556.0),
            (ConfigInputValue::Text("10000".into()), 2556.0),
        ] {
            build
                .config
                .raw_inputs
                .values
                .insert("detonateDeadCorpseLife".into(), value);
            let out = calculate_with_data(&build, &data, &opts(50)).unwrap();
            // Game level-20 stats: 107 permillage Physical for Detonate Dead,
            // 100 permillage Chaos for Decompose. Level-50 monster Life is 2556.
            let component = out
                .damage_components
                .iter()
                .find(|c| c.damage_type == damage_type);
            let (min, max) = component.map_or((0.0, 0.0), |c| (c.min, c.max));
            assert!((min - life * share).abs() < 1e-6, "{skill_id}: {min}");
            assert!((max - life * share).abs() < 1e-6, "{skill_id}: {max}");
        }
    }
    let mut fireball =
        Build::new().add_socket_group(SocketGroup::new().with_active_skill("FireballPlayer", 20));
    let plain = calculate_with_data(&fireball, &data, &opts(50)).unwrap();
    fireball.config.raw_inputs.values.insert(
        "detonateDeadCorpseLife".into(),
        ConfigInputValue::Number(10000.0),
    );
    let configured = calculate_with_data(&fireball, &data, &opts(50)).unwrap();
    assert_eq!(plain.dps, configured.dps);
}

#[test]
fn configured_shock_magnitude_uses_the_existing_dynamic_ailment_reader() {
    let data = load_build_data();
    let mut build = detonate_dead_build();
    let mut options = opts(50);
    options.mode_effective = true;
    let plain = calculate_with_data(&build, &data, &options).unwrap();
    build
        .config
        .raw_inputs
        .values
        .insert("conditionEnemyShocked".into(), ConfigInputValue::Bool(true));
    build.config.raw_inputs.values.insert(
        "conditionShockEffect".into(),
        ConfigInputValue::Number(30.0),
    );
    let shocked = calculate_with_data(&build, &data, &options).unwrap();
    // base_damage_taken_+%_from_shock: a configured 30% shock increases damage taken
    // by 30%; it is not a 30% modifier to the player's shock magnitude.
    assert!((shocked.dps / plain.dps - 1.3).abs() < 1e-6);
    build.config.raw_inputs.values.insert(
        "conditionEnemyShocked".into(),
        ConfigInputValue::Bool(false),
    );
    let disabled = calculate_with_data(&build, &data, &options).unwrap();
    assert_eq!(plain.dps, disabled.dps);
}

#[test]
fn corpse_life_uses_the_configured_level_without_changing_the_skills_share() {
    let data = load_build_data();
    for (skill, damage_type, share) in [
        ("DetonateDeadPlayer", DamageType::Physical, 0.107),
        ("CorpseCloudPlayer", DamageType::Chaos, 0.1),
    ] {
        // The game table gives 32956 Life at level 82, 36012 at 85, and
        // 2556 at 50. An absent level follows the level-90 character,
        // capped at 85; it does not imply a different corpse percentage.
        for (input, placeholder, option, life) in [
            (None, None, 0, 36012.0),
            (Some(82), None, 0, 32956.0),
            (None, Some(82), 0, 32956.0),
            (Some(50), Some(82), 0, 2556.0),
            (Some(82), Some(50), 85, 36012.0),
            (None, None, 100, 36012.0),
        ] {
            let mut build = Build::new()
                .with_character(CharacterIdentity {
                    level: 90,
                    class_name: "Witch".into(),
                    ascendancy_name: String::new(),
                })
                .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20));
            if let Some(level) = input {
                build.config.raw_inputs.values.insert(
                    "enemyLevel".into(),
                    ConfigInputValue::Number(f64::from(level)),
                );
            }
            if let Some(level) = placeholder {
                build.config.raw_inputs.placeholders.insert(
                    "enemyLevel".into(),
                    ConfigInputValue::Number(f64::from(level)),
                );
            }
            let output = calculate_with_data(&build, &data, &opts(option)).unwrap();
            let component = output
                .damage_components
                .iter()
                .find(|component| component.damage_type == damage_type)
                .unwrap();
            let expected = life * share;
            assert!(
                (component.min - expected).abs() < 1e-6 && (component.max - expected).abs() < 1e-6,
                "{skill}: input={input:?}, placeholder={placeholder:?}, option={option}: {component:?}"
            );
        }
    }
}

#[test]
fn corpse_and_enemy_use_the_same_default_level_for_the_selected_tier() {
    let data = load_build_data();
    for skill in ["DetonateDeadPlayer", "CorpseCloudPlayer"] {
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20));
        // Ordinary monsters follow the character up to level 85. The
        // Pinnacle/Uber preset starts at level 82, independently of it.
        for (tier, level) in [
            (EnemyTier::None, 85),
            (EnemyTier::Boss, 85),
            (EnemyTier::Pinnacle, 82),
            (EnemyTier::Uber, 82),
        ] {
            let options = DataOrchestratorOptions {
                enemy_tier: tier,
                ..DataOrchestratorOptions::against_reference_enemy()
            };
            let default = calculate_with_data(&build, &data, &options).unwrap();
            let explicit = calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    enemy_level: level,
                    ..options
                },
            )
            .unwrap();
            assert!(
                (default.dps - explicit.dps).abs() < 1e-6,
                "{skill} vs {tier:?}: default {} != level {level} {}",
                default.dps,
                explicit.dps
            );
            assert!((default.combined_dps - explicit.combined_dps).abs() < 1e-6);
            let configured_tier = calculate_with_data(
                &build
                    .clone()
                    .with_config(BuildConfig::default().with_enemy_tier(tier)),
                &data,
                &DataOrchestratorOptions::against_reference_enemy(),
            )
            .unwrap();
            assert!((default.dps - configured_tier.dps).abs() < 1e-6);
        }
    }
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
    assert_eq!(
        phys_avg, 0.0,
        "Fireball should not gain corpse physical base damage"
    );
}

/// Decompose's cloud "Poisons enemies as though dealing Chaos damage equal to
/// {0}% of Corpse's maximum Life": it poisons without hitting, so it cannot
/// critically hit. Its explosion deals base Fire damage, a hit with the
/// skill's critical hit chance.
#[test]
fn decompose_cloud_cannot_crit_but_its_explosion_can() {
    let build_data = load_build_data();
    let crit = |stat_set: u32| {
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("weapon1")
                    .with_gem("Metadata/Items/Gems/SkillGemCorpseCloud")
                    .with_active_skill("CorpseCloudPlayer", 20)
                    .with_gem_skill_stat_set("CorpseCloudPlayer", 20, stat_set),
            );
        calculate_with_data(&build, &build_data, &opts(0))
            .expect("calculate")
            .crit_chance
    };
    assert_eq!(crit(1), 0.0, "the cloud");
    assert!(crit(2) > 0.0, "the explosion: {}", crit(2));
}
