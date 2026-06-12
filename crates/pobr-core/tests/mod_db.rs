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
    let inner = Modifier::number("DamageTaken", ModType::Inc, 10.0)
        .with_tag(ModTag::condition("Effective", false));
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

// ===========================================================================
// M4-T1 W-A2：写侧原语 ReplaceMod / ConvertMod / ScaleAddMod
// ===========================================================================

/// ReplaceMod 命中（vendor ModDB.lua:38-66）：同 name+type+flags+keywordFlags+source
/// → 原位替换（桶内数量不变、顺序保持）；不同 source → append。
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

    // 命中：同 name+type+flags+kw+source → 值被替换，桶内仍 2 条。
    let replaced = db.replace_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 5.0).with_source("Reload"),
    );
    assert!(replaced);
    assert_eq!(db.sum(ModType::Base, &cfg, &names), 6.0, "5 + 1（Other）");

    // 未命中（source 不同）→ append。
    let replaced = db.replace_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 2.0).with_source("Third"),
    );
    assert!(!replaced);
    assert_eq!(db.sum(ModType::Base, &cfg, &names), 8.0, "5 + 1 + 2");

    // flags 不同也算未命中（vendor 比对含 flags/keywordFlags）。
    let replaced = db.replace_mod(
        Modifier::number("Multiplier:BoltsReloaded", ModType::Base, 4.0)
            .with_source("Reload")
            .with_flags(ModFlags::ATTACK),
    );
    assert!(!replaced);
}

/// ConvertMod 跨桶（vendor ModDB.lua:75-105）：旧名桶移除 + 新名桶落地；
/// 未命中 → 直接 append 新 mod。
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

    // 未命中（source 无匹配）→ append 新 mod（vendor :129-131）。
    let converted = db.convert_mod(
        &ModName::from("ColdDamage"),
        Modifier::number("FireDamage", ModType::Inc, 5.0).with_source("Nowhere"),
    );
    assert!(!converted);
    assert_eq!(db.sum(ModType::Inc, &cfg, &new_names), 35.0);
    assert_eq!(db.sum(ModType::Inc, &cfg, &old_names), 10.0, "旧桶不动");
}

