//! Trigger domain integration tests:
//! - §1  cooldown-driven triggers (rate cap + frame throttling + ICDR + dual gating)
//! - §2  energy-driven support-gem model (max_energy / energy_per_event / effective_trigger_rate)
//! - §3  multi-skill rotation (ported from calcMultiSpellRotationImpact)
//! - §4  CWC (Cast While Channelling)
//! - §5  TraceGraph attribution
//!
//! Source: agent-docs/triggers.md §2-§5; PoB2 CalcTriggers.lua / Data.lua / act_int.lua.

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

// §1  cooldown-driven triggers, basics (regression: kept strictly identical to the
// pre-existing tests)

#[test]
fn server_tick_rate_is_inverse_of_constant() {
    assert!((server_tick_rate(SERVER_TICK_SECONDS) - 1.0 / SERVER_TICK_SECONDS).abs() < 1e-12);
    // ~= 30.30/s.
    assert!((server_tick_rate(SERVER_TICK_SECONDS) - 30.303_030_303_03).abs() < 1e-6);
}

#[test]
fn cooldown_is_rounded_up_to_server_frame() {
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    // 0.2s cooldown: ceil(0.2 x 30.303) = ceil(6.06) = 7 frames -> 7/30.303 ~= 0.231s.
    let rounded = round_cooldown_to_tick(0.2, rate);
    assert!((rounded - 7.0 / rate).abs() < 1e-9);
    assert!(rounded >= 0.2);
}

#[test]
fn trigger_rate_cap_formula_matches_doc() {
    // cap = 1 / (ceil(cd x rate) / rate), matching agent-docs/triggers.md §3.1.
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
    // ICDR=2.0 shortens the trigger gem's 0.4s cooldown to 0.2s (the triggered skill
    // itself has no cooldown).
    let cd = action_cooldown(0.4, 0.0, 2.0);
    assert!((cd - 0.2).abs() < 1e-9);
}

#[test]
fn action_cooldown_takes_larger_of_two() {
    // max(triggeredCD=0.6, triggerCD/icdr=0.4/2=0.2) = 0.6.
    let cd = action_cooldown(0.4, 0.6, 2.0);
    assert!((cd - 0.6).abs() < 1e-9);
}

#[test]
fn zero_icdr_falls_back_to_raw_trigger_cd() {
    // icdr<=0 skips the division (avoids divide-by-zero) and uses trigger_cd as-is.
    let cd = action_cooldown(0.4, 0.0, 0.0);
    assert!((cd - 0.4).abs() < 1e-9);
}

#[test]
fn skill_trigger_rate_is_min_of_cap_and_source() {
    // Dual gating: source rate 1.5/s is below the cap -> the effective rate is
    // gated by the source.
    let r = resolve_trigger_rate(0.15, 0.0, 1.0, 1.5, SERVER_TICK_SECONDS);
    assert!(r.limited_by_source);
    assert!((r.skill_trigger_rate - 1.5).abs() < 1e-6);
    assert!(r.trigger_rate_cap > 1.5);
}

