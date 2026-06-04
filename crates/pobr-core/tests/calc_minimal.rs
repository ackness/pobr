use pobr_core::calc::{MinimalInput, calculate_minimal, calculate_minimal_traced};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

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

#[test]
fn minimal_calculation_includes_crit_average() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
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
    assert_eq!(output.crit_multiplier, 2.0);
    assert_eq!(output.total_hit_avg, 110.0);
    assert_eq!(output.dps, 110.0);
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
