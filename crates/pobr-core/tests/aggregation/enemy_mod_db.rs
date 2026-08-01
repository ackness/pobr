//! Step-2: integration tests wiring enemy.mod_db into mode_effective — the damage-taken
//! chain, strongest-of exposure, CannotBeEvaded, enemy block chance, and traceable
//! enemy contributions.
//!
//! Sources: agent-docs/accuracy-and-enemy.md §二,§五,§六,§七; agent-docs/debuffs.md §曝光;
//!       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2,§5.

use pobr_core::calc::setup_env::{env_with_enemy, reduce_enemy_exposure, setup_enemy};
use pobr_core::calc::{
    Actor, ActorBaseStats, Env, MinimalInput, calculate_minimal, calculate_minimal_traced,
    calculate_minimal_traced_vs_enemy, calculate_minimal_vs_enemy, perform,
};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

/// Standard attack input: 100-200 physical, 1/s, guaranteed hit (enemy has no evasion).
fn attack_input() -> MinimalInput {
    MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
    }
}

fn effective_attack() -> CalcConfig {
    CalcConfig::attack().with_mode_effective(true)
}

// 1. Enemy DamageTaken raises effective DPS (enemy-damage-taken-chain).

#[test]
fn enemy_damage_taken_inc_raises_effective_dps() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("DamageTaken", ModType::Inc, 20.0));

    let input = attack_input();
    // Panel mode (mode_effective=false): DamageTaken has no effect.
    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    // Effective mode: the +20% damage-taken chain applies.
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);

    assert_eq!(
        panel.dps, 150.0,
        "no effective: physical 150 average x 1/s x 100% hit chance"
    );
    assert!(
        (effective.dps - 180.0).abs() < 1e-6,
        "effective DamageTaken+20% → 150*1.2 = 180, got {}",
        effective.dps
    );
}

#[test]
fn enemy_damage_taken_more_multiplies() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("DamageTaken", ModType::More, 50.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (out.dps - 225.0).abs() < 1e-6,
        "150 * 1.5 = 225, got {}",
        out.dps
    );
}

#[test]
fn enemy_typed_damage_taken_only_affects_that_type() {
    // FireDamageTaken only affects fire damage; a pure physical hit is untouched.
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireDamageTaken", ModType::Inc, 100.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert_eq!(
        out.dps, 150.0,
        "pure physical is unaffected by FireDamageTaken"
    );
}

// 2. Enemy resistance / armour mitigation (effective mode only).

#[test]
fn enemy_fire_resist_reduces_fire_dps_only_in_effective() {
    let mut player = ModDb::new();
    // Add 100 fire damage as a per-type flat.
    player.add_mod(Modifier::number("FireDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FireDamageMax", ModType::Base, 100.0));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    // base_hit 0: isolates the fire damage component.
    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);

    assert_eq!(
        panel.dps, 100.0,
        "panel: 100 fire damage, resist not deducted"
    );
    assert!(
        (effective.dps - 50.0).abs() < 1e-6,
        "effective 50% fire resist -> 100*0.5 = 50, got {}",
        effective.dps
    );
}

#[test]
fn enemy_armour_reduces_physical_dps_in_effective() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    // armour=1500, raw_hit=150 → reduction = 1500/(1500+10*150)=0.5
    enemy.add_mod(Modifier::number("Armour", ModType::Base, 1500.0));

    let effective =
        calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (effective.dps - 75.0).abs() < 1e-6,
        "armour 50% mitigation -> 150*0.5 = 75, got {}",
        effective.dps
    );
}

/// Pure fire-damage input (base_hit 0, a flat 100 fire roll).
fn fire_only_input() -> MinimalInput {
    MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    }
}

fn fire_only_player() -> ModDb {
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("FireDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FireDamageMax", ModType::Base, 100.0));
    player
}

// Final resistance semantics match vendor calcResistForType (CalcOffence.lua:530-543).

#[test]
fn enemy_shared_elemental_resist_applies_to_elements_not_chaos() {
    // The shared name `ElementalResist BASE` applies to fire/cold/lightning (vendor :539
    // merges names via isElemental); chaos is excluded (isElemental[Chaos]=false).
    let player = fire_only_player();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("ElementalResist", ModType::Base, 40.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 60.0).abs() < 1e-6,
        "ElementalResist 40 -> fire damage 100*0.6 = 60, got {}",
        out.dps
    );

    // Chaos damage does not benefit from ElementalResist.
    let mut chaos_player = ModDb::new();
    chaos_player.add_mod(Modifier::number("ChaosDamageMin", ModType::Base, 100.0));
    chaos_player.add_mod(Modifier::number("ChaosDamageMax", ModType::Base, 100.0));
    let chaos_out = calculate_minimal_vs_enemy(
        &chaos_player,
        &enemy,
        &effective_attack(),
        &fire_only_input(),
    );
    assert!(
        (chaos_out.dps - 100.0).abs() < 1e-6,
        "chaos damage is not affected by ElementalResist, got {}",
        chaos_out.dps
    );
}

#[test]
fn enemy_resist_inc_scaling_applies_before_clamp() {
    // INC scaling on the resist itself (vendor :539 `× calcLib.mod(...)`): 30 BASE × (1+50%) = 45.
    let player = fire_only_player();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 30.0));
    enemy.add_mod(Modifier::number("FireResist", ModType::Inc, 50.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 55.0).abs() < 1e-6,
        "30x1.5=45 resist -> 100*0.55 = 55, got {}",
        out.dps
    );
}

#[test]
fn enemy_resist_negative_scale_floors_at_zero() {
    // Scaling factor floors at `max(..., 0)` (vendor :539 m_max(calcLib.mod, 0)): -150% INC → factor 0 → resist 0.
    let player = fire_only_player();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));
    enemy.add_mod(Modifier::number("FireResist", ModType::Inc, -150.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 100.0).abs() < 1e-6,
        "scale floors at 0 -> resist 0 -> 100, got {}",
        out.dps
    );
}

#[test]
fn enemy_resist_override_wins_over_base_sum() {
    // Override wins (vendor :531 `enemyDB:Override(cfg, ..)`; the config "treat as 0 resist" case).
    let player = fire_only_player();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));
    enemy.add_mod(Modifier::number("FireResist", ModType::Override, 0.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 100.0).abs() < 1e-6,
        "Override 0 resist -> 100, got {}",
        out.dps
    );
}

