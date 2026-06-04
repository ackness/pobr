use pobr_core::{CalcConfig, ModDb, ModList, ModTag, Modifier, TraceGraph, TraceOperation};
use pobr_data::prelude::*;

#[test]
fn sum_filters_by_condition_flags_and_damage_type() {
    let mut db = ModDb::new();

    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 40.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Physical)),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 25.0)
            .with_flags(ModFlags::SPELL)
            .with_tag(ModTag::DamageType(DamageType::Physical)),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 15.0).with_tag(ModTag::Condition {
            var: "OnFullLife".into(),
            negated: false,
        }),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 90.0)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let cfg = CalcConfig::new()
        .with_flags(ModFlags::ATTACK)
        .with_damage_type(DamageType::Physical)
        .with_condition("OnFullLife", true);

    assert_eq!(
        db.sum(ModType::Inc, &cfg, &[ModName::from("PhysicalDamage")]),
        55.0
    );
}

#[test]
fn more_uses_path_of_building_percent_factor_semantics() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackDamage", ModType::More, 20.0));
    db.add_mod(Modifier::number("AttackDamage", ModType::More, -10.0));

    let more = db.more(&CalcConfig::new(), &[ModName::from("AttackDamage")]);

    assert!((more - 1.08).abs() < f64::EPSILON);
}

#[test]
fn multiplier_tags_scale_values_and_apply_limits() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 8.0).with_tag(ModTag::Multiplier {
            var: "AllocatedSmallPassives".into(),
            limit: Some(5.0),
        }),
    );

    let cfg = CalcConfig::new().with_multiplier("AllocatedSmallPassives", 9.0);

    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumLife")]),
        40.0
    );
}

#[test]
fn negated_conditions_match_when_condition_is_disabled() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("AttackDamage", ModType::Inc, 30.0).with_tag(ModTag::Condition {
            var: "OnFullLife".into(),
            negated: true,
        }),
    );

    assert_eq!(
        db.sum(
            ModType::Inc,
            &CalcConfig::new().with_condition("OnFullLife", false),
            &[ModName::from("AttackDamage")]
        ),
        30.0
    );
    assert_eq!(
        db.sum(
            ModType::Inc,
            &CalcConfig::new().with_condition("OnFullLife", true),
            &[ModName::from("AttackDamage")]
        ),
        0.0
    );
}

#[test]
fn flag_override_and_list_queries_return_matching_mods() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("CanIgnite"));
    db.add_mod(Modifier::number(
        "MaximumFireResistance",
        ModType::Override,
        80.0,
    ));
    db.add_mod(Modifier::text(
        "GrantedSkill",
        ModType::List,
        "Level 20 Fireball",
    ));

    let cfg = CalcConfig::new();

    assert!(db.flag(&cfg, ModName::from("CanIgnite")));
    assert_eq!(
        db.override_(&cfg, ModName::from("MaximumFireResistance")),
        Some(80.0)
    );
    assert_eq!(
        db.list(&cfg, ModName::from("GrantedSkill")),
        vec!["Level 20 Fireball".to_string()]
    );
}

#[test]
fn mod_list_sums_local_and_parent_modifiers() {
    let mut parent = ModList::new();
    parent.add_mod(Modifier::number("MaximumLife", ModType::Base, 100.0));

    let mut child = ModList::with_parent(parent);
    child.add_mod(Modifier::number("MaximumLife", ModType::Base, 25.0));

    assert_eq!(
        child.sum(
            ModType::Base,
            &CalcConfig::new(),
            &[ModName::from("MaximumLife")]
        ),
        125.0
    );
}

#[test]
fn contributions_keep_structured_sources_after_filtering() {
    let weapon_affix = ModifierSource::new(
        SourceId::new(SourceKind::ItemAffix, "weapon.explicit.1").with_label_key("items.weapon"),
    )
    .with_parent(SourceId::new(SourceKind::Item, "weapon"))
    .with_raw_text("+50 to maximum Life");
    let passive_node = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.123"))
        .with_raw_text("+25 to maximum Life while on Full Life");
    let inactive_config =
        ModifierSource::new(SourceId::new(SourceKind::Config, "condition.low_life"));

    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 50.0).with_origin(weapon_affix.clone()),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 25.0)
            .with_origin(passive_node.clone())
            .with_tag(ModTag::Condition {
                var: "FullLife".into(),
                negated: false,
            }),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 99.0)
            .with_origin(inactive_config)
            .with_tag(ModTag::Condition {
                var: "LowLife".into(),
                negated: false,
            }),
    );

    let cfg = CalcConfig::new().with_condition("FullLife", true);
    let contributions = db.contributions(ModType::Base, &cfg, &[ModName::from("MaximumLife")]);

    assert_eq!(contributions.len(), 2);
    assert_eq!(contributions[0].value, 50.0);
    let first_origin = contributions[0].origin.as_ref().unwrap();
    assert_eq!(first_origin.source_id, weapon_affix.source_id);
    assert_eq!(first_origin.raw_text, weapon_affix.raw_text);
    assert_eq!(first_origin.stat_id, Some(ModName::from("MaximumLife")));
    assert_eq!(first_origin.mod_type, Some(ModType::Base));
    assert_eq!(contributions[1].value, 25.0);
    assert_eq!(
        contributions[1].origin.as_ref().unwrap().source_id,
        passive_node.source_id
    );
    assert_eq!(
        contributions.iter().map(|entry| entry.value).sum::<f64>(),
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumLife")])
    );
}

#[test]
fn sum_traced_links_matching_contributions_to_query_node() {
    let weapon_affix = ModifierSource::new(
        SourceId::new(SourceKind::ItemAffix, "weapon.explicit.1").with_label_key("items.weapon"),
    )
    .with_parent(SourceId::new(SourceKind::Item, "weapon"))
    .with_raw_text("+50 to maximum Life");
    let passive_node = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.123"))
        .with_raw_text("+25 to maximum Life while on Full Life");

    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 50.0).with_origin(weapon_affix.clone()),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 25.0)
            .with_origin(passive_node.clone())
            .with_tag(ModTag::Condition {
                var: "FullLife".into(),
                negated: false,
            }),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 99.0)
            .with_origin(ModifierSource::new(SourceId::new(
                SourceKind::Config,
                "condition.low_life",
            )))
            .with_tag(ModTag::Condition {
                var: "LowLife".into(),
                negated: false,
            }),
    );

    let cfg = CalcConfig::new().with_condition("FullLife", true);
    let mut trace = TraceGraph::new();
    let traced = db.sum_traced(
        ModType::Base,
        &cfg,
        &[ModName::from("MaximumLife")],
        &mut trace,
        "MaximumLife Base sum",
    );

    assert_eq!(traced.value, 75.0);
    let query = trace.node(traced.node_id).unwrap();
    assert_eq!(query.label, "MaximumLife Base sum");
    assert_eq!(query.value, 75.0);
    assert_eq!(query.operation, TraceOperation::QuerySum);

    let incoming = trace.incoming(traced.node_id);
    assert_eq!(incoming.len(), 2);
    let sources = incoming
        .iter()
        .map(|node_id| trace.node(*node_id).unwrap().source.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert!(
        sources
            .iter()
            .any(|source| source.kind == SourceKind::ItemAffix)
    );
    assert!(
        sources
            .iter()
            .any(|source| source.kind == SourceKind::PassiveNode)
    );
}
