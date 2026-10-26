use pobr_core::calc::{MinimalInput, calculate_minimal, calculate_minimal_traced};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

#[test]
fn eldritch_will_combined_pools_use_player_names_and_retain_source() {
    use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
    let text = "3% increased maximum Life, Mana and [EnergyShield|Energy Shield]";
    let parsed = crate::support::parse_mod(text).unwrap();
    assert!(parsed.is_complete());
    let mut env = Env::new(Actor::new(
        90,
        ActorBaseStats {
            life: 4353.0,
            mana: 894.0,
            energy_shield: 1000.0,
            ..Default::default()
        },
    ));
    env.player.mod_db.add_list([
        Modifier::number("MaximumLife", ModType::Inc, 73.0),
        Modifier::number("MaximumMana", ModType::Inc, 16.0),
    ]);
    env.player.mod_db.add_list(parsed.mods.into_iter().map(|m| {
        m.with_origin(
            ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.3894"))
                .with_raw_text(text),
        )
    }));
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.life, 7661.28);
    assert_eq!(env.player.output.mana, 1063.86);
    assert_eq!(env.player.output.energy_shield, 1030.0);
    for name in ["MaximumLife", "MaximumMana", "EnergyShield"] {
        let modifier = env
            .player
            .mod_db
            .named(name)
            .find(|m| {
                m.origin
                    .as_ref()
                    .is_some_and(|o| o.source_id.id == "node.3894")
            })
            .unwrap();
        assert_eq!(modifier.mod_type, ModType::Inc);
        assert_eq!(
            modifier.origin.as_ref().unwrap().raw_text.as_deref(),
            Some(text)
        );
    }
    assert!(env.player.mod_db.named("Life").next().is_none());
    assert!(env.player.mod_db.named("Mana").next().is_none());
}

#[test]
fn blackflame_covenant_converts_only_fire_spells_and_preserves_ignite_magnitude() {
    use pobr_core::TraceGraph;
    use pobr_core::calc::ailment::{AilmentSource, ignite_traced};
    use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};

    let fire_type = SkillTypes::from_pob2_name("Fire").unwrap();
    let lines = [
        "Fire Spells Convert 100% of Fire Damage to Chaos Damage",
        "Chaos Damage from Fire Spells Contributes to Flammability and Ignite Magnitudes",
        "Ignite inflicted with Fire Spells deals Chaos Damage instead of Fire Damage",
    ];
    for (types, enabled) in [
        (SkillTypes::SPELL | fire_type, true),
        (SkillTypes::SPELL, false),
        (SkillTypes::ATTACK | fire_type, false),
    ] {
        let mut env = Env::new(Actor::new(
            80,
            ActorBaseStats {
                life: 1000.0,
                action_rate: 1.0,
                ..Default::default()
            },
        ));
        env.cfg = CalcConfig::spell()
            .with_skill_types(types)
            .with_mode_effective(true);
        for line in lines {
            let parsed = crate::support::parse_mod(line).unwrap();
            assert!(parsed.is_complete(), "{line}: {parsed:?}");
            env.player.mod_db.add_list(parsed.mods.into_iter().map(|m| {
                m.with_origin(
                    ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.42680"))
                        .with_raw_text(line),
                )
            }));
        }
        env.player.mod_db.add_list([
            Modifier::number("FireDamageMin", ModType::Base, 100.0),
            Modifier::number("FireDamageMax", ModType::Base, 100.0),
            Modifier::number("FireDamage", ModType::Inc, 900.0),
            Modifier::number("ChaosDamage", ModType::Inc, 100.0),
            Modifier::number("AilmentChance", ModType::Base, 100.0),
            Modifier::number("ChaosPenetration", ModType::Base, 20.0),
        ]);
        env.enemy.mod_db.add_list([
            Modifier::number("FireResist", ModType::Base, 75.0),
            Modifier::number("ChaosResist", ModType::Base, 20.0),
            Modifier::number("ChaosDamageTaken", ModType::Inc, 50.0),
        ]);
        perform(&mut env).unwrap();
        // Conversion uses the destination increases only. Ignite magnitude
        // comes from that hit once; only its mitigation changes to Chaos.
        let source = if enabled { 200.0 } else { 1000.0 };
        let mitigation = if enabled { 1.2 } else { 0.25 };
        assert!((env.player.output.total_hit_avg - source).abs() < 1e-9);
        assert!((env.player.output.ignite_dps - source * 0.2 * mitigation).abs() < 1e-9);

        let mut trace = TraceGraph::new();
        let (ignite, node) = ignite_traced(
            &AilmentSource::new(source, 1.0, 0.0, false),
            &env.player.mod_db,
            &env.enemy.mod_db,
            &env.cfg,
            1000.0,
            &mut trace,
        );
        assert!((ignite.eff_mult - mitigation).abs() < 1e-9);
        assert_eq!(
            trace
                .source_ancestors(node)
                .iter()
                .any(|s| s.id == "node.42680"),
            enabled
        );
        assert_eq!(
            trace
                .nodes()
                .iter()
                .any(|n| n.label == "enemy ChaosResist BASE"),
            enabled
        );
        assert_eq!(
            trace
                .nodes()
                .iter()
                .any(|n| n.label == "enemy FireResist BASE"),
            !enabled
        );
        // Changing an Ignite's type alone does not change source eligibility.
        assert_eq!(env.player.mod_db.flag(&env.cfg, "ChaosCanIgnite"), enabled);
        let mut session =
            pobr_core::calc::CalculationSession::new(MinimalInput::from(env.player.base))
                .with_config(env.cfg.clone());
        session.add_modifiers(env.player.mod_db.iter_mods().cloned());
        session.add_enemy_modifiers(env.enemy.mod_db.iter_mods().cloned());
        session.perform_minimal().unwrap();
        let detail = session.derived_detail("IgniteEffMult").unwrap();
        let input = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
        let reconstructed = (1.0 - input("AilmentEnemyResistance") / 100.0)
            * (1.0 + input("AilmentDamageTakenIncreased") / 100.0)
            * input("AilmentDamageTakenMore");
        assert!((reconstructed - mitigation).abs() < 1e-9);
        assert!(
            (pobr_core::display_catalog::display_value(session.output(), "IgniteEffMult")
                - reconstructed)
                .abs()
                < 1e-9
        );
    }
}

