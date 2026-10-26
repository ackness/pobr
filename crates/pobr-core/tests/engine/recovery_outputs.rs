use pobr_core::calc::{Actor, ActorBaseStats, Env, perform::perform};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

fn run(mods: Vec<Modifier>, rate: f64) -> Env {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            life: 1000.0,
            mana: 500.0,
            energy_shield: 300.0,
            hit_min: 1000.0,
            hit_max: 1000.0,
            action_rate: rate,
            ..ActorBaseStats::default()
        },
    );
    player.mod_db.add_list(mods);
    let mut env = Env::new(player);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut env).unwrap();
    env
}

fn value(env: &Env, name: &str, expected: f64) {
    let actual = display_value(&env.player.output, name);
    assert!(
        (actual - expected).abs() < 1e-6,
        "{name}: {actual} != {expected}"
    );
}

#[test]
fn recovery_accounts_for_regen_recovery_drain_and_main_skill_costs() {
    // life_recovery_rate_+% applies to regeneration and other recovery.
    // life_degeneration_%_per_minute_not_in_grace is loss, not damage.
    let env = run(
        vec![
            Modifier::number("LifeRegen", ModType::Base, 100.0),
            Modifier::number("LifeRegen", ModType::Inc, 50.0),
            Modifier::number("LifeRecoveryRate", ModType::Inc, 50.0),
            Modifier::number("LifeRecovery", ModType::Base, 20.0),
            Modifier::number("LifeDegenPercent", ModType::Base, 2.0),
            Modifier::number("ManaRegen", ModType::Base, 50.0),
            Modifier::number("EnergyShieldRegen", ModType::Base, 30.0),
            Modifier::number("WardRegen", ModType::Base, 5.0),
            Modifier::number("RageRegen", ModType::Base, 3.0),
            Modifier::number("SkillLifeCostBase", ModType::Base, 10.0),
            Modifier::number("SkillManaCostBase", ModType::Base, 20.0),
        ],
        2.0,
    );
    value(&env, "LifeRegenRecovery", 210.0);
    value(&env, "ManaRegenRecovery", 50.0);
    value(&env, "EnergyShieldRegenRecovery", 30.0);
    value(&env, "WardRegenRecovery", 5.0);
    value(&env, "RageRegenRecovery", 3.0);
    value(&env, "NetLifeRegen", 190.0);
    value(&env, "NetManaRegen", 10.0);
    value(&env, "NetEnergyShieldRegen", 30.0);
    value(&env, "TotalNetRegen", 235.0);
    value(&env, "TotalBuildDegen", 80.0);
}

#[test]
fn recovery_obeys_gain_restrictions_and_zealots_oath() {
    let env = run(
        vec![
            Modifier::number("LifeRegen", ModType::Base, 100.0),
            Modifier::number("ManaRegen", ModType::Base, 50.0),
            Modifier::number("ManaRecovery", ModType::Base, 10.0),
            Modifier::number("EnergyShieldRegen", ModType::Base, 30.0),
            Modifier::flag("ZealotsOath"),
            Modifier::flag("CannotGainMana"),
        ],
        0.0,
    );
    value(&env, "LifeRegenRecovery", 0.0);
    value(&env, "ManaRegenRecovery", 0.0);
    value(&env, "EnergyShieldRegenRecovery", 130.0);
}

#[test]
fn leech_gains_include_on_hit_and_obey_hit_frequency() {
    let mods = vec![
        Modifier::number("PhysicalDamageLifeLeech", ModType::Base, 2.0),
        Modifier::number("LifeOnHit", ModType::Base, 5.0),
        Modifier::number("LifeRecoveryRate", ModType::Inc, 50.0),
        Modifier::number("ManaOnHit", ModType::Base, 3.0),
        Modifier::number("EnergyShieldOnHit", ModType::Base, 4.0),
    ];
    let fast = run(mods.clone(), 2.0);
    value(&fast, "LifeLeechGainRate", 40.0);
    value(&fast, "LifeLeechGainPerHit", 35.0);
    value(&fast, "ManaLeechGainRate", 6.0);
    value(&fast, "ManaLeechGainPerHit", 3.0);
    value(&fast, "EnergyShieldLeechGainRate", 8.0);
    value(&fast, "EnergyShieldLeechGainPerHit", 4.0);
    let slow = run(mods, 0.5);
    value(&slow, "LifeLeechGainRate", 17.5);
    value(&slow, "LifeLeechGainPerHit", 35.0);
}

