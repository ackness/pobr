//! Integration tests for the crit/non-crit dual-pass pipeline.
//!
//! I5 equivalence (no CriticalStrike-conditional mods → short-circuits to the old
//! single-factor formula, mathematically identical to
//! `blend(c, x×m, x) == x×crit.effect`) + crit-leg-only mods amplify just the crit leg
//! + Stored-family values. Vendor reference: CalcOffence.lua:3978-4057 / :4395.

use pobr_core::calc::{MinimalInput, calculate_minimal};
use pobr_core::{CalcConfig, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn input() -> MinimalInput {
    MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    }
}

fn crit_db() -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        40.0,
    ));
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        100.0, // total crit multiplier = 1 + (100+100)/100 = 3.0
    ));
    db
}

/// I5: with no CriticalStrike-conditional mods, total_hit_avg == non-crit average ×
/// crit.effect (the old single-factor formula; the short-circuit path replicates the
/// same rounding order — byte-for-byte equivalence is proven by the full workspace
/// regression suite, and this test explicitly asserts the mathematical identity).
#[test]
fn i5_no_crit_conditional_mods_equals_single_factor_formula() {
    let db = crit_db();
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());

    // Panel figures without any on-hit degradation: c = 0.40, m = 3.0, effect = 1 - c + c×m = 1.8.
    let non_crit_avg = 150.0;
    let effect = 1.0 - out.crit_chance + out.crit_chance * out.crit_multiplier;
    assert!((out.crit_chance - 0.40).abs() < 1e-9);
    assert!((out.crit_multiplier - 3.0).abs() < 1e-9);
    assert!(
        (out.total_hit_avg - non_crit_avg * effect).abs() < 1e-9,
        "I5 identity: got {}",
        out.total_hit_avg
    );

    // Stored family (player-side, pre-resist): crit leg = hit leg × m; combined = weighted average.
    let (_, hit_avg) = out.stored_hit_avg[0];
    let (_, crit_avg) = out.stored_crit_avg[0];
    let (_, combined) = out.stored_combined_avg[0];
    assert!((hit_avg - 150.0).abs() < 1e-9);
    assert!((crit_avg - 450.0).abs() < 1e-9);
    assert!(
        (combined - (450.0 * 0.40 + 150.0 * 0.60)).abs() < 1e-9,
        "StoredCombinedAvg = crit×c + hit×(1−c)，got {combined}"
    );
}

/// A crit-leg-only mod (of the `increased Damage on Critical Hit` shape) amplifies only
/// the crit leg (vendor :3979 `skillCond["CriticalStrike"] = (pass == 1)`).
#[test]
fn crit_conditional_mod_only_amplifies_crit_leg() {
    let mut db = crit_db();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
    );
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());

    let (_, hit_avg) = out.stored_hit_avg[0];
    let (_, crit_avg) = out.stored_crit_avg[0];
    // The non-crit leg doesn't take this mod: 150; the crit leg: 150×2 (inc) × 3
    // (crit mult) = 900.
    assert!(
        (hit_avg - 150.0).abs() < 1e-9,
        "the non-crit leg must not be affected by the on-crit mod"
    );
    assert!(
        (crit_avg - 900.0).abs() < 1e-9,
        "the crit leg is affected by the on-crit mod + crit multiplier"
    );

    // Genuine dual-leg blend (:4395): 150×0.6 + 900×0.4 = 450.
    assert!(
        (out.total_hit_avg - 450.0).abs() < 1e-9,
        "blend got {}",
        out.total_hit_avg
    );
    // For comparison: the single-factor formula would give 150×2.5 (=300+150, an
    // incorrect fold)... the two figures must diverge in this scenario —
    // the old formula = 150 × 1.8 = 270 ≠ 450.
    assert!((out.total_hit_avg - 270.0).abs() > 1.0);
}

/// A negative mod (active only on **non**-crit hits, negated condition) is likewise
/// routed by leg.
#[test]
fn negated_crit_condition_routes_to_non_crit_leg() {
    let mut db = crit_db();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", true)), // negated: active on non-crit
    );
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());

    let (_, hit_avg) = out.stored_hit_avg[0];
    let (_, crit_avg) = out.stored_crit_avg[0];
    assert!(
        (hit_avg - 300.0).abs() < 1e-9,
        "the non-crit leg is affected by the negated mod"
    );
    assert!(
        (crit_avg - 450.0).abs() < 1e-9,
        "the crit leg is unaffected (only crit multiplier x3)"
    );
    // blend: 300×0.6 + 450×0.4 = 360.
    assert!((out.total_hit_avg - 360.0).abs() < 1e-9);
}

/// A build with no crit mods (c=0): the dual pass leaves every output unchanged (effect=1).
#[test]
fn zero_crit_chance_is_neutral() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());
    assert!((out.total_hit_avg - 150.0).abs() < 1e-9);
    let (_, combined) = out.stored_combined_avg[0];
    assert!((combined - 150.0).abs() < 1e-9);
}

