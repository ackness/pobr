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

    // `+N to <stat> per M <resource>` parses to a plain stat mod + Multiplier{var, div=M}
    // tag (PoB2's PerStat).
    let outcome = parse_mod("+2 to Armour per 1 Spirit").unwrap();
    let modifier = &outcome.mods[0];
    assert_eq!(modifier.name, ModName::from("Armour"));
    assert_eq!(modifier.mod_type, ModType::Base);
    assert!(
        modifier
            .tags
            .contains(&ModTag::multiplier("Spirit", 1.0, None))
    );
    // effective_number expands via cfg.multipliers[Spirit] / div: 336 Spirit -> 2 * 336 = 672.
    let cfg = CalcConfig::new().with_multiplier("Spirit", 336.0);
    assert_eq!(modifier.effective_number(&cfg), Some(672.0));
}

#[test]
fn parses_per_n_attribute_scaling() {
    use pobr_core::CalcConfig;

    // `per 10 Intelligence` -> div=10: 100 Int -> 5 * (100/10) = 50.
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
    // `per Strength` (no number) -> div=1.
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
    // The engine never errors on unrecognized lines -- the whole line becomes
    // Unsupported, with the original text preserved in unparsed.
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
    // [internal|display] -> display name; expands compounds and aggregates.
    let o = parse_mod("15% increased [Evasion|Evasion Rating]").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("Evasion"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);

    let o = parse_mod("12% increased [ElementalDamage|Elemental] Damage with [Attack|Attacks]")
        .unwrap();
    assert_eq!(o.mods[0].name, ModName::from("ElementalDamage"));

    // `any attribute` (the tree's pick-one-of-three attribute node) is not expanded
    // here -- the player's choice gets rewritten to a concrete attribute by
    // AttributeOverride during tree collection, before parsing. The "no net
    // contribution" shape varies by data version: older rules leave a residue
    // (dropped whole-line by the production gate), the 4.5.4.3 vendor rules
    // recognize the whole line as zero mods. Both are net-equivalent, so we only
    // assert the version-independent invariant: either zero mods or a residue,
    // never a concrete attribute mod.
    let o = parse_mod("+5 to any [Attributes|Attribute]").unwrap();
    assert!(
        o.mods.is_empty() || o.unparsed.is_some(),
        "any attribute 原文不得产出净贡献（零 mod 或留残丢弃）：{:?}",
        o.mods
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

/// Multi-source combined converted-to (vendor ModParser.lua:2405-2409 specialModList).
/// The generic suffix path only consumes the lightning branch and leaves a
/// `Physical, Cold and` residue -- the special channel must consume the whole
/// line and produce one ConvertToFire BASE mod per source.
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

/// Whole-line converted-to with no explicit number (vendor ModParser.lua:2410-2414):
/// all three elements get 100.
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

/// Generic "damage taken as" form (modNameList `<src> damage taken` + suffixTypes
/// `as <dst> damage`; consumed by calc/taken.rs as `<Src>DamageTakenAs<Dst>`).
/// Pins down that the whole line gets consumed with no residue.
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

/// Bare-target taken-as (vendor ModParser.lua:5655-5656 specialModList): suffixTypes
/// has no bare `as lightning` entry, so the generic path would wrongly produce
/// `<Src>DamageTaken BASE` plus a leftover `as Lightning` -- the special channel
/// must take over.
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

/// Flask dual-source taken-as (vendor ModParser.lua:5657-5660): fire and lightning
/// each produce one FromHitsTakenAsCold mod, carrying `Condition: UsingFlask`.
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

/// Random element damage taken (vendor ModParser.lua:5661-5665): AVERAGE splits the
/// value three ways (num/3). Real-world corpus text (`5% of Physical Damage from Hits
/// taken as Damage of a Random Element`) regressed to `PhysicalDamageFromHitsTaken
/// BASE 5` plus a residue after the legacy path was removed.
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

/// Aggregated-source gain-as (vendor ModParser.lua:702 `["elemental damage"] =
/// "ElementalDamage"` + suffix table `:6173` -> `ElementalDamageGainAsCold`; verified
/// via ModCache against `Gain 10% of Elemental Damage as Extra Cold Damage`; matches
/// the druid-oracle ember-fusillade Storm Bane/Blood Barrier mods).
#[test]
fn parses_elemental_source_gain_as_extra() {
    let o = parse_mod("Gain 16% of Elemental Damage as Extra Cold Damage").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("ElementalDamageGainAsCold"));
    assert_eq!(o.mods[0].mod_type, ModType::Base);
    assert_eq!(o.mods[0].value, ModValue::Number(16.0));
}

/// Random-element gain-as tier (vendor ModParser.lua:6182 `["as extra damage of a
/// random element"] = "GainAsRandom"`; ModCache.lua:5257 `Gain 5% of Damage as Extra
/// Damage of a random Element` -> `DamageGainAsRandom BASE 5`; consumed by
/// CalcOffence.lua:1175-1200's physMode expansion).
#[test]
fn parses_random_element_gain_as() {
    let o = parse_mod("Gain 5% of Damage as Extra Damage of a random Element").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("DamageGainAsRandom"));
    assert_eq!(o.mods[0].value, ModValue::Number(5.0));

    // Weapon-phys variant (ModParser.lua:3691 `gain N% of physical damage as extra
    // damage of a random element` -> PhysicalDamageGainAsRandom).
    let o = parse_mod("Gain 10% of Physical Damage as Extra Damage of a random Element").unwrap();
    assert_eq!(o.mods[0].name, ModName::from("PhysicalDamageGainAsRandom"));
}

