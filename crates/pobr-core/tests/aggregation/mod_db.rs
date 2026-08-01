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
        Modifier::number("PhysicalDamage", ModType::Inc, 15.0)
            .with_tag(ModTag::condition("OnFullLife", false)),
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
        Modifier::number("MaximumLife", ModType::Base, 8.0).with_tag(ModTag::multiplier(
            "AllocatedSmallPassives",
            1.0,
            Some(5.0),
        )),
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
        Modifier::number("AttackDamage", ModType::Inc, 30.0)
            .with_tag(ModTag::condition("OnFullLife", true)),
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
            .with_tag(ModTag::condition("FullLife", false)),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 99.0)
            .with_origin(inactive_config)
            .with_tag(ModTag::condition("LowLife", false)),
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
            .with_tag(ModTag::condition("FullLife", false)),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 99.0)
            .with_origin(ModifierSource::new(SourceId::new(
                SourceKind::Config,
                "condition.low_life",
            )))
            .with_tag(ModTag::condition("LowLife", false)),
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

// ModFlags matching is subset, not intersection (PoB2 ModList.lua `band(cfg.flags, mod.flags) == mod.flags`).

#[test]
fn mod_flags_match_requires_subset_not_intersection() {
    // mod.flags = ATTACK|PROJECTILE: only matches when cfg.flags is a superset.
    let m = Modifier::number("Damage", ModType::Inc, 10.0)
        .with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE);

    // cfg = ATTACK alone: band(ATTACK, ATTACK|PROJECTILE)=ATTACK != mod.flags → rejected (subset semantics).
    let cfg_attack_only = CalcConfig::new().with_flags(ModFlags::ATTACK);
    assert!(
        !m.matches(&cfg_attack_only),
        "纯 Attack（非投射）不应命中 Attack|Projectile mod"
    );

    // cfg = ATTACK|PROJECTILE: equal → matches.
    let cfg_ap = CalcConfig::new().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE);
    assert!(m.matches(&cfg_ap), "Attack|Projectile cfg 命中");

    // cfg = ATTACK|PROJECTILE|MELEE: superset → matches.
    let cfg_super =
        CalcConfig::new().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::MELEE);
    assert!(m.matches(&cfg_super), "超集 cfg 命中");

    // Single-flag mod: subset and intersects are equivalent here, behaviour unchanged.
    let single = Modifier::number("Damage", ModType::Inc, 10.0).with_flags(ModFlags::ATTACK);
    assert!(single.matches(&cfg_attack_only));
    assert!(!single.matches(&CalcConfig::new().with_flags(ModFlags::SPELL)));

    // Empty-flag mod: matches any cfg (NONE is a subset of every set).
    let no_flags = Modifier::number("Damage", ModType::Inc, 10.0);
    assert!(no_flags.matches(&CalcConfig::new()));
    assert!(no_flags.matches(&cfg_attack_only));
}

// MORE aggregation rounds per modName to 2 decimals (PoB2 ModList.lua MoreInternal).

