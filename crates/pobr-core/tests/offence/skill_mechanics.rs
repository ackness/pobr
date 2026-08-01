//! 技能功能面机制集成测试（Lane C）。
//!
//! 涵盖 AoE 半径、投射物数量/行为、冷却、消耗/保留四个子系统。
//! 测试数值对照 `agent-docs/skill-mechanics.md` + PoB2 `CalcOffence.lua`。

use pobr_core::calc::skill_mechanics::{
    ProjectileBehavior, ProjectileBehaviorInput, calc_aoe, calc_aoe_traced_value, calc_cooldown,
    calc_mana_cost, calc_projectile_count, calc_spirit_reservation, resolve_projectile_behavior,
};
use pobr_core::{CalcConfig, ModDb, Modifier, TraceGraph};
use pobr_data::prelude::*;

// §1  AoE 半径

/// PoB2 `calcRadius` 参考值：baseRadius=12（120/10），无 AoE 修饰词 → areaMod=1.0 → radius=12。
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

/// more Area Of Effect + increased 叠加测试。
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

/// extra_base（Base AreaOfEffect 加值）参与半径计算。
#[test]
fn aoe_radius_with_extra_base() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    // base_radius=10, extra_base=2 → effective_base=12
    let result = calc_aoe(&db, &cfg, 10.0, 2.0);
    assert_eq!(result.radius, 12.0);
    assert_eq!(result.base_radius_input, 10.0);
}

/// base_radius=0 → radius=0（防止除零）。
#[test]
fn aoe_radius_zero_base() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_aoe(&db, &cfg, 0.0, 0.0);
    assert_eq!(result.radius, 0.0);
}

/// 追踪版本与非追踪版本数值一致。
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

// §2  投射物数量

/// 无修饰词：ProjectileCount BASE=1（或 0 时 more 结果为 0）。
/// PoB2 约定：ProjectileCount BASE = -1 + 技能本身 base（SkillStatMap `base=-1`），
/// 因此若不加任何词条，base sum = 0 → count = 0.0。
/// 测试只加 1 枚基础投射物词条（相当于宝石本身），期望 count = 1.0。
#[test]
fn projectile_count_base_one() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 1.0));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 1.0).abs() < 1e-9);
    assert!((result.additional_count - 0.0).abs() < 1e-9);
}

/// 基础 1 枚 + 额外 2 枚：base sum=3（SkillStatMap base=-1 约定下会是 base + additional=-1+3=2，
/// 但在本测试直接用 BASE sum=3），count=3。
#[test]
fn projectile_count_with_additional() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 3.0));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 3.0).abs() < 1e-9);
    assert!((result.additional_count - 2.0).abs() < 1e-9); // count-1
}

/// More("ProjectileCount") 为乘区。base=2, +50% more → count = 2*1.5 = 3.0。
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

/// NoAdditionalProjectiles flag → 强制 1 发。
#[test]
fn projectile_count_no_additional_projectiles_flag() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ProjectileCount", ModType::Base, 5.0));
    db.add_mod(Modifier::flag("NoAdditionalProjectiles"));
    let cfg = CalcConfig::attack();
    let result = calc_projectile_count(&db, &cfg);
    assert!((result.projectile_count - 1.0).abs() < 1e-9);
}

// §3  投射物行为优先级

/// 无任何行为激活 → behaviors 为空。
#[test]
fn projectile_behavior_none_active() {
    let input = ProjectileBehaviorInput::default();
    let result = resolve_projectile_behavior(&input, 0);
    assert!(result.behaviors.is_empty());
}

/// Split 激活（split_count=2）。
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

/// CannotSplit 锁定分裂。
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

/// PierceAllTargets → 无限穿透，Fork/Chain 不触发。
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
    // Fork 和 Chain 被无限穿透屏蔽
    assert!(!result.behaviors.contains(&ProjectileBehavior::Fork));
    assert!(!result.behaviors.contains(&ProjectileBehavior::Chain));
}

/// 优先级顺序：Split + Pierce + Fork + Chain 全激活时按顺序排列。
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
    // 按优先级顺序出现
    let order: Vec<usize> = [
        ProjectileBehavior::Split,
        ProjectileBehavior::Pierce,
        ProjectileBehavior::Fork,
        ProjectileBehavior::Chain,
    ]
    .iter()
    .filter_map(|b| result.behaviors.iter().position(|rb| rb == b))
    .collect();
    // 确认每个行为都存在且下标递增（有序）
    assert_eq!(order.len(), 4);
    assert!(order.windows(2).all(|w| w[0] < w[1]));
}

/// AdditionalProjectilesAddSplitsInstead：额外投射物转 Split。
#[test]
fn projectile_behavior_additional_adds_splits() {
    let input = ProjectileBehaviorInput {
        additional_projectiles_add_splits_instead: true,
        ..Default::default()
    };
    // 额外 2 枚转为 split
    let result = resolve_projectile_behavior(&input, 2);
    assert!(result.behaviors.contains(&ProjectileBehavior::Split));
    assert_eq!(result.split_count, 2);
}

/// AdditionalProjectilesAddChainsInstead：额外投射物转 Chain。
#[test]
fn projectile_behavior_additional_adds_chains() {
    let input = ProjectileBehaviorInput {
        chain_count_max: 1,
        additional_projectiles_add_chains_instead: true,
        ..Default::default()
    };
    // base chain=1 + 额外2 = 3
    let result = resolve_projectile_behavior(&input, 2);
    assert!(result.behaviors.contains(&ProjectileBehavior::Chain));
    assert_eq!(result.chain_count_max, 3);
}

/// ForkTwice → fork_count_max=2；ForkOnce → fork_count_max=1。
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

