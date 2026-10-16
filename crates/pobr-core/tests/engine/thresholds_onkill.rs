use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

fn session(lines: &[&str]) -> CalculationSession {
    let mut session = super::support::session(MinimalInput {
        base_life: 1000.0,
        base_mana: 500.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..Default::default()
    })
    .with_config(CalcConfig::spell().with_mode_effective(true));
    session.add_modifier_texts(lines).unwrap();
    session
}

#[test]
fn ailment_threshold_adds_arbitrary_es_rolls_before_increases() {
    let mut session = session(&[
        "+2000 to maximum Energy Shield",
        "Gain additional Ailment Threshold equal to 17% of maximum Energy Shield",
        "40% increased Elemental Ailment Threshold",
    ]);
    session.perform_minimal().unwrap();
    // Game keyword: half Life, plus 17% ES, then 40% increased threshold.
    // (1000 * 0.5 + 2000 * 0.17) * 1.4 = 1176.
    assert_eq!(session.output().energy_shield, 2000.0);
    assert!((display_value(session.output(), "AilmentThreshold") - 1176.0).abs() < 1e-9);
    assert!(session.unsupported_modifier_texts().is_empty());
    assert_eq!(session.detail_sources("AilmentThreshold")[0].rows.len(), 2);
}

#[test]
fn on_kill_recovery_adds_flat_and_percent_of_final_maximum_pools() {
    let mut session = session(&[
        "+2000 to maximum Energy Shield",
        "7 Life per Enemy Killed",
        "11 Mana per Enemy Killed",
        "13 Energy Shield per Enemy Killed",
        "Recover 2% of maximum Life on Kill",
        "Recover 3% of maximum Mana on Kill",
        "Recover 4% of maximum Energy Shield on Kill",
        "50% increased maximum Life",
        "100% increased maximum Mana",
    ]);
    session.perform_minimal().unwrap();
    // 7 + 1500*.02 = 37; 11 + 1000*.03 = 41; 13 + 2000*.04 = 93.
    for (id, expected) in [
        ("LifeOnKill", 37.0),
        ("ManaOnKill", 41.0),
        ("EnergyShieldOnKill", 93.0),
    ] {
        assert_eq!(display_value(session.output(), id), expected, "{id}");
        assert!(
            session
                .detail_sources(id)
                .iter()
                .any(|group| !group.rows.is_empty())
        );
    }
    assert!(
        session.unsupported_modifier_texts().is_empty(),
        "{:?}",
        session.unsupported_modifier_texts()
    );
}

#[test]
fn on_kill_recovery_respects_resource_gain_restrictions() {
    for restriction in [
        "CannotGainLife",
        "CannotRecoverLifeOutsideLeech",
        "CannotGainMana",
        "CannotGainEnergyShield",
    ] {
        let mut session = session(&[
            "+2000 to maximum Energy Shield",
            "Recover 2% of maximum Life on Kill",
            "Recover 3% of maximum Mana on Kill",
            "Recover 4% of maximum Energy Shield on Kill",
        ]);
        session.add_modifiers([Modifier::flag(restriction)]);
        session.perform_minimal().unwrap();
        let out = session.output();
        assert_eq!(
            out.life_on_kill,
            if restriction.contains("Life") {
                0.0
            } else {
                20.0
            }
        );
        assert_eq!(
            out.mana_on_kill,
            if restriction.contains("Mana") {
                0.0
            } else {
                15.0
            }
        );
        assert_eq!(
            out.energy_shield_on_kill,
            if restriction.contains("EnergyShield") {
                0.0
            } else {
                80.0
            }
        );
    }
}

#[test]
fn freeze_buildup_reads_the_game_text_modifier() {
    let db = {
        let mut session = session(&[
            "50% increased Freeze Buildup",
            "20% increased Immobilisation Buildup",
        ]);
        // Test the same traced query used by perform, with a 1000 poise threshold.
        session.add_modifiers([Modifier::number("FreezeBuildup", ModType::More, 20.0)]);
        session.mod_db().clone()
    };
    let (buildup, _) = pobr_core::calc::ailment::freeze_poise_buildup_traced(
        1000.0,
        &db,
        &CalcConfig::spell(),
        &mut pobr_core::TraceGraph::new(),
    );
    // 2.1 / 1000 * (1 + .5 + .2) * 1.2 * 100 = .4284% per cold damage.
    assert!((buildup - 0.4284).abs() < 1e-9);
}

