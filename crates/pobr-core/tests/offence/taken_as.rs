//! (13-G1 / 13-G7): integration tests for the taken-as pipeline + effectiveAppliedArmour.
//!
//! Expected values are hand-computed from the PoB2 formulas; comments cite the
//! `Modules/CalcDefence.lua` line numbers. Mod text goes through mod_parser; intermediate
//! values with no parseable source (e.g. ArmourDefense) are injected as a Modifier directly.

use crate::support::parse_mod;
use pobr_core::calc::{
    MitigationCtx, MitigationInputs, armour_applies_pct, build_mitigation_ctx, damage_shift_table,
    effective_applied_armour, taken_hit_from_damage,
};
use pobr_core::mod_parser::ParseStatus;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const PHYS: usize = DamageType::Physical as usize;
const FIRE: usize = DamageType::Fire as usize;
const COLD: usize = DamageType::Cold as usize;
const LIGHT: usize = DamageType::Lightning as usize;
const CHAOS: usize = DamageType::Chaos as usize;

/// Applies a local override on top of a neutral ctx (works around clippy's
/// field_reassign_with_default lint).
fn ctx_with(adjust: impl FnOnce(&mut MitigationCtx)) -> MitigationCtx {
    let mut ctx = MitigationCtx::default();
    adjust(&mut ctx);
    ctx
}

/// Parses mod text and injects it into db (W0.1: the parse table is the contract).
fn add_text(db: &mut ModDb, text: &str) {
    let outcome = parse_mod(text).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
    assert_eq!(
        outcome.status,
        ParseStatus::Parsed,
        "unsupported modifier in fixture: {text:?}"
    );
    db.add_list(outcome.mods);
}

// damage_shift_table (CalcDefence.lua:2171-2190)

/// No mods → identity matrix (each type retains 100% of itself).
#[test]
fn shift_table_defaults_to_identity() {
    let db = ModDb::new();
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    for (s, row) in shift.iter().enumerate() {
        for (d, value) in row.iter().enumerate() {
            let expected = if s == d { 1.0 } else { 0.0 };
            assert_eq!(*value, expected, "shift[{s}][{d}]");
        }
    }
}

/// "30% of Cold Damage taken as Lightning" → 30% of cold converts to lightning, 70% is
/// retained (:2184 BASE sum, :2189 source retention max(100−total,0)).
#[test]
fn shift_table_single_conversion() {
    // The bare "taken as" text is parsed whole via special_mods
    // `cold_damage_taken_as_lightning` (vendor ModParser.lua:5655) — end to end via the
    // text channel.
    let mut db = ModDb::new();
    add_text(&mut db, "30% of Cold Damage taken as Lightning");
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    assert_eq!(shift[COLD][LIGHT], 0.3);
    assert_eq!(shift[COLD][COLD], 0.7);
    assert_eq!(shift[COLD][FIRE], 0.0);
    // Other source types are unaffected.
    assert_eq!(shift[PHYS][PHYS], 1.0);
}

/// Total conversion out >100%: the source is clamped to 0, but the target shares are
/// **not** renormalized (same vendor semantics — :2189 only applies max(100−total,0)
/// to the source retention; each target keeps its own BASE sum).
#[test]
fn shift_table_over_100_truncates_source_only() {
    let mut db = ModDb::new();
    add_text(&mut db, "60% of Physical Damage taken as Fire Damage");
    add_text(&mut db, "60% of Physical Damage taken as Lightning Damage");
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    assert_eq!(shift[PHYS][PHYS], 0.0, "源保留 max(1-1.2, 0) = 0");
    assert_eq!(shift[PHYS][FIRE], 0.6);
    assert_eq!(shift[PHYS][LIGHT], 0.6);
}

/// The from-hits variant and the plain variant both feed into the hit-level shiftTable
/// (:2184 sums both families).
#[test]
fn shift_table_sums_hits_variant() {
    let mut db = ModDb::new();
    add_text(&mut db, "20% of Physical Damage taken as Fire Damage");
    add_text(
        &mut db,
        "30% of Physical Damage from Hits taken as Fire Damage",
    );
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    assert_eq!(shift[PHYS][FIRE], 0.5);
    assert_eq!(shift[PHYS][PHYS], 0.5);
}

// effectiveAppliedArmour (CalcDefence.lua:2336-2362, :1862-1863)

/// Physical has an implicit BASE 100 (:1862-1863): with an empty db, physical takes
/// the full armour value and elemental types take none.
#[test]
fn effective_armour_physical_implicit_100() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    assert_eq!(armour_applies_pct(&db, &cfg, DamageType::Physical), 100.0);
    assert_eq!(armour_applies_pct(&db, &cfg, DamageType::Fire), 0.0);
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Physical),
        1000.0
    );
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Fire),
        0.0
    );
}

/// "50% of armour applies to fire, cold and lightning" partial application
/// (ModParser.lua:2525-2529): elemental types get 50% of the armour, physical
/// **keeps the full amount** (distinct from the "instead" variant).
#[test]
fn effective_armour_partial_pct_keeps_physical() {
    // This mod text is not covered by the current engine rules — inject it directly per
    // its data expansion (ModParser.lua:2525-2529).
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 50.0));
    }
    let cfg = CalcConfig::attack();
    for dt in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
        assert_eq!(
            effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, dt),
            500.0,
            "{dt:?}"
        );
    }
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Physical),
        1000.0,
        "无 instead flag → 物理隐式 100 保留"
    );
}

