//! PoB2 测试套件移植（golden parity）。
//!
//! 把 vendor PoB2 `spec/System/Test*_spec.lua` 中**孤立机制**用例的输入/期望值移植为
//! PoBR 计算断言：每个用例锁定一个机制（暴击倍率、暴击均值公式、增伤聚合、最大承受
//! 击中…），期望值取自 PoB2 测试（字面量）。目标：与 PoB2 偏差 < 10%（纯公式用例要求精确）。
//!
//! 来源映射记于每个用例。新增机制时优先在此补 golden 用例（比真实 ninja build 易调试）。

use pobr_core::calc::{CalculationSession, MinimalInput, calculate_minimal};
use pobr_core::mod_parser::{ParseStatus, parse_mod};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

/// 相对误差 < 10%（PoB2 parity 目标）。`golden` 为 0 时退化为绝对误差。
fn within_10pct(actual: f64, golden: f64) -> bool {
    if golden == 0.0 {
        actual.abs() < 1e-6
    } else {
        ((actual - golden) / golden).abs() < 0.10
    }
}

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

/// PoB2 TestDefence「no armour max hits」(基线)：默认角色 Life 60、四类抗性 -60%、
/// 无护甲 → 物理最大承受击中 = Life 60；元素/混沌 = 60/(1+0.6) = 37.5（PoB2 取整 38）。
/// 验证 PoBR `max_hit_for_type = pool/(1-res/100)` 与 PoB2 一致。
#[test]
fn defence_no_armour_max_hits() {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    // 混沌抗不在 MinimalInput，单独注入 -60%。
    session.add_modifiers([Modifier::number("ChaosResistance", ModType::Base, -60.0)]);
    session.perform_minimal();
    let o = session.output();

    assert!(
        within_10pct(o.physical_max_hit, 60.0),
        "physical_max_hit = {} (PoB2 60)",
        o.physical_max_hit
    );
    for (name, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
        ("chaos", o.chaos_max_hit),
    ] {
        assert!(within_10pct(v, 38.0), "{name}_max_hit = {v} (PoB2 38)");
    }
}

/// PoB2 TestDefence「no armour max hits」(+200 抗 / +200% 物理减伤)：
/// 元素/混沌抗 -60+200=140 → 上限 75% → 承受 0.25 → 60/0.25 = 240；
/// 物理 200% 额外减伤 → 减伤上限 90% → 承受 0.1 → 60/0.1 = 600。
/// 验证 PoBR 抗性上限（75）与物理减伤上限（90%）与 PoB2 一致。
#[test]
fn defence_capped_res_and_pdr() {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("FireResistance", ModType::Base, 200.0),
        Modifier::number("ColdResistance", ModType::Base, 200.0),
        Modifier::number("LightningResistance", ModType::Base, 200.0),
        Modifier::number("ChaosResistance", ModType::Base, 200.0),
        Modifier::number("PhysicalDamageReduction", ModType::Base, 200.0),
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 600.0),
        "physical_max_hit = {} (PoB2 600)",
        o.physical_max_hit
    );
    for (name, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
        ("chaos", o.chaos_max_hit),
    ] {
        assert!(within_10pct(v, 240.0), "{name}_max_hit = {v} (PoB2 240)");
    }
}

/// PoB2 TestAttacks「correctly adds damage with oracle forced outcome」(inevitable)：
/// base 1、暴击 10%、倍率 2、「inevitable critical hits」→ 平均击中
/// = 1 + (2-1)*(1*0.1 + 0.7*0.9*0.1 + 0.4*0.9²*0.1 + 0.1*0.9³*0.1) = 1.20269。
#[test]
fn crit_inevitable_forced_outcome() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let out = calculate_minimal(&db, &cfg, &input_base_hit(1.0, 1.0));
    let s = 1.0 * 0.1 + 0.7 * 0.9 * 0.1 + 0.4 * 0.9_f64.powi(2) * 0.1 + 0.1 * 0.9_f64.powi(3) * 0.1;
    let expected = 1.0 * (1.0 + (2.0 - 1.0) * s);
    assert!(
        within_10pct(out.total_hit_avg, expected),
        "inevitable total_hit_avg = {} (PoB2 {:.5})",
        out.total_hit_avg,
        expected
    );
}

/// PoB2 TestAttacks「correctly calculates forced outcome with bifurcated critical hits」：
/// base 1、暴击 10%、倍率 2、bifurcate+inevitable → 平均击中
/// = 1 + (2-1)*(2*0.1*(1 + 0.7*(1-0.1)² + 0.4*((1-0.1)²)² + 0.1*((1-0.1)²)³))。
#[test]
fn crit_bifurcate_inevitable() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    db.add_mod(Modifier::flag("BifurcateCrit"));
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let out = calculate_minimal(&db, &cfg, &input_base_hit(1.0, 1.0));
    let q = (1.0_f64 - 0.1).powi(2);
    let expected =
        1.0 + (2.0 - 1.0) * (2.0 * 0.1 * (1.0 + 0.7 * q + 0.4 * q.powi(2) + 0.1 * q.powi(3)));
    assert!(
        within_10pct(out.total_hit_avg, expected),
        "bifurcate+inevitable total_hit_avg = {} (PoB2 {:.5})",
        out.total_hit_avg,
        expected
    );
}

