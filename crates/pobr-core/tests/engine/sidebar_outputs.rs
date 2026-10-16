use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::calc::{HandSource, MinimalInput, WeaponBase, run_hand_passes};
use pobr_core::{CalcConfig, ModDb, ModTag, Modifier, display_value};
use pobr_data::prelude::*;

#[test]
fn damage_views_keep_mitigation_hit_chance_and_quantity_separate() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 2.0,
            ..ActorBaseStats::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::number("QuantityMultiplier", ModType::Base, 3.0),
        Modifier::number("RepeatCount", ModType::Base, 2.0),
    ]);
    let mut env = Env::new(actor);
    env.cfg.mode_effective = true;
    env.enemy.mod_db.add_list(vec![
        Modifier::number("PhysicalDamageReduction", ModType::Base, 50.0),
        Modifier::number("BlockChance", ModType::Base, 20.0),
    ]);
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert_eq!(out.total_hit_avg, 100.0);
    assert_eq!(out.average_damage, 40.0);
    assert_eq!(out.average_burst_damage, 120.0);
    assert_eq!(out.quantity_multiplier, 3.0);
    assert_eq!(out.dps, 240.0);
    assert_eq!(out.hit_speed, 2.0);
    assert_eq!(out.combined_avg, 240.0);
    for stat in ["WithIgniteDPS", "WithBleedDPS", "WithPoisonDPS"] {
        assert_eq!(display_value(out, stat), 240.0);
    }
}

#[test]
fn accuracy_and_damage_follow_each_hands_scope() {
    let mut db = ModDb::new();
    db.add_list(vec![
        Modifier::number("Accuracy", ModType::Base, 1000.0),
        Modifier::number("Accuracy", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("OffHandAttack", false)),
        Modifier::flag("Condition:HitChanceCanExceed100"),
    ]);
    let cfg = CalcConfig::attack();
    let input = MinimalInput {
        enemy_evasion: 100.0,
        ..MinimalInput::default()
    };
    let weapon = WeaponBase {
        hit_min: 100.0,
        hit_max: 100.0,
        ..WeaponBase::default()
    };
    let out = run_hand_passes(
        &db,
        &ModDb::new(),
        &cfg,
        &[HandSource::main_hand(weapon), HandSource::off_hand(weapon)],
        &input,
        false,
    );
    assert_eq!(out.main_hand.as_ref().unwrap().accuracy, 1000.0);
    assert_eq!(out.off_hand.as_ref().unwrap().accuracy, 2000.0);
    assert!(out.combined.accuracy_hit_chance_uncapped > 100.0);
    assert_eq!(out.combined.average_damage, 100.0);
}

#[test]
fn ailment_total_damage_uses_its_effective_duration() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 2.0,
            ..ActorBaseStats::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::number("PoisonChance", ModType::Base, 100.0),
        Modifier::number("PoisonDuration", ModType::Inc, 100.0),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell().with_damage_type(DamageType::Chaos);
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert!(out.poison_dps > 0.0);
    assert_eq!(out.poison_damage, out.poison_dps * 4.0);
    assert_eq!(out.with_poison_dps, out.dps + out.poison_stacked_dps);
    assert_eq!(display_value(out, "PoisonDamage"), out.poison_damage);
}

#[test]
fn skill_average_display_uses_damage_per_use_and_full_ailment_damage() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 2.0,
            ..ActorBaseStats::default()
        },
    );
    actor.mod_db.add_mod(Modifier::flag("ShowAverageDamage"));
    actor
        .mod_db
        .add_mod(Modifier::number("PoisonChance", ModType::Base, 100.0));
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell().with_damage_type(DamageType::Chaos);
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert_eq!(out.average_damage, 100.0);
    assert_eq!(out.dps, 200.0);
    assert_eq!(out.combined_avg, out.average_damage + out.poison_damage);
    assert_eq!(out.with_poison_dps, out.combined_avg);
    assert_eq!(out.with_ignite_dps, out.average_damage);
}

#[test]
fn flicker_burst_counts_consumed_charges_and_their_double_benefit() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 1.0,
            ..ActorBaseStats::default()
        },
    );
    for (stat, value) in [
        ("flicker_strike_additional_flickers_from_power_charges", 2.0),
        ("chance_%_to_double_effect_of_removing_charges", 50.0),
    ] {
        actor
            .mod_db
            .add_list(pobr_core::rules::skill_stats::skill_modifiers(stat, value).unwrap());
    }
    actor.mod_db.add_mod(Modifier::number(
        "Multiplier:ExtraConsumablePowerCharges",
        ModType::Base,
        1.0,
    ));
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell().with_multiplier("PowerCharge", 3.0);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.average_burst_damage, 1300.0);
}