#[test]
fn chain_remaining_trace_uses_the_selected_hits_snapshot_in_both_crit_legs() {
    use pobr_core::ModTag;

    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ChainCountMax", ModType::Base, 2.0));
    db.add_mod(
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_keyword_flags(KeywordFlags::HIT)
            .with_tag(ModTag::multiplier("ChainRemaining", 1.0, None)),
    );
    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..Default::default()
    };
    for (used, remaining) in [(None, 2.0), (Some(1.0), 1.0), (Some(7.0), 0.0)] {
        let mut cfg = CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE);
        if let Some(used) = used {
            cfg = cfg.with_multiplier("Chain", used);
        }
        // A minimal caller has not precomputed ChainRemaining or the hit keyword.
        assert!(cfg.multiplier_if_set("ChainRemaining").is_none());
        let traced = calculate_minimal_traced(&db, &cfg, &input);
        let plain = calculate_minimal(&db, &cfg, &input);
        assert_eq!(traced.output, plain);
        assert!((plain.total_hit_avg - (100.0 + remaining * 10.0)).abs() < 1e-9);
        assert_eq!(plain.dps, plain.total_hit_avg);
        for leg in ["non-crit", "crit"] {
            let label = format!("Damage INC modifier sum ({leg} pass)");
            let sum = traced
                .trace
                .nodes()
                .iter()
                .find(|node| node.label == label)
                .unwrap();
            assert_eq!(sum.value, remaining * 10.0, "{leg}, used={used:?}");
        }
    }
}

#[test]
fn calculates_life_resistance_hit_damage_and_dps() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("MaximumLife", ModType::Base, 100.0));
    db.add_mod(Modifier::number("MaximumLife", ModType::Inc, 20.0));
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 40.0));
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 10.0).with_flags(ModFlags::ATTACK));

    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };

    let output = calculate_minimal(&db, &CalcConfig::attack(), &input);

    assert_eq!(output.life, 1_320.0);
    assert_eq!(output.mana, 100.0);
    assert_eq!(output.fire_resistance, 40.0);
    assert_eq!(output.total_hit_avg, 270.0);
    assert_eq!(output.action_rate, 2.2);
    assert_eq!(output.dps, 594.0);
    assert!(
        output
            .breakdown
            .iter()
            .any(|step| step.name == "total_hit_avg")
    );
}

