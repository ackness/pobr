//! Hand-computed unit tests for LuckyHits average rolls and canDeal / DealNo<Type> gating.
//! vendor cross-reference: CalcOffence.lua:4036-4046 (lucky), :2226-2230 (canDeal).

use pobr_core::calc::{DamageComponent, apply_can_deal, convert_damage, lucky_hit_chance};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const EPS: f64 = 1e-9;

// ---------------------------------------------------------------- the avg function family

/// Hand calc: (min,max)=(10,100) — lucky avg = 70 vs. normal 55.
#[test]
fn lucky_avg_hand_calc() {
    let comp = DamageComponent::new(DamageType::Physical, 10.0, 100.0);
    // p=0 matches the old avg() bit for bit (an equivalence anchor).
    assert!((comp.avg_with_lucky(0.0) - comp.avg()).abs() < EPS);
    assert!((comp.avg() - 55.0).abs() < EPS);
    // p=1: min/3 + 2max/3 = 10/3 + 200/3 = 70.
    assert!((comp.avg_with_lucky(1.0) - 70.0).abs() < EPS);
    // p=0.5: 55x0.5 + 70x0.5 = 62.5.
    assert!((comp.avg_with_lucky(0.5) - 62.5).abs() < EPS);
    // out-of-range clamping.
    assert!((comp.avg_with_lucky(1.5) - 70.0).abs() < EPS);
    assert!((comp.avg_with_lucky(-0.5) - 55.0).abs() < EPS);
}

/// The `LuckyHits` flag: every pass x every damage type is always lucky (p=1).
#[test]
fn lucky_hits_flag_applies_everywhere() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("LuckyHits"));
    let cfg = CalcConfig::attack();
    for crit in [true, false] {
        for ty in [
            DamageType::Physical,
            DamageType::Lightning,
            DamageType::Chaos,
        ] {
            assert!((lucky_hit_chance(&db, &cfg, ty, crit) - 1.0).abs() < EPS);
        }
    }
}

/// `CritLucky` only affects the crit pass.
#[test]
fn crit_lucky_only_affects_crit_pass() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("CritLucky"));
    let cfg = CalcConfig::attack();
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Physical, true) - 1.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Physical, false) - 0.0).abs() < EPS);
}

/// `LightningNoCritLucky` applies only to the non-crit pass and only the Lightning component.
#[test]
fn lightning_no_crit_lucky_scope() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("LightningNoCritLucky"));
    let cfg = CalcConfig::attack();
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, false) - 1.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, true) - 0.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Cold, false) - 0.0).abs() < EPS);
}

/// `ElementalLuckHits` covers only the three elements (Lightning/Cold/Fire);
/// Physical/Chaos don't get it.
#[test]
fn elemental_luck_hits_only_elements() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("ElementalLuckHits"));
    let cfg = CalcConfig::attack();
    for ty in [DamageType::Lightning, DamageType::Cold, DamageType::Fire] {
        assert!((lucky_hit_chance(&db, &cfg, ty, false) - 1.0).abs() < EPS);
    }
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Physical, false) - 0.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Chaos, false) - 0.0).abs() < EPS);
}

/// The Sum channel: `<Type>LuckyHitsChance + LuckyHitsChance` are summed, capped at
/// 100, and divided by 100 into a fraction.
#[test]
fn lucky_chance_sums_typed_and_generic_with_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "LightningLuckyHitsChance",
        ModType::Base,
        30.0,
    ));
    db.add_mod(Modifier::number("LuckyHitsChance", ModType::Base, 20.0));
    let cfg = CalcConfig::attack();
    // Lightning: 30 + 20 = 50 -> 0.5; Cold: only the generic 20 -> 0.2.
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, false) - 0.5).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Cold, false) - 0.2).abs() < EPS);

    // cap 100: add 90 more -> 30+20+90=140 -> capped at 100 -> 1.0.
    db.add_mod(Modifier::number("LuckyHitsChance", ModType::Base, 90.0));
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, false) - 1.0).abs() < EPS);
}

// ---------------------------------------------------------------- canDeal

/// A specific composed scenario (the Avatar of Fire shape): after converting 50%
/// physical to fire, `DealNoPhysical` zeroes the leftover physical but keeps the
/// already-converted fire (conversion happens first; only the post-conversion
/// leftover gets zeroed).
#[test]
fn avatar_of_fire_zeroes_residual_physical_keeps_converted_fire() {
    let components = vec![DamageComponent::new(DamageType::Physical, 100.0, 200.0)];
    let mut converted = convert_damage(&components, DamageType::Physical, DamageType::Fire, 0.5);

    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("DealNoPhysical"));
    apply_can_deal(&mut converted, &db, &CalcConfig::attack());

    let phys = converted
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .unwrap();
    assert!((phys.min - 0.0).abs() < EPS);
    assert!((phys.max - 0.0).abs() < EPS);
    let fire = converted
        .iter()
        .find(|c| c.damage_type == DamageType::Fire)
        .unwrap();
    assert!((fire.min - 50.0).abs() < EPS);
    assert!((fire.max - 100.0).abs() < EPS);
}

/// `DealNoDamage` zeroes every damage type (the generic entry among the 5+1 list).
#[test]
fn deal_no_damage_zeroes_all_components() {
    let mut components = vec![
        DamageComponent::new(DamageType::Physical, 10.0, 20.0),
        DamageComponent::new(DamageType::Fire, 30.0, 40.0),
        DamageComponent::new(DamageType::Chaos, 50.0, 60.0),
    ];
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("DealNoDamage"));
    apply_can_deal(&mut components, &db, &CalcConfig::attack());
    for component in &components {
        assert!((component.min - 0.0).abs() < EPS);
        assert!((component.max - 0.0).abs() < EPS);
    }
}

/// With no DealNo* flags, components are unchanged bit for bit (an equivalence
/// anchor that the wiring introduces zero behavior change).
#[test]
fn no_flags_leaves_components_unchanged() {
    let original = vec![
        DamageComponent::new(DamageType::Physical, 10.0, 20.0),
        DamageComponent::new(DamageType::Cold, 5.0, 15.0),
    ];
    let mut components = original.clone();
    apply_can_deal(&mut components, &ModDb::new(), &CalcConfig::attack());
    assert_eq!(components, original);
}

/// Per-type gating doesn't cross-contaminate: `DealNoCold` only clears Cold, other
/// types are kept.
#[test]
fn per_type_gating_is_independent() {
    let mut components = vec![
        DamageComponent::new(DamageType::Physical, 10.0, 20.0),
        DamageComponent::new(DamageType::Cold, 5.0, 15.0),
        DamageComponent::new(DamageType::Fire, 7.0, 9.0),
    ];
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("DealNoCold"));
    apply_can_deal(&mut components, &db, &CalcConfig::attack());
    assert!((components[0].min - 10.0).abs() < EPS);
    assert!((components[1].min - 0.0).abs() < EPS);
    assert!((components[1].max - 0.0).abs() < EPS);
    assert!((components[2].max - 9.0).abs() < EPS);
}