/// The "instead of physical" variant (ModParser.lua:2519-2524): elemental types get
/// the full armour value, and the `ArmourDoesNotApplyToPhysicalDamageTaken` flag zeroes
/// physical — physical is only zeroed in **this** variant.
#[test]
fn effective_armour_instead_zeroes_physical() {
    // This mod text is not covered by the current engine rules — inject it directly per
    // its data expansion (ModParser.lua:2519-2524).
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 100.0));
    }
    db.add_mod(Modifier::flag("ArmourDoesNotApplyToPhysicalDamageTaken"));
    let cfg = CalcConfig::attack();
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Physical),
        0.0
    );
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Fire),
        1000.0
    );
}

/// ArmourDefense multiplier (:1392 `Max("ArmourDefense")/100`, :2343 `×(1+ArmourDefense)`)
/// and the Evasion borrow term (:2354-2356, only `max(…,0)`, not scaled by ArmourDefense).
#[test]
fn effective_armour_armour_defense_and_evasion_share() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ArmourDefense", ModType::Base, 20.0));
    db.add_mod(Modifier::number(
        "EvasionAppliesToFireDamageTaken",
        ModType::Base,
        50.0,
    ));
    let cfg = CalcConfig::attack();
    // Physical: 1000 × 100% × 1.2 = 1200.
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 800.0, 0.0, DamageType::Physical),
        1200.0
    );
    // Fire: armour share 0 + Evasion 800 × 50% = 400 (the Evasion term is not multiplied
    // by ArmourDefense).
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 800.0, 0.0, DamageType::Fire),
        400.0
    );
}

// taken_hit_from_damage (CalcDefence.lua:422-455)

/// Pure resistance: fire raw 1000, fire resist 75% → taken 250 (resMult = 1−75/100,
/// :2363/:432).
#[test]
fn taken_hit_pure_resist() {
    let ctx = ctx_with(|c| c.resist_taken_multi[FIRE] = 0.25);
    let (sum, parts) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(sum, 250.0);
    assert_eq!(parts[FIRE], 250.0);
}

/// flat DR + overwhelm (:429-431): physical raw 1000, flat DR 30%, overwhelm 10% →
/// drMulti = 1 − (30−10)/100 = 0.8 → 800.
#[test]
fn taken_hit_flat_dr_minus_overwhelm() {
    let ctx = ctx_with(|c| {
        c.flat_dr_pct[PHYS] = 30.0;
        c.overwhelm_pct[PHYS] = 10.0;
    });
    let (sum, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(sum, 800.0);
}

/// Damage reduction cap (:429/:431, both use min `<src>DamageReductionMax`): armour 1e9
/// → armourDR caps at 90%; overwhelm 15% then knocks it down to 75% → taken 0.25
/// (matches the same figure in pob2_golden `defence_physical_overwhelm`).
#[test]
fn taken_hit_dr_capped_then_overwhelmed() {
    let ctx = ctx_with(|c| {
        c.effective_applied_armour[PHYS] = 1.0e9;
        c.overwhelm_pct[PHYS] = 15.0;
    });
    let (sum, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(sum, 250.0);
}

/// takenFlat addend + AfterReduction multiplier (:438/:442): fire raw 100, takenFlat
/// +20, afterReduction 1.2 → round((100+20) × 1.2) = 144.
#[test]
fn taken_hit_taken_flat_and_after_multi() {
    let ctx = ctx_with(|c| {
        c.taken_flat[FIRE] = 20.0;
        c.after_reduction_multi[FIRE] = 1.2;
    });
    let (sum, _) = taken_hit_from_damage(100.0, DamageType::Fire, &ctx);
    assert_eq!(sum, 144.0);
}

/// Negative takenFlat floor (:442 `m_max(…, 0)`): mitigation never produces negative
/// taken damage.
#[test]
fn taken_hit_floors_at_zero() {
    let ctx = ctx_with(|c| c.taken_flat[COLD] = -500.0);
    let (sum, parts) = taken_hit_from_damage(100.0, DamageType::Cold, &ctx);
    assert_eq!(sum, 0.0);
    assert_eq!(parts[COLD], 0.0);
}

// End to end (builder + mod text)

/// Lightning-Coil-style "50% of Physical Damage from Hits taken as Lightning" end to
/// end: armour 2000, lightning resist 75%, physical raw 1000 —
/// - physical half: converted 500, armourDR = 2000/(2000+10×500) = 28.5714% (armour_ratio
///   10), taken round(500 × 0.714286) = 357;
/// - lightning half: converted 500 × (1−0.75) = round(125) = 125;
///
/// Total 482 < the unconverted round(1000 × (1 − 2000/12000)) = 833 →
/// physical taken drops significantly (= physical max hit rises), while the lightning
/// portion is constrained by resistance.
#[test]
fn taken_hit_lightning_coil_end_to_end() {
    let mut db = ModDb::new();
    add_text(
        &mut db,
        "50% of Physical Damage from Hits taken as Lightning Damage",
    );
    let cfg = CalcConfig::attack();
    let inputs = MitigationInputs {
        armour: 2000.0,
        resist_pct: [0.0, 0.0, 0.0, 75.0, 0.0],
        ..MitigationInputs::default()
    };
    let ctx = build_mitigation_ctx(&db, &cfg, &inputs);
    assert_eq!(ctx.shift[PHYS][LIGHT], 0.5);
    assert_eq!(ctx.shift[PHYS][PHYS], 0.5);
    assert_eq!(ctx.effective_applied_armour[PHYS], 2000.0);
    assert_eq!(ctx.effective_applied_armour[LIGHT], 0.0);

    let (sum, parts) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(parts[PHYS], 357.0, "物理半经护甲减伤");
    assert_eq!(parts[LIGHT], 125.0, "电半经抗性");
    assert_eq!(sum, 482.0);

    // For comparison: under the same conditions without conversion, taken would be 833 —
    // taken-as reduces physical taken damage by 42%.
    let baseline = build_mitigation_ctx(&ModDb::new(), &cfg, &inputs);
    let (no_shift_sum, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &baseline);
    assert_eq!(no_shift_sum, 833.0);
    assert!(sum < no_shift_sum);
}

/// "50% of armour applies to fire" partial application end to end: fire raw 1000,
/// armour 2000, fire resist 0 → effArmour 1000 → armourDR = 1000/(1000+10000) = 9.0909%
/// → round(909) = 909; physical under the same conditions still takes the full armour
/// value (2000) → round(1000 × (1−1/6)) = 833.
#[test]
fn builder_partial_armour_applies_to_fire() {
    // This mod text is not covered by the current engine rules — inject it directly per
    // its data expansion (as above).
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 50.0));
    }
    let cfg = CalcConfig::attack();
    let inputs = MitigationInputs {
        armour: 2000.0,
        ..MitigationInputs::default()
    };
    let ctx = build_mitigation_ctx(&db, &cfg, &inputs);
    assert_eq!(ctx.effective_applied_armour[FIRE], 1000.0);
    assert_eq!(ctx.effective_applied_armour[PHYS], 2000.0);

    let (fire_taken, _) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(fire_taken, 909.0);
    let (phys_taken, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(phys_taken, 833.0);
}