#[test]
fn daze_chance_is_a_capped_hit_chance() {
    for (extra, expected) in [(15.0, 25.0), (150.0, 100.0)] {
        let mut session = session(&["10% chance to Daze on Hit"]);
        session.add_modifiers([Modifier::number("DazeChance", ModType::Base, extra)]);
        session.perform_minimal().unwrap();
        assert_eq!(display_value(session.output(), "DazeChance"), expected);
    }
}

#[test]
fn fully_broken_armour_scales_physical_hits_and_leaves_bleed_unchanged() {
    let run = |broken, amount| {
        let mut session = session(&[
            "50% increased effect of Fully Broken Armour",
            "100% chance to inflict Bleeding on Hit",
        ]);
        if broken {
            session.add_enemy_modifiers([
                Modifier::number("Armour", ModType::Base, 1000.0),
                Modifier::flag("ArmourBreakConfigured"),
                Modifier::number("ArmourBreakAmount", ModType::Base, amount),
            ]);
        }
        session.perform_minimal().unwrap();
        session.output().clone()
    };
    let base = run(false, 0.0);
    let full = run(true, 0.0);
    let partial = run(true, 250.0);
    // Game keyword: Fully Broken Armour gives 20% increased physical hit
    // damage taken. 50% increased effect makes it 30%: 100 * 1.3 = 130.
    assert_eq!(full.fully_broken_armour_effect, 30.0);
    assert_eq!(full.average_damage, 130.0);
    assert_eq!(full.bleed_dps, base.bleed_dps);
    assert!(partial.average_damage < 100.0);
    assert_eq!(base.average_damage, 100.0);
}

#[test]
fn heavy_stun_uses_eligible_crit_weighted_hits_and_shared_poise_scaling() {
    let run = |distance, cannot_stun: Option<&str>| {
        let mut session = session(&[
            "50% increased Stun Buildup",
            "20% increased Immobilisation Buildup",
            "40% increased Stun Buildup against enemies within 2 metres",
            "Adds 100 to 100 Fire Damage",
            "Adds 100 to 100 Cold Damage",
            "Adds 100 to 100 Lightning Damage",
            "Adds 100 to 100 Chaos Damage",
        ])
        .with_config(
            CalcConfig::spell()
                .with_flags(ModFlags::SPELL | ModFlags::MELEE)
                .with_multiplier("enemyDistance", distance)
                .with_mode_effective(true),
        );
        session.setup_enemy(10, pobr_data::monster::EnemyTier::None);
        session.add_modifiers([
            Modifier::number("SkillBaseCritChance", ModType::Base, 50.0),
            Modifier::number("EnemyHeavyStunBuildup", ModType::More, 20.0),
            Modifier::number("FirePenetration", ModType::Base, 25.0),
        ]);
        session.add_enemy_modifiers([
            Modifier::number("PoiseThreshold", ModType::More, 100.0),
            Modifier::number("EnemyStunThreshold", ModType::Inc, -20.0),
            Modifier::number("HeavyStunBuildup", ModType::Inc, 30.0),
            Modifier::number("StunBuildup", ModType::More, 10.0),
            Modifier::number("FireResist", ModType::Base, 75.0),
            Modifier::number("Armour", ModType::Override, 0.0),
            Modifier::number("DamageTaken", ModType::Inc, 20.0),
        ]);
        if let Some(flag) = cannot_stun {
            session.add_modifiers([Modifier::flag(flag)]);
        }
        session.perform_minimal().unwrap();
        session
    };
    for (distance, inc) in [(19.9, 110.0), (20.0, 110.0), (20.1, 70.0)] {
        let session = run(distance, None);
        // Each type: 100 noncrit, 200 crit, 50% crit => 150 average.
        // Fire: (75 resist - 25 penetration) leaves 75; others remain 150.
        // All take 20% increased damage. Cold contributes zero; physical and melee
        // each get 1.5: (150*1.5 + 75 + 150 + 150)*1.2*1.5 = 1080.
        // Level 10 poise = 142;
        // floor(142*.8*2) = 227. INC = 50+20(+40 within 2m), MORE=1.2.
        // Enemy buildup = 1.3*1.1 = 1.43.
        let per_damage = (0.58 / 227.0 * (1.0 + inc / 100.0) * 1.2 * 100.0 * 1e9_f64).round() / 1e9;
        let expected = 1080.0 * per_damage * 1.43;
        let actual = display_value(session.output(), "HeavyStunBuildupAvg");
        assert!(
            (actual - expected).abs() < 1e-6,
            "{distance}: {actual} vs {expected}"
        );
        let detail = session.derived_detail("HeavyStunBuildupAvg").unwrap();
        let get = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
        let pct = (get("HeavyStunDamageScale") / get("EnemyPoiseThreshold")
            * (1.0 + get("StunBuildupIncreased") / 100.0)
            * get("StunBuildupMore")
            * 100.0
            * 1e9)
            .round()
            / 1e9;
        let reconstructed = (get("PhysicalHitAverage") * get("PhysicalStunMultiplier")
            + get("OtherStunHitAverage"))
            * get("MeleeStunMultiplier")
            * pct
            * get("EnemyStunBuildupMultiplier");
        assert!((actual - reconstructed).abs() < 1e-9);
    }
    for flag in ["CannotStun", "CannotHeavyStun"] {
        let session = run(20.0, Some(flag));
        assert_eq!(session.output().heavy_stun_buildup_avg, 0.0);
        assert_eq!(
            session
                .derived_detail("HeavyStunBuildupAvg")
                .unwrap()
                .inputs[0]
                .1,
            0.0
        );
    }
    let mut non_hit = session(&["Adds 100 to 100 Fire Damage"])
        .with_config(CalcConfig::spell().with_skill_name(Some("Non-hit skill".into())));
    non_hit.perform_minimal().unwrap();
    assert_eq!(non_hit.output().heavy_stun_buildup_avg, 0.0);
    for damage_type in [DamageType::Physical, DamageType::Cold] {
        let mut session = super::support::session(MinimalInput {
            base_hit_min: 0.0,
            base_hit_max: 0.0,
            ..Default::default()
        })
        .with_config(CalcConfig::spell());
        if damage_type == DamageType::Cold {
            session
                .add_modifier_texts(&["Adds 100 to 100 Cold Damage"])
                .unwrap();
        }
        session.perform_minimal().unwrap();
        assert_eq!(session.output().heavy_stun_buildup_avg, 0.0);
    }
}