/// ScaleAddMod 取整 oracle 对拍（12 条，蓝图 W-A2 门禁 ≥10 全中）。
///
/// 期望值由 vendor 公式（ModStore.lua:55-80 数值分支 + Common.lua:648 round）
/// 在 luajit 下逐条求得（脚本见 commit message；精度表 = 入库
/// `overlay/high_precision_mods.json`，与 vendor Data.lua:413-530 逐值一致，
/// 有独立加载测试）。LuaJIT 与 Rust 同为 IEEE754 double，浮点逐位可比。
#[test]
fn scale_add_mod_rounding_matches_pob2_oracle() {
    use pobr_core::HighPrecisionRules;
    let cfg = CalcConfig::new();
    // 与 data/4.5.0.3.4/overlay/high_precision_mods.json 一致的最小子集
    // （样本涉及条目；全表等价性由 gamedata 加载测试钉住）。
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

    // (name, type, value, scale, oracle 期望)——luajit 实跑输出，2026-06-12。
    let samples: &[(&str, ModType, f64, f64, f64)] = &[
        // 例外表命中（精度 2/1/4，floor 截位）：
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
        // 负值 floor（向 −∞）：
        (
            "ReservationMultiplier",
            ModType::More,
            -30.0,
            0.6667,
            -20.001,
        ),
        // 未命中 + 小数原值 → defaultHighPrecision = 1：
        ("Damage", ModType::Base, 2.5, 0.5, 1.2),
        // 未命中 + 整数原值 → m_modf(round(·, 2)) 截整：
        ("Damage", ModType::Base, 7.0, 0.5, 3.0),
        // 负值截整向零（m_modf 语义，区别于 floor）：
        ("Damage", ModType::Base, -7.0, 0.5, -3.0),
        ("Damage", ModType::Inc, 33.0, 0.3, 9.0),
        // scale == 1 → 原值直返（含小数也不取整，vendor :54）：
        ("Damage", ModType::Base, 7.77, 1.0, 7.77),
        ("Damage", ModType::Base, 19.0, 1.37, 26.0),
    ];
    for &(name, mod_type, value, scale, expected) in samples {
        let mut db = ModDb::new();
        db.scale_add_mod(Modifier::number(name, mod_type, value), scale, &rules);
        let names = [ModName::from(name)];
        let got = match mod_type {
            ModType::More => {
                // MORE 聚合是 Π(1+v/100)，直接读贡献值对拍缩放结果。
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

/// ScaleAddMod 非数值载荷：Bool/Text 原样入库；NestedMods 逐内层 Number 同规则缩放。
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

/// HighPrecisionRules 默认 fallback（未注入数据）：无例外表，仅默认精度
/// （default_high_precision = 1，与入库 JSON 同值——搬迁不变式）。
#[test]
fn high_precision_rules_default_has_no_exceptions() {
    use pobr_core::HighPrecisionRules;
    let rules = HighPrecisionRules::default();
    assert_eq!(rules.precision_for("CritChance", ModType::Base), None);
    assert_eq!(rules.default_high_precision(), 1);
}

/// MORE 聚合精度例外（M4-T1 W-A2 行为修复，10-G6；vendor ModDB.lua:156-190）。
///
/// 期望值 = vendor MoreInternal 在 luajit 下逐字复刻实跑（脚本见 commit
/// message；样本 S1-S6）。未注入规则（Default）时走默认 round(·,2)——
/// 与例外分支引入前逐字一致（S2 同时是该锚点）。
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

    // S1：例外条目多 mod 连乘 → floor(累计积, 4) = 2.1839（默认 round2 会给 2.18）。
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "SupportManaMultiplier", &[40.0, 30.0, 20.0]);
    let names = [ModName::from("SupportManaMultiplier")];
    assert_eq!(db.more(&cfg, &names), 2.1839, "S1");

    // S2：普通条目同输入 → 默认 round2（= 未注入规则的旧口径锚点）。
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "Damage", &[40.0, 30.0, 20.0]);
    let names = [ModName::from("Damage")];
    assert_eq!(db.more(&cfg, &names), 2.18, "S2");
    let mut plain = ModDb::new(); // Default 规则（未注入）同值。
    more_mods(&mut plain, "Damage", &[40.0, 30.0, 20.0]);
    assert_eq!(plain.more(&cfg, &names), 2.18, "S2 未注入锚点");

    // S3/S4：vendor quirk——modPrecision 跨名持留：例外名在前则后续普通名也
    // floor4（1.4444）；普通名在前则先 round2 再 floor4（1.443）。
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "SupportManaMultiplier", &[30.0]);
    more_mods(&mut db, "Damage", &[11.111]);
    let smm = ModName::from("SupportManaMultiplier");
    let dmg = ModName::from("Damage");
    assert_eq!(db.more(&cfg, &[smm.clone(), dmg.clone()]), 1.4444, "S3");
    assert_eq!(db.more(&cfg, &[dmg, smm]), 1.443, "S4");

    // S5：精度置位后缺桶名（modResult = 1）也被 re-floor。
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "ReservationMultiplier", &[-29.97]);
    let names = [
        ModName::from("ReservationMultiplier"),
        ModName::from("Missing"),
    ];
    assert_eq!(db.more(&cfg, &names), 0.7003, "S5");

    // S6：负 more 例外条目（floor 向 −∞ 截位）。
    let mut db = ModDb::new();
    db.set_high_precision_rules(rules());
    more_mods(&mut db, "ReservationMultiplier", &[-50.0, 33.333]);
    let names = [ModName::from("ReservationMultiplier")];
    assert_eq!(db.more(&cfg, &names), 0.6666, "S6");

    // traced 与非 traced 同口径（共用实现）。
    let mut trace = TraceGraph::new();
    let traced = db.more_traced(&cfg, &names, &mut trace, "more");
    assert_eq!(traced.value, 0.6666, "traced 同值");
}