#[test]
fn minimal_calculation_respects_damage_context() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(pobr_core::ModTag::DamageType(DamageType::Physical)),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(pobr_core::ModTag::DamageType(DamageType::Fire)),
    );

    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 10.0,
        base_hit_max: 10.0,
        base_action_rate: 1.0,
    };

    let output = calculate_minimal(
        &db,
        &CalcConfig::attack().with_damage_type(DamageType::Physical),
        &input,
    );

    assert_eq!(output.total_hit_avg, 20.0);
    assert_eq!(output.dps, 20.0);
}

/// PoE2 crit expected-value test (PoE2's base crit damage bonus is +100%, not PoE1's +50%).
///
/// PoE2 formula (agent-docs/critical-hits.md §Crit Damage Bonus, CalcOffence.lua):
///   crit_mult = 1 + max(0, (PLAYER_BASE_CRIT_DAMAGE_BONUS=100 + base_mod=50) / 100) = 2.5
///   crit_avg_factor = 1 + 0.1 * (2.5 - 1.0) = 1.15
///   total_hit_avg = 100 * 1.15 = 115
#[test]
fn minimal_calculation_includes_crit_average() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    // +50 CritMultiplier BASE (from a weapon/gem modifier, for example)
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        50.0,
    ));

    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };

    let output = calculate_minimal(&db, &CalcConfig::attack(), &input);

    assert_eq!(output.crit_chance, 0.1);
    // PoE2: base_bonus=100+50=150 → crit_mult = 1 + 150/100 = 2.5
    assert_eq!(output.crit_multiplier, 2.5);
    // crit_avg_factor = 1 + 0.1*(2.5-1.0) = 1.15
    assert_eq!(output.total_hit_avg, 115.0);
    assert_eq!(output.dps, 115.0);
}

#[test]
fn minimal_calculation_applies_hit_chance_to_dps_only() {
    let db = ModDb::new();
    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 600.0,
        enemy_evasion: 1_000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };
    let expected_hit_chance = pobr_core::calc::hit_chance(1_000.0, 600.0);

    let output = calculate_minimal(&db, &CalcConfig::attack(), &input);

    assert_eq!(output.total_hit_avg, 100.0);
    assert_eq!(output.hit_chance, expected_hit_chance);
    assert_eq!(output.dps, 100.0 * expected_hit_chance);
}

#[test]
fn traced_minimal_calculation_links_pool_and_resistance_outputs_to_sources() {
    let item_life = ModifierSource::new(
        SourceId::new(SourceKind::ItemAffix, "body.explicit.1").with_label_key("items.body"),
    )
    .with_parent(SourceId::new(SourceKind::Item, "body"))
    .with_raw_text("+100 to maximum Life");
    let passive_life = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.life.1"))
        .with_raw_text("20% increased maximum Life");
    let item_res = ModifierSource::new(SourceId::new(SourceKind::ItemAffix, "ring.explicit.1"))
        .with_parent(SourceId::new(SourceKind::Item, "ring"))
        .with_raw_text("+40% to Fire Resistance");

    let mut db = ModDb::new();
    db.add_mod(Modifier::number("MaximumLife", ModType::Base, 100.0).with_origin(item_life));
    db.add_mod(Modifier::number("MaximumLife", ModType::Inc, 20.0).with_origin(passive_life));
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 40.0).with_origin(item_res));

    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };

    let traced = calculate_minimal_traced(&db, &CalcConfig::new(), &input);
    let plain = calculate_minimal(&db, &CalcConfig::new(), &input);

    assert_eq!(traced.output.life, plain.life);
    assert_eq!(traced.output.fire_resistance, plain.fire_resistance);

    let life_node = traced.node_for(DisplayStatId::from("Life")).unwrap();
    let life_sources = traced.trace.source_ancestors(life_node);
    assert!(
        life_sources
            .iter()
            .any(|source| source.kind == SourceKind::CharacterBase)
    );
    assert!(
        life_sources
            .iter()
            .any(|source| source.kind == SourceKind::ItemAffix && source.id == "body.explicit.1")
    );
    assert!(
        life_sources
            .iter()
            .any(|source| source.kind == SourceKind::PassiveNode && source.id == "node.life.1")
    );

    let fire_node = traced.node_for(DisplayStatId::from("FireResist")).unwrap();
    let fire_sources = traced.trace.source_ancestors(fire_node);
    assert!(
        fire_sources
            .iter()
            .any(|source| source.kind == SourceKind::ItemAffix && source.id == "ring.explicit.1")
    );
}

