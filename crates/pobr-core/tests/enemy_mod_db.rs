//! Step-2：enemy.mod_db 接线 + mode_effective + 受伤链 + 曝光取最强 + CannotBeEvaded
//! + 敌人格挡 + 敌人贡献可 trace 的集成测试。
//!
//! 来源：agent-docs/accuracy-and-enemy.md §二,§五,§六,§七；agent-docs/debuffs.md §曝光；
//!       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2,§5。

use pobr_core::calc::setup_env::{env_with_enemy, reduce_enemy_exposure, setup_enemy};
use pobr_core::calc::{
    Actor, ActorBaseStats, Env, MinimalInput, calculate_minimal, calculate_minimal_vs_enemy,
    perform,
};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

/// 标准攻击输入：100~200 物理，1/s，满命中（敌人无闪避）。
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

// ---------------------------------------------------------------------------
// 1. 敌人 DamageTaken 提升有效 DPS（enemy-damage-taken-chain）
// ---------------------------------------------------------------------------

#[test]
fn enemy_damage_taken_inc_raises_effective_dps() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("DamageTaken", ModType::Inc, 20.0));

    let input = attack_input();
    // 面板口径（mode_effective=false）：DamageTaken 不生效。
    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    // 有效口径：受伤链 +20% 生效。
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);

    assert_eq!(
        panel.dps, 150.0,
        "无 effective：物理 150 平均 × 1/s × 100%命中"
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
    // FireDamageTaken 只作用火伤；纯物理击中不受影响。
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireDamageTaken", ModType::Inc, 100.0));

    let out = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());
    assert_eq!(out.dps, 150.0, "纯物理不受 FireDamageTaken 影响");
}

// ---------------------------------------------------------------------------
// 2. 敌人抗性/护甲减伤（仅 effective）
// ---------------------------------------------------------------------------

#[test]
fn enemy_fire_resist_reduces_fire_dps_only_in_effective() {
    let mut player = ModDb::new();
    // 把 100 火伤加为分类型 flat added。
    player.add_mod(Modifier::number("FireDamageMin", ModType::Base, 100.0));
    player.add_mod(Modifier::number("FireDamageMax", ModType::Base, 100.0));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    // base_hit 0：仅火伤分量。
    let input = MinimalInput {
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        ..attack_input()
    };

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);

    assert_eq!(panel.dps, 100.0, "面板：100 火伤不减抗");
    assert!(
        (effective.dps - 50.0).abs() < 1e-6,
        "effective 50% 火抗 → 100*0.5 = 50, got {}",
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
        "armour 减伤 50% → 150*0.5 = 75, got {}",
        effective.dps
    );
}

// ---------------------------------------------------------------------------
// 3. 曝光取最强（exposure-min-of via ModDb::max_of + reduce_enemy_exposure）
// ---------------------------------------------------------------------------

#[test]
fn exposure_takes_strongest_single_source() {
    // 两条 FireExposure BASE 20 与 30 → 最终 FireResist BASE -30（取最大，非求和 50）。
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireExposure", ModType::Base, 20.0));
    enemy.add_mod(Modifier::number("FireExposure", ModType::Base, 30.0));

    reduce_enemy_exposure(&mut enemy, &CalcConfig::attack());

    let fire_resist = enemy.sum(
        ModType::Base,
        &CalcConfig::attack(),
        &[ModName::from("FireResist")],
    );
    assert!(
        (fire_resist + 30.0).abs() < 1e-6,
        "曝光取最强 30 → FireResist BASE -30, got {}",
        fire_resist
    );
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

// ---------------------------------------------------------------------------
// 4. CannotBeEvaded / 敌方 CannotEvade（cannot-be-evaded-flag）
// ---------------------------------------------------------------------------

#[test]
fn cannot_be_evaded_flag_forces_full_hit() {
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("CannotBeEvaded"));
    let enemy = ModDb::new();

    // 高闪避敌人 + 0 精准 → 正常会命中下限；CannotBeEvaded 应置满命中。
    let input = MinimalInput {
        enemy_evasion: 10_000.0,
        base_accuracy: 100.0,
        ..attack_input()
    };
    let out = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    assert_eq!(out.hit_chance, 1.0, "CannotBeEvaded → 满命中");
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
    // 面板口径：CannotEvade 不生效（仍走精准公式，命中率 < 1）。
    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &input);
    assert!(panel.hit_chance < 1.0, "面板下敌方 CannotEvade 不生效");
    // 有效口径：CannotEvade → 满命中。
    let effective = calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &input);
    assert_eq!(
        effective.hit_chance, 1.0,
        "effective 下敌方 CannotEvade → 满命中"
    );
}

// ---------------------------------------------------------------------------
// 5. 敌人格挡（enemy-block-chance-hit-chain）
// ---------------------------------------------------------------------------

#[test]
fn enemy_block_chance_reduces_hit_in_effective() {
    let player = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("BlockChance", ModType::Base, 25.0));

    let panel = calculate_minimal_vs_enemy(&player, &enemy, &CalcConfig::attack(), &attack_input());
    let effective =
        calculate_minimal_vs_enemy(&player, &enemy, &effective_attack(), &attack_input());

    assert_eq!(panel.hit_chance, 1.0, "面板不扣格挡");
    assert!(
        (effective.hit_chance - 0.75).abs() < 1e-6,
        "25% 格挡 → 命中 ×0.75, got {}",
        effective.hit_chance
    );
    assert!(
        (effective.dps - 112.5).abs() < 1e-6,
        "150 * 0.75 = 112.5, got {}",
        effective.dps
    );
}

