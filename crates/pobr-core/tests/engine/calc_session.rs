use pobr_core::calc::MinimalInput;

use crate::support::session;

#[test]
fn per_attribute_modifiers_use_final_attributes_with_class_and_inc_more() {
    use pobr_core::{CalcConfig, CharacterBase, Modifier};
    use pobr_data::prelude::ModType;

    let class = CharacterBase {
        level: 1,
        strength: 10.0,
        dexterity: 10.0,
        intelligence: 10.0,
    };
    let mut s = session(MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        ..Default::default()
    })
    .with_config(CalcConfig::spell());
    s.add_modifiers(class.modifiers(&Default::default()));
    for attribute in ["Strength", "Dexterity", "Intelligence"] {
        s.add_modifiers([
            Modifier::number(attribute, ModType::Base, 10.0),
            Modifier::number(attribute, ModType::Inc, 50.0),
            Modifier::number(attribute, ModType::More, 20.0),
        ]);
    }
    // Real passive/item wording: final attributes are (10 + 10) * 1.5 * 1.2 = 36.
    s.add_modifier_texts([
        "2% increased Spell Damage per 10 Strength",
        "+1 Life per 4 Dexterity",
        "+5 to maximum Mana per 10 Intelligence",
    ])
    .unwrap();
    assert!(s.unsupported_modifier_texts().is_empty());
    s.prepare_player_stats(1, Some(class));
    assert_eq!(s.pool_total("MaximumLife"), 109.0); // 28 + 36 * 2 + floor(36 / 4)
    assert_eq!(s.pool_total("MaximumMana"), 121.0); // 34 + 36 * 2 + floor(36 / 10) * 5
    let out = s.perform_minimal().unwrap();
    assert_eq!(out.life, 109.0);
    assert_eq!(out.mana, 121.0);
    assert!((out.total_hit_avg - 106.0).abs() < 1e-9);
}

/// Jack of all Trades: "2% increased Damage per 5 of your lowest Attribute"
/// reads the least of the three final attributes.
#[test]
fn per_lowest_attribute_modifiers_read_the_least_final_attribute() {
    use pobr_core::{CalcConfig, CharacterBase, Modifier};
    use pobr_data::prelude::ModType;

    let class = CharacterBase {
        level: 1,
        strength: 10.0,
        dexterity: 10.0,
        intelligence: 10.0,
    };
    let mut s = session(MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        ..Default::default()
    })
    .with_config(CalcConfig::spell());
    s.add_modifiers(class.modifiers(&Default::default()));
    // Final attributes: 40 Strength, 26 Dexterity, 30 Intelligence.
    s.add_modifiers([
        Modifier::number("Strength", ModType::Base, 30.0),
        Modifier::number("Dexterity", ModType::Base, 16.0),
        Modifier::number("Intelligence", ModType::Base, 20.0),
    ]);
    s.add_modifier_texts(["2% increased Damage per 5 of your lowest Attribute"])
        .unwrap();
    assert!(s.unsupported_modifier_texts().is_empty());
    s.prepare_player_stats(1, Some(class));
    let unprovided = s.unprovided_multipliers();
    assert!(unprovided.iter().all(|(var, _)| *var != "LowestAttribute"));
    // floor(26 / 5) = 5 steps of 2%.
    let out = s.perform_minimal().unwrap();
    assert!((out.total_hit_avg - 110.0).abs() < 1e-9);
}

