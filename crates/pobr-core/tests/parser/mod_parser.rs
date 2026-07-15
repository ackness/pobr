use crate::support::parse_mod;
use pobr_core::mod_cache::ModCache;
use pobr_core::mod_parser::{ParseOutcome, ParseStatus};
use pobr_core::{CalcConfig, ModTag, ModValue};
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
    assert!(
        modifier
            .tags
            .contains(&ModTag::condition("FullLife", false))
    );
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
    assert!(
        modifier
            .tags
            .contains(&ModTag::multiplier("PowerCharge", 1.0, None))
    );
}

#[test]
fn parses_per_resource_scaling_with_divisor() {
    use pobr_core::CalcConfig;

    // `+N to <stat> per M <resource>` → 纯 stat + Multiplier{var, div=M}（PoB2 PerStat）。
    let outcome = parse_mod("+2 to Armour per 1 Spirit").unwrap();
    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("Armour"));
    assert_eq!(modifier.mod_type, ModType::Base);
    assert!(
        modifier
            .tags
            .contains(&ModTag::multiplier("Spirit", 1.0, None))
    );
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
    assert!(
        modifier
            .tags
            .contains(&ModTag::multiplier("Intelligence", 10.0, None))
    );
    let cfg = CalcConfig::new().with_multiplier("Intelligence", 100.0);
    assert_eq!(modifier.effective_number(&cfg), Some(50.0));
}

#[test]
fn parses_per_resource_without_divisor() {
    // `per Strength`（无数字）→ div=1。
    let outcome = parse_mod("+1 to Accuracy per Strength").unwrap();
    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("Accuracy"));
    assert!(
        modifier
            .tags
            .contains(&ModTag::multiplier("Strength", 1.0, None))
    );
}

#[test]
fn unsupported_text_returns_no_mods_and_original_line() {
    let outcome = parse_mod("Mirrored").unwrap();

    assert_eq!(outcome.status, ParseStatus::Unsupported);
    assert!(outcome.mods.is_empty());
    assert_eq!(outcome.unparsed.as_deref(), Some("Mirrored"));
}

#[test]
fn unknown_text_is_unsupported_with_original_line() {
    // 引擎对无法识别的行永不报错——整行 Unsupported，原文进 unparsed。
    let outcome = parse_mod("this line is not a known modifier").unwrap();

    assert_eq!(outcome.status, ParseStatus::Unsupported);
    assert!(outcome.mods.is_empty());
    assert_eq!(
        outcome.unparsed.as_deref(),
        Some("this line is not a known modifier")
    );
}

#[test]
fn cache_returns_hits_and_stores_successful_outcomes() {
    let mut cache = ModCache::new();

    let first = cache
        .parse_or_insert_with_ctx("20% increased Fire Damage", crate::support::ctx())
        .unwrap();
    let second = cache
        .parse_or_insert_with_ctx("20% increased Fire Damage", crate::support::ctx())
        .unwrap();

    assert_eq!(cache.len(), 1);
    assert_eq!(first, second);
    assert_eq!(second.status, ParseStatus::Parsed);
}