#[test]
fn more_rounds_per_name_product_to_two_decimals() {
    // Two MORE mods with the same name: 10% + 13% → 1.10*1.13 = 1.243 → round2 = 1.24 (not 1.243).
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
    // Two MORE mods each on two different names: Damage 10%+13% → round2(1.243)=1.24;
    // AttackDamage 7%+7% → round2(1.1449)=1.14. Total = 1.24 * 1.14 = 1.4136.
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

/// `ModDb::get_multiplier` matches PoB2 `GetMultiplier` (Override wins, else cfg + Sum(BASE, Multiplier:var)).
#[test]
fn get_multiplier_matches_pob2_getmultiplier_semantics() {
    // (a) cfg.multipliers only.
    let db_empty = ModDb::new();
    let cfg_a = CalcConfig::attack().with_multiplier("Rage", 30.0);
    assert_eq!(db_empty.get_multiplier("Rage", &cfg_a), 30.0);

    // (b) modDB Multiplier:X BASE mods accumulate onto the cfg baseline (0 + 7 + 5 = 12).
    let mut db_base = ModDb::new();
    db_base.add_mod(Modifier::number("Multiplier:Virulence", ModType::Base, 7.0));
    db_base.add_mod(Modifier::number("Multiplier:Virulence", ModType::Base, 5.0));
    let cfg_b = CalcConfig::attack();
    assert_eq!(db_base.get_multiplier("Virulence", &cfg_b), 12.0);
    // The mod name is "Multiplier:"+var; the bare var name does not match.
    assert_eq!(
        db_base.sum(ModType::Base, &cfg_b, &[ModName::from("Virulence")]),
        0.0
    );

    // (c) Override short-circuits.
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
    // `EnemyModifier`-style nested LIST payloads: `list_nested` only passes inner mods
    // through without evaluating them; the text List channel (`list`) stays blind to nested payloads.
    let mut db = ModDb::new();
    let inner = Modifier::number("DamageTaken", ModType::Inc, 10.0)
        .with_tag(ModTag::condition("Effective", false));
    db.add_mod(Modifier::new(
        "EnemyModifier",
        ModType::List,
        ModValue::NestedMods(vec![inner.clone()]),
    ));
    // Same-name text List entry: the two channels must not cross-contaminate.
    db.add_mod(Modifier::text(
        "EnemyModifier",
        ModType::List,
        "placeholder",
    ));

    let cfg = CalcConfig::new();
    let name = ModName::from("EnemyModifier");

    assert_eq!(db.list_nested(&cfg, name.clone()), vec![inner]);
    assert_eq!(db.list(&cfg, name.clone()), vec!["placeholder".to_string()]);
    // Nested payloads never enter any scalar aggregation channel.
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

// Write-side primitives ReplaceMod / ConvertMod / ScaleAddMod.

/// ReplaceMod hit (vendor ModDB.lua:38-66): same name+type+flags+keywordFlags+source
/// → replaced in place (bucket count unchanged, order preserved); different source → append.
#[test]
fn replace_mod_replaces_on_full_param_match_else_appends() {
    let cfg = CalcConfig::new();
    let names = [ModName::from("Multiplier:BoltsReloaded")];
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 3.0).with_source("Reload"),
    );
    db.add_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 1.0).with_source("Other"),
    );

    // Hit: same name+type+flags+kw+source → value replaced, bucket still holds 2 entries.
    let replaced = db.replace_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 5.0).with_source("Reload"),
    );
    assert!(replaced);
    assert_eq!(db.sum(ModType::Base, &cfg, &names), 6.0, "5 + 1（Other）");

    // Miss (different source) → append.
    let replaced = db.replace_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 2.0).with_source("Third"),
    );
    assert!(!replaced);
    assert_eq!(db.sum(ModType::Base, &cfg, &names), 8.0, "5 + 1 + 2");

    // Different flags also count as a miss (vendor comparison includes flags/keywordFlags).
    let replaced = db.replace_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 4.0)
            .with_source("Reload")
            .with_flags(ModFlags::ATTACK),
    );
    assert!(!replaced);
}

/// ConvertMod moves a mod between buckets (vendor ModDB.lua:75-105): removed from the
/// old-name bucket and landed in the new-name bucket; on a miss it just appends the new mod.
#[test]
fn convert_mod_moves_between_buckets() {
    let cfg = CalcConfig::new();
    let old_names = [ModName::from("ColdDamage")];
    let new_names = [ModName::from("FireDamage")];
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ColdDamage", ModType::Inc, 30.0).with_source("Tree"));
    db.add_mod(Modifier::number("ColdDamage", ModType::Inc, 10.0).with_source("Gear"));

    let converted = db.convert_mod(
        &ModName::from("ColdDamage"),
        Modifier::number("FireDamage", ModType::Inc, 30.0).with_source("Tree"),
    );
    assert!(converted);
    assert_eq!(
        db.sum(ModType::Inc, &cfg, &old_names),
        10.0,
        "Tree 条已移走"
    );
    assert_eq!(db.sum(ModType::Inc, &cfg, &new_names), 30.0, "落入新桶");

    // Miss (no matching source) → append the new mod (vendor :129-131).
    let converted = db.convert_mod(
        &ModName::from("ColdDamage"),
        Modifier::number("FireDamage", ModType::Inc, 5.0).with_source("Nowhere"),
    );
    assert!(!converted);
    assert_eq!(db.sum(ModType::Inc, &cfg, &new_names), 35.0);
    assert_eq!(db.sum(ModType::Inc, &cfg, &old_names), 10.0, "旧桶不动");
}

