use pobr_core::mod_cache::ModCache;
use pobr_core::mod_parser::{ParseOutcome, ParseStatus, parse_mod};
use pobr_core::{ModTag, ModValue};
use pobr_data::prelude::*;

#[test]
fn parses_increased_damage_with_condition_tag() {
    let outcome = parse_mod("20% increased Fire Damage while on Full Life").unwrap();

    assert_eq!(outcome.status, ParseStatus::Parsed);
    assert!(outcome.unparsed.is_none());
    assert_eq!(outcome.mods.len(), 1);

    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("FireDamage"));
    assert_eq!(modifier.mod_type, ModType::Inc);
    assert_eq!(modifier.value, ModValue::Number(20.0));
    assert!(modifier.tags.contains(&ModTag::Condition {
        var: "FullLife".into(),
        negated: false,
    }));
}

#[test]
fn parses_reduced_as_negative_increased() {
    let outcome = parse_mod("15% reduced Attack Speed").unwrap();
    let modifier = &outcome.mods[0];

    assert_eq!(modifier.name, ModName::from("AttackSpeed"));
    assert_eq!(modifier.mod_type, ModType::Inc);
    assert_eq!(modifier.value, ModValue::Number(-15.0));
}

#[test]
fn parses_less_as_negative_more() {
    let outcome = parse_mod("10% less Physical Damage").unwrap();
    let modifier = &outcome.mods[0];

    assert_eq!(modifier.name, ModName::from("PhysicalDamage"));
    assert_eq!(modifier.mod_type, ModType::More);
    assert_eq!(modifier.value, ModValue::Number(-10.0));
}

#[test]
fn parses_flat_life_and_resistance_values() {
    let life = parse_mod("+50 to maximum Life").unwrap();
    let fire_res = parse_mod("+35% to Fire Resistance").unwrap();

    assert_eq!(life.mods[0].name, ModName::from("MaximumLife"));
    assert_eq!(life.mods[0].mod_type, ModType::Base);
    assert_eq!(life.mods[0].value, ModValue::Number(50.0));
    assert_eq!(fire_res.mods[0].name, ModName::from("FireResistance"));
    assert_eq!(fire_res.mods[0].value, ModValue::Number(35.0));
}

#[test]
fn parses_accuracy_and_defence_stats() {
    let accuracy = parse_mod("+300 to Accuracy Rating").unwrap();
    let armour = parse_mod("20% increased Armour").unwrap();
    let evasion = parse_mod("100% increased Evasion Rating").unwrap();
    let energy_shield = parse_mod("+50 to maximum Energy Shield").unwrap();

    assert_eq!(accuracy.mods[0].name, ModName::from("Accuracy"));
    assert_eq!(accuracy.mods[0].mod_type, ModType::Base);
    assert_eq!(accuracy.mods[0].value, ModValue::Number(300.0));
    assert_eq!(armour.mods[0].name, ModName::from("Armour"));
    assert_eq!(armour.mods[0].mod_type, ModType::Inc);
    assert_eq!(evasion.mods[0].name, ModName::from("Evasion"));
    assert_eq!(evasion.mods[0].mod_type, ModType::Inc);
    assert_eq!(energy_shield.mods[0].name, ModName::from("EnergyShield"));
    assert_eq!(energy_shield.mods[0].mod_type, ModType::Base);
}

#[test]
fn parses_attack_preflag_and_damage_type() {
    let outcome = parse_mod("Attacks deal 25% increased Physical Damage").unwrap();
    let modifier = &outcome.mods[0];

    assert_eq!(modifier.name, ModName::from("PhysicalDamage"));
    assert_eq!(modifier.flags, ModFlags::ATTACK);
    assert!(
        modifier
            .tags
            .contains(&ModTag::DamageType(DamageType::Physical))
    );
}