#[test]
fn enemy_resist_clamps_to_enemy_max_resist() {
    // BASE 90 clamps to EnemyMaxResist 75 (Data.lua:200 = 75).
    let player = fire_only_player();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 90.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 25.0).abs() < 1e-6,
        "clamp 75 → 100*0.25 = 25, got {}",
        out.dps
    );
}

/// (vendor CalcOffence.lua:532): an explicit `enemy<Type>Resist` config input can raise
/// maxResist from EnemyMaxResist(75) up to MaxResistCap(90); the `DoNotChangeMaxResFromConfig`
/// flag (config "always 75%", ConfigOptions.lua:2158-2159) pins it back to 75 when set.
#[test]
fn explicit_config_resist_raises_max_resist_cap() {
    let config_origin = || {
        ModifierSource::new(SourceId::new(
            SourceKind::EnemyConfig,
            "config.enemyFireResist",
        ))
    };
    let player = fire_only_player();

    // Explicit config 85 (injected as BASE with EnemyConfig/config.<var> origin, the
    // config_resolve shape) → maxResist = min(max(85, 75), 90) = 85 → 100×0.15 = 15.
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 85.0).with_origin(config_origin()));
    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 15.0).abs() < 1e-6,
        "config 85 raises the cap -> 100*0.15 = 15, got {}",
        out.dps
    );

    // Explicit config 95 caps at MaxResistCap 90 (Data.lua:181) → 100×0.10 = 10.
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 95.0).with_origin(config_origin()));
    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 10.0).abs() < 1e-6,
        "config 95 → cap 90 → 100*0.10 = 10, got {}",
        out.dps
    );

    // DoNotChangeMaxResFromConfig set → pinned at 75 (vendor :532 first branch).
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 85.0).with_origin(config_origin()));
    enemy.add_mod(Modifier::flag("DoNotChangeMaxResFromConfig"));
    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 25.0).abs() < 1e-6,
        "always-75 flag → clamp 75 → 25, got {}",
        out.dps
    );

    // A non-config origin (e.g. tier presets — EnemyConfig kind but id isn't config.<var>) doesn't raise the cap.
    let mut enemy = ModDb::new();
    enemy.add_mod(
        Modifier::number("FireResist", ModType::Base, 85.0).with_origin(ModifierSource::new(
            SourceId::new(SourceKind::EnemyConfig, "fire_resist"),
        )),
    );
    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 25.0).abs() < 1e-6,
        "a preset origin doesn't raise the cap -> clamp 75 -> 25, got {}",
        out.dps
    );
}

// Damage-taken chain: INC-only additional names (vendor CalcOffence.lua:4141/:4152-4156).

#[test]
fn elemental_damage_taken_applies_to_elements_only() {
    // ElementalDamageTaken INC only feeds elemental types' takenInc (vendor :4141); physical is unaffected.
    let player = fire_only_player();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("ElementalDamageTaken", ModType::Inc, 25.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 125.0).abs() < 1e-6,
        "fire damage is affected by ElementalDamageTaken 25% -> 125, got {}",
        out.dps
    );

    // Pure physical is unaffected.
    let phys_out =
        calculate_minimal_vs_enemy(&ModDb::new(), &enemy, &effective_attack(), &attack_input());
    assert!(
        (phys_out.dps - 150.0).abs() < 1e-6,
        "physical is not affected by ElementalDamageTaken, got {}",
        phys_out.dps
    );
}

#[test]
fn projectile_damage_taken_gated_by_projectile_flag() {
    // ProjectileDamageTaken only feeds takenInc under a projectile cfg (vendor :4152-4153).
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number(
        "ProjectileDamageTaken",
        ModType::Inc,
        40.0,
    ));

    // Non-projectile attack: unaffected.
    let melee = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (melee.dps - 150.0).abs() < 1e-6,
        "non-projectile is not affected by ProjectileDamageTaken, got {}",
        melee.dps
    );

    // Projectile attack: applies.
    let proj_cfg = effective_attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE);
    let proj = calculate_minimal_vs_enemy(&player, &enemy, &proj_cfg, &attack_input());
    assert!(
        (proj.dps - 210.0).abs() < 1e-6,
        "projectile +40% taken -> 150*1.4 = 210, got {}",
        proj.dps
    );
}

#[test]
fn trap_mine_damage_taken_gated_by_skill_types() {
    // (h3): TrapMineDamageTaken only feeds takenInc for trap/mine skills (vendor
    // CalcOffence.lua:4158-4159 `if skillFlags.trap or skillFlags.mine`; PoBR expresses
    // this via cfg.skill_types containing Trapped(33)/RemoteMined(36)).
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("TrapMineDamageTaken", ModType::Inc, 30.0));

    // Plain attack: unaffected.
    let plain = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (plain.dps - 150.0).abs() < 1e-6,
        "non trap/mine is not affected by TrapMineDamageTaken, got {}",
        plain.dps
    );

    // Trap skill: applies (mine follows the same or-logic).
    let trap_cfg = effective_attack().with_skill_types(SkillTypes::ATTACK | SkillTypes::TRAPPED);
    let trap = calculate_minimal_vs_enemy(&player, &enemy, &trap_cfg, &attack_input());
    assert!(
        (trap.dps - 195.0).abs() < 1e-6,
        "trap +30% taken → 150*1.3 = 195, got {}",
        trap.dps
    );

    let mine_cfg =
        effective_attack().with_skill_types(SkillTypes::ATTACK | SkillTypes::REMOTE_MINED);
    let mine = calculate_minimal_vs_enemy(&player, &enemy, &mine_cfg, &attack_input());
    assert!(
        (mine.dps - 195.0).abs() < 1e-6,
        "mine +30% taken → 150*1.3 = 195, got {}",
        mine.dps
    );
}

// Physical mitigation uses an additive formula (vendor CalcOffence.lua:4074-4096).

#[test]
fn enemy_physical_reduction_is_additive_not_multiplicative_union() {
    // armour=1500/raw=150 → 50% armour mitigation; a flat enemy PDR 20 → additive 70%
    // (a multiplicative union would give 1-(1-0.5)(1-0.8)=60%; vendor :4095 adds instead).
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("Armour", ModType::Base, 1500.0));
    enemy.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        20.0,
    ));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (out.dps - 45.0).abs() < 1e-6,
        "additive 50+20=70% → 150*0.3 = 45, got {}",
        out.dps
    );
}

