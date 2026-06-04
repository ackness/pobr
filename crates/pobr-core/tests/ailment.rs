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

#[test]
fn shock_effect_is_clamped_between_5_and_50_percent() {
    assert_eq!(shock_effect(0.0, 1000.0), 0.0);
    let huge = shock_effect(1_000_000.0, 100.0);
    assert!(huge <= 0.50);
    let tiny = shock_effect(1.0, 1_000_000.0);
    assert!(tiny >= 0.05);
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