#[test]
fn heavy_stun_reuses_separate_hit_and_crit_armour_mitigation() {
    let mut session = session(&[]);
    session.setup_enemy(10, pobr_data::monster::EnemyTier::None);
    session.add_modifiers([Modifier::number("SkillBaseCritChance", ModType::Base, 50.0)]);
    session.add_enemy_modifiers([Modifier::number("Armour", ModType::Override, 1000.0)]);
    session.perform_minimal().unwrap();
    // Armour ratio 10: noncrit 100 loses 50%; crit 200 loses 1/3.
    // Blending after mitigation gives (50 + 200*2/3)/2 = 275/3.
    // Physical's 50% more and GameConstants.HeavyStunDamageScale = .58 apply.
    let per_damage = (0.58 / 142.0 * 100.0 * 1e9_f64).round() / 1e9;
    let expected = (275.0 / 3.0) * 1.5 * per_damage;
    assert!((session.output().heavy_stun_buildup_avg - expected).abs() < 1e-6);
    let detail = session.derived_detail("HeavyStunBuildupAvg").unwrap();
    let physical = detail
        .inputs
        .iter()
        .find(|(k, _)| k == "PhysicalHitAverage")
        .unwrap()
        .1;
    assert!((physical - 275.0 / 3.0).abs() < 1e-6);
}

#[test]
fn heavy_stun_keeps_damage_types_that_only_occur_on_critical_hits() {
    for noncrit_cold in [false, true] {
        let mut s = session(&[]);
        s.setup_enemy(10, pobr_data::monster::EnemyTier::None);
        s.add_modifiers([Modifier::number("SkillBaseCritChance", ModType::Base, 50.0)]);
        for name in ["FireDamageMin", "FireDamageMax"] {
            s.add_modifiers([Modifier::number(name, ModType::Base, 100.0)
                .with_tag(pobr_core::ModTag::condition("CriticalStrike", false))]);
        }
        if noncrit_cold {
            for name in ["ColdDamageMin", "ColdDamageMax"] {
                s.add_modifiers([Modifier::number(name, ModType::Base, 100.0)
                    .with_tag(pobr_core::ModTag::condition("CriticalStrike", true))]);
            }
        }
        s.add_enemy_modifiers([
            Modifier::number("Armour", ModType::Override, 0.0),
            Modifier::number("FireResist", ModType::Base, 50.0),
        ]);
        s.perform_minimal().unwrap();
        // Physical: (100 + 200)/2 = 150. Crit-only fire: 200*.5 resist*.5 crit = 50.
        // KeywordPopups[55]/[3]: physical gets 1.5; noncrit-only cold is excluded.
        // The two legs may have different lengths OR different types at the same index.
        let per_damage = (0.58 / 142.0 * 100.0 * 1e9_f64).round() / 1e9;
        let expected = (150.0 * 1.5 + 50.0) * per_damage;
        assert!(
            (s.output().heavy_stun_buildup_avg - expected).abs() < 1e-6,
            "noncrit cold={noncrit_cold}: {} vs {expected}",
            s.output().heavy_stun_buildup_avg
        );
    }
}

