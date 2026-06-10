//! M2 Track E 集成测试：Evade 四分型 + Stun 体系（13-G9 / 13-G12）。
//!
//! 期望值全部按 PoB2 公式手算，逐用例注明 `vendor/PathOfBuilding-PoE2/src/Modules/
//! CalcDefence.lua` 行号（蓝图 m2-defence §4.1-6 公式单测惯例）。

use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::defence::{EvadeSuite, calc_evade_suite};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const EPS: f64 = 1e-9;

fn db_with(mods: Vec<Modifier>) -> ModDb {
    let mut db = ModDb::new();
    db.add_list(mods);
    db
}

/// 默认四分型入口：无敌方 HitChance 乘区（=1.0）、无 CannotBeEvaded。
fn suite(db: &ModDb, evasion: f64, accuracy: f64) -> EvadeSuite {
    calc_evade_suite(db, &CalcConfig::default(), evasion, accuracy, 1.0, false)
}

// ─────────────────────────────────────────────────────────────────
// Evade 四分型（CalcDefence.lua:1394-1456）
// ─────────────────────────────────────────────────────────────────

/// 基线：Evasion=4000, enemyAccuracy=1000。
/// monsterHitChance（:40-46）= round((1 − 0.95×4000/(4000+4000))×100) = round(52.5) = 53。
/// EvadeChance（:1437）= 100 − (53 − 0)×1 = 47；四分型同值 → 不 split，综合 = melee = 47。
#[test]
fn evade_baseline_uniform_47() {
    let s = suite(&ModDb::new(), 4000.0, 1000.0);
    assert!(
        (s.evade_chance - 47.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 47.0).abs() < EPS);
    assert!((s.projectile - 47.0).abs() < EPS);
    assert!((s.spell - 47.0).abs() < EPS);
    assert!((s.spell_projectile - 47.0).abs() < EPS);
}

