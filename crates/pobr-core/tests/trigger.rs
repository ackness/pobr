//! 触发域集成测试（速率上限 + 帧节流 + ICDR + 双门控）。
//!
//! 出处：agent-docs/triggers.md §三；PoB2 CalcTriggers.lua / Data.lua。

use pobr_core::calc::trigger::{
    action_cooldown, resolve_trigger_rate, round_cooldown_to_tick, server_tick_rate,
    trigger_rate_cap,
};
use pobr_data::prelude::SERVER_TICK_SECONDS;

#[test]
fn server_tick_rate_is_inverse_of_constant() {
    assert!((server_tick_rate() - 1.0 / SERVER_TICK_SECONDS).abs() < 1e-12);
    // ≈ 30.30/s。
    assert!((server_tick_rate() - 30.303_030_303_03).abs() < 1e-6);
}

#[test]
fn cooldown_is_rounded_up_to_server_frame() {
    let rate = server_tick_rate();
    // 0.2s 冷却：ceil(0.2 × 30.303) = ceil(6.06) = 7 帧 → 7/30.303 ≈ 0.231s。
    let rounded = round_cooldown_to_tick(0.2, rate);
    assert!((rounded - 7.0 / rate).abs() < 1e-9);
    assert!(rounded >= 0.2);
}

#[test]
fn trigger_rate_cap_formula_matches_doc() {
    // cap = 1 / (ceil(cd × rate) / rate)，对照 agent-docs/triggers.md §3.1。
    let rate = server_tick_rate();
    let cd = 0.3;
    let cap = trigger_rate_cap(cd, rate);
    let expected = 1.0 / round_cooldown_to_tick(cd, rate);
    assert!((cap - expected).abs() < 1e-6);
}

#[test]
fn zero_cooldown_yields_zero_cap() {
    assert_eq!(trigger_rate_cap(0.0, server_tick_rate()), 0.0);
    assert_eq!(round_cooldown_to_tick(0.0, server_tick_rate()), 0.0);
}

#[test]
fn icdr_divides_trigger_cooldown() {
    // ICDR=2.0 把 0.4s 触发宝石冷却缩短到 0.2s（被触发技能无冷却）。
    let cd = action_cooldown(0.4, 0.0, 2.0);
    assert!((cd - 0.2).abs() < 1e-9);
}

#[test]
fn action_cooldown_takes_larger_of_two() {
    // max(triggeredCD=0.6, triggerCD/icdr=0.4/2=0.2) = 0.6。
    let cd = action_cooldown(0.4, 0.6, 2.0);
    assert!((cd - 0.6).abs() < 1e-9);
}

#[test]
fn zero_icdr_falls_back_to_raw_trigger_cd() {
    // icdr<=0 时不除（避免除零），直接用 trigger_cd。
    let cd = action_cooldown(0.4, 0.0, 0.0);
    assert!((cd - 0.4).abs() < 1e-9);
}

#[test]
fn skill_trigger_rate_is_min_of_cap_and_source() {
    // 双门控：源速率 1.5/s 低于上限 → 实际速率被源门控。
    let r = resolve_trigger_rate(0.15, 0.0, 1.0, 1.5);
    assert!(r.limited_by_source);
    assert!((r.skill_trigger_rate - 1.5).abs() < 1e-6);
    assert!(r.trigger_rate_cap > 1.5);
}

#[test]
fn fast_source_is_capped_by_rate_cap() {
    // 源速率 50/s 高于上限 → 实际速率 = 上限。
    let r = resolve_trigger_rate(0.3, 0.0, 1.0, 50.0);
    assert!(!r.limited_by_source);
    assert!((r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9);
}

#[test]
fn rate_cap_cooldown_is_frame_aligned() {
    let rate = server_tick_rate();
    let r = resolve_trigger_rate(0.25, 0.0, 1.0, 100.0);
    // rate_cap_cooldown 应为帧对齐值，且 cap = 1/它。
    assert!((r.rate_cap_cooldown - round_cooldown_to_tick(0.25, rate)).abs() < 1e-9);
    assert!((r.trigger_rate_cap - 1.0 / r.rate_cap_cooldown).abs() < 1e-6);
}

#[test]
fn higher_icdr_raises_trigger_rate_cap() {
    // ICDR 越高 → 冷却越短 → 上限越高（单调）。
    let low = resolve_trigger_rate(0.5, 0.0, 1.0, 100.0);
    let high = resolve_trigger_rate(0.5, 0.0, 2.0, 100.0);
    assert!(high.trigger_rate_cap > low.trigger_rate_cap);
}
