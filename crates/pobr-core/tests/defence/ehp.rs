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

/// Bug#10 test: Chaos Inoculation build EHP.
///
/// CI: maximum life becomes 1, ES is used as the life pool, and chaos immunity is granted.
/// Source: agent-docs/active-defences.md §5 Keystone table.
#[test]
fn chaos_inoculation_uses_es_as_life_pool_and_grants_chaos_immunity() {
    use pobr_core::calc::ehp::{EhpOptions, ResistanceSuite, calc_ehp_with_opts};

    let resistances = ResistanceSuite {
        physical_pdr: 0.0,
        fire: 0.0,
        cold: 0.0,
        lightning: 0.0,
        chaos: 0.0, // ignored under CI (immune)
    };
    let ci_opts = EhpOptions {
        chaos_inoculation: true,
        ..EhpOptions::default()
    };
    // life=1 (CI keystone), es=5000
    let result = calc_ehp_with_opts(1.0, 5000.0, 0.0, &resistances, 0.0, 1000.0, ci_opts);

    // ES is used as the main pool, ele pool = 5000 (life=es=5000, effective_es=0)
    assert_eq!(result.fire_max_hit, 5000.0);
    assert_eq!(result.physical_max_hit, 5000.0);
    // Chaos immune → infinite
    assert!(result.chaos_max_hit.is_infinite());
    // total_ehp = min(ele types) = 5000
    assert_eq!(result.total_ehp, 5000.0);
}

#[test]
fn non_ci_chaos_uses_life_plus_half_es_pool() {
    use pobr_core::calc::ehp::{EhpOptions, ResistanceSuite, calc_ehp_with_opts};

    let resistances = ResistanceSuite::default();
    let normal_opts = EhpOptions::default();
    let result = calc_ehp_with_opts(1000.0, 1000.0, 0.0, &resistances, 0.0, 1000.0, normal_opts);

    // chaos pool = life + es*0.5 = 1000 + 500 = 1500
    assert_eq!(result.chaos_max_hit, 1500.0);
}

/// 06-04: the damage reduction cap is variable (`+Maximum Damage Reduction`). Default 0.9;
/// raising it to 0.95 doubles max hit.
#[test]
fn damage_reduction_cap_raised_by_mod() {
    use pobr_core::calc::ehp::{
        physical_max_hit_overwhelm_cap, physical_taken_fraction_overwhelm,
        physical_taken_fraction_overwhelm_cap,
    };
    let frac_default = physical_taken_fraction_overwhelm_cap(0.95, 0.0, 1000.0, 0.0, 0.9);
    assert!(
        (frac_default - 0.1).abs() < 1e-9,
        "default cap 0.9 → taken 0.1"
    );
    let frac_raised = physical_taken_fraction_overwhelm_cap(0.95, 0.0, 1000.0, 0.0, 0.95);
    assert!(
        (frac_raised - 0.05).abs() < 1e-9,
        "raised cap 0.95 → taken 0.05"
    );

    let mh_default = physical_max_hit_overwhelm_cap(1000.0, 0.95, 0.0, 1000.0, 0.0, 0.9);
    let mh_raised = physical_max_hit_overwhelm_cap(1000.0, 0.95, 0.0, 1000.0, 0.0, 0.95);
    assert!(
        (mh_default - 10000.0).abs() < 1.0,
        "default ~10000, got {mh_default}"
    );
    assert!(
        (mh_raised - 20000.0).abs() < 1.0,
        "raised ~20000, got {mh_raised}"
    );

    // The legacy-signature wrapper is equivalent to dr_max=0.9.
    let legacy = physical_taken_fraction_overwhelm(0.95, 0.0, 1000.0, 0.0);
    assert!(
        (legacy - frac_default).abs() < 1e-9,
        "legacy wrapper == cap 0.9"
    );
}