#[test]
fn fast_source_is_capped_by_rate_cap() {
    // Source rate 50/s exceeds the cap -> the effective rate equals the cap.
    let r = resolve_trigger_rate(0.3, 0.0, 1.0, 50.0, SERVER_TICK_SECONDS);
    assert!(!r.limited_by_source);
    assert!((r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9);
}

#[test]
fn rate_cap_cooldown_is_frame_aligned() {
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    let r = resolve_trigger_rate(0.25, 0.0, 1.0, 100.0, SERVER_TICK_SECONDS);
    // rate_cap_cooldown should be frame-aligned, and cap = 1/it.
    assert!((r.rate_cap_cooldown - round_cooldown_to_tick(0.25, rate)).abs() < 1e-9);
    assert!((r.trigger_rate_cap - 1.0 / r.rate_cap_cooldown).abs() < 1e-6);
}

#[test]
fn higher_icdr_raises_trigger_rate_cap() {
    // Higher ICDR -> shorter cooldown -> higher cap (monotone).
    let low = resolve_trigger_rate(0.5, 0.0, 1.0, 100.0, SERVER_TICK_SECONDS);
    let high = resolve_trigger_rate(0.5, 0.0, 2.0, 100.0, SERVER_TICK_SECONDS);
    assert!(high.trigger_rate_cap > low.trigger_rate_cap);
}

// §2  energy-driven support-gem model

/// Baseline check of the max-energy formula: every 0.1s of cast time = 10 energy.
/// Source: agent-docs/triggers.md §2.1; PoB2 other.lua `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time=10`.
#[test]
fn max_energy_formula_10_per_0_1s() {
    // 0.5s cast time -> (0.5/0.1)x10 = 50 energy.
    let spells = [SocketedSpellInfo::new(0.5)];
    assert!((calc_max_energy(&spells) - 50.0).abs() < 1e-6);
}

#[test]
fn max_energy_sums_over_socketed_spells() {
    // 0.3s + 0.7s = 1.0s -> 100 energy.
    let spells = [SocketedSpellInfo::new(0.3), SocketedSpellInfo::new(0.7)];
    assert!((calc_max_energy(&spells) - 100.0).abs() < 1e-6);
}

/// Total-use-time modifiers count double in the energy calculation.
/// Source: agent-docs/triggers.md §2.1 "modifiers to Total use time are treated as though
/// they were double the value"; PoE2 Wiki CoC; PoE2DB Energy.
#[test]
fn use_time_increase_is_doubled_for_energy_calc() {
    // base=0.5s, use_time_increase=10% -> effective = 0.5 x (1 + 0.10 x 2) = 0.5 x 1.2 = 0.6s.
    let spell = SocketedSpellInfo::new(0.5).with_use_time_increase(10.0);
    assert!((spell.effective_cast_time_for_energy() - 0.6).abs() < 1e-9);
}

/// Freeze's centienergy base = 1000, 10x that of Crit/Ignite/Shock's 100.
/// Source: agent-docs/triggers.md §2.2 table; PoB2 act_int.lua.
#[test]
fn freeze_centienergy_is_10x_crit() {
    let crit = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 1000.0, 100.0, 1.0);
    let freeze = calc_energy_per_event(TriggerCondition::Freeze, 1.0, 0.0, 1.0, 1.0);
    // crit = 1x100x(1000/100)/100 = 10; freeze = 1x1000/100 = 10.
    // With these particular params, freeze's energy happens to match crit's
    // (ratio=10, both = 10). Compare against a ratio=1 crit instead:
    // crit_low = 1x100x(100/100)/100 = 1; freeze=10 -> 10x.
    let crit_low = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 100.0, 100.0, 1.0);
    assert!(
        (freeze / crit_low - 10.0).abs() < 1e-3,
        "freeze={freeze}, crit_low={crit_low}"
    );
    let _ = crit; // suppress unused warning
}

/// CoC's energy gain scales linearly with raw-damage / ailment-threshold.
/// Source: agent-docs/triggers.md §2.2 "energy gain = Monster Power x crit raw damage /
///       monster ailment threshold"; PoE2 Wiki Cast on Critical Strike.
#[test]
fn coc_energy_scales_with_damage_ratio() {
    // MonsterPower=1, hit_damage=1000, threshold=100 -> ratio=10 -> energy=1x100x10/100=10.
    let e = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 1000.0, 100.0, 1.0);
    assert!((e - 10.0).abs() < 1e-6);
}

/// The gem-level bonus (energy_generated_scale) scales energy gain linearly and
/// doesn't touch max energy.
/// Source: agent-docs/triggers.md §2.4 "gem level only scales energy_generated_+%";
/// PoB2 act_int.lua.
#[test]
fn level_scale_multiplies_energy_per_event() {
    let base = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.0);
    let scaled = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.5);
    assert!((scaled / base - 1.5).abs() < 1e-6);
}

/// energy_trigger_rate end-to-end: when energy gain is plentiful, the cooldown cap
/// becomes the binding constraint.
#[test]
fn energy_trigger_rate_capped_by_cooldown() {
    // max_energy=10 (0.1s x 1 spell), Freeze's energy gain is huge (Monster Power=20)
    // -> energy gain >> the cooldown cap.
    let spells = [SocketedSpellInfo::new(0.1)];
    let r = calc_energy_trigger_rate(
        &spells,
        TriggerCondition::Freeze,
        20.0, // high MonsterPower
        0.0,
        1.0,
        1.0,
        50.0, // high source rate
        0.5,  // trigger gem cooldown 0.5s -> cap ~= 2/s
        0.0,
        1.0,
        SERVER_TICK_SECONDS,
    );
    assert!(r.limited_by_cooldown, "should be limited by cooldown");
    assert!(r.effective_trigger_rate <= r.cooldown_rate_cap + 1e-6);
}