#[test]
fn leech_supports_grant_only_physical_attack_leech() {
    // Soul Thief grants 800 permyriad = 8%; Life Thief grants 1200 = 12%.
    // A 100 damage physical attack therefore leeches 8 Mana and 12 Life.
    for (resource, stat, amount, expected) in [
        (
            "Mana",
            "base_mana_leech_from_physical_attack_damage_permyriad",
            800.0,
            8.0,
        ),
        (
            "Life",
            "base_life_leech_from_physical_attack_damage_permyriad",
            1200.0,
            12.0,
        ),
    ] {
        for (cfg, conversion, expected) in [
            (
                CalcConfig::attack().with_damage_type(DamageType::Physical),
                0.0,
                expected,
            ),
            (
                CalcConfig::spell().with_damage_type(DamageType::Physical),
                0.0,
                0.0,
            ),
            (CalcConfig::attack(), 100.0, 0.0),
        ] {
            let mut player = Actor::new(
                90,
                ActorBaseStats {
                    life: 1000.0,
                    mana: 500.0,
                    hit_min: 100.0,
                    hit_max: 100.0,
                    action_rate: 1.0,
                    ..ActorBaseStats::default()
                },
            );
            player
                .mod_db
                .add_list(pobr_core::rules::skill_stats::skill_modifiers(stat, amount).unwrap());
            player.mod_db.add_mod(Modifier::number(
                "PhysicalDamageConvertToFire",
                ModType::Base,
                conversion,
            ));
            let mut env = Env::new(player);
            env.cfg = cfg;
            perform(&mut env).unwrap();
            value(&env, &format!("{resource}LeechGainPerHit"), expected);
        }
    }
}

#[test]
fn own_damage_over_time_uses_resistance_but_never_enemy_damage() {
    let mut env = run(
        vec![
            Modifier::number("LifeRegen", ModType::Base, 100.0),
            Modifier::number("FireDegen", ModType::Base, 200.0),
            Modifier::number("FireResistance", ModType::Base, 75.0),
            Modifier::number("FireDamageTakenWhenHit", ModType::More, 100.0),
        ],
        0.0,
    );
    value(&env, "TotalBuildDegen", 50.0);
    value(&env, "NetLifeRegen", 50.0);
    env.enemy
        .mod_db
        .add_mod(Modifier::number("FireDegen", ModType::Base, 10000.0));
    let mut repeated = Env::new(env.player.clone());
    repeated.cfg = env.cfg;
    repeated.enemy = env.enemy;
    perform(&mut repeated).unwrap();
    value(&repeated, "TotalBuildDegen", 50.0);
}

#[test]
fn regeneration_can_recover_another_resource_without_using_its_regen_modifiers() {
    let env = run(
        vec![
            Modifier::number("ManaRegen", ModType::Base, 30.04),
            Modifier::number("ManaRegen", ModType::More, -40.0),
            Modifier::number("WardRegen", ModType::Base, 2.0),
            Modifier::number("WardRegen", ModType::Inc, 100.0),
            Modifier::flag("ManaRegenerationRecoversWard"),
        ],
        0.0,
    );
    value(&env, "ManaRegenRecovery", 18.0);
    value(&env, "WardRegenRecovery", 22.024);
}

#[test]
fn own_energy_shield_drain_is_counted_once_and_can_make_net_recovery_negative() {
    let env = run(
        vec![
            Modifier::number("EnergyShieldRegen", ModType::Base, 10.0),
            Modifier::number("EnergyShieldDegenPercent", ModType::Base, 5.0),
            Modifier::number("EnergyShieldDegen", ModType::Base, 20.0),
        ],
        0.0,
    );
    value(&env, "EnergyShieldRegenRecovery", -25.0);
    value(&env, "NetEnergyShieldRegen", -25.0);
    value(&env, "TotalBuildDegen", 35.0);
    value(&env, "TotalNetRegen", -25.0);
}

#[test]
fn leech_caps_and_instant_recovery_do_not_cap_gain_on_hit() {
    let env = run(
        vec![
            Modifier::number("PhysicalDamageLifeLeech", ModType::Base, 50.0),
            Modifier::number("PhysicalDamageManaLeech", ModType::Base, 50.0),
            Modifier::number("PhysicalDamageEnergyShieldLeech", ModType::Base, 50.0),
            Modifier::number("LifeOnHit", ModType::Base, 20.0),
            Modifier::number("InstantLifeLeech", ModType::Base, 100.0),
        ],
        2.0,
    );
    value(&env, "LifeLeechGainPerHit", 520.0);
    value(&env, "LifeLeechGainRate", 1040.0);
    value(&env, "ManaLeechGainPerHit", 500.0);
    value(&env, "ManaLeechGainRate", 100.0);
    value(&env, "EnergyShieldLeechGainPerHit", 500.0);
    value(&env, "EnergyShieldLeechGainRate", 30.0);
}

