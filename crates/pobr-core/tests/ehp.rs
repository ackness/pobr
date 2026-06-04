use pobr_core::calc::ehp::{ResistanceSuite, calc_ehp, max_hit_for_type, physical_max_hit};

#[test]
fn max_hit_for_type_scales_pool_by_resistance() {
    // 75% resist → take 25% → max hit = pool / 0.25 = 4x pool.
    assert_eq!(max_hit_for_type(1000.0, 75.0), 4000.0);
    // 0% resist → max hit == pool.
    assert_eq!(max_hit_for_type(1000.0, 0.0), 1000.0);
}

#[test]
fn full_immunity_yields_infinite_max_hit() {
    assert!(max_hit_for_type(1000.0, 100.0).is_infinite());
}

#[test]
fn physical_max_hit_accounts_for_armour_and_pdr() {
    // No mitigation: take 100% → max hit == pool.
    let bare = physical_max_hit(1000.0, 0.0, 0.0, 1000.0);
    assert_eq!(bare, 1000.0);

    // With armour the max hit should exceed bare pool (more mitigation).
    let armoured = physical_max_hit(1000.0, 0.0, 5000.0, 1000.0);
    assert!(armoured > bare);
}

#[test]
fn total_ehp_takes_the_lowest_type_max_hit() {
    let resistances = ResistanceSuite {
        physical_pdr: 0.0,
        fire: 75.0,
        cold: 0.0, // cold is the weak point.
        lightning: 75.0,
        chaos: 0.0,
    };
    let result = calc_ehp(1000.0, 0.0, 0.0, &resistances, 0.0, 1000.0);

    // cold has 0 resist → cold_max_hit == pool == 1000, the lowest finite.
    assert_eq!(result.cold_max_hit, 1000.0);
    assert_eq!(result.total_ehp, 1000.0);
}

#[test]
fn energy_shield_adds_to_elemental_pool_but_half_for_chaos() {
    let resistances = ResistanceSuite::default();
    let result = calc_ehp(1000.0, 1000.0, 0.0, &resistances, 0.0, 1000.0);

    // Elemental pool = life + es = 2000 (0% resist → max hit 2000).
    assert_eq!(result.fire_max_hit, 2000.0);
    // Chaos pool = life + es*0.5 = 1500.
    assert_eq!(result.chaos_max_hit, 1500.0);
}
