use pobr_core::calc::stat_boundary::stat_boundary;
use pobr_data::prelude::*;

#[test]
fn resistance_boundary_caps_at_default_max_and_reports_over_cap() {
    let spec = BoundarySpec::resistance();
    // uncapped 90, no bonus → max 75, final 75, over 15, missing 0.
    let result = stat_boundary(90.0, 0.0, &spec);

    assert_eq!(result.max, 75.0);
    assert_eq!(result.final_value, 75.0);
    assert_eq!(result.over_cap, 15.0);
    assert_eq!(result.missing, 0.0);
}

#[test]
fn max_bonus_raises_the_cap_up_to_hard_cap() {
    let spec = BoundarySpec::resistance();
    // +30 bonus → 105 but hard-capped at 90.
    let result = stat_boundary(100.0, 30.0, &spec);

    assert_eq!(result.max, 90.0);
    assert_eq!(result.final_value, 90.0);
    assert_eq!(result.over_cap, 10.0);
}

#[test]
fn below_cap_reports_missing_and_zero_over_cap() {
    let spec = BoundarySpec::resistance();
    let result = stat_boundary(60.0, 0.0, &spec);

    assert_eq!(result.final_value, 60.0);
    assert_eq!(result.over_cap, 0.0);
    assert_eq!(result.missing, 15.0);
}

#[test]
fn floor_clamps_negative_values_when_specified() {
    let spec = BoundarySpec::new(Some(-200.0), Some(75.0), Some(90.0));
    let result = stat_boundary(-500.0, 0.0, &spec);

    assert_eq!(result.final_value, -200.0);
}

#[test]
fn no_max_constraint_yields_infinite_max_and_zero_over_cap() {
    let spec = BoundarySpec::new(None, None, None);
    let result = stat_boundary(1234.0, 0.0, &spec);

    assert!(result.max.is_infinite());
    assert_eq!(result.final_value, 1234.0);
    assert_eq!(result.over_cap, 0.0);
}
