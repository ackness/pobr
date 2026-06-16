use pobr_core::calc::damage::{
    DamageComponent, convert_damage, gain_as_extra, normalize_conversion, sum_avg,
};
use pobr_data::prelude::*;

fn phys(min: f64, max: f64) -> DamageComponent {
    DamageComponent::new(DamageType::Physical, min, max)
}

#[test]
fn convert_moves_damage_from_source_to_target() {
    let components = vec![phys(100.0, 200.0)];
    let result = convert_damage(&components, DamageType::Physical, DamageType::Fire, 0.5);

    let physical = result
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .unwrap();
    let fire = result
        .iter()
        .find(|c| c.damage_type == DamageType::Fire)
        .unwrap();

    assert_eq!(physical.min, 50.0);
    assert_eq!(physical.max, 100.0);
    assert_eq!(fire.min, 50.0);
    assert_eq!(fire.max, 100.0);
}

#[test]
fn gain_as_extra_keeps_the_source_intact() {
    let components = vec![phys(100.0, 200.0)];
    let result = gain_as_extra(
        &components,
        DamageType::Physical,
        DamageType::Lightning,
        0.25,
    );

    let physical = result
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .unwrap();
    let lightning = result
        .iter()
        .find(|c| c.damage_type == DamageType::Lightning)
        .unwrap();

    // source unchanged.
    assert_eq!(physical.min, 100.0);
    assert_eq!(physical.max, 200.0);
    // extra is 25% of source.
    assert_eq!(lightning.min, 25.0);
    assert_eq!(lightning.max, 50.0);
}

#[test]
fn normalize_conversion_scales_overflow_to_one() {
    let normalized = normalize_conversion(&[1.0, 0.5]);
    let total: f64 = normalized.iter().sum();
    assert!((total - 1.0).abs() < 1e-9);
    // proportions preserved: first is twice the second.
    assert!((normalized[0] - 2.0 * normalized[1]).abs() < 1e-9);
}

#[test]
fn normalize_conversion_leaves_under_100_percent_untouched() {
    let normalized = normalize_conversion(&[0.3, 0.4]);
    assert_eq!(normalized, vec![0.3, 0.4]);
}

#[test]
fn sum_avg_totals_component_averages() {
    let components = vec![
        phys(100.0, 200.0),
        DamageComponent::new(DamageType::Fire, 0.0, 100.0),
    ];
    // 150 + 50 = 200.
    assert_eq!(sum_avg(&components), 200.0);
}