/// Per-curse scaling suffix (vendor ModParser.lua:1507-1510 -> `Multiplier:CurseOnEnemy`,
/// multiplier = `#curseSlots` per CalcPerform.lua:2969); witch-blood-mage's Liminal Coil
/// mod `Spell Hits Gain 30% of Damage as Extra Physical Damage per Curse on target` ->
/// raw 30 x 5 curses = 150, matching vendor's measured value.
#[test]
fn parses_gain_as_per_curse_with_spell_hits_prefix() {
    let o = parse_mod("Spell Hits Gain 30% of Damage as Extra Physical Damage per Curse on target")
        .unwrap();
    assert_eq!(o.mods.len(), 1);
    let m = &o.mods[0];
    assert_eq!(m.name, ModName::from("DamageGainAsPhysical"));
    assert_eq!(m.mod_type, ModType::Base);
    assert_eq!(m.value, ModValue::Number(30.0));
    // vendor `^spell hits [ghd][ae][iva][eln] ` -> flags=Hit, keywordFlags=Spell
    // (ModParser.lua:1273); PoBR has no separate Spell keyword bit, so this collapses
    // to the equivalent HIT|SPELL ModFlags.
    assert_eq!(m.flags, ModFlags::HIT | ModFlags::SPELL);
    assert!(
        m.tags
            .iter()
            .any(|t| matches!(t, ModTag::Multiplier { var, .. } if var == "CurseOnEnemy")),
        "应携带 Multiplier:CurseOnEnemy tag"
    );

    // Multiplier evaluation: 5 curse slots -> 30 x 5 = 150 (pinned to the vendor witch
    // oracle value).
    let cfg = CalcConfig::new()
        .with_flags(ModFlags::HIT | ModFlags::SPELL)
        .with_multiplier("CurseOnEnemy", 5.0);
    assert_eq!(m.effective_number(&cfg), Some(150.0));
}

/// Per-different-grenade suffix (vendor ModParser.lua:1528 ->
/// `Multiplier:DifferentGrenadeFired`, limitVar=GrenadeTypes); mercenary's
/// Demolitionist tree node `Gain 4% of Damage as Extra Fire Damage for every
/// different Grenade fired in the past 8 seconds`.
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
    // "against Rare or Unique Enemies" is a conditional damage bonus (set true via
    // cfg when computing DPS against a boss).
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
    // PoB2 ModParser modNameList: `fire and chaos resistances` -> fire resistance +
    // chaos resistance.
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
    // `all resistances` (includes chaos) is distinct from `all elemental resistances`
    // (excludes chaos).
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
    // The `... for Spells` suffix scope maps to the SPELL flag (leaving it unstripped
    // would break resolve_names).
    let o = parse_mod("+15% to Critical Hit Chance for Spells").unwrap();
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("CriticalStrikeChance"));
    assert!(o.mods[0].flags.intersects(ModFlags::SPELL));
    assert!(!o.mods[0].flags.intersects(ModFlags::ATTACK));
}