// §4  冷却

/// 无修饰词、基础 0.5s 冷却 → 向上取整到服务器帧。
/// 服务器帧率 ≈ 30.303/s；0.5 × 30.303 = 15.15 → ceil = 16 帧 → 16/30.303 ≈ 0.528s。
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

/// +100% increased Cooldown Recovery → 冷却减半。
#[test]
fn cooldown_increased_recovery_halves_cooldown() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("CooldownRecovery", ModType::Inc, 100.0));
    let cfg = CalcConfig::attack();
    // base=1.0s / 2.0 = 0.5s，然后取整到帧
    let result = calc_cooldown(&db, &cfg, 1.0, 1);
    assert!((result.recovery_rate - 2.0).abs() < 1e-9);
    // 实际 cd 应 ≤ 0.5s（取整后）
    assert!(result.cooldown <= 0.5 + 1.0 / (1.0 / SERVER_TICK_SECONDS));
}

/// stored_uses > 1 时不取整到服务器帧。
#[test]
fn cooldown_stored_uses_not_rounded() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    // base=0.3s, stored_uses=2 → 不取整
    let result = calc_cooldown(&db, &cfg, 0.3, 2);
    assert!(!result.rounded_to_tick);
    // cooldown 应接近 0.3（无 INC/MORE）
    assert!((result.cooldown - 0.3).abs() < 1e-6);
}

/// AdditionalCooldownUses BASE modifier 增加 stored_uses。
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
    // base_stored_uses=1 + AdditionalCooldownUses=1 = 2 → 不取整
    assert_eq!(result.stored_uses, 2);
    assert!(!result.rounded_to_tick);
}

/// 无冷却（base_cooldown_s=0.0，无加值）→ cooldown=0.0。
#[test]
fn cooldown_zero_when_no_cooldown() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_cooldown(&db, &cfg, 0.0, 1);
    assert_eq!(result.cooldown, 0.0);
}

// §5  消耗/保留

/// 无修饰词 Mana 消耗：base=20 → final=20。
#[test]
fn mana_cost_no_modifiers() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    assert!((result.final_cost - 20.0).abs() < 1e-9);
    assert!(!result.no_cost);
}

/// -30% ManaCost（reduced，即 Inc=-30）→ floor(20 × 0.7) = 14。
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

/// +50% increased Mana Cost → floor(20 × 1.5) = 30。
#[test]
fn mana_cost_increased_50pct() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ManaCost", ModType::Inc, 50.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    // floor(20 × 1.5) = 30
    assert!((result.final_cost - 30.0).abs() < 1e-9);
}

/// "no cost" ManaCost MORE = -100 → floor(x × 0) = 0。
#[test]
fn mana_cost_no_cost_more() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ManaCost", ModType::More, -100.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 30.0);
    // more = (1 + (-100)/100) = 0.0 → ceil(30 × 0) = 0（more < 1 用 ceil）
    assert_eq!(result.final_cost, 0.0);
}

/// HasNoCost flag → final_cost=0 且 no_cost=true。
#[test]
fn mana_cost_has_no_cost_flag() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("HasNoCost"));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 50.0);
    assert_eq!(result.final_cost, 0.0);
    assert!(result.no_cost);
}

/// Cost（通用 INC）也作用于 Mana：+20% Cost → floor(20 × 1.2) = 24。
#[test]
fn mana_cost_generic_cost_inc() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("Cost", ModType::Inc, 20.0));
    let cfg = CalcConfig::attack();
    let result = calc_mana_cost(&db, &cfg, 20.0);
    // inc comes from "Cost" bucket
    assert!((result.final_cost - 24.0).abs() < 1e-9);
}

/// 辅助宝石 cost 正倍率：SupportManaMultiplier MORE +30 →
/// finalBase = floor(10 × 1.3) = 13（PoB2 CalcOffence.lua:2052/:2076-2077，
/// 先于 inc/more 链作用于 base）。
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

/// 辅助宝石 cost 负倍率：SupportManaMultiplier MORE -50 →
/// finalBase = floor(9 × 0.5) = 4（mult 截断 4 位小数后 floor，**不**走
/// inc/more 链的负值 ceil 分支——base 段恒 floor，对齐 PoB2 m_floor）。
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

/// 多个辅助倍率连乘 + 截断 4 位小数后才乘 base：+30% × +10% = 1.43 →
/// floor(20 × 1.43) = 28；再叠 +50% ManaCost INC → floor(28 × 1.5) = 42
/// （SupportManaMultiplier 先于 inc 链，分步取整）。
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

/// Spirit 保留**不**吃辅助宝石通用 cost 倍率（PoB2 Reservation 段只认
/// ReservationMultiplier；SupportManaMultiplier 仅进 costs 通用路径）。
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

/// Spirit 保留：base=30，ReservationMultiplier MORE +20% → reserved = floor(30 × 1.2) = 36。
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

/// Spirit 保留：ExtraSpirit BASE 加值。
#[test]
fn spirit_reservation_extra_flat() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ExtraSpirit", ModType::Base, 5.0));
    let cfg = CalcConfig::attack();
    let result = calc_spirit_reservation(&db, &cfg, 30.0);
    // floor(30 * 1.0 + 5) = 35
    assert!((result.final_cost - 35.0).abs() < 1e-9);
}

/// HasNoCost 对 Spirit 保留同样免除。
#[test]
fn spirit_reservation_no_cost_flag() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("HasNoCost"));
    let cfg = CalcConfig::attack();
    let result = calc_spirit_reservation(&db, &cfg, 40.0);
    assert_eq!(result.final_cost, 0.0);
    assert!(result.no_cost);
}