/// ScaleAddMod rounding oracle diff (12 samples, gate requires ≥10 to match).
///
/// Expected values were computed sample-by-sample under luajit from the vendor formula
/// (ModStore.lua:55-80 numeric branch + Common.lua:648 round; script in the commit message).
/// The precision table = the ingested `overlay/high_precision_mods.json`, which matches
/// vendor Data.lua:413-530 value-for-value (has its own load test). LuaJIT and Rust are both
/// IEEE754 double, so floating-point results are bit-comparable.
#[test]
fn scale_add_mod_rounding_matches_pob2_oracle() {
    use pobr_core::HighPrecisionRules;
    let cfg = CalcConfig::new();
    // Minimal subset consistent with data/4.5.0.3.4/overlay/high_precision_mods.json
    // (only the entries the samples touch; full-table equivalence is pinned by the gamedata load test).
    let mut mods = std::collections::BTreeMap::new();
    for (name, mod_type, p) in [
        ("CritChance", "BASE", 2u32),
        ("LifeRegen", "BASE", 1),
        ("LifeRegenPercent", "BASE", 2),
        ("SupportManaMultiplier", "MORE", 4),
        ("ReservationMultiplier", "MORE", 4),
    ] {
        mods.entry(name.to_string())
            .or_insert_with(std::collections::BTreeMap::new)
            .insert(mod_type.to_string(), p);
    }
    let rules = HighPrecisionRules::from_def(
        pobr_data::catalog::high_precision_mods::HighPrecisionModsDef {
            default_high_precision: 1,
            more_default_round_decimals: 2,
            mods,
        },
    );

    // (name, type, value, scale, oracle expected) — real luajit output, 2026-06-12.
    let samples: &[(&str, ModType, f64, f64, f64)] = &[
        // Exception-table hit (precision 2/1/4, floor truncation):
        ("CritChance", ModType::Base, 7.0, 0.5, 3.5),
        ("CritChance", ModType::Base, 7.77, 1.2, 9.32),
        ("LifeRegen", ModType::Base, 2.5, 0.3, 0.7),
        ("LifeRegenPercent", ModType::Base, 0.25, 0.6667, 0.16),
        (
            "SupportManaMultiplier",
            ModType::More,
            25.0,
            1.1111,
            27.7775,
        ),
        // Negative value floors toward -infinity:
        (
            "ReservationMultiplier",
            ModType::More,
            -30.0,
            0.6667,
            -20.001,
        ),
        // Miss + fractional original value → defaultHighPrecision = 1:
        ("Damage", ModType::Base, 2.5, 0.5, 1.2),
        // Miss + integer original value → m_modf(round(·, 2)) truncates to integer:
        ("Damage", ModType::Base, 7.0, 0.5, 3.0),
        // Negative value truncates toward zero (m_modf semantics, distinct from floor):
        ("Damage", ModType::Base, -7.0, 0.5, -3.0),
        ("Damage", ModType::Inc, 33.0, 0.3, 9.0),
        // scale == 1 → original value returned as-is (not rounded even if fractional, vendor :54):
        ("Damage", ModType::Base, 7.77, 1.0, 7.77),
        ("Damage", ModType::Base, 19.0, 1.37, 26.0),
    ];
    for &(name, mod_type, value, scale, expected) in samples {
        let mut db = ModDb::new();
        db.scale_add_mod(Modifier::number(name, mod_type, value), scale, &rules);
        let names = [ModName::from(name)];
        let got = match mod_type {
            ModType::More => {
                // MORE aggregation is Π(1+v/100); read the contribution value directly to diff the scaled result.
                db.contributions(ModType::More, &cfg, &names)[0].value
            }
            t => db.sum(t, &cfg, &names),
        };
        assert_eq!(
            got, expected,
            "ScaleAddMod({name}, {mod_type:?}, {value}, ×{scale}) oracle 不中"
        );
    }
}