#[test]
fn cache_keeps_unsupported_outcomes_for_stable_diffs() {
    let mut cache = ModCache::new();

    let outcome = cache
        .parse_or_insert_with_ctx("Mirrored", crate::support::ctx())
        .unwrap();

    assert_eq!(
        outcome,
        ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some("Mirrored".into()),
            special_meta: None,
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

    // `any attribute`（属性小点三选一）不展开——玩家选择经 AttributeOverride 在树
    // 收集阶段改写为具体属性后再解析。引擎对原文有残留（unparsed 非空 → 生产闸门
    // 整行丢弃），且绝不产出任何具体属性 mod。
    let o = parse_mod("+5 to any [Attributes|Attribute]").unwrap();
    assert!(
        o.unparsed.is_some(),
        "any attribute 原文应留残（生产闸门整行丢弃）"
    );
    for attr in ["Strength", "Dexterity", "Intelligence"] {
        assert!(
            o.mods.iter().all(|m| m.name != ModName::from(attr)),
            "any attribute 不直接贡献属性 {attr}"
        );
    }
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

/// 多源合写 converted-to（vendor ModParser.lua:2405-2409 specialModList）。
/// 通用后缀路径只会吃掉 lightning 分支并留残 `Physical, Cold and`——special
/// 通道必须整行吃净、三源各产一条 ConvertToFire BASE。
#[test]
fn parses_multi_source_converted_to_fire() {
    let o =
        parse_mod("30% of Physical, Cold and Lightning Damage Converted to Fire Damage").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert!(o.unparsed.is_none(), "整行须被吃净，不得留残");
    assert_eq!(o.mods.len(), 3);
    for src in ["Physical", "Lightning", "Cold"] {
        let m = o
            .mods
            .iter()
            .find(|m| m.name == ModName::from(format!("{src}DamageConvertToFire")))
            .unwrap_or_else(|| panic!("缺 {src}DamageConvertToFire"));
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value, ModValue::Number(30.0));
    }
}

/// 无数值整行 converted-to（vendor ModParser.lua:2410-2414）：三元素各 100。
#[test]
fn parses_all_elemental_converted_to_chaos() {
    let o = parse_mod("All Elemental Damage Converted to Chaos Damage").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert!(o.unparsed.is_none());
    assert_eq!(o.mods.len(), 3);
    for src in ["Cold", "Fire", "Lightning"] {
        let m = o
            .mods
            .iter()
            .find(|m| m.name == ModName::from(format!("{src}DamageConvertToChaos")))
            .unwrap_or_else(|| panic!("缺 {src}DamageConvertToChaos"));
        assert_eq!(m.value, ModValue::Number(100.0));
    }
}

/// 承伤转换通用形（modNameList `<src> damage taken` + suffixTypes `as <dst>
/// damage`，消费侧 calc/taken.rs `<Src>DamageTakenAs<Dst>`）。钉住整行吃净。
#[test]
fn parses_generic_damage_taken_as() {
    let o = parse_mod("5% of Physical Damage taken as Fire Damage").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert!(o.unparsed.is_none());
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("PhysicalDamageTakenAsFire"));
    assert_eq!(o.mods[0].mod_type, ModType::Base);
    assert_eq!(o.mods[0].value, ModValue::Number(5.0));

    let o = parse_mod("10% of Physical Damage from Hits taken as Cold Damage").unwrap();
    assert!(o.unparsed.is_none());
    assert_eq!(
        o.mods[0].name,
        ModName::from("PhysicalDamageFromHitsTakenAsCold")
    );
}

/// 裸目标 taken-as（vendor ModParser.lua:5655-5656 specialModList）：suffixTypes
/// 无 bare `as lightning`，通用路径会误产 `<Src>DamageTaken BASE` + 残留
/// `as Lightning`——special 通道必须接管。
#[test]
fn parses_bare_target_taken_as_lightning() {
    for (src, text) in [
        ("Cold", "30% of Cold Damage taken as Lightning"),
        ("Fire", "30% of Fire Damage taken as Lightning"),
    ] {
        let o = parse_mod(text).unwrap();
        assert_eq!(o.status, ParseStatus::Parsed);
        assert!(o.unparsed.is_none(), "{text}: 整行须被吃净");
        assert_eq!(o.mods.len(), 1);
        assert_eq!(
            o.mods[0].name,
            ModName::from(format!("{src}DamageTakenAsLightning"))
        );
        assert_eq!(o.mods[0].value, ModValue::Number(30.0));
    }
}

/// flask 双源 taken-as（vendor ModParser.lua:5657-5660）：fire+lightning 各产
/// 一条 FromHitsTakenAsCold，带 `Condition: UsingFlask`。
#[test]
fn parses_flask_fire_lightning_from_hits_taken_as_cold() {
    let o =
        parse_mod("20% of Fire and Lightning Damage from Hits taken as Cold Damage during Effect")
            .unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert!(o.unparsed.is_none());
    assert_eq!(o.mods.len(), 2);
    for src in ["Fire", "Lightning"] {
        let m = o
            .mods
            .iter()
            .find(|m| m.name == ModName::from(format!("{src}DamageFromHitsTakenAsCold")))
            .unwrap_or_else(|| panic!("缺 {src}DamageFromHitsTakenAsCold"));
        assert_eq!(m.value, ModValue::Number(20.0));
        assert!(
            m.tags.contains(&ModTag::condition("UsingFlask", false)),
            "{src}: 缺 UsingFlask 条件"
        );
    }
}

