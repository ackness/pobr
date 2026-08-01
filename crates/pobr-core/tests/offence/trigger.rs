//! 触发域集成测试：
//! - §一  冷却驱动（速率上限 + 帧节流 + ICDR + 双门控）
//! - §二  能量驱动元宝石模型（max_energy / energy_per_event / effective_trigger_rate）
//! - §三  多技能轮转（calcMultiSpellRotationImpact 移植）
//! - §四  CWC（Cast While Channelling）
//! - §五  归因 TraceGraph
//!
//! 出处：agent-docs/triggers.md §二~§五；PoB2 CalcTriggers.lua / Data.lua / act_int.lua。

use pobr_core::TraceGraph;
use pobr_core::calc::trigger::{
    RotationSkill, SocketedSpellInfo, TriggerCondition, action_cooldown, calc_cwc_trigger_rate,
    calc_energy_per_event, calc_energy_trigger_rate, calc_max_energy, calc_multi_spell_rotation,
    resolve_trigger_rate, round_cooldown_to_tick, server_tick_rate,
    spell_cast_time_added_to_cooldown, trigger_rate_cap,
};
use pobr_core::calc::{
    calc_cwc_trigger_rate_traced, calc_energy_trigger_rate_traced, resolve_trigger_rate_traced,
};
use pobr_data::prelude::SERVER_TICK_SECONDS;

// §一  冷却驱动基础（回归：与原有测试保持严格一致）

#[test]
fn server_tick_rate_is_inverse_of_constant() {
    assert!((server_tick_rate(SERVER_TICK_SECONDS) - 1.0 / SERVER_TICK_SECONDS).abs() < 1e-12);
    // ≈ 30.30/s。
    assert!((server_tick_rate(SERVER_TICK_SECONDS) - 30.303_030_303_03).abs() < 1e-6);
}

#[test]
fn cooldown_is_rounded_up_to_server_frame() {
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    // 0.2s 冷却：ceil(0.2 × 30.303) = ceil(6.06) = 7 帧 → 7/30.303 ≈ 0.231s。
    let rounded = round_cooldown_to_tick(0.2, rate);
    assert!((rounded - 7.0 / rate).abs() < 1e-9);
    assert!(rounded >= 0.2);
}

#[test]
fn trigger_rate_cap_formula_matches_doc() {
    // cap = 1 / (ceil(cd × rate) / rate)，对照 agent-docs/triggers.md §3.1。
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    let cd = 0.3;
    let cap = trigger_rate_cap(cd, rate);
    let expected = 1.0 / round_cooldown_to_tick(cd, rate);
    assert!((cap - expected).abs() < 1e-6);
}

#[test]
fn zero_cooldown_yields_zero_cap() {
    assert_eq!(
        trigger_rate_cap(0.0, server_tick_rate(SERVER_TICK_SECONDS)),
        0.0
    );
    assert_eq!(
        round_cooldown_to_tick(0.0, server_tick_rate(SERVER_TICK_SECONDS)),
        0.0
    );
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
    let r = resolve_trigger_rate(0.15, 0.0, 1.0, 1.5, SERVER_TICK_SECONDS);
    assert!(r.limited_by_source);
    assert!((r.skill_trigger_rate - 1.5).abs() < 1e-6);
    assert!(r.trigger_rate_cap > 1.5);
}