/// ScaleAddMod on non-numeric payloads: Bool/Text pass through unchanged; NestedMods scales
/// each inner Number by the same rules.
#[test]
fn scale_add_mod_non_number_payloads() {
    use pobr_core::HighPrecisionRules;
    let cfg = CalcConfig::new();
    let rules = HighPrecisionRules::default();
    let mut db = ModDb::new();

    db.scale_add_mod(Modifier::flag("SomeFlag"), 0.5, &rules);
    assert!(db.flag(&cfg, ModName::from("SomeFlag")), "Bool 载荷不缩放");

    db.scale_add_mod(
        Modifier::new(
            "EnemyModifier",
            ModType::List,
            ModValue::NestedMods(vec![Modifier::number("Damage", ModType::Base, 7.0)]),
        ),
        0.5,
        &rules,
    );
    let nested = db.list_nested(&cfg, ModName::from("EnemyModifier"));
    assert_eq!(
        nested[0].value,
        ModValue::Number(3.0),
        "嵌套 Number 同规则缩放（7 × 0.5 → round2 → 截整 = 3）"
    );
}

/// HighPrecisionRules default fallback (no data injected): no exception table, just the
/// default precision (default_high_precision = 1, matching the ingested JSON — a migration invariant).
#[test]
fn high_precision_rules_default_has_no_exceptions() {
    use pobr_core::HighPrecisionRules;
    let rules = HighPrecisionRules::default();
    assert_eq!(rules.precision_for("CritChance", ModType::Base), None);
    assert_eq!(rules.default_high_precision(), 1);
}

/// MORE aggregation precision exceptions (vendor `ModDB.lua:156-190`).
///
/// Expected values = a literal port of vendor MoreInternal run under luajit (script in the
/// commit message; samples S1-S6). With no rules injected (Default), it falls back to the
/// default round(·,2) — identical to the behaviour before the exception branch existed
/// (S2 doubles as that anchor).
#[test]
fn more_precision_exception_matches_more_internal_oracle() {
    use pobr_core::HighPrecisionRules;
    let cfg = CalcConfig::new();
    let rules = || {
        let mut mods = std::collections::BTreeMap::new();
        for name in ["SupportManaMultiplier", "ReservationMultiplier"] {
            mods.insert(
                name.to_string(),
                std::collections::BTreeMap::from([("MORE".to_string(), 4u32)]),
            );
        }
        HighPrecisionRules::from_def(
            pobr_data::catalog::high_precision_mods::HighPrecisionModsDef {
                default_high_precision: 1,
                more_default_round_decimals: 2,
                mods,
            },
        )
    };
    let more_mods = |db: &mut ModDb, name: &str, values: &[f64]| {
        for &v in values {
            db.add_mod(Modifier::number(name, ModType::More, v));
        }
    };

    // S1: exception-entry with multiple mods chained → floor(running product, 4) = 2.1839
    // (default round2 would give 2.18).
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "SupportManaMultiplier", &[40.0, 30.0, 20.0]);
    let names = [ModName::from("SupportManaMultiplier")];
    assert_eq!(db.more(&cfg, &names), 2.1839, "S1");

    // S2: a plain entry with the same inputs → default round2 (the anchor for the old
    // behaviour with no rules injected).
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "Damage", &[40.0, 30.0, 20.0]);
    let names = [ModName::from("Damage")];
    assert_eq!(db.more(&cfg, &names), 2.18, "S2");
    let mut plain = ModDb::new(); // Default rules (not injected) give the same value.
    more_mods(&mut plain, "Damage", &[40.0, 30.0, 20.0]);
    assert_eq!(plain.more(&cfg, &names), 2.18, "S2 未注入锚点");

    // S3/S4: vendor quirk — modPrecision persists across names. If the exception name comes
    // first, subsequent plain names also floor4 (1.4444); if the plain name comes first, it
    // round2's then floor4's (1.443).
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "SupportManaMultiplier", &[30.0]);
    more_mods(&mut db, "Damage", &[11.111]);
    let smm = ModName::from("SupportManaMultiplier");
    let dmg = ModName::from("Damage");
    assert_eq!(db.more(&cfg, &[smm.clone(), dmg.clone()]), 1.4444, "S3");
    assert_eq!(db.more(&cfg, &[dmg, smm]), 1.443, "S4");

    // S5: once precision is latched, even a missing bucket name (modResult = 1) gets re-floored.
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "ReservationMultiplier", &[-29.97]);
    let names = [
        ModName::from("ReservationMultiplier"),
        ModName::from("Missing"),
    ];
    assert_eq!(db.more(&cfg, &names), 0.7003, "S5");

    // S6: negative MORE on an exception entry (floors toward -infinity).
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "ReservationMultiplier", &[-50.0, 33.333]);
    let names = [ModName::from("ReservationMultiplier")];
    assert_eq!(db.more(&cfg, &names), 0.6666, "S6");

    // traced and non-traced share the same behaviour (common implementation).
    let mut trace = TraceGraph::new();
    let traced = db.more_traced(&cfg, &names, &mut trace, "more");
    assert_eq!(traced.value, 0.6666, "traced 同值");
}