#[test]
fn traced_minimal_calculation_links_total_dps_to_damage_speed_and_accuracy_sources() {
    let weapon_damage = ModifierSource::new(
        SourceId::new(SourceKind::ItemAffix, "weapon.explicit.1").with_label_key("items.weapon"),
    )
    .with_parent(SourceId::new(SourceKind::Item, "weapon"))
    .with_raw_text("50% increased Physical Damage");
    let support_more = ModifierSource::new(SourceId::new(SourceKind::SupportGem, "brutality"))
        .with_raw_text("20% more Physical Damage");
    let passive_speed = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.speed.1"))
        .with_raw_text("10% increased Attack Speed");
    let ring_accuracy =
        ModifierSource::new(SourceId::new(SourceKind::ItemAffix, "ring.explicit.2"))
            .with_parent(SourceId::new(SourceKind::Item, "ring"))
            .with_raw_text("+600 to Accuracy Rating");

    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 50.0)
            .with_flags(ModFlags::ATTACK)
            .with_origin(weapon_damage),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0)
            .with_flags(ModFlags::ATTACK)
            .with_origin(support_more),
    );
    db.add_mod(
        Modifier::number("AttackSpeed", ModType::Inc, 10.0)
            .with_flags(ModFlags::ATTACK)
            .with_origin(passive_speed),
    );
    db.add_mod(Modifier::number("Accuracy", ModType::Base, 600.0).with_origin(ring_accuracy));

    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 1_000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };
    let cfg = CalcConfig::attack();

    let traced = calculate_minimal_traced(&db, &cfg, &input);
    let plain = calculate_minimal(&db, &cfg, &input);

    assert_eq!(traced.output.total_hit_avg, plain.total_hit_avg);
    assert_eq!(traced.output.action_rate, plain.action_rate);
    assert_eq!(traced.output.hit_chance, plain.hit_chance);
    assert_eq!(traced.output.dps, plain.dps);

    let dps_node = traced.node_for(DisplayStatId::from("TotalDPS")).unwrap();
    let dps_sources = traced.trace.source_ancestors(dps_node);

    assert!(
        dps_sources
            .iter()
            .any(|source| source.kind == SourceKind::ItemAffix && source.id == "weapon.explicit.1")
    );
    assert!(
        dps_sources
            .iter()
            .any(|source| source.kind == SourceKind::SupportGem && source.id == "brutality")
    );
    assert!(
        dps_sources
            .iter()
            .any(|source| source.kind == SourceKind::PassiveNode && source.id == "node.speed.1")
    );
    assert!(
        dps_sources
            .iter()
            .any(|source| source.kind == SourceKind::ItemAffix && source.id == "ring.explicit.2")
    );
    assert!(
        dps_sources
            .iter()
            .any(|source| source.kind == SourceKind::EnemyConfig && source.id == "enemy.evasion")
    );
}

/// Bug#2 test: the crit-multiplier INC bucket applies.
///
/// Source: agent-docs/critical-hits.md §Crit Damage Bonus, CalcOffence.lua
///   `extraDamage = Sum("BASE","CritMultiplier")/100 * (1+inc/100) * more`
#[test]
fn crit_multiplier_scales_with_inc() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        100.0,
    ));
    // INC 100 → base_bonus * (1+1.0) = 2× base_bonus (base_bonus = 100/100 = 1.0)
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Inc,
        100.0,
    ));

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let output = calculate_minimal(&db, &CalcConfig::attack(), &input);

    // base_bonus = (100+0)/100=1.0; *(1+100/100)=2.0 → crit_mult=1+2.0=3.0
    assert_eq!(output.crit_multiplier, 3.0);
    assert_eq!(output.total_hit_avg, 300.0);
}

#[test]
fn crit_multiplier_scales_with_more() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        100.0,
    ));
    // MORE 50 → factor 1.5; bonus=1.0*1.5=1.5 → crit_mult=2.5
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::More,
        50.0,
    ));

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let output = calculate_minimal(&db, &CalcConfig::attack(), &input);

    assert_eq!(output.crit_multiplier, 2.5);
}

