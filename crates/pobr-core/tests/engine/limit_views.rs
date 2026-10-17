//! Calcs views of limits, speed and recovery multipliers, requirement
//! multipliers and Chill / Shock maximums (`calc::limit_display`).
use pobr_core::calc::{
    CalculationSession, DEFAULT_ENEMY_CURSE_LIMIT, DEFAULT_ENEMY_MARK_LIMIT, DEFAULT_MAX_CHARGES,
    MinimalInput,
};
use pobr_core::{ModDb, Modifier, display_value};
use pobr_data::prelude::*;

fn run(mods: Vec<Modifier>, enemy: Vec<Modifier>) -> CalculationSession {
    let mut session = super::support::session(MinimalInput::default());
    session.add_modifiers(mods);
    session.add_enemy_modifiers(enemy);
    session.perform_minimal().unwrap();
    session
}

fn value(session: &CalculationSession, id: &str) -> f64 {
    display_value(session.output(), id)
}

fn close(left: f64, right: f64, what: &str) {
    assert!((left - right).abs() < 1e-9, "{what}: {left} != {right}");
}

fn group_totals(session: &CalculationSession, stat: &str, name: &str) -> (f64, f64, f64) {
    session
        .detail_sources(stat)
        .into_iter()
        .find(|group| group.name == name)
        .unwrap_or_else(|| panic!("{stat} has no {name} group"))
        .totals
}

#[test]
fn neutral_character_reports_game_defaults() {
    let session = run(Vec::new(), Vec::new());
    for id in [
        "ActionSpeedMod",
        "MovementSpeedMod",
        "LightRadiusMod",
        "LifeRecoveryRateMod",
        "ManaRecoveryRateMod",
        "EnergyShieldRecoveryRateMod",
        "GlobalGemAttributeRequirements",
        "GlobalItemAttributeRequirements",
        "GlobalWeaponAttributeRequirements",
    ] {
        close(value(&session, id), 1.0, id);
    }
    // Game constants: Presence radius 4 m, `LowPoolThreshold` 35%, Chill
    // maximum 50% and Shock maximum 100%; nothing applies either ailment.
    close(value(&session, "PresenceRadius"), 40.0, "PresenceRadius");
    close(
        value(&session, "PresenceRadius"),
        value(&session, "PresenceRadiusMetres") * 10.0,
        "tenths of a metre",
    );
    close(
        value(&session, "LowLifePercentage"),
        35.0,
        "LowLifePercentage",
    );
    close(value(&session, "MaximumChill"), 50.0, "MaximumChill");
    close(value(&session, "MaximumShock"), 100.0, "MaximumShock");
    close(value(&session, "CurrentChill"), 0.0, "CurrentChill");
    close(value(&session, "CurrentShock"), 0.0, "CurrentShock");
    close(value(&session, "CharmLimit"), 0.0, "CharmLimit");
    close(
        value(&session, "EnemyCurseLimit"),
        DEFAULT_ENEMY_CURSE_LIMIT,
        "EnemyCurseLimit",
    );
    close(
        value(&session, "EnemyMarkLimit"),
        DEFAULT_ENEMY_MARK_LIMIT,
        "EnemyMarkLimit",
    );
}

