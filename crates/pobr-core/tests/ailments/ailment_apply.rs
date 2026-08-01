//! 非伤害异常施加闭环集成测试：
//! - 端到端：enemy `ShockVal` 20 → 有效 DPS ×1.20（DamageTaken 链经 offence
//!   `mode_effective` 消费）；
//! - override 与 Val 取 max；Maximum clamp + 精度截断（prec=0 → 整数 floor）逐值；
//! - `Condition:Already<X>` 置位后二次施加无效；
//! - 空转兼容（无来源词条 → env 逐值不变）；
//! - Chill ActionSpeed / Bonechill 分支 / magnitude 缩放 / Multiplier 增量更新。
//!
//! 公式出处：vendor CalcPerform.lua:3076-3180（行号 0.18.0 实读）。

use pobr_core::calc::ailment_apply::apply_nondamaging_ailments;
use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

/// 标准攻击 Env：100~200 物理、1/s、满命中（敌人无闪避），有效 DPS 口径。
/// 与 `enemy_mod_db.rs` 的 `attack_input` 同形：基线 DPS = 150。
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

/// 注入 enemy 条件 flag（config `conditionEnemyShocked` 等价产物的最小形）。
fn enemy_condition(env: &mut Env, condition: &str) {
    env.enemy
        .mod_db
        .add_mod(Modifier::flag(format!("Condition:{condition}")).with_source("Config"));
}

/// enemy db 某名字在给定 cfg 下的 INC 聚合。
fn enemy_inc(env: &Env, name: &str) -> f64 {
    env.enemy
        .mod_db
        .sum(ModType::Inc, &env.cfg, &[ModName::from(name)])
}

/// enemy db 某名字在给定 cfg 下的 BASE 聚合。
fn enemy_base(env: &Env, name: &str) -> f64 {
    env.enemy
        .mod_db
        .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
}

// 1. 端到端：enemy ShockVal 20 → 有效 DPS ×1.20

#[test]
fn shock_val_20_raises_effective_dps_by_20_percent() {
    // 基线：无感电。
    let mut base_env = effective_env();
    perform(&mut base_env).expect("baseline perform");
    let base_dps = base_env.player.output.dps;
    assert!(
        (base_dps - 150.0).abs() < 1e-9,
        "基线 150 平均 × 1/s × 100% 命中，got {base_dps}"
    );

    // 感电：config 形状 = enemy `Condition:Shocked` flag + enemy `ShockVal` BASE 20。
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
    // 面板口径不受影响（DamageTaken 消费链 mode_effective 门控）。
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

// 2. override 与 Val 取 max（CalcPerform.lua:3164 的 m_max(override, ΣVal)）

#[test]
fn override_and_val_take_max() {
    // override(30) > Val(20) → 30。
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
    // Override 来源 → 置 Condition:Shocked（vendor :3136-3138）+ cfg 桥接。
    assert!(
        env.enemy
            .mod_db
            .flag(&env.cfg, ModName::from("Condition:Shocked")),
        "Override 来源置 enemy Condition:Shocked"
    );
    assert!(env.cfg.condition("Shocked"), "条件桥接回填 cfg");

    // Val(40) > override(30) → 40。
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

// 3. Maximum clamp + 精度截断（prec=0 → 整数 floor）逐值

#[test]
fn maximum_clamp_and_precision_floor() {
    // Shock 上限 = non_damaging_ailments.json Shock.max = 100：150 → 100。
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

    // Chill 上限 = 50（cfg.constants.game().chill_max_effect）：80 → 50。
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

    // 精度截断：prec=0 → floor（33.7 → 33）。
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

    // `<X>Max` BASE 词条抬高上限：max = 100 + 20 → ShockVal 110 → 110。
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

    // `<X>Max` Override 直接覆盖上限：override 60 → ShockVal 110 → 60。
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

// 4. Already 置位后二次施加无效（防 minion 双重施加，:3130/:3168）

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

// 5. 空转兼容：无来源词条 → env 逐值不变

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

// 6. magnitude 缩放：Base/Minimum 乘 Enemy<X>Magnitude/AilmentMagnitude，Override 不乘

#[test]
fn magnitude_scales_base_and_minimum_but_not_override() {
    // ShockBase 20 × (1 + 50%) = 30。
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

    // ShockOverride 20 不乘 magnitude → 20。
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

    // 敌侧 SelfShockMagnitude 同样参与（:3146）：20 × 1.5(skill) × 1.1(enemy) = 33。
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

    // Minimum 累加（:3148-3150）：ShockMinimum 10+15 → 25 高于各单项 → 25。
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

// 7. Chill：ActionSpeed 负向 + Bonechill 分支

#[test]
fn chill_writes_negative_action_speed_and_bonechill() {
    // 无 Bonechill：仅 ActionSpeed INC -num。
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

    // HasBonechill + enemy ChillVal > 0 → ColdDamageTaken INC num（:3092-3094）。
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

// 8. Multiplier:ChillEffect/ShockEffect 增量更新（:3173-3180）

#[test]
fn effect_multiplier_updates_incrementally() {
    // 无既有 multiplier：补到 Current。
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

    // 既有 5 → 增量 15，总和 20。
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

    // 既有 30 ≥ Current 20 → 不更新（保持 30）。
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
