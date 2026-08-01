//! End-to-end integration tests for the damage conversion chain (wired into the
//! `calculate_components` pipeline).
//!
//! Covers the gaps: conversion-chain-not-wired, double-dip-accumulated-type-flags,
//! hit-dot-split-missing-in-damagecomponent, gain-as-extra.
//!
//! Reference: PoB2 `CalcOffence.lua` (`processDamageConversion` / `calcConvertedDamage`
//! / `buildGainTable` / `calcGainedDamage`), agent-docs/damage-scaling.md §damage conversion.

use pobr_core::calc::damage::DAMAGE_TYPES;
use pobr_core::calc::{DamageComponent, MinimalInput, calculate_minimal};
use pobr_core::{CalcConfig, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn base_input() -> MinimalInput {
    MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
    }
}

fn component(output: &pobr_core::calc::MinimalOutput, ty: DamageType) -> &DamageComponent {
    output
        .damage_components
        .iter()
        .find(|c| c.damage_type == ty)
        .unwrap_or_else(|| panic!("missing {ty:?} damage component"))
}

fn find(output: &pobr_core::calc::MinimalOutput, ty: DamageType) -> Option<&DamageComponent> {
    output
        .damage_components
        .iter()
        .find(|c| c.damage_type == ty)
}

/// 50% Phys→Fire conversion: physical keeps 50%, fire gets 50%.
/// Verifies the basic transfer is correct and the fire component's `type_path` contains [Physical, Fire].
#[test]
fn convert_50_percent_phys_to_fire_splits_base() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    // base 100-200 → 50% each
    assert_eq!(phys.min, 50.0);
    assert_eq!(phys.max, 100.0);
    assert_eq!(fire.min, 50.0);
    assert_eq!(fire.max, 100.0);
    // The fire component carries the set of types along the phys→fire path.
    assert!(fire.type_path.contains(&DamageType::Physical));
    assert!(fire.type_path.contains(&DamageType::Fire));
}

/// PoE2 semantics (no conversion-source double-dip): after a 50% Phys→Fire conversion, the
/// fire component only takes **final-type** FireDamage inc and ElementalDamage inc — it does
/// **not** take the conversion source's PhysicalDamage inc. Primary source: PoB2
/// `CalcOffence.lua` `calcDamage(..., damageType, 0)` (:3990, typeFlags passed as 0, i.e. only
/// the final type) + headless oracle verification per component.
#[test]
fn converted_fire_uses_final_type_inc_only_no_conversion_source_double_dip() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );
    // PhysicalDamage inc 100: both the physical component and the converted fire component take it.
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Physical)),
    );
    // FireDamage inc 100: only the fire component takes it.
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );
    // ElementalDamage inc 50: the fire component takes it.
    db.add_mod(
        Modifier::number("ElementalDamage", ModType::Inc, 50.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // Physical component: base 50-100, takes PhysicalDamage inc 100 → ×2.0.
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);

    // Fire component: base 50-100, inc = Fire100 + Elem50 = 150 (**excludes** the conversion
    // source's Phys100) → ×2.5. min 50*2.5 = 125, max 100*2.5 = 250.
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 125.0);
    assert_eq!(fire.max, 250.0);
}

/// Over-100% conversion is normalized: 100% Phys→Fire + 50% Phys→Cold normalizes to
/// ~67% Fire / ~33% Cold. Physical is fully converted away (0 retained).
#[test]
fn over_100_percent_conversion_normalizes_to_one() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 100.0)
            .with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToCold", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // Physical is fully converted away (0 retained) → no physical component, or it's 0.
    let phys = find(&output, DamageType::Physical);
    if let Some(p) = phys {
        assert!(p.min.abs() < 1e-6 && p.max.abs() < 1e-6, "phys should be 0");
    }
    // fire = 100/150 = 2/3, cold = 50/150 = 1/3 (no inc).
    let fire = component(&output, DamageType::Fire);
    let cold = component(&output, DamageType::Cold);
    // base avg 150 → fire avg ~100, cold avg ~50.
    assert!(
        (fire.avg() - 100.0).abs() < 0.5,
        "fire avg ~100, got {}",
        fire.avg()
    );
    assert!(
        (cold.avg() - 50.0).abs() < 0.5,
        "cold avg ~50, got {}",
        cold.avg()
    );
    // Total damage is conserved after conversion (~ base avg 150).
    let total: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert!(
        (total - 150.0).abs() < 0.5,
        "total conserved ~150, got {total}"
    );
}