#[test]
fn strips_scope_prefix_attack_critical_into_flag() {
    // The `Attack Critical Hit Chance` prefix scope maps to the ATTACK flag.
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
    // Chaos Inoculation keystone: `Maximum Life is 1` -> MaximumLife OVERRIDE 1 +
    // ChaosInoculation flag.
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
    // A pure immunity phrase carries no numeric value: it produces no mods (the engine
    // recognizes it as an empty-output line rather than erroring), avoiding noise.
    let o = parse_mod("Immune to Chaos Damage and Bleeding").unwrap();
    assert!(o.mods.is_empty());
}

#[test]
fn weapon_type_guard_keeps_damage_as_weapon_keyword_name() {
    // "damage with crossbows" must map to the weapon-class damage name CrossbowDamage
    // (keyword aggregation), not get misconverted by the weapon-type condition guard
    // into a generic Damage mod gated on UsingCrossbow (which would drop weapon damage).
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
    // "Archon Buffs also grant +20% to all Elemental Resistances" -- vendor doesn't
    // support this mod family (ModCache.lua:4527-4532 caches it as nil); the panel
    // should not inject any numeric value (the stormweaver FireResist golden value of
    // 71 confirms PoB2 does not apply the +20).
    let outcome = parse_mod("Archon Buffs also grant +20% to all Elemental Resistances").unwrap();
    assert_eq!(outcome.status, ParseStatus::Unsupported);
    assert!(outcome.mods.is_empty());
}

#[test]
fn parses_increased_armour_from_equipped_body_armour_as_slot_tag() {
    // Titan's `80% increased Armour from Equipped Body Armour`: the slot clause is
    // stripped off, yielding a plain Armour INC mod + SlotName("bodyarmour") tag
    // (per-slot defence aggregation applies it to that slot).
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
    // Focus (offhand caster weapon) lives in the weapon2 slot: `44% increased Energy
    // Shield from Equipped Focus` -> EnergyShield INC + SlotName("weapon2").
    let outcome = parse_mod("44% increased Energy Shield from Equipped Focus").unwrap();
    assert_eq!(outcome.status, ParseStatus::Parsed);
    let m = &outcome.mods[0];
    assert_eq!(m.name, ModName::from("EnergyShield"));
    assert_eq!(m.slot_name(), Some("weapon2"));
}

#[test]
fn parses_more_armour_from_equipped_body_armour_as_slot_more() {
    // `50% more Armour from Equipped Body Armour` -> Armour MORE + SlotName(bodyarmour).
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
    // Global inc + slot-scoped base + slot-scoped inc.
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
    // global_only excludes the slot-scoped inc.
    assert_eq!(db.sum_global_only(ModType::Inc, &cfg, &names), 100.0);
    // for_slot only picks up that slot's inc.
    assert_eq!(
        db.sum_for_slot(ModType::Inc, &cfg, &names, "bodyarmour"),
        80.0
    );
    // slot_bases retrieves that slot's base.
    assert_eq!(
        db.slot_bases(&cfg, &ModName::from("Armour")),
        vec![("bodyarmour".to_string(), 1000.0)]
    );
}

/// 01-03: PoB2's EvalMod applies `m_floor(base/div + 0.0001)` to Multiplier/PerStat
/// ratios, so non-integral resource counts must round down. `+5 ... per 10
/// Intelligence` at 95 Int -> floor(9.5)=9 -> 45 (not 47.5).
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
    // 95 Int -> floor(9.5 + 0.0001) = 9 -> 5 * 9 = 45.
    let cfg_non_integral = CalcConfig::new().with_multiplier("Intelligence", 95.0);
    assert_eq!(modifier.effective_number(&cfg_non_integral), Some(45.0));
    // 100 Int -> floor(10.0001) = 10 -> 50 (an exact multiple is unaffected by floor).
    let cfg_integral = CalcConfig::new().with_multiplier("Intelligence", 100.0);
    assert_eq!(modifier.effective_number(&cfg_integral), Some(50.0));
}

/// `Bonded: <mod>` rune mods are inactive by default -- the whole mod carries a
/// `CanUseBondedModifiers` condition (PoB2 ModParser `["^bonded: "]`), and only
/// activates via cfg when an enabling source grants the corresponding flag.
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

    // Condition unset -> excluded from aggregation; set true -> takes effect.
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

