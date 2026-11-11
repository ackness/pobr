//! Trigger domain integration tests:
//! - §1  cooldown-driven triggers (rate cap + frame throttling + ICDR + dual gating)
//! - §3  multi-skill rotation (ported from calcMultiSpellRotationImpact)
//! - §4  CWC (Cast While Channelling)
//! - §5  TraceGraph attribution
//!
//! The Energy of a meta gem is `calc::energy`, tested there and in
//! `pobr-build`'s `skills::energy_triggers`.
//!
//! Source: agent-docs/triggers.md §3-§5; PoB2 CalcTriggers.lua / Data.lua.

use pobr_core::TraceGraph;
use pobr_core::calc::trigger::{
    RotationSkill, action_cooldown, calc_cwc_trigger_rate, calc_multi_spell_rotation,
    resolve_trigger_rate, round_cooldown_to_tick, server_tick_rate,
    spell_cast_time_added_to_cooldown, trigger_rate_cap,
};
use pobr_core::calc::{calc_cwc_trigger_rate_traced, resolve_trigger_rate_traced};
use pobr_data::prelude::SERVER_TICK_SECONDS;

// §1  Cooldown-driven trigger helpers, including the former internal cases.

#[test]
fn server_tick_rate_is_inverse_of_constant() {
    assert!((server_tick_rate(SERVER_TICK_SECONDS) - 1.0 / SERVER_TICK_SECONDS).abs() < 1e-12);
    // ~= 30.30/s.
    assert!((server_tick_rate(SERVER_TICK_SECONDS) - 30.303_030_303_03).abs() < 1e-6);
}

#[test]
fn cooldown_is_rounded_up_to_server_frame() {
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    for (cooldown, frames) in [(0.10, 4.0), (0.20, 7.0)] {
        let rounded = round_cooldown_to_tick(cooldown, rate);
        assert!(
            (rounded - frames / rate).abs() < 1e-9,
            "cooldown={cooldown}"
        );
        assert!(rounded > cooldown, "cooldown={cooldown}");
    }
}