/// "for each Minion in your Presence" counts the minions the calculation
/// created, or the configured "# of Minions in your Presence".
#[test]
fn minions_in_presence_are_the_minions_created_or_the_configured_number() {
    use pobr_core::calc::minion::AttributeInfusion;
    use pobr_core::{CalcConfig, ModTag, Modifier};
    use pobr_data::minion::minion_def_zombie;
    use pobr_data::prelude::ModType;

    let attack_hit = |zombies: u32, configured: Option<f64>| {
        let mut s = session(MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            ..Default::default()
        })
        .with_config(CalcConfig::attack().with_mode_combat(true));
        s.add_modifier_texts([
            "5% increased Attack Damage for each Minion in your Presence, up to a maximum of 80%",
        ])
        .unwrap();
        assert!(s.unsupported_modifier_texts().is_empty());
        if let Some(count) = configured {
            // What the config option adds: a combat input.
            let name = "Multiplier:MinionPresenceCount";
            let modifier = Modifier::number(name, ModType::Override, count)
                .with_tag(ModTag::condition("Combat", false));
            s.add_modifiers([modifier]);
        }
        s.prepare_player_stats(1, None);
        if zombies > 0 {
            s.add_minion_from_def(
                &minion_def_zombie(),
                20,
                zombies,
                Vec::new(),
                Vec::new(),
                AttributeInfusion::default(),
                false,
            );
        }
        s.bridge_minion_presence();
        s.perform_minimal().unwrap().total_hit_avg
    };
    // Three Raised Zombies: 15%.
    assert!((attack_hit(3, None) - 115.0).abs() < 1e-9);
    // Twenty reach the maximum of 80%.
    assert!((attack_hit(20, None) - 180.0).abs() < 1e-9);
    assert!((attack_hit(0, None) - 100.0).abs() < 1e-9);
    // The configured number is the count.
    assert!((attack_hit(3, Some(5.0)) - 125.0).abs() < 1e-9);
}

#[test]
fn completed_session_rejects_recalculation_without_mutating_derived_resources() {
    use pobr_core::{Modifier, calc::CalcError};
    use pobr_data::prelude::ModType;
    let mut s = session(MinimalInput {
        base_life: 100.0,
        base_mana: 100.0,
        ..Default::default()
    });
    s.add_modifiers([
        Modifier::number("EnergyShield", ModType::Base, 200.0),
        Modifier::number("EnergyShieldConvertToMana", ModType::Base, 100.0),
    ]);
    s.prepare_player_stats(1, None);
    assert_eq!(s.perform_minimal().unwrap().mana, 300.0);
    let output = s.output().clone();
    let modifier_count = s.all_mods().len();
    assert_eq!(s.perform_minimal(), Err(CalcError::AlreadyPerformed));
    assert_eq!(s.output(), &output);
    assert_eq!(s.all_mods().len(), modifier_count);
}

#[test]
fn player_preparation_preserves_attribute_bonus_rules_and_sources() {
    use pobr_core::{CharacterBase, Modifier};
    use pobr_data::prelude::ModType;

    let class = CharacterBase {
        level: 1,
        strength: 10.0,
        dexterity: 0.0,
        intelligence: 5.0,
    };
    for (flag, life, mana) in [
        (None, 88.0, 84.0),
        (Some("HalvesLifeFromStrength"), 58.0, 84.0),
        (Some("NoAttributeBonuses"), 28.0, 34.0),
    ] {
        let mut session = session(MinimalInput::default());
        session.add_modifiers(class.modifiers(&Default::default()));
        session.add_modifiers([
            Modifier::number("Strength", ModType::Base, 10.0),
            Modifier::number("Strength", ModType::Inc, 50.0),
            Modifier::number("Intelligence", ModType::Base, 20.0),
        ]);
        if let Some(flag) = flag {
            session.add_modifiers([Modifier::flag(flag)]);
        }
        session.prepare_player_stats(class.level, Some(class));
        let output = session.perform_minimal().expect("perform");
        assert_eq!(output.life, life, "{flag:?}");
        assert_eq!(output.mana, mana, "{flag:?}");
        assert!(session.mods_named("MaximumLife").iter().any(|m| {
            m.origin
                .as_ref()
                .is_some_and(|source| source.source_id.id == "base.attr_derived")
        }));
    }
}