#[test]
fn fast_source_is_capped_by_rate_cap() {
    // 源速率 50/s 高于上限 → 实际速率 = 上限。
    let r = resolve_trigger_rate(0.3, 0.0, 1.0, 50.0, SERVER_TICK_SECONDS);
    assert!(!r.limited_by_source);
    assert!((r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9);
}

#[test]
fn rate_cap_cooldown_is_frame_aligned() {
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    let r = resolve_trigger_rate(0.25, 0.0, 1.0, 100.0, SERVER_TICK_SECONDS);
    // rate_cap_cooldown 应为帧对齐值，且 cap = 1/它。
    assert!((r.rate_cap_cooldown - round_cooldown_to_tick(0.25, rate)).abs() < 1e-9);
    assert!((r.trigger_rate_cap - 1.0 / r.rate_cap_cooldown).abs() < 1e-6);
}

#[test]
fn higher_icdr_raises_trigger_rate_cap() {
    // ICDR 越高 → 冷却越短 → 上限越高（单调）。
    let low = resolve_trigger_rate(0.5, 0.0, 1.0, 100.0, SERVER_TICK_SECONDS);
    let high = resolve_trigger_rate(0.5, 0.0, 2.0, 100.0, SERVER_TICK_SECONDS);
    assert!(high.trigger_rate_cap > low.trigger_rate_cap);
}

// §二  能量驱动元宝石模型

/// 能量上限公式的基础验证：每 0.1s 施法时间 = 10 能量。
/// 出处：agent-docs/triggers.md §2.1；PoB2 other.lua `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time=10`。
#[test]
fn max_energy_formula_10_per_0_1s() {
    // 0.5s 施法时间 → (0.5/0.1)×10 = 50 能量。
    let spells = [SocketedSpellInfo::new(0.5)];
    assert!((calc_max_energy(&spells) - 50.0).abs() < 1e-6);
}

#[test]
fn max_energy_sums_over_socketed_spells() {
    // 0.3s + 0.7s = 1.0s → 100 能量。
    let spells = [SocketedSpellInfo::new(0.3), SocketedSpellInfo::new(0.7)];
    assert!((calc_max_energy(&spells) - 100.0).abs() < 1e-6);
}

/// total-use-time 修饰词在能量计算中按 2 倍处理。
/// 出处：agent-docs/triggers.md §2.1「modifiers to Total use time are treated as though
/// they were double the value」；PoE2 Wiki CoC；PoE2DB Energy。
#[test]
fn use_time_increase_is_doubled_for_energy_calc() {
    // base=0.5s, use_time_increase=10% → effective = 0.5 × (1 + 0.10 × 2) = 0.5 × 1.2 = 0.6s。
    let spell = SocketedSpellInfo::new(0.5).with_use_time_increase(10.0);
    assert!((spell.effective_cast_time_for_energy() - 0.6).abs() < 1e-9);
}

/// Freeze 的 centienergy 基数 = 1000，是 Crit/Ignite/Shock = 100 的 10 倍。
/// 出处：agent-docs/triggers.md §2.2 表格；PoB2 act_int.lua。
#[test]
fn freeze_centienergy_is_10x_crit() {
    let crit = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 1000.0, 100.0, 1.0);
    let freeze = calc_energy_per_event(TriggerCondition::Freeze, 1.0, 0.0, 1.0, 1.0);
    // crit = 1×100×(1000/100)/100 = 10；freeze = 1×1000/100 = 10。
    // 同参数下 freeze 产能与 crit（ratio=10）相同（均=10）。
    // 改为对比 ratio=1 的 crit：crit_low = 1×100×(100/100)/100=1；freeze=10 → 10x。
    let crit_low = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 100.0, 100.0, 1.0);
    assert!(
        (freeze / crit_low - 10.0).abs() < 1e-3,
        "freeze={freeze}, crit_low={crit_low}"
    );
    let _ = crit; // 抑制 unused 警告
}

/// CoC 产能随「原始伤害/异常阈值」线性增长。
/// 出处：agent-docs/triggers.md §2.2「能量获取 = Monster Power × 暴击原始伤害 / 怪物异常阈值」；
///       PoE2 Wiki Cast on Critical Strike。
#[test]
fn coc_energy_scales_with_damage_ratio() {
    // MonsterPower=1, hit_damage=1000, threshold=100 → ratio=10 → energy=1×100×10/100=10。
    let e = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 1000.0, 100.0, 1.0);
    assert!((e - 10.0).abs() < 1e-6);
}

/// 宝石等级加成（energy_generated_scale）线性放大产能，不改最大能量。
/// 出处：agent-docs/triggers.md §2.4「gem 等级只缩放 energy_generated_+%」；PoB2 act_int.lua。
#[test]
fn level_scale_multiplies_energy_per_event() {
    let base = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.0);
    let scaled = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.5);
    assert!((scaled / base - 1.5).abs() < 1e-6);
}

/// energy_trigger_rate 端到端：产能充足时受冷却上限截断。
#[test]
fn energy_trigger_rate_capped_by_cooldown() {
    // max_energy=10（0.1s × 1 spell），Freeze 产能极高（Monster Power=20）→ 产能 >> 冷却上限。
    let spells = [SocketedSpellInfo::new(0.1)];
    let r = calc_energy_trigger_rate(
        &spells,
        TriggerCondition::Freeze,
        20.0, // 高 MonsterPower
        0.0,
        1.0,
        1.0,
        50.0, // 高源速率
        0.5,  // 触发宝石冷却 0.5s → cap ≈ 2/s
        0.0,
        1.0,
        SERVER_TICK_SECONDS,
    );
    assert!(r.limited_by_cooldown, "should be limited by cooldown");
    assert!(r.effective_trigger_rate <= r.cooldown_rate_cap + 1e-6);
}

