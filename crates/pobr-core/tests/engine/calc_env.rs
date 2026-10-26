use pobr_core::calc::{
    Actor, ActorBaseStats, BreakdownTable, Env, OutputTable, armour_reduction, hit_chance,
    monster_hit_chance, perform,
};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

mod doryani {
    use super::*;
    use pobr_core::ModTag;
    use pobr_core::calc::session::{BuffKind, BuffSpec};

    const LINE: &str = "Enemies in your Presence have Lightning Resistance equal to yours";

    fn add_line(env: &mut Env, text: &str) {
        let parsed = crate::support::parse_mod(text).unwrap();
        assert!(parsed.is_complete(), "{text}: {parsed:?}");
        env.player.mod_db.add_list(parsed.mods.into_iter().map(|m| {
            m.with_origin(ModifierSource::new(SourceId::new(
                SourceKind::Item,
                "Doryani's Prototype",
            )))
        }));
    }

    fn environment(base_resistance: f64, in_presence: bool) -> Env {
        let mut env = Env::new(Actor::new(
            90,
            ActorBaseStats {
                life: 1000.0,
                lightning_resistance: base_resistance,
                action_rate: 1.0,
                ..Default::default()
            },
        ))
        .with_config(
            CalcConfig::spell()
                .with_mode_effective(true)
                .with_mode_buffs(true)
                .with_mode_combat(true)
                .with_condition("EnemyInPresence", in_presence),
        );
        add_line(&mut env, LINE);
        add_line(&mut env, "Adds 100 to 100 Lightning Damage");
        env.enemy.mod_db.add_list([
            Modifier::number("ElementalResist", ModType::Base, 50.0),
            Modifier::number("LightningExposure", ModType::Base, 10.0),
        ]);
        env
    }

    #[test]
    fn replaces_enemy_resistance_without_reapplying_exposure_or_curses() {
        let mut env = environment(-60.0, true);
        add_line(&mut env, "-124% to Lightning Resistance");
        env.enemy.mod_db.add_list([
            Modifier::number("ElementalResist", ModType::Base, -20.0)
                .with_source("Elemental Weakness"),
            Modifier::number("LightningResist", ModType::Inc, -50.0),
        ]);
        perform(&mut env).unwrap();
        assert_eq!(env.player.output.lightning_resistance, -184.0);
        assert!((env.player.output.dps - 284.0).abs() < 1e-9);
        assert_eq!(
            env.enemy.mod_db.override_(&env.cfg, "LightningResist"),
            Some(-184.0)
        );
        let copied = env
            .enemy
            .mod_db
            .named("LightningResist")
            .find(|m| m.mod_type == ModType::Override)
            .unwrap();
        assert_eq!(
            copied.origin.as_ref().unwrap().source_id,
            SourceId::new(SourceKind::Item, "Doryani's Prototype")
        );
    }

    #[test]
    fn copies_final_player_resistance_after_buffs_charges_and_resource_scaling() {
        let mut env = environment(-100.0, true);
        env.cfg = env.cfg.with_condition("UsePowerCharges", true);
        env.player.mod_db.add_list([
            Modifier::number("LightningResist", ModType::Base, 4.0).with_tag(ModTag::multiplier(
                "PowerCharge",
                1.0,
                None,
            )),
            Modifier::number("LightningResist", ModType::Base, 1.0)
                .with_tag(ModTag::multiplier("Life", 100.0, None)),
        ]);
        env.buff_skills.push(BuffSpec {
            name: "Resistance buff".into(),
            kind: BuffKind::Buff,
            skill_id: "resistance-buff".into(),
            mods: vec![
                Modifier::number("LightningResist", ModType::Base, 20.0),
                Modifier::number("MaximumLife", ModType::Base, 500.0),
            ],
            magnitude: 1.0,
            slot: None,
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
            local_effect_inc: 0.0,
            local_effect_more: 1.0,
            skill_types: pobr_data::skill::SkillTypes::NONE,
        });
        perform(&mut env).unwrap();
        // -100 + 20 buff + 3 charges * 4 + 1500 final Life / 100 = -53.
        assert_eq!(env.player.output.lightning_resistance, -53.0);
        assert_eq!(
            env.enemy.mod_db.override_(&env.cfg, "LightningResist"),
            Some(-53.0)
        );
        assert!((env.player.output.dps - 153.0).abs() < 1e-9);
    }

    #[test]
    fn copies_player_floor_and_maximum_instead_of_uncapped_resistance() {
        for (base, max_bonus, expected, enemy_expected) in [
            (-250.0, 0.0, -200.0, -200.0),
            (120.0, -10.0, 65.0, 65.0),
            // Equality does not grant maximum resistance to the enemy:
            // retain its own cap (the >75 interaction is not measured in game).
            (120.0, 5.0, 80.0, 75.0),
        ] {
            let mut env = environment(base, true);
            env.player.mod_db.add_mod(Modifier::number(
                "MaximumLightningResistance",
                ModType::Base,
                max_bonus,
            ));
            perform(&mut env).unwrap();
            assert_eq!(env.player.output.lightning_resistance, expected);
            assert_eq!(
                env.enemy.mod_db.override_(&env.cfg, "LightningResist"),
                Some(expected)
            );
            assert!((env.player.output.dps - (100.0 - enemy_expected)).abs() < 1e-9);
        }
    }