#[test]
fn skill_data_durations_reach_each_scoped_output() {
    use pobr_core::rules::stat_map_engine::StatMapCatalog;
    use pobr_core::skill_env::{StatMapCtx, mapped_stat_modifiers};
    use pobr_data::catalog::SkillDamageStat;
    let catalog = StatMapCatalog::new(serde_json::from_str(r#"{
        "global": {
            "base_skill_effect_duration": { "div": 1000, "mods": [{"kind":"skill_data", "value":{"key":"duration"}}] },
            "base_secondary_skill_effect_duration": { "div": 1000, "mods": [{"kind":"skill_data", "value":{"key":"durationSecondary"}}] },
            "aura_duration": { "div": 1000, "mods": [{"kind":"skill_data", "value":{"key":"auraDuration"}}] },
            "reserve_duration": { "div": 1000, "mods": [{"kind":"skill_data", "value":{"key":"reserveDuration"}}] }
        }
    }"#).unwrap());
    let stats = [
        ("base_skill_effect_duration", 4000.0),
        ("base_secondary_skill_effect_duration", 2000.0),
        ("aura_duration", 5000.0),
        ("reserve_duration", 6000.0),
    ]
    .map(|(stat, value)| SkillDamageStat {
        stat: stat.into(),
        value,
    });
    let mut actor = Actor::new(1, ActorBaseStats::default());
    actor.mod_db.add_list(mapped_stat_modifiers(
        &mut StatMapCtx::data(Some(&catalog)),
        &stats,
        SourceKind::SkillGem,
        "skill",
        "DurationSkill",
        None,
    ));
    actor.mod_db.add_list(vec![
        Modifier::number("Duration", ModType::Inc, 50.0),
        Modifier::number("Duration", ModType::Base, 1.0),
        Modifier::number("Duration", ModType::Inc, 100.0).with_flags(ModFlags::ATTACK),
        Modifier::number("PrimaryDuration", ModType::Inc, 50.0),
        Modifier::number("SecondaryDuration", ModType::More, -50.0),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell();
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert_eq!(out.duration, 10.032);
    assert_eq!(out.duration_secondary, 2.277);
    assert_eq!(out.aura_duration, 7.524);
    assert_eq!(out.reserve_duration, 9.009);
    for (id, expected) in [
        ("Duration", 10.032),
        ("DurationSecondary", 2.277),
        ("AuraDuration", 7.524),
        ("ReserveDuration", 9.009),
    ] {
        assert_eq!(display_value(out, id), expected);
    }
}

#[test]
fn swift_affliction_changes_skill_and_damaging_ailment_durations_once_each() {
    use pobr_core::calc::ailment::ailment_duration;
    use pobr_core::calc::skill_mechanics::skill_durations;
    use pobr_core::rules::stat_map_engine::{MappedItem, MappedOutcome, StatMapCatalog, map_stat};
    let catalog = StatMapCatalog::new(serde_json::from_str(r#"{
        "global": { "support_swift_affliction_skill_effect_and_damaging_ailment_duration_+%_final": {
            "mods": [
                {"kind":"mod", "name":"Duration", "mod_type":"MORE"},
                {"kind":"mod", "name":"DamagingAilmentDuration", "mod_type":"MORE"}
            ]
        } }
    }"#).unwrap());
    let MappedOutcome::Mapped(items) = map_stat(
        &catalog,
        "SupportSwiftAfflictionPlayerThree",
        None,
        "support_swift_affliction_skill_effect_and_damaging_ailment_duration_+%_final",
        -30.0,
    ) else {
        panic!("duration stat must map")
    };
    let mut db = ModDb::new();
    for item in items {
        if let MappedItem::Modifier(modifier) = item {
            db.add_mod(*modifier);
        }
    }
    db.add_mod(Modifier::number(
        "EnemyElementalAilmentDuration",
        ModType::Inc,
        10.0,
    ));
    db.add_mod(Modifier::number("SkillDurationBase", ModType::Base, 4.0));
    let cfg = CalcConfig::spell();
    assert_eq!(ailment_duration(AilmentType::Ignite, &db, &cfg), 3.08);
    assert_eq!(ailment_duration(AilmentType::Poison, &db, &cfg), 1.4);
    assert_eq!(skill_durations(&db, &ModDb::new(), &cfg).primary, 2.805);
}

#[test]
fn stackable_skill_dot_uses_its_resolved_duration() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            action_rate: 3.0,
            ..ActorBaseStats::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::number("SkillDurationBase", ModType::Base, 2.0),
        Modifier::number("ChaosDot", ModType::Base, 10.0),
        Modifier::flag("DotCanStack"),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell();
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.duration, 2.013);
    assert_eq!(env.player.output.skill_total_dot, 60.39);
}

#[test]
fn game_radius_and_scoped_additions_reach_metres_display() {
    use pobr_core::rules::stat_map_engine::StatMapCatalog;
    use pobr_core::skill_env::{StatMapCtx, mapped_stat_modifiers};
    use pobr_data::catalog::SkillDamageStat;
    let catalog = StatMapCatalog::new(serde_json::from_str(r#"{
        "global": {
            "active_skill_base_area_of_effect_radius": { "mods": [{"kind":"skill_data", "value":{"key":"radius"}}] },
            "active_skill_base_radius_+": { "mods": [{"kind":"skill_data", "value":{"key":"radiusExtra"}, "tags":[{"type":"Multiplier", "var":"Stage"}]}] },
            "active_skill_area_of_effect_radius_+%_final": { "mods": [{"kind":"mod", "name":"AreaOfEffectRadius", "mod_type":"MORE"}] }
        }
    }"#).unwrap());
    let stats = [
        ("active_skill_base_area_of_effect_radius", 28.0),
        ("active_skill_base_radius_+", 2.0),
        ("active_skill_area_of_effect_radius_+%_final", 20.0),
    ]
    .map(|(stat, value)| SkillDamageStat {
        stat: stat.into(),
        value,
    });
    let mut actor = Actor::new(1, ActorBaseStats::default());
    actor.mod_db.add_list(mapped_stat_modifiers(
        &mut StatMapCtx::data(Some(&catalog)),
        &stats,
        SourceKind::SkillGem,
        "skill",
        "CometPlayer",
        None,
    ));
    actor.mod_db.add_list(vec![
        Modifier::number("AreaOfEffect", ModType::Inc, 50.0),
        Modifier::number("AreaOfEffect", ModType::Inc, 100.0).with_flags(ModFlags::ATTACK),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell().with_multiplier("Stage", 3.0);
    perform(&mut env).unwrap();
    // 34 base units, sqrt(1.5) -> 122%, then 20% more radius -> 146%.
    assert_eq!(env.player.output.aoe_radius, 49.0);
    assert_eq!(
        display_value(&env.player.output, "AreaOfEffectRadiusMetres"),
        4.9
    );
}

#[test]
fn projectile_counts_keep_fractional_chances_and_conversion_flags() {
    use pobr_core::calc::skill_mechanics::projectile_behavior_counts;
    let mut db = ModDb::new();
    db.add_list(vec![
        Modifier::number("ProjectileCount", ModType::Base, 3.0),
        Modifier::number("ProjectileCount", ModType::More, 50.0),
        Modifier::number("PierceChance", ModType::Base, 15.0),
        Modifier::number("ChainCountMax", ModType::Base, 2.0),
        Modifier::number("SplitCount", ModType::Base, 1.0),
        Modifier::flag("ForkOnce"),
        Modifier::flag("NoAdditionalProjectiles"),
        Modifier::flag("AdditionalProjectilesAddChainsInstead"),
        Modifier::flag("AdditionalProjectilesAddBouncesInstead"),
    ]);
    db.add_list(
        pobr_core::rules::skill_stats::skill_modifiers(
            "spectral_spiral_weapon_base_number_of_bounces",
            2.0,
        )
        .unwrap(),
    );
    let cfg = CalcConfig::spell().with_flags(ModFlags::SPELL | ModFlags::PROJECTILE);
    let result = projectile_behavior_counts(&db, &ModDb::new(), &cfg);
    assert_eq!(result.pierce, 0.15);
    assert_eq!(result.chain, 5.0);
    assert_eq!(result.fork, 1.0);
    assert_eq!(result.split, 1.0);
    assert_eq!(result.bounce, 6.0);
    db.add_mod(Modifier::flag("CannotChain"));
    assert_eq!(
        projectile_behavior_counts(&db, &ModDb::new(), &cfg).chain,
        0.0
    );
    db.add_mod(Modifier::flag("PierceAllTargets"));
    let result = projectile_behavior_counts(&db, &ModDb::new(), &cfg);
    assert_eq!(result.pierce, 100.0);
    assert_eq!(result.fork, 0.0);
    assert_eq!(result.chain, 0.0);
}

#[test]
fn requirements_take_maxima_with_source_specific_modifiers() {
    use pobr_core::calc::skill_mechanics::attribute_requirements;
    let mut cfg = CalcConfig::new();
    for (name, value) in [
        ("ItemRequirementStr", 150.0),
        ("WeaponRequirementStr", 200.0),
        ("GemRequirementStr", 180.0),
        ("SupportRequirementStr", 95.0),
        ("MeleeWeaponRequirementDex", 140.0),
        ("GemRequirementInt", 100.0),
        ("Strength", 180.0),
    ] {
        cfg.stats.insert(name.into(), value);
    }
    let mut db = ModDb::new();
    db.add_list(vec![
        Modifier::number("GlobalAttributeRequirements", ModType::Inc, -20.0),
        Modifier::number("GlobalGemAttributeRequirements", ModType::More, 50.0),
        Modifier::number("GlobalWeaponAttributeRequirements", ModType::Inc, -40.0),
    ]);
    assert_eq!(attribute_requirements(&db, &cfg), [216.0, 56.0, 120.0]);
    db.add_list(vec![
        Modifier::flag("GemAttributeRequirementsSatisfiedByHighestAttribute"),
        Modifier::flag("StrengthSatisfiesMeleeWeaponsAndSkills"),
    ]);
    assert_eq!(attribute_requirements(&db, &cfg), [216.0, 0.0, 0.0]);
    db.add_mod(Modifier::number(
        "GlobalGemAttributeRequirements",
        ModType::More,
        -100.0,
    ));
    assert_eq!(attribute_requirements(&db, &cfg), [120.0, 0.0, 0.0]);
    db.add_mod(Modifier::flag("IgnoreAttributeRequirements"));
    assert_eq!(attribute_requirements(&db, &cfg), [0.0; 3]);
}

#[test]
fn all_resource_cost_channels_use_resolved_data_and_cost_scopes() {
    use pobr_core::calc::skill_mechanics::resource_costs;
    use pobr_core::skill_env::{ResolvedCost, additional_resource_cost_modifiers};
    let costs = [
        ("Life", 10.0, false),
        ("ES", 8.0, false),
        ("Rage", 4.0, false),
        ("Ward", 5.0, false),
        ("ManaPercent", 3.0, false),
        ("LifePercent", 2.0, false),
        ("ESPercentPerMinute", 1.5, true),
        ("ManaPerMinute", 4.0, true),
    ]
    .map(|(resource, amount, per_second)| ResolvedCost {
        resource: resource.into(),
        amount,
        per_second,
    });
    let mut db = ModDb::new();
    db.add_list(additional_resource_cost_modifiers(&costs));
    db.add_list(vec![
        Modifier::number("SkillManaCostBase", ModType::Base, 10.0),
        Modifier::number("SupportManaMultiplier", ModType::More, 100.0),
        Modifier::number("WardCostAsPercentOfManaCost", ModType::Base, 50.0),
        Modifier::number("WardCost", ModType::Inc, 50.0),
    ]);
    let out = resource_costs(&db, &CalcConfig::spell());
    assert_eq!(out.upfront, [20.0, 20.0, 16.0, 8.0, 30.0]);
    assert_eq!(out.percent, [6.0, 4.0, 0.0, 0.0, 0.0]);
    assert_eq!(out.per_second, [8.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(out.percent_per_second, [0.0, 0.0, 3.0, 0.0, 0.0]);
    db.add_mod(Modifier::flag("HasNoCost"));
    assert_eq!(
        resource_costs(&db, &CalcConfig::spell()),
        Default::default()
    );
}

#[test]
fn cost_rate_counts_casts_and_reloads_instead_of_projectile_hits() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            action_rate: 2.0,
            ..Default::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::number("SkillManaCostBase", ModType::Base, 10.0),
        Modifier::number("PeriodicHitInterval", ModType::Base, 100.0),
        Modifier::number("SkillRageCostBase", ModType::Base, 4.0),
        Modifier::number("SkillESPercentCostBase", ModType::Base, 2.0),
        Modifier::number("SkillWardPercentCostBase", ModType::Base, 3.0),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell();
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.effective_action_rate, 10.0);
    assert_eq!(env.player.output.mana_per_second_cost, 20.0);
    assert_eq!(env.player.output.rage_per_second_cost, 8.0);
    assert_eq!(env.player.output.es_percent_per_second_cost, 4.0);
    assert_eq!(env.player.output.ward_percent_per_second_cost, 6.0);

    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            action_rate: 2.0,
            ..Default::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::number("SkillManaCostBase", ModType::Base, 10.0),
        Modifier::number("CrossbowReloadTimeBase", ModType::Base, 1.0),
        Modifier::number("CrossbowBoltCount", ModType::Base, 2.0),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::attack();
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.mana_per_second_cost, 5.0);
}

#[test]
fn placement_throwing_and_channel_times_use_their_own_speed_scopes() {
    use pobr_core::calc::skill_mechanics::skill_timings;
    let mut db = ModDb::new();
    db.add_list(vec![
        Modifier::number("SkillTotemPlacementTimeBase", ModType::Base, 0.6),
        Modifier::number("SkillTrapThrowingTimeBase", ModType::Base, 0.6),
        Modifier::number("SkillMineLayingTimeBase", ModType::Base, 0.3),
        Modifier::number("SkillWarcryCastTimeBase", ModType::Base, 0.8),
        Modifier::number("ChannelMinimumTime", ModType::Base, 0.3),
        Modifier::number("TotemPlacementSpeed", ModType::Inc, 100.0),
        Modifier::number("TrapThrowingSpeed", ModType::Inc, 50.0),
        Modifier::number("WarcrySpeed", ModType::Inc, 25.0),
        Modifier::number("ActionSpeed", ModType::Inc, 100.0),
        Modifier::number("CastSpeed", ModType::Inc, 100.0),
        Modifier::number("TrapThrowCount", ModType::Base, 2.0),
        Modifier::number("MineThrowCount", ModType::Base, 1.5),
    ]);
    let out = skill_timings(&db, &CalcConfig::spell(), 0.5);
    assert_eq!(out.totem, 0.15);
    assert_eq!(out.trap, 0.2);
    assert_eq!(out.mine, 0.15);
    assert_eq!(out.warcry, 0.32);
    assert_eq!(out.channel, 0.3);
    assert_eq!(out.trap_count, 3.0);
    assert_eq!(out.mine_count, 2.5);
    db.add_mod(Modifier::flag("InstantWarcry"));
    assert_eq!(skill_timings(&db, &CalcConfig::spell(), 0.5).warcry, 0.0);
}

#[test]
fn proxy_costs_are_paid_when_placing_it_and_respect_cooldown() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            action_rate: 20.0,
            ..Default::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::number("SkillManaCostBase", ModType::Base, 10.0),
        Modifier::number("SkillTotemPlacementTimeBase", ModType::Base, 0.5),
        Modifier::number("TotemPlacementSpeed", ModType::Inc, 100.0),
    ]);
    let mut env = Env::new(actor.clone());
    env.cfg = CalcConfig::spell();
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.total_totem_placement_time, 0.25);
    assert_eq!(env.player.output.mana_per_second_cost, 40.0);
    actor
        .mod_db
        .add_mod(Modifier::number("SkillCooldownBase", ModType::Base, 1.0));
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell();
    perform(&mut env).unwrap();
    assert!((env.player.output.mana_per_second_cost - 10.0 / 1.023).abs() < 1e-6);
}