#[test]
fn player_condition_bridge_preserves_explicit_low_life_choice() {
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::prelude::ModType;

    for explicit in [None, Some(false), Some(true)] {
        let mut cfg = CalcConfig::default();
        if let Some(value) = explicit {
            cfg = cfg.with_condition("LowLife", value);
        }
        let mut session = session(MinimalInput {
            base_life: 100.0,
            ..Default::default()
        })
        .with_config(cfg);
        session.add_modifiers([Modifier::number("LifeReserved", ModType::Base, 70.0)]);
        session
            .add_modifier_texts(["+20 to maximum Mana while on Low Life"])
            .unwrap();
        session.prepare_player_stats(1, None);
        session.bridge_player_conditions();
        assert_eq!(
            session.pool_total("MaximumMana"),
            if explicit == Some(false) { 0.0 } else { 20.0 }
        );
    }
}

/// Five enemies make a character Surrounded before "Require N fewer enemies to
/// be Surrounded"; the calculation is against one, so only a requirement
/// lowered to a single enemy makes it Surrounded. A configured choice is kept.
#[test]
fn player_condition_bridge_derives_surrounded_from_the_required_enemies() {
    use pobr_core::CalcConfig;

    for (fewer, configured, surrounded) in [
        (Some(4), None, true),
        (Some(3), None, false),
        (None, None, false),
        (Some(4), Some(false), false),
        (None, Some(true), true),
    ] {
        let mut cfg = CalcConfig::default();
        if let Some(value) = configured {
            cfg = cfg.with_condition("Surrounded", value);
        }
        let mut s = session(MinimalInput::default()).with_config(cfg);
        let lines = fewer
            .map(|n| format!("Require {n} fewer enemies to be Surrounded"))
            .into_iter()
            .chain(["20% increased Armour while Surrounded".to_string()]);
        s.add_modifier_texts(lines).unwrap();
        assert!(
            s.unsupported_modifier_texts().is_empty(),
            "{:?}",
            s.unsupported_modifier_texts()
        );
        s.prepare_player_stats(1, None);
        s.bridge_player_conditions();
        let (_, increased, _) = s.stat_totals("Armour");
        assert_eq!(
            increased,
            if surrounded { 20.0 } else { 0.0 },
            "{fewer:?} fewer, configured {configured:?}"
        );
    }
}

/// Sorcery Ward is an Ascendancy skill: "Grants Skill: Sorcery Ward" (the
/// Witchhunter's Obsessive Rituals) gives it to the character, and its
/// Barrier reads the condition that line sets. A skill group that lists the
/// gem without the grant gives no Barrier.
#[test]
fn player_condition_bridge_sets_sorcery_ward_from_its_grant() {
    use pobr_core::{ModTag, Modifier};
    use pobr_data::prelude::ModType;

    for granted in [false, true] {
        let mut s = session(MinimalInput::default());
        if granted {
            s.add_modifier_texts(["Grants Skill: Sorcery Ward"])
                .unwrap();
        }
        s.add_modifiers([Modifier::number("Armour", ModType::Inc, 20.0)
            .with_tag(ModTag::condition("SorceryWard", false))]);
        s.prepare_player_stats(1, None);
        s.bridge_player_conditions();
        let (_, increased, _) = s.stat_totals("Armour");
        assert_eq!(
            increased,
            if granted { 20.0 } else { 0.0 },
            "granted {granted}"
        );
    }
}

#[test]
fn per_mana_damage_uses_the_pool_after_defence_conversion() {
    use pobr_core::{ModTag, Modifier};
    use pobr_data::prelude::ModType;

    let mut session = session(MinimalInput {
        base_life: 100.0,
        base_mana: 100.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        ..Default::default()
    });
    session.add_modifiers([
        Modifier::number("EnergyShield", ModType::Base, 200.0),
        Modifier::number("EnergyShieldConvertToMana", ModType::Base, 100.0),
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::multiplier("Mana", 100.0, None)),
    ]);
    session.prepare_player_stats(1, None);
    assert_eq!(session.pool_total("MaximumMana"), 100.0);
    let output = session.perform_minimal().expect("perform");
    assert_eq!(output.mana, 300.0);
    assert!((output.total_hit_avg - 130.0).abs() < 1e-10);
}