#[test]
fn enemy_physical_reduction_caps_at_75() {
    // PDR 60 + armour 50% = 110 → clamped to EnemyPhysicalDamageReductionCap 75.
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("Armour", ModType::Base, 1500.0));
    enemy.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        60.0,
    ));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (out.dps - 37.5).abs() < 1e-6,
        "cap 75% → 150*0.25 = 37.5, got {}",
        out.dps
    );
}

#[test]
fn enemy_negative_physical_reduction_amplifies_up_to_neg_cap() {
    // Armour driven negative (Armour Override −1500, raw 150) → −50% mitigation (i.e. a
    // damage bonus), floored at −NegArmourDmgBonusCap(−100) (vendor :4095 m_max(-100, ..)).
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("Armour", ModType::Override, -1500.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (out.dps - 225.0).abs() < 1e-6,
        "negative armour −50% mitigation -> 150*1.5 = 225, got {}",
        out.dps
    );

    // Extreme negative mitigation clamps at −100 (damage capped at ×2).
    let mut enemy2 = ModDb::new();
    enemy2.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        -400.0,
    ));
    let out2 = calculate_minimal_vs_enemy(&player, &enemy2, &effective_attack(), &attack_input());
    assert!(
        (out2.dps - 300.0).abs() < 1e-6,
        "negative mitigation clamps at −100 -> 150*2 = 300, got {}",
        out2.dps
    );
}

#[test]
fn ignore_enemy_armour_flag_zeroes_armour_component() {
    // Player IgnoreEnemyArmour flag (vendor :4084-4085) → enemy armour counts as 0, leaving only the flat PDR.
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("IgnoreEnemyArmour"));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("Armour", ModType::Base, 1500.0));
    enemy.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        20.0,
    ));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (out.dps - 120.0).abs() < 1e-6,
        "ignore enemy armour -> only PDR 20% -> 150*0.8 = 120, got {}",
        out.dps
    );
}

// Penetration floor minPen (vendor CalcOffence.lua:4140/:4163).

#[test]
fn penetration_minimum_caps_penetration_floor() {
    // Resist 50, pen 30, minPen 35 → max(50-30, 35) = 35 → fire damage 100*0.65 = 65.
    let mut player = fire_only_player();
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 30.0));
    player.add_mod(Modifier::number(
        "FirePenetrationMinimum",
        ModType::Base,
        35.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 65.0).abs() < 1e-6,
        "minPen 35 raises the floor -> 100*0.65 = 65, got {}",
        out.dps
    );
}

#[test]
fn penetration_skipped_when_resist_at_or_below_min_pen() {
    // Resist 30 ≤ minPen 30 → penetration is skipped entirely (vendor's `resist > minPen` gate), resist stays 30.
    let mut player = fire_only_player();
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 50.0));
    player.add_mod(Modifier::number(
        "FirePenetrationMinimum",
        ModType::Base,
        30.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 30.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 70.0).abs() < 1e-6,
        "resist <= minPen -> penetration has no effect -> 100*0.7 = 70, got {}",
        out.dps
    );
}

// Hits treat enemy elemental resist as inverted (Rakiata's Flow
// `treat_enemy_resistances_as_negated_…` → HitsInvertEleResChance CHANCE,
// SkillStatMap.lua:941-944; consumed by vendor CalcOffence.lua:4145-4148
// `resist = resist - 2 * invertChance * resist`, after clamp and before penetration).

#[test]
fn hits_invert_ele_res_chance_inverts_enemy_resist() {
    // Resist 50, invert chance 1.0 → resist = 50 - 2*50 = -50 → fire damage 100 × 1.5 = 150.
    let mut player = fire_only_player();
    player.add_mod(Modifier::number(
        "HitsInvertEleResChance",
        ModType::Base,
        1.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 150.0).abs() < 1e-6,
        "resist inverted to -50 -> 100*1.5 = 150, got {}",
        out.dps
    );
}

#[test]
fn hits_invert_partial_chance_blends_resist() {
    // Resist 50, chance 0.5 → resist = 50 - 2*0.5*50 = 0 → fire damage 100 (vendor's linear blend).
    let mut player = fire_only_player();
    player.add_mod(Modifier::number(
        "HitsInvertEleResChance",
        ModType::Base,
        0.5,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 100.0).abs() < 1e-6,
        "chance 0.5 -> resist 0 -> 100, got {}",
        out.dps
    );
}

#[test]
fn hits_invert_applies_before_penetration() {
    // Inversion happens first (vendor :4145 runs before the :4163 effMult penetration step):
    // resist 50 inverts to -50, so resist ≤ minPen(0) and penetration is skipped entirely
    // (wasted), still yielding 1.5×.
    let mut player = fire_only_player();
    player.add_mod(Modifier::number(
        "HitsInvertEleResChance",
        ModType::Base,
        1.0,
    ));
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 30.0));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 150.0).abs() < 1e-6,
        "penetration skipped under negative resist -> 100*1.5 = 150, got {}",
        out.dps
    );
}