#[test]
fn attack_channel_display_reuses_the_use_cycles_stage_time() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            action_rate: 2.0,
            ..Default::default()
        },
    );
    actor.mod_db.add_list(vec![
        Modifier::flag("ChannelCycle"),
        Modifier::number("ChannelMaxStages", ModType::Base, 3.0),
        Modifier::number("ChannelMinimumTime", ModType::Base, 0.3),
        Modifier::number("ChannelReleaseTime", ModType::Base, 0.2),
        Modifier::number("AttackSpeed", ModType::Inc, 100.0),
        Modifier::number("SkillManaCostBase", ModType::Base, 10.0),
    ]);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::attack().with_multiplier("ChannelStages", 2.0);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.channel_time, 0.5);
    assert!((env.player.output.mana_per_second_cost - 10.0 / 0.7).abs() < 1e-6);
}

#[test]
fn movement_penalty_and_presence_use_independent_modifier_groups() {
    let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
    env.cfg.skill_types = SkillTypes::from_pob2_name("UsableWhileMoving").unwrap();
    env.player.mod_db.add_list(vec![
        Modifier::number("MovementSpeed", ModType::Inc, 30.0),
        Modifier::number("ActionSpeed", ModType::Inc, 10.0),
        Modifier::number("SkillMovementSpeed", ModType::More, -70.0),
        Modifier::number("MovementSpeedPenalty", ModType::More, -40.0),
        Modifier::number("PresenceArea", ModType::Inc, 125.0),
        Modifier::number("PresenceRadius", ModType::More, 100.0),
        Modifier::number("AreaOfEffect", ModType::Inc, 900.0),
    ]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.effective_movement_speed_mod, 1.43);
    assert!((env.player.output.movement_speed_while_using_skill - 0.754).abs() < 1e-9);
    assert_eq!(env.player.output.presence_radius_metres, 12.0);
}