/// random element 承伤（vendor ModParser.lua:5661-5665）：AVERAGE 三分 num/3。
/// 真实语料词条（`5% of Physical Damage from Hits taken as Damage of a Random
/// Element`），legacy 删除后曾回归为 `PhysicalDamageFromHitsTaken BASE 5` + 残留。
#[test]
fn parses_phys_from_hits_taken_as_random_element() {
    let o = parse_mod(
        "5% of [Physical] Damage from [HitDamage|Hits] taken as Damage of a Random [ElementalDamage|Element]",
    )
    .unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert!(o.unparsed.is_none());
    assert_eq!(o.mods.len(), 3);
    for dst in ["Fire", "Cold", "Lightning"] {
        let m = o
            .mods
            .iter()
            .find(|m| m.name == ModName::from(format!("PhysicalDamageFromHitsTakenAs{dst}")))
            .unwrap_or_else(|| panic!("缺 PhysicalDamageFromHitsTakenAs{dst}"));
        assert_eq!(m.value, ModValue::Number(5.0 / 3.0));
    }
}

/// 聚合源 gain-as（M4-H；vendor ModParser.lua:702 `["elemental damage"] =
/// "ElementalDamage"` + 后缀表 `:6173` → `ElementalDamageGainAsCold`，ModCache
/// 实证 `Gain 10% of Elemental Damage as Extra Cold Damage`；druid-oracle
/// ember-fusillade 的 Storm Bane/Blood Barrier 词条）。
#[test]
fn parses_elemental_source_gain_as_extra() {
    let o = parse_mod("Gain 16% of Elemental Damage as Extra Cold Damage").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("ElementalDamageGainAsCold"));
    assert_eq!(o.mods[0].mod_type, ModType::Base);
    assert_eq!(o.mods[0].value, ModValue::Number(16.0));
}

/// random element 档（M4-H；vendor ModParser.lua:6182 `["as extra damage of a
/// random element"] = "GainAsRandom"`，ModCache.lua:5257 `Gain 5% of Damage as
/// Extra Damage of a random Element` → `DamageGainAsRandom BASE 5`；消费 =
/// CalcOffence.lua:1175-1200 physMode 展开）。
#[test]
fn parses_random_element_gain_as() {
    let o = parse_mod("Gain 5% of Damage as Extra Damage of a random Element").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("DamageGainAsRandom"));
    assert_eq!(o.mods[0].value, ModValue::Number(5.0));

    // weapon phys 变体（ModParser.lua:3691 `gain N% of physical damage as extra
    // damage of a random element` → PhysicalDamageGainAsRandom）。
    let o = parse_mod("Gain 10% of Physical Damage as Extra Damage of a random Element").unwrap();
    assert_eq!(o.mods[0].name, ModName::from("PhysicalDamageGainAsRandom"));
}

/// per-curse 数值缩放尾缀（M4-H；vendor ModParser.lua:1507-1510 →
/// `Multiplier:CurseOnEnemy`，乘数 = `#curseSlots`（CalcPerform.lua:2969）；
/// witch-blood-mage Liminal Coil 词条 `Spell Hits Gain 30% of Damage as Extra
/// Physical Damage per Curse on target` → raw 30 × 5 curse = vendor 实测 150）。
#[test]
fn parses_gain_as_per_curse_with_spell_hits_prefix() {
    let o = parse_mod("Spell Hits Gain 30% of Damage as Extra Physical Damage per Curse on target")
        .unwrap();
    assert_eq!(o.mods.len(), 1);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("DamageGainAsPhysical"));
    assert_eq!(m.mod_type, ModType::Base);
    assert_eq!(m.value, ModValue::Number(30.0));
    // vendor `^spell hits [ghd][ae][iva][eln] ` → flags=Hit, keywordFlags=Spell
    // （ModParser.lua:1273）；PoBR 无 Spell keyword 位，等价折为 HIT|SPELL ModFlags。
    assert_eq!(m.flags, ModFlags::HIT | ModFlags::SPELL);
    assert!(
        m.tags
            .iter()
            .any(|t| matches!(t, ModTag::Multiplier { var, .. } if var == "CurseOnEnemy")),
        "应携带 Multiplier:CurseOnEnemy tag"
    );

    // 乘数求值：5 个 curse 槽 → 30 × 5 = 150（vendor witch oracle 钉值）。
    let cfg = CalcConfig::new()
        .with_flags(ModFlags::HIT | ModFlags::SPELL)
        .with_multiplier("CurseOnEnemy", 5.0);
    assert_eq!(m.effective_number(&cfg), Some(150.0));
}