/// ΣBASE EvadeChance 进括号内（:1437）：+10 BASE → 100 − (53 − 10) = 57。
#[test]
fn evade_base_evade_chance_additive_inside() {
    let db = db_with(vec![Modifier::number("EvadeChance", ModType::Base, 10.0)]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!(
        (s.evade_chance - 57.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
}

/// 四分型独立 inc 乘区（:1438）：`MeleeEvadeChance` INC 20% 只放大 melee：
/// melee = 47 × 1.2 = 56.4（min 95 / max 0 截断不触发）；其余三型仍 47。
/// melee 与三型全部不同 → split（:1443-1445），综合保留独立公式值 47。
#[test]
fn evade_melee_inc_splits_and_scales_only_melee() {
    let db = db_with(vec![Modifier::number(
        "MeleeEvadeChance",
        ModType::Inc,
        20.0,
    )]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!((s.melee - 56.4).abs() < EPS, "got {}", s.melee);
    assert!((s.projectile - 47.0).abs() < EPS);
    assert!((s.spell - 47.0).abs() < EPS);
    assert!((s.spell_projectile - 47.0).abs() < EPS);
    // split：综合 = 独立公式值（非 melee）。
    assert!(
        (s.evade_chance - 47.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
}

/// `SpellProjectileEvadeChance` 乘区名集含 ProjectileEvadeChance（:1441）：
/// `ProjectileEvadeChance` INC 20% 同时放大 projectile 与 spell_projectile。
#[test]
fn evade_projectile_inc_applies_to_spell_projectile_too() {
    let db = db_with(vec![Modifier::number(
        "ProjectileEvadeChance",
        ModType::Inc,
        20.0,
    )]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!((s.projectile - 56.4).abs() < EPS);
    assert!((s.spell_projectile - 56.4).abs() < EPS);
    assert!((s.spell - 47.0).abs() < EPS);
    // melee(47) == spell(47) → 不 split（:1443 条件要求全不同）→ 综合 = melee。
    assert!((s.evade_chance - 47.0).abs() < EPS);
}

/// `MeleeEvasion` 改变四分型有效闪避值（:1394-1397）：
/// INC 100% → MeleeEvasion = 8000；mhc = round((1 − 0.95×8000/(8000+4000))×100)
/// = round(36.666…) = 37 → melee = 100 − 37 = 63；其余 47。
#[test]
fn evade_melee_evasion_rating_scales_hit_chance_input() {
    let db = db_with(vec![Modifier::number("MeleeEvasion", ModType::Inc, 100.0)]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!((s.melee - 63.0).abs() < EPS, "got {}", s.melee);
    assert!((s.projectile - 47.0).abs() < EPS);
}

/// cap 95（:1436/:1449，game_constants `evade_chance_cap`）：
/// Evasion=100000, acc=1000 → mhc = round((1 − 0.95×100000/104000)×100) = round(8.65…) = 9；
/// +10 BASE → 100 − (9 − 10) = 101 → min(95)。四分型 min(evadeMax, …) 同。
#[test]
fn evade_capped_at_95() {
    let db = db_with(vec![Modifier::number("EvadeChance", ModType::Base, 10.0)]);
    let s = suite(&db, 100000.0, 1000.0);
    assert!(
        (s.evade_chance - 95.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 95.0).abs() < EPS);
}

/// `EvadeChanceMax` Override 收紧上限（:1436；W0.1 vendor MAX → Override）。
#[test]
fn evade_max_override_lowers_cap() {
    let db = db_with(vec![Modifier::number(
        "EvadeChanceMax",
        ModType::Override,
        30.0,
    )]);
    let s = suite(&db, 4000.0, 1000.0);
    // 未截断前 47 → Override 30。
    assert!(
        (s.evade_chance - 30.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 30.0).abs() < EPS);
}

/// `CannotEvade` → 全 0（:1421-1426）。
#[test]
fn evade_cannot_evade_zeroes_all() {
    let db = db_with(vec![Modifier::flag("CannotEvade")]);
    let s = suite(&db, 4000.0, 1000.0);
    assert_eq!(s, EvadeSuite::default());
}

/// 敌方 `CannotBeEvaded` flag（:1421）→ 全 0。
#[test]
fn evade_enemy_cannot_be_evaded_zeroes_all() {
    let s = calc_evade_suite(
        &ModDb::new(),
        &CalcConfig::default(),
        4000.0,
        1000.0,
        1.0,
        true,
    );
    assert_eq!(s, EvadeSuite::default());
}

/// `AlwaysEvade`（"Attacks cannot Hit you"）→ 全 100（:1427-1433）。
#[test]
fn evade_always_evade_all_100() {
    let db = db_with(vec![Modifier::flag("AlwaysEvade")]);
    let s = suite(&db, 0.0, 1000.0);
    assert!((s.evade_chance - 100.0).abs() < EPS);
    assert!((s.melee - 100.0).abs() < EPS);
    assert!((s.spell_projectile - 100.0).abs() < EPS);
}

/// `UnluckyEvade` → 各值 x²/100（:1450-1456）：47 → 22.09。
#[test]
fn evade_unlucky_squares_each_value() {
    let db = db_with(vec![Modifier::flag("UnluckyEvade")]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!(
        (s.evade_chance - 22.09).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 22.09).abs() < EPS);
}

/// 敌方 HitChance 乘区进公式（:1437 `× hitChance`）：
/// enemy +100% HitChance INC → mult 2.0 → 100 − 53×2 = −6；
/// 四分型 clamp 0（:1438 `m_max(0, …)`）；综合不 split → = melee = 0。
#[test]
fn evade_enemy_hit_chance_mult_can_floor_types_to_zero() {
    let s = calc_evade_suite(
        &ModDb::new(),
        &CalcConfig::default(),
        4000.0,
        1000.0,
        2.0,
        false,
    );
    assert!((s.melee - 0.0).abs() < EPS, "got {}", s.melee);
    assert!((s.evade_chance - 0.0).abs() < EPS);
}

/// 零闪避裸面板：monsterHitChance(0, acc) = 100（:44 分子为 0）→ 全 0。
#[test]
fn evade_zero_evasion_is_zero() {
    let s = suite(&ModDb::new(), 0.0, 1000.0);
    assert_eq!(s, EvadeSuite::default());
}

/// 敌精准 0 且有闪避：mhc = round((1 − 0.95)×100) = 5（clamp 下限）→ evade = 95。
#[test]
fn evade_zero_enemy_accuracy_hits_floor_5() {
    let s = suite(&ModDb::new(), 4000.0, 0.0);
    assert!(
        (s.evade_chance - 95.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
}

/// perform 端到端：fill_evade_stun 把四分型写入 OutputTable（perform.rs 一行调用）。
/// 玩家 Evasion=4000（base 直喂），enemy accuracy 设 1000 → 全 47。
#[test]
fn perform_fills_evade_suite_into_output() {
    let base = ActorBaseStats {
        life: 1000.0,
        evasion: 4000.0,
        ..ActorBaseStats::default()
    };
    let mut env = Env::new(Actor::new(1, base));
    env.enemy.base.accuracy = 1000.0;
    perform(&mut env).unwrap();

    assert!((env.player.output.evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.melee_evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.projectile_evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.spell_evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.spell_projectile_evade_chance - 47.0).abs() < EPS);
}

// ─────────────────────────────────────────────────────────────────
// Stun 体系（CalcDefence.lua:2525-2643）+ avoid_stun ES 条件修复（:2554-2557）
// ─────────────────────────────────────────────────────────────────

use pobr_core::calc::defence::calc_avoidance;
use pobr_core::calc::stun::{StunInputs, calc_stun, calc_stun_threshold};

fn stun_inputs(life: f64) -> StunInputs {
    StunInputs {
        life,
        ..StunInputs::default()
    }
}

/// 阈值默认基底 = output.Life（:2542-2543）：life 1000 → 1000。
#[test]
fn stun_threshold_defaults_to_life() {
    let t = calc_stun_threshold(&ModDb::new(), &CalcConfig::default(), &stun_inputs(1000.0));
    assert!((t - 1000.0).abs() < EPS, "got {t}");
}

/// ES 基底切换（:2529-2532）：flag + StunThresholdEnergyShieldPercent 50，ES 2000
/// → 2000×50/100 = 1000（life 被忽略）。
#[test]
fn stun_threshold_es_based() {
    let db = db_with(vec![
        Modifier::flag("StunThresholdBasedOnEnergyShieldInsteadOfLife"),
        Modifier::number("StunThresholdEnergyShieldPercent", ModType::Base, 50.0),
    ]);
    let inp = StunInputs {
        life: 5000.0,
        energy_shield: 2000.0,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 1000.0).abs() < EPS, "got {t}");
}

/// Mana 基底切换（:2533-2536）：flag + StunThresholdManaPercent 100，mana 800 → 800。
#[test]
fn stun_threshold_mana_based() {
    let db = db_with(vec![
        Modifier::flag("StunThresholdBasedOnManaInsteadOfLife"),
        Modifier::number("StunThresholdManaPercent", ModType::Base, 100.0),
    ]);
    let inp = StunInputs {
        life: 5000.0,
        mana: 800.0,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 800.0).abs() < EPS, "got {t}");
}

/// CI 分支（:2537-2539）：阈值取「CI 前 Life」= flat 基础 + ΣBASE MaximumLife，
/// 而非 CI 后的 output.Life(=1)。
#[test]
fn stun_threshold_ci_uses_pre_ci_life() {
    let db = db_with(vec![Modifier::number("MaximumLife", ModType::Base, 900.0)]);
    let inp = StunInputs {
        life: 1.0, // CI 后池
        life_base_flat: 100.0,
        chaos_inoculation: true,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 1000.0).abs() < EPS, "got {t}");
}

/// AddESToStunThreshold（:2544-2548）：life 1000 + ES 2000×50% → 2000。
#[test]
fn stun_threshold_add_es() {
    let db = db_with(vec![
        Modifier::flag("AddESToStunThreshold"),
        Modifier::number("ESToStunThresholdPercent", ModType::Base, 50.0),
    ]);
    let inp = StunInputs {
        life: 1000.0,
        energy_shield: 2000.0,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 2000.0).abs() < EPS, "got {t}");
}

/// BASE/INC/MORE 聚合（:2549-2552）：(1000 + 200) × 1.3 × 2（阈值翻倍词条 MORE 100）= 3120。
#[test]
fn stun_threshold_aggregation() {
    let db = db_with(vec![
        Modifier::number("StunThreshold", ModType::Base, 200.0),
        Modifier::number("StunThreshold", ModType::Inc, 30.0),
        Modifier::number("StunThreshold", ModType::More, 100.0),
    ]);
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &stun_inputs(1000.0));
    assert!((t - 3120.0).abs() < EPS, "got {t}");
}

/// 时长帧上取整（:2594）：base 0.5s / tick 0.033 = 15.15… → ceil 16 帧 → 0.528s。
#[test]
fn stun_duration_rounds_up_to_server_tick() {
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &stun_inputs(1000.0));
    assert!(
        (r.stun_duration - 0.528).abs() < EPS,
        "got {}",
        r.stun_duration
    );
}

/// 时长 INC（:2591/:2594）：StunDuration +100% → 1.0/0.033 = 30.3 → 31 帧 → 1.023s。
#[test]
fn stun_duration_scales_with_inc() {
    let db = db_with(vec![Modifier::number("StunDuration", ModType::Inc, 100.0)]);
    let r = calc_stun(&db, &CalcConfig::default(), &stun_inputs(1000.0));
    assert!(
        (r.stun_duration - 1.023).abs() < EPS,
        "got {}",
        r.stun_duration
    );
}

/// 恢复缩短时长（:2593-2594 分母）：StunRecovery +100% → 0.25/0.033 = 7.57 → 8 帧 → 0.264s。
#[test]
fn stun_duration_shortened_by_recovery() {
    let db = db_with(vec![Modifier::number("StunRecovery", ModType::Inc, 100.0)]);
    let r = calc_stun(&db, &CalcConfig::default(), &stun_inputs(1000.0));
    assert!(
        (r.stun_duration - 0.264).abs() < EPS,
        "got {}",
        r.stun_duration
    );
}

/// 规避 ≥100%（StunImmune 等）→ 时长 0、几率 0（:2584-2586）。
#[test]
fn stun_avoid_100_zeroes_duration_and_chance() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 1000.0,
        physical_taken_hit: 1000.0,
        avoid_stun: 100.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert_eq!(r.stun_duration, 0.0);
    assert_eq!(r.self_stun_chance, 0.0);
}

/// 受眩晕几率（:2617-2624）：taken 1000 / phys 1000 / 阈值 1000 →
/// eff = (1000 + 1000×0.25) × 1.0 = 1250；base = min(200×1250/1000, 100) = 100 > 20
/// → SelfStunChance = 100 × (100−0)/100 = 100。
#[test]
fn stun_chance_full_hit_is_100() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 1000.0,
        physical_taken_hit: 1000.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert!(
        (r.self_stun_chance - 100.0).abs() < EPS,
        "got {}",
        r.self_stun_chance
    );
}

/// MinStunChanceNeeded(20) 门槛（:2624）：taken 50 / phys 50 → eff 62.5 →
/// base = 200×62.5/1000 = 12.5 < 20 → 归 0。
#[test]
fn stun_chance_below_min_needed_is_zero() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 50.0,
        physical_taken_hit: 50.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert_eq!(r.self_stun_chance, 0.0);
}

/// 几率乘 (1 − 规避)（:2624）：avoid_stun 50 → 100 × 50/100 = 50。
#[test]
fn stun_chance_scaled_by_not_avoid() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 1000.0,
        physical_taken_hit: 1000.0,
        avoid_stun: 50.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert!(
        (r.self_stun_chance - 50.0).abs() < EPS,
        "got {}",
        r.self_stun_chance
    );
}