#[test]
fn selected_skill_magnitudes_and_minion_limit_do_not_leak() {
    let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
    env.cfg.skill_types = SkillTypes::AURA | SkillTypes::from_pob2_name("AppliesCurse").unwrap();
    env.cfg.skill_name = Some("test".into());
    env.player.mod_db.add_list(vec![
        Modifier::number("AuraEffect", ModType::Inc, 50.0),
        Modifier::number("SkillAuraEffectOnSelf", ModType::Inc, 20.0),
        Modifier::number("Magnitude", ModType::More, 50.0),
        Modifier::number("CurseEffect", ModType::Inc, 40.0),
        Modifier::number("CurseEffect", ModType::More, -50.0),
        Modifier::number("SelectedMinionLimit", ModType::Base, 3.0).with_tag(ModTag::SkillName {
            names: vec!["test".into()],
        }),
        Modifier::number("SelectedMinionLimit", ModType::Base, 8.0).with_tag(ModTag::SkillName {
            names: vec!["other".into()],
        }),
    ]);
    perform(&mut env).unwrap();
    assert!((env.player.output.aura_effect_mod - 2.55).abs() < 1e-9);
    assert_eq!(env.player.output.curse_effect_mod, 0.7);
    assert_eq!(env.player.output.active_minion_limit, 3.0);
}