#[test]
fn parses_multiplier_tag_with_limitless_charge_scaling() {
    let outcome = parse_mod("8% increased Damage per Power Charge").unwrap();
    let modifier = &outcome.mods[0];

    assert_eq!(modifier.name, ModName::from("Damage"));
    assert_eq!(modifier.mod_type, ModType::Inc);
    assert!(modifier.tags.contains(&ModTag::Multiplier {
        var: "PowerCharge".into(),
        div: 1.0,
        limit: None,
    }));
}

#[test]
fn parses_per_resource_scaling_with_divisor() {
    use pobr_core::CalcConfig;

    // `+N to <stat> per M <resource>` → 纯 stat + Multiplier{var, div=M}（PoB2 PerStat）。
    let outcome = parse_mod("+2 to Armour per 1 Spirit").unwrap();
    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("Armour"));
    assert_eq!(modifier.mod_type, ModType::Base);
    assert!(modifier.tags.contains(&ModTag::Multiplier {
        var: "Spirit".into(),
        div: 1.0,
        limit: None,
    }));
    // effective_number 按 cfg.multipliers[Spirit] / div 展开：336 Spirit → 2 * 336 = 672。
    let cfg = CalcConfig::new().with_multiplier("Spirit", 336.0);
    assert_eq!(modifier.effective_number(&cfg), Some(672.0));
}

#[test]
fn parses_per_n_attribute_scaling() {
    use pobr_core::CalcConfig;

    // `per 10 Intelligence` → div=10：100 Int → 5 * (100/10) = 50。
    let outcome = parse_mod("+5 to maximum Mana per 10 Intelligence").unwrap();
    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("MaximumMana"));
    assert!(modifier.tags.contains(&ModTag::Multiplier {
        var: "Intelligence".into(),
        div: 10.0,
        limit: None,
    }));
    let cfg = CalcConfig::new().with_multiplier("Intelligence", 100.0);
    assert_eq!(modifier.effective_number(&cfg), Some(50.0));
}

#[test]
fn parses_per_resource_without_divisor() {
    // `per Strength`（无数字）→ div=1。
    let outcome = parse_mod("+1 to Accuracy per Strength").unwrap();
    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("Accuracy"));
    assert!(modifier.tags.contains(&ModTag::Multiplier {
        var: "Strength".into(),
        div: 1.0,
        limit: None,
    }));
}

#[test]
fn unsupported_text_returns_no_mods_and_original_line() {
    let outcome = parse_mod("Mirrored").unwrap();

    assert_eq!(outcome.status, ParseStatus::Unsupported);
    assert!(outcome.mods.is_empty());
    assert_eq!(outcome.unparsed.as_deref(), Some("Mirrored"));
}

#[test]
fn unknown_text_is_an_error_with_original_line() {
    let error = parse_mod("this line is not a known modifier").unwrap_err();

    assert_eq!(error.input, "this line is not a known modifier");
}

#[test]
fn cache_returns_hits_and_stores_successful_outcomes() {
    let mut cache = ModCache::new();

    let first = cache.parse_or_insert("20% increased Fire Damage").unwrap();
    let second = cache.parse_or_insert("20% increased Fire Damage").unwrap();

    assert_eq!(cache.len(), 1);
    assert_eq!(first, second);
    assert_eq!(second.status, ParseStatus::Parsed);
}

#[test]
fn cache_keeps_unsupported_outcomes_for_stable_diffs() {
    let mut cache = ModCache::new();

    let outcome = cache.parse_or_insert("Mirrored").unwrap();

    assert_eq!(
        outcome,
        ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some("Mirrored".into()),
        }
    );
    assert_eq!(cache.len(), 1);
}

#[test]
fn parses_adds_range_damage_to_two_base_mods() {
    let outcome = parse_mod("Adds 1 to 356 Lightning Damage").unwrap();
    assert_eq!(outcome.status, ParseStatus::Parsed);
    assert_eq!(outcome.mods.len(), 2, "range adds two mods (min + max)");

    let min = &outcome.mods[0];
    assert_eq!(min.name, ModName::from("LightningDamageMin"));
    assert_eq!(min.mod_type, ModType::Base);
    assert_eq!(min.value, ModValue::Number(1.0));

    let max = &outcome.mods[1];
    assert_eq!(max.name, ModName::from("LightningDamageMax"));
    assert_eq!(max.value, ModValue::Number(356.0));
}