// Second EvalMod-tag batch: PerStat reads output, GlobalLimit does cumulative clamping.

/// PerStat reads the actor output snapshot (vendor ModStore.lua:440-489 PerStat branch +
/// :280-325 GetStat), via EvalContext::stat_lookup; with no snapshot it's conservatively 0.
/// End-to-end for the `per 100 maximum Life` shape (mod construction → aggregation query).
#[test]
fn per_stat_reads_output_snapshot_via_eval_context() {
    use pobr_core::EvalContext;
    let mut db = ModDb::new();
    // "1% increased Damage per 100 maximum Life" shape.
    db.add_mod(
        Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::PerStat {
            stat: "Life".into(),
            div: 100.0,
            limit: None,
            limit_var: None,
            actor: None,
        }),
    );
    let cfg = CalcConfig::new();
    let names = [ModName::from("Damage")];

    // No output snapshot (the legacy call shape, &cfg passed directly) → stat = 0 → multiplier 0.
    assert_eq!(db.sum(ModType::Inc, &cfg, &names), 0.0);

    // With an output snapshot: Life = 5430 → floor(5430/100 + eps) = 54 → 1 × 54.
    let lookup = |stat: &str| (stat == "Life").then_some(5430.0);
    let ctx = EvalContext::with_stat_lookup(&cfg, &lookup);
    assert_eq!(db.sum(ModType::Inc, ctx, &names), 54.0);
}

/// When EvalContext::stat has no lookup, it falls back to the cfg.stats snapshot (the
/// production wiring: orchestration stage 6c backfills cfg.stats, so an internal
/// `EvalContext::new(cfg)` query picks it up — PerStat/PercentStat no longer require the
/// caller to explicitly build a stat_lookup). A lookup hit still takes priority.
#[test]
fn eval_context_stat_falls_back_to_cfg_stats_snapshot() {
    use pobr_core::EvalContext;
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::PerStat {
            stat: "Life".into(),
            div: 100.0,
            limit: None,
            limit_var: None,
            actor: None,
        }),
    );
    let names = [ModName::from("Damage")];

    // cfg.stats snapshot only (&cfg passed directly → EvalContext::new, no lookup) → falls back and finds it.
    let cfg = CalcConfig::new().with_stat("Life", 5430.0);
    assert_eq!(db.sum(ModType::Inc, &cfg, &names), 54.0);

    // lookup takes priority over the snapshot (lookup Life=200 overrides snapshot 5430).
    let lookup = |stat: &str| (stat == "Life").then_some(200.0);
    let ctx = EvalContext::with_stat_lookup(&cfg, &lookup);
    assert_eq!(db.sum(ModType::Inc, ctx, &names), 2.0);
}

/// PercentStat scales by a percentage of an already-computed stat (V2 slice 2; vendor
/// ModStore.lua:506-555): `value = ceil(value × stat × percent/100)` — ceil applies to the
/// final contribution (unlike PerStat, where floor applies to the multiplier); no snapshot
/// → stat=0 → contribution 0 (conservative); missing percent → mult=stat.
#[test]
fn percent_stat_scales_by_stat_percentage_with_ceil() {
    use pobr_core::EvalContext;
    let cfg = CalcConfig::new();
    // "gain Accuracy equal to 40% of Dexterity" shape: value=1 × Dex×40%.
    let m = Modifier::number("Accuracy", ModType::Base, 1.0).with_tag(ModTag::PercentStat {
        stat: "Dex".into(),
        percent: Some(40.0),
    });

    // No output snapshot → stat=0 → ceil(0)=0.
    assert_eq!(m.effective_number(&cfg), Some(0.0));

    // Dex=333 → 1 × 333×0.4 = 133.2 → ceil = 134 (vendor m_ceil applies to the final value).
    let lookup = |stat: &str| (stat == "Dex").then_some(333.0);
    let ctx = EvalContext::with_stat_lookup(&cfg, &lookup);
    assert_eq!(m.effective_number(ctx), Some(134.0));

    // Missing percent (the or-1 side of vendor `(percent and percent/100 or 1)`) → mult=stat.
    let plain = Modifier::number("X", ModType::Base, 2.0).with_tag(ModTag::PercentStat {
        stat: "Dex".into(),
        percent: None,
    });
    let ctx = EvalContext::with_stat_lookup(&cfg, &lookup);
    assert_eq!(plain.effective_number(ctx), Some(666.0));
}