    #[test]
    fn preserves_explicit_enemy_resistance_override_priority() {
        let mut env = environment(-184.0, true);
        env.enemy.mod_db.add_mod(
            Modifier::number("LightningResist", ModType::Override, 0.0)
                .with_source("Enemies in your Presence have no Elemental Resistances"),
        );
        perform(&mut env).unwrap();
        assert_eq!(env.player.output.lightning_resistance, -184.0);
        assert_eq!(
            env.enemy.mod_db.override_(&env.cfg, "LightningResist"),
            Some(0.0)
        );
        assert!((env.player.output.dps - 100.0).abs() < 1e-9);
    }

    #[test]
    fn explicit_out_of_presence_leaves_enemy_resistance_and_exposure_in_effect() {
        let mut env = environment(-184.0, false);
        perform(&mut env).unwrap();
        assert_eq!(
            env.enemy.mod_db.override_(&env.cfg, "LightningResist"),
            None
        );
        assert!((env.player.output.dps - 60.0).abs() < 1e-9);
    }

    #[test]
    fn sacred_flame_minions_and_damage_over_time_use_the_same_enemy_resistance() {
        let mut env = environment(-184.0, true);
        add_line(
            &mut env,
            "Enemies in your Presence Resist Elemental Damage based on their Lowest Resistance",
        );
        add_line(&mut env, "Adds 100 to 100 Fire Damage");
        env.player.mod_db.add_list([
            Modifier::number("FireDot", ModType::Base, 100.0),
            Modifier::number("LightningDot", ModType::Base, 100.0),
        ]);
        let mut minion = Actor::new(
            90,
            ActorBaseStats {
                lightning_resistance: 30.0,
                hit_min: 100.0,
                hit_max: 100.0,
                action_rate: 1.0,
                ..Default::default()
            },
        );
        minion.mod_db.add_mod(Modifier::number(
            "PhysicalDamageConvertToLightning",
            ModType::Base,
            100.0,
        ));
        env.minions.push(minion);
        perform(&mut env).unwrap();
        assert!((env.player.output.dps - 568.0).abs() < 1e-9);
        assert!((env.player.output.skill_total_dot - 568.0).abs() < 1e-9);
        assert!((env.minions[0].output.dps - 284.0).abs() < 1e-9);
        assert_eq!(env.minions[0].output.lightning_resistance, 30.0);
    }
}

#[test]
fn perform_writes_player_output_and_breakdown() {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            life: 1_000.0,
            mana: 100.0,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            accuracy: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            hit_min: 100.0,
            hit_max: 200.0,
            action_rate: 2.0,
        },
    );
    player
        .mod_db
        .add_mod(Modifier::number("MaximumLife", ModType::Base, 100.0));
    player
        .mod_db
        .add_mod(Modifier::number("MaximumLife", ModType::Inc, 20.0));
    player
        .mod_db
        .add_mod(Modifier::number("FireResistance", ModType::Base, 40.0));
    player.mod_db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );
    player.mod_db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0).with_flags(ModFlags::ATTACK),
    );
    player
        .mod_db
        .add_mod(Modifier::number("AttackSpeed", ModType::Inc, 10.0).with_flags(ModFlags::ATTACK));

    let mut env = Env::new(player).with_config(CalcConfig::attack());

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.life, 1_320.0);
    assert_eq!(env.player.output.fire_resistance, 40.0);
    assert_eq!(env.player.output.total_hit_avg, 270.0);
    assert_eq!(env.player.output.action_rate, 2.2);
    assert_eq!(env.player.output.dps, 594.0);
    assert_eq!(env.player.output.armour, 0.0);
    assert_eq!(env.player.output.evasion, 0.0);
    assert_eq!(env.player.output.energy_shield, 0.0);
    assert!(env.player.breakdown.contains("life"));
    assert!(env.player.breakdown.contains("fire_resistance"));
    assert!(env.player.breakdown.contains("total_hit_avg"));
    assert!(env.player.breakdown.contains("dps"));
}

#[test]
fn perform_writes_hit_chance_and_applies_it_to_dps() {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            accuracy: 600.0,
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 1.0,
            ..ActorBaseStats::default()
        },
    );
    player
        .mod_db
        .add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 100.0));

    let enemy = Actor::new(
        1,
        ActorBaseStats {
            evasion: 1_000.0,
            ..ActorBaseStats::default()
        },
    );
    let expected_hit_chance = hit_chance(enemy.base.evasion, player.base.accuracy);

    let mut env = Env::new(player);
    env.enemy = enemy;

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.total_hit_avg, 200.0);
    assert_eq!(env.player.output.hit_chance, expected_hit_chance);
    assert_eq!(env.player.output.dps, 200.0 * expected_hit_chance);
    assert!(env.player.breakdown.contains("hit_chance"));
}

