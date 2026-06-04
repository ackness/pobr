use pobr_core::calc::ailment::{
    bleed_instance, corrupted_blood_instance, ignite_instance, poison_instance, shock_effect,
};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

#[test]
fn bleed_magnitude_is_15_percent_of_physical_hit_per_second() {
    let gc = GameConstants::poe2();
    let instance = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());

    assert_eq!(instance.ailment, AilmentType::Bleed);
    assert_eq!(instance.magnitude_dps, 1000.0 * gc.bleed_base_fraction);
    assert_eq!(instance.duration_secs, gc.bleed_base_duration);
    assert!(instance.bypasses_es);
}

#[test]
fn ignite_uses_fire_fraction_and_duration() {
    let gc = GameConstants::poe2();
    let instance = ignite_instance(500.0, &ModDb::new(), &CalcConfig::attack());

    assert_eq!(instance.ailment, AilmentType::Ignite);
    assert_eq!(instance.magnitude_dps, 500.0 * gc.ignite_base_fraction);
    assert_eq!(instance.duration_secs, gc.ignite_base_duration);
}

#[test]
fn poison_magnitude_scales_with_ailment_damage_modifiers() {
    let gc = GameConstants::poe2();
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PoisonDamage", ModType::Inc, 100.0));

    let instance = poison_instance(1000.0, &db, &CalcConfig::attack());

    let base = 1000.0 * gc.poison_base_fraction;
    assert_eq!(instance.magnitude_dps, base * 2.0);
    assert_eq!(instance.ailment, AilmentType::Poison);
}

/// PoE2 0.5.0 感电效果范围测试。
///
/// **Bug#9 修正**：感电最小值 20%（非 PoE1 的 5%），最大值 100%（非 PoE1 的 50%）。
/// 出处：agent-docs/ailments.md §感电 `BaseShockMagnitude=20, max=100`；
///       PoB2 `nonDamagingAilmentsConfig.Shock, clamp [20, 100]`。
#[test]
fn shock_effect_is_clamped_between_20_and_100_percent_poe2() {
    // 无击中 → 返回 0（不施加感电）
    assert_eq!(shock_effect(0.0, 1000.0), 0.0);
    // 极大击中 → 感电上限 100%（= 1.0 fraction）
    let huge = shock_effect(1_000_000.0, 100.0);
    assert_eq!(huge, 1.0);
    // 极小击中（相对阈值）→ 感电下限 20%（= 0.20 fraction）
    let tiny = shock_effect(1.0, 1_000_000.0);
    assert_eq!(tiny, 0.20);
    // 满阈值击中（ratio=1）→ 50% 感电（0.5 * 1.0^0.4 = 0.5，fraction 0.50）
    let at_threshold = shock_effect(1000.0, 1000.0);
    assert_eq!(at_threshold, 0.50);
}

#[test]
fn corrupted_blood_is_a_ten_stack_physical_debuff() {
    let debuff = corrupted_blood_instance(10.0);
    assert_eq!(debuff.max_stacks, 10);
    assert_eq!(debuff.total_dps(), 100.0);
}

#[test]
fn ailment_total_damage_is_dps_times_duration() {
    let instance = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    assert_eq!(
        instance.total_damage(),
        instance.magnitude_dps * instance.duration_secs
    );
}