#[test]
fn net_recovery_spends_all_resources_at_cast_rate_not_periodic_hit_rate() {
    let regeneration = vec![
        Modifier::number("LifeRegen", ModType::Base, 100.0),
        Modifier::number("ManaRegen", ModType::Base, 50.0),
        Modifier::number("EnergyShieldRegen", ModType::Base, 30.0),
        Modifier::number("WardRegen", ModType::Base, 5.0),
        Modifier::number("PeriodicHitInterval", ModType::Base, 100.0),
    ];
    let bare = run(regeneration.clone(), 2.0);
    let mut spending = regeneration;
    for (resource, cost) in [
        ("Life", 10.0),
        ("Mana", 20.0),
        ("ES", 3.0),
        ("Ward", 4.0),
        ("Rage", 5.0),
    ] {
        spending.push(Modifier::number(
            format!("Skill{resource}CostBase"),
            ModType::Base,
            cost,
        ));
    }
    let paid = run(spending, 2.0);
    assert_eq!(paid.player.output.effective_action_rate, 10.0);
    assert_eq!(
        bare.player.output.net_life_regen - paid.player.output.net_life_regen,
        20.0
    );
    assert_eq!(
        bare.player.output.net_mana_regen - paid.player.output.net_mana_regen,
        40.0
    );
    value(&paid, "NetEnergyShieldRegen", 24.0);
    value(&paid, "TotalNetRegen", 111.0);
    value(&paid, "TotalBuildDegen", 84.0);
}

#[test]
fn net_recovery_converts_percent_spending_to_each_resource_pool() {
    let env = run(
        vec![
            Modifier::number("Ward", ModType::Base, 100.0),
            Modifier::number("SkillLifePercentCostBase", ModType::Base, 1.0),
            Modifier::number("SkillManaPercentCostBase", ModType::Base, 2.0),
            Modifier::number("SkillESPercentCostBase", ModType::Base, 3.0),
            Modifier::number("SkillWardPercentCostBase", ModType::Base, 4.0),
            Modifier::number("SkillLifePerSecondCostBase", ModType::Base, 3.0),
            Modifier::number("SkillManaPerSecondCostBase", ModType::Base, 7.0),
        ],
        2.0,
    );
    value(&env, "NetLifeRegen", -23.0);
    value(&env, "NetManaRegen", -27.0);
    value(&env, "NetEnergyShieldRegen", -18.0);
    value(&env, "TotalNetRegen", -76.0);
    value(&env, "TotalBuildDegen", 76.0);
}

#[test]
fn net_life_and_mana_recovery_follow_the_channel_use_cycle() {
    let mut modifiers = vec![
        Modifier::flag("ChannelCycle"),
        Modifier::number("ChannelMaxStages", ModType::Base, 2.0),
        Modifier::number("ChannelMinimumTime", ModType::Base, 0.3),
        Modifier::number("ChannelReleaseTime", ModType::Base, 0.2),
        Modifier::number("AttackSpeed", ModType::Inc, 100.0),
        Modifier::number("LifeRegen", ModType::Base, 100.0),
        Modifier::number("ManaRegen", ModType::Base, 50.0),
    ];
    let bare = run(modifiers.clone(), 2.0);
    modifiers.extend([
        Modifier::number("SkillLifeCostBase", ModType::Base, 10.0),
        Modifier::number("SkillManaCostBase", ModType::Base, 20.0),
    ]);
    let paid = run(modifiers, 2.0);
    assert_eq!(paid.player.output.channel_time, 0.5);
    assert!(
        (bare.player.output.net_life_regen - paid.player.output.net_life_regen - 10.0 / 0.7).abs()
            < 1e-6
    );
    assert!(
        (bare.player.output.net_mana_regen - paid.player.output.net_mana_regen - 20.0 / 0.7).abs()
            < 1e-6
    );
}

#[test]
fn leech_recovers_over_one_second_without_a_pool_based_amount_cap() {
    // 50% of a 1000 hit is 500 Life, even though the pool is only 1000.
    // 0.1 hits/s yields 50/s; the full amount per hit is still 500.
    let env = run(
        vec![Modifier::number(
            "PhysicalDamageLifeLeech",
            ModType::Base,
            50.0,
        )],
        0.1,
    );
    value(&env, "LifeLeechGainPerHit", 500.0);
    value(&env, "LifeLeechGainRate", 50.0);
    let env = run(
        vec![Modifier::number(
            "PhysicalDamageLifeLeech",
            ModType::Base,
            2.0,
        )],
        10.0,
    );
    // 20 Life recovered over one second; ten overlapping hits do not stack.
    value(&env, "LifeLeechGainRate", 20.0);
}

