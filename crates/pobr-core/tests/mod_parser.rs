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
