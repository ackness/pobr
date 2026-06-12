//! 弩 reload 端到端（M4-T4 W-D2）：perform `fill_crossbow_reload` 把弹匣循环平均
//! （bolt_count 发 × 攻速 + reload 间隔）折进有效速率与 DPS。
//!
//! vendor 参照 `CalcOffence.lua:2867-2887`；数据通道 = `CrossbowReloadTimeBase` /
//! `CrossbowBoltCount` BASE（编排层注入，此处直接经 session 注入等价词条）。

use pobr_core::Modifier;
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_data::prelude::*;

fn crossbow_input() -> MinimalInput {
    MinimalInput {
        base_life: 100.0,
        base_mana: 0.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 3.0,
    }
}

/// 蓝图手算用例：bolt=5、reload=0.8s、射速 3/s →
/// 循环平均 = 5 / (5/3 + 0.8) ≈ 2.027/s；DPS 等比缩放（avg 150 × 2.027 ≈ 304.05）。
#[test]
fn reload_cycle_average_scales_rate_and_dps() {
    let mut session = CalculationSession::new(crossbow_input());
    session.add_modifiers(vec![
        Modifier::number("CrossbowReloadTimeBase", ModType::Base, 0.8),
        Modifier::number("CrossbowBoltCount", ModType::Base, 5.0),
    ]);
    session.perform_minimal();
    let out = session.output();

    let expected_rate = 5.0 / (5.0 / 3.0 + 0.8); // ≈ 2.027027
    assert!(
        (out.effective_action_rate - expected_rate).abs() < 1e-2,
        "循环平均速率：{} vs {expected_rate}",
        out.effective_action_rate
    );
    assert!(
        (out.action_rate - expected_rate).abs() < 1e-2,
        "面板速率（PoB2 output.Speed 改写口径）：{}",
        out.action_rate
    );
    // DPS = avg(150) × 循环平均速率。
    assert!(
        (out.dps - 150.0 * expected_rate).abs() < 1.0,
        "DPS 等比折算：{} vs {}",
        out.dps,
        150.0 * expected_rate
    );
    // AverageDamage 恒等式（parity 口径）不受影响：dps / action_rate == 150。
    assert!(
        (out.dps / out.action_rate - 150.0).abs() < 1e-6,
        "dps/action_rate 恒等式破坏：{}",
        out.dps / out.action_rate
    );
}

/// ChanceToNotConsumeAmmo ≥ 100：退化为纯射速（reload 不触发，输出与无 reload 一致）。
#[test]
fn not_consume_ammo_100_keeps_firing_rate() {
    let mut session = CalculationSession::new(crossbow_input());
    session.add_modifiers(vec![
        Modifier::number("CrossbowReloadTimeBase", ModType::Base, 0.8),
        Modifier::number("CrossbowBoltCount", ModType::Base, 5.0),
        Modifier::number("ChanceToNotConsumeAmmo", ModType::Base, 100.0),
    ]);
    session.perform_minimal();
    let out = session.output();
    assert!(
        (out.effective_action_rate - 3.0).abs() < 1e-9,
        "不消耗弹药必须退化为纯射速：{}",
        out.effective_action_rate
    );
    assert!((out.dps - 450.0).abs() < 1e-6, "DPS 不得折算：{}", out.dps);
}

/// 无 reload 词条（非弩）：整段零行为，输出与历史逐值一致。
#[test]
fn no_reload_data_is_neutral() {
    let mut baseline = CalculationSession::new(crossbow_input());
    baseline.perform_minimal();
    let base_out = baseline.output().clone();

    let mut with_bolts_only = CalculationSession::new(crossbow_input());
    // 只有弹匣词条、无 reload 基值：同样零行为（reload 数据缺失不进模型）。
    with_bolts_only.add_modifiers(vec![Modifier::number(
        "CrossbowBoltCount",
        ModType::Base,
        5.0,
    )]);
    with_bolts_only.perform_minimal();
    let out = with_bolts_only.output();
    assert_eq!(out.effective_action_rate, base_out.effective_action_rate);
    assert_eq!(out.dps, base_out.dps);
}

/// ReloadSpeed / 攻速增益加快装填（vendor calcLib.mod("ReloadSpeed","Speed")）：
/// +50% 攻速同时提升射速与装填速度。
#[test]
fn attack_speed_scales_reload_too() {
    let mut session = CalculationSession::new(crossbow_input());
    session.add_modifiers(vec![
        Modifier::number("CrossbowReloadTimeBase", ModType::Base, 0.8),
        Modifier::number("CrossbowBoltCount", ModType::Base, 5.0),
        Modifier::number("AttackSpeed", ModType::Inc, 50.0),
    ]);
    session.perform_minimal();
    let out = session.output();
    // 射速 4.5/s；reload = 0.8 / 1.5 ≈ 0.5333 → 5 / (5/4.5 + 0.5333) ≈ 3.0405。
    let expected = 5.0 / (5.0 / 4.5 + 0.8 / 1.5);
    assert!(
        (out.effective_action_rate - expected).abs() < 1e-2,
        "攻速须同时加快装填：{} vs {expected}",
        out.effective_action_rate
    );
}