/// 物理 ×0.25 加权（:2617）：taken 100（全物理）→ eff = 125 →
/// base = 200×125/1000 = 25 > 20 → 25。
#[test]
fn stun_chance_physical_weighted_quarter() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 100.0,
        physical_taken_hit: 100.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert!(
        (r.self_stun_chance - 25.0).abs() < EPS,
        "got {}",
        r.self_stun_chance
    );
}

// --- avoid_stun ES 条件修复（CalcDefence.lua:2554-2557，M2-E2）---

/// ES > totalTakenHit 且非 EB → notAvoid ×0.5 → 隐式 50% 规避。
#[test]
fn avoid_stun_es_above_taken_hit_halves() {
    let r = calc_avoidance(&ModDb::new(), &CalcConfig::default(), 2000.0, 1000.0, false);
    assert_eq!(r.avoid_stun, 50.0);
}

/// ES ≤ totalTakenHit → 不减半（旧实现「ES > 0 即减半」在此修正）。
#[test]
fn avoid_stun_es_below_taken_hit_no_halving() {
    let r = calc_avoidance(&ModDb::new(), &CalcConfig::default(), 500.0, 1000.0, false);
    assert_eq!(r.avoid_stun, 0.0);
}

/// EB（EnergyShieldProtectsMana）→ 即使 ES > takenHit 也不减半（:2555）。
#[test]
fn avoid_stun_eb_disables_es_halving() {
    let r = calc_avoidance(&ModDb::new(), &CalcConfig::default(), 2000.0, 1000.0, true);
    assert_eq!(r.avoid_stun, 0.0);
}

