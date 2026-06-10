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