#[test]
fn hits_invert_does_not_touch_chaos_or_panel_mode() {
    // Elemental-only (vendor's isElemental gate): chaos resist is never inverted; panel mode skips the enemy mitigation stage entirely.
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("ChaosDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("ChaosDamageMax", ModType::Base, 100.0));
    player.add_mod(Modifier::number(
        "HitsInvertEleResChance",
        ModType::Base,
        1.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("ChaosResist", ModType::Base, 40.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &fire_only_input());
    assert!(
        (out.dps - 60.0).abs() < 1e-6,
        "chaos is not inverted -> 100*0.6 = 60, got {}",
        out.dps
    );
}

// 3. Exposure takes the strongest source (via ModDb::max_of + reduce_enemy_exposure).

#[test]
fn exposure_takes_strongest_single_source() {
    // Two FireExposure BASE mods, 20 and 30 → final FireResist BASE -30 (max, not the 50 sum).
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireExposure", ModType::Base, 20.0));
    enemy.add_mod(Modifier::number("FireExposure", ModType::Base, 30.0));

    reduce_enemy_exposure(&mut enemy, &ModDb::new(), &CalcConfig::attack());

    let fire_resist = enemy.sum(
        ModType::Base,
        &CalcConfig::attack(),
        &[ModName::from("FireResist")],
    );
    assert!(
        (fire_resist + 30.0).abs() < 1e-6,
        "exposure takes the strongest 30 -> FireResist BASE -30, got {}",
        fire_resist
    );
}

// Exposure effect scaling (vendor CalcPerform.lua:3222-3227).

#[test]
fn exposure_effect_inc_scales_magnitude_before_effect_on_self() {
    // Player FireExposureEffect INC 60 (a Potent Exposure-style mod) + boss effect-on-self
    // −50%: floor(20 × 1.6 × 0.5) = 16 (vendor :3227; matches the stormweaver-comet oracle
    // dump of −16).
    let actor = Actor::new(85, ActorBaseStats::default());
    let mut env = Env::new(actor);
    setup_enemy(&mut env, 0, EnemyTier::Pinnacle);
    env.enemy
        .mod_db
        .add_mod(Modifier::number("FireExposure", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("FireExposureEffect", ModType::Inc, 60.0));
    let cfg = effective_attack();
    reduce_enemy_exposure(&mut env.enemy.mod_db, &env.player.mod_db, &cfg);

    let fire = env
        .enemy
        .mod_db
        .sum(ModType::Base, &cfg, &[ModName::from("FireResist")]);
    // Pinnacle base 50 − 16 = 34.
    assert!(
        (fire - 34.0).abs() < 1e-6,
        "exposure 20x1.6x0.5=16 -> FireResist 50-16=34, got {fire}"
    );
}

#[test]
fn extra_exposure_adds_before_scaling() {
    // Player ExtraExposure BASE 10: the extra amount is added before scaling
    // (vendor :3222/:3227): floor((20+10) × 1.0 × 1.0) = 30.
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireExposure", ModType::Base, 20.0));
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("ExtraExposure", ModType::Base, 10.0));

    reduce_enemy_exposure(&mut enemy, &player, &CalcConfig::attack());

    let fire = enemy.sum(
        ModType::Base,
        &CalcConfig::attack(),
        &[ModName::from("FireResist")],
    );
    assert!(
        (fire + 30.0).abs() < 1e-6,
        "(20+10)×1 = 30 → FireResist -30, got {fire}"
    );
}

// Exposure-consuming side: the boss's `ExposureEffectOnSelf MORE -50` halves the exposure
// magnitude in effective mode, and is gated off in panel mode. Cross-checked against
// PoB2 CalcPerform.lua:3225-3227.
#[test]
fn exposure_effect_on_self_halves_magnitude_only_in_effective() {
    let player = Actor::new(85, ActorBaseStats::default());

    // Effective mode: setup_enemy(Pinnacle) injects ExposureEffectOnSelf MORE -50 (gated by Condition:Effective).
    let mut env = Env::new(player.clone());
    setup_enemy(&mut env, 0, EnemyTier::Pinnacle);
    env.enemy
        .mod_db
        .add_mod(Modifier::number("FireExposure", ModType::Base, 25.0));
    let cfg_eff = effective_attack();
    reduce_enemy_exposure(&mut env.enemy.mod_db, &env.player.mod_db, &cfg_eff);
    let fire_eff = env
        .enemy
        .mod_db
        .sum(ModType::Base, &cfg_eff, &[ModName::from("FireResist")]);
    // Pinnacle fire resist base is +50; exposure floor(25 * 0.5) = 12 contributes -12 to
    // the FireResist bucket. Final value: 50 (boss base) + (-12) = 38.
    assert!(
        (fire_eff - 38.0).abs() < 1e-6,
        "effective scope: exposure 25 halved floor=12, FireResist=50-12=38, got {}",
        fire_eff
    );

    // Panel mode: ExposureEffectOnSelf is gated by Condition:Effective → factor 1.0, exposure isn't halved.
    let mut env2 = Env::new(player);
    setup_enemy(&mut env2, 0, EnemyTier::Pinnacle);
    env2.enemy
        .mod_db
        .add_mod(Modifier::number("FireExposure", ModType::Base, 25.0));
    let cfg_panel = CalcConfig::attack();
    reduce_enemy_exposure(&mut env2.enemy.mod_db, &env2.player.mod_db, &cfg_panel);
    let fire_panel =
        env2.enemy
            .mod_db
            .sum(ModType::Base, &cfg_panel, &[ModName::from("FireResist")]);
    // Panel: exposure 25 stays unhalved → FireResist 50 + (-25) = 25.
    assert!(
        (fire_panel - 25.0).abs() < 1e-6,
        "panel scope: exposure not halved, FireResist=50-25=25, got {}",
        fire_panel
    );
}

// Boss debuff effect-on-self is gated: only applies in effective mode, panel mode is always 1.0.
#[test]
fn boss_debuff_effect_on_self_gated_by_effective() {
    let player = Actor::new(85, ActorBaseStats::default());
    let mut env = Env::new(player);
    setup_enemy(&mut env, 0, EnemyTier::Pinnacle);
    let db = &env.enemy.mod_db;

    for name in [
        "CurseEffectOnSelf",
        "ExposureEffectOnSelf",
        "SlowEffectOnSelf",
    ] {
        let panel = db.more(&CalcConfig::attack(), &[ModName::from(name)]);
        assert!(
            (panel - 1.0).abs() < 1e-9,
            "{name} panel scope is gated -> 1.0, got {panel}"
        );
        let eff = db.more(&effective_attack(), &[ModName::from(name)]);
        assert!(
            (eff - 0.5).abs() < 1e-9,
            "{name} effective scope MORE -50 -> 0.5, got {eff}"
        );
    }
}

#[test]
fn max_of_empty_is_zero() {
    let enemy = ModDb::new();
    let m = enemy.max_of(
        ModType::Base,
        &CalcConfig::attack(),
        &[ModName::from("FireExposure")],
    );
    assert_eq!(m, 0.0);
}

// 4. CannotBeEvaded / enemy CannotEvade (cannot-be-evaded-flag).

#[test]
fn cannot_be_evaded_flag_forces_full_hit() {
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("CannotBeEvaded"));
    let enemy = ModDb::new();

    // High-evasion enemy + 0 accuracy would normally hit the floor chance; CannotBeEvaded should force a guaranteed hit.
    let input = MinimalInput {
        enemy_evasion: 10_000.0,
        base_accuracy: 100.0,
        ..attack_input()
    };
    let out = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    assert_eq!(out.hit_chance, 1.0, "CannotBeEvaded -> full hit chance");
    assert_eq!(out.dps, 150.0);
}

#[test]
fn enemy_cannot_evade_flag_forces_full_hit_in_effective() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::flag("CannotEvade"));

    let input = MinimalInput {
        enemy_evasion: 10_000.0,
        base_accuracy: 100.0,
        ..attack_input()
    };
    // Panel mode: CannotEvade has no effect (still runs the accuracy formula, hit chance < 1).
    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    assert!(
        panel.hit_chance < 1.0,
        "enemy CannotEvade has no effect in panel mode"
    );
    // Effective mode: CannotEvade → guaranteed hit.
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);
    assert_eq!(
        effective.hit_chance, 1.0,
        "enemy CannotEvade in effective mode -> full hit chance"
    );
}

