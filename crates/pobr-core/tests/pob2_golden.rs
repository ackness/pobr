//! PoB2 测试套件移植（golden parity）。
//!
//! 把 vendor PoB2 `spec/System/Test*_spec.lua` 中**孤立机制**用例的输入/期望值移植为
//! PoBR 计算断言：每个用例锁定一个机制（暴击倍率、暴击均值公式、增伤聚合、最大承受
//! 击中…），期望值取自 PoB2 测试（字面量）。目标：与 PoB2 偏差 < 10%（纯公式用例要求精确）。
//!
//! 来源映射记于每个用例。新增机制时优先在此补 golden 用例（比真实 ninja build 易调试）。

use pobr_core::calc::{MinimalInput, calculate_minimal};
use pobr_core::mod_parser::{ParseStatus, parse_mod};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

/// 把 PoB 词条文本解析并注入 db（用于复现 PoB 测试的 customMods / 物品词条）。
fn add_text(db: &mut ModDb, text: &str) {
    let outcome = parse_mod(text).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
    if outcome.status == ParseStatus::Parsed {
        db.add_list(outcome.mods);
    } else {
        panic!("unsupported modifier in golden test: {text:?}");
    }
}

fn input_base_hit(min: f64, max: f64) -> MinimalInput {
    MinimalInput {
        base_life: 1000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: min,
        base_hit_max: max,
        base_action_rate: 1.0,
    }
}

/// PoB2 TestAttacks「creates an item and has the correct crit multi」：
/// 基础暴击倍率 2.0 + 「25% increased Critical Damage Bonus」→ 2.25。
#[test]
fn crit_multiplier_base_plus_increase() {
    let mut db = ModDb::new();
    add_text(&mut db, "25% increased Critical Damage Bonus");
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(1.0, 1.0));
    assert!(
        (out.crit_multiplier - 2.25).abs() < 1e-6,
        "crit_multiplier = {} (expected 2.25)",
        out.crit_multiplier
    );
}

/// PoB2 TestAttacks「correctly calculates critical hit damage with static values」：
/// 基础击中 1、暴击率 10%、倍率 2 → 平均击中 = (1-0.1)*1 + 0.1*1*2 = 1.1。
#[test]
fn crit_average_static() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(1.0, 1.0));
    assert!(
        (out.crit_chance - 0.10).abs() < 1e-6,
        "crit_chance = {} (expected 0.10)",
        out.crit_chance
    );
    assert!(
        (out.total_hit_avg - 1.1).abs() < 1e-6,
        "total_hit_avg = {} (expected 1.1)",
        out.total_hit_avg
    );
}

/// PoB2 TestAttacks「does not force critical hits when critical hit chance is zero」：
/// 暴击率 0 → 暴击率 0、平均击中 = 基础（1）。
#[test]
fn no_crit_when_zero_chance() {
    let db = ModDb::new();
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(1.0, 1.0));
    assert_eq!(out.crit_chance, 0.0);
    assert!(
        (out.total_hit_avg - 1.0).abs() < 1e-6,
        "total_hit_avg = {} (expected 1.0)",
        out.total_hit_avg
    );
}

/// PoB2 TestAttacks「correctly converts spell damage per stat to attack damage」(节选)：
/// 「10% increased attack damage」→ 攻击域 Damage INC = 10。
#[test]
fn attack_damage_increase_aggregation() {
    let mut db = ModDb::new();
    add_text(&mut db, "10% increased attack damage");
    add_text(&mut db, "10% increased spell damage");
    let attack = CalcConfig::attack();
    let inc = db.sum(ModType::Inc, &attack, &[ModName::from("AttackDamage")]);
    assert!(
        (inc - 10.0).abs() < 1e-6,
        "attack Damage INC = {inc} (expected 10)"
    );
}