#[test]
fn parses_adds_damage_to_attacks_with_flag() {
    let outcome = parse_mod("Adds 27 to 39 Fire Damage to Attacks").unwrap();
    assert_eq!(outcome.status, ParseStatus::Parsed);
    assert_eq!(outcome.mods.len(), 2);
    assert_eq!(outcome.mods[0].name, ModName::from("FireDamageMin"));
    assert!(
        outcome.mods[0].flags.intersects(ModFlags::ATTACK),
        "'to Attacks' should set the Attack flag"
    );
}

#[test]
fn strips_pob_bracket_markup() {
    // [内部|显示] → 显示名；展开复合 + 聚合。
    let o = parse_mod("15% increased [Evasion|Evasion Rating]").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("Evasion"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);

    let o = parse_mod("12% increased [ElementalDamage|Elemental] Damage with [Attack|Attacks]")
        .unwrap();
    assert_eq!(o.mods[0].name, ModName::from("ElementalDamage"));

    // `any attribute`（属性小点三选一）不展开——PoB2 ModParser 映射为空 mod；
    // 玩家选择经 AttributeOverride 在树收集阶段改写为具体属性后再解析。
    assert!(
        parse_mod("+5 to any [Attributes|Attribute]").is_err(),
        "any attribute 不直接贡献属性"
    );
    let o = parse_mod("+10 to all Attributes").unwrap();
    assert_eq!(o.mods.len(), 3, "all attributes → str/dex/int");
}

#[test]
fn parses_conversion_and_gain_as_extra() {
    let o = parse_mod("100% of Fire Damage Converted to Lightning Damage").unwrap();
    assert_eq!(
        o.mods[0].name,
        ModName::from("FireDamageConvertToLightning")
    );
    assert_eq!(o.mods[0].value, ModValue::Number(100.0));

    let o = parse_mod("Gain 13% of Damage as Extra Chaos Damage").unwrap();
    assert_eq!(o.mods[0].name, ModName::from("DamageGainAsChaos"));

    let o = parse_mod("Gain 5% of Damage as Extra Damage of all Elements").unwrap();
    assert_eq!(o.mods.len(), 3, "all elements → fire/cold/lightning");
}

#[test]
fn parses_against_rarity_conditions() {
    // 「against Rare or Unique Enemies」→ 条件型增伤（DPS vs boss 时由 cfg 置真）。
    let o = parse_mod("50% increased Attack Damage against Rare or Unique Enemies").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("AttackDamage"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);
    assert!(
        o.mods[0]
            .tags
            .iter()
            .any(|t| matches!(t, ModTag::Condition { var, .. } if var == "RareOrUnique")),
        "should carry RareOrUnique condition tag"
    );
}

#[test]
fn parses_compound_attack_and_cast_speed_to_two_mods() {
    let outcome = parse_mod("8% increased Attack and Cast Speed").unwrap();
    assert_eq!(outcome.status, ParseStatus::Parsed);
    let names: Vec<_> = outcome.mods.iter().map(|m| m.name.clone()).collect();
    assert!(names.contains(&ModName::from("AttackSpeed")));
    assert!(names.contains(&ModName::from("CastSpeed")));
    for m in &outcome.mods {
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value, ModValue::Number(8.0));
    }
}

#[test]
fn parses_weapon_type_attack_speed_as_condition() {
    let outcome = parse_mod("3% increased Attack Speed with Quarterstaves").unwrap();
    let m = &outcome.mods[0];
    assert_eq!(m.name, ModName::from("AttackSpeed"));
    assert_eq!(m.mod_type, ModType::Inc);
    assert!(
        m.tags
            .iter()
            .any(|t| matches!(t, ModTag::Condition { var, .. } if var == "UsingQuarterstaff")),
        "weapon-type attack speed should carry UsingQuarterstaff condition"
    );
}