/// Bonded-enabling line (Druid Oracle ascendancy, ModParser.lua:3423-3424) ->
/// `Condition:CanUseBondedModifiers` FLAG (special `bonded_modifiers_enabler`).
/// Consumed at orchestration layer calc_orchestrator step 6d: session.has_flag ->
/// set_condition, which unlocks the `Bonded:`-prefixed mods tested above.
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

/// Belt implicit "Has N Charm Slot(s)" / tree "+N Charm Slot" -> `CharmLimit` BASE N
/// (vendor ModParser.lua:5453 `h?a?s? ?+?(%d+) charm slots?`). With no CharmLimit
/// source, env_finalize stage 3's charm budget is 0 (no charms take effect at all) --
/// this mod is the primary source that unlocks charms.
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
    // A non-charm-slot mod must not be mismatched.
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

// commit-2 / switchover: weapon-suffix bit channel (dual-write of condition + weapon bits)

/// End-to-end parse -> match coverage for `with Maces` / `with One Handed Melee
/// Weapons` / `with Unarmed Attacks`.
mod weapon_bits_e2e {
    use super::*;
    use pobr_core::CalcConfig;

    /// `with Maces`: dual-writes the MACE bit + UsingMace condition; matches cfg only
    /// when both the weapon bit and the condition are present.
    #[test]
    fn with_maces_parses_and_matches_per_weapon_bits() {
        let o = parse_mod("10% increased Attack Speed with Maces").unwrap();
        let m = &o.mods[0];
        assert!(m.tags.contains(&ModTag::condition("UsingMace", false)));
        assert!(m.flags.is_subset_of(ModFlags::MACE));
        assert_eq!(m.flags.bits(), ModFlags::MACE.bits());

        // cfg = wielding a one-handed mace (per the orchestration layer's
        // weapon_cfg_flags derivation) + UsingMace condition.
        let mace_cfg = CalcConfig::attack()
            .with_flags(
                ModFlags::ATTACK | ModFlags::weapon_flags("One Hand Mace", "Mace", true, true),
            )
            .with_condition("UsingMace", true);
        assert!(m.matches(&mace_cfg));

        // Wielding a bow: both the bit and condition channels consistently reject.
        let bow_cfg = CalcConfig::attack()
            .with_flags(ModFlags::ATTACK | ModFlags::weapon_flags("Bow", "Bow", false, false));
        assert!(!m.matches(&bow_cfg));
    }

    /// `with One Handed Melee Weapons`: Weapon1H|WeaponMelee bits (vendor :1017).
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

        // Two-handed weapon: Weapon1H bit is absent -> rejected.
        let th_cfg = CalcConfig::attack()
            .with_flags(
                ModFlags::ATTACK | ModFlags::weapon_flags("Two Hand Mace", "Mace", false, true),
            )
            .with_condition("UsingOneHandedMelee", true);
        assert!(!m.matches(&th_cfg));
    }

    /// `with Unarmed Attacks`: UNARMED bit (vendor :1006; an always-present unlock
    /// phrase post-switchover).
    #[test]
    fn with_unarmed_attacks_matches_unarmed_bit() {
        let o = parse_mod("10% increased Attack Speed with Unarmed Attacks").unwrap();
        assert_eq!(o.status, ParseStatus::Parsed);
        let m = &o.mods[0];
        // The engine also carries the Hit scope bit (subset-match semantics
        // unchanged); what matters is that the UNARMED bit is present.
        assert!(ModFlags::UNARMED.is_subset_of(m.flags));

        // Unarmed (the empty-mainhand branch of the orchestration layer's
        // weapon_cfg_flags; production attack contexts always carry the HIT bit).
        let unarmed_cfg = CalcConfig::attack().with_flags(
            ModFlags::ATTACK
                | ModFlags::HIT
                | ModFlags::weapon_flags("None", "Unarmed", true, true),
        );
        assert!(m.matches(&unarmed_cfg));

        // Wielding a weapon -> UNARMED bit absent -> rejected.
        let mace_cfg = CalcConfig::attack().with_flags(
            ModFlags::ATTACK
                | ModFlags::HIT
                | ModFlags::weapon_flags("One Hand Mace", "Mace", true, true),
        );
        assert!(!m.matches(&mace_cfg));
    }
}