#[test]
fn session_parses_modifier_texts_and_calculates_minimal_output() {
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

    let mut session = session(input);
    session
        .add_modifier_texts([
            "+50 to maximum Life",
            "20% increased maximum Life",
            "+35% to Fire Resistance",
            "Attacks deal 50% increased Physical Damage",
            "20% more Physical Damage",
            "10% increased Attack Speed",
        ])
        .unwrap();

    let output = session.perform_minimal().expect("perform");

    assert_eq!(output.life, 1_260.0);
    assert_eq!(output.fire_resistance, 35.0);
    assert_eq!(output.total_hit_avg, 270.0);
    assert_eq!(output.action_rate, 2.2);
    assert_eq!(output.dps, 594.0);
}

#[test]
fn session_preserves_accuracy_inputs_for_hit_chance_and_dps() {
    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 400.0,
        enemy_evasion: 1_000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };
    let mut session = session(input);
    session
        .add_modifier_texts(["+200 to Accuracy Rating"])
        .unwrap();

    let output = session.perform_minimal().expect("perform");
    let expected_hit_chance = pobr_core::calc::hit_chance(1_000.0, 600.0);

    assert_eq!(output.total_hit_avg, 100.0);
    assert_eq!(output.hit_chance, expected_hit_chance);
    assert_eq!(output.dps, 100.0 * expected_hit_chance);
}

/// Final attribute total (PoB2's `calculateAttributes`, CalcPerform.lua:381-388):
/// `round((class_base + sum_base) x (1 + sum_inc/100) x prod_more)`, floored at 0.
/// `N% increased Dexterity`-style mods must scale the full BASE amount, including
/// the class starting value.
#[test]
fn attribute_total_applies_increased_attribute_modifiers() {
    // Arrange
    let mut session = session(MinimalInput::default());
    session
        .add_modifier_texts(["+100 to Dexterity", "8% increased Dexterity"])
        .unwrap();

    // Act + Assert: round((20 + 100) x 1.08) = round(129.6) = 130.
    assert_eq!(session.attribute_total("Dexterity", 20.0), 130.0);
    // An attribute with no INC mods passes through as class_base + sum_base
    // (Strength has no mods at all).
    assert_eq!(session.attribute_total("Strength", 7.0), 7.0);
}

/// Final resource pool total (vendor's PerStat denominator = actor output,
/// ModStore.lua:440-460): `pool_total` must go through the full
/// base x (1+inc) x more pipeline, sourced identically to the offence pool values
/// computed inside perform -- a BASE-only value (`base_sum`) would miss the
/// inc/more scaling.
#[test]
fn pool_total_applies_full_pool_pipeline() {
    // Arrange
    let input = MinimalInput {
        base_mana: 100.0,
        ..MinimalInput::default()
    };
    let mut session = session(input);
    session
        .add_modifier_texts(["+200 to maximum Mana", "50% increased maximum Mana"])
        .unwrap();

    // Act + Assert: (100 + 200) x 1.5 = 450 (base_sum alone would give 200).
    assert_eq!(session.pool_total("MaximumMana"), 450.0);
    assert_eq!(session.base_sum("MaximumMana"), 200.0);

    // The pool value shares its source with perform's output (the same scaled_pool
    // pipeline).
    let output = session.perform_minimal().expect("perform");
    assert_eq!(output.mana, 450.0);
}

#[test]
fn session_preserves_unsupported_modifier_texts() {
    let mut session = session(MinimalInput::default());
    session.add_modifier_texts(["Mirrored"]).unwrap();

    assert_eq!(session.unsupported_modifier_texts(), ["Mirrored"]);
}

/// A tag-suffix clause (threshold/condition) legitimately consumes the tail and
/// attaches a tag to the mod; even if a cosmetic residue remains, the mod must
/// still be injected -- it must not be blanket-downgraded just because of
/// "Parsed+residue" (this previously broke the RedSupportGems threshold).
#[test]
fn session_injects_tag_suffixed_mod_despite_leftover() {
    let mut session = session(MinimalInput::default());
    session
        .add_modifier_texts([
            "5% increased maximum Life if you have at least 10 Red Support Gems Socketed",
        ])
        .expect("engine never errors");

    // The threshold mod was injected (carrying a MultiplierThreshold tag, non-empty
    // aggregation); leftover fragments don't block injection.
    assert!(
        !session.mods_named("Life").is_empty() || !session.mods_named("MaximumLife").is_empty(),
        "tag-suffixed threshold mod must be injected"
    );
}

