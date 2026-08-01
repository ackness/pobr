//! Integration tests for the non-damaging-ailment application loop:
//! - end-to-end: enemy `ShockVal` 20 → effective DPS ×1.20 (via the DamageTaken chain
//!   consumed by offence's `mode_effective`);
//! - override and Val take the max; Maximum clamp + precision truncation (prec=0 → integer
//!   floor), value by value;
//! - a second application is a no-op once `Condition:Already<X>` is set;
//! - empty-spin compatibility (no source mods → env values unchanged);
//! - Chill ActionSpeed / Bonechill branch / magnitude scaling / incremental Multiplier updates.
//!
//! Formula source: vendor CalcPerform.lua:3076-3180 (line numbers read against 0.18.0).

use pobr_core::calc::ailment_apply::apply_nondamaging_ailments;
use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

/// Standard attack Env: 100-200 physical, 1/s, full hit chance (enemy has no evasion),
/// effective-DPS semantics. Shaped like `enemy_mod_db.rs`'s `attack_input`: baseline DPS = 150.
fn effective_env() -> Env {
    let base = ActorBaseStats {
        life: 100.0,
        hit_min: 100.0,
        hit_max: 200.0,
        action_rate: 1.0,
        ..Default::default()
    };
    let mut env = Env::new(Actor::new(80, base));
    env.cfg = CalcConfig::attack()
        .with_damage_type(DamageType::Physical)
        .with_mode_effective(true);
    env
}

/// Inject an enemy condition flag (the minimal shape equivalent to config `conditionEnemyShocked`).
fn enemy_condition(env: &mut Env, condition: &str) {
    env.enemy
        .mod_db
        .add_mod(Modifier::flag(format!("Condition:{condition}")).with_source("Config"));
}

/// The INC aggregation of a given name in the enemy db, under the given cfg.
fn enemy_inc(env: &Env, name: &str) -> f64 {
    env.enemy
        .mod_db
        .sum(ModType::Inc, &env.cfg, &[ModName::from(name)])
}

/// The BASE aggregation of a given name in the enemy db, under the given cfg.
fn enemy_base(env: &Env, name: &str) -> f64 {
    env.enemy
        .mod_db
        .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
}

// 1. End-to-end: enemy ShockVal 20 -> effective DPS x1.20

#[test]
fn shock_val_20_raises_effective_dps_by_20_percent() {
    // Baseline: no shock.
    let mut base_env = effective_env();
    perform(&mut base_env).expect("baseline perform");
    let base_dps = base_env.player.output.dps;
    assert!(
        (base_dps - 150.0).abs() < 1e-9,
        "基线 150 平均 × 1/s × 100% 命中，got {base_dps}"
    );

    // Shocked: the config shape = enemy `Condition:Shocked` flag + enemy `ShockVal` BASE 20.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0).with_source("Shock"));
    perform(&mut env).expect("shocked perform");

    assert!(
        (env.player.output.dps - base_dps * 1.20).abs() < 1e-9,
        "ShockVal 20 → DamageTaken INC 20 → DPS ×1.20：expect {}, got {}",
        base_dps * 1.20,
        env.player.output.dps
    );
    // Panel semantics are unaffected (the DamageTaken consumer chain is gated by mode_effective).
    let mut panel = effective_env();
    panel.cfg.mode_effective = false;
    enemy_condition(&mut panel, "Shocked");
    panel
        .enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0).with_source("Shock"));
    perform(&mut panel).expect("panel perform");
    assert!(
        (panel.player.output.dps - base_dps).abs() < 1e-9,
        "面板口径不吃敌人 DamageTaken：expect {base_dps}, got {}",
        panel.player.output.dps
    );
}

// 2. override and Val take the max (CalcPerform.lua:3164's m_max(override, SumVal))

#[test]
fn override_and_val_take_max() {
    // override(30) > Val(20) -> 30.
    let mut env = effective_env();
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockOverride", ModType::Base, 30.0).with_source("Skitterbots"));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 30.0).abs() < 1e-9,
        "override 30 胜出"
    );
    // An Override source -> sets Condition:Shocked (vendor :3136-3138) + bridges into cfg.
    assert!(
        env.enemy
            .mod_db
            .flag(&env.cfg, ModName::from("Condition:Shocked")),
        "Override 来源置 enemy Condition:Shocked"
    );
    assert!(env.cfg.condition("Shocked"), "条件桥接回填 cfg");

    // Val(40) > override(30) -> 40.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 40.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockOverride", ModType::Base, 30.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 40.0).abs() < 1e-9,
        "Val 40 胜出"
    );
}

// 3. Maximum clamp + precision truncation (prec=0 -> integer floor), value by value

#[test]
fn maximum_clamp_and_precision_floor() {
    // Shock cap = non_damaging_ailments.json Shock.max = 100: 150 -> 100.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 150.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 100.0).abs() < 1e-9,
        "Shock clamp 到数据 max=100"
    );

    // Chill cap = 50 (cfg.constants.game().chill_max_effect): 80 -> 50.
    let mut env = effective_env();
    enemy_condition(&mut env, "Chilled");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ChillVal", ModType::Base, 80.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "ActionSpeed") - (-50.0)).abs() < 1e-9,
        "Chill clamp 到数据 max=50，ActionSpeed INC -50"
    );

    // Precision truncation: prec=0 -> floor (33.7 -> 33).
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 33.7));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 33.0).abs() < 1e-9,
        "prec=0 整数 floor：33.7 → 33"
    );

    // A `<X>Max` BASE mod raises the cap: max = 100 + 20 -> ShockVal 110 -> 110.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 110.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMax", ModType::Base, 20.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 110.0).abs() < 1e-9,
        "ShockMax BASE +20 抬高上限到 120"
    );

    // A `<X>Max` Override directly overrides the cap: override 60 -> ShockVal 110 -> 60.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 110.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMax", ModType::Override, 60.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 60.0).abs() < 1e-9,
        "ShockMax Override=60 优先于数据 max"
    );
}