#[test]
fn leech_applies_increased_maximum_before_the_rate_cap() {
    // 500 Life per instance, 100% increased maximum: 1000 * 20% * 2 = 400/s.
    let env = run(
        vec![
            Modifier::number("PhysicalDamageLifeLeech", ModType::Base, 50.0),
            Modifier::number("MaxLifeLeechRate", ModType::Inc, 100.0),
        ],
        10.0,
    );
    value(&env, "LifeLeechGainRate", 400.0);
    value(&env, "LifeLeechGainPerHit", 500.0);
}

#[test]
fn faster_leech_changes_rate_but_not_recovered_amount() {
    // base_life_leech_rate_+%: "Leech Life 100% faster".
    // 20 Life / (1 second / 2) = 40/s, with the same 20 Life per hit.
    let env = run(
        vec![
            Modifier::number("PhysicalDamageLifeLeech", ModType::Base, 2.0),
            Modifier::number("LifeLeechRate", ModType::Inc, 100.0),
        ],
        10.0,
    );
    value(&env, "LifeLeechGainRate", 40.0);
    value(&env, "LifeLeechGainPerHit", 20.0);
}

#[test]
fn increased_leech_amount_is_distinct_from_increased_maximum_rate() {
    // base_life_leech_amount_+%: 50% increased amount turns 20 Life into
    // 30 Life. It does not merely raise the unused 200/s recovery ceiling.
    let parsed = crate::support::parse_mod("50% increased amount of Life Leeched").unwrap();
    assert_eq!(parsed.mods[0].name.as_str(), "LifeLeechAmount");
    let mut mods = parsed.mods;
    mods.push(Modifier::number(
        "PhysicalDamageLifeLeech",
        ModType::Base,
        2.0,
    ));
    let env = run(mods, 10.0);
    value(&env, "LifeLeechGainPerHit", 30.0);
    value(&env, "LifeLeechGainRate", 30.0);
}

#[test]
fn strongest_leech_instance_recovers_instead_of_averaging_critical_rates() {
    // Noncritical hits leech 20, critical hits 40. At 10 hits/s and 50%
    // critical chance the stronger one-second instances sustain 40/s.
    let env = run(
        vec![
            Modifier::number("PhysicalDamageLifeLeech", ModType::Base, 2.0),
            Modifier::number("CritChanceBase", ModType::Override, 50.0),
            Modifier::number("CritMultiplier", ModType::Base, 100.0),
        ],
        10.0,
    );
    value(&env, "LifeLeechGainRate", 40.0);
}

#[test]
fn ghost_reaver_converts_base_leech_and_uses_energy_shield_modifiers() {
    // convert_all_life_leech_to_energy_shield_leech and the game's Stat
    // Conversion keyword: base values transfer; destination modifiers apply.
    let mut mods = Vec::new();
    for text in [
        "Life Leech is Converted to Energy Shield Leech",
        "Leech 2% of Physical Attack Damage as Life",
    ] {
        mods.extend(crate::support::parse_mod(text).unwrap().mods);
    }
    mods.extend([
        Modifier::number("PhysicalDamageEnergyShieldLeech", ModType::Base, 1.0),
        Modifier::number("LifeOnHit", ModType::Base, 5.0),
        Modifier::number("EnergyShieldOnHit", ModType::Base, 4.0),
        Modifier::number("LifeLeechAmount", ModType::Inc, 900.0),
        Modifier::number("LifeLeechRate", ModType::Inc, 900.0),
        Modifier::number("LifeRecoveryRate", ModType::Inc, 900.0),
        Modifier::flag("InstantLifeLeech"),
        Modifier::number("EnergyShieldLeechAmount", ModType::Inc, 100.0),
        Modifier::number("EnergyShieldLeechRate", ModType::Inc, 100.0),
        Modifier::number("MaxEnergyShieldLeechRate", ModType::Inc, 100.0),
        Modifier::number("EnergyShieldRecoveryRate", ModType::Inc, 50.0),
    ]);
    let env = run(mods.clone(), 2.0);
    // (2% converted + 1% native) * 1000 * 2 amount = 60 per instance.
    // Recovery is 90 per hit; the ES cap is 300 * 10% * 2 = 60/s before
    // recovery rate. Neither Life's pool nor its modifiers contribute.
    value(&env, "LifeLeechGainPerHit", 5.0);
    value(&env, "LifeLeechGainRate", 10.0);
    value(&env, "EnergyShieldLeechGainPerHit", 94.0);
    value(&env, "EnergyShieldLeechGainRate", 98.0);
    let panel = pobr_core::calc::calc_leech_from_db(
        &env.player.mod_db,
        &env.cfg,
        env.player.output.energy_shield,
        1000.0,
        pobr_core::calc::LeechResource::EnergyShield,
    );
    assert_eq!(panel.instance_total, 60.0);
    assert_eq!(panel.rate_cap_per_second, 60.0);
    assert_eq!(panel.display_rate_per_second, 60.0);

    mods.push(Modifier::number(
        "InstantEnergyShieldLeech",
        ModType::Base,
        25.0,
    ));
    let env = run(mods, 2.0);
    // Instant ES recovery is 15/hit; the remaining 45 receives recovery rate.
    value(&env, "EnergyShieldLeechGainPerHit", 86.5);
    value(&env, "EnergyShieldLeechGainRate", 128.0);
}

