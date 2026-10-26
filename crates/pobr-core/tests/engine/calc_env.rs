use pobr_core::calc::{
    Actor, ActorBaseStats, BreakdownTable, Env, OutputTable, armour_reduction, hit_chance,
    monster_hit_chance, perform,
};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

mod resistance_scaled_defences {
    use super::*;
    use pobr_core::ModTag;
    use pobr_core::calc::session::{BuffKind, BuffSpec};

    const LINES: [&str; 4] = [
        "Armour is increased by Uncapped Fire Resistance",
        "Energy Shield is increased by Uncapped Cold Resistance",
        "Evasion Rating is increased by Uncapped Lightning Resistance",
        "Elemental Ailment Threshold is increased by Uncapped Chaos Resistance",
    ];

    fn environment(enabled: bool) -> Env {
        let mut env = Env::new(Actor::new(
            90,
            ActorBaseStats {
                life: 1000.0,
                armour: 100.0,
                evasion: 100.0,
                energy_shield: 100.0,
                fire_resistance: 125.0,
                cold_resistance: 150.0,
                lightning_resistance: 175.0,
                ..Default::default()
            },
        ))
        .with_config(CalcConfig::default().with_condition("SkinActive", enabled));
        for line in LINES {
            let parsed = crate::support::parse_mod(line).unwrap();
            assert!(parsed.is_complete(), "{line}: {parsed:?}");
            for modifier in parsed.mods {
                let modifier = modifier
                    .with_tag(ModTag::condition("SkinActive", false))
                    .with_origin(
                        ModifierSource::new(SourceId::new(SourceKind::Item, "skin"))
                            .with_slot("bodyarmour")
                            .with_raw_text(line),
                    );
                // Two sources of the same property establish it once.
                env.player.mod_db.add_list([modifier.clone(), modifier]);
            }
        }
        env.player
            .mod_db
            .add_mod(Modifier::number("ChaosResist", ModType::Base, 50.0));
        for name in ["Armour", "Evasion", "EnergyShield", "AilmentThreshold"] {
            env.player.mod_db.add_list([
                Modifier::number(name, ModType::Inc, 10.0),
                Modifier::number(name, ModType::More, 20.0),
            ]);
        }
        env
    }