#[test]
fn speed_and_recovery_multipliers_reuse_the_calculation_terms() {
    let session = run(
        vec![
            // A 50% slow at 30% reduced potency is a 35% slow, then +20%.
            Modifier::number("ActionSpeed", ModType::Inc, -50.0),
            Modifier::number("ActionSpeed", ModType::Inc, 20.0),
            Modifier::number("SlowEffectOnSelf", ModType::Inc, -30.0),
            Modifier::number("MovementSpeed", ModType::Inc, 20.0),
            Modifier::number("LightRadius", ModType::Inc, 20.0),
            Modifier::number("LightRadius", ModType::More, -10.0),
            Modifier::number("LifeRecoveryRate", ModType::Inc, 15.0),
            Modifier::number("ManaRecoveryRate", ModType::More, -50.0),
            Modifier::number("EnergyShieldRecoveryRate", ModType::Inc, 6.0),
        ],
        Vec::new(),
    );
    // (1 + 20%) * (1 - 50% * 0.7)
    close(
        value(&session, "ActionSpeedMod"),
        1.2 * 0.65,
        "ActionSpeedMod",
    );
    close(value(&session, "MovementSpeedMod"), 1.2, "MovementSpeedMod");
    // The existing view is the product of the two, so neither is applied twice.
    close(
        value(&session, "EffectiveMovementSpeedMod"),
        value(&session, "MovementSpeedMod") * value(&session, "ActionSpeedMod"),
        "EffectiveMovementSpeedMod",
    );
    close(
        value(&session, "LightRadiusMod"),
        1.2 * 0.9,
        "LightRadiusMod",
    );
    close(value(&session, "LifeRecoveryRateMod"), 1.15, "Life");
    close(value(&session, "ManaRecoveryRateMod"), 0.5, "Mana");
    close(value(&session, "EnergyShieldRecoveryRateMod"), 1.06, "ES");
    // Each source group is the modifier query behind the number.
    let (_, inc, _) = group_totals(&session, "ActionSpeedMod", "ActionSpeed");
    close(inc, -30.0, "ActionSpeed increases");
    let (_, inc, _) = group_totals(&session, "ActionSpeedMod", "SlowEffectOnSelf");
    close(inc, -30.0, "SlowEffectOnSelf");
    let (_, inc, _) = group_totals(&session, "MovementSpeedMod", "MovementSpeed");
    close(inc, 20.0, "MovementSpeed");
    let (_, inc, more) = group_totals(&session, "LightRadiusMod", "LightRadius");
    close(
        (1.0 + inc / 100.0) * more,
        value(&session, "LightRadiusMod"),
        "light",
    );
    let (_, inc, more) = group_totals(&session, "ManaRecoveryRateMod", "ManaRecoveryRate");
    close(
        (1.0 + inc / 100.0) * more,
        value(&session, "ManaRecoveryRateMod"),
        "mana",
    );
}

#[test]
fn unaffected_by_slows_removes_the_penalty_from_action_speed_mod() {
    let session = run(
        vec![
            Modifier::number("ActionSpeed", ModType::Inc, -50.0),
            Modifier::flag("UnaffectedBySlows"),
        ],
        Vec::new(),
    );
    close(value(&session, "ActionSpeedMod"), 1.0, "ActionSpeedMod");
    assert!(
        session
            .detail_sources("ActionSpeedMod")
            .iter()
            .any(|group| group.name == "UnaffectedBySlows")
    );
}

#[test]
fn presence_radius_is_the_tenths_of_a_metre_form_of_the_metre_view() {
    let session = run(
        vec![
            Modifier::number("PresenceRadius", ModType::Base, 20.0),
            Modifier::number("PresenceRadius", ModType::Inc, 50.0),
        ],
        Vec::new(),
    );
    // (40 + 20) * 1.5
    close(value(&session, "PresenceRadius"), 90.0, "PresenceRadius");
    close(value(&session, "PresenceRadiusMetres"), 9.0, "metres");
    let (base, inc, _) = group_totals(&session, "PresenceRadius", "PresenceRadius");
    close(base, 20.0, "base");
    close(inc, 50.0, "inc");
}

#[test]
fn charm_and_curse_limits_follow_their_game_stats() {
    // Charm slots are capped by the game constant of 3.
    let session = run(
        vec![
            Modifier::number("CharmLimit", ModType::Base, 2.0),
            Modifier::number("CharmLimit", ModType::Base, 2.0),
            Modifier::number("EnemyCurseLimit", ModType::Base, 1.0),
            Modifier::number("EnemyMarkLimit", ModType::Base, 1.0),
        ],
        Vec::new(),
    );
    close(value(&session, "CharmLimit"), 3.0, "CharmLimit");
    close(
        value(&session, "EnemyCurseLimit"),
        DEFAULT_ENEMY_CURSE_LIMIT + 1.0,
        "EnemyCurseLimit",
    );
    close(
        value(&session, "EnemyMarkLimit"),
        DEFAULT_ENEMY_MARK_LIMIT + 1.0,
        "EnemyMarkLimit",
    );
    let (base, _, _) = group_totals(&session, "CharmLimit", "CharmLimit");
    close(base, 4.0, "uncapped modifier total");
    // The curse limit becomes the maximum Power Charges when stated.
    let session = run(
        vec![
            Modifier::flag("CurseLimitIsMaximumPowerCharges"),
            Modifier::number("PowerChargesMax", ModType::Base, 2.0),
            Modifier::number("EnemyCurseLimit", ModType::Base, 5.0),
        ],
        Vec::new(),
    );
    close(
        value(&session, "EnemyCurseLimit"),
        f64::from(DEFAULT_MAX_CHARGES) + 2.0,
        "power charge limit",
    );
}