/// per-different-grenade 尾缀（M4-H；vendor ModParser.lua:1528 →
/// `Multiplier:DifferentGrenadeFired`，limitVar=GrenadeTypes；mercenary
/// Demolitionist 树点 `Gain 4% of Damage as Extra Fire Damage for every
/// different Grenade fired in the past 8 seconds`）。
#[test]
fn parses_gain_as_per_different_grenade_fired() {
    let o = parse_mod(
        "Gain 4% of Damage as Extra Fire Damage for every different Grenade fired in the past 8 seconds",
    )
    .unwrap();
    assert_eq!(o.mods.len(), 1);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("DamageGainAsFire"));
    assert_eq!(m.value, ModValue::Number(4.0));
    assert!(
        m.tags.iter().any(|t| matches!(
            t,
            ModTag::Multiplier { var, limit_var: Some(lv), .. }
                if var == "DifferentGrenadeFired" && lv == "GrenadeTypes"
        )),
        "应携带 Multiplier:DifferentGrenadeFired（limitVar=GrenadeTypes）tag"
    );
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
fn pure_immunity_phrase_yields_no_mods() {
    // 纯免疫短语无数值：不产数值 mod（引擎识别为空产出，不报错），避免噪声。
    let o = parse_mod("Immune to Chaos Damage and Bleeding").unwrap();
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
    assert!(
        modifier
            .tags
            .contains(&ModTag::multiplier("Intelligence", 10.0, None))
    );
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
            ModTag::Condition { var, negated: false, .. } if var == "CanUseBondedModifiers"
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

/// Bonded 激活行（Druid Oracle 升华，ModParser.lua:3423-3424）→
/// `Condition:CanUseBondedModifiers` FLAG（special `bonded_modifiers_enabler`）。
/// 编排层消费点 calc_orchestrator 6d：session.has_flag → set_condition，
/// 解锁上面测试的 `Bonded:` 前缀词条。
#[test]
fn bonded_enabler_line_grants_condition_flag() {
    let o = parse_mod("Gain the benefits of Bonded modifiers on Runes and Idols").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 1);
    assert_eq!(
        o.mods[0].name,
        ModName::from("Condition:CanUseBondedModifiers")
    );
    assert_eq!(o.mods[0].value, ModValue::Bool(true));
}