/// perform 端到端：fill_evade_stun 写 stun 三字段。
/// life 1000、无 ES → 阈值 1000；参考伤 = reference_hit = 1000（视作纯物理）→
/// eff = 1250 → SelfStunChance 100；avoid_stun = 0 → 时长 0.528s。
#[test]
fn perform_fills_stun_into_output() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = Env::new(Actor::new(1, base));
    perform(&mut env).unwrap();

    assert!((env.player.output.stun_threshold - 1000.0).abs() < EPS);
    assert!((env.player.output.self_stun_chance - 100.0).abs() < EPS);
    assert!((env.player.output.stun_duration - 0.528).abs() < EPS);
}

// ─────────────────────────────────────────────────────────────────
// 抗性下界 −200（M2-E3 选做，13-G13；CalcDefence.lua:886/:919，Data.lua:180）
// ─────────────────────────────────────────────────────────────────

use pobr_core::calc::{MinimalInput, calculate_minimal};

/// 深负抗触底：base −300 → final = max(min(−300, 75), −200) = −200。
#[test]
fn resistance_floored_at_minus_200() {
    let input = MinimalInput {
        base_life: 100.0,
        base_fire_resistance: -300.0,
        ..MinimalInput::default()
    };
    let out = calculate_minimal(&ModDb::new(), &CalcConfig::default(), &input);
    assert_eq!(out.fire_resistance, -200.0);
}

/// 下界以内的负抗不受影响：base −60 → final = −60（修复不改常规路径）。
#[test]
fn resistance_above_floor_unchanged() {
    let input = MinimalInput {
        base_life: 100.0,
        base_cold_resistance: -60.0,
        ..MinimalInput::default()
    };
    let out = calculate_minimal(&ModDb::new(), &CalcConfig::default(), &input);
    assert_eq!(out.cold_resistance, -60.0);
}