#[test]
fn global_requirement_multipliers_split_gem_item_and_weapon() {
    let session = run(
        vec![
            Modifier::number("GlobalAttributeRequirements", ModType::Inc, -8.0),
            Modifier::number("GlobalItemAttributeRequirements", ModType::Inc, -4.0),
            Modifier::number("GlobalWeaponAttributeRequirements", ModType::More, 50.0),
        ],
        Vec::new(),
    );
    close(
        value(&session, "GlobalGemAttributeRequirements"),
        0.92,
        "gem",
    );
    close(
        value(&session, "GlobalItemAttributeRequirements"),
        0.88,
        "item",
    );
    // Weapons take the item modifiers as well as their own.
    close(
        value(&session, "GlobalWeaponAttributeRequirements"),
        0.88 * 1.5,
        "weapon",
    );
    let (_, inc, _) = group_totals(
        &session,
        "GlobalWeaponAttributeRequirements",
        "GlobalAttributeRequirements",
    );
    close(inc, -8.0, "global increases");
}

#[test]
fn low_life_percentage_and_lowest_attribute_have_reconstructing_details() {
    // The attributes are settled by `prepare_player_stats`, as an application
    // does after the sources are in.
    let mut session = super::support::session(MinimalInput::default());
    session.add_modifiers(vec![
        Modifier::number("Strength", ModType::Base, 60.0),
        Modifier::number("Dexterity", ModType::Base, 40.0),
        Modifier::number("Intelligence", ModType::Base, 90.0),
    ]);
    session.prepare_player_stats(1, None);
    session.perform_minimal().unwrap();
    let low = session.derived_detail("LowLifePercentage").unwrap();
    close(
        low.inputs[0].1 * 100.0,
        value(&session, "LowLifePercentage"),
        "LowLifePercentage detail",
    );
    let lowest = session.derived_detail("LowestAttribute").unwrap();
    let min = lowest
        .inputs
        .iter()
        .map(|(_, input)| *input)
        .fold(f64::INFINITY, f64::min);
    close(
        min,
        value(&session, "LowestAttribute"),
        "LowestAttribute detail",
    );
    close(value(&session, "LowestAttribute"), 40.0, "LowestAttribute");
}

#[test]
fn chill_and_shock_maximums_and_current_values_share_the_application_stage() {
    // The enemy is Chilled at 30 by a stated value; a player source of 40
    // is stronger; Shock has no source.
    let session = run(
        vec![Modifier::number("ChillBase", ModType::Base, 40.0)],
        vec![Modifier::number("ChillVal", ModType::Base, 30.0)],
    );
    close(value(&session, "MaximumChill"), 50.0, "MaximumChill");
    close(value(&session, "CurrentChill"), 40.0, "CurrentChill");
    close(value(&session, "CurrentShock"), 0.0, "CurrentShock");
    let detail = session.derived_detail("CurrentChill").unwrap();
    let input = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
    close(
        input("AilmentSourceMagnitude")
            .max(input("EnemyAilmentMagnitude"))
            .min(input("MaximumMagnitude"))
            .floor(),
        value(&session, "CurrentChill"),
        "CurrentChill detail",
    );
    // The enemy-side value alone also applies the ailment.
    let session = run(
        Vec::new(),
        vec![Modifier::number("ChillVal", ModType::Base, 30.0)],
    );
    close(value(&session, "CurrentChill"), 30.0, "enemy Chill value");
    // Nothing applies it: the current value is 0 and the detail says so.
    let session = run(Vec::new(), Vec::new());
    let detail = session.derived_detail("CurrentChill").unwrap();
    assert_eq!(detail.formula, "NotApplied");
    close(
        detail.inputs[0].1,
        value(&session, "CurrentChill"),
        "not applied",
    );
}