/// Curses ignoring the curse limit (vendor ModParser.lua:4275 -> `EnemyCurseLimit
/// BASE 99`; consumed as buff_pass slot budget DEFAULT 1 + sum, matching vendor
/// CalcPerform.lua:2832's formula); witch-blood-mage's Doedre's Undoing mod, vendor
/// measured EnemyCurseLimit = 100.
#[test]
fn parses_curses_ignore_curse_limit() {
    let o = parse_mod("Curses you inflict ignore Curse limit").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("EnemyCurseLimit"));
    assert_eq!(o.mods[0].mod_type, ModType::Base);
    assert_eq!(o.mods[0].value, ModValue::Number(99.0));
}

/// "N% increased Damage for each type of Elemental Ailment on Enemy" (The Taming,
/// vendor ModParser.lua:3798-3804) -> 5 Damage INC mods, each gated on a different
/// enemy-ailment condition (the `Enemy<X>` cfg key space, consistent with the
/// `against <X> enemies` suffix).
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
    // Condition evaluation: Chilled + Ignited true (the twister config shape) -> 2
    // tiers active.
    let cfg = CalcConfig::attack()
        .with_condition("EnemyChilled", true)
        .with_condition("EnemyIgnited", true);
    let active = o.mods.iter().filter(|m| m.matches(&cfg)).count();
    assert_eq!(active, 2);
}

/// Companion-in-presence condition suffix (vendor ModParser.lua:1803 ->
/// CompanionInPresence).
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

/// Arcane Surge condition suffix (vendor ModParser.lua:1817 -> AffectedByArcaneSurge).
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

/// "N% chance to Gain Arcane Surge when you deal a Critical Hit" ->
/// `Condition:ArcaneSurge` FLAG (the vendor FLAG form ignores the chance value,
/// ModParser.lua:92/:4197; the trigger suffix :1902 -> CritRecently condition).
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

/// "Damage with One Handed Weapons" (vendor ModParser.lua:1016 `bor(Weapon1H, Hit)`)
/// -> name=Damage + Weapon1H|Hit bits; matches when cfg carries the weapon bit
/// (one-handed spear) + HIT, rejects when the two-handed bit is absent.
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

    // Two-handed variant (vendor :1018).
    let o2 = parse_mod("10% increased Damage with Two Handed Weapons").unwrap();
    assert_eq!(
        o2.mods[0].flags.bits(),
        (ModFlags::WEAPON_2H | ModFlags::HIT).bits()
    );
}

/// "Projectile Speed" name unlock (vendor ModName `ProjectileSpeed`) -- normally has
/// no direct consumer; feeds into the `ProjectileSpeedAppliesToProjectileDamage`
/// conversion (CalcOffence.lua:840-845).
#[test]
fn parses_projectile_speed_name() {
    let o = parse_mod("8% increased [Projectile] Speed").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods[0].name, ModName::from("ProjectileSpeed"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);
}

/// "N% increased [Exposure] Effect" (tree Exposure Effect notable = 10 /
/// Overexposure = 30) -> expands to three elements' `<El>ExposureEffect` INC
/// (vendor ModParser.lua:693 `["exposure effect"] = { "FireExposureEffect",
/// "ColdExposureEffect", "LightningExposureEffect" }`). Consumed by
/// `calc::reduce_enemy_exposure` (CalcPerform.lua:3223's player-effect INC on
/// exposure magnitude).
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

/// Single-element exposure effect (vendor ModParser.lua:690-692 `fire/cold/lightning
/// exposure effect`) -> a single `<El>ExposureEffect` mod.
#[test]
fn parses_single_element_exposure_effect() {
    let o = parse_mod("25% increased Cold Exposure Effect").unwrap();
    assert_eq!(o.status, ParseStatus::Parsed);
    assert_eq!(o.mods.len(), 1);
    assert_eq!(o.mods[0].name, ModName::from("ColdExposureEffect"));
    assert_eq!(o.mods[0].mod_type, ModType::Inc);
    assert_eq!(o.mods[0].value, ModValue::Number(25.0));
}

// Minion mod wrapping (engine MinionModifier LIST channel +
// extract_minion_modifier_entries consumer)

