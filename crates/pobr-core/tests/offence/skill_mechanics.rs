//! Skill-mechanics integration tests (Lane C).
//!
//! Covers four subsystems: AoE radius, projectile count/behavior, cooldown, and
//! cost/reservation. Expected values are checked against
//! `agent-docs/skill-mechanics.md` + PoB2 `CalcOffence.lua`.

use pobr_core::calc::skill_mechanics::{
    ProjectileBehavior, ProjectileBehaviorInput, calc_aoe, calc_aoe_traced_value, calc_cooldown,
    calc_mana_cost, calc_projectile_count, calc_spirit_reservation, resolve_projectile_behavior,
};
use pobr_core::{CalcConfig, ModDb, Modifier, TraceGraph};
use pobr_data::prelude::*;

// §1  AoE radius

/// PoB2 `calcRadius` reference value: baseRadius=12 (120/10), no AoE mods ->
/// areaMod=1.0 -> radius=12.
#[test]
fn aoe_radius_no_modifiers() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_aoe(&db, &cfg, 12.0, 0.0);
    // areaMod=1.0, floor(12 * floor(100)/100) = floor(12 * 1.0) = 12
    assert_eq!(result.radius, 12.0);
    assert!((result.area_mod - 1.0).abs() < 1e-6);
}

/// +50% increased Area → areaMod=1.5 → radius = floor(12 × floor(100×√1.5)/100)
/// floor(100×√1.5) = floor(122.47...) = 122
/// → floor(12 × 122/100) = floor(14.64) = 14
#[test]
fn aoe_radius_50pct_increased() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AreaOfEffect", ModType::Inc, 50.0));
    let cfg = CalcConfig::attack();
    let result = calc_aoe(&db, &cfg, 12.0, 0.0);
    // areaMod = (1 + 50/100) * 1.0 = 1.5
    assert!((result.area_mod - 1.5).abs() < 1e-4);
    // radius = floor(12 * floor(100*sqrt(1.5))/100) = floor(12 * 122/100) = floor(14.64) = 14
    assert_eq!(result.radius, 14.0);
}

/// +40% increased Area → areaMod=1.4 → radius = floor(12 × floor(118)/100) = floor(14.16) = 14
#[test]
fn aoe_radius_40pct_increased() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AreaOfEffect", ModType::Inc, 40.0));
    let cfg = CalcConfig::attack();
    let result = calc_aoe(&db, &cfg, 12.0, 0.0);
    // areaMod = 1.4; floor(100*sqrt(1.4)) = floor(118.32) = 118
    // radius = floor(12 * 118/100) = floor(14.16) = 14
    assert_eq!(result.radius, 14.0);
}

/// more Area Of Effect + increased stacking test.
/// +50% inc × 20% more → areaMod = 1.5 × 1.2 = 1.8
/// floor(100×√1.8) = floor(134.16) = 134 → floor(12×134/100)=floor(16.08)=16
#[test]
fn aoe_radius_inc_and_more_stacking() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AreaOfEffect", ModType::Inc, 50.0));
    db.add_mod(Modifier::number("AreaOfEffect", ModType::More, 20.0));
    let cfg = CalcConfig::attack();
    let result = calc_aoe(&db, &cfg, 12.0, 0.0);
    // areaMod = 1.5 * 1.2 = 1.8
    assert!((result.area_mod - 1.8).abs() < 1e-4);
    assert_eq!(result.radius, 16.0);
}

/// extra_base (a Base AreaOfEffect addend) participates in the radius calculation.
#[test]
fn aoe_radius_with_extra_base() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    // base_radius=10, extra_base=2 → effective_base=12
    let result = calc_aoe(&db, &cfg, 10.0, 2.0);
    assert_eq!(result.radius, 12.0);
    assert_eq!(result.base_radius_input, 10.0);
}

/// base_radius=0 -> radius=0 (guards against division by zero).
#[test]
fn aoe_radius_zero_base() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_aoe(&db, &cfg, 0.0, 0.0);
    assert_eq!(result.radius, 0.0);
}

/// The traced and non-traced versions produce identical values.
#[test]
fn aoe_traced_matches_non_traced() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AreaOfEffect", ModType::Inc, 30.0));
    let cfg = CalcConfig::attack();
    let plain = calc_aoe(&db, &cfg, 12.0, 0.0);
    let mut trace = TraceGraph::new();
    let traced = calc_aoe_traced_value(&db, &cfg, 12.0, 0.0, &mut trace);
    assert!((plain.radius - traced.value).abs() < 1e-9);
}

// §2  Projectile count