#[test]
fn ghost_reaver_retains_damage_eligibility_and_destination_restrictions() {
    for leech_name in ["PhysicalDamageLifeLeech", "LifeLeech"] {
        let mods = vec![
            Modifier::number(leech_name, ModType::Base, 2.0),
            Modifier::flag("GhostReaver"),
            Modifier::flag("LifeLeechBasedOnElementalDamage"),
            Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 100.0),
            // Converted recovery does not require recovering Life.
            Modifier::flag("CannotGainLife"),
        ];
        let env = run(mods.clone(), 2.0);
        value(&env, "LifeLeechGainRate", 0.0);
        value(&env, "EnergyShieldLeechGainRate", 20.0);
        value(&env, "EnergyShieldLeechGainPerHit", 20.0);
        for flag in ["CannotGainEnergyShield", "CannotLeechEnergyShield"] {
            let mut blocked = mods.clone();
            blocked.push(Modifier::flag(flag));
            let env = run(blocked, 2.0);
            value(&env, "EnergyShieldLeechGainRate", 0.0);
            let panel = pobr_core::calc::calc_leech_from_db(
                &env.player.mod_db,
                &env.cfg,
                env.player.output.energy_shield,
                1000.0,
                pobr_core::calc::LeechResource::EnergyShield,
            );
            assert_eq!(panel.instance_total, 0.0);
            assert_eq!(panel.display_rate_per_second, 0.0);
        }
    }
}

#[test]
fn parsed_mystic_harvest_extends_physical_attack_leech_before_ghost_reaver() {
    let leech = crate::support::parse_mod("Leech 2% of Physical Attack Damage as Life")
        .unwrap()
        .mods;
    let harvest = crate::support::parse_mod(
        "[LifeLeech|Life Leech] recovers based on your [ElementalDamage|Elemental] damage as well as [Physical] damage",
    ).unwrap().mods;
    assert!(
        harvest
            .iter()
            .any(|m| m.name.as_str() == "LifeLeechBasedOnElementalDamage")
    );
    let ghost = crate::support::parse_mod("Life Leech is Converted to Energy Shield Leech")
        .unwrap()
        .mods;
    for conversion in [50.0, 100.0] {
        let mut mods = leech.clone();
        mods.extend(harvest.clone());
        mods.extend(ghost.clone());
        mods.extend([
            Modifier::number("PhysicalDamageConvertToFire", ModType::Base, conversion),
            Modifier::number("ChaosDamageMin", ModType::Base, 1000.0),
            Modifier::number("ChaosDamageMax", ModType::Base, 1000.0),
        ]);
        let env = run(mods, 2.0);
        // 1000 Physical/Fire damage funds 20 ES; the extra Chaos funds none.
        value(&env, "LifeLeechGainPerHit", 0.0);
        value(&env, "EnergyShieldLeechGainPerHit", 20.0);
        value(&env, "EnergyShieldLeechGainRate", 20.0);
        let mut spell = Env::new(env.player.clone());
        spell.cfg = CalcConfig::spell();
        perform(&mut spell).unwrap();
        value(&spell, "EnergyShieldLeechGainPerHit", 0.0);
    }
    for mut mods in [leech, harvest] {
        mods.extend(ghost.clone());
        mods.push(Modifier::number(
            "PhysicalDamageConvertToFire",
            ModType::Base,
            100.0,
        ));
        // Conversion alone cannot extend physical eligibility, and Mystic
        // Harvest alone cannot grant a leech percentage.
        value(&run(mods, 2.0), "EnergyShieldLeechGainPerHit", 0.0);
    }
}
