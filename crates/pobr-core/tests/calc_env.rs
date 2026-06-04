use pobr_core::calc::{
    Actor, ActorBaseStats, BreakdownTable, Env, OutputTable, armour_reduction, hit_chance,
    monster_hit_chance, perform,
};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

#[test]
fn perform_writes_player_output_and_breakdown() {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            life: 1_000.0,
            mana: 100.0,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            accuracy: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            hit_min: 100.0,
            hit_max: 200.0,
            action_rate: 2.0,
        },
    );
    player
        .mod_db
        .add_mod(Modifier::number("MaximumLife", ModType::Base, 100.0));
    player
        .mod_db
        .add_mod(Modifier::number("MaximumLife", ModType::Inc, 20.0));
    player
        .mod_db
        .add_mod(Modifier::number("FireResistance", ModType::Base, 40.0));
    player.mod_db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );
    player.mod_db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0).with_flags(ModFlags::ATTACK),
    );
    player
        .mod_db
        .add_mod(Modifier::number("AttackSpeed", ModType::Inc, 10.0).with_flags(ModFlags::ATTACK));

    let mut env = Env::new(player).with_config(CalcConfig::attack());

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.life, 1_320.0);
    assert_eq!(env.player.output.fire_resistance, 40.0);
    assert_eq!(env.player.output.total_hit_avg, 270.0);
    assert_eq!(env.player.output.action_rate, 2.2);
    assert_eq!(env.player.output.dps, 594.0);
    assert_eq!(env.player.output.armour, 0.0);
    assert_eq!(env.player.output.evasion, 0.0);
    assert_eq!(env.player.output.energy_shield, 0.0);
    assert!(env.player.breakdown.contains("life"));
    assert!(env.player.breakdown.contains("fire_resistance"));
    assert!(env.player.breakdown.contains("total_hit_avg"));
    assert!(env.player.breakdown.contains("dps"));
}

#[test]
fn perform_writes_hit_chance_and_applies_it_to_dps() {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            accuracy: 600.0,
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 1.0,
            ..ActorBaseStats::default()
        },
    );
    player
        .mod_db
        .add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 100.0));

    let enemy = Actor::new(
        1,
        ActorBaseStats {
            evasion: 1_000.0,
            ..ActorBaseStats::default()
        },
    );
    let expected_hit_chance = hit_chance(enemy.base.evasion, player.base.accuracy);

    let mut env = Env::new(player);
    env.enemy = enemy;

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.total_hit_avg, 200.0);
    assert_eq!(env.player.output.hit_chance, expected_hit_chance);
    assert_eq!(env.player.output.dps, 200.0 * expected_hit_chance);
    assert!(env.player.breakdown.contains("hit_chance"));
}

#[test]
fn output_and_breakdown_have_clear_defaults() {
    let output = OutputTable::default();
    let breakdown = BreakdownTable::default();

    assert_eq!(output.life, 0.0);
    assert_eq!(output.dps, 0.0);
    assert_eq!(output.armour, 0.0);
    assert_eq!(output.hit_chance, 0.0);
    assert_eq!(output.chance_to_be_hit, 0.0);
    assert!(breakdown.is_empty());
}

#[test]
fn perform_writes_defence_output_and_breakdown() {
    let mut player = Actor::new(
        90,
        ActorBaseStats {
            life: 1_000.0,
            mana: 100.0,
            armour: 1_000.0,
            evasion: 500.0,
            energy_shield: 200.0,
            accuracy: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            hit_min: 0.0,
            hit_max: 0.0,
            action_rate: 0.0,
        },
    );
    player
        .mod_db
        .add_mod(Modifier::number("Armour", ModType::Base, 500.0));
    player
        .mod_db
        .add_mod(Modifier::number("Armour", ModType::Inc, 20.0));
    player
        .mod_db
        .add_mod(Modifier::number("Evasion", ModType::Inc, 100.0));
    player
        .mod_db
        .add_mod(Modifier::number("EnergyShield", ModType::Base, 50.0));

    let mut env = Env::new(player);
    env.enemy.base.accuracy = 600.0;

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.armour, 1_800.0);
    assert_eq!(env.player.output.evasion, 1_000.0);
    assert_eq!(env.player.output.energy_shield, 250.0);
    // 防御侧（怪物命中玩家）用 monster_hit_chance（PoE2 双侧公式不对称）。
    // 出处：agent-docs/accuracy-and-enemy.md §二注、CalcDefence.lua calcs.monsterHitChance
    assert_eq!(
        env.player.output.chance_to_be_hit,
        monster_hit_chance(env.player.output.evasion, 600.0)
    );
    assert!(env.player.breakdown.contains("armour"));
    assert!(env.player.breakdown.contains("evasion"));
    assert!(env.player.breakdown.contains("energy_shield"));
    assert!(env.player.breakdown.contains("chance_to_be_hit"));
}

#[test]
fn hit_chance_is_clamped_to_path_of_exile_bounds() {
    assert_eq!(hit_chance(0.0, 0.0), 1.0);
    assert_eq!(hit_chance(1_000_000.0, 1.0), 0.05);
    assert_eq!(hit_chance(1.0, 1_000_000.0), 1.0);
}

#[test]
fn armour_reduction_uses_armour_against_raw_hit() {
    assert_eq!(armour_reduction(0.0, 1_000.0), 0.0);
    assert_eq!(armour_reduction(10_000.0, 1_000.0), 0.5);
    assert_eq!(armour_reduction(10_000.0, 0.0), 0.0);
}

/// Bug#3 测试：PoE2 进攻侧命中率公式（accuracy*1.25/(accuracy+evasion*0.3)）。
///
/// 出处：agent-docs/accuracy-and-enemy.md §二、CalcDefence.lua `calcs.hitChance`。
#[test]
fn poe2_attack_hit_chance_formula() {
    // accuracy=1000, evasion=1000
    // raw = 1000*1.25/(1000+1000*0.3) = 1250/1300 ≈ 0.9615
    let hc = hit_chance(1000.0, 1000.0);
    let expected = (1000.0_f64 * 1.25 / (1000.0 + 1000.0 * 0.3)).clamp(0.05, 1.0);
    assert!(
        (hc - expected).abs() < 1e-6,
        "hit_chance={hc}, expected={expected}"
    );
}

/// Bug#3 测试：防御侧公式不同（怪物命中玩家）。
///
/// 出处：agent-docs/accuracy-and-enemy.md §二注、CalcDefence.lua `calcs.monsterHitChance`。
#[test]
fn poe2_monster_hit_chance_formula() {
    // evasion=1000, accuracy=1000
    // raw = 1 - 0.95*1000/(1000+4*1000) = 1 - 0.95/5 = 1 - 0.19 = 0.81
    let mhc = monster_hit_chance(1000.0, 1000.0);
    let raw_expected: f64 = 1.0 - 0.95_f64 * 1000.0 / (1000.0 + 4.0 * 1000.0);
    let expected = raw_expected.clamp(0.05_f64, 1.0_f64);
    assert!(
        (mhc - expected).abs() < 1e-6,
        "monster_hit_chance={mhc}, expected={expected}"
    );
    // 两侧公式应不同
    assert_ne!(
        hit_chance(1000.0, 1000.0),
        monster_hit_chance(1000.0, 1000.0)
    );
}