/// No mods: ProjectileCount BASE=1 (or the more result is 0 at 0).
/// PoB2's convention: ProjectileCount BASE = -1 + the skill's own base
/// (SkillStatMap `base=-1`), so with no mods at all, base sum = 0 -> count = 0.0.
/// This test adds only 1 base projectile mod (representing the gem itself),
/// expecting count = 1.0.
#[test]
fn projectile_count_base_one() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 1.0));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 1.0).abs() < 1e-9);
    assert!((result.additional_count - 0.0).abs() < 1e-9);
}

/// 1 base + 2 additional: base sum=3 (under the SkillStatMap base=-1 convention
/// this would be base + additional = -1+3=2, but this test uses BASE sum=3
/// directly), count=3.
#[test]
fn projectile_count_with_additional() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 3.0));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 3.0).abs() < 1e-9);
    assert!((result.additional_count - 2.0).abs() < 1e-9); // count-1
}

/// More("ProjectileCount") is a multiplicative bucket. base=2, +50% more -> count
/// = 2*1.5 = 3.0.
#[test]
fn projectile_count_more_multiplier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 2.0));
    db.add_mod(Modifier::number("ProjectileCount", ModType::More, 50.0));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 3.0).abs() < 1e-9);
    assert!((result.more_factor - 1.5).abs() < 1e-9);
}

/// NoAdditionalProjectiles flag -> forces a single projectile.
#[test]
fn projectile_count_no_additional_projectiles_flag() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 5.0));
    db.add_mod(Modifier::flag("NoAdditionalProjectiles"));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 1.0).abs() < 1e-9);
}

// §3  Projectile behavior priority

/// No behavior active -> behaviors is empty.
#[test]
fn projectile_behavior_none_active() {
    let input = ProjectileBehaviorInput::default();
    let result = resolve_projectile_behavior(&input, 0);
    assert!(result.behaviors.is_empty());
}

/// Split active (split_count=2).
#[test]
fn projectile_behavior_split_active() {
    let input = ProjectileBehaviorInput {
        split_count: 2,
        ..Default::default()
    };
    let result = resolve_projectile_behavior(&input, 0);
    assert!(result.behaviors.contains(&ProjectileBehavior::Split));
    assert_eq!(result.split_count, 2);
}

/// CannotSplit locks out splitting.
#[test]
fn projectile_behavior_cannot_split() {
    let input = ProjectileBehaviorInput {
        split_count: 3,
        cannot_split: true,
        ..Default::default()
    };
    let result = resolve_projectile_behavior(&input, 0);
    assert!(!result.behaviors.contains(&ProjectileBehavior::Split));
    assert_eq!(result.split_count, 0);
}

/// PierceAllTargets -> unlimited pierce; Fork/Chain don't trigger.
#[test]
fn projectile_behavior_pierce_all_blocks_fork_chain() {
    let input = ProjectileBehaviorInput {
        pierce_all_targets: true,
        fork_once: true,
        chain_count_max: 3,
        ..Default::default()
    };
    let result = resolve_projectile_behavior(&input, 0);
    assert!(result.effective_pierce_all);
    assert!(result.behaviors.contains(&ProjectileBehavior::Pierce));
    // Fork and Chain are suppressed by unlimited pierce
    assert!(!result.behaviors.contains(&ProjectileBehavior::Fork));
    assert!(!result.behaviors.contains(&ProjectileBehavior::Chain));
}

/// Priority order: with Split + Pierce + Fork + Chain all active, they appear in
/// order.
#[test]
fn projectile_behavior_all_active_order() {
    let input = ProjectileBehaviorInput {
        split_count: 1,
        pierce_count: 1,
        fork_once: true,
        chain_count_max: 2,
        ..Default::default()
    };
    let result = resolve_projectile_behavior(&input, 0);
    // Appear in priority order
    let order: Vec<usize> = [
        ProjectileBehavior::Split,
        ProjectileBehavior::Pierce,
        ProjectileBehavior::Fork,
        ProjectileBehavior::Chain,
    ]
    .iter()
    .filter_map(|b| result.behaviors.iter().position(|rb| rb == b))
    .collect();
    // Confirm every behavior is present and indices are increasing (ordered)
    assert_eq!(order.len(), 4);
    assert!(order.windows(2).all(|w| w[0] < w[1]));
}

/// AdditionalProjectilesAddSplitsInstead: extra projectiles convert to Split.
#[test]
fn projectile_behavior_additional_adds_splits() {
    let input = ProjectileBehaviorInput {
        additional_projectiles_add_splits_instead: true,
        ..Default::default()
    };
    // 2 extra projectiles convert to split
    let result = resolve_projectile_behavior(&input, 2);
    assert!(result.behaviors.contains(&ProjectileBehavior::Split));
    assert_eq!(result.split_count, 2);
}