/// Combines engine parsing with MinionModifier extraction (mirrors the production
/// consumer path).
fn minion_entries(text: &str) -> Vec<pobr_core::calc::minion::MinionModifierEntry> {
    let outcome = parse_mod(text).unwrap();
    pobr_core::calc::minion::extract_minion_modifier_entries(&outcome.mods)
}

/// `Minions deal X% increased Damage` -> the inner `20% increased damage` parses to
/// Damage INC 20, wrapped as a MinionModifierEntry.
#[test]
fn parses_minion_increased_damage_wrapper() {
    let entries = minion_entries("Minions deal 20% increased Damage");
    assert_eq!(entries.len(), 1);
    let inner = &entries[0].inner;
    assert_eq!(inner.name, ModName::from("Damage"));
    assert_eq!(inner.mod_type, ModType::Inc);
    assert_eq!(inner.value, ModValue::Number(20.0));
    // minion_type=None -> applies to all minions.
    assert!(entries[0].minion_type.is_none());
}

/// `Minions have X% increased maximum Life` -> wraps a Life INC mod.
#[test]
fn parses_minion_increased_life_wrapper() {
    let entries = minion_entries("Minions have 30% increased maximum Life");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].inner.mod_type, ModType::Inc);
    assert_eq!(entries[0].inner.value, ModValue::Number(30.0));
}

/// Text without a `minions ` prefix -> no MinionModifier (player mods aren't
/// misclassified as minion mods).
#[test]
fn non_minion_text_yields_no_entries() {
    assert!(minion_entries("20% increased Fire Damage").is_empty());
    assert!(minion_entries("Minions ").is_empty()); // empty remainder
}

/// `Minions ...` with an unparsable remainder -> no entry (never produces an empty
/// entry).
#[test]
fn minion_unparsable_remainder_yields_no_entries() {
    assert!(minion_entries("Minions wibble wobble zorp").is_empty());
}

// wave2-defence: defence-side special mods (special_mods `wave2-defence` batch;
// backfilled as engine-form pins after the legacy-only coverage in
// mod_parser_m2_defence.rs was removed, matching vendor ModParser.lua's
// specialModList output)

/// Asserts the outcome is exactly a set of FLAG mods, in order.
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

/// `Armour applies to Fire, Cold and Lightning Damage taken from Hits instead of
/// Physical Damage` (ModParser.lua:2545-2550) -> three-element
/// `ArmourAppliesTo<X>DamageTaken` BASE 100 mods + the
/// `ArmourDoesNotApplyToPhysicalDamageTaken` flag. Consumed by taken.rs's
/// armour_applies_pct.
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
/// (ModParser.lua:2551-2555) -> three-element BASE N mods (no "instead" flag).
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

/// `Armour applies to Elemental Damage` (ModParser.lua:2556-2560) and
/// `+N% of Armour also applies to Elemental Damage` (:2561-2565) -> three-element
/// BASE mods.
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

/// `Energy Shield protects Mana instead of Life` (the Eldritch Battery family,
/// ModParser.lua:2465) -> `EnergyShieldProtectsMana` FLAG. Consumed by
/// keystone_registry.rs's DefenceKeystones -> pool_damage/ehp/defence.
#[test]
fn parses_energy_shield_protects_mana() {
    assert_flags(
        "Energy Shield protects Mana instead of Life",
        &["EnergyShieldProtectsMana"],
    );
}

/// `Converts all Evasion Rating to Armour` (the Iron Reflexes family,
/// ModParser.lua:2369) -> `IronReflexes` FLAG + `EvasionConvertToArmour` BASE 100.
/// Consumed by defence.rs's five-way ConvertTo matrix + keystone_registry
/// (interacts with Unbreakable).
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

/// `Chance to Deflect is Lucky` (ModParser.lua:4202) -> `DeflectIsLucky` FLAG.
/// Consumed by defence_panels.rs's deflect-chance lucky-roll math.
#[test]
fn parses_chance_to_deflect_is_lucky() {
    assert_flags("Chance to Deflect is Lucky", &["DeflectIsLucky"]);
}

/// `Chance to Block Damage is Lucky` (ModParser.lua:4371) is a single flag;
/// `(Your )?Chance to Block is Lucky` (:4372) is the full set of four flags.
/// Consumed by defence_panels.rs's effective(BlockChance/SpellBlockChance);
/// the Projectile/SpellProjectile flags currently have no consumer (kept only
/// to match the vendor shape).
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