/// gain-as-extra: 25% Phys gain as Lightning — the physical source is **not reduced**,
/// a lightning component is added on top.
#[test]
fn gain_as_extra_does_not_reduce_source() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsLightning", ModType::Base, 25.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // The physical source stays unchanged (gain doesn't reduce it).
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    // Extra lightning = 25% of physical.
    let lightning = component(&output, DamageType::Lightning);
    assert_eq!(lightning.min, 25.0);
    assert_eq!(lightning.max, 50.0);
    // The lightning component's type_path includes the Physical source (used only for
    // attribution/display; inc aggregation goes by final type only, no double-dip).
    assert!(lightning.type_path.contains(&DamageType::Physical));
    assert!(lightning.type_path.contains(&DamageType::Lightning));
    // Total damage = physical 150 + lightning 37.5 = 187.5 (gain is a net addition).
    let total: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert_eq!(total, 187.5);
}

/// PoE2 semantics: after physical gain-as-fire, the extra fire component only takes
/// **final-type** FireDamage inc, **not** the source PhysicalDamage inc (same semantics as
/// conversion — PoB2 calcDamage's typeFlags contains only the final type; oracle-verified).
#[test]
fn gain_as_extra_fire_uses_final_type_inc_only() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Physical)),
    );
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // Physical: base 100-200 not reduced, takes PhysicalDamage inc 100 → ×2 = 200-400.
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 200.0);
    assert_eq!(phys.max, 400.0);
    // Extra fire: 50% of physical base = 50-100, inc = Fire100 (**excludes** source Phys100) → ×2 = 100-200.
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 100.0);
    assert_eq!(fire.max, 200.0);
}