#[test]
fn output_and_breakdown_have_clear_defaults() {
    let output = OutputTable::default();
    let breakdown = BreakdownTable::default();

    assert_eq!(output.life, 0.0);
    assert_eq!(output.dps, 0.0);
    assert_eq!(output.armour, 0.0);
    assert_eq!(output.hit_chance, 0.0);
    assert_eq!(output.chance_to_be_hit, 0.0);
    assert!(breakdown.is_empty());
}

#[test]
fn perform_writes_defence_output_and_breakdown() {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            life: 1_000.0,
            mana: 100.0,
            armour: 1_000.0,
            evasion: 500.0,
            energy_shield: 200.0,
            accuracy: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            hit_min: 0.0,
            hit_max: 0.0,
            action_rate: 0.0,
        },
    );
    player
        .mod_db
        .add_mod(Modifier::number("Armour", ModType::Base, 500.0));
    player
        .mod_db
        .add_mod(Modifier::number("Armour", ModType::Inc, 20.0));
    player
        .mod_db
        .add_mod(Modifier::number("Evasion", ModType::Inc, 100.0));
    player
        .mod_db
        .add_mod(Modifier::number("EnergyShield", ModType::Base, 50.0));

    let mut env = Env::new(player);
    env.enemy.base.accuracy = 600.0;

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.armour, 1_800.0);
    assert_eq!(env.player.output.evasion, 1_000.0);
    assert_eq!(env.player.output.energy_shield, 250.0);
    // The defence side (monster hitting the player) uses monster_hit_chance (PoE2's
    // two sides use asymmetric formulas).
    // Source: agent-docs/accuracy-and-enemy.md §2 note, CalcDefence.lua calcs.monsterHitChance
    assert_eq!(
        env.player.output.chance_to_be_hit,
        monster_hit_chance(env.player.output.evasion, 600.0)
    );
    assert!(env.player.breakdown.contains("armour"));
    assert!(env.player.breakdown.contains("evasion"));
    assert!(env.player.breakdown.contains("energy_shield"));
    assert!(env.player.breakdown.contains("chance_to_be_hit"));
}

#[test]
fn hit_chance_is_clamped_to_path_of_exile_bounds() {
    assert_eq!(hit_chance(0.0, 0.0), 1.0);
    assert_eq!(hit_chance(1_000_000.0, 1.0), 0.05);
    assert_eq!(hit_chance(1.0, 1_000_000.0), 1.0);
}

#[test]
fn armour_reduction_uses_armour_against_raw_hit() {
    assert_eq!(armour_reduction(0.0, 1_000.0), 0.0);
    assert_eq!(armour_reduction(10_000.0, 1_000.0), 0.5);
    assert_eq!(armour_reduction(10_000.0, 0.0), 0.0);
}

/// Bug#3 test: PoE2's attack-side hit-chance formula (accuracy*1.25/(accuracy+evasion*0.3)).
///
/// Source: agent-docs/accuracy-and-enemy.md §2, CalcDefence.lua `calcs.hitChance`.
#[test]
fn poe2_attack_hit_chance_formula() {
    // accuracy=1000, evasion=1000
    // raw = 1000*1.25/(1000+1000*0.3) = 1250/1300 ≈ 0.9615
    let hc = hit_chance(1000.0, 1000.0);
    let expected = (1000.0_f64 * 1.25 / (1000.0 + 1000.0 * 0.3)).clamp(0.05, 1.0);
    assert!(
        (hc - expected).abs() < 1e-6,
        "hit_chance={hc}, expected={expected}"
    );
}

/// Bug#3 test: the defence-side formula is different (monster hitting the player).
///
/// Source: agent-docs/accuracy-and-enemy.md §2 note, CalcDefence.lua `calcs.monsterHitChance`.
#[test]
fn poe2_monster_hit_chance_formula() {
    // evasion=1000, accuracy=1000
    // raw = 1 - 0.95*1000/(1000+4*1000) = 1 - 0.95/5 = 1 - 0.19 = 0.81
    let mhc = monster_hit_chance(1000.0, 1000.0);
    let raw_expected: f64 = 1.0 - 0.95_f64 * 1000.0 / (1000.0 + 4.0 * 1000.0);
    let expected = raw_expected.clamp(0.05_f64, 1.0_f64);
    assert!(
        (mhc - expected).abs() < 1e-6,
        "monster_hit_chance={mhc}, expected={expected}"
    );
    // The two sides' formulas should differ
    assert_ne!(
        hit_chance(1000.0, 1000.0),
        monster_hit_chance(1000.0, 1000.0)
    );
}