#[test]
fn heavy_stun_daze_bonus_follows_the_effective_enemy_condition() {
    let run = |effective, dazed| {
        let mut s = session(&[]).with_config(CalcConfig::spell().with_mode_effective(effective));
        s.setup_enemy(10, pobr_data::monster::EnemyTier::None);
        s.add_enemy_modifiers([Modifier::number("Armour", ModType::Override, 0.0)]);
        if dazed {
            // Same flag and Effective gate as config conditionEnemyDazed.
            s.add_enemy_modifiers([Modifier::flag("Condition:Dazed")
                .with_tag(pobr_core::ModTag::condition("Effective", false))]);
        }
        s.perform_minimal().unwrap();
        s.output().heavy_stun_buildup_avg
    };
    // KeywordPopups[252]: Dazed enemies take 50% more Stun Buildup.
    assert!((run(true, true) - run(true, false) * 1.5).abs() < 1e-9);
    assert_eq!(run(false, true), run(false, false));
}

fn pin_session(lines: &[&str], flags: ModFlags) -> CalculationSession {
    let mut session = session(lines).with_config(
        CalcConfig::spell()
            .with_flags(flags)
            .with_mode_effective(true),
    );
    session.setup_enemy(10, pobr_data::monster::EnemyTier::None);
    session.add_enemy_modifiers([Modifier::number("Armour", ModType::Override, 0.0)]);
    session
}

/// `GameConstants.PinDamageScale` (4.2) over the poise threshold, with the
/// percentage rounded the way the calculation does.
fn pin_per_damage(threshold: f64, inc: f64, more: f64) -> f64 {
    (4.2 / threshold * (1.0 + inc / 100.0) * more * 100.0 * 1e9_f64).round() / 1e9
}

#[test]
fn pin_buildup_is_opt_in_per_damage_type_without_heavy_stun_multipliers() {
    let per_damage = pin_per_damage(142.0, 0.0, 1.0);
    let lines = ["Adds 100 to 100 Fire Damage", "Adds 100 to 100 Cold Damage"];
    // No Pin source: the hit builds Heavy Stun but no Pin.
    let mut none = pin_session(&lines, ModFlags::SPELL | ModFlags::MELEE);
    none.perform_minimal().unwrap();
    assert_eq!(display_value(none.output(), "PinBuildupAvg"), 0.0);
    assert!(display_value(none.output(), "HeavyStunBuildupAvg") > 0.0);
    // "Physical Damage is Pinning": only the 100 Physical damage counts. Heavy
    // Stun's Physical (1.5) and Melee (1.5) multipliers do not apply to Pin.
    let mut physical = pin_session(
        &[lines[0], lines[1], "Physical Damage is Pinning"],
        ModFlags::SPELL | ModFlags::MELEE,
    );
    physical.perform_minimal().unwrap();
    let actual = display_value(physical.output(), "PinBuildupAvg");
    assert!((actual - 100.0 * per_damage).abs() < 1e-6, "{actual}");
    // All damage (`base_all_damage_can_pin`): Fire is resisted by 50%, so
    // 100 Physical + 50 Fire + 100 Cold build Pin.
    let mut all = pin_session(&lines, ModFlags::SPELL);
    all.add_modifiers([Modifier::flag("CanPin")]);
    all.add_enemy_modifiers([Modifier::number("FireResist", ModType::Base, 50.0)]);
    all.perform_minimal().unwrap();
    let actual = display_value(all.output(), "PinBuildupAvg");
    assert!((actual - 250.0 * per_damage).abs() < 1e-6, "{actual}");
    // The derived view reconstructs the value from its own inputs.
    let detail = all.derived_detail("PinBuildupAvg").unwrap();
    let get = |id: &str| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
    let pct = (get("PinDamageScale") / get("EnemyPoiseThreshold")
        * (1.0 + get("PinBuildupIncreased") / 100.0)
        * get("PinBuildupMore")
        * 100.0
        * 1e9)
        .round()
        / 1e9;
    let reconstructed = (get("PhysicalPinningHitAverage")
        + get("FirePinningHitAverage")
        + get("ColdPinningHitAverage"))
        * pct
        * get("EnemyPinBuildupMultiplier");
    assert_eq!(get("PinDamageScale"), 4.2);
    assert!((actual - reconstructed).abs() < 1e-9);
    // `cannot_pin` forces zero.
    let mut cannot = pin_session(&lines, ModFlags::SPELL);
    cannot.add_modifiers([Modifier::flag("CanPin"), Modifier::flag("CannotPin")]);
    cannot.perform_minimal().unwrap();
    assert_eq!(display_value(cannot.output(), "PinBuildupAvg"), 0.0);
}

