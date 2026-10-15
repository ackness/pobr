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
    value(&fast, "LifeLeechGainRate", 70.0);
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
    value(&env, "LifeLeechGainPerHit", 120.0);
    value(&env, "LifeLeechGainRate", 240.0);
    value(&env, "ManaLeechGainPerHit", 50.0);
    value(&env, "ManaLeechGainRate", 100.0);
    value(&env, "EnergyShieldLeechGainPerHit", 30.0);
    value(&env, "EnergyShieldLeechGainRate", 30.0);
}
