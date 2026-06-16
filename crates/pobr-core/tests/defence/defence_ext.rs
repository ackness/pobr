//! Lane 2 防御扩展测试：ES 充能、规避几率、承受伤害乘数、暴击额外伤害减免。
//!
//! 出处：agent-docs/energy-shield.md、active-defences.md、recovery-charges-buffs.md（PoE2 0.5.0）；
//!       PoB2 `src/Modules/CalcDefence.lua`、`src/Data/Misc.lua`。

use pobr_core::calc::defence::{
    ANY_TAKEN_REFLECT_ENABLED, AVOID_AILMENT_CAP, AVOID_HIT_CAP, CritExtraReduction, EsRecharge,
    HitSource, calc_avoidance, calc_crit_extra_reduction, calc_es_recharge, calc_taken_multi_suite,
    enemy_crit_effect, es_recharge_per_second, taken_mult_for_type, taken_mult_for_type_default,
    taken_mult_for_type_with_source, taken_mult_over_time,
};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

// ─────────────────────────────────────────────────────────────────
// ES 充能测试（gap: es-recharge-missing）
// ─────────────────────────────────────────────────────────────────

/// PoB2 Misc.lua: `character_inherent_energy_shield_recharge_rate_per_minute_% = 750`
/// → 12.5%/s。
#[test]
fn es_recharge_default_rate_is_12_5_pct_per_second() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);

    // 750 / 60 / 100 = 0.125
    assert!(
        (result.rate_fraction - 0.125).abs() < 1e-9,
        "default rate should be 12.5%/s, got {}",
        result.rate_fraction
    );
}

/// 默认充能延迟 4 秒（PoB2 CalcDefence.lua 确认）。
#[test]
fn es_recharge_default_delay_is_4_seconds() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);

    assert_eq!(
        result.delay_seconds, 4.0,
        "default recharge delay should be 4s"
    );
}

/// `EnergyShieldRechargeRate` INC 词条提升速率。
/// +100% INC → 速率翻倍（0.25/s）。
#[test]
fn es_recharge_rate_scales_with_inc_modifier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeRate",
        ModType::Inc,
        100.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);

    // base 750%/min * 2.0 = 1500%/min = 25%/s = 0.25
    assert!(
        (result.rate_fraction - 0.25).abs() < 1e-9,
        "+100% INC should double rate to 0.25/s, got {}",
        result.rate_fraction
    );
}

/// PoB2 CalcDefence.lua:1762-1763：**INC** `EnergyShieldRechargeFaster` 缩短延迟（分母）。
/// delay = (4 + Sum(BASE)) / (1 + Sum(INC)/100) = 4 / (1 + 100/100) = 2.0s。
#[test]
fn es_recharge_delay_halved_with_faster_inc_100pct() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeFaster",
        ModType::Inc,
        100.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);
    assert_eq!(result.delay_seconds, 2.0, "100% INC faster → delay 2s");
}

/// BASE `EnergyShieldRechargeFaster` 是「秒」，加到分子（4 + base），不缩放分母。
/// delay = (4 + 2) / (1 + 0) = 6.0s。
#[test]
fn es_recharge_delay_base_adds_seconds_to_numerator() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeFaster",
        ModType::Base,
        2.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);
    assert_eq!(result.delay_seconds, 6.0, "+2s BASE → delay 6s (4+2)");
}

/// Override('EnergyShieldRechargeBase') 直接替换基础，再除 INC：1.0 / (1+100/100) = 0.5s。
#[test]
fn es_recharge_delay_respects_override_base() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeBase",
        ModType::Override,
        1.0,
    ));
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeFaster",
        ModType::Inc,
        100.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);
    assert_eq!(result.delay_seconds, 0.5, "override base 1.0s / 2 = 0.5s");
}

/// ZealotsOath：ES 由再生驱动，充能禁用（rate_fraction = 0）。
#[test]
fn es_recharge_disabled_by_zealots_oath() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, true /* zealots_oath */);

    assert_eq!(
        result.rate_fraction, 0.0,
        "ZealotsOath should disable ES recharge"
    );
}

/// ES 为 0 时无充能（rate = 0）。
#[test]
fn es_recharge_zero_when_no_energy_shield() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 0.0, false);

    assert_eq!(result.rate_fraction, 0.0);
}

