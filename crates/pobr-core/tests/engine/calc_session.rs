use pobr_core::calc::MinimalInput;

use crate::support::session;

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

    let output = session.perform_minimal();

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

    let output = session.perform_minimal();
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
    let output = session.perform_minimal();
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
    let base = without.perform_minimal();
    assert_eq!(base.total_hit_avg, 100.0);

    // Flag active: 8% Projectile Speed -> 8% increased Damage (Projectile).
    let mut with = session(input).with_config(cfg);
    with.add_modifier_texts(["8% increased Projectile Speed"])
        .unwrap();
    with.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToProjectileDamage")]);
    let converted = with.perform_minimal();
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
    let scoped_out = scoped.perform_minimal();
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
    let converted = with.perform_minimal();
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
    let non_bow_out = non_bow.perform_minimal();
    assert_eq!(non_bow_out.total_hit_avg, 100.0);
}