#[test]
fn double_critical_roll_multiplier_stores_one_plus_conditional_probability() {
    let mut env = Env::new(Actor::new(
        1,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 1.0,
            ..ActorBaseStats::default()
        },
    ));
    env.cfg = CalcConfig::spell();
    env.cfg.mode_effective = true;
    env.player.mod_db.add_list(vec![
        Modifier::number("SkillBaseCritChance", ModType::Base, 50.0),
        Modifier::flag("BifurcateCrit"),
        Modifier::number("EnemyExplodeChance", ModType::Base, 120.0),
    ]);
    perform(&mut env).unwrap();
    assert!(
        (env.player.output.crit_bifurcates - 4.0 / 3.0).abs() < 1e-6,
        "double={} capped={} hit={} crit={}",
        env.player.output.crit_bifurcates,
        env.player.output.pre_effective_crit_chance,
        env.player.output.hit_chance,
        env.player.output.crit_chance
    );
    assert_eq!(env.player.output.explode_chance, 100.0);
    let life = f64::from(env.cfg.constants.monster_scaling.life_at(1));
    assert!(
        (env.player.output.combined_avg_to_monster_life
            - env.player.output.combined_avg / life * 100.0)
            .abs()
            < 1e-8
    );
}

#[test]
fn enemy_reservation_uses_the_strongest_minimum_and_requires_effective_mode() {
    use pobr_core::calc::skill_dot::{SkillDotInputs, calc_skill_dot};
    let db = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_list(vec![
        Modifier::number("LifeReservationPercent", ModType::Base, 10.0),
        Modifier::number("LifeReservationPercent", ModType::Base, 20.0),
    ]);
    let mut cfg = CalcConfig::spell();
    let input = SkillDotInputs {
        base_dps: 100.0,
        ..Default::default()
    };
    assert_eq!(
        calc_skill_dot(&db, &enemy, &cfg, &input).combined_dps,
        100.0
    );
    cfg.mode_effective = true;
    assert_eq!(
        calc_skill_dot(&db, &enemy, &cfg, &input).combined_dps,
        125.0
    );
}

#[test]
fn clone_damage_and_ground_are_views_without_counting_damage_twice() {
    let mut env = Env::new(Actor::new(
        1,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 2.0,
            ..Default::default()
        },
    ));
    env.player.mod_db.add_list(vec![
        Modifier::flag("MirageSkill"),
        Modifier::flag("BurningGround"),
        Modifier::number("FireDot", ModType::Base, 100.0),
    ]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.burning_ground_dps, 100.0);
    assert_eq!(env.player.output.mirage_dps, env.player.output.combined_dps);
    assert_eq!(env.player.output.combined_dps, 300.0);
}

#[test]
fn net_ward_regeneration_subtracts_flat_and_percent_spending() {
    let mut env = Env::new(Actor::new(
        1,
        ActorBaseStats {
            action_rate: 2.0,
            ..Default::default()
        },
    ));
    env.player.mod_db.add_list(vec![
        Modifier::number("Ward", ModType::Base, 100.0),
        Modifier::number("WardRegen", ModType::Base, 30.0),
        Modifier::number("SkillWardCostBase", ModType::Base, 5.0),
        Modifier::number("SkillWardPercentCostBase", ModType::Base, 10.0),
    ]);
    perform(&mut env).unwrap();
    let out = env.player.output;
    assert!(out.ward_per_second_cost > 0.0);
    assert_eq!(
        out.net_ward_regen,
        out.ward_regen_recovery
            - out.ward_per_second_cost
            - out.ward * out.ward_percent_per_second_cost / 100.0
    );
}

