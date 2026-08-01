//! DamageComponent vector integration tests: verifies that hit damage is split and
//! aggregated by damage type, that the components sum to the total hit damage, and
//! that the pure-physical path matches the old implementation exactly (regression safe).

use pobr_core::calc::output::OutputTable;
use pobr_core::calc::{MinimalInput, calculate_minimal};
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

/// Finds the component for a given damage type; panics if missing (keeps assertion
/// failures easy to locate).
fn component(
    output: &pobr_core::calc::MinimalOutput,
    ty: DamageType,
) -> &pobr_core::calc::DamageComponent {
    output
        .damage_components
        .iter()
        .find(|c| c.damage_type == ty)
        .unwrap_or_else(|| panic!("missing {ty:?} damage component"))
}

#[test]
fn pure_physical_path_matches_legacy_total_hit_avg() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0).with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // base avg 150 * (1 + 0.5) * 1.2 = 270
    assert_eq!(output.total_hit_avg, 270.0);
    // The only component is physical, so its avg is the total hit damage
    assert_eq!(output.damage_components.len(), 1);
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.avg(), 270.0);
    assert_eq!(phys.min, 100.0 * 1.5 * 1.2);
    assert_eq!(phys.max, 200.0 * 1.5 * 1.2);
}

#[test]
fn physical_component_is_emitted_with_no_modifiers() {
    let db = ModDb::new();
    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    assert_eq!(output.total_hit_avg, 150.0);
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    assert_eq!(phys.avg(), 150.0);
}

#[test]
fn fire_flat_added_contributes_to_total_hit() {
    let mut db = ModDb::new();
    // Flat added fire damage, expressed with separate min/max mods
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 30.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 50.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    assert_eq!(phys.avg(), 150.0);
    assert_eq!(fire.min, 30.0);
    assert_eq!(fire.max, 50.0);
    assert_eq!(fire.avg(), 40.0);
    // Total hit damage = physical avg + fire avg
    assert_eq!(output.total_hit_avg, 150.0 + 40.0);
}

#[test]
fn type_specific_inc_only_scales_its_own_component() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // increased that only applies to fire: scoped via the DamageType tag
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 50.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    // Physical is unaffected by the fire-only inc
    assert_eq!(phys.avg(), 150.0);
    // fire: 100 * (1 + 0.5) = 150
    assert_eq!(fire.avg(), 150.0);
    assert_eq!(output.total_hit_avg, 150.0 + 150.0);
}

#[test]
fn generic_damage_inc_scales_all_components() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // A generic Damage inc should apply to both physical and fire
    db.add_mod(Modifier::number("Damage", ModType::Inc, 100.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    // physical 150 * 2 = 300; fire 100 * 2 = 200
    assert_eq!(phys.avg(), 300.0);
    assert_eq!(fire.avg(), 200.0);
    assert_eq!(output.total_hit_avg, 500.0);
}

/// Crit expected-value test for PoE2 (base_crit_bonus = PLAYER_BASE_CRIT_DAMAGE_BONUS=100,
/// not PoE1's 50).
///
/// PoE2：CritMultiplier BASE 50 → bonus = 100+50=150 → mult = 1+1.5 = 2.5
///         crit_avg_factor = 1 + 0.1*(2.5-1) = 1.15
///         non-crit avg sum = 150+30 = 180 → total_hit_avg = 180*1.15 = 207
#[test]
fn components_sum_equals_total_hit_avg_with_crit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 20.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 40.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    // CritMultiplier BASE 50 → PoE2 bonus = (100+50)/100 = 1.5 → crit_mult = 2.5
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        50.0,
    ));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // The component vector holds the "non-crit" hit components; their sum = 150 + 30 = 180
    let sum_avg: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert_eq!(sum_avg, 180.0);
    // PoE2 total_hit_avg = 180 * (1 + 0.1*(2.5-1)) = 180 * 1.15 = 207
    assert_eq!(output.total_hit_avg, 207.0);
}

#[test]
fn output_table_exposes_damage_components() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 30.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 50.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let table = OutputTable::from(&output);

    assert_eq!(table.damage_components, output.damage_components);
    let phys = table
        .damage_components
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .unwrap();
    let fire = table
        .damage_components
        .iter()
        .find(|c| c.damage_type == DamageType::Fire)
        .unwrap();
    assert_eq!(phys.avg(), 150.0);
    assert_eq!(fire.avg(), 40.0);
}