// 5. Enemy block chance (enemy-block-chance-hit-chain).

#[test]
fn enemy_block_chance_reduces_hit_in_effective() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("BlockChance", ModType::Base, 25.0));

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &attack_input());
    let effective =
        calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());

    assert_eq!(panel.hit_chance, 1.0, "panel mode doesn't deduct block");
    assert!(
        (effective.hit_chance - 0.75).abs() < 1e-6,
        "25% block -> hit chance x0.75, got {}",
        effective.hit_chance
    );
    assert!(
        (effective.dps - 112.5).abs() < 1e-6,
        "150 * 0.75 = 112.5, got {}",
        effective.dps
    );
}

// 6. mode_effective: panel vs. effective divergence (mode-effective-missing).

#[test]
fn panel_dps_not_lower_than_effective_dps() {
    // Default Pinnacle boss scenario: effective DPS should be <= panel DPS (panel mode doesn't deduct enemy mitigation).
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("FireDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FireDamageMax", ModType::Base, 100.0));

    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };
    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);

    assert!(
        panel.dps >= effective.dps,
        "panel DPS ({}) should be >= effective DPS ({})",
        panel.dps,
        effective.dps
    );
}

#[test]
fn legacy_three_arg_entry_equals_empty_enemy() {
    // calculate_minimal (the old 3-arg entry point) should behave like an empty enemy modDB (backward compatibility).
    let mut player = ModDb::new();
    player.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );
    let input = attack_input();
    let legacy = calculate_minimal(&player, &CalcConfig::attack(), &input);
    let via_empty = calculate_minimal_vs_enemy(&player, &ModDb::new(), &effective_attack(), &input);
    assert_eq!(
        legacy.dps, via_empty.dps,
        "empty enemy + effective matches the legacy entry point"
    );
}

// 7. setup_enemy injection + Pinnacle default tier (setup-env-missing).

#[test]
fn setup_enemy_injects_pinnacle_defaults() {
    let player = Actor::new(85, ActorBaseStats::default());
    let mut env = Env::new(player);
    setup_enemy(&mut env, 0, EnemyTier::Pinnacle); // level 0 → follows the character level, min(85,85)=85

    let cfg = CalcConfig::attack();
    let db = &env.enemy.mod_db;

    // Elemental resist +50% (Pinnacle).
    let fire = db.sum(ModType::Base, &cfg, &[ModName::from("FireResist")]);
    assert_eq!(fire, 50.0, "Pinnacle fire resist +50");
    // Accuracy = monsterAccuracyTable[85] = 2357.
    let acc = db.sum(ModType::Base, &cfg, &[ModName::from("Accuracy")]);
    assert_eq!(acc, monster_accuracy(85) as f64);
    // Generic boss debuff resistances (gated by `Condition:Effective`):
    // - Panel mode (mode_effective=false) doesn't match → factor 1.0 (keeps raw DPS clean).
    let curse_panel = db.more(&cfg, &[ModName::from("CurseEffectOnSelf")]);
    assert!(
        (curse_panel - 1.0).abs() < 1e-9,
        "panel scope CurseEffectOnSelf is gated -> 1.0, got {}",
        curse_panel
    );
    // - Effective DPS mode (mode_effective=true) matches → MORE -50 → 0.5.
    let cfg_eff = effective_attack();
    let curse_eff = db.more(&cfg_eff, &[ModName::from("CurseEffectOnSelf")]);
    assert!(
        (curse_eff - 0.5).abs() < 1e-9,
        "effective scope CurseEffectOnSelf MORE -50 -> 0.5, got {}",
        curse_eff
    );
    // Condition:PinnacleBoss is set.
    assert!(
        db.flag(&cfg, ModName::from("Condition:PinnacleBoss")),
        "Pinnacle sets the condition flag"
    );
    // Level is raised to >=82 by Pinnacle (85 here).
    assert_eq!(env.enemy.level, 85);

    // A boss's innate elemental penetration only flows into the defence-side `Enemy<El>Pen`
    // (enemy modDB, consumed by EHP/hit-taken calc); it must **not** be injected into the
    // player's offensive penetration (vendor `enemy<El>Pen` config var has no apply function,
    // ConfigOptions.lua:2269-2273 — only CalcDefence.lua:2363 consumes it).
    let pen = env.player.mod_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("ElementalPenetration")],
    );
    assert_eq!(
        pen, 0.0,
        "boss penetration must not leak into the player's offensive penetration"
    );
    let def_pen = db.sum(ModType::Base, &cfg, &[ModName::from("EnemyFirePen")]);
    assert_eq!(
        def_pen, 3.0,
        "Pinnacle defence-side EnemyFirePen +3 injected into enemy db"
    );
}

#[test]
fn setup_enemy_uber_injects_damage_taken_penalty() {
    let player = Actor::new(80, ActorBaseStats::default());
    let mut env = Env::new(player);
    setup_enemy(&mut env, 0, EnemyTier::Uber);
    let cfg = CalcConfig::attack();
    let dt = env.enemy.mod_db.more(&cfg, &[ModName::from("DamageTaken")]);
    assert!(
        (dt - 0.3).abs() < 1e-9,
        "Uber DamageTaken MORE -70 → 0.3, got {}",
        dt
    );
    // Uber's minimum level is 82 (character level 80 is raised to 82).
    assert_eq!(env.enemy.level, 82);
    // Uber elemental penetration = uberBossPen 40/5 = 8 — defence-side `Enemy<El>Pen`
    // (enemy db) only, the player's offensive penetration is unaffected.
    let pen = env.player.mod_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("ElementalPenetration")],
    );
    assert_eq!(
        pen, 0.0,
        "boss penetration must not leak into the player's offensive penetration"
    );
    let def_pen = env
        .enemy
        .mod_db
        .sum(ModType::Base, &cfg, &[ModName::from("EnemyColdPen")]);
    assert_eq!(
        def_pen, 8.0,
        "Uber defence-side EnemyColdPen +8 injected into enemy db"
    );
}