/// 腰带 implicit「Has N Charm Slot(s)」/ 天赋「+N Charm Slot」→ `CharmLimit` BASE N
/// （vendor ModParser.lua:5453 `h?a?s? ?+?(%d+) charm slots?`）。无 CharmLimit 来源
/// 时 env_finalize 阶段 3 的 charm 预算为 0（charm 全不生效），本词条是主要解锁源。
#[test]
fn charm_slots_implicit_parses_to_charm_limit_base() {
    for (text, expect) in [
        ("Has 2 Charm Slots", 2.0),
        ("Has 1 Charm Slot", 1.0),
        ("Has 3 Charm Slots", 3.0),
        ("+1 Charm Slot", 1.0),
        ("2 Charm Slots", 2.0),
    ] {
        let o = parse_mod(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(o.status, ParseStatus::Parsed, "{text}");
        assert_eq!(o.mods.len(), 1, "{text}");
        assert_eq!(o.mods[0].name, ModName::from("CharmLimit"), "{text}");
        assert_eq!(o.mods[0].mod_type, ModType::Base, "{text}");
        assert_eq!(o.mods[0].value, ModValue::Number(expect), "{text}");
    }
    // 非 charm-slot 词条不得误命中。
    let o = parse_mod("Has 1 Abyssal Socket");
    assert!(
        o.is_err()
            || o.unwrap()
                .mods
                .iter()
                .all(|m| m.name != ModName::from("CharmLimit")),
        "非 charm slot 词条不应产出 CharmLimit"
    );
}

// ===========================================================================
// M4-T1 W-A1 commit-2 / M4-I 切换：武器后缀位通道（condition + 武器位双写）
// ===========================================================================

/// `with Maces` / `with One Handed Melee Weapons` / `with Unarmed Attacks`
/// 三条词条的解析→匹配端到端（蓝图 W-A1 测试计划；切换 commit 起常驻）。
mod weapon_bits_e2e {
    use super::*;
    use pobr_core::CalcConfig;

    /// `with Maces`：MACE 位 + UsingMace 条件双写；cfg 武器位与条件齐备才匹配。
    #[test]
    fn with_maces_parses_and_matches_per_weapon_bits() {
        let o = parse_mod("10% increased Attack Speed with Maces").unwrap();
        let m = &o.mods[0];
        assert!(m.tags.contains(&ModTag::condition("UsingMace", false)));
        assert!(m.flags.is_subset_of(ModFlags::MACE));
        assert_eq!(m.flags.bits(), ModFlags::MACE.bits());

        // cfg = 持单手锤（编排侧 weapon_cfg_flags 派生口径）+ UsingMace 条件。
        let mace_cfg = CalcConfig::attack()
            .with_flags(
                ModFlags::ATTACK | ModFlags::weapon_flags("One Hand Mace", "Mace", true, true),
            )
            .with_condition("UsingMace", true);
        assert!(m.matches(&mace_cfg));

        // 持弓：bit 与 condition 两通道一致拒绝。
        let bow_cfg = CalcConfig::attack()
            .with_flags(ModFlags::ATTACK | ModFlags::weapon_flags("Bow", "Bow", false, false));
        assert!(!m.matches(&bow_cfg));
    }

    /// `with One Handed Melee Weapons`：Weapon1H|WeaponMelee 位（vendor :1017）。
    #[test]
    fn with_one_handed_melee_weapons_matches_weapon_class_bits() {
        let o = parse_mod("8% increased Attack Speed with One Handed Melee Weapons").unwrap();
        let m = &o.mods[0];
        assert!(
            m.tags
                .contains(&ModTag::condition("UsingOneHandedMelee", false))
        );
        assert_eq!(
            m.flags.bits(),
            (ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE).bits()
        );

        let dagger_cfg = CalcConfig::attack()
            .with_flags(ModFlags::ATTACK | ModFlags::weapon_flags("Dagger", "Dagger", true, true))
            .with_condition("UsingOneHandedMelee", true);
        assert!(m.matches(&dagger_cfg));

        // 双手武器：Weapon1H 缺位 → 拒绝。
        let th_cfg = CalcConfig::attack()
            .with_flags(
                ModFlags::ATTACK | ModFlags::weapon_flags("Two Hand Mace", "Mace", false, true),
            )
            .with_condition("UsingOneHandedMelee", true);
        assert!(!m.matches(&th_cfg));
    }

    /// `with Unarmed Attacks`：UNARMED 位（vendor :1006，切换后常驻解锁短语）。
    #[test]
    fn with_unarmed_attacks_matches_unarmed_bit() {
        let o = parse_mod("10% increased Attack Speed with Unarmed Attacks").unwrap();
        assert_eq!(o.status, ParseStatus::Parsed);
        let m = &o.mods[0];
        // 引擎另带 Hit 作用域位（子集匹配语义不变）；核心是 UNARMED 位在。
        assert!(ModFlags::UNARMED.is_subset_of(m.flags));

        // 空手（编排侧 weapon_cfg_flags 空主手分支；生产攻击上下文恒带 HIT 位）。
        let unarmed_cfg = CalcConfig::attack().with_flags(
            ModFlags::ATTACK
                | ModFlags::HIT
                | ModFlags::weapon_flags("None", "Unarmed", true, true),
        );
        assert!(m.matches(&unarmed_cfg));

        // 持武器 → UNARMED 缺位 → 拒绝。
        let mace_cfg = CalcConfig::attack().with_flags(
            ModFlags::ATTACK
                | ModFlags::HIT
                | ModFlags::weapon_flags("One Hand Mace", "Mace", true, true),
        );
        assert!(!m.matches(&mace_cfg));
    }
}

/// 诅咒无视上限（M4-H；vendor ModParser.lua:4275 → `EnemyCurseLimit BASE 99`，
/// 消费 = buff_pass 槽位预算 DEFAULT 1 + Σ，vendor CalcPerform.lua:2832 同式；
/// witch-blood-mage 的 Doedre's Undoing 词条，vendor EnemyCurseLimit 实测 100）。
#[test]
fn parses_curses_ignore_curse_limit() {
    let o = parse_mod("Curses you inflict ignore Curse limit").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("EnemyCurseLimit"));
    assert_eq!(o.mods[0].mod_type, ModType::Base);
    assert_eq!(o.mods[0].value, ModValue::Number(99.0));
}

/// 「N% increased Damage for each type of Elemental Ailment on Enemy」（The Taming，
/// vendor ModParser.lua:3798-3804）→ 5 条 Damage INC，各挂一种敌方异常条件
/// （`Enemy<X>` cfg 键空间，与 `against <X> enemies` 后缀一致）。
#[test]
fn parses_damage_per_elemental_ailment_type_on_enemy() {
    let o = parse_mod("21% increased Damage for each type of Elemental Ailment on Enemy").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 5);
    for m in &o.mods {
        assert_eq!(m.name, ModName::from("Damage"));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value, ModValue::Number(21.0));
    }
    let conds: Vec<_> = o.mods.iter().map(|m| m.tags[0].clone()).collect();
    for var in [
        "EnemyElectrocuted",
        "EnemyFrozen",
        "EnemyChilled",
        "EnemyIgnited",
        "EnemyShocked",
    ] {
        assert!(conds.contains(&ModTag::condition(var, false)), "{var}");
    }
    // 条件求值：Chilled + Ignited 真（twister 配置形态）→ 2 档生效。
    let cfg = CalcConfig::attack()
        .with_condition("EnemyChilled", true)
        .with_condition("EnemyIgnited", true);
    let active = o.mods.iter().filter(|m| m.matches(&cfg)).count();
    assert_eq!(active, 2);
}