/// AdditionalProjectilesAddChainsInstead: extra projectiles convert to Chain.
#[test]
fn projectile_behavior_additional_adds_chains() {
    let input = ProjectileBehaviorInput {
        chain_count_max: 1,
        additional_projectiles_add_chains_instead: true,
        ..Default::default()
    };
    // base chain=1 + 2 extra = 3
    let result = resolve_projectile_behavior(&input, 2);
    assert!(result.behaviors.contains(&ProjectileBehavior::Chain));
    assert_eq!(result.chain_count_max, 3);
}

/// ForkTwice -> fork_count_max=2; ForkOnce -> fork_count_max=1.
#[test]
fn projectile_behavior_fork_once_and_twice() {
    let input_once = ProjectileBehaviorInput {
        fork_once: true,
        ..Default::default()
    };
    let r_once = resolve_projectile_behavior(&input_once, 0);
    assert_eq!(r_once.fork_count_max, 1);

    let input_twice = ProjectileBehaviorInput {
        fork_twice: true,
        ..Default::default()
    };
    let r_twice = resolve_projectile_behavior(&input_twice, 0);
    assert_eq!(r_twice.fork_count_max, 2);
}

// §4  Cooldown

/// No mods, a base 0.5s cooldown -> rounds up to a server frame.
/// Server tick rate ~= 30.303/s; 0.5 x 30.303 = 15.15 -> ceil = 16 frames ->
/// 16/30.303 ~= 0.528s.
#[test]
fn cooldown_rounds_up_to_server_frame() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_cooldown(&db, &cfg, 0.5, 1);
    assert!(result.rounded_to_tick);
    // 0.5 × 30.303 = 15.15 → ceil = 16 → 16/30.303 = 0.528s
    let tick_rate = 1.0 / SERVER_TICK_SECONDS;
    let expected = (0.5 * tick_rate).ceil() / tick_rate;
    assert!((result.cooldown - expected).abs() < 1e-9);
}

/// +100% increased Cooldown Recovery -> halves the cooldown.
#[test]
fn cooldown_increased_recovery_halves_cooldown() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("CooldownRecovery", ModType::Inc, 100.0));
    let cfg = CalcConfig::attack();
    // base=1.0s / 2.0 = 0.5s, then rounds to a frame
    let result = calc_cooldown(&db, &cfg, 1.0, 1);
    assert!((result.recovery_rate - 2.0).abs() < 1e-9);
    // The actual cooldown should be <= 0.5s (after rounding)
    assert!(result.cooldown <= 0.5 + 1.0 / (1.0 / SERVER_TICK_SECONDS));
}

/// stored_uses > 1 skips the round-to-server-frame step.
#[test]
fn cooldown_stored_uses_not_rounded() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    // base=0.3s, stored_uses=2 -> no rounding
    let result = calc_cooldown(&db, &cfg, 0.3, 2);
    assert!(!result.rounded_to_tick);
    // cooldown should be close to 0.3 (no INC/MORE)
    assert!((result.cooldown - 0.3).abs() < 1e-6);
}

/// AdditionalCooldownUses BASE modifier increases stored_uses.
#[test]
fn cooldown_additional_uses_from_modifier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "AdditionalCooldownUses",
        ModType::Base,
        1.0,
    ));
    let cfg = CalcConfig::attack();
    let result = calc_cooldown(&db, &cfg, 0.3, 1);
    // base_stored_uses=1 + AdditionalCooldownUses=1 = 2 -> no rounding
    assert_eq!(result.stored_uses, 2);
    assert!(!result.rounded_to_tick);
}

/// No cooldown (base_cooldown_s=0.0, no addend) -> cooldown=0.0.
#[test]
fn cooldown_zero_when_no_cooldown() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_cooldown(&db, &cfg, 0.0, 1);
    assert_eq!(result.cooldown, 0.0);
}

// §5  Cost/reservation

/// No mods, Mana cost: base=20 -> final=20.
#[test]
fn mana_cost_no_modifiers() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    assert!((result.final_cost - 20.0).abs() < 1e-9);
    assert!(!result.no_cost);
}

/// -30% ManaCost (reduced, i.e. Inc=-30) -> floor(20 x 0.7) = 14.
#[test]
fn mana_cost_reduced_30pct() {
    let mut db = ModDb::new();
    // reduced = Inc with negative value
    db.add_mod(Modifier::number("ManaCost", ModType::Inc, -30.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    // inc=-30 → ceil(20 × 0.7) = ceil(14.0) = 14
    assert!((result.final_cost - 14.0).abs() < 1e-9);
}

/// +50% increased Mana Cost -> floor(20 x 1.5) = 30.
#[test]
fn mana_cost_increased_50pct() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ManaCost", ModType::Inc, 50.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    // floor(20 × 1.5) = 30
    assert!((result.final_cost - 30.0).abs() < 1e-9);
}

/// "no cost" ManaCost MORE = -100 -> floor(x * 0) = 0.
#[test]
fn mana_cost_no_cost_more() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ManaCost", ModType::More, -100.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 30.0);
    // more = (1 + (-100)/100) = 0.0 -> ceil(30 * 0) = 0 (more < 1 uses ceil)
    assert_eq!(result.final_cost, 0.0);
}