#[test]
fn setup_enemy_none_tier_has_no_resist_or_boss_debuff() {
    let player = Actor::new(60, ActorBaseStats::default());
    let mut env = Env::new(player);
    setup_enemy(&mut env, 0, EnemyTier::None);
    let cfg = CalcConfig::attack();
    let db = &env.enemy.mod_db;
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("FireResist")]),
        0.0
    );
    assert!(
        !db.flag(&cfg, ModName::from("Condition:Unique")),
        "a plain monster has no Unique condition"
    );
    assert!(!db.flag(&cfg, ModName::from("Condition:PinnacleBoss")));
    // A plain monster has no innate penetration → the player db should not receive ElementalPenetration.
    assert_eq!(
        env.player.mod_db.sum(
            ModType::Base,
            &cfg,
            &[ModName::from("ElementalPenetration")]
        ),
        0.0,
        "a plain monster doesn't inject penetration"
    );
}

// 8. Enemy contributions are traceable (EnemyConfig origin) + perform integration.

#[test]
fn enemy_mods_carry_enemy_config_origin() {
    let player = Actor::new(85, ActorBaseStats::default());
    let env = env_with_enemy(player, 0, EnemyTier::Pinnacle);

    // Every enemy modifier should carry an EnemyConfig origin, so TraceGraph can distinguish innate enemy stats from our own debuffs on them.
    let mut count = 0;
    for modifier in env.enemy.mod_db.iter_mods() {
        count += 1;
        let origin = modifier
            .origin
            .as_ref()
            .expect("enemy modifier carries origin");
        assert_eq!(
            origin.source_id.kind,
            SourceKind::EnemyConfig,
            "enemy mod {:?} should attribute to EnemyConfig",
            modifier.name
        );
    }
    assert!(count > 0, "Pinnacle enemy modDB should not be empty");
}

#[test]
fn perform_uses_enemy_damage_taken_in_effective_mode() {
    // End-to-end verification via perform that the enemy.mod_db damage-taken chain applies.
    let base = ActorBaseStats {
        hit_min: 100.0,
        hit_max: 200.0,
        action_rate: 1.0,
        ..ActorBaseStats::default()
    };
    let mut player = Actor::new(85, base);
    // Player has no extra damage mods; pure physical averages 150.
    let _ = &mut player;

    let mut env = Env::new(player).with_config(effective_attack());
    let mut enemy_db = ModDb::new();
    enemy_db.add_mod(
        Modifier::number("DamageTaken", ModType::Inc, 20.0).with_origin(ModifierSource::new(
            SourceId::new(SourceKind::EnemyConfig, "shock"),
        )),
    );
    env.enemy.mod_db = enemy_db;

    perform(&mut env).expect("perform succeeds");
    assert!(
        (env.player.output.dps - 180.0).abs() < 1e-6,
        "perform effective DamageTaken+20% → 180, got {}",
        env.player.output.dps
    );
}

// 9. Elemental/chaos penetration (elemental-penetration-missing): player penetration lowers the enemy's effective resist.

/// Input isolating the fire component: base_hit zeroed, player adds 100 flat fire.
fn fire_only_player_input() -> (ModDb, MinimalInput) {
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("FireDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FireDamageMax", ModType::Base, 100.0));
    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };
    (player, input)
}

#[test]
fn fire_penetration_raises_effective_dps_vs_high_resist_enemy() {
    // Acceptance for gap elemental-penetration-missing: FirePenetration 30 against 75%
    // enemy fire resist → effective resist 45%, fire-component DPS 100*(1-0.45)=55
    // (100*0.25=25 before penetration).
    let (mut player, input) = fire_only_player_input();
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 30.0));

    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 75.0));

    // No-penetration baseline: effective fire resist 75% → 100*0.25 = 25.
    let baseline = {
        let (player_no_pen, _) = fire_only_player_input();
        calculate_minimal_vs_enemy(&player_no_pen, &enemy, &effective_attack(), &input).dps
    };
    let with_pen = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;

    assert!(
        (baseline - 25.0).abs() < 1e-6,
        "no penetration, 75% resist -> 25, got {baseline}"
    );
    assert!(
        (with_pen - 55.0).abs() < 1e-6,
        "FirePen30 vs 75% resist -> effective 45% -> 100*0.55 = 55, got {with_pen}"
    );
    assert!(with_pen > baseline, "penetration raises effective DPS");
}

#[test]
fn elemental_penetration_shared_applies_to_all_elements() {
    // ElementalPenetration's shared group applies to fire/cold/lightning simultaneously.
    let (mut player, input) = fire_only_player_input();
    player.add_mod(Modifier::number(
        "ElementalPenetration",
        ModType::Base,
        25.0,
    ));

    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;
    // Effective fire resist 50-25 = 25% → 100*0.75 = 75.
    assert!(
        (dps - 75.0).abs() < 1e-6,
        "ElementalPen25 vs 50% resist -> effective 25% -> 75, got {dps}"
    );
}

#[test]
fn penetration_cannot_push_resist_below_zero() {
    // Penetration can't push resist below 0: FirePen 100 vs 30% resist → effective 0% (not -70%), 100*1.0 = 100.
    let (mut player, input) = fire_only_player_input();
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 100.0));

    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 30.0));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;
    assert!(
        (dps - 100.0).abs() < 1e-6,
        "penetration can't break 0 -> effective 0% -> 100, got {dps}"
    );
}

#[test]
fn penetration_wasted_against_negative_resist() {
    // Against negative resist (e.g. -50% after exposure) penetration is entirely wasted: effective resist stays -50%, 100*1.5 = 150 (unchanged by penetration).
    let (mut player, input) = fire_only_player_input();
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 40.0));

    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, -50.0));

    let with_pen = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;
    let no_pen = {
        let (player_no_pen, _) = fire_only_player_input();
        calculate_minimal_vs_enemy(&player_no_pen, &enemy, &effective_attack(), &input).dps
    };
    assert!(
        (with_pen - 150.0).abs() < 1e-6,
        "negative resist -50% -> 150, got {with_pen}"
    );
    assert!(
        (with_pen - no_pen).abs() < 1e-6,
        "penetration has no effect under negative resist: with penetration {with_pen} == without penetration {no_pen}"
    );
}