#[test]
fn light_radius_transfers_only_increases_and_reductions_at_the_games_rolled_share() {
    let run = |transfer: &str| {
        let mut session = super::support::session(MinimalInput::default());
        session
            .add_modifier_texts(&[
                "30% increased Light Radius",
                "10% reduced Light Radius",
                transfer,
            ])
            .unwrap();
        session.add_modifiers(vec![
            Modifier::number("LightRadius", ModType::More, 100.0),
            Modifier::number("SkillAreaRadiusBase", ModType::Base, 20.0),
        ]);
        session.perform_minimal().unwrap();
        display_value(session.output(), "AoeAreaMod")
    };
    // Mods UniqueMutatedVaalLightRadiusModifiersApplyToAreaOfEffect rolls 25-50.
    assert!((run("Increases and Reductions to Light Radius also apply to Area of Effect at 25% of their value") - 1.05).abs() < 1e-9);
    assert!((run("Increases and Reductions to Light Radius also apply to Area of Effect at 50% of their value") - 1.10).abs() < 1e-9);
    let mut plain = super::support::session(MinimalInput::default());
    plain
        .add_modifier_texts(&["30% increased Light Radius"])
        .unwrap();
    plain.add_modifiers(vec![Modifier::number(
        "SkillAreaRadiusBase",
        ModType::Base,
        20.0,
    )]);
    plain.perform_minimal().unwrap();
    assert_eq!(display_value(plain.output(), "AoeAreaMod"), 1.0);
}

#[test]
fn slower_buff_expiry_divides_elapsed_time_and_does_not_change_non_buff_duration() {
    for buff in [false, true] {
        let mut cfg = CalcConfig::new();
        if buff {
            cfg.skill_types = SkillTypes::from_pob2_name("Buff").unwrap();
        }
        let mut session = super::support::session(MinimalInput::default()).with_config(cfg);
        // Prolonged Assault: buff_time_passed_+%_only_buff_category = -10.
        session
            .add_modifier_texts(&[
                "Buffs on you expire 10% slower",
                "20% increased Skill Effect Duration",
            ])
            .unwrap();
        session.add_modifiers(vec![Modifier::number(
            "SkillDurationBase",
            ModType::Base,
            9.0,
        )]);
        session.perform_minimal().unwrap();
        // 9 * 1.2 / .9 = 12 seconds; rounded upward to 33 ms server ticks.
        assert!((session.output().duration - if buff { 12.012 } else { 10.824 }).abs() < 1e-9);
        // Archon is a buff even when the selected skill is not.
        assert!((display_value(session.output(), "ArchonDuration") - 10.0 / 0.9).abs() < 1e-9);
    }
}

#[test]
fn ailment_calcs_expose_retained_chance_duration_roll_and_source_ranges() {
    // Existing game bases: ignite 4s, bleed 5s, poison 2s. 50% duration
    // and 50% faster damage cancel; 50% magnitude and 20% more give 1.8.
    for (prefix, damage_type, damage_name, base_duration) in [
        ("Ignite", DamageType::Fire, "Fire", 4.0),
        ("Ignite", DamageType::Cold, "Cold", 4.0),
        ("Ignite", DamageType::Lightning, "Lightning", 4.0),
        ("Bleed", DamageType::Physical, "Physical", 5.0),
        ("Poison", DamageType::Physical, "Physical", 2.0),
        ("Poison", DamageType::Chaos, "Chaos", 2.0),
    ] {
        let mut session = super::support::session(MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 200.0,
            base_action_rate: 2.0,
            ..Default::default()
        })
        .with_config(
            CalcConfig::spell()
                .with_damage_type(damage_type)
                .with_mode_effective(true),
        );
        if damage_type != DamageType::Physical {
            session.add_modifiers([Modifier::number(
                "PhysicalDamageConvertTo".to_string() + damage_name,
                ModType::Base,
                100.0,
            )]);
        }
        session.add_modifiers([
            Modifier::number("AilmentChance", ModType::Base, 100.0),
            Modifier::number("EnemyAilmentDuration", ModType::Inc, 50.0),
            Modifier::number(format!("{prefix}Faster"), ModType::Inc, 50.0),
            Modifier::number("AilmentMagnitude", ModType::Inc, 50.0),
            Modifier::number("AilmentMagnitude", ModType::More, 20.0),
            Modifier::flag(format!("{damage_name}Can{prefix}")),
            Modifier::number(format!("{damage_name}{prefix}Buildup"), ModType::More, 25.0),
        ]);
        session.add_enemy_modifiers([
            Modifier::number("FireResist", ModType::Base, 25.0),
            Modifier::number("ChaosResist", ModType::Base, 25.0),
            Modifier::number("DamageTakenOverTime", ModType::Inc, 20.0),
        ]);
        session.perform_minimal().unwrap();
        if damage_type == DamageType::Lightning {
            assert_eq!(display_value(session.output(), "ShockChance"), 100.0);
            let detail = session.derived_detail("ShockChance").unwrap();
            assert_eq!(detail.inputs[0].1 * 100.0, 100.0);
        }
        let expected_stacks = base_duration * 2.0;
        let chance_suffix = if prefix == "Ignite" {
            "ChancePerHit"
        } else {
            "Chance"
        };
        for (suffix, expected) in [
            (chance_suffix.to_string(), 100.0),
            ("Duration".into(), base_duration),
            ("EffMult".into(), if prefix == "Bleed" { 1.2 } else { 0.9 }),
            ("MagnitudeEffect".into(), 1.8),
            ("StacksMax".into(), 1.0),
            ("StackPotentialPercent".into(), expected_stacks * 100.0),
            (
                "RollAverage".into(),
                expected_stacks / (expected_stacks + 1.0) * 100.0,
            ),
            (format!("{damage_name}Min"), 125.0),
            (format!("{damage_name}Max"), 250.0),
        ] {
            let id = format!("{prefix}{suffix}");
            let value = display_value(session.output(), &id);
            assert!(
                (value - expected).abs() < 1e-8,
                "{id}: {value} != {expected}"
            );
            let detail = session.derived_detail(&id).expect(&id);
            assert!(!detail.inputs.iter().any(|(input, _)| input == &id));
            let input = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
            let reconstructed = match suffix.as_str() {
                "Duration" => {
                    input("AilmentDurationBeforeRate") * input("EnemyDebuffDuration")
                        / input("AilmentDamageRate")
                }
                "MagnitudeEffect" => {
                    (1.0 + input("AilmentMagnitudeIncreased") / 100.0)
                        * input("AilmentMagnitudeMore")
                }
                "StackPotentialPercent" => {
                    input("EstimatedAilmentStacks") / input("MaximumAilmentStacks") * 100.0
                }
                "RollAverage" => {
                    (input("EstimatedAilmentStacks") - (input("MaximumAilmentStacks") - 1.0) / 2.0)
                        / (input("EstimatedAilmentStacks") + 1.0)
                        * 100.0
                }
                "EffMult" => {
                    (1.0 - input("AilmentEnemyResistance") / 100.0)
                        * (1.0 + input("AilmentDamageTakenIncreased") / 100.0)
                        * input("AilmentDamageTakenMore")
                }
                "ChancePerHit" => input("AilmentApplicationChance") * 100.0,
                "Chance" => input("AilmentPass1Chance") / input("AilmentPassCount") * 100.0,
                _ => detail.inputs[0].1,
            };
            assert!((value - reconstructed).abs() < 1e-8, "{id} detail");
        }
    }
    // No damage/chance creates no ailment instance, including neutral multipliers.
    let mut empty = super::support::session(MinimalInput::default());
    empty.perform_minimal().unwrap();
    for id in [
        "IgniteChancePerHit",
        "BleedDuration",
        "PoisonEffMult",
        "IgniteMagnitudeEffect",
        "PoisonRollAverage",
        "BleedStacksMax",
        "IgniteStackPotentialPercent",
        "IgniteFireMin",
        "PoisonChaosMax",
        "ShockChance",
    ] {
        assert_eq!(display_value(empty.output(), id), 0.0, "{id}");
        assert!(empty.derived_detail(id).is_some(), "{id}");
    }
}