/// 伙伴在场条件后缀（vendor ModParser.lua:1803 → CompanionInPresence）。
#[test]
fn parses_damage_while_companion_in_presence() {
    let o = parse_mod("10% increased Damage while your [Companion] is in your [Presence]").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("Damage"));
    assert!(
        m.tags
            .contains(&ModTag::condition("CompanionInPresence", false))
    );
}

/// 奥术涌动条件后缀（vendor ModParser.lua:1817 → AffectedByArcaneSurge）。
#[test]
fn parses_spell_damage_while_you_have_arcane_surge() {
    let o =
        parse_mod("30% increased Spell Damage while you have [ArcaneSurge|Arcane Surge]").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("SpellDamage"));
    assert!(
        m.tags
            .contains(&ModTag::condition("AffectedByArcaneSurge", false))
    );
}

/// 「N% chance to Gain Arcane Surge when you deal a Critical Hit」→
/// `Condition:ArcaneSurge` FLAG（vendor FLAG form 忽略几率，ModParser.lua:92/:4197；
/// 触发后缀 :1902 → CritRecently 条件）。
#[test]
fn parses_chance_to_gain_arcane_surge_on_crit() {
    let o = parse_mod(
        "10% chance to Gain [ArcaneSurgeDuration|Arcane Surge] when you deal a [Critical|Critical Hit]",
    )
    .unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 1);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("Condition:ArcaneSurge"));
    assert_eq!(m.mod_type, ModType::Flag);
    assert!(m.tags.contains(&ModTag::condition("CritRecently", false)));
}

/// 「Damage with One Handed Weapons」（vendor ModParser.lua:1016 `bor(Weapon1H,
/// Hit)`）→ name=Damage + Weapon1H|Hit 位；cfg 武器位（单手矛）+ HIT 命中、双手缺位拒绝。
#[test]
fn parses_damage_with_one_handed_weapons_bits() {
    let o = parse_mod("10% increased Damage with One Handed Weapons").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("Damage"));
    assert_eq!(m.flags.bits(), (ModFlags::WEAPON_1H | ModFlags::HIT).bits());

    let spear_cfg = CalcConfig::attack().with_flags(
        ModFlags::ATTACK | ModFlags::HIT | ModFlags::weapon_flags("Spear", "Spear", true, true),
    );
    assert!(m.matches(&spear_cfg));
    let th_cfg = CalcConfig::attack().with_flags(
        ModFlags::ATTACK
            | ModFlags::HIT
            | ModFlags::weapon_flags("Two Hand Mace", "Mace", false, true),
    );
    assert!(!m.matches(&th_cfg));

    // 双手变体（vendor :1018）。
    let o2 = parse_mod("10% increased Damage with Two Handed Weapons").unwrap();
    assert_eq!(
        o2.mods[0].flags.bits(),
        (ModFlags::WEAPON_2H | ModFlags::HIT).bits()
    );
}

/// 「Projectile Speed」名解锁（vendor ModName `ProjectileSpeed`）——常规无消费方，
/// `ProjectileSpeedAppliesToProjectileDamage` 转换（CalcOffence.lua:840-845）的输入。
#[test]
fn parses_projectile_speed_name() {
    let o = parse_mod("8% increased [Projectile] Speed").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods[0].name, ModName::from("ProjectileSpeed"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);
}

/// 「N% increased [Exposure] Effect」（树 Exposure Effect 小点 10 / Overexposure
/// 30）→ 三元素 `<El>ExposureEffect` INC 展开（vendor ModParser.lua:693
/// `["exposure effect"] = { "FireExposureEffect", "ColdExposureEffect",
/// "LightningExposureEffect" }`）。消费方 = `calc::reduce_enemy_exposure`
/// （CalcPerform.lua:3223 曝光 magnitude 的玩家效果 INC）。
#[test]
fn parses_exposure_effect_to_three_elements() {
    let o = parse_mod("30% increased [Exposure] Effect").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 3);
    for (m, name) in o.mods.iter().zip([
        "FireExposureEffect",
        "ColdExposureEffect",
        "LightningExposureEffect",
    ]) {
        assert_eq!(m.name, ModName::from(name));
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value, ModValue::Number(30.0));
    }
}