#[test]
fn ailment_maximums_take_bonuses_overrides_and_cap_the_current_value() {
    let session = run(
        vec![
            Modifier::number("ChillMax", ModType::Base, 10.0),
            Modifier::number("ChillBase", ModType::Base, 70.0),
            Modifier::number("ShockBase", ModType::Base, 150.0),
        ],
        Vec::new(),
    );
    close(value(&session, "MaximumChill"), 60.0, "MaximumChill");
    close(value(&session, "CurrentChill"), 60.0, "capped Chill");
    close(value(&session, "MaximumShock"), 100.0, "MaximumShock");
    close(value(&session, "CurrentShock"), 100.0, "capped Shock");
    for (id, value_id) in [
        ("MaximumChill", "MaximumChill"),
        ("MaximumShock", "MaximumShock"),
    ] {
        let detail = session.derived_detail(id).unwrap();
        let sum: f64 = detail.inputs.iter().map(|(_, input)| *input).sum();
        close(sum, value(&session, value_id), id);
    }
    let session = run(
        vec![
            Modifier::number("ChillMax", ModType::Override, 25.0),
            Modifier::number("ChillMax", ModType::Base, 10.0),
        ],
        vec![Modifier::number("ChillVal", ModType::Base, 30.0)],
    );
    close(value(&session, "MaximumChill"), 25.0, "overridden maximum");
    close(value(&session, "CurrentChill"), 25.0, "overridden current");
    let detail = session.derived_detail("MaximumChill").unwrap();
    assert_eq!(detail.inputs.len(), 1);
    close(detail.inputs[0].1, 25.0, "override input");
}

fn pool_session(mods: Vec<Modifier>) -> CalculationSession {
    let mut session = super::support::session(MinimalInput {
        base_life: 1000.0,
        base_mana: 500.0,
        ..Default::default()
    });
    session.add_modifiers(mods);
    session.prepare_player_stats(1, None);
    session.perform_minimal().unwrap();
    session
}

#[test]
fn cancellable_reservation_excludes_the_permanent_share_and_ignores_efficiency() {
    // "Reserves 25% of Life" (Beidat's Hand) is a permanent 25% of 1000 Life,
    // and 10% of 500 Mana is reserved the same way. Neither share is scaled by
    // Reservation Efficiency or the reservation multiplier.
    let session = pool_session(vec![
        Modifier::number("ExtraLifeReserved", ModType::Base, 25.0),
        Modifier::number("ExtraManaReserved", ModType::Base, 10.0),
        Modifier::number("ReservationEfficiency", ModType::Inc, 100.0),
        Modifier::number("ReservationMultiplier", ModType::More, -50.0),
    ]);
    close(value(&session, "LifeReserved"), 250.0, "LifeReserved");
    close(value(&session, "LifeUnreserved"), 750.0, "LifeUnreserved");
    close(value(&session, "ManaReserved"), 50.0, "ManaReserved");
    close(
        value(&session, "LifeCancellableReservation"),
        75.0,
        "LifeCancellableReservation",
    );
    close(
        value(&session, "ManaCancellableReservation"),
        90.0,
        "ManaCancellableReservation",
    );
    // The permanent share is a source group of the reservation total.
    let (base, _, _) = group_totals(&session, "LifeReserved", "ExtraLifeReserved");
    close(base, 25.0, "LifeReserved source group");
    // Each detail reconstructs the displayed value from its input.
    for (stat, input) in [
        ("LifeCancellableReservation", "ExtraLifeReserved"),
        ("ManaCancellableReservation", "ExtraManaReserved"),
    ] {
        let detail = session.derived_detail(stat).unwrap();
        assert_eq!(detail.inputs[0].0, input);
        close(
            (100.0 - detail.inputs[0].1).max(0.0),
            value(&session, stat),
            stat,
        );
    }
}

#[test]
fn nothing_permanent_leaves_the_whole_reservation_cancellable() {
    // A skill's reservation is cancellable: turning the skill off releases it.
    let session = pool_session(vec![Modifier::number("LifeReserved", ModType::Base, 400.0)]);
    close(value(&session, "LifeReserved"), 400.0, "LifeReserved");
    close(
        value(&session, "LifeCancellableReservation"),
        100.0,
        "LifeCancellableReservation",
    );
    close(
        value(&session, "ManaCancellableReservation"),
        100.0,
        "ManaCancellableReservation",
    );
    // More than the whole pool permanently reserved cancels nothing, not less.
    let session = pool_session(vec![Modifier::number(
        "ExtraLifeReserved",
        ModType::Base,
        130.0,
    )]);
    close(
        value(&session, "LifeCancellableReservation"),
        0.0,
        "over-reserved",
    );
}