/// es_recharge_per_second：1000 ES × 12.5%/s = 125/s。
#[test]
fn es_recharge_per_second_gives_absolute_value() {
    let recharge = EsRecharge {
        rate_fraction: 0.125,
        delay_seconds: 4.0,
    };
    assert_eq!(es_recharge_per_second(&recharge, 1000.0), 125.0);
}

// ─────────────────────────────────────────────────────────────────
// 规避（Avoidance）测试（gap: avoidance-ailment-missing）
// ─────────────────────────────────────────────────────────────────

/// 无词条 → 全部规避几率为 0（眩晕除外：ES > totalTakenHit 时 = 50%）。
#[test]
fn avoidance_all_zero_without_modifiers_no_es() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0 /* no ES */, 0.0, false);

    assert_eq!(result.avoid_all_damage_from_hits, 0.0);
    assert_eq!(result.avoid_ignite, 0.0);
    assert_eq!(result.avoid_shock, 0.0);
    assert_eq!(result.avoid_chill, 0.0);
    assert_eq!(result.avoid_freeze, 0.0);
    assert_eq!(result.avoid_poison, 0.0);
    assert_eq!(result.avoid_bleeding, 0.0);
    // 无 ES 时眩晕规避 = 0
    assert_eq!(result.avoid_stun, 0.0);
}

/// ES > totalTakenHit（且非 EB）→ 眩晕规避隐式 50%（PoB2 CalcDefence.lua:2554-2557）。
#[test]
fn avoidance_stun_50pct_implicit_when_es_present() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    // M2-E2（CalcDefence.lua:2554-2557）：减半条件 = ES > totalTakenHit 且非 EB。
    let result = calc_avoidance(&db, &cfg, 500.0 /* ES > takenHit */, 100.0, false);

    // notAvoidChance = 100; with ES → notAvoidChance *= 0.5 = 50; effectiveAvoid = 50%
    assert_eq!(
        result.avoid_stun, 50.0,
        "ES > totalTakenHit should give 50% implicit stun avoidance"
    );
}

/// AvoidStun 词条 70% + ES > totalTakenHit（隐式减半应用于 notAvoidChance）。
/// notAvoidChance = 100 - 70 = 30; × 0.5 = 15; effectiveAvoid = 85%.
#[test]
fn avoidance_stun_combines_explicit_mod_and_es_implicit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AvoidStun", ModType::Base, 70.0));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 500.0, 100.0, false);

    // notAvoid = 100 - 70 = 30; × 0.5 = 15; effectiveAvoid = 85
    assert_eq!(result.avoid_stun, 85.0);
}

/// AvoidAllDamageFromHitsChance 上限 75%（AVOID_HIT_CAP）。
#[test]
fn avoidance_all_damage_capped_at_75() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "AvoidAllDamageFromHitsChance",
        ModType::Base,
        90.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_all_damage_from_hits, AVOID_HIT_CAP);
}

/// 异常规避（点燃）上限 100%（AVOID_AILMENT_CAP）。
#[test]
fn avoidance_ailment_capped_at_100() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AvoidIgnite", ModType::Base, 120.0));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_ignite, AVOID_AILMENT_CAP);
}

/// `IgniteImmune` 旗标 → 直接置 100%。
#[test]
fn avoidance_immune_flag_sets_100() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("IgniteImmune"));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_ignite, 100.0);
}

/// `ElementalAilmentImmune` → 点燃/感电/冰缓/冰冻全部置 100%。
#[test]
fn avoidance_elemental_immune_covers_all_elemental_ailments() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("ElementalAilmentImmune"));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_ignite, 100.0);
    assert_eq!(result.avoid_shock, 100.0);
    assert_eq!(result.avoid_chill, 100.0);
    assert_eq!(result.avoid_freeze, 100.0);
}