#[test]
fn parses_compound_dual_type_resistance_to_two_mods() {
    // PoB2 ModParser modNameList：`fire and chaos resistances` → 火抗 + 混沌抗。
    let o = parse_mod("+13% to Fire and Chaos Resistances").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    let names: Vec<_> = o.mods.iter().map(|m| m.name.clone()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&ModName::from("FireResistance")));
    assert!(names.contains(&ModName::from("ChaosResistance")));
    for m in &o.mods {
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value, ModValue::Number(13.0));
    }
}

#[test]
fn parses_all_resistances_including_chaos() {
    // `all resistances`（含混沌）区别于不含混沌的 `all elemental resistances`。
    let o = parse_mod("+10% to all Resistances").unwrap();
    let names: Vec<_> = o.mods.iter().map(|m| m.name.clone()).collect();
    assert_eq!(names.len(), 4);
    assert!(names.contains(&ModName::from("FireResistance")));
    assert!(names.contains(&ModName::from("ColdResistance")));
    assert!(names.contains(&ModName::from("LightningResistance")));
    assert!(names.contains(&ModName::from("ChaosResistance")));

    let elem = parse_mod("+10% to all Elemental Resistances").unwrap();
    assert_eq!(elem.mods.len(), 3, "elemental variant excludes chaos");
}

#[test]
fn strips_scope_suffix_for_spells_into_flag() {
    // `... for Spells` 后缀作用域 → SPELL flag（残留会使 resolve_names 失败）。
    let o = parse_mod("+15% to Critical Hit Chance for Spells").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("CriticalStrikeChance"));
    assert!(o.mods[0].flags.intersects(ModFlags::SPELL));
    assert!(!o.mods[0].flags.intersects(ModFlags::ATTACK));
}

#[test]
fn strips_scope_prefix_attack_critical_into_flag() {
    // `Attack Critical Hit Chance` 前缀作用域 → ATTACK flag。
    let o = parse_mod("8% increased Attack Critical Hit Chance").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("CriticalStrikeChance"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);
    assert!(o.mods[0].flags.intersects(ModFlags::ATTACK));
}

#[test]
fn maps_skill_speed_and_freeze_buildup_and_mana_regen() {
    assert_eq!(
        parse_mod("10% increased Skill Speed").unwrap().mods[0].name,
        ModName::from("SkillSpeed")
    );
    assert_eq!(
        parse_mod("20% increased Freeze Buildup").unwrap().mods[0].name,
        ModName::from("FreezeBuildup")
    );
    assert_eq!(
        parse_mod("15% increased Mana Regeneration Rate")
            .unwrap()
            .mods[0]
            .name,
        ModName::from("ManaRegen")
    );
}

#[test]
fn keystone_maximum_life_is_one_yields_override_and_ci_flag() {
    // CI 关键石：`Maximum Life is 1` → MaximumLife OVERRIDE 1 + ChaosInoculation flag。
    let o = parse_mod("Maximum Life is 1").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    let life = o
        .mods
        .iter()
        .find(|m| m.name == ModName::from("MaximumLife"))
        .expect("MaximumLife override present");
    assert_eq!(life.mod_type, ModType::Override);
    assert_eq!(life.value, ModValue::Number(1.0));
    assert!(
        o.mods
            .iter()
            .any(|m| m.name == ModName::from("ChaosInoculation") && m.mod_type == ModType::Flag),
        "CI flag should accompany the life override"
    );
}

#[test]
fn keystone_no_mana_yields_zero_override() {
    let o = parse_mod("You have no Mana").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("MaximumMana"));
    assert_eq!(o.mods[0].mod_type, ModType::Override);
    assert_eq!(o.mods[0].value, ModValue::Number(0.0));
}