#[test]
fn session_collects_unknown_modifier_text_as_unsupported() {
    // The engine never errors on unrecognized text -- the whole line goes into the
    // unsupported collection.
    let mut session = session(MinimalInput::default());

    session
        .add_modifier_texts(["not a real modifier"])
        .expect("engine never errors on unknown text");

    assert_eq!(
        session.unsupported_modifier_texts(),
        ["not a real modifier"]
    );
}

/// Projectile Speed -> Projectile Damage conversion (vendor CalcOffence.lua:840-845):
/// when the `ProjectileSpeedAppliesToProjectileDamage` flag is active, each INC
/// ProjectileSpeed mod is copied into a Damage INC mod (with flags replaced by
/// Projectile); absent the flag, this is a no-op.
#[test]
fn projectile_speed_applies_to_projectile_damage_conversion() {
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::modifier::ModFlags;

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let cfg =
        CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT);

    // No flag: ProjectileSpeed has no consumer, so this is a no-op.
    let mut without = session(input).with_config(cfg.clone());
    without
        .add_modifier_texts(["8% increased Projectile Speed"])
        .unwrap();
    let base = without.perform_minimal().expect("perform");
    assert_eq!(base.total_hit_avg, 100.0);

    // Flag active: 8% Projectile Speed -> 8% increased Damage (Projectile).
    let mut with = session(input).with_config(cfg);
    with.add_modifier_texts(["8% increased Projectile Speed"])
        .unwrap();
    with.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToProjectileDamage")]);
    let converted = with.perform_minimal().expect("perform");
    assert_eq!(converted.total_hit_avg, 108.0);

    // A source mod scoped by flags (e.g. "for Spell Skills") doesn't participate in
    // the conversion (matching vendor Tabulate's empty-cfg semantics).
    let mut scoped = session(MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    })
    .with_config(
        CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT),
    );
    scoped
        .add_modifier_texts(["8% increased Projectile Speed for Spell Skills"])
        .unwrap();
    scoped.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToProjectileDamage")]);
    let scoped_out = scoped.perform_minimal().expect("perform");
    assert_eq!(scoped_out.total_hit_avg, 100.0);
}

/// Bow variant (tree notable "Feathered Fletching", ModParser.lua:3648 ->
/// `ProjectileSpeedAppliesToBowDamage`; consumed at CalcOffence.lua:796-802): INC
/// ProjectileSpeed mods are copied into Damage INC mods (with flags replaced by
/// Bow|Hit, matching vendor Tabulate's `{ flags = ModFlag.Bow }`); the copy doesn't
/// match against a non-bow cfg (no BOW bit).
#[test]
fn projectile_speed_applies_to_bow_damage_conversion() {
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::modifier::ModFlags;

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let bow_cfg = CalcConfig::attack()
        .with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT | ModFlags::BOW);

    // End-to-end through the parser: notable text -> flag -> conversion applies
    // (46% proj speed -> +46% Damage).
    let mut with = session(input).with_config(bow_cfg.clone());
    with.add_modifier_texts([
        "46% increased Projectile Speed",
        "Increases and Reductions to [Projectile|Projectile] Speed also apply to Damage with [Bow|Bows]",
    ])
    .unwrap();
    let converted = with.perform_minimal().expect("perform");
    assert_eq!(converted.total_hit_avg, 146.0);

    // Non-bow skill cfg (no BOW bit): the copy's flags=Bow|Hit are not a subset of
    // cfg -> no match.
    let mut non_bow = session(input).with_config(
        CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT),
    );
    non_bow
        .add_modifier_texts(["46% increased Projectile Speed"])
        .unwrap();
    non_bow.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToBowDamage")]);
    let non_bow_out = non_bow.perform_minimal().expect("perform");
    assert_eq!(non_bow_out.total_hit_avg, 100.0);
}