/// `ShockAvoidAppliesToElementalAilments`（Stormshroud）联动：
/// 感电规避 50% 也作用于点燃/冰缓/冰冻。
#[test]
fn avoidance_stormshroud_shock_applies_to_elemental_ailments() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AvoidShock", ModType::Base, 50.0));
    db.add_mod(Modifier::flag("ShockAvoidAppliesToElementalAilments"));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_shock, 50.0);
    // 点燃 = AvoidIgnite(0) + AvoidElementalAilments(0) + shock_avoid_raw(50) = 50%
    assert_eq!(result.avoid_ignite, 50.0);
    assert_eq!(result.avoid_chill, 50.0);
    assert_eq!(result.avoid_freeze, 50.0);
}

// ─────────────────────────────────────────────────────────────────
// 承受伤害乘数（Taken multiplier）测试（gap: ehp-no-taken-multiplier）
// ─────────────────────────────────────────────────────────────────

/// 无词条 → 承受乘数 = 1.0（基准）。
#[test]
fn taken_mult_default_is_1() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();

    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Physical), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Fire), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Cold), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Lightning), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Chaos), 1.0);
}

/// `DamageTaken` INC −20（Fortify 等效）→ 乘数 0.8。
#[test]
fn taken_mult_reduced_by_inc_modifier() {
    let mut db = ModDb::new();
    // Fortify: DamageTakenWhenHit MORE -10 (per stack), 10 stacks = -10% MORE
    db.add_mod(Modifier::number("DamageTakenWhenHit", ModType::Inc, -20.0));
    let cfg = CalcConfig::default();

    let mult = taken_mult_for_type(&db, &cfg, DamageType::Physical);
    assert!(
        (mult - 0.8).abs() < 1e-9,
        "-20% INC should give mult=0.8, got {mult}"
    );
}

/// `PhysicalDamageTakenWhenHit` INC 仅影响物理，不影响火焰。
#[test]
fn taken_mult_type_specific_mod_only_affects_that_type() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "PhysicalDamageTakenWhenHit",
        ModType::Inc,
        -30.0,
    ));
    let cfg = CalcConfig::default();

    let phys = taken_mult_for_type(&db, &cfg, DamageType::Physical);
    let fire = taken_mult_for_type(&db, &cfg, DamageType::Fire);

    assert!(
        (phys - 0.7).abs() < 1e-9,
        "phys mult should be 0.7, got {phys}"
    );
    assert_eq!(fire, 1.0, "fire mult should be unaffected");
}

/// `ElementalDamageTaken` INC 应作用于火/冰/电但不影响混沌。
#[test]
fn taken_mult_elemental_mod_applies_to_elemental_types_only() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ElementalDamageTaken", ModType::Inc, 20.0));
    let cfg = CalcConfig::default();

    let fire = taken_mult_for_type(&db, &cfg, DamageType::Fire);
    let cold = taken_mult_for_type(&db, &cfg, DamageType::Cold);
    let lightning = taken_mult_for_type(&db, &cfg, DamageType::Lightning);
    let chaos = taken_mult_for_type(&db, &cfg, DamageType::Chaos);

    assert!((fire - 1.2).abs() < 1e-9, "fire should be 1.2, got {fire}");
    assert!((cold - 1.2).abs() < 1e-9);
    assert!((lightning - 1.2).abs() < 1e-9);
    assert_eq!(
        chaos, 1.0,
        "chaos should not be affected by elemental taken"
    );
}

/// OverTime 版本：`DamageTakenOverTime` INC 仅影响 OverTime 乘数，不影响 WhenHit。
#[test]
fn taken_mult_over_time_independent_from_when_hit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DamageTakenOverTime", ModType::Inc, 50.0));
    let cfg = CalcConfig::default();

    let ot = taken_mult_over_time(&db, &cfg, DamageType::Physical);
    let hit = taken_mult_for_type(&db, &cfg, DamageType::Physical);

    assert!(
        (ot - 1.5).abs() < 1e-9,
        "OverTime mult should be 1.5, got {ot}"
    );
    assert_eq!(hit, 1.0, "WhenHit should be unaffected by OverTime mod");
}

/// MORE 词条（Fortify 实际注入 DamageTakenWhenHit MORE −10 per stack）。
/// 10 stacks of Fortify = DamageTakenWhenHit MORE −10 → mult = 0.9。
#[test]
fn taken_mult_more_modifier_multiplies_correctly() {
    let mut db = ModDb::new();
    // MORE = -10 意味着 × (1 + -10/100) = × 0.9
    db.add_mod(Modifier::number("DamageTakenWhenHit", ModType::More, -10.0));
    let cfg = CalcConfig::default();

    let mult = taken_mult_for_type(&db, &cfg, DamageType::Physical);
    assert!(
        (mult - 0.9).abs() < 1e-9,
        "MORE -10 should give mult=0.9, got {mult}"
    );
}