/// 单元素曝光效果（vendor ModParser.lua:690-692 `fire/cold/lightning exposure
/// effect`）→ 单条 `<El>ExposureEffect`。
#[test]
fn parses_single_element_exposure_effect() {
    let o = parse_mod("25% increased Cold Exposure Effect").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("ColdExposureEffect"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);
    assert_eq!(o.mods[0].value, ModValue::Number(25.0));
}

// ---------------------------------------------------------------------------
// M5a-B3：召唤物词条包裹（engine MinionModifier LIST 通道 +
// extract_minion_modifier_entries 消费端）
// ---------------------------------------------------------------------------

/// engine 解析 + MinionModifier 抽取的组合（生产消费路径同款）。
fn minion_entries(text: &str) -> Vec<pobr_core::calc::minion::MinionModifierEntry> {
    let outcome = parse_mod(text).unwrap();
    pobr_core::calc::minion::extract_minion_modifier_entries(&outcome.mods)
}

/// `Minions deal X% increased Damage` → 内层 `20% increased damage` 解析为
/// Damage INC 20，包裹为 MinionModifierEntry。
#[test]
fn parses_minion_increased_damage_wrapper() {
    let entries = minion_entries("Minions deal 20% increased Damage");
    assert_eq!(entries.len(), 1);
    let inner = &entries[0].inner;
    assert_eq!(inner.name, ModName::from("Damage"));
    assert_eq!(inner.mod_type, ModType::Inc);
    assert_eq!(inner.value, ModValue::Number(20.0));
    // minion_type=None → 适用所有召唤物。
    assert!(entries[0].minion_type.is_none());
}

/// `Minions have X% increased maximum Life` → Life INC 包裹。
#[test]
fn parses_minion_increased_life_wrapper() {
    let entries = minion_entries("Minions have 30% increased maximum Life");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].inner.mod_type, ModType::Inc);
    assert_eq!(entries[0].inner.value, ModValue::Number(30.0));
}

/// 非 `minions ` 前缀 → 无 MinionModifier（玩家词条不误判为召唤词条）。
#[test]
fn non_minion_text_yields_no_entries() {
    assert!(minion_entries("20% increased Fire Damage").is_empty());
    assert!(minion_entries("Minions ").is_empty()); // 空余文
}

/// `Minions …` 余文不可解析 → 无 entry（不产空 entry）。
#[test]
fn minion_unparsable_remainder_yields_no_entries() {
    assert!(minion_entries("Minions wibble wobble zorp").is_empty());
}

// ---------------------------------------------------------------------------
// wave2-defence：防御向特殊词条（special_mods `wave2-defence` 批；原 legacy-only
// 覆盖 mod_parser_m2_defence.rs 删除后回填的 engine 形态钉，产出对齐 vendor
// ModParser.lua specialModList）
// ---------------------------------------------------------------------------

/// 断言 outcome 恰为一组 FLAG mods（顺序一致）。
fn assert_flags(text: &str, names: &[&str]) {
    let o = parse_mod(text).unwrap();
    assert_eq!(o.status, ParseStatus::Parsed, "{text}");
    assert_eq!(o.mods.len(), names.len(), "{text}: {:?}", o.mods);
    for (m, name) in o.mods.iter().zip(names) {
        assert_eq!(m.name, ModName::from(*name), "{text}");
        assert_eq!(m.mod_type, ModType::Flag, "{text}");
        assert_eq!(m.value, ModValue::Bool(true), "{text}");
    }
}

/// `Armour applies to Fire, Cold and Lightning Damage taken from Hits instead
/// of Physical Damage`（ModParser.lua:2545-2550）→ 三元素
/// `ArmourAppliesTo<X>DamageTaken` BASE 100 + `ArmourDoesNotApplyToPhysicalDamageTaken`
/// flag。消费端 taken.rs armour_applies_pct。
#[test]
fn parses_armour_applies_to_fcl_instead_of_physical() {
    let o = parse_mod(
        "Armour applies to Fire, Cold and Lightning Damage taken from Hits instead of Physical Damage",
    )
    .unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 4, "{:?}", o.mods);
    for (m, el) in o.mods.iter().zip(["Fire", "Cold", "Lightning"]) {
        assert_eq!(
            m.name,
            ModName::from(format!("ArmourAppliesTo{el}DamageTaken"))
        );
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value, ModValue::Number(100.0));
    }
    assert_eq!(
        o.mods[3].name,
        ModName::from("ArmourDoesNotApplyToPhysicalDamageTaken")
    );
    assert_eq!(o.mods[3].mod_type, ModType::Flag);
}