    #[test]
    fn uncapped_resistance_adds_to_existing_increases_once_with_item_origin() {
        for enabled in [false, true] {
            let mut env = environment(enabled);
            perform(&mut env).unwrap();
            let expected = if enabled {
                [282.0, 312.0, 342.0, 960.0]
            } else {
                [132.0, 132.0, 132.0, 660.0]
            };
            let actual = [
                env.player.output.armour,
                env.player.output.energy_shield,
                env.player.output.evasion,
                env.player.output.ailment_threshold,
            ];
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
            }
            assert_eq!(env.player.output.cold_resistance, 75.0);
            if enabled {
                let derived = env
                    .player
                    .mod_db
                    .iter_mods()
                    .find(|m| {
                        m.name.as_str() == "EnergyShield" && m.value.as_number() == Some(150.0)
                    })
                    .unwrap();
                let origin = derived.origin.as_ref().unwrap();
                assert_eq!(origin.slot.as_deref(), Some("bodyarmour"));
                assert_eq!(origin.raw_text.as_deref(), Some(LINES[1]));
            }
        }
    }

    #[test]
    fn resistance_buff_and_charge_snapshot_scales_conversion_destination() {
        let mut env = environment(true);
        env.cfg = env
            .cfg
            .with_mode_buffs(true)
            .with_condition("UsePowerCharges", true);
        env.player.mod_db.add_list([
            Modifier::number("ColdResist", ModType::Base, 5.0).with_tag(ModTag::multiplier(
                "PowerCharge",
                1.0,
                None,
            )),
            Modifier::number("ArmourConvertToEnergyShield", ModType::Base, 100.0),
        ]);
        env.buff_skills.push(BuffSpec {
            name: "Resistance buff".into(),
            kind: BuffKind::Buff,
            skill_id: "resistance-buff".into(),
            mods: vec![Modifier::number("ColdResist", ModType::Base, 25.0)],
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
        // 150 + 3 * 5 + 25 resistance, plus 10% existing increase and 20% more.
        assert_eq!(env.player.output.armour, 0.0);
        assert_eq!(env.player.output.energy_shield, 720.0);
    }

    #[test]
    fn uncapped_resistance_respects_overrides_and_negative_totals() {
        let mut env = environment(true);
        env.player.mod_db.add_list([
            Modifier::number("ColdResist", ModType::Override, -20.0),
            Modifier::number("FireResist", ModType::Override, 1009.0),
        ]);
        perform(&mut env).unwrap();
        assert_eq!(env.player.output.cold_resistance, -20.0);
        assert_eq!(env.player.output.energy_shield, 108.0);
        assert_eq!(env.player.output.armour, 1343.0);
    }
}

mod hindered {
    use super::*;
    use pobr_core::calc::session::{BuffKind, BuffSpec};

    fn environment(curse: bool, zone_of_control: bool) -> Env {
        let mut env = Env::new(Actor::new(
            90,
            ActorBaseStats {
                life: 1000.0,
                action_rate: 1.0,
                ..Default::default()
            },
        ))
        .with_config(
            CalcConfig::spell()
                .with_mode_effective(true)
                .with_mode_buffs(true),
        );
        let mut lines = vec![
            "Adds 100 to 100 Fire Damage",
            "30% increased Damage with Hits against Hindered Enemies",
            "25% increased Damage with Hits against Hindered Enemies",
        ];
        if zone_of_control {
            lines.push("Enemies you Curse are Hindered, with 15% reduced Movement Speed");
        }
        for line in lines {
            let parsed = crate::support::parse_mod(line).unwrap();
            assert!(parsed.is_complete(), "{line}: {parsed:?}");
            env.player.mod_db.add_list(parsed.mods);
        }
        if curse {
            env.buff_skills.push(BuffSpec {
                name: "Temporal Chains".into(),
                kind: BuffKind::Curse,
                skill_id: "TemporalChainsPlayer".into(),
                mods: vec![Modifier::number("ActionSpeed", ModType::More, -10.0)],
                magnitude: 1.0,
                slot: None,
                socket_index: 1,
                is_mark: false,
                ignore_curse_limit: false,
                local_effect_inc: 0.0,
                local_effect_more: 1.0,
                skill_types: pobr_data::skill::SkillTypes::NONE,
            });
        }
        env
    }

    #[test]
    fn cursed_enemies_are_hindered_after_the_curse_pass() {
        for (curse, zone_of_control) in [(false, true), (true, false), (true, true)] {
            let mut env = environment(curse, zone_of_control);
            assert!(!env.cfg.condition("EnemyCursed"));
            perform(&mut env).unwrap();
            let hindered = curse && zone_of_control;
            assert_eq!(env.cfg.condition("EnemyHindered"), hindered);
            let expected = if hindered { 155.0 } else { 100.0 };
            assert!((env.player.output.dps - expected).abs() < 1e-9);
        }
    }
}

mod doryani {
    use super::*;
    use pobr_core::ModTag;
    use pobr_core::calc::session::{BuffKind, BuffSpec};
    use pobr_core::calc::{CalculationSession, MinimalInput};

    const LINE: &str = "Enemies in your Presence have Lightning Resistance equal to yours";

    fn add_line(env: &mut Env, text: &str) {
        let parsed = crate::support::parse_mod(text).unwrap();
        assert!(parsed.is_complete(), "{text}: {parsed:?}");
        env.player.mod_db.add_list(parsed.mods.into_iter().map(|m| {
            m.with_origin(
                ModifierSource::new(SourceId::new(SourceKind::Item, "Doryani's Prototype"))
                    .with_slot("bodyarmour")
                    .with_parent(SourceId::new(SourceKind::Item, "equipped.bodyarmour"))
                    .with_raw_text(text),
            )
        }));
    }

    // Query the public breakdown API with the completed enemy snapshot;
    // do not perform again, which would reapply exposure and other debuffs.
    fn breakdown_session(env: &Env) -> CalculationSession {
        let mut session = CalculationSession::new(MinimalInput::from(env.player.base))
            .with_config(env.cfg.clone());
        session.add_enemy_modifiers(env.enemy.mod_db.iter_mods().cloned());
        session
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
        let origin = copied.origin.as_ref().unwrap();
        assert_eq!(
            origin.source_id,
            SourceId::new(SourceKind::Item, "Doryani's Prototype")
        );
        assert_eq!(origin.slot.as_deref(), Some("bodyarmour"));
        assert_eq!(
            origin.parent_source_id,
            Some(SourceId::new(SourceKind::Item, "equipped.bodyarmour"))
        );
        assert_eq!(origin.raw_text.as_deref(), Some(LINE));
        assert_eq!(origin.stat_id, Some("LightningResist".into()));
        assert_eq!(origin.mod_type, Some(ModType::Override));

        let session = breakdown_session(&env);
        let rows = session.hit_mitigation_mods(0, &[DamageType::Lightning]);
        let lines = &rows[0].1.defence;
        assert_eq!(lines.iter().filter(|line| line.active).count(), 1);
        let active = lines.iter().find(|line| line.active).unwrap();
        assert_eq!(active.value, Some(-184.0));
        assert_eq!(active.modifier.origin.as_ref(), Some(origin));
        // The retained ordinary resistance, curse, exposure and scaling
        // rows are bypassed by the override, not additional contributions.
        for (name, kind, value) in [
            ("ElementalResist", ModType::Base, 50.0),
            ("ElementalResist", ModType::Base, -20.0),
            ("LightningResist", ModType::Base, -10.0),
            ("LightningResist", ModType::Inc, -50.0),
        ] {
            assert!(lines.iter().any(|line| !line.active
                && line.modifier.name.as_str() == name
                && line.modifier.mod_type == kind
                && line.value == Some(value)));
        }
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
            Modifier::number("LightningResist", ModType::Override, -25.0)
                .with_source("earlier override"),
        );
        env.enemy.mod_db.add_mod(
            Modifier::number("LightningResist", ModType::Override, 0.0)
                .with_source("Enemies in your Presence have no Elemental Resistances"),
        );
        env.enemy.mod_db.add_mod(
            Modifier::number("LightningResist", ModType::Override, 99.0)
                .with_tag(ModTag::condition("InactiveOverride", false)),
        );
        perform(&mut env).unwrap();
        assert_eq!(env.player.output.lightning_resistance, -184.0);
        assert_eq!(
            env.enemy.mod_db.override_(&env.cfg, "LightningResist"),
            Some(0.0)
        );
        assert!((env.player.output.dps - 100.0).abs() < 1e-9);
        let session = breakdown_session(&env);
        let rows = session.hit_mitigation_mods(0, &[DamageType::Lightning]);
        let lines = &rows[0].1.defence;
        assert_eq!(lines.iter().filter(|line| line.active).count(), 1);
        assert_eq!(
            lines.iter().find(|line| line.active).unwrap().value,
            Some(0.0)
        );
        for value in [-25.0, 99.0] {
            assert!(
                lines
                    .iter()
                    .any(|line| line.modifier.mod_type == ModType::Override
                        && line.value == Some(value)
                        && !line.active)
            );
        }
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
        let session = breakdown_session(&env);
        let rows = session.hit_mitigation_mods(0, &[DamageType::Lightning]);
        assert!(rows[0].1.defence.iter().all(|line| line.active));
        assert_eq!(
            rows[0]
                .1
                .defence
                .iter()
                .filter_map(|line| line.value)
                .sum::<f64>(),
            40.0
        );
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
        env.enemy.mod_db.add_list([
            Modifier::number("FireResist", ModType::Base, 25.0),
            Modifier::number("FireExposure", ModType::Base, 10.0),
            Modifier::number("FireDamageTaken", ModType::Inc, 10.0),
            Modifier::number("LightningDamageTaken", ModType::Inc, 20.0),
            Modifier::number("DamageTaken", ModType::Inc, 10.0)
                .with_tag(ModTag::DamageType(DamageType::Lightning)),
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
        assert!((env.player.output.dps - 681.6).abs() < 1e-9);
        assert!((env.player.output.skill_total_dot - 681.6).abs() < 1e-9);
        assert!((env.minions[0].output.dps - 369.2).abs() < 1e-9);
        assert_eq!(env.minions[0].output.lightning_resistance, 30.0);
        let session = breakdown_session(&env);
        for (damage_type, lines) in
            session.hit_mitigation_mods(0, &[DamageType::Fire, DamageType::Lightning])
        {
            assert_eq!(lines.defence.iter().filter(|line| line.active).count(), 1);
            let active = lines.defence.iter().find(|line| line.active).unwrap();
            assert_eq!(active.modifier.name.as_str(), "LightningResist");
            assert_eq!(active.modifier.mod_type, ModType::Override);
            assert_eq!(active.value, Some(-184.0));
            assert_eq!(
                active.modifier.origin.as_ref().unwrap().slot.as_deref(),
                Some("bodyarmour")
            );
            assert!(
                lines
                    .defence
                    .iter()
                    .all(|line| line.modifier.name.as_str() != "FireResist")
            );
            assert!(lines.defence.iter().any(|line| !line.active
                && line.modifier.name.as_str() == "LightningResist"
                && line.value == Some(-10.0)));
            let taken: f64 = lines
                .taken
                .iter()
                .filter(|line| line.active)
                .filter_map(|line| line.value)
                .sum();
            assert_eq!(
                taken,
                if damage_type == DamageType::Fire {
                    10.0
                } else {
                    30.0
                }
            );
        }
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