#[test]
fn duration_mod_is_the_increased_and_more_terms_of_the_duration_calculation() {
    use pobr_core::calc::skill_mechanics::{duration_modifier_terms, duration_terms};
    let neutral = run(Vec::new(), Vec::new());
    close(value(&neutral, "DurationMod"), 1.0, "neutral DurationMod");
    // 40% increased and 10% more Duration: 1.4 * 1.1.
    let mods = vec![
        Modifier::number("Duration", ModType::Inc, 40.0),
        Modifier::number("Duration", ModType::More, 10.0),
        Modifier::number("SkillDurationBase", ModType::Base, 4.0),
    ];
    let session = run(mods.clone(), Vec::new());
    close(value(&session, "DurationMod"), 1.54, "DurationMod");
    let detail = session.derived_detail("DurationMod").unwrap();
    let input = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
    close(
        ((1.0 + input("DurationIncreased") / 100.0) * input("DurationMore")).max(0.0),
        value(&session, "DurationMod"),
        "DurationMod detail",
    );
    // The skill duration applies the same modifier exactly once.
    let mut db = ModDb::new();
    for modifier in mods {
        db.add_mod(modifier);
    }
    let cfg = pobr_core::CalcConfig::spell();
    close(
        duration_terms(&db, &ModDb::new(), &cfg, "Duration").modifier,
        duration_modifier_terms(&db, &cfg, Some("PrimaryDuration")).value(),
        "duration uses the shared terms",
    );
    // Reduced below zero cannot make the modifier negative.
    let session = run(
        vec![Modifier::number("Duration", ModType::Inc, -150.0)],
        Vec::new(),
    );
    close(value(&session, "DurationMod"), 0.0, "floored DurationMod");
}

#[test]
fn full_life_percentage_defaults_to_the_whole_pool_and_takes_the_override() {
    let overridden = |context: &[(String, f64)]| {
        context
            .iter()
            .find(|(name, _)| name == "FullLifeThresholdOverridden")
            .unwrap()
            .1
    };
    let session = run(Vec::new(), Vec::new());
    close(value(&session, "FullLifePercentage"), 100.0, "default");
    let detail = session.derived_detail("FullLifePercentage").unwrap();
    close(detail.inputs[0].1, 100.0, "default detail");
    close(overridden(&detail.context), 0.0, "not overridden");
    // "You count as on Full Life while at 90% of maximum Life or above"
    // (game stat `full_life_threshold_%_override`).
    let parsed = super::support::parse_mod(
        "You count as on Full Life while at 90% of maximum Life or above",
    )
    .unwrap();
    assert_eq!(
        parsed.status,
        pobr_core::mod_parser::ParseStatus::Parsed,
        "{parsed:?}"
    );
    let session = run(parsed.mods, Vec::new());
    close(value(&session, "FullLifePercentage"), 90.0, "override");
    let detail = session.derived_detail("FullLifePercentage").unwrap();
    close(detail.inputs[0].1, 90.0, "override detail");
    close(overridden(&detail.context), 1.0, "overridden");
}

#[test]
fn full_mana_percentage_defaults_to_the_whole_pool_and_takes_the_override() {
    let session = run(Vec::new(), Vec::new());
    close(value(&session, "FullManaPercentage"), 100.0, "default");
    // Idle Hands: "You count as on Full Mana while at 90% of maximum Mana or
    // above" (game stat `full_mana_threshold_%_override`,
    // `Mods.UniqueFullManaThreshold1` = 90).
    let parsed = super::support::parse_mod(
        "You count as on Full Mana while at 90% of maximum Mana or above",
    )
    .unwrap();
    assert_eq!(
        parsed.status,
        pobr_core::mod_parser::ParseStatus::Parsed,
        "{parsed:?}"
    );
    let session = run(parsed.mods, Vec::new());
    close(value(&session, "FullManaPercentage"), 90.0, "override");
    let detail = session.derived_detail("FullManaPercentage").unwrap();
    close(detail.inputs[0].1, 90.0, "override detail");
    let overridden = detail
        .context
        .iter()
        .find(|(name, _)| name == "FullManaThresholdOverridden")
        .unwrap()
        .1;
    close(overridden, 1.0, "overridden");
    close(
        value(&session, "FullLifePercentage"),
        100.0,
        "Life untouched",
    );
}