#[test]
fn hit_calcs_keep_fractional_bounds_and_reuse_noncritical_mitigation() {
    // 7.5–12.5 base, 20% increased and 50% more = 13.5–22.5.
    // 25% enemy reduction and 20% increased taken = 0.9, hence 12.15–20.25.
    for name in ["Physical", "Fire", "Cold", "Lightning", "Chaos"] {
        let mut session = super::support::session(MinimalInput {
            base_hit_min: 7.5,
            base_hit_max: 12.5,
            ..Default::default()
        })
        .with_config(CalcConfig::spell().with_mode_effective(true));
        session.add_modifiers([
            Modifier::number("Damage", ModType::Inc, 20.0),
            Modifier::number("Damage", ModType::More, 50.0),
            Modifier::number("CriticalStrikeChance", ModType::Base, 25.0),
            Modifier::number("DPS", ModType::Inc, 20.0),
            Modifier::number("DPS", ModType::More, 50.0),
            Modifier::number("GrenadeActivateTwice", ModType::Base, 50.0),
            Modifier::number("RepeatCount", ModType::Base, 2.0),
            Modifier::number("QuantityMultiplier", ModType::Base, 3.0),
        ]);
        if name != "Physical" {
            session.add_modifiers([Modifier::number(
                format!("PhysicalDamageConvertTo{name}"),
                ModType::Base,
                100.0,
            )]);
        }
        session.add_enemy_modifiers([
            Modifier::number(
                if name == "Physical" {
                    "PhysicalDamageReduction".into()
                } else {
                    format!("{name}Resist")
                },
                ModType::Base,
                25.0,
            ),
            Modifier::number("DamageTaken", ModType::Inc, 20.0),
        ]);
        session.perform_minimal().unwrap();
        let out = session.output();
        for (id, expected) in [
            (format!("{name}Min"), 12.15),
            (format!("{name}Max"), 20.25),
            (format!("{name}EffMult"), 0.9),
            ("TotalMin".into(), 12.15),
            ("TotalMax".into(), 20.25),
            ("CritEffect".into(), 1.25),
            ("DpsMultiplier".into(), 2.7),
            ("Repeats".into(), 3.0),
        ] {
            assert!(
                (display_value(out, &id) - expected).abs() < 1e-9,
                "{name} {id}: {}",
                display_value(out, &id)
            );
            let d = session.derived_detail(&id).unwrap();
            let get = |key: &str| d.inputs.iter().find(|(k, _)| k == key).unwrap().1;
            let rebuilt = if id.ends_with("EffMult") {
                get("HitTypePresent")
                    * get("EnemyTakenFactor")
                    * (1.0 - get("HitReduction") / 100.0)
            } else if id.starts_with("Total") {
                d.inputs.iter().map(|(_, v)| v).sum()
            } else if id.ends_with("Min") || id.ends_with("Max") {
                get("PreMitigationBound") * get("HitEnemyMultiplier")
            } else if id == "CritEffect" {
                1.0 - get("HitCritChance") + get("HitCritChance") * get("HitCritMultiplier")
            } else if id == "Repeats" {
                1.0 + get("AdditionalRepeats").max(0.0)
            } else {
                get("SkillDpsFactor")
                    * (1.0 + get("IncreasedDps") / 100.0)
                    * get("MoreDps")
                    * get("GrenadeDpsFactor")
                    * get("BarrageDpsFactor")
            };
            assert!((rebuilt - expected).abs() < 1e-9, "{id} detail");
        }
    }
}