/// `N% of Armour applies to Fire, Cold and Lightning Damage taken from Hits`
/// （ModParser.lua:2551-2555）→ 三元素 BASE N（无 instead flag）。
#[test]
fn parses_pct_of_armour_applies_to_fcl() {
    let o = parse_mod("50% of Armour applies to Fire, Cold and Lightning Damage taken from Hits")
        .unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 3, "{:?}", o.mods);
    for (m, el) in o.mods.iter().zip(["Fire", "Cold", "Lightning"]) {
        assert_eq!(
            m.name,
            ModName::from(format!("ArmourAppliesTo{el}DamageTaken"))
        );
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value, ModValue::Number(50.0));
    }
}

/// `Armour applies to Elemental Damage`（ModParser.lua:2556-2560）与
/// `+N% of Armour also applies to Elemental Damage`（:2561-2565）→ 三元素 BASE。
#[test]
fn parses_armour_applies_to_elemental_damage_variants() {
    for (text, expect) in [
        ("Armour applies to Elemental Damage", 100.0),
        ("+30% of Armour also applies to Elemental Damage", 30.0),
        ("25% of Armour applies to Elemental Damage", 25.0),
    ] {
        let o = parse_mod(text).unwrap();
        assert_eq!(o.status, ParseStatus::Parsed, "{text}");
        assert_eq!(o.mods.len(), 3, "{text}: {:?}", o.mods);
        for (m, el) in o.mods.iter().zip(["Fire", "Cold", "Lightning"]) {
            assert_eq!(
                m.name,
                ModName::from(format!("ArmourAppliesTo{el}DamageTaken")),
                "{text}"
            );
            assert_eq!(m.value, ModValue::Number(expect), "{text}");
        }
    }
}

/// `Energy Shield protects Mana instead of Life`（Eldritch Battery 类，
/// ModParser.lua:2465）→ `EnergyShieldProtectsMana` FLAG。消费端
/// keystone_registry.rs DefenceKeystones → pool_damage/ehp/defence。
#[test]
fn parses_energy_shield_protects_mana() {
    assert_flags(
        "Energy Shield protects Mana instead of Life",
        &["EnergyShieldProtectsMana"],
    );
}

/// `Converts all Evasion Rating to Armour`（Iron Reflexes 类，ModParser.lua:2369）
/// → `IronReflexes` FLAG + `EvasionConvertToArmour` BASE 100。消费端
/// defence.rs 五元 ConvertTo 矩阵 + keystone_registry（Unbreakable 联动）。
#[test]
fn parses_converts_all_evasion_rating_to_armour() {
    let o = parse_mod("Converts all Evasion Rating to Armour").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 2, "{:?}", o.mods);
    assert_eq!(o.mods[0].name, ModName::from("IronReflexes"));
    assert_eq!(o.mods[0].mod_type, ModType::Flag);
    assert_eq!(o.mods[1].name, ModName::from("EvasionConvertToArmour"));
    assert_eq!(o.mods[1].mod_type, ModType::Base);
    assert_eq!(o.mods[1].value, ModValue::Number(100.0));
}

/// `Chance to Deflect is Lucky`（ModParser.lua:4202）→ `DeflectIsLucky` FLAG。
/// 消费端 defence_panels.rs 偏斜几率 lucky 幂。
#[test]
fn parses_chance_to_deflect_is_lucky() {
    assert_flags("Chance to Deflect is Lucky", &["DeflectIsLucky"]);
}

/// `Chance to Block Damage is Lucky`（ModParser.lua:4371）单 flag；
/// `(Your )?Chance to Block is Lucky`（:4372）四 flag 全套。消费端
/// defence_panels.rs effective(BlockChance/SpellBlockChance)；
/// Projectile/SpellProjectile 两 flag 暂无消费端（形态对齐 vendor）。
#[test]
fn parses_chance_to_block_is_lucky_variants() {
    assert_flags("Chance to Block Damage is Lucky", &["BlockChanceIsLucky"]);
    let all_four = [
        "BlockChanceIsLucky",
        "ProjectileBlockChanceIsLucky",
        "SpellBlockChanceIsLucky",
        "SpellProjectileBlockChanceIsLucky",
    ];
    assert_flags("Your Chance to Block is Lucky", &all_four);
    assert_flags("Chance to Block is Lucky", &all_four);
}