#[test]
fn pure_immunity_phrase_is_unsupported_not_error() {
    // 纯免疫短语无数值：归 Unsupported（不报错、不产数值），避免噪声。
    let o = parse_mod("Immune to Chaos Damage and Bleeding").unwrap();
    assert_eq!(o.status, ParseStatus::Unsupported);
    assert!(o.mods.is_empty());
}

#[test]
fn weapon_type_guard_keeps_damage_as_weapon_keyword_name() {
    // 「damage with crossbows」必须映射到武器类伤害名 CrossbowDamage（keyword 聚合），
    // 不能被武器类条件守卫误转成带 UsingCrossbow 条件的通用 Damage（否则丢失武器伤害）。
    let outcome = parse_mod("20% increased Damage with Crossbows").unwrap();
    let m = &outcome.mods[0];
    assert_eq!(m.name, ModName::from("CrossbowDamage"));
    assert!(
        !m.tags.iter().any(|t| matches!(t, ModTag::Condition { .. })),
        "damage-with-weapon should not become a conditional mod"
    );
}

#[test]
fn buffs_also_grant_is_unsupported() {
    // 「Archon Buffs also grant +20% to all Elemental Resistances」——vendor 不支持该词条族
    // （ModCache.lua:4527-4532 缓存为 nil）；面板不应注入任何数值（stormweaver FireResist
    // 黄金 71 证实 PoB2 未应用 +20）。
    let outcome = parse_mod("Archon Buffs also grant +20% to all Elemental Resistances").unwrap();
    assert_eq!(outcome.status, ParseStatus::Unsupported);
    assert!(outcome.mods.is_empty());
}

#[test]
fn parses_increased_armour_from_equipped_body_armour_as_slot_tag() {
    // Titan `80% increased Armour from Equipped Body Armour`：剥离槽位子句 → 纯 Armour INC
    // + SlotName("bodyarmour") tag（per-slot 防御聚合在该槽生效）。
    let outcome = parse_mod("80% increased Armour from Equipped Body Armour").unwrap();
    assert_eq!(outcome.status, ParseStatus::Parsed);
    assert_eq!(outcome.mods.len(), 1);
    let m = &outcome.mods[0];
    assert_eq!(m.name, ModName::from("Armour"));
    assert_eq!(m.mod_type, ModType::Inc);
    assert_eq!(m.value, ModValue::Number(80.0));
    assert!(
        m.tags
            .iter()
            .any(|t| matches!(t, ModTag::SlotName(s) if s == "bodyarmour")),
        "expected SlotName(bodyarmour) tag, got {:?}",
        m.tags
    );
    assert_eq!(m.slot_name(), Some("bodyarmour"));
}

#[test]
fn parses_energy_shield_from_equipped_focus_as_weapon2_slot() {
    // Focus（副手法器）在 weapon2 槽：`44% increased Energy Shield from Equipped Focus`
    // → EnergyShield INC + SlotName("weapon2")。
    let outcome = parse_mod("44% increased Energy Shield from Equipped Focus").unwrap();
    assert_eq!(outcome.status, ParseStatus::Parsed);
    let m = &outcome.mods[0];
    assert_eq!(m.name, ModName::from("EnergyShield"));
    assert_eq!(m.slot_name(), Some("weapon2"));
}

#[test]
fn parses_more_armour_from_equipped_body_armour_as_slot_more() {
    // `50% more Armour from Equipped Body Armour` → Armour MORE + SlotName(bodyarmour)。
    let outcome = parse_mod("50% more Armour from Equipped Body Armour").unwrap();
    let m = &outcome.mods[0];
    assert_eq!(m.mod_type, ModType::More);
    assert_eq!(m.slot_name(), Some("bodyarmour"));
}