/// PoB2 TestDefence「armoured max hits」：Life 1000、抗性 -60%、护甲 10000、无 PDR。
/// 物理最大承受击中需**自洽**（护甲减伤依赖击中大小）：H 满足 H*taken(H)=pool →
/// H²-1000H-10^6=0 → H≈1618（PoB2 PhysicalMaximumHitTaken）；元素 = 1000/1.6 = 625。
#[test]
fn defence_armoured_max_hits() {
    let input = MinimalInput {
        base_life: 1000.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("Armour", ModType::Base, 10000.0),
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 1618.0),
        "physical_max_hit = {} (PoB2 1618)",
        o.physical_max_hit
    );
    for (name, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
        ("chaos", o.chaos_max_hit),
    ] {
        assert!(within_10pct(v, 625.0), "{name}_max_hit = {v} (PoB2 625)");
    }
}

/// 共用：无甲、Life 60、四类抗 -60% 基线 + 自定义防御词条 → (phys, fire, cold, lightning, chaos) 最大承受击中。
fn defence_max_hits(mods: &[(&str, ModType, f64)]) -> (f64, f64, f64, f64, f64) {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    let mut list = vec![Modifier::number("ChaosResistance", ModType::Base, -60.0)];
    for (n, t, v) in mods {
        list.push(Modifier::number(*n, *t, *v));
    }
    session.add_modifiers(list);
    session.perform_minimal();
    let o = session.output();
    (
        o.physical_max_hit,
        o.fire_max_hit,
        o.cold_max_hit,
        o.lightning_max_hit,
        o.chaos_max_hit,
    )
}

/// PoB2 TestDefence「no armour max hits」(+200 抗 / +200 max 抗 / 50% reduced damage taken)：
/// 元素抗 140→硬上限 90→承受 0.1，再 ×(1-0.5) reduced → 60/0.05 = 1200；物理 60/0.5 = 120。
/// 验证最大抗性硬上限(90) + 玩家侧 DamageTaken(reduced=INC<0)。
#[test]
fn defence_max_res_and_reduced_taken() {
    let (p, f, c, l, ch) = defence_max_hits(&[
        ("FireResistance", ModType::Base, 200.0),
        ("ColdResistance", ModType::Base, 200.0),
        ("LightningResistance", ModType::Base, 200.0),
        ("ChaosResistance", ModType::Base, 200.0),
        ("MaximumAllElementalResistances", ModType::Base, 200.0),
        ("MaximumChaosResistance", ModType::Base, 200.0),
        ("DamageTaken", ModType::Inc, -50.0),
    ]);
    assert!(within_10pct(p, 120.0), "physical = {p} (PoB2 120)");
    for (n, v) in [("fire", f), ("cold", c), ("lightning", l), ("chaos", ch)] {
        assert!(within_10pct(v, 1200.0), "{n} = {v} (PoB2 1200)");
    }
}

/// PoB2 TestDefence「no armour max hits」(+ 再叠 50% less damage taken)：
/// dt = (1-0.5) reduced × 0.5 less = 0.25 → 元素 60/(0.1*0.25)=2400；物理 60/0.25 = 240。
/// 验证 DamageTaken less=MORE 与 reduced 叠乘。
#[test]
fn defence_reduced_and_less_taken() {
    let (p, f, c, l, ch) = defence_max_hits(&[
        ("FireResistance", ModType::Base, 200.0),
        ("ColdResistance", ModType::Base, 200.0),
        ("LightningResistance", ModType::Base, 200.0),
        ("ChaosResistance", ModType::Base, 200.0),
        ("MaximumAllElementalResistances", ModType::Base, 200.0),
        ("MaximumChaosResistance", ModType::Base, 200.0),
        ("DamageTaken", ModType::Inc, -50.0),
        ("DamageTaken", ModType::More, -50.0),
    ]);
    assert!(within_10pct(p, 240.0), "physical = {p} (PoB2 240)");
    for (n, v) in [("fire", f), ("cold", c), ("lightning", l), ("chaos", ch)] {
        assert!(within_10pct(v, 2400.0), "{n} = {v} (PoB2 2400)");
    }
}

/// PoB2 TestDefence「no armour max hits」(ES 血池 + 满叠减伤)：Life 60 + ES 60（pool 120）、
/// max抗硬上限 90、reduced 50% + less 50% + nearby 20% less（dt = 0.5×0.5×0.8 = 0.2）。
/// 元素 = 120/(0.1×0.2) = 6000；物理 = 120/0.2 = 600；混沌 pool = life + 0.5×ES = 90 →
/// 90/(0.1×0.2) = 4500。验证 ES 血池 + 混沌半 ES + 多重 DamageTaken 叠乘。
#[test]
fn defence_es_pool_and_chaos_half() {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("EnergyShield", ModType::Base, 60.0),
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("FireResistance", ModType::Base, 200.0),
        Modifier::number("ColdResistance", ModType::Base, 200.0),
        Modifier::number("LightningResistance", ModType::Base, 200.0),
        Modifier::number("ChaosResistance", ModType::Base, 200.0),
        Modifier::number("MaximumAllElementalResistances", ModType::Base, 200.0),
        Modifier::number("MaximumChaosResistance", ModType::Base, 200.0),
        Modifier::number("DamageTaken", ModType::Inc, -50.0), // 50% reduced
        Modifier::number("DamageTaken", ModType::More, -50.0), // 50% less
        Modifier::number("DamageTaken", ModType::More, -20.0), // nearby enemies 20% less
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 600.0),
        "physical = {} (PoB2 600)",
        o.physical_max_hit
    );
    for (n, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
    ] {
        assert!(within_10pct(v, 6000.0), "{n} = {v} (PoB2 6000)");
    }
    assert!(
        within_10pct(o.chaos_max_hit, 4500.0),
        "chaos = {} (PoB2 4500, pool=life+0.5*ES)",
        o.chaos_max_hit
    );
}