/// Crit-only and non-crit-only damage types (via `Condition:CriticalStrike`) must
/// merge by `DamageType` union rather than positional zip, preserving both in
/// `stored_combined_avg`, `stored_ranges`, `mitigated_avg`, and `hit_mitigation_terms`.
#[test]
fn crit_only_and_non_crit_only_damage_types_merge_by_type_and_mitigate() {
    use pobr_core::calc::{CalculationSession, calculate_minimal_vs_enemy};

    let mut db = crit_db();
    db.add_mod(Modifier::flag("CannotBeEvaded"));
    // Non-crit-only Lightning: 50-50.
    db.add_mod(
        Modifier::number("LightningDamageMin", ModType::Base, 50.0)
            .with_tag(ModTag::condition("CriticalStrike", true)),
    );
    db.add_mod(
        Modifier::number("LightningDamageMax", ModType::Base, 50.0)
            .with_tag(ModTag::condition("CriticalStrike", true)),
    );
    // Crit-only Fire: 100-100 (×3 crit mult = 300-300 on crit leg).
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
    );

    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("LightningResist", ModType::Base, 20.0));
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 50.0));

    let cfg = CalcConfig::attack().with_mode_effective(true);
    let inp = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let out = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &inp);

    let find_avg = |slice: &[(DamageType, f64)], ty: DamageType| -> f64 {
        slice
            .iter()
            .find(|(t, _)| *t == ty)
            .unwrap_or_else(|| panic!("missing {ty:?}"))
            .1
    };
    let eps = 1e-9;

    // c = 0.4, 1-c = 0.6, m = 3.0:
    // Physical: 100*0.6 + 300*0.4 = 180
    // Lightning (non-crit only): 50*0.6 = 30
    // Fire (crit only): 300*0.4 = 120
    assert!((find_avg(&out.stored_combined_avg, DamageType::Physical) - 180.0).abs() < eps);
    assert!((find_avg(&out.stored_combined_avg, DamageType::Lightning) - 30.0).abs() < eps);
    assert!((find_avg(&out.stored_combined_avg, DamageType::Fire) - 120.0).abs() < eps);

    // stored_ranges must contain Physical, Lightning (crit=0), and Fire (hit=0).
    let fire_range = out
        .stored_ranges
        .iter()
        .find(|r| r.damage_type == DamageType::Fire)
        .expect("crit-only Fire must be present in stored_ranges");
    assert_eq!(fire_range.hit_min, 0.0);
    assert_eq!(fire_range.hit_max, 0.0);
    assert!((fire_range.crit_min - 300.0).abs() < eps);
    assert!((fire_range.crit_max - 300.0).abs() < eps);

    let lightning_range = out
        .stored_ranges
        .iter()
        .find(|r| r.damage_type == DamageType::Lightning)
        .expect("non-crit-only Lightning must be present in stored_ranges");
    assert!((lightning_range.hit_min - 50.0).abs() < eps);
    assert!((lightning_range.hit_max - 50.0).abs() < eps);
    assert_eq!(lightning_range.crit_min, 0.0);
    assert_eq!(lightning_range.crit_max, 0.0);

    // Mitigated averages: Phys 180, Lightning 30*0.8 = 24, Fire 120*0.5 = 60.
    assert!((find_avg(&out.mitigated_avg, DamageType::Physical) - 180.0).abs() < eps);
    assert!((find_avg(&out.mitigated_avg, DamageType::Lightning) - 24.0).abs() < eps);
    assert!((find_avg(&out.mitigated_avg, DamageType::Fire) - 60.0).abs() < eps);

    // Session mitigation_terms must succeed and report all three types by DamageType.
    let mut session = CalculationSession::new(inp).with_config(cfg);
    session.add_modifiers(db.iter_mods().cloned());
    session.add_enemy_modifiers(enemy.iter_mods().cloned());
    session.perform_minimal().expect("perform");
    let parts = session
        .hit_mitigation_terms(0)
        .expect("hit_mitigation_terms must handle crit-only damage types");
    assert_eq!(parts.len(), 3);
    let fire_part = parts
        .iter()
        .find(|p| p.damage_type == DamageType::Fire)
        .expect("Fire mitigation part");
    assert!((fire_part.average - 120.0).abs() < eps);
    assert!((fire_part.mitigated - 60.0).abs() < eps);
    assert_eq!(fire_part.resist, Some(50.0));
    let lightning_part = parts
        .iter()
        .find(|p| p.damage_type == DamageType::Lightning)
        .expect("Lightning mitigation part");
    assert!((lightning_part.average - 30.0).abs() < eps);
    assert!((lightning_part.mitigated - 24.0).abs() < eps);
    assert_eq!(lightning_part.resist, Some(20.0));
}
