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

    let o = parse_mod("+5 to any [Attributes|Attribute]").unwrap();
    assert_eq!(o.mods.len(), 3, "any attribute → str/dex/int");
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