/// "instead of physical" flag end to end: physical armour is zeroed only in this
/// variant — physical taken reverts to the no-armour value 1000, and fire taken now
/// gets the full armour value 833.
#[test]
fn builder_instead_of_physical_flag() {
    // This mod text is not covered by the current engine rules — inject it directly per
    // its data expansion (as above).
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 100.0));
    }
    db.add_mod(Modifier::flag("ArmourDoesNotApplyToPhysicalDamageTaken"));
    let cfg = CalcConfig::attack();
    let inputs = MitigationInputs {
        armour: 2000.0,
        ..MitigationInputs::default()
    };
    let ctx = build_mitigation_ctx(&db, &cfg, &inputs);
    assert_eq!(ctx.armour_applies_pct[PHYS], 0.0, "flag 清零物理份额");
    assert_eq!(ctx.effective_applied_armour[PHYS], 0.0);
    assert_eq!(ctx.effective_applied_armour[FIRE], 2000.0);

    let (phys_taken, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(phys_taken, 1000.0);
    let (fire_taken, _) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(fire_taken, 833.0);
}

/// builder defaults: empty db → dr cap 90 (vendor Data.lua:178 DamageReductionCap,
/// injected via `cfg.constants.character().maximum_physical_damage_reduction_pct`),
/// identity shift, multiplier 1.
#[test]
fn builder_neutral_defaults() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let ctx = build_mitigation_ctx(&db, &cfg, &MitigationInputs::default());
    assert_eq!(ctx.dr_max_pct, [90.0; 5]);
    assert_eq!(ctx.flat_dr_pct, [0.0; 5]);
    assert_eq!(ctx.resist_taken_multi, [1.0; 5]);
    assert_eq!(ctx.after_reduction_multi, [1.0; 5]);
    assert_eq!(ctx.taken_flat, [0.0; 5]);
    assert_eq!(ctx.armour_applies_pct[PHYS], 100.0);
    assert_eq!(ctx.shift[CHAOS][CHAOS], 1.0);
}

/// The builder folds taken inc/more into afterReduction (:2429 Average figure ×
/// `taken_mult_for_type_default`): "20% increased Damage Taken" → multiplier 1.2.
#[test]
fn builder_after_reduction_from_taken_inc() {
    let mut db = ModDb::new();
    add_text(&mut db, "20% increased Damage Taken");
    let cfg = CalcConfig::attack();
    let ctx = build_mitigation_ctx(&db, &cfg, &MitigationInputs::default());
    assert!(
        (ctx.after_reduction_multi[FIRE] - 1.2).abs() < 1e-9,
        "after = {}",
        ctx.after_reduction_multi[FIRE]
    );
    let (sum, _) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(sum, 1200.0);
}