/// 无插槽法术 → max_energy=0 → effective_trigger_rate=0。
#[test]
fn energy_trigger_rate_zero_when_no_spells() {
    let r = calc_energy_trigger_rate(
        &[],
        TriggerCondition::CriticalStrike,
        5.0,
        0.0,
        1.0,
        1.0,
        3.0,
        0.3,
        0.0,
        1.0,
        SERVER_TICK_SECONDS,
    );
    assert_eq!(r.max_energy, 0.0);
    assert_eq!(r.effective_trigger_rate, 0.0);
}

/// 更高的 source_rate 带来更多能量，触发速率不减（受冷却上限截断时保持不变）。
#[test]
fn energy_trigger_rate_monotone_with_source_rate() {
    let spells = [SocketedSpellInfo::new(0.5)];
    let r_low = calc_energy_trigger_rate(
        &spells,
        TriggerCondition::Shock,
        3.0,
        0.0,
        1.0,
        1.0,
        1.0,
        0.5,
        0.0,
        1.0,
        SERVER_TICK_SECONDS,
    );
    let r_high = calc_energy_trigger_rate(
        &spells,
        TriggerCondition::Shock,
        3.0,
        0.0,
        1.0,
        1.0,
        5.0,
        0.5,
        0.0,
        1.0,
        SERVER_TICK_SECONDS,
    );
    assert!(r_high.effective_trigger_rate >= r_low.effective_trigger_rate);
}

// §三  多技能轮转

/// 单技能：无冷却瓶颈时速率近似源速率。
/// 出处：agent-docs/triggers.md §五；PoB2 calcMultiSpellRotationImpact。
#[test]
fn single_skill_rate_approaches_source_rate() {
    // 0.1s 冷却 < 0.25s 触发间隔（4/s）→ 每次机会都能触发，速率≈4/s。
    let skill = RotationSkill::new(0.1);
    let result = calc_multi_spell_rotation(&[skill], 4.0, SERVER_TICK_SECONDS);
    assert!(!result.rates.is_empty());
    // 速率应接近 source_rate（不超过）。
    assert!(result.rates[0] > 3.0);
    assert!(result.rates[0] <= 4.0 + 1e-6);
    assert_eq!(result.wasted_fraction, 0.0);
}

/// 两技能共享触发机会，各自速率之和 ≤ 源速率。
#[test]
fn two_skills_total_rate_bounded_by_source() {
    let a = RotationSkill::new(0.5);
    let b = RotationSkill::new(0.5);
    let source_rate = 4.0;
    let result = calc_multi_spell_rotation(&[a, b], source_rate, SERVER_TICK_SECONDS);
    let total: f64 = result.rates.iter().sum();
    assert!(
        total <= source_rate + 1e-6,
        "total={total} source={source_rate}"
    );
    assert!(result.rates[0] > 0.0);
    assert!(result.rates[1] > 0.0);
}

/// 长冷却技能导致大量触发机会浪费。
#[test]
fn long_cooldown_causes_high_waste_fraction() {
    let skills: Vec<RotationSkill> = (0..3).map(|_| RotationSkill::new(8.0)).collect();
    let result = calc_multi_spell_rotation(&skills, 8.0, SERVER_TICK_SECONDS);
    assert!(
        result.wasted_fraction > 0.4,
        "wasted={}",
        result.wasted_fraction
    );
}

/// 触发几率 < 1 时稳态速率低于 chance=1 的情况。
/// 出处：agent-docs/triggers.md §五「触发几率用几何分布期望值折算」。
#[test]
fn lower_trigger_chance_reduces_effective_rate() {
    let full = RotationSkill::new(0.3).with_trigger_chance(1.0);
    let half = RotationSkill::new(0.3).with_trigger_chance(0.5);
    let r_full = calc_multi_spell_rotation(&[full], 4.0, SERVER_TICK_SECONDS);
    let r_half = calc_multi_spell_rotation(&[half], 4.0, SERVER_TICK_SECONDS);
    assert!(
        r_half.rates[0] < r_full.rates[0],
        "half={} full={}",
        r_half.rates[0],
        r_full.rates[0]
    );
}