fn strike_session(skill_types: SkillTypes, mods: Vec<Modifier>) -> CalculationSession {
    let mut cfg = pobr_core::CalcConfig::attack();
    cfg.skill_types = skill_types;
    let mut session = super::support::session(MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 2.0,
        ..Default::default()
    })
    .with_config(cfg);
    session.add_modifiers(mods);
    session.bridge_player_conditions();
    session.perform_minimal().unwrap();
    session
}

#[test]
fn strike_targets_count_the_enemy_hit_plus_the_additional_targets_without_changing_dps() {
    let strike = SkillTypes::from_pob2_name("MeleeSingleTarget").unwrap();
    // Not a Strike (a Slam, or any other skill): the view is 0.
    let plain = strike_session(SkillTypes::default(), Vec::new());
    close(value(&plain, "StrikeTargets"), 0.0, "non-strike");
    let granted = strike_session(
        SkillTypes::default(),
        vec![Modifier::number(
            "AdditionalStrikeTarget",
            ModType::Base,
            3.0,
        )],
    );
    close(
        value(&granted, "StrikeTargets"),
        0.0,
        "additional targets do not make a Strike",
    );
    // A Strike hits one enemy; "Non-Vaal Strike Skills target 1 additional
    // nearby Enemy" (game stat `melee_attack_number_of_spirit_strikes`) adds one.
    let base = strike_session(strike, Vec::new());
    close(value(&base, "StrikeTargets"), 1.0, "strike");
    let parsed =
        super::support::parse_mod("Non-Vaal Strike Skills target 1 additional nearby Enemy")
            .unwrap();
    assert_eq!(
        parsed.status,
        pobr_core::mod_parser::ParseStatus::Parsed,
        "{parsed:?}"
    );
    let extra = strike_session(strike, parsed.mods);
    close(
        value(&extra, "StrikeTargets"),
        2.0,
        "strike with an additional target",
    );
    // The targets multiply no damage.
    close(
        value(&extra, "TotalDPS"),
        value(&base, "TotalDPS"),
        "TotalDPS with additional targets",
    );
    let detail = extra.derived_detail("StrikeTargets").unwrap();
    let input = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
    close(
        input("StrikeSkill") * (1.0 + input("AdditionalStrikeTarget")),
        value(&extra, "StrikeTargets"),
        "StrikeTargets detail",
    );
}

#[test]
fn ancestrally_boosted_strikes_target_two_additional_enemies_per_boost_effect() {
    // "An Ancestrally Boosted Strike will target 2 additional Enemies"
    // (`ancestral_cry_empowered_attacks_strike_X_additional_enemies` = 2);
    // `double_ancestral_boost_effect` doubles the effect.
    let targets = Modifier::number("AdditionalStrikeTarget", ModType::Base, 2.0)
        .with_tag(pobr_core::ModTag::multiplier(
            "AncestralBoostEffect",
            1.0,
            None,
        ))
        .with_tag(pobr_core::ModTag::condition("AncestrallyBoosted", false));
    let strike = SkillTypes::from_pob2_name("MeleeSingleTarget").unwrap();
    let boosted = strike_session(
        strike,
        vec![
            Modifier::flag("Condition:AncestrallyBoosted"),
            targets.clone(),
        ],
    );
    close(value(&boosted, "StrikeTargets"), 3.0, "one boost effect");
    let doubled = strike_session(
        strike,
        vec![
            Modifier::flag("Condition:AncestrallyBoosted"),
            Modifier::number("Multiplier:AncestralBoostEffect", ModType::Base, 1.0),
            targets.clone(),
        ],
    );
    close(
        value(&doubled, "StrikeTargets"),
        5.0,
        "doubled boost effect",
    );
    // Without the boosted state the additional targets wait.
    let idle = strike_session(strike, vec![targets]);
    close(value(&idle, "StrikeTargets"), 1.0, "not boosted");
}