// 4. A second application is a no-op once Already is set (guards against double application by minions, :3130/:3168)

#[test]
fn second_application_is_noop_after_already_flag() {
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));

    apply_nondamaging_ailments(&mut env);
    let taken_after_first = enemy_inc(&env, "DamageTaken");
    let mods_after_first = env.enemy.mod_db.iter_mods().count();
    assert!(
        env.enemy
            .mod_db
            .flag(&env.cfg, ModName::from("Condition:AlreadyShocked")),
        "首次施加置 Condition:AlreadyShocked"
    );

    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - taken_after_first).abs() < 1e-12,
        "二次施加不叠加 DamageTaken"
    );
    assert_eq!(
        env.enemy.mod_db.iter_mods().count(),
        mods_after_first,
        "二次施加不写入任何新 mod"
    );
}

// 5. Empty-spin compatibility: no source mods -> env values unchanged

#[test]
fn empty_spin_leaves_env_unchanged() {
    let mut env = effective_env();
    let player_mods = env.player.mod_db.iter_mods().count();
    let enemy_mods = env.enemy.mod_db.iter_mods().count();
    let conditions = env.cfg.conditions.clone();

    apply_nondamaging_ailments(&mut env);

    assert_eq!(env.player.mod_db.iter_mods().count(), player_mods);
    assert_eq!(env.enemy.mod_db.iter_mods().count(), enemy_mods);
    assert_eq!(env.cfg.conditions, conditions, "cfg.conditions 不被触碰");
}

// 6. Magnitude scaling: Base/Minimum multiply by Enemy<X>Magnitude/AilmentMagnitude, Override doesn't

#[test]
fn magnitude_scales_base_and_minimum_but_not_override() {
    // ShockBase 20 x (1 + 50%) = 30.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockBase", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 50.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 30.0).abs() < 1e-9,
        "ShockBase 吃 EnemyShockMagnitude INC：20×1.5=30"
    );

    // ShockOverride 20 doesn't multiply by magnitude -> 20.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockOverride", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 50.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 20.0).abs() < 1e-9,
        "Override 来源不乘 magnitude"
    );

    // The enemy side's SelfShockMagnitude also participates (:3146): 20 x 1.5(skill) x 1.1(enemy) = 33.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockBase", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 50.0));
    env.enemy
        .mod_db
        .add_mod(Modifier::number("SelfShockMagnitude", ModType::Inc, 10.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 33.0).abs() < 1e-9,
        "skill×enemy 双侧 magnitude：20×1.5×1.1=33"
    );

    // Minimum accumulates (:3148-3150): ShockMinimum 10+15 -> 25, higher than either single value -> 25.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMinimum", ModType::Base, 10.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMinimum", ModType::Base, 15.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 25.0).abs() < 1e-9,
        "Minimum 累加 25 作为下界"
    );
}

// 7. Chill: negative ActionSpeed + the Bonechill branch

#[test]
fn chill_writes_negative_action_speed_and_bonechill() {
    // No Bonechill: only ActionSpeed INC -num.
    let mut env = effective_env();
    enemy_condition(&mut env, "Chilled");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ChillVal", ModType::Base, 25.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "ActionSpeed") - (-25.0)).abs() < 1e-9,
        "Chill → ActionSpeed INC -25"
    );
    assert!(
        enemy_inc(&env, "ColdDamageTaken").abs() < 1e-12,
        "无 HasBonechill 不写 ColdDamageTaken"
    );

    // HasBonechill + enemy ChillVal > 0 -> ColdDamageTaken INC num (:3092-3094).
    let mut env = effective_env();
    enemy_condition(&mut env, "Chilled");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ChillVal", ModType::Base, 25.0));
    env.player
        .mod_db
        .add_mod(Modifier::flag("HasBonechill").with_source("Bonechill Support"));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "ColdDamageTaken") - 25.0).abs() < 1e-9,
        "Bonechill → ColdDamageTaken INC 25"
    );
}

// 8. Incremental updates to Multiplier:ChillEffect/ShockEffect (:3173-3180)

#[test]
fn effect_multiplier_updates_incrementally() {
    // No existing multiplier: top up to Current.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_base(&env, "Multiplier:ShockEffect") - 20.0).abs() < 1e-9,
        "Multiplier:ShockEffect 补到 20"
    );

    // Existing 5 -> +15 increment, total 20.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    env.enemy.mod_db.add_mod(Modifier::number(
        "Multiplier:ShockEffect",
        ModType::Base,
        5.0,
    ));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_base(&env, "Multiplier:ShockEffect") - 20.0).abs() < 1e-9,
        "既有 5 + 增量 15 = 20"
    );

    // Existing 30 >= Current 20 -> no update (stays at 30).
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    env.enemy.mod_db.add_mod(Modifier::number(
        "Multiplier:ShockEffect",
        ModType::Base,
        30.0,
    ));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_base(&env, "Multiplier:ShockEffect") - 30.0).abs() < 1e-9,
        "既有 30 ≥ 20 不更新"
    );
}