#[test]
fn hit_calcs_keep_each_hands_range_and_combine_only_dps_multiplier() {
    for double_hits in [false, true] {
        let mut session = super::support::session(MinimalInput::default())
            .with_config(CalcConfig::attack().with_mode_effective(true));
        session.set_hand_sources(
            vec![
                HandSource::main_hand(WeaponBase {
                    hit_min: 7.5,
                    hit_max: 12.5,
                    ..Default::default()
                }),
                HandSource::off_hand(WeaponBase {
                    hit_min: 30.0,
                    hit_max: 40.0,
                    ..Default::default()
                }),
            ],
            double_hits,
        );
        session.add_enemy_modifiers([Modifier::number(
            "PhysicalDamageReduction",
            ModType::Base,
            50.0,
        )]);
        session.add_modifiers([
            Modifier::number("DPS", ModType::More, 100.0)
                .with_tag(ModTag::condition("MainHandAttack", false)),
            Modifier::number("DPS", ModType::More, 200.0)
                .with_tag(ModTag::condition("OffHandAttack", false)),
        ]);
        session.perform_minimal().unwrap();
        let out = session.output();
        for (id, expected) in [
            ("MainHandPhysicalMin", 3.75),
            ("MainHandTotalMax", 6.25),
            ("OffHandPhysicalMin", 15.0),
            ("OffHandTotalMax", 20.0),
            ("MainHandPhysicalEffMult", 0.5),
            ("OffHandPhysicalEffMult", 0.5),
            ("MainHandDpsMultiplier", 2.0),
            ("OffHandDpsMultiplier", 3.0),
            ("TotalMin", 0.0),
            ("PhysicalMax", 0.0),
            ("CritEffect", 0.0),
            ("DpsMultiplier", if double_hits { 5.0 } else { 2.5 }),
        ] {
            assert_eq!(display_value(out, id), expected, "{id}");
            assert!(session.derived_detail(id).is_some(), "{id}");
        }
        let d = session.derived_detail("DpsMultiplier").unwrap();
        let values: Vec<_> = d.inputs.iter().map(|(_, v)| *v).collect();
        assert_eq!(
            (values[0] + values[1]) / values[2],
            display_value(out, "DpsMultiplier")
        );
    }
}

#[test]
fn total_number_of_hits_exposes_ehp_survival_after_avoidance() {
    let mut session = super::support::session(MinimalInput {
        base_life: 2000.0,
        ..Default::default()
    });
    session.setup_enemy(82, pobr_data::prelude::EnemyTier::Pinnacle);
    session.perform_minimal().unwrap();
    let d = session.derived_detail("TotalNumberOfHits").unwrap();
    let get = |key: &str| d.inputs.iter().find(|(k, _)| k == key).unwrap().1;
    let expected = get("MitigatedSurvivalHits") / (1.0 - get("AvoidedHitChance") / 100.0);
    assert_eq!(
        display_value(session.output(), "TotalNumberOfHits"),
        expected
    );
    assert!(
        (expected * session.output().total_enemy_damage_in - session.output().total_ehp).abs()
            < 1e-8
    );
}

/// Freeze lasts 4 seconds by default (game Freeze keyword: "lasts 4 seconds
/// by default"). `freeze_duration_+%` / `chill_and_freeze_duration_+%`
/// (`EnemyFreezeDuration` INC), the elemental-ailment duration increase and
/// the target's own `SelfFreezeDuration` add; `active_skill_freeze_duration_+%_final`
/// multiplies; the enemy's debuff expiration divides. The derived detail
/// rebuilds the value from its retained inputs.
#[test]
fn freeze_duration_uses_the_keyword_base_and_inflicted_duration_modifiers() {
    let cold_session = |mods: Vec<Modifier>, enemy: Vec<Modifier>| {
        let mut session = super::support::session(MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 200.0,
            base_action_rate: 2.0,
            ..Default::default()
        })
        .with_config(
            CalcConfig::spell()
                .with_damage_type(DamageType::Cold)
                .with_mode_effective(true),
        );
        session.add_modifiers([Modifier::number(
            "PhysicalDamageConvertToCold",
            ModType::Base,
            100.0,
        )]);
        session.add_modifiers(mods);
        session.add_enemy_modifiers(enemy);
        session.perform_minimal().unwrap();
        session
    };
    let plain = cold_session(vec![], vec![]);
    assert!(plain.output().freeze_buildup_avg > 0.0);
    assert_eq!(display_value(plain.output(), "FreezeDuration"), 4.0);

    let session = cold_session(
        vec![
            Modifier::number("EnemyFreezeDuration", ModType::Inc, 25.0),
            Modifier::number("EnemyElementalAilmentDuration", ModType::Inc, 10.0),
            Modifier::number("EnemyFreezeDuration", ModType::More, 50.0),
        ],
        vec![
            Modifier::number("SelfFreezeDuration", ModType::Inc, -15.0),
            Modifier::number("BuffExpireFaster", ModType::More, -20.0),
        ],
    );
    // 4 * (1 + (25 + 10 - 15) / 100) * 1.5 / 0.8 = 9.
    let value = display_value(session.output(), "FreezeDuration");
    assert!((value - 9.0).abs() < 1e-9, "{value}");
    let detail = session.derived_detail("FreezeDuration").unwrap();
    let input = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
    let rebuilt = input("BaseAilmentDuration")
        * (1.0 + input("AilmentDurationIncreased") / 100.0)
        * input("AilmentDurationMore")
        * input("EnemyDebuffDuration");
    assert!((rebuilt - value).abs() < 1e-9);

    // A hit that cannot build Freeze reports no Freeze duration.
    let cannot = cold_session(vec![Modifier::flag("CannotFreeze")], vec![]);
    assert_eq!(display_value(cannot.output(), "FreezeDuration"), 0.0);
}