#[test]
fn slot_tag_is_transparent_to_normal_sum_but_scoped_in_per_slot() {
    use pobr_core::{CalcConfig, ModDb, Modifier};
    let cfg = CalcConfig::default();
    let mut db = ModDb::new();
    // 全局 inc + 槽位限定底 + 槽位限定 inc。
    db.add_mod(Modifier::number("Armour", ModType::Inc, 100.0)); // global inc
    db.add_mod(
        Modifier::number("Armour", ModType::Base, 1000.0)
            .with_tag(ModTag::SlotName("bodyarmour".into())),
    );
    db.add_mod(
        Modifier::number("Armour", ModType::Inc, 80.0)
            .with_tag(ModTag::SlotName("bodyarmour".into())),
    );
    let names = [ModName::from("Armour")];
    // global_only 排除槽位 inc。
    assert_eq!(db.sum_global_only(ModType::Inc, &cfg, &names), 100.0);
    // for_slot 只取该槽 inc。
    assert_eq!(
        db.sum_for_slot(ModType::Inc, &cfg, &names, "bodyarmour"),
        80.0
    );
    // slot_bases 取该槽底。
    assert_eq!(
        db.slot_bases(&cfg, &ModName::from("Armour")),
        vec![("bodyarmour".to_string(), 1000.0)]
    );
}

/// 01-03：PoB2 EvalMod 对 Multiplier/PerStat 倍率做 `m_floor(base/div + 0.0001)`，
/// 非整倍资源须向下取整。`+5 ... per 10 Intelligence` 在 95 Int → floor(9.5)=9 → 45（非 47.5）。
#[test]
fn per_stat_multiplier_floors_non_integral_count_like_pob2() {
    use pobr_core::CalcConfig;

    let outcome = parse_mod("+5 to maximum Mana per 10 Intelligence").unwrap();
    let modifier = &outcome.mods[0];
    assert!(modifier.tags.contains(&ModTag::Multiplier {
        var: "Intelligence".into(),
        div: 10.0,
        limit: None,
    }));
    // 95 Int → floor(9.5 + 0.0001) = 9 → 5 * 9 = 45。
    let cfg_non_integral = CalcConfig::new().with_multiplier("Intelligence", 95.0);
    assert_eq!(modifier.effective_number(&cfg_non_integral), Some(45.0));
    // 100 Int → floor(10.0001) = 10 → 50（整倍不受 floor 影响）。
    let cfg_integral = CalcConfig::new().with_multiplier("Intelligence", 100.0);
    assert_eq!(modifier.effective_number(&cfg_integral), Some(50.0));
}

/// `Bonded: <mod>` 符文词条默认不生效——整条挂 `CanUseBondedModifiers` 条件
/// （PoB2 ModParser `["^bonded: "]`），仅当激活源授予对应 flag 时按 cfg 条件激活。
#[test]
fn bonded_prefix_gates_mod_behind_condition() {
    use pobr_core::{CalcConfig, ModDb};

    let o = parse_mod("Bonded: +20 to maximum Mana").unwrap();
    assert_eq!(o.mods.len(), 1);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("MaximumMana"));
    assert!(
        m.tags.iter().any(|t| matches!(
            t,
            ModTag::Condition { var, negated: false } if var == "CanUseBondedModifiers"
        )),
        "Bonded 词条须挂 CanUseBondedModifiers 条件"
    );

    // 条件未置真 → 不参与聚合；置真 → 生效。
    let mut db = ModDb::new();
    db.add_list(o.mods.clone());
    let off = CalcConfig::new();
    assert_eq!(
        db.sum(ModType::Base, &off, &[ModName::from("MaximumMana")]),
        0.0
    );
    let on = CalcConfig::new().with_condition("CanUseBondedModifiers", true);
    assert_eq!(
        db.sum(ModType::Base, &on, &[ModName::from("MaximumMana")]),
        20.0
    );
}

/// Bonded 激活源 → `Condition:CanUseBondedModifiers` FLAG（编排层据此置 cfg 条件）。
#[test]
fn bonded_enabler_parses_to_condition_flag() {
    let o = parse_mod("Gain the benefits of Bonded modifiers on Runes and Idols").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(
        o.mods[0].name,
        ModName::from("Condition:CanUseBondedModifiers")
    );
    assert_eq!(o.mods[0].mod_type, ModType::Flag);
}