#[test]
fn pin_buildup_critical_source_counts_only_the_critical_physical_portion() {
    // 100 Physical, 50% critical chance, base critical damage doubles it.
    let mut session = pin_session(&["Physical Spell Critical Hits build Pin"], ModFlags::SPELL);
    session.add_modifiers([Modifier::number("SkillBaseCritChance", ModType::Base, 50.0)]);
    session.perform_minimal().unwrap();
    // Crit leg: 200 * 0.5 = 100 of the 150 average builds Pin.
    let per_damage = pin_per_damage(142.0, 0.0, 1.0);
    let actual = display_value(session.output(), "PinBuildupAvg");
    assert!((actual - 100.0 * per_damage).abs() < 1e-6, "{actual}");
    // Non-spell hits do not carry the spell-only source.
    let mut attack = pin_session(
        &["Physical Spell Critical Hits build Pin"],
        ModFlags::ATTACK,
    );
    attack.add_modifiers([Modifier::number("SkillBaseCritChance", ModType::Base, 50.0)]);
    attack.perform_minimal().unwrap();
    assert_eq!(display_value(attack.output(), "PinBuildupAvg"), 0.0);
}

#[test]
fn pin_buildup_projectile_source_and_modifier_readers() {
    let lines = [
        "Projectile Damage builds Pin",
        "50% increased Pin Buildup",
        "20% increased Immobilisation Buildup",
    ];
    let build = |flags| {
        let mut session = pin_session(&lines, flags);
        session.add_modifiers([Modifier::number("EnemyPinBuildup", ModType::More, 20.0)]);
        session.add_enemy_modifiers([
            Modifier::number("PoiseThreshold", ModType::More, 100.0),
            Modifier::number("PinBuildup", ModType::Inc, 30.0),
            Modifier::number("ImmobilisationBuildup", ModType::More, 10.0),
        ]);
        session.perform_minimal().unwrap();
        session
    };
    assert_eq!(
        display_value(build(ModFlags::SPELL).output(), "PinBuildupAvg"),
        0.0
    );
    let session = build(ModFlags::SPELL | ModFlags::PROJECTILE);
    // floor(142 * 2) = 284; INC 50 + 20; MORE 1.2; enemy 1.3 * 1.1.
    let expected = 100.0 * pin_per_damage(284.0, 70.0, 1.2) * 1.3 * 1.1;
    let actual = display_value(session.output(), "PinBuildupAvg");
    assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
}

#[test]
fn pin_support_pins_instead_of_heavy_stunning() {
    let mut session = pin_session(&["Physical Damage is Pinning"], ModFlags::SPELL);
    // `support_pin_buildup_pin_instead_of_stun` as the skill stat map writes it.
    session.add_modifiers([Modifier::flag("PinBuildupInsteadOfHeavyStunBuildup")]);
    session.perform_minimal().unwrap();
    assert_eq!(display_value(session.output(), "HeavyStunBuildupAvg"), 0.0);
    let pin = display_value(session.output(), "PinBuildupAvg");
    assert!(
        (pin - 100.0 * pin_per_damage(142.0, 0.0, 1.0)).abs() < 1e-6,
        "{pin}"
    );
    // Without the Pin support the same hit builds Heavy Stun (Physical 1.5).
    let mut stun = pin_session(&["Physical Damage is Pinning"], ModFlags::SPELL);
    stun.perform_minimal().unwrap();
    let expected = 100.0 * 1.5 * (0.58 / 142.0 * 100.0 * 1e9_f64).round() / 1e9;
    let actual = display_value(stun.output(), "HeavyStunBuildupAvg");
    assert!((actual - expected).abs() < 1e-6, "{actual} vs {expected}");
}
