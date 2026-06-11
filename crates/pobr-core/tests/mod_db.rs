use pobr_core::{
    CalcConfig, ModDb, ModList, ModTag, ModValue, Modifier, TraceGraph, TraceOperation,
};
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
            div: 1.0,
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

// ---------------------------------------------------------------------------
// 01-02：ModFlags 子集匹配语义（PoB2 ModList.lua `band(cfg.flags, mod.flags) == mod.flags`）
// ---------------------------------------------------------------------------

#[test]
fn mod_flags_match_requires_subset_not_intersection() {
    // mod.flags = ATTACK|PROJECTILE：只有当 cfg.flags 是其超集时才命中。
    let m = Modifier::number("Damage", ModType::Inc, 10.0)
        .with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE);

    // cfg = ATTACK 单独：band(ATTACK, ATTACK|PROJECTILE)=ATTACK ≠ mod.flags → 拒绝（子集语义）。
    let cfg_attack_only = CalcConfig::new().with_flags(ModFlags::ATTACK);
    assert!(
        !m.matches(&cfg_attack_only),
        "纯 Attack（非投射）不应命中 Attack|Projectile mod"
    );

    // cfg = ATTACK|PROJECTILE：等于 → 命中。
    let cfg_ap = CalcConfig::new().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE);
    assert!(m.matches(&cfg_ap), "Attack|Projectile cfg 命中");

    // cfg = ATTACK|PROJECTILE|MELEE：超集 → 命中。
    let cfg_super =
        CalcConfig::new().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::MELEE);
    assert!(m.matches(&cfg_super), "超集 cfg 命中");

    // 单 flag mod：子集与 intersects 等价，行为不变。
    let single = Modifier::number("Damage", ModType::Inc, 10.0).with_flags(ModFlags::ATTACK);
    assert!(single.matches(&cfg_attack_only));
    assert!(!single.matches(&CalcConfig::new().with_flags(ModFlags::SPELL)));

    // 空 flag mod：对任意 cfg 恒命中（NONE 是任意集合子集）。
    let no_flags = Modifier::number("Damage", ModType::Inc, 10.0);
    assert!(no_flags.matches(&CalcConfig::new()));
    assert!(no_flags.matches(&cfg_attack_only));
}

// ---------------------------------------------------------------------------
// 01-01：MORE 逐 modName round(modResult, 2)（PoB2 ModList.lua MoreInternal）
// ---------------------------------------------------------------------------

#[test]
fn more_rounds_per_name_product_to_two_decimals() {
    // 同名两条 MORE：10% + 13% → 1.10*1.13 = 1.243 → round2 = 1.24（非 1.243）。
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("Damage", ModType::More, 10.0));
    db.add_mod(Modifier::number("Damage", ModType::More, 13.0));
    let cfg = CalcConfig::new();
    let m = db.more(&cfg, &[ModName::from("Damage")]);
    assert!(
        (m - 1.24).abs() < 1e-9,
        "同名 more 逐名 round2 → 1.24, got {m}"
    );
}

#[test]
fn more_rounds_each_name_separately_then_multiplies() {
    // 两个不同名各两条 MORE：Damage 10%+13% → round2(1.243)=1.24；
    // AttackDamage 7%+7% → round2(1.1449)=1.14。总 = 1.24 * 1.14 = 1.4136。
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("Damage", ModType::More, 10.0));
    db.add_mod(Modifier::number("Damage", ModType::More, 13.0));
    db.add_mod(Modifier::number("AttackDamage", ModType::More, 7.0));
    db.add_mod(Modifier::number("AttackDamage", ModType::More, 7.0));
    let cfg = CalcConfig::new();
    let m = db.more(
        &cfg,
        &[ModName::from("Damage"), ModName::from("AttackDamage")],
    );
    assert!(
        (m - 1.4136).abs() < 1e-9,
        "逐名 round2 后跨名连乘 → 1.4136, got {m}"
    );
}

/// 01-05：ModDb::get_multiplier 对齐 PoB2 GetMultiplier（Override 优先，否则 cfg + Sum(BASE, Multiplier:var)）。
#[test]
fn get_multiplier_matches_pob2_getmultiplier_semantics() {
    // (a) 纯 cfg.multipliers。
    let db_empty = ModDb::new();
    let cfg_a = CalcConfig::attack().with_multiplier("Rage", 30.0);
    assert_eq!(db_empty.get_multiplier("Rage", &cfg_a), 30.0);

    // (b) modDB Multiplier:X BASE 累加进 cfg 基线（0 + 7 + 5 = 12）。
    let mut db_base = ModDb::new();
    db_base.add_mod(Modifier::number("Multiplier:Virulence", ModType::Base, 7.0));
    db_base.add_mod(Modifier::number("Multiplier:Virulence", ModType::Base, 5.0));
    let cfg_b = CalcConfig::attack();
    assert_eq!(db_base.get_multiplier("Virulence", &cfg_b), 12.0);
    // 名称用 "Multiplier:"+var（裸 var 不命中）。
    assert_eq!(
        db_base.sum(ModType::Base, &cfg_b, &[ModName::from("Virulence")]),
        0.0
    );

    // (c) Override 短路覆盖。
    let mut db_ovr = ModDb::new();
    db_ovr.add_mod(Modifier::number("Multiplier:Virulence", ModType::Base, 7.0));
    db_ovr.add_mod(Modifier::number(
        "Multiplier:Virulence",
        ModType::Override,
        99.0,
    ));
    let cfg_c = CalcConfig::attack().with_multiplier("Virulence", 50.0);
    assert_eq!(db_ovr.get_multiplier("Virulence", &cfg_c), 99.0);
}

#[test]
fn list_nested_passes_through_nested_mods_without_evaluating() {
    // M3 C4-1：`EnemyModifier` 类嵌套 LIST 载荷——`list_nested` 只透传内层 mods，
    // 不参与数值聚合；文本 List 通道（`list`）对嵌套载荷保持不可见。
    let mut db = ModDb::new();
    let inner = Modifier::number("DamageTaken", ModType::Inc, 10.0).with_tag(ModTag::Condition {
        var: "Effective".to_string(),
        negated: false,
    });
    db.add_mod(Modifier::new(
        "EnemyModifier",
        ModType::List,
        ModValue::NestedMods(vec![inner.clone()]),
    ));
    // 同名文本 List 条目：两条通道互不串扰。
    db.add_mod(Modifier::text(
        "EnemyModifier",
        ModType::List,
        "placeholder",
    ));

    let cfg = CalcConfig::new();
    let name = ModName::from("EnemyModifier");

    assert_eq!(db.list_nested(&cfg, name.clone()), vec![inner]);
    assert_eq!(db.list(&cfg, name.clone()), vec!["placeholder".to_string()]);
    // 嵌套载荷不进入任何标量聚合通道。
    assert_eq!(
        db.sum(ModType::List, &cfg, std::slice::from_ref(&name)),
        0.0
    );
    assert!(!db.flag(&cfg, name));
}

#[test]
fn nested_mods_value_has_no_scalar_views() {
    let value = ModValue::NestedMods(vec![Modifier::flag("Onslaught")]);
    assert_eq!(value.as_number(), None);
    assert_eq!(value.as_bool(), None);
    assert_eq!(value.as_text(), None);
    assert_eq!(value.as_nested_mods().map(<[Modifier]>::len), Some(1));
}