#[test]
fn chaos_penetration_uses_chaos_resist() {
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("ChaosDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("ChaosDamageMax", ModType::Base, 100.0));
    player.add_mod(Modifier::number("ChaosPenetration", ModType::Base, 20.0));
    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("ChaosResist", ModType::Base, 60.0));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;
    // Effective chaos resist 60-20 = 40% → 100*0.6 = 60.
    assert!(
        (dps - 60.0).abs() < 1e-6,
        "ChaosPen20 vs 60% resist -> effective 40% -> 60, got {dps}"
    );
}

#[test]
fn penetration_does_not_affect_panel_dps() {
    // Panel mode (mode_effective=false) ignores penetration/resist entirely, for exact backward compatibility.
    let (mut player, input) = fire_only_player_input();
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 30.0));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 75.0));

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input).dps;
    assert!(
        (panel - 100.0).abs() < 1e-6,
        "panel: 100 fire damage, neither resist nor penetration applied, got {panel}"
    );
}

#[test]
fn penetration_only_affects_its_element() {
    // FirePenetration doesn't affect the cold-damage component.
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("ColdDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("ColdDamageMax", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FirePenetration", ModType::Base, 50.0));
    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("ColdResist", ModType::Base, 40.0));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;
    // Cold resist 40% is not penetrated by FirePenetration → 100*0.6 = 60.
    assert!(
        (dps - 60.0).abs() < 1e-6,
        "FirePen doesn't affect cold damage: 40% cold resist -> 60, got {dps}"
    );
}

#[test]
fn penetration_attribution_player_and_enemy_resist_traceable() {
    // Penetration attributes to the player source; enemy resist attributes to EnemyConfig.
    let mut player = ModDb::new();
    player.add_mod(
        Modifier::number("FirePenetration", ModType::Base, 30.0).with_origin(ModifierSource::new(
            SourceId::new(SourceKind::PassiveNode, "pen.node"),
        )),
    );
    let mut enemy = ModDb::new();
    enemy.add_mod(
        Modifier::number("FireResist", ModType::Base, 75.0).with_origin(ModifierSource::new(
            SourceId::new(SourceKind::EnemyConfig, "boss_fire_resist"),
        )),
    );

    let cfg = effective_attack().with_damage_type(DamageType::Fire);
    let pen_contribs =
        player.contributions(ModType::Base, &cfg, &[ModName::from("FirePenetration")]);
    assert_eq!(
        pen_contribs[0].origin.as_ref().unwrap().source_id.kind,
        SourceKind::PassiveNode,
        "penetration attributes to the player source"
    );
    let resist_contribs = enemy.contributions(ModType::Base, &cfg, &[ModName::from("FireResist")]);
    assert_eq!(
        resist_contribs[0].origin.as_ref().unwrap().source_id.kind,
        SourceKind::EnemyConfig,
        "enemy resist attributes to EnemyConfig"
    );
}

// 10. Overwhelm (overwhelm-not-wired): a negative player EnemyPhysicalDamageReduction lowers the enemy's PDR.

#[test]
fn overwhelm_reduces_enemy_pdr_and_raises_physical_dps() {
    // Acceptance for gap overwhelm-not-wired: enemy PDR 20% (PhysicalDamageReduction
    // Base=20), player Overwhelm 20 (EnemyPhysicalDamageReduction Base=-20) → net PDR 0%
    // → full physical DPS.
    let mut player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        20.0,
    ));

    // Baseline (no Overwhelm): PDR 20% → 150*0.8 = 120.
    let baseline =
        calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (baseline.dps - 120.0).abs() < 1e-6,
        "enemy PDR20% -> 150*0.8 = 120, got {}",
        baseline.dps
    );

    player.add_mod(Modifier::number(
        "EnemyPhysicalDamageReduction",
        ModType::Base,
        -20.0,
    ));
    let with_overwhelm =
        calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert!(
        (with_overwhelm.dps - 150.0).abs() < 1e-6,
        "Overwhelm20 offsets PDR20% -> net 0% -> 150, got {}",
        with_overwhelm.dps
    );
    assert!(
        with_overwhelm.dps > baseline.dps,
        "Overwhelm raises effective physical DPS"
    );
}

#[test]
fn overwhelm_against_armour_reduction() {
    // Enemy armour=1500 against raw_hit=150 → 50% armour mitigation; Overwhelm 20 → net 30% → 150*0.7 = 105.
    let mut player = ModDb::new();
    player.add_mod(Modifier::number(
        "EnemyPhysicalDamageReduction",
        ModType::Base,
        -20.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("Armour", ModType::Base, 1500.0));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input()).dps;
    assert!(
        (dps - 105.0).abs() < 1e-6,
        "armour 50% - Overwhelm20 -> net 30% -> 150*0.7 = 105, got {dps}"
    );
}

#[test]
fn overwhelm_can_push_pdr_negative_down_to_neg_cap() {
    // vendor CalcOffence.lua:4095: the additive sum floors at −NegArmourDmgBonusCap(−100),
    // with **no** per-source 0 floor — enemy PDR 10%, Overwhelm 50 → net −40% → a ×1.4
    // damage bonus. (The wiki's "Overwhelm can't go below 0" claim disagrees with the
    // PoB2 implementation; parity is pinned to vendor — see agent-docs/damage-scaling.md
    // §Overwhelm note.)
    let mut player = ModDb::new();
    player.add_mod(Modifier::number(
        "EnemyPhysicalDamageReduction",
        ModType::Base,
        -50.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        10.0,
    ));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input()).dps;
    assert!(
        (dps - 210.0).abs() < 1e-6,
        "net −40% mitigation -> 150*1.4 = 210, got {dps}"
    );
}

#[test]
fn overwhelm_only_affects_physical() {
    // Overwhelm doesn't affect the fire-damage component.
    let (mut player, input) = fire_only_player_input();
    player.add_mod(Modifier::number(
        "EnemyPhysicalDamageReduction",
        ModType::Base,
        -30.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let dps = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input).dps;
    // Fire resist 50% is unaffected by Overwhelm → 100*0.5 = 50.
    assert!(
        (dps - 50.0).abs() < 1e-6,
        "Overwhelm doesn't affect fire damage: 50% fire resist -> 50, got {dps}"
    );
}

#[test]
fn overwhelm_does_not_affect_panel_dps() {
    // Panel mode ignores Overwhelm/PDR entirely, for exact backward compatibility.
    let mut player = ModDb::new();
    player.add_mod(Modifier::number(
        "EnemyPhysicalDamageReduction",
        ModType::Base,
        -20.0,
    ));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        20.0,
    ));

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &attack_input());
    assert_eq!(panel.dps, 150.0, "panel scope ignores Overwhelm/PDR");
}