/// No socketed spells -> max_energy=0 -> effective_trigger_rate=0.
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

/// A higher source_rate never decreases the trigger rate (it holds steady once
/// capped by the cooldown).
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

// §3  multi-skill rotation

/// Single skill: with no cooldown bottleneck, the rate approaches the source rate.
/// Source: agent-docs/triggers.md §5; PoB2 calcMultiSpellRotationImpact.
#[test]
fn single_skill_rate_approaches_source_rate() {
    // 0.1s cooldown < 0.25s trigger interval (4/s) -> every opportunity can trigger,
    // rate ~= 4/s.
    let skill = RotationSkill::new(0.1);
    let result = calc_multi_spell_rotation(&[skill], 4.0, SERVER_TICK_SECONDS);
    assert!(!result.rates.is_empty());
    // The rate should approach source_rate (never exceed it).
    assert!(result.rates[0] > 3.0);
    assert!(result.rates[0] <= 4.0 + 1e-6);
    assert_eq!(result.wasted_fraction, 0.0);
}

/// Two skills share the same trigger opportunities, so their rates sum to at most
/// the source rate.
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

/// A long-cooldown skill wastes a large share of trigger opportunities.
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

/// A trigger chance below 1 yields a lower steady-state rate than chance=1.
/// Source: agent-docs/triggers.md §5 "trigger chance is folded in via the expected
/// value of a geometric distribution".
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

/// added_cooldown (SpellCastTimeAddedToCooldownIfTriggered) slows down the rotation rate.
/// Source: agent-docs/triggers.md §4.3; PoB2 addsCastTime.
#[test]
fn added_cooldown_slows_rotation_rate() {
    let no_add = RotationSkill::new(0.3).with_added_cooldown(0.0);
    let with_add = RotationSkill::new(0.3).with_added_cooldown(1.0);
    let r_no = calc_multi_spell_rotation(&[no_add], 5.0, SERVER_TICK_SECONDS);
    let r_with = calc_multi_spell_rotation(&[with_add], 5.0, SERVER_TICK_SECONDS);
    assert!(r_with.rates[0] <= r_no.rates[0] + 1e-6);
}

/// An empty rotation / zero source rate should return an empty result, not panic.
#[test]
fn rotation_edge_cases_no_panic() {
    let empty = calc_multi_spell_rotation(&[], 5.0, SERVER_TICK_SECONDS);
    assert!(empty.rates.is_empty());
    let zero = calc_multi_spell_rotation(&[RotationSkill::new(0.3)], 0.0, SERVER_TICK_SECONDS);
    assert!(zero.rates.is_empty() || zero.rates.iter().all(|&r| r == 0.0));
}

/// More skill slots dilute each skill's trigger rate (single skill vs. two skills).
#[test]
fn more_skills_dilute_individual_rates() {
    let skills_1 = [RotationSkill::new(0.5)];
    let skills_2 = [RotationSkill::new(0.5), RotationSkill::new(0.5)];
    let source_rate = 4.0;
    let r1 = calc_multi_spell_rotation(&skills_1, source_rate, SERVER_TICK_SECONDS);
    let r2 = calc_multi_spell_rotation(&skills_2, source_rate, SERVER_TICK_SECONDS);
    // The single-skill rate should be >= either of the two-skill rates (resource dilution).
    assert!(r1.rates[0] >= r2.rates[0] - 1e-6);
}

// §4  CWC (Cast While Channelling)

/// CWC basics: triggerTime rounds up to the server frame, and the base frequency is correct.
/// Source: agent-docs/triggers.md §4.2; PoB2 CWCHandler `adjTriggerInterval`.
#[test]
fn cwc_adjusted_interval_is_frame_aligned() {
    let tick_rate = server_tick_rate(SERVER_TICK_SECONDS);
    let r = calc_cwc_trigger_rate(0.25, 0.0, 0.0, 1.0, SERVER_TICK_SECONDS);
    let expected_interval = round_cooldown_to_tick(0.25, tick_rate);
    assert!((r.adjusted_trigger_interval - expected_interval).abs() < 1e-9);
    assert!((r.channelling_trigger_rate - 1.0 / expected_interval).abs() < 1e-6);
}