/// calc_taken_multi_suite：套件中各类型独立。
#[test]
fn taken_multi_suite_type_independent() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "FireDamageTakenWhenHit",
        ModType::Inc,
        30.0,
    ));
    let cfg = CalcConfig::default();

    let suite = calc_taken_multi_suite(&db, &cfg);
    assert!((suite.fire_when_hit - 1.3).abs() < 1e-9);
    assert_eq!(suite.physical_when_hit, 1.0);
    assert_eq!(suite.cold_when_hit, 1.0);
    assert_eq!(suite.chaos_when_hit, 1.0);
}

// ─────────────────────────────────────────────────────────────────
// 暴击额外伤害减免（gap: crit-extra-damage-reduction-missing）
// ─────────────────────────────────────────────────────────────────

/// 无词条 → reduction_pct = 0，EnemyCritEffect = 完整爆伤乘数。
#[test]
fn crit_extra_reduction_default_zero() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_crit_extra_reduction(&db, &cfg);

    assert_eq!(result.reduction_pct, 0.0);
}

/// 50% ReduceCritExtraDamage + 敌人 50% 暴击几率 + 100% 爆伤加成：
/// PoB2 公式：EnemyCritEffect = 1 + 0.5 * (100/100) * (1 − 50/100) = 1 + 0.5 * 0.5 = 1.25。
#[test]
fn enemy_crit_effect_with_50pct_reduction() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "ReduceCritExtraDamage",
        ModType::Base,
        50.0,
    ));
    let cfg = CalcConfig::default();
    let reduction = calc_crit_extra_reduction(&db, &cfg);

    assert_eq!(reduction.reduction_pct, 50.0);

    let effect = enemy_crit_effect(50.0, 100.0, &reduction);
    // 1 + 0.5 * 1.0 * 0.5 = 1.25
    assert!(
        (effect - 1.25).abs() < 1e-9,
        "EnemyCritEffect should be 1.25, got {effect}"
    );
}

/// 100% 减免 → EnemyCritEffect = 1.0（不承受暴击额外伤害）。
#[test]
fn enemy_crit_effect_100pct_reduction_equals_no_extra() {
    let reduction = CritExtraReduction {
        reduction_pct: 100.0,
    };
    let effect = enemy_crit_effect(50.0, 200.0, &reduction);

    assert!(
        (effect - 1.0).abs() < 1e-9,
        "100% reduction should make EnemyCritEffect = 1.0, got {effect}"
    );
}

/// 上限：ReduceCritExtraDamage 不能超过 100%（钳到 100）。
#[test]
fn crit_extra_reduction_capped_at_100() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "ReduceCritExtraDamage",
        ModType::Base,
        150.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_crit_extra_reduction(&db, &cfg);

    assert_eq!(result.reduction_pct, 100.0);
}

/// 无暴击时 EnemyCritEffect = 1.0（enemy_crit_chance = 0）。
#[test]
fn enemy_crit_effect_no_crit_chance_returns_1() {
    let reduction = CritExtraReduction { reduction_pct: 0.0 };
    let effect = enemy_crit_effect(0.0, 100.0, &reduction);

    assert_eq!(effect, 1.0);
}

// ─────────────────────────────────────────────────────────────────
// Attack/Spell takenMult 上下文 + 反射 defer（finding 06-06）
// PoB2 CalcDefence.lua L2265-2269（hitSourceList={"Attack","Spell"}）。
// ─────────────────────────────────────────────────────────────────

/// `source=None` 与 [`taken_mult_for_type`] 等价（基础 hit 口径，无 Attack/Spell 层）。
#[test]
fn taken_mult_with_source_none_equals_base() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireDamageTaken", ModType::Inc, 30.0));
    let cfg = CalcConfig::default();

    let base = taken_mult_for_type(&db, &cfg, DamageType::Fire);
    let none = taken_mult_for_type_with_source(&db, &cfg, DamageType::Fire, None);
    assert!((base - none).abs() < 1e-9);
}