/// Bug#4 test: spells always hit (skip the accuracy pipeline, hit_chance = 1.0).
///
/// Source: agent-docs/accuracy-and-enemy.md §3
///   `if not isAttack then output.AccuracyHitChance = 100`.
#[test]
fn spell_always_hits_regardless_of_evasion() {
    let db = ModDb::new();
    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        base_accuracy: 1.0,
        enemy_evasion: 999_999.0,
        ..MinimalInput::default()
    };
    let spell_output = calculate_minimal(&db, &CalcConfig::spell(), &input);
    assert_eq!(spell_output.hit_chance, 1.0);
    assert_eq!(spell_output.dps, 100.0);

    let attack_output = calculate_minimal(&db, &CalcConfig::attack(), &input);
    assert!(attack_output.hit_chance < 1.0);
}

/// Bug#5 test: traced DPS numerically matches non-traced DPS.
#[test]
fn traced_dps_matches_non_traced_dps() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 10.0).with_flags(ModFlags::ATTACK));

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::attack();
    let plain = calculate_minimal(&db, &cfg, &input);
    let traced = calculate_minimal_traced(&db, &cfg, &input);

    assert_eq!(traced.output.dps, plain.dps);
    assert_eq!(traced.output.total_hit_avg, plain.total_hit_avg);
    assert_eq!(traced.output.crit_multiplier, plain.crit_multiplier);
}

/// Player-side resist has two naming conventions: vendor's special short name
/// (`FireResist`) and the PoBR parser's long name (`FireResistance`) sum into the
/// same bucket (vendor CalcDefence.lua:895 sums both names).
#[test]
fn resistance_vendor_short_name_shares_bucket_with_long_name() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 30.0));
    db.add_mod(Modifier::number("FireResist", ModType::Base, 20.0));
    db.add_mod(Modifier::number("ElementalResist", ModType::Base, 5.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &MinimalInput::default());

    assert_eq!(output.fire_resistance, 55.0);
    // The shared name `ElementalResist` likewise feeds the cold/lightning buckets (vendor isElemental).
    assert_eq!(output.cold_resistance, 5.0);
    assert_eq!(output.lightning_resistance, 5.0);
}

/// OVERRIDE pins a resist ("fire resistance is N%"): it skips base×inc, but the final
/// value is still clamped to the max resist (vendor CalcDefence.lua:891 override / :924
/// final clamp).
#[test]
fn resistance_override_locks_value_but_respects_max_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 40.0));
    db.add_mod(Modifier::number("FireResist", ModType::Override, 120.0));
    db.add_mod(Modifier::number("ColdResistance", ModType::Base, 60.0));
    db.add_mod(Modifier::number("ColdResist", ModType::Override, 0.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &MinimalInput::default());

    // Override 120 overrides the BASE modifiers, but the final value clamps to the default max 75.
    assert_eq!(output.fire_resistance, 75.0);
    // "elemental resistances are zero": override 0 overrides the BASE 60.
    assert_eq!(output.cold_resistance, 0.0);
}

/// The INC/MORE multiplier stage ("reduced fire resistance"):
/// `(base + ΣBASE) × max(1+ΣINC/100, 0)` (vendor CalcDefence.lua:896-897; a negative
/// factor clamps to 0).
#[test]
fn resistance_inc_scales_and_clamps_at_zero() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 40.0));
    db.add_mod(Modifier::number("FireResist", ModType::Inc, -50.0));
    db.add_mod(Modifier::number("ColdResistance", ModType::Base, 40.0));
    db.add_mod(Modifier::number("ColdResist", ModType::Inc, -150.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &MinimalInput::default());

    assert_eq!(output.fire_resistance, 20.0);
    // factor = max(1 - 1.5, 0) = 0.
    assert_eq!(output.cold_resistance, 0.0);
}

/// Max-resist OVERRIDE ("your maximum resistances are N%", vendor name `<X>ResistMax`):
/// the override is taken as-is and bypasses hard_cap (vendor CalcDefence.lua:875/:914);
/// the BASE form (long name `Maximum<X>Resistance`) is still clamped by hard_cap.
#[test]
fn resistance_max_override_bypasses_hard_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 100.0));
    db.add_mod(Modifier::number("FireResistMax", ModType::Override, 92.0));
    db.add_mod(Modifier::number("ColdResistance", ModType::Base, 100.0));
    db.add_mod(Modifier::number(
        "MaximumColdResistance",
        ModType::Base,
        30.0,
    ));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &MinimalInput::default());

    // max override 92 > hard_cap 90 still applies (vendor's override bypasses m_min).
    assert_eq!(output.fire_resistance, 92.0);
    // BASE raises it to 75+30=105 → hard_cap clamps to 90.
    assert_eq!(output.cold_resistance, 90.0);
}