/// A line can parse and still do nothing: its name has no reader, or it scales
/// by a variable nothing sets. Diagnostics name both instead of leaving a
/// number quietly short.
#[test]
fn session_reports_modifiers_that_parsed_and_did_nothing() {
    use pobr_core::{ModTag, Modifier};
    use pobr_data::prelude::ModType;

    let mut s = session(MinimalInput {
        base_life: 100.0,
        ..Default::default()
    });
    s.add_modifiers([
        Modifier::number("MaximumLife", ModType::Base, 50.0),
        Modifier::flag("NothingReadsThisFlag").with_source("Synthetic keystone"),
        Modifier::number("MaximumLife", ModType::Base, 5.0).with_tag(ModTag::multiplier(
            "NothingSetsThis",
            1.0,
            None,
        )),
        // The offence pass sets this one for the hit it computes.
        Modifier::number("CriticalStrikeChance", ModType::Base, 0.25).with_tag(ModTag::multiplier(
            "ExcessHitChance",
            1.0,
            None,
        )),
    ]);
    s.track_modifier_reads();
    let output = s.perform_minimal().unwrap();

    // The per-multiplier line reads as 0: only the plain +50 arrived.
    assert_eq!(output.life, 150.0);
    let unread: Vec<&str> = s
        .unread_modifiers()
        .iter()
        .map(|modifier| modifier.name.as_str())
        .collect();
    assert_eq!(unread, ["NothingReadsThisFlag"]);
    assert!(
        s.read_modifier_names()
            .iter()
            .any(|name| name.as_str() == "MaximumLife")
    );
    let unprovided: Vec<(&str, &str, f64)> = s
        .unprovided_multipliers()
        .into_iter()
        .map(|(variable, modifier)| {
            (
                variable,
                modifier.name.as_str(),
                modifier.value.as_number().unwrap(),
            )
        })
        .collect();
    assert_eq!(unprovided, [("NothingSetsThis", "MaximumLife", 5.0)]);
}

/// Without tracking, nothing is reported and nothing is paid for.
#[test]
fn session_reports_no_unread_modifiers_unless_reads_are_tracked() {
    use pobr_core::Modifier;

    let mut s = session(MinimalInput::default());
    s.add_modifiers([Modifier::flag("NothingReadsThisFlag")]);
    s.perform_minimal().unwrap();
    assert!(s.unread_modifiers().is_empty());
    assert!(s.read_modifier_names().is_empty());
}

/// "per 50 current Life" counts the Life that is not reserved. Nothing set
/// that amount for a modifier to read, so such lines scaled by zero.
#[test]
fn per_unreserved_life_modifiers_read_the_pool_less_its_reservation() {
    use pobr_core::{ModTag, Modifier};
    use pobr_data::prelude::ModType;

    let mut session = session(MinimalInput {
        base_life: 1000.0,
        base_mana: 500.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        ..Default::default()
    });
    session.add_modifiers([
        Modifier::number("LifeReservedPercent", ModType::Inc, 40.0),
        Modifier::number("ManaReserved", ModType::Base, 200.0),
        // 600 unreserved Life / 50 = 12 steps of 5%.
        Modifier::number("Damage", ModType::Inc, 5.0).with_tag(ModTag::multiplier(
            "LifeUnreserved",
            50.0,
            None,
        )),
        // 300 unreserved Mana / 100 = 3 steps of 10%.
        Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::multiplier(
            "ManaUnreserved",
            100.0,
            None,
        )),
    ]);
    session.prepare_player_stats(1, None);
    session.track_modifier_reads();
    let output = session.perform_minimal().expect("perform");
    assert!(
        (output.total_hit_avg - 190.0).abs() < 1e-10,
        "{}",
        output.total_hit_avg
    );
    assert!(session.unprovided_multipliers().is_empty());
}