/// `AttackDamageTaken` 仅在 Attack 上下文叠加，Spell/None 不读。
#[test]
fn attack_damage_taken_only_in_attack_context() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackDamageTaken", ModType::Inc, 25.0));
    let cfg = CalcConfig::default();

    let attack =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Physical, Some(HitSource::Attack));
    let spell =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Physical, Some(HitSource::Spell));
    let none = taken_mult_for_type(&db, &cfg, DamageType::Physical);

    assert!(
        (attack - 1.25).abs() < 1e-9,
        "Attack 上下文 +25% → 1.25, got {attack}"
    );
    assert_eq!(spell, 1.0, "Spell 上下文不读 AttackDamageTaken");
    assert_eq!(none, 1.0, "基础 hit 口径不读 AttackDamageTaken");
}

/// `SpellDamageTaken` 仅在 Spell 上下文叠加。
#[test]
fn spell_damage_taken_only_in_spell_context() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("SpellDamageTaken", ModType::Inc, -40.0));
    let cfg = CalcConfig::default();

    let spell =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Fire, Some(HitSource::Spell));
    let attack =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Fire, Some(HitSource::Attack));

    assert!(
        (spell - 0.6).abs() < 1e-9,
        "Spell 上下文 -40% → 0.6, got {spell}"
    );
    assert_eq!(attack, 1.0, "Attack 上下文不读 SpellDamageTaken");
}

/// base + WhenHit + Attack 层叠加：DamageTaken+10、PhysicalDamageTakenWhenHit+10、
/// AttackDamageTaken+10 → (1 + 0.30) = 1.30（同一 INC 加法桶）。
#[test]
fn attack_context_stacks_with_base_and_when_hit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DamageTaken", ModType::Inc, 10.0));
    db.add_mod(Modifier::number(
        "PhysicalDamageTakenWhenHit",
        ModType::Inc,
        10.0,
    ));
    db.add_mod(Modifier::number("AttackDamageTaken", ModType::Inc, 10.0));
    let cfg = CalcConfig::default();

    let attack =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Physical, Some(HitSource::Attack));
    assert!(
        (attack - 1.30).abs() < 1e-9,
        "10+10+10 INC → 1.30, got {attack}"
    );
}

/// PoB2 default（"Average"）= (Attack 层 + Spell 层) / 2。
/// AttackDamageTaken+40 → Attack=1.4，Spell=1.0 → 默认 1.2。
#[test]
fn taken_mult_default_is_average_of_attack_and_spell() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackDamageTaken", ModType::Inc, 40.0));
    let cfg = CalcConfig::default();

    let def = taken_mult_for_type_default(&db, &cfg, DamageType::Physical);
    assert!(
        (def - 1.2).abs() < 1e-9,
        "Average of 1.4 and 1.0 = 1.2, got {def}"
    );
}

/// 无 Attack/Spell 词条时 default 退化为基础 hit 口径（对现有回归保持一致）。
#[test]
fn taken_mult_default_degenerates_to_base_without_source_mods() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DamageTaken", ModType::Inc, -25.0));
    db.add_mod(Modifier::number(
        "FireDamageTakenWhenHit",
        ModType::Inc,
        10.0,
    ));
    let cfg = CalcConfig::default();

    for dt in [
        DamageType::Physical,
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Lightning,
        DamageType::Chaos,
    ] {
        let base = taken_mult_for_type(&db, &cfg, dt);
        let def = taken_mult_for_type_default(&db, &cfg, dt);
        assert!(
            (base - def).abs() < 1e-9,
            "{dt:?}: default {def} 应等于基础 hit 口径 {base}（无 Attack/Spell 词条）"
        );
    }
}

/// 反射 takenMult 当前 defer（PoB2 自身 AnyTakenReflect 写死 false）。
#[test]
fn any_taken_reflect_is_deferred_false() {
    // 把 const 读进运行时变量，避免对编译期常量做平凡断言（clippy::assertions_on_constants）。
    let enabled = std::hint::black_box(ANY_TAKEN_REFLECT_ENABLED);
    assert!(!enabled, "反射受伤链 defer，与 PoB2 当前行为一致");
}