/// Bug#6 test: the ElementalDamage shared inc group applies to all three elemental components.
///
/// Source: damage-scaling.md §core stacking semantics; PoB2's `typeFlags + modNames`
/// ElementalDamage expansion.
#[test]
fn elemental_damage_inc_scales_all_three_elemental_components() {
    let mut db = ModDb::new();
    // Add 100 flat fire/cold/lightning damage each
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("ColdDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("ColdDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("LightningDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("LightningDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // ElementalDamage INC 50 should apply to all three elements
    db.add_mod(
        Modifier::number("ElementalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let fire = component(&output, DamageType::Fire);
    let cold = component(&output, DamageType::Cold);
    let lightning = component(&output, DamageType::Lightning);
    // each element: 100 * (1 + 0.5) = 150
    assert_eq!(fire.avg(), 150.0);
    assert_eq!(cold.avg(), 150.0);
    assert_eq!(lightning.avg(), 150.0);
    // Physical is unaffected by ElementalDamage
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.avg(), 150.0); // base_input: 100-200 avg 150, no scaler
}

/// Bug#7 test: DAMAGE_TYPES follows the PoE2 conversion-chain order
/// Phys→Lightning→Cold→Fire→Chaos.
///
/// Source: damage-scaling.md §conversion order, PoB2 CalcOffence.lua `dmgTypeList`.
#[test]
fn damage_types_array_follows_poe2_conversion_chain_order() {
    use pobr_core::calc::damage::DAMAGE_TYPES;
    assert_eq!(DAMAGE_TYPES[0], DamageType::Physical);
    assert_eq!(DAMAGE_TYPES[1], DamageType::Lightning);
    assert_eq!(DAMAGE_TYPES[2], DamageType::Cold);
    assert_eq!(DAMAGE_TYPES[3], DamageType::Fire);
    assert_eq!(DAMAGE_TYPES[4], DamageType::Chaos);
}

/// Bug#8 test: AddedDamage MORE only applies to external flat added damage, not to the
/// weapon's own base damage.
///
/// Source: damage-scaling.md §Added Damage Effectiveness;
///       PoB2 `addedMin * addedMult` excludes `source[...]` (the weapon's/skill's own base).
#[test]
fn added_damage_effectiveness_only_scales_flat_added_not_weapon_base() {
    let mut db = ModDb::new();
    // AddedDamage MORE 200 (200% effectiveness = ×3.0 ... MORE 200 → factor = 1+200/100 = 3.0)
    db.add_mod(Modifier::number("AddedDamage", ModType::More, 200.0).with_flags(ModFlags::ATTACK));
    // Add 10 flat fire damage
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let fire = component(&output, DamageType::Fire);
    // fire flat=10 * eff_factor=3.0 = 30 avg
    assert_eq!(fire.avg(), 30.0);

    // Physical base (from the weapon's base_hit_min/max) is unaffected by AddedDamage MORE
    // flat added physical=0 → phys = (base_hit + 0*eff) scaled = base_hit * scale (no INC/MORE)
    let phys = component(&output, DamageType::Physical);
    // PhysicalDamage MORE = 3.0 also applies to phys base? NO: AddedDamage MORE only
    // multiplies added damage
    // phys base = (100 + 0*3) = 100 min, (200+0*3) = 200 max → avg 150
    assert_eq!(phys.avg(), 150.0);
}

/// Test: addedMult includes an INC leg — vendor `calcLib.mod` (CalcTools.lua:16-18) =
/// `(1 + Sum(INC, "Added<Type>Damage", "AddedDamage")/100) × More(...)`.
///
/// Source: PoB2 CalcOffence.lua:3909 + CalcTools.lua:16-18.
#[test]
fn added_damage_effectiveness_includes_inc_leg() {
    let mut db = ModDb::new();
    // INC leg: AddedDamage INC 50 + AddedFireDamage INC 30 → (1 + 80/100) = 1.8
    db.add_mod(Modifier::number("AddedDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(
        Modifier::number("AddedFireDamage", ModType::Inc, 30.0).with_flags(ModFlags::ATTACK),
    );
    // MORE leg: AddedDamage MORE 100 → ×2.0; combined addedMult = 1.8 × 2.0 = 3.6
    db.add_mod(Modifier::number("AddedDamage", ModType::More, 100.0).with_flags(ModFlags::ATTACK));
    // Add 10 flat fire damage
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let fire = component(&output, DamageType::Fire);
    // fire flat=10 × addedMult=3.6 = 36 avg
    assert_eq!(fire.avg(), 36.0);

    // The INC leg likewise does not multiply the weapon's own base: physical remains
    // (100,200) → avg 150.
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.avg(), 150.0);
}

/// Test: type-specific `Added<Type>Damage` INC only applies to the flat added damage of
/// the matching type.
#[test]
fn added_type_damage_inc_is_type_scoped() {
    let mut db = ModDb::new();
    // Only AddedFireDamage INC 100: fire ×2.0, cold unaffected.
    db.add_mod(
        Modifier::number("AddedFireDamage", ModType::Inc, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("ColdDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("ColdDamageMax", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    assert_eq!(component(&output, DamageType::Fire).avg(), 20.0);
    assert_eq!(component(&output, DamageType::Cold).avg(), 10.0);
}

// 04-01: Min<Type>Damage / Max<Type>Damage have independent MORE buckets for min/max
// (PoB2 CalcOffence.lua:138-139,153-154)

#[test]
fn min_max_type_more_scales_only_one_end() {
    // "35% less minimum Physical Damage" → MinPhysicalDamage MORE -35
    // "35% more maximum Physical Damage" → MaxPhysicalDamage MORE +35
    //   min = round(100 * (1 - 0.35)) = 65; max = round(200 * (1 + 0.35)) = 270
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("MinPhysicalDamage", ModType::More, -35.0));
    db.add_mod(Modifier::number("MaxPhysicalDamage", ModType::More, 35.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 65.0, "min is only scaled by MinPhysicalDamage");
    assert_eq!(phys.max, 270.0, "max is only scaled by MaxPhysicalDamage");
    assert_eq!(phys.avg(), 167.5);
}

#[test]
fn min_max_type_more_stacks_with_generic_inc_more() {
    // Generic PhysicalDamage INC 50% → scale=1.5; MaxPhysicalDamage MORE +10% only
    // multiplies max.
    //   min = round(100 * 1.5) = 150; max = round(200 * 1.5 * 1.10) = 330
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("MaxPhysicalDamage", ModType::More, 10.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 150.0);
    assert_eq!(phys.max, 330.0);
}