/// added_cooldown（SpellCastTimeAddedToCooldownIfTriggered）减慢轮转速率。
/// 出处：agent-docs/triggers.md §4.3；PoB2 addsCastTime。
#[test]
fn added_cooldown_slows_rotation_rate() {
    let no_add = RotationSkill::new(0.3).with_added_cooldown(0.0);
    let with_add = RotationSkill::new(0.3).with_added_cooldown(1.0);
    let r_no = calc_multi_spell_rotation(&[no_add], 5.0, SERVER_TICK_SECONDS);
    let r_with = calc_multi_spell_rotation(&[with_add], 5.0, SERVER_TICK_SECONDS);
    assert!(r_with.rates[0] <= r_no.rates[0] + 1e-6);
}

/// 空轮转 / 零源速率应返回空结果，不 panic。
#[test]
fn rotation_edge_cases_no_panic() {
    let empty = calc_multi_spell_rotation(&[], 5.0, SERVER_TICK_SECONDS);
    assert!(empty.rates.is_empty());
    let zero = calc_multi_spell_rotation(&[RotationSkill::new(0.3)], 0.0, SERVER_TICK_SECONDS);
    assert!(zero.rates.is_empty() || zero.rates.iter().all(|&r| r == 0.0));
}

/// 更多技能插槽稀释每个技能的触发速率（单技能 vs 两技能）。
#[test]
fn more_skills_dilute_individual_rates() {
    let skills_1 = [RotationSkill::new(0.5)];
    let skills_2 = [RotationSkill::new(0.5), RotationSkill::new(0.5)];
    let source_rate = 4.0;
    let r1 = calc_multi_spell_rotation(&skills_1, source_rate, SERVER_TICK_SECONDS);
    let r2 = calc_multi_spell_rotation(&skills_2, source_rate, SERVER_TICK_SECONDS);
    // 单技能速率 ≥ 双技能中任意一个的速率（资源稀释）。
    assert!(r1.rates[0] >= r2.rates[0] - 1e-6);
}

// §四  CWC（Cast While Channelling）

/// CWC 基础：triggerTime 取整到服务器帧，基准频率正确。
/// 出处：agent-docs/triggers.md §4.2；PoB2 CWCHandler `adjTriggerInterval`。
#[test]
fn cwc_adjusted_interval_is_frame_aligned() {
    let tick_rate = server_tick_rate(SERVER_TICK_SECONDS);
    let r = calc_cwc_trigger_rate(0.25, 0.0, 0.0, 1.0, SERVER_TICK_SECONDS);
    let expected_interval = round_cooldown_to_tick(0.25, tick_rate);
    assert!((r.adjusted_trigger_interval - expected_interval).abs() < 1e-9);
    assert!((r.channelling_trigger_rate - 1.0 / expected_interval).abs() < 1e-6);
}

/// 被触发技能冷却超过引导间隔时成为速率瓶颈。
/// 出处：agent-docs/triggers.md §4.2 `TriggerRateCap = min(1/effCDTriggeredSkill, triggerRateOfTrigger)`。
#[test]
fn cwc_triggered_cd_limits_rate_when_long() {
    let r = calc_cwc_trigger_rate(0.1, 0.8, 0.0, 1.0, SERVER_TICK_SECONDS);
    assert!(
        r.limited_by_triggered_cd,
        "triggered_cd=0.8 > triggerTime=0.1 → should limit"
    );
    assert!(r.trigger_rate_cap < r.channelling_trigger_rate);
}

/// ICDR 提高 CWC 触发速率上限（缩短被触发技能有效冷却）。
#[test]
fn cwc_icdr_reduces_effective_triggered_cd() {
    let r_no_icdr = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 1.0, SERVER_TICK_SECONDS);
    let r_icdr2 = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 2.0, SERVER_TICK_SECONDS);
    // ICDR=2 → effective_cd = 0.6/2 = 0.3 < 0.6。
    assert!(r_icdr2.effective_triggered_cd < r_no_icdr.effective_triggered_cd);
    assert!(r_icdr2.trigger_rate_cap >= r_no_icdr.trigger_rate_cap);
}