// ---------------------------------------------------------------------------
// 6. mode_effective 面板 vs 有效差异（mode-effective-missing）
// ---------------------------------------------------------------------------

#[test]
fn panel_dps_not_lower_than_effective_dps() {
    // Pinnacle Boss 默认场景：有效 DPS 应 <= 面板 DPS（面板不扣敌人减伤）。
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
        "面板 DPS({}) 应 >= 有效 DPS({})",
        panel.dps,
        effective.dps
    );
}

#[test]
fn legacy_three_arg_entry_equals_empty_enemy() {
    // calculate_minimal（旧三参）应等价于空敌人 modDB（向后兼容）。
    let mut player = ModDb::new();
    player.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );
    let input = attack_input();
    let legacy = calculate_minimal(&player, &CalcConfig::attack(), &input);
    let via_empty = calculate_minimal_vs_enemy(&player, &ModDb::new(), &effective_attack(), &input);
    assert_eq!(legacy.dps, via_empty.dps, "空敌人 + effective 与旧入口一致");
}

// ---------------------------------------------------------------------------
// 7. setup_enemy 注入 + Pinnacle 默认档位（setup-env-missing）
// ---------------------------------------------------------------------------

#[test]
fn setup_enemy_injects_pinnacle_defaults() {
    let player = Actor::new(85, ActorBaseStats::default());
    let mut env = Env::new(player);
    setup_enemy(&mut env, 0, EnemyTier::Pinnacle); // level 0 → 跟随角色等级 min(85,85)=85

    let cfg = CalcConfig::attack();
    let db = &env.enemy.mod_db;

    // 元素抗性 +50%（Pinnacle）。
    let fire = db.sum(ModType::Base, &cfg, &[ModName::from("FireResist")]);
    assert_eq!(fire, 50.0, "Pinnacle 火抗 +50");
    // 精准 = monsterAccuracyTable[85] = 2357。
    let acc = db.sum(ModType::Base, &cfg, &[ModName::from("Accuracy")]);
    assert_eq!(acc, monster_accuracy(85) as f64);
    // 通用 Boss debuff 抗性。
    let curse = db.more(&cfg, &[ModName::from("CurseEffectOnSelf")]);
    assert!(
        (curse - 0.5).abs() < 1e-9,
        "CurseEffectOnSelf MORE -50 → 0.5, got {}",
        curse
    );
    // Condition:PinnacleBoss 已设置。
    assert!(
        db.flag(&cfg, ModName::from("Condition:PinnacleBoss")),
        "Pinnacle 设条件态"
    );
    // 等级被 Pinnacle 抬到 >=82（这里 85）。
    assert_eq!(env.enemy.level, 85);
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
    // Uber 最低等级 82（角色 80 被抬到 82）。
    assert_eq!(env.enemy.level, 82);
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
        "普通怪无 Unique 条件"
    );
    assert!(!db.flag(&cfg, ModName::from("Condition:PinnacleBoss")));
}

// ---------------------------------------------------------------------------
// 8. 敌人贡献可 trace（EnemyConfig 归因）+ perform 集成
// ---------------------------------------------------------------------------

#[test]
fn enemy_mods_carry_enemy_config_origin() {
    let player = Actor::new(85, ActorBaseStats::default());
    let env = env_with_enemy(player, 0, EnemyTier::Pinnacle);

    // 所有敌人 modifier 都应带 EnemyConfig 归因，便于 TraceGraph 区分敌人天生属性 vs 我方 debuff。
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
            "enemy mod {:?} 归因应为 EnemyConfig",
            modifier.name
        );
    }
    assert!(count > 0, "Pinnacle enemy modDB 非空");
}

#[test]
fn perform_uses_enemy_damage_taken_in_effective_mode() {
    // 通过 perform 端到端验证 enemy.mod_db 受伤链生效。
    let base = ActorBaseStats {
        hit_min: 100.0,
        hit_max: 200.0,
        action_rate: 1.0,
        ..ActorBaseStats::default()
    };
    let mut player = Actor::new(85, base);
    // 玩家无额外 damage mod；纯物理 150 平均。
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

#[test]
fn perform_panel_mode_ignores_enemy_damage_taken() {
    // 默认面板口径（mode_effective=false）：enemy.mod_db 受伤链不改变 DPS（向后兼容）。
    let base = ActorBaseStats {
        hit_min: 100.0,
        hit_max: 200.0,
        action_rate: 1.0,
        ..ActorBaseStats::default()
    };
    let player = Actor::new(85, base);
    let mut env = Env::new(player); // 默认 CalcConfig::attack(), mode_effective=false
    env.enemy
        .mod_db
        .add_mod(Modifier::number("DamageTaken", ModType::Inc, 20.0));

    perform(&mut env).expect("perform succeeds");
    assert_eq!(
        env.player.output.dps, 150.0,
        "面板口径忽略 enemy DamageTaken"
    );
}
