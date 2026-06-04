use pobr_core::calc::survivability::{
    block_chance, capped_chance, regen, reservation, suppression_chance,
};

#[test]
fn reservation_combines_flat_and_percent_and_clamps_to_pool() {
    // 50% of 100 + 10 flat = 60 reserved, 40 unreserved.
    let result = reservation(100.0, 10.0, 50.0);
    assert_eq!(result.reserved, 60.0);
    assert_eq!(result.unreserved, 40.0);
}

#[test]
fn reservation_cannot_exceed_pool() {
    let result = reservation(100.0, 0.0, 150.0);
    assert_eq!(result.reserved, 100.0);
    assert_eq!(result.unreserved, 0.0);
}

#[test]
fn regen_combines_flat_percent_and_rate_modifiers() {
    // base = 5 flat + 2% of 1000 = 25; *(1 + 100%/100) * 1 = 50.
    let value = regen(1000.0, 5.0, 2.0, 100.0, 1.0);
    assert_eq!(value, 50.0);
}

#[test]
fn capped_chance_clamps_to_cap() {
    assert_eq!(capped_chance(120.0, 75.0), 75.0);
    assert_eq!(capped_chance(-5.0, 75.0), 0.0);
    assert_eq!(capped_chance(40.0, 75.0), 40.0);
}

#[test]
fn block_caps_at_75_and_suppression_at_100() {
    assert_eq!(block_chance(90.0), 75.0);
    assert_eq!(suppression_chance(120.0), 100.0);
    assert_eq!(suppression_chance(60.0), 60.0);
}