#[test]
fn perform_panel_mode_ignores_enemy_damage_taken() {
    // Default panel mode (mode_effective=false): the enemy.mod_db damage-taken chain doesn't change DPS (backward compatibility).
    let base = ActorBaseStats {
        hit_min: 100.0,
        hit_max: 200.0,
        action_rate: 1.0,
        ..ActorBaseStats::default()
    };
    let player = Actor::new(85, base);
    let mut env = Env::new(player); // Default CalcConfig::attack(), mode_effective=false
    env.enemy
        .mod_db
        .add_mod(Modifier::number("DamageTaken", ModType::Inc, 20.0));

    perform(&mut env).expect("perform succeeds");
    assert_eq!(
        env.player.output.dps, 150.0,
        "panel scope ignores enemy DamageTaken"
    );
}

// 02-02: setup_enemy treats enemy.mod_db as a persistent, incremental db (PoB2
// CalcSetup.lua:682-691) — it must not wholesale-replace the actor or clear enemy
// mods injected earlier.

#[test]
fn setup_enemy_preserves_preexisting_enemy_mods() {
    let player = Actor::new(85, ActorBaseStats::default());
    let mut env = Env::new(player);

    // Inject a custom enemy mod before setup_enemy runs (simulating a config
    // enemyPhysicalReduction / user-defined enemy modifier injected ahead of tier setup).
    env.enemy.mod_db.add_mod(Modifier::number(
        "PhysicalDamageReduction",
        ModType::Base,
        12.0,
    ));

    setup_enemy(&mut env, 0, EnemyTier::Pinnacle);

    // The old implementation (env.enemy = Actor::new(...), a wholesale replace) would read 0 here; the new incremental-append implementation keeps it at 12.
    let cfg = CalcConfig::attack();
    let pdr = env.enemy.mod_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("PhysicalDamageReduction")],
    );
    assert_eq!(
        pdr, 12.0,
        "setup_enemy should not clear already-injected enemy mods"
    );

    // Tier mods are still injected normally (incremental assembly, both coexist).
    let fire = env
        .enemy
        .mod_db
        .sum(ModType::Base, &cfg, &[ModName::from("FireResist")]);
    assert_eq!(
        fire, 50.0,
        "Pinnacle tier FireResist is still injected normally"
    );
}

// 05-05: the traced DPS path threads enemy_db through and matches the non-traced
// panel semantics (hit × (1-enemy_block), per-type mitigation, resolve_crit_traced
// using enemy_db).

/// In effective mode, traced DPS matches non-traced `calculate_minimal_vs_enemy`
/// exactly — the two paths no longer diverge once the enemy damage-taken chain
/// applies (finding 05-05).
#[test]
fn traced_vs_enemy_dps_matches_panel_with_damage_taken() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("DamageTaken", ModType::Inc, 20.0));

    let input = attack_input();
    let cfg = effective_attack();

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &cfg, &input);
    let traced = calculate_minimal_traced_vs_enemy(&player, &enemy, &cfg, &input);

    assert!(
        (traced.output.dps - panel.dps).abs() < 1e-6,
        "traced DPS {} should equal panel DPS {}",
        traced.output.dps,
        panel.dps
    );
    // Damage-taken chain +20% applies: 150 * 1.2 = 180.
    assert!((traced.output.dps - 180.0).abs() < 1e-6);
}

/// In effective mode, enemy block is deducted from the traced hit chance the same way as non-traced.
#[test]
fn traced_vs_enemy_dps_matches_panel_with_block() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("BlockChance", ModType::Base, 40.0));

    let input = attack_input();
    let cfg = effective_attack();

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &cfg, &input);
    let traced = calculate_minimal_traced_vs_enemy(&player, &enemy, &cfg, &input);

    assert!(
        (traced.output.dps - panel.dps).abs() < 1e-6,
        "traced DPS {} should equal panel DPS {} (with block)",
        traced.output.dps,
        panel.dps
    );
    // 40% block: 150 * (1 - 0.4) = 90.
    assert!((traced.output.dps - 90.0).abs() < 1e-6);
}

/// In effective mode, enemy resist mitigation (fire) is deducted from traced fire damage the same way as non-traced.
#[test]
fn traced_vs_enemy_dps_matches_panel_with_resist() {
    let mut player = ModDb::new();
    // A 100 fire-damage component (same setup as enemy_fire_resist_reduces_fire_dps_only_in_effective).
    player.add_mod(Modifier::number("FireDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FireDamageMax", ModType::Base, 100.0));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    // base_hit 0: isolates the fire component, guaranteed hit.
    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };
    let cfg = effective_attack();

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &cfg, &input);
    let traced = calculate_minimal_traced_vs_enemy(&player, &enemy, &cfg, &input);

    assert!(
        (traced.output.dps - panel.dps).abs() < 1e-6,
        "traced DPS {} should equal panel DPS {} (with fire resist)",
        traced.output.dps,
        panel.dps
    );
    // 50% fire resist: 100 * 0.5 = 50.
    assert!((traced.output.dps - 50.0).abs() < 1e-6);
}

/// `calculate_minimal_traced` (the old 4-arg entry point) behaves like calling with
/// an empty enemy_db — panel/non-effective output matches history (the 5 legacy
/// traced tests keep their old semantics).
#[test]
fn traced_empty_enemy_equals_legacy_entry() {
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0));
    let input = attack_input();

    // Even with an enemy mod supplied, mode_effective=false (panel mode) keeps both paths' DPS equal and introduces no enemy interaction.
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("DamageTaken", ModType::Inc, 99.0));
    let cfg = CalcConfig::attack(); // 非有效

    let legacy = calculate_minimal_traced(&player, &cfg, &input);
    let vs_empty = calculate_minimal_traced_vs_enemy(&player, &ModDb::new(), &cfg, &input);
    let vs_enemy_panel = calculate_minimal_traced_vs_enemy(&player, &enemy, &cfg, &input);

    assert!((legacy.output.dps - vs_empty.output.dps).abs() < 1e-9);
    assert!(
        (legacy.output.dps - vs_enemy_panel.output.dps).abs() < 1e-9,
        "panel scope: enemy DamageTaken should not affect traced DPS"
    );
}