/// HasNoCost flag -> final_cost=0 and no_cost=true.
#[test]
fn mana_cost_has_no_cost_flag() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("HasNoCost"));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 50.0);
    assert_eq!(result.final_cost, 0.0);
    assert!(result.no_cost);
}

/// Cost (the generic INC) also applies to Mana: +20% Cost -> floor(20 * 1.2) = 24.
#[test]
fn mana_cost_generic_cost_inc() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("Cost", ModType::Inc, 20.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    // inc comes from "Cost" bucket
    assert!((result.final_cost - 24.0).abs() < 1e-9);
}

/// Support gem positive cost multiplier: SupportManaMultiplier MORE +30 ->
/// finalBase = floor(10 x 1.3) = 13 (PoB2 CalcOffence.lua:2052/:2076-2077;
/// applied to base before the inc/more chain).
#[test]
fn mana_cost_support_multiplier_positive() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "SupportManaMultiplier",
        ModType::More,
        30.0,
    ));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 10.0);
    assert!((result.final_cost - 13.0).abs() < 1e-9);
}

/// Support gem negative cost multiplier: SupportManaMultiplier MORE -50 ->
/// finalBase = floor(9 x 0.5) = 4 (the multiplier is truncated to 4 decimal
/// places then floored -- it does **not** take the negative-value ceil branch of
/// the inc/more chain; the base stage always floors, matching PoB2's m_floor).
#[test]
fn mana_cost_support_multiplier_negative() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "SupportManaMultiplier",
        ModType::More,
        -50.0,
    ));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 9.0);
    assert!((result.final_cost - 4.0).abs() < 1e-9);
}

/// Multiple support multipliers chain-multiply, truncated to 4 decimal places
/// before multiplying base: +30% x +10% = 1.43 -> floor(20 x 1.43) = 28; then
/// stacking +50% ManaCost INC -> floor(28 x 1.5) = 42 (SupportManaMultiplier
/// applies before the inc chain, rounding at each step).
#[test]
fn mana_cost_support_multiplier_stacks_then_inc_applies() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "SupportManaMultiplier",
        ModType::More,
        30.0,
    ));
    db.add_mod(Modifier::number(
        "SupportManaMultiplier",
        ModType::More,
        10.0,
    ));
    db.add_mod(Modifier::number("ManaCost", ModType::Inc, 50.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    assert!((result.final_cost - 42.0).abs() < 1e-9);
}

/// Spirit reservation is **not** affected by the generic support-gem cost
/// multiplier (PoB2's Reservation stage only recognizes ReservationMultiplier;
/// SupportManaMultiplier only feeds the generic costs path).
#[test]
fn spirit_reservation_ignores_support_mana_multiplier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "SupportManaMultiplier",
        ModType::More,
        50.0,
    ));
    let cfg = CalcConfig::attack();
    let result = calc_spirit_reservation(&db, &cfg, 30.0);
    assert!((result.final_cost - 30.0).abs() < 1e-9);
}

/// Spirit reservation: base=30, ReservationMultiplier MORE +20% -> reserved =
/// floor(30 x 1.2) = 36.
#[test]
fn spirit_reservation_with_multiplier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "ReservationMultiplier",
        ModType::More,
        20.0,
    ));
    let cfg = CalcConfig::attack();
    let result = calc_spirit_reservation(&db, &cfg, 30.0);
    // floor(30 * 1.2) = 36
    assert!((result.final_cost - 36.0).abs() < 1e-9);
    assert_eq!(result.kind, SkillCostKind::Spirit);
}

/// Spirit reservation: ExtraSpirit BASE addend.
#[test]
fn spirit_reservation_extra_flat() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ExtraSpirit", ModType::Base, 5.0));
    let cfg = CalcConfig::attack();
    let result = calc_spirit_reservation(&db, &cfg, 30.0);
    // floor(30 * 1.0 + 5) = 35
    assert!((result.final_cost - 35.0).abs() < 1e-9);
}

/// HasNoCost also waives Spirit reservation.
#[test]
fn spirit_reservation_no_cost_flag() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("HasNoCost"));
    let cfg = CalcConfig::attack();
    let result = calc_spirit_reservation(&db, &cfg, 40.0);
    assert_eq!(result.final_cost, 0.0);
    assert!(result.no_cost);
}