#[test]
fn trigger_rate_cap_formula_matches_doc() {
    let rate = server_tick_rate(SERVER_TICK_SECONDS);
    for cooldown in [0.15, 0.3] {
        let cap = trigger_rate_cap(cooldown, rate);
        let expected = 1.0 / round_cooldown_to_tick(cooldown, rate);
        assert!((cap - expected).abs() < 1e-6, "cooldown={cooldown}");
    }
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
fn action_cooldown_combines_trigger_skill_and_recovery() {
    for (case, trigger_cd, triggered_cd, icdr, expected) in [
        ("recovery", 0.3, 0.0, 1.5, 0.2),
        ("double recovery", 0.4, 0.0, 2.0, 0.2),
        ("skill cooldown", 0.3, 0.5, 1.0, 0.5),
        ("skill beats recovered trigger", 0.4, 0.6, 2.0, 0.6),
        ("zero recovery fallback", 0.4, 0.0, 0.0, 0.4),
    ] {
        let actual = action_cooldown(trigger_cd, triggered_cd, icdr);
        assert!((actual - expected).abs() < 1e-9, "{case}: {actual}");
    }
}

#[test]
fn skill_trigger_rate_is_min_of_cap_and_source() {
    for (cooldown, source_rate) in [(0.05, 2.0), (0.15, 1.5)] {
        let r = resolve_trigger_rate(cooldown, 0.0, 1.0, source_rate, SERVER_TICK_SECONDS);
        assert!(r.limited_by_source, "cooldown={cooldown}");
        assert!(
            (r.skill_trigger_rate - source_rate).abs() < 1e-6,
            "cooldown={cooldown}"
        );
        assert!(r.trigger_rate_cap > source_rate, "cooldown={cooldown}");
    }
}

#[test]
fn fast_source_is_capped_by_rate_cap() {
    for source_rate in [50.0, 100.0] {
        let r = resolve_trigger_rate(0.3, 0.0, 1.0, source_rate, SERVER_TICK_SECONDS);
        assert!(!r.limited_by_source, "source={source_rate}");
        assert!(
            (r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9,
            "source={source_rate}"
        );
    }
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

// §3  multi-skill rotation

/// Without a cooldown bottleneck, a single skill uses every opportunity.
#[test]
fn single_skill_rate_approaches_source_rate() {
    for (cooldown, minimum_rate) in [(0.10, 3.0), (0.15, 0.0)] {
        let result =
            calc_multi_spell_rotation(&[RotationSkill::new(cooldown)], 4.0, SERVER_TICK_SECONDS);
        assert_eq!(result.rates.len(), 1, "cooldown={cooldown}");
        assert!(result.rates[0] > minimum_rate, "cooldown={cooldown}");
        assert!(result.rates[0] <= 4.0 + 1e-6, "cooldown={cooldown}");
        assert_eq!(result.wasted_fraction, 0.0, "cooldown={cooldown}");
    }
}

/// Two skills share the same trigger opportunities, so their rates sum to at most
/// the source rate.
#[test]
fn two_skills_total_rate_bounded_by_source() {
    let a = RotationSkill::new(0.5);
    let b = RotationSkill::new(0.5);
    let source_rate = 4.0;
    let result = calc_multi_spell_rotation(&[a, b], source_rate, SERVER_TICK_SECONDS);
    assert_eq!(result.rates.len(), 2);
    let total: f64 = result.rates.iter().sum();
    assert!(
        total <= source_rate + 1e-6,
        "total={total} source={source_rate}"
    );
    assert!(result.rates[0] > 0.0);
    assert!(result.rates[1] > 0.0);
}

/// Long cooldowns waste a large share of trigger opportunities.
#[test]
fn long_cooldown_causes_high_waste_fraction() {
    for (cooldown, source_rate, minimum_waste) in [(8.0, 8.0, 0.4), (10.0, 10.0, 0.5)] {
        let skills: Vec<RotationSkill> = (0..3).map(|_| RotationSkill::new(cooldown)).collect();
        let result = calc_multi_spell_rotation(&skills, source_rate, SERVER_TICK_SECONDS);
        assert!(
            result.wasted_fraction > minimum_waste,
            "cooldown={cooldown}: wasted={}",
            result.wasted_fraction
        );
    }
}

/// A trigger chance below 1 reduces the steady-state rate.
#[test]
fn lower_trigger_chance_reduces_effective_rate() {
    for source_rate in [3.0, 4.0] {
        let full = RotationSkill::new(0.3).with_trigger_chance(1.0);
        let half = RotationSkill::new(0.3).with_trigger_chance(0.5);
        let r_full = calc_multi_spell_rotation(&[full], source_rate, SERVER_TICK_SECONDS);
        let r_half = calc_multi_spell_rotation(&[half], source_rate, SERVER_TICK_SECONDS);
        assert!(
            r_half.rates[0] < r_full.rates[0],
            "source={source_rate}: half={} full={}",
            r_half.rates[0],
            r_full.rates[0]
        );
    }
}

/// SpellCastTimeAddedToCooldownIfTriggered slows the rotation.
#[test]
fn added_cooldown_slows_rotation_rate() {
    for (added_cooldown, tolerance) in [(0.5, 1e-9), (1.0, 1e-6)] {
        let no_add = RotationSkill::new(0.3).with_added_cooldown(0.0);
        let with_add = RotationSkill::new(0.3).with_added_cooldown(added_cooldown);
        let r_no = calc_multi_spell_rotation(&[no_add], 5.0, SERVER_TICK_SECONDS);
        let r_with = calc_multi_spell_rotation(&[with_add], 5.0, SERVER_TICK_SECONDS);
        assert!(
            r_with.rates[0] <= r_no.rates[0] + tolerance,
            "added={added_cooldown}"
        );
    }
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

/// CWC's interval rounds up to the server frame, determining its frequency.
#[test]
fn cwc_adjusted_interval_is_frame_aligned() {
    let tick_rate = server_tick_rate(SERVER_TICK_SECONDS);
    for interval in [0.25, 0.3] {
        let r = calc_cwc_trigger_rate(interval, 0.0, 0.0, 1.0, SERVER_TICK_SECONDS);
        let expected_interval = round_cooldown_to_tick(interval, tick_rate);
        assert!(
            (r.adjusted_trigger_interval - expected_interval).abs() < 1e-9,
            "interval={interval}"
        );
        assert!(
            (r.channelling_trigger_rate - 1.0 / expected_interval).abs() < 1e-6,
            "interval={interval}"
        );
        assert!(!r.limited_by_triggered_cd, "interval={interval}");
    }
}

/// A triggered skill cooldown longer than the channel interval limits the rate.
#[test]
fn cwc_triggered_cd_limits_rate_when_long() {
    for cooldown in [0.8, 1.0] {
        let r = calc_cwc_trigger_rate(0.1, cooldown, 0.0, 1.0, SERVER_TICK_SECONDS);
        assert!(r.limited_by_triggered_cd, "cooldown={cooldown}");
        assert!(
            r.trigger_rate_cap < r.channelling_trigger_rate,
            "cooldown={cooldown}"
        );
    }
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

/// Added cast time contributes to the effective triggered cooldown.
#[test]
fn cwc_adds_cast_time_raises_effective_cd() {
    for interval in [0.1, 0.2] {
        let r = calc_cwc_trigger_rate(interval, 0.2, 0.5, 1.0, SERVER_TICK_SECONDS);
        assert!(
            (r.effective_triggered_cd - 0.5).abs() < 1e-6,
            "interval={interval}: effective_cd={}",
            r.effective_triggered_cd
        );
        assert!(r.limited_by_triggered_cd, "interval={interval}");
    }
}

/// Added cooldown divides the base cast time by the cast speed multiplier.
#[test]
fn spell_cast_time_to_cooldown_divides_by_speed() {
    for (base, speed, expected, tolerance) in [
        (0.5, 1.5, 0.5 / 1.5, 1e-6),
        (0.6, 1.5, 0.4, 1e-9),
        (0.8, 1.0, 0.8, 1e-9),
    ] {
        let actual = spell_cast_time_added_to_cooldown(base, speed);
        assert!(
            (actual - expected).abs() < tolerance,
            "base={base}, speed={speed}: {actual}"
        );
    }
}

// §5  TraceGraph attribution

/// Tracing preserves the result and records the rate's inputs.
#[test]
fn trace_cooldown_trigger_result_matches_non_traced() {
    for (triggered_cd, icdr, source_rate) in [(0.0, 1.0, 5.0), (0.5, 1.5, 4.0)] {
        let expected =
            resolve_trigger_rate(0.3, triggered_cd, icdr, source_rate, SERVER_TICK_SECONDS);
        let mut trace = TraceGraph::new();
        let (result, rate_node) = resolve_trigger_rate_traced(
            0.3,
            triggered_cd,
            icdr,
            source_rate,
            SERVER_TICK_SECONDS,
            &mut trace,
        );
        assert!((result.skill_trigger_rate - expected.skill_trigger_rate).abs() < 1e-9);
        let node = trace.node(rate_node).unwrap();
        assert!((node.value - result.skill_trigger_rate).abs() < 1e-9);
        assert!(trace.nodes().len() >= 5);
        assert!(trace.incoming(rate_node).len() >= 2);
    }
}

/// CWC tracing records the rate cap and its input edges.
#[test]
fn trace_cwc_trigger_result_matches_non_traced() {
    for (added_cast_time, icdr) in [(0.0, 1.0), (0.2, 1.5)] {
        let mut trace = TraceGraph::new();
        let (result, node) = calc_cwc_trigger_rate_traced(
            0.3,
            0.5,
            added_cast_time,
            icdr,
            SERVER_TICK_SECONDS,
            &mut trace,
        );
        let n = trace.node(node).unwrap();
        assert!((n.value - result.trigger_rate_cap).abs() < 1e-9);
        assert!(trace.nodes().len() >= 4);
        assert!(trace.incoming(node).len() >= 2);
    }
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