/// StatThreshold is a binary gate (V2s4; vendor ModStore.lua:556-573): it reads the cfg.stats
/// snapshot during matches — the FLAG query path is gated the same way (the gate lives in
/// matches, not at evaluation time, which is the key difference from PerStat's evaluation-time
/// consumption, though it's structurally the same as MultiplierThreshold).
#[test]
fn stat_threshold_gates_in_matches_for_all_query_paths() {
    let mut db = ModDb::new();
    // "cannot be stunned if you have at least 5 crab barriers" shape (FLAG).
    db.add_mod(
        Modifier::flag("StunImmune").with_tag(ModTag::StatThreshold {
            stat: "CrabBarriers".into(),
            threshold: 5.0,
            upper: false,
        }),
    );
    // Numeric path: "30% more damage while energy shield is at most 100" (upper).
    db.add_mod(
        Modifier::number("Damage", ModType::More, 30.0).with_tag(ModTag::StatThreshold {
            stat: "EnergyShield".into(),
            threshold: 100.0,
            upper: true,
        }),
    );
    let names = [ModName::from("Damage")];

    // No snapshot (missing key = 0): lower gate is closed (0 < 5), upper gate is open (0 <= 100) —
    // matches vendor value-for-value when output is missing the stat (GetStat=0).
    let cfg = CalcConfig::new();
    assert!(!db.flag(&cfg, ModName::from("StunImmune")));
    assert_eq!(db.more(&cfg, &names), 1.3);

    // Snapshot crosses the threshold: lower opens, upper closes.
    let cfg = CalcConfig::new()
        .with_stat("CrabBarriers", 5.0)
        .with_stat("EnergyShield", 250.0);
    assert!(db.flag(&cfg, ModName::from("StunImmune")));
    assert_eq!(db.more(&cfg, &names), 1.0);
}

/// PerStat's limit / limit_var / actor dimensions (unified with the Multiplier shape).
#[test]
fn per_stat_applies_limits_and_actor_dimension() {
    use pobr_core::{ActorRef, EvalContext};
    let cfg = CalcConfig::new()
        .with_multiplier("MaxBonus", 30.0)
        .with_actor_multiplier(ActorRef::Minion, "EnergyShield", 800.0);
    let lookup = |stat: &str| (stat == "EnergyShield").then_some(1250.0);
    let ctx = EvalContext::with_stat_lookup(&cfg, &lookup);

    // Static limit takes priority (vendor :461-468 tag.limit).
    let limited = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::PerStat {
        stat: "EnergyShield".into(),
        div: 25.0,
        limit: Some(40.0),
        limit_var: None,
        actor: None,
    });
    assert_eq!(limited.effective_number(ctx), Some(40.0), "50 → min(·,40)");

    // Dynamic limit_var → cfg.multiplier (vendor :462 GetMultiplier(self, limitVar)).
    let dyn_limited = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::PerStat {
        stat: "EnergyShield".into(),
        div: 25.0,
        limit: None,
        limit_var: Some("MaxBonus".into()),
        actor: None,
    });
    assert_eq!(dyn_limited.effective_number(ctx), Some(30.0));

    // actor dimension: reads the actor_multipliers snapshot (unified with the Multiplier
    // actor channel), not this actor's own output lookup.
    let cross = Modifier::number("Damage", ModType::Inc, 1.0).with_tag(ModTag::PerStat {
        stat: "EnergyShield".into(),
        div: 100.0,
        limit: None,
        limit_var: None,
        actor: Some(ActorRef::Minion),
    });
    assert_eq!(cross.effective_number(ctx), Some(8.0), "800/100 = 8");
}