/// Regression (type_path-last-not-final-type): when a component's `type_path` contains a
/// type that comes **later in chain order** than the component's own type, inc/more
/// aggregation must key on the component's own `damage_type`, not the last entry in `type_path`.
///
/// Scenario: physical 100% gain as Cold (the Cold component's path starts with Physical),
/// then stack Fire gain as Cold on top (pushing Fire into Cold's path). Under chain order
/// [Phys, Lightning, Cold, Fire], the last entry in Cold's sorted path is Fire. The old
/// implementation mistakenly aggregated the Cold component using `FireDamage` inc; the fix
/// must use `ColdDamage` instead. PoB2 `calcDamage`'s typeFlags contains only the final
/// damageType — verified per component against the ice-shot/deadeye oracle.
#[test]
fn component_uses_own_type_inc_not_path_last() {
    let mut db = ModDb::new();
    // Physical 100% gain as Cold (Cold gets the physical base, path includes Physical).
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsCold", ModType::Base, 100.0)
            .with_flags(ModFlags::ATTACK),
    );
    // Flat fire base (gives Fire→Cold gain a source, pushing Fire into Cold's type_path).
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 20.0).with_flags(ModFlags::ATTACK));
    // Fire 100% gain as Cold — pushes Fire into Cold's type_path.
    db.add_mod(
        Modifier::number("FireDamageGainAsCold", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // ColdDamage inc 100: Cold only takes this if aggregation keys on the component's own type.
    db.add_mod(
        Modifier::number("ColdDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Cold)),
    );
    // FireDamage inc 999: if the buggy path-last logic is used (Fire), Cold gets polluted by this huge value.
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 999.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let cold = component(&output, DamageType::Cold);
    // Cold base = physical gain(100-200) + fire gain(10-20) = 110-220, takes ColdDamage inc 100
    // → ×2 = 220-440. If the buggy path-last FireDamage inc 999 were used → ×10.99 (assertion would fail).
    assert!(cold.type_path.contains(&DamageType::Fire), "path 应含 Fire");
    assert_eq!(
        cold.min, 220.0,
        "Cold 须按 ColdDamage inc 聚合，非 path 末位 Fire"
    );
    assert_eq!(cold.max, 440.0);
}

/// Enriched fields: components produced by conversion / gain have kind=Hit, source=Attack;
/// type_path is correctly deduplicated and ordered.
#[test]
fn converted_components_carry_hit_attack_kind_and_ordered_path() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 100.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let fire = component(&output, DamageType::Fire);

    assert_eq!(fire.kind, DamageKind::Hit);
    assert_eq!(fire.source, DamageSource::Attack);
    // type_path follows DAMAGE_TYPES chain order: Physical(0) before Fire(3).
    let idx_phys = DAMAGE_TYPES
        .iter()
        .position(|t| *t == DamageType::Physical)
        .unwrap();
    let idx_fire = DAMAGE_TYPES
        .iter()
        .position(|t| *t == DamageType::Fire)
        .unwrap();
    let path_phys = fire
        .type_path
        .iter()
        .position(|t| *t == DamageType::Physical)
        .unwrap();
    let path_fire = fire
        .type_path
        .iter()
        .position(|t| *t == DamageType::Fire)
        .unwrap();
    assert!(idx_phys < idx_fire);
    assert!(path_phys < path_fire, "type_path must follow chain order");
}

/// Regression: with no conversion / gain modifiers at all, the components match the legacy
/// output verbatim (pure physical + additional fire).
#[test]
fn no_conversion_path_matches_legacy_output_verbatim() {
    // Pure physical.
    let db = ModDb::new();
    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    assert_eq!(output.damage_components.len(), 1);
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    assert_eq!(phys.type_path, vec![DamageType::Physical]);

    // Additional flat fire + elemental inc, no conversion.
    let mut db2 = ModDb::new();
    db2.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db2.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db2.add_mod(
        Modifier::number("ElementalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );
    let output2 = calculate_minimal(&db2, &CalcConfig::attack(), &base_input());
    // Fire 100 * 1.5 = 150 (matches the historical assertion in damage_components.rs).
    let fire = component(&output2, DamageType::Fire);
    assert_eq!(fire.avg(), 150.0);
    assert_eq!(fire.type_path, vec![DamageType::Fire]);
    // Physical is unaffected by the elemental inc.
    let phys2 = component(&output2, DamageType::Physical);
    assert_eq!(phys2.avg(), 150.0);
}

/// Skill conversion runs before global conversion: skill 50% Phys→Cold, chained into global
/// 50% Cold→Fire. Physical keeps 50%, and 50% of the resulting Cold converts again to Fire,
/// giving Cold 25% and Fire 25%.
#[test]
fn skill_conversion_chains_into_global_conversion() {
    let mut db = ModDb::new();
    // Skill: 50% Phys→Cold.
    db.add_mod(
        Modifier::number("SkillPhysicalDamageConvertToCold", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );
    // Global: 50% Cold→Fire (applies to the Cold produced by the skill conversion).
    db.add_mod(
        Modifier::number("ColdDamageConvertToFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // base avg 150. phys keeps 50% → 75; cold 50%*keeps 50% → 37.5; fire 50%*converts 50% → 37.5.
    let phys = component(&output, DamageType::Physical);
    let cold = component(&output, DamageType::Cold);
    let fire = component(&output, DamageType::Fire);
    assert!(
        (phys.avg() - 75.0).abs() < 0.5,
        "phys ~75, got {}",
        phys.avg()
    );
    assert!(
        (cold.avg() - 37.5).abs() < 0.5,
        "cold ~37.5, got {}",
        cold.avg()
    );
    assert!(
        (fire.avg() - 37.5).abs() < 0.5,
        "fire ~37.5, got {}",
        fire.avg()
    );
    // Fire's type_path goes through the Physical→Cold→Fire chain, so it should contain all three.
    assert!(fire.type_path.contains(&DamageType::Physical));
    assert!(fire.type_path.contains(&DamageType::Cold));
    assert!(fire.type_path.contains(&DamageType::Fire));
}

/// Random-element gain folding (vendor CalcOffence.lua:1175-1200: `DamageGainAsRandom BASE n`,
/// under physMode=AVERAGE (the default configInput), expands to `DamageGainAs{Fire,Cold,Lightning}
/// BASE n/3`; PoBR folds it the same way in build_gain_matrix — instanced by the Relentless
/// Vindicator tree node from the druid-oracle ember-fusillade case).
#[test]
fn random_element_gain_as_splits_average_across_elements() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("DamageGainAsRandom", ModType::Base, 30.0).with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // Physical source 100-200 not reduced; each of the three elements gains 10% (30/3).
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    for ty in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
        let c = component(&output, ty);
        assert_eq!(c.min, 10.0, "{ty:?} min = 100 × 10%");
        assert_eq!(c.max, 20.0, "{ty:?} max = 200 × 10%");
    }
    assert!(find(&output, DamageType::Chaos).is_none_or(|c| c.avg() == 0.0));
}

/// PhysicalDamageGainAsRandom only applies to the physical-source line (vendor expands it to
/// `PhysicalDamageGainAs<Elem>`; CalcOffence.lua:1193-1200).
#[test]
fn physical_random_gain_as_only_from_physical_source() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsRandom", ModType::Base, 30.0)
            .with_flags(ModFlags::ATTACK),
    );
    // Non-physical source: flat fire addition of 60-60 (should not be amplified again by phys-random).
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 60.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 60.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // fire = flat 60 + phys 100×10% = 70 / 60 + 200×10% = 80.
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 70.0);
    assert_eq!(fire.max, 80.0);
    // cold/lightning come only from the 10% physical-source gain.
    let cold = component(&output, DamageType::Cold);
    assert_eq!(cold.min, 10.0);
    assert_eq!(cold.max, 20.0);
}