/// SpellCastTimeAddedToCooldownIfTriggered：施法时间追加到冷却。
/// 出处：agent-docs/triggers.md §4.3；PoB2 CalcTriggers.lua `processAddedCastTime`。
#[test]
fn cwc_adds_cast_time_raises_effective_cd() {
    // triggered_cd=0.2, adds_cast_time=0.5 → max(0.2, 0.5)=0.5 → effective_cd=0.5/icdr=0.5。
    let r = calc_cwc_trigger_rate(0.1, 0.2, 0.5, 1.0, SERVER_TICK_SECONDS);
    assert!(
        (r.effective_triggered_cd - 0.5).abs() < 1e-6,
        "effective_cd={}",
        r.effective_triggered_cd
    );
    assert!(r.limited_by_triggered_cd);
}

/// spell_cast_time_added_to_cooldown：基础施法时间 / 施法速度乘数。
#[test]
fn spell_cast_time_to_cooldown_divides_by_speed() {
    // base=0.6s, speed=1.5 → 0.6/1.5 = 0.4s。
    let added = spell_cast_time_added_to_cooldown(0.6, 1.5);
    assert!((added - 0.4).abs() < 1e-9);
}

/// 无施法速度加成时追加冷却 = 基础施法时间。
#[test]
fn spell_cast_time_to_cooldown_no_bonus() {
    let added = spell_cast_time_added_to_cooldown(0.8, 1.0);
    assert!((added - 0.8).abs() < 1e-9);
}

// §五  归因 TraceGraph

/// 冷却驱动归因：结果节点值与无 trace 版本一致，且有足够输入节点。
/// 出处：agent-docs/triggers.md §对 pobr 实现的启示 #5。
#[test]
fn trace_cooldown_trigger_result_matches_non_traced() {
    let expected = resolve_trigger_rate(0.3, 0.5, 1.5, 4.0, SERVER_TICK_SECONDS);
    let mut trace = TraceGraph::new();
    let (result, rate_node) =
        resolve_trigger_rate_traced(0.3, 0.5, 1.5, 4.0, SERVER_TICK_SECONDS, &mut trace);
    // traced 版本与非 traced 版本数值一致。
    assert!((result.skill_trigger_rate - expected.skill_trigger_rate).abs() < 1e-9);
    let node = trace.node(rate_node).unwrap();
    assert!((node.value - result.skill_trigger_rate).abs() < 1e-9);
    assert!(trace.nodes().len() >= 5);
    let incoming = trace.incoming(rate_node);
    assert!(
        incoming.len() >= 2,
        "rate_node should have >=2 inputs (cap, sourceRate)"
    );
}

/// 能量驱动归因：结果节点值一致，TraceGraph 有节点。
#[test]
fn trace_energy_trigger_result_matches_non_traced() {
    let spells = [SocketedSpellInfo::new(0.5)];
    let mut trace = TraceGraph::new();
    let (result, node) = calc_energy_trigger_rate_traced(
        &spells,
        TriggerCondition::Shock,
        5.0,
        0.0,
        1.0,
        1.0,
        3.0,
        0.3,
        0.0,
        1.0,
        SERVER_TICK_SECONDS,
        &mut trace,
    );
    let n = trace.node(node).unwrap();
    assert!((n.value - result.effective_trigger_rate).abs() < 1e-9);
    assert!(trace.nodes().len() >= 4);
}

/// CWC 归因：结果节点值一致，有足够节点与输入边。
#[test]
fn trace_cwc_trigger_result_matches_non_traced() {
    let mut trace = TraceGraph::new();
    let (result, node) =
        calc_cwc_trigger_rate_traced(0.3, 0.5, 0.2, 1.5, SERVER_TICK_SECONDS, &mut trace);
    let n = trace.node(node).unwrap();
    assert!((n.value - result.trigger_rate_cap).abs() < 1e-9);
    assert!(trace.nodes().len() >= 4);
    let incoming = trace.incoming(node);
    assert!(incoming.len() >= 2);
}

/// 触发速率 source_ancestors 能回溯到 SourceId 来源。
#[test]
fn trace_source_ancestors_reachable_from_rate_node() {
    let mut trace = TraceGraph::new();
    let (_result, rate_node) =
        resolve_trigger_rate_traced(0.3, 0.0, 1.0, 4.0, SERVER_TICK_SECONDS, &mut trace);
    let ancestors = trace.source_ancestors(rate_node);
    // 至少有 trigger_cd、source_rate 两个 SourceId 节点。
    assert!(
        ancestors.len() >= 2,
        "expected >=2 source ancestors, got {}",
        ancestors.len()
    );
}