/// GlobalLimit does cumulative clamping (vendor ModStore.lua:895-905): two mods sharing a key
/// are capped cumulatively within a single aggregation; separate queries account independently
/// (vendor allocates a fresh globalLimits table on every Sum).
#[test]
fn global_limit_accumulates_and_truncates_within_one_query() {
    let cfg = CalcConfig::new();
    let names = [ModName::from("DoubleDamageChance")];
    let tag = || ModTag::GlobalLimit {
        value: 50.0,
        key: "DoubleDamage".into(),
    };
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DoubleDamageChance", ModType::Base, 30.0).with_tag(tag()));
    db.add_mod(Modifier::number("DoubleDamageChance", ModType::Base, 35.0).with_tag(tag()));

    // 30 + min(35, 50-30) = 30 + 20 = 50.
    assert_eq!(db.sum(ModType::Base, &cfg, &names), 50.0);
    // The second query accounts independently (no accumulation across queries).
    assert_eq!(db.sum(ModType::Base, &cfg, &names), 50.0);

    // Contribution view: the second entry has clamped_from = Some(35), truncated to 20; Σ == sum().
    let contributions = db.contributions(ModType::Base, &cfg, &names);
    assert_eq!(contributions[0].value, 30.0);
    assert_eq!(contributions[0].clamped_from, None);
    assert_eq!(contributions[1].value, 20.0);
    assert_eq!(contributions[1].clamped_from, Some(35.0));

    // Different keys don't share accounting.
    let mut db2 = ModDb::new();
    db2.add_mod(Modifier::number("DoubleDamageChance", ModType::Base, 30.0).with_tag(tag()));
    db2.add_mod(
        Modifier::number("DoubleDamageChance", ModType::Base, 35.0).with_tag(ModTag::GlobalLimit {
            value: 50.0,
            key: "Other".into(),
        }),
    );
    assert_eq!(db2.sum(ModType::Base, &cfg, &names), 65.0);
}

/// GlobalLimit also accounts within MORE aggregation (vendor ModDB.lua:159-169 MoreInternal
/// passes globalLimits; the clamp applies to the percentage value, before it's folded into
/// the multiplier).
#[test]
fn global_limit_applies_to_more_aggregation() {
    let cfg = CalcConfig::new();
    let names = [ModName::from("SomeMore")];
    let tag = || ModTag::GlobalLimit {
        value: 30.0,
        key: "MoreCap".into(),
    };
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("SomeMore", ModType::More, 20.0).with_tag(tag()));
    db.add_mod(Modifier::number("SomeMore", ModType::More, 25.0).with_tag(tag()));
    // 20 in full + min(25, 30-20)=10 → 1.20 × 1.10 = 1.32 (round2 unchanged).
    assert_eq!(db.more(&cfg, &names), 1.32);
}

/// GlobalLimit's traced path: a clamped contribution enters the graph through a Clamp node
/// (the source node carries the original value, the Clamp node the actual counted value);
/// an unclamped contribution connects directly (the clamp is explicit in the attribution graph).
#[test]
fn global_limit_traced_inserts_clamp_node() {
    let cfg = CalcConfig::new();
    let names = [ModName::from("DoubleDamageChance")];
    let tag = || ModTag::GlobalLimit {
        value: 50.0,
        key: "DoubleDamage".into(),
    };
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DoubleDamageChance", ModType::Base, 30.0).with_tag(tag()));
    db.add_mod(Modifier::number("DoubleDamageChance", ModType::Base, 35.0).with_tag(tag()));

    let mut trace = TraceGraph::new();
    let traced = db.sum_traced(ModType::Base, &cfg, &names, &mut trace, "ddc");
    assert_eq!(traced.value, 50.0, "traced 与非 traced 同值");

    let clamp_nodes: Vec<_> = trace
        .nodes()
        .iter()
        .filter(|n| n.operation == TraceOperation::Clamp)
        .collect();
    assert_eq!(clamp_nodes.len(), 1, "仅被截断的贡献挂 Clamp 节点");
    assert_eq!(clamp_nodes[0].value, 20.0, "Clamp 节点值 = 实际计入值");
}