/// When the triggered skill's cooldown exceeds the channelling interval, it becomes
/// the rate bottleneck.
/// Source: agent-docs/triggers.md §4.2 `TriggerRateCap = min(1/effCDTriggeredSkill, triggerRateOfTrigger)`.
#[test]
fn cwc_triggered_cd_limits_rate_when_long() {
    let r = calc_cwc_trigger_rate(0.1, 0.8, 0.0, 1.0, SERVER_TICK_SECONDS);
    assert!(
        r.limited_by_triggered_cd,
        "triggered_cd=0.8 > triggerTime=0.1 → should limit"
    );
    assert!(r.trigger_rate_cap < r.channelling_trigger_rate);
}

/// ICDR raises the CWC trigger-rate cap (by shortening the triggered skill's
/// effective cooldown).
#[test]
fn cwc_icdr_reduces_effective_triggered_cd() {
    let r_no_icdr = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 1.0, SERVER_TICK_SECONDS);
    let r_icdr2 = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 2.0, SERVER_TICK_SECONDS);
    // ICDR=2 -> effective_cd = 0.6/2 = 0.3 < 0.6.
    assert!(r_icdr2.effective_triggered_cd < r_no_icdr.effective_triggered_cd);
    assert!(r_icdr2.trigger_rate_cap >= r_no_icdr.trigger_rate_cap);
}

/// SpellCastTimeAddedToCooldownIfTriggered: cast time is appended to the cooldown.
/// Source: agent-docs/triggers.md §4.3; PoB2 CalcTriggers.lua `processAddedCastTime`.
#[test]
fn cwc_adds_cast_time_raises_effective_cd() {
    // triggered_cd=0.2, adds_cast_time=0.5 -> max(0.2, 0.5)=0.5 -> effective_cd=0.5/icdr=0.5.
    let r = calc_cwc_trigger_rate(0.1, 0.2, 0.5, 1.0, SERVER_TICK_SECONDS);
    assert!(
        (r.effective_triggered_cd - 0.5).abs() < 1e-6,
        "effective_cd={}",
        r.effective_triggered_cd
    );
    assert!(r.limited_by_triggered_cd);
}

/// spell_cast_time_added_to_cooldown: base cast time / cast speed multiplier.
#[test]
fn spell_cast_time_to_cooldown_divides_by_speed() {
    // base=0.6s, speed=1.5 -> 0.6/1.5 = 0.4s.
    let added = spell_cast_time_added_to_cooldown(0.6, 1.5);
    assert!((added - 0.4).abs() < 1e-9);
}

/// With no cast-speed bonus, the added cooldown equals the base cast time.
#[test]
fn spell_cast_time_to_cooldown_no_bonus() {
    let added = spell_cast_time_added_to_cooldown(0.8, 1.0);
    assert!((added - 0.8).abs() < 1e-9);
}

// §5  TraceGraph attribution

/// Cooldown-driven attribution: the result node's value matches the non-traced
/// version, with enough input nodes recorded.
/// Source: agent-docs/triggers.md §"Implications for the pobr implementation" #5.
#[test]
fn trace_cooldown_trigger_result_matches_non_traced() {
    let expected = resolve_trigger_rate(0.3, 0.5, 1.5, 4.0, SERVER_TICK_SECONDS);
    let mut trace = TraceGraph::new();
    let (result, rate_node) =
        resolve_trigger_rate_traced(0.3, 0.5, 1.5, 4.0, SERVER_TICK_SECONDS, &mut trace);
    // The traced version's value matches the non-traced version's.
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

/// Energy-driven attribution: the result node's value matches, and the TraceGraph
/// has nodes.
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

/// CWC attribution: the result node's value matches, with enough nodes and input edges.
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

/// The trigger rate's source_ancestors can be traced back to SourceId sources.
#[test]
fn trace_source_ancestors_reachable_from_rate_node() {
    let mut trace = TraceGraph::new();
    let (_result, rate_node) =
        resolve_trigger_rate_traced(0.3, 0.0, 1.0, 4.0, SERVER_TICK_SECONDS, &mut trace);
    let ancestors = trace.source_ancestors(rate_node);
    // At least the two SourceId nodes trigger_cd and source_rate.
    assert!(
        ancestors.len() >= 2,
        "expected >=2 source ancestors, got {}",
        ancestors.len()
    );
}
