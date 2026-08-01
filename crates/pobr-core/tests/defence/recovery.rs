//! Extended tests for charges / leech / recoup / regen.
//!
//! Cross-referenced against agent-docs/recovery-charges-buffs.md + PoB2 CalcSetup.lua /
//! CalcDefence.lua / Data/Misc.lua.

use pobr_core::{
    CalcConfig, ModDb, Modifier,
    calc::{
        AllChargeStates, ChargeKind, ChargeState, DEFAULT_CHARGE_DURATION_SECONDS,
        DEFAULT_MAX_CHARGES, LEECH_EFFECTIVE_MAX_HIT_DAMAGE, LEECH_MAX_ES_RATE_PCT,
        LEECH_MAX_INSTANCE_PCT, LEECH_MAX_LIFE_RATE_PCT, LEECH_MAX_MANA_RATE_PCT, LeechResource,
        RECOUP_DURATION_4S, RECOUP_DURATION_DEFAULT, RecoupResource, calc_leech,
        calc_leech_from_db, calc_recoup, calc_recoup_from_db, calc_regen, charge_maximum,
        charge_minimum, regen, regen_with_rate, resolve_all_charges, resolve_charge_state,
    },
};
use pobr_data::prelude::*;

// Helper functions

fn make_modifier(name: &str, mod_type: ModType, value: f64) -> Modifier {
    Modifier::number(name, mod_type, value)
}

fn make_flag_modifier(name: &str) -> Modifier {
    Modifier::flag(name)
}

// Charge constants

#[test]
fn charge_defaults_are_correct() {
    // PoB2 Data/Misc.lua: max_power_charges = 3; CalcSetup.lua: ChargeDuration = 15
    assert_eq!(DEFAULT_MAX_CHARGES, 3);
    assert_eq!(DEFAULT_CHARGE_DURATION_SECONDS, 15.0);
}

// charge_maximum

#[test]
fn charge_maximum_returns_default_when_no_mods() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    // No mods -> default 3
    assert_eq!(charge_maximum(&db, &cfg, ChargeKind::Power), 3);
    assert_eq!(charge_maximum(&db, &cfg, ChargeKind::Frenzy), 3);
    assert_eq!(charge_maximum(&db, &cfg, ChargeKind::Endurance), 3);
}

#[test]
fn charge_maximum_increased_by_mod() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    // +2 to Maximum Power Charges -> max becomes 5
    db.add_mod(make_modifier("PowerChargesMax", ModType::Base, 2.0));
    assert_eq!(charge_maximum(&db, &cfg, ChargeKind::Power), 5);
    // Frenzy wasn't added -> still 3
    assert_eq!(charge_maximum(&db, &cfg, ChargeKind::Frenzy), 3);
}

#[test]
fn charge_maximum_cannot_go_below_zero() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    // -5 gets clamped to 0
    db.add_mod(make_modifier("EnduranceChargesMax", ModType::Base, -10.0));
    assert_eq!(charge_maximum(&db, &cfg, ChargeKind::Endurance), 0);
}

// charge_minimum

#[test]
fn charge_minimum_returns_zero_when_no_mods() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    assert_eq!(charge_minimum(&db, &cfg, ChargeKind::Power, 3), 0);
}

#[test]
fn charge_minimum_set_by_mod() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("FrenzyChargesMin", ModType::Base, 1.0));
    assert_eq!(charge_minimum(&db, &cfg, ChargeKind::Frenzy, 3), 1);
}

#[test]
fn charge_minimum_clamped_to_maximum() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    // minimum = 5, but maximum = 3 -> clamped to 3
    db.add_mod(make_modifier("PowerChargesMin", ModType::Base, 5.0));
    assert_eq!(charge_minimum(&db, &cfg, ChargeKind::Power, 3), 3);
}

#[test]
fn charge_minimum_is_maximum_flag_pulls_min_to_cap() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_flag_modifier(
        "MinimumFrenzyChargesIsMaximumFrenzyCharges",
    ));
    // Regardless of the FrenzyChargesMin mod, minimum = maximum = 3 (always full stacks)
    assert_eq!(charge_minimum(&db, &cfg, ChargeKind::Frenzy, 3), 3);
}

// resolve_charge_state

#[test]
fn resolve_charge_state_reads_from_cfg_multipliers() {
    let db = ModDb::new();
    // Currently at 2 Power Charge stacks
    let cfg = CalcConfig::new().with_multiplier("PowerCharge", 2.0);
    let state = resolve_charge_state(&db, &cfg, ChargeKind::Power);
    assert_eq!(
        state,
        ChargeState {
            current: 2,
            maximum: 3,
            minimum: 0,
        }
    );
}

#[test]
fn resolve_charge_state_clamps_current_to_max() {
    let db = ModDb::new();
    // Claims 10 stacks (over the cap of 3) -> clamped to 3
    let cfg = CalcConfig::new().with_multiplier("EnduranceCharge", 10.0);
    let state = resolve_charge_state(&db, &cfg, ChargeKind::Endurance);
    assert_eq!(state.current, 3);
    assert_eq!(state.maximum, 3);
}

#[test]
fn resolve_charge_state_clamps_current_to_min() {
    let mut db = ModDb::new();
    // minimum 1 stack, but cfg has FrenzyCharge = 0 -> clamped to 1
    db.add_mod(make_modifier("FrenzyChargesMin", ModType::Base, 1.0));
    let cfg = CalcConfig::new().with_multiplier("FrenzyCharge", 0.0);
    let state = resolve_charge_state(&db, &cfg, ChargeKind::Frenzy);
    assert_eq!(state.current, 1);
    assert_eq!(state.minimum, 1);
}

// resolve_all_charges & AllChargeStates::total

#[test]
fn all_charges_total_sums_three_kinds() {
    let db = ModDb::new();
    let cfg = CalcConfig::new()
        .with_multiplier("PowerCharge", 1.0)
        .with_multiplier("FrenzyCharge", 2.0)
        .with_multiplier("EnduranceCharge", 3.0);
    let all = resolve_all_charges(&db, &cfg);
    assert_eq!(all.total(), 6);
}

#[test]
fn all_charges_zero_when_no_multipliers_set() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    let all = resolve_all_charges(&db, &cfg);
    assert_eq!(all.total(), 0);
    assert_eq!(
        all,
        AllChargeStates {
            power: ChargeState {
                current: 0,
                maximum: 3,
                minimum: 0,
            },
            frenzy: ChargeState {
                current: 0,
                maximum: 3,
                minimum: 0,
            },
            endurance: ChargeState {
                current: 0,
                maximum: 3,
                minimum: 0,
            },
        }
    );
}

// regen & regen_with_rate

#[test]
fn regen_flat_plus_percent_of_pool() {
    // base = 10 flat + 2% of 1000 = 30; ×1 ×1 = 30
    assert_eq!(regen(1000.0, 10.0, 2.0, 0.0, 1.0), 30.0);
}

#[test]
fn regen_with_inc_and_more() {
    // base = 5 + 1% of 1000 = 15; ×(1+100/100) = 30; ×1.5 = 45
    assert_eq!(regen(1000.0, 5.0, 1.0, 100.0, 1.5), 45.0);
}

#[test]
fn regen_with_rate_multiplies_by_recovery_rate() {
    // base = 10; regen = 10; × recovery_rate_mod=2.0 → 20
    let r = regen_with_rate(0.0, 10.0, 0.0, 0.0, 1.0, 2.0);
    assert_eq!(r, 20.0);
}

#[test]
fn regen_with_rate_zero_when_rate_mod_zero() {
    // recovery_rate_mod = 0 → 0
    let r = regen_with_rate(1000.0, 50.0, 0.0, 0.0, 1.0, 0.0);
    assert_eq!(r, 0.0);
}

// calc_regen (ModDb-based)

#[test]
fn calc_regen_life_no_mods_returns_zero() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    assert_eq!(calc_regen(&db, &cfg, 1000.0, "LifeRegen"), 0.0);
}

#[test]
fn calc_regen_life_with_flat_and_percent() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRegen", ModType::Base, 5.0));
    db.add_mod(make_modifier("LifeRegenPercent", ModType::Base, 1.0));
    // base = 5 + 1% × 1000 = 15; no inc/more → 15
    assert_eq!(calc_regen(&db, &cfg, 1000.0, "LifeRegen"), 15.0);
}

#[test]
fn calc_regen_applies_xregen_rate_inc() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRegen", ModType::Base, 10.0));
    // LifeRegenRate +100% → ×2 → 20
    db.add_mod(make_modifier("LifeRegenRate", ModType::Inc, 100.0));
    assert_eq!(calc_regen(&db, &cfg, 0.0, "LifeRegen"), 20.0);
}

#[test]
fn calc_regen_applies_recovery_rate_inc() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRegen", ModType::Base, 10.0));
    // LifeRecoveryRate +50% → ×1.5 → 15
    db.add_mod(make_modifier("LifeRecoveryRate", ModType::Inc, 50.0));
    assert_eq!(calc_regen(&db, &cfg, 0.0, "LifeRegen"), 15.0);
}

/// Bare `<stat>`-named INC/MORE mods feed into the same aggregation (vendor
/// CalcDefence.lua:1642-1643 `Sum("INC", nil, resource.."Regen", resource.."RecoveryRate")` /
/// `More(nil, resource.."Regen", ...)` — mod_parser's "increased Mana/Life
/// Regeneration Rate" and the statmap buff domain (Clarity's `ManaRegen INC`,
/// sup_int.txt:305-315) both produce bare names).
#[test]
fn calc_regen_applies_bare_stat_inc_and_more() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("ManaRegen", ModType::Base, 10.0));
    // Bare-name INC +50% (the Clarity II shape) -> x1.5 -> 15
    db.add_mod(make_modifier("ManaRegen", ModType::Inc, 50.0));
    assert_eq!(calc_regen(&db, &cfg, 0.0, "ManaRegen"), 15.0);
    // Bare-name MORE +20% (the Arcane Surge shape, CalcPerform.lua:1586) -> x1.5x1.2 = 18
    db.add_mod(make_modifier("ManaRegen", ModType::More, 20.0));
    assert_eq!(calc_regen(&db, &cfg, 0.0, "ManaRegen"), 18.0);
}

#[test]
fn calc_regen_stacks_rate_and_recovery_rate_inc() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRegen", ModType::Base, 10.0));
    // LifeRegenRate +50% + LifeRecoveryRate +50% → inc_sum=100% → ×2 → 20
    db.add_mod(make_modifier("LifeRegenRate", ModType::Inc, 50.0));
    db.add_mod(make_modifier("LifeRecoveryRate", ModType::Inc, 50.0));
    assert_eq!(calc_regen(&db, &cfg, 0.0, "LifeRegen"), 20.0);
}

// Leech constants

#[test]
fn leech_constants_match_pob2_data_misc() {
    // PoB2 Data/Misc.lua: MaxLifeLeechRate=20, MaxManaLeechRate=20, MaxEnergyShieldLeechRate=10
    assert_eq!(LEECH_MAX_LIFE_RATE_PCT, 20.0);
    assert_eq!(LEECH_MAX_MANA_RATE_PCT, 20.0);
    assert_eq!(LEECH_MAX_ES_RATE_PCT, 10.0);
    // MaxLeechInstance=10%, EffectiveMaxDamageForLeech=40000
    assert_eq!(LEECH_MAX_INSTANCE_PCT, 10.0);
    assert_eq!(LEECH_EFFECTIVE_MAX_HIT_DAMAGE, 40000.0);
}

// calc_leech, the pure function

#[test]
fn calc_leech_zero_when_no_pct() {
    let r = calc_leech(1000.0, 0.0, 5000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 0.0);
    assert_eq!(r.display_rate_per_second, 0.0);
    // rate_cap is still computed from the pool (used for the panel's displayed ceiling)
    assert_eq!(r.rate_cap_per_second, 200.0); // 1000 x 20%
}

#[test]
fn calc_leech_zero_when_pool_zero() {
    let r = calc_leech(0.0, 5.0, 5000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 0.0);
    assert_eq!(r.rate_cap_per_second, 0.0);
}

#[test]
fn calc_leech_single_instance_basic_life() {
    // pool=1000, leech_pct=2% hit_damage=5000 → raw_total = 100; inst_cap = 100 → inst_total=100
    // rate_cap = 200/s; display_rate = min(200, 100*2) = 200
    let r = calc_leech(1000.0, 2.0, 5000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 100.0);
    assert_eq!(r.rate_cap_per_second, 200.0);
    assert_eq!(r.display_rate_per_second, 200.0);
}

#[test]
fn calc_leech_instance_cap_applied_when_hit_very_large() {
    // hit_damage = 40000 (max), leech_pct = 2%, pool=1000
    // raw_total = 800; inst_cap = 100; inst_total = 100
    let r = calc_leech(1000.0, 2.0, 40000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 100.0); // inst_cap = 1000 × 10%
}

#[test]
fn calc_leech_hit_capped_at_effective_max() {
    // hit = 100000 → effective_hit = 40000 → raw_total = 2%*40000 = 800
    // inst_cap = 1000 × 10% = 100 → inst_total = 100
    let r = calc_leech(1000.0, 2.0, 100_000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 100.0);
}

#[test]
fn calc_leech_es_has_lower_rate_cap() {
    // pool=1000, ES max_rate=10%/s → rate_cap = 100
    let r = calc_leech(1000.0, 1.0, 5000.0, LeechResource::EnergyShield);
    assert_eq!(r.rate_cap_per_second, 100.0); // 1000 × 10%
}

#[test]
fn calc_leech_small_instance_limits_display_rate() {
    // pool=1000, leech_pct=0.1%, hit=1000 → raw_total=1.0; inst_cap=100
    // rate_cap=200; ratio=20/10=2; display = min(200, 1.0×2) = 2
    let r = calc_leech(1000.0, 0.1, 1000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 1.0);
    assert_eq!(r.display_rate_per_second, 2.0);
}

// calc_leech_from_db

#[test]
fn calc_leech_from_db_with_life_leech_mod() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    // LifeLeech = 2% (BASE)
    db.add_mod(make_modifier("LifeLeech", ModType::Base, 2.0));
    let r = calc_leech_from_db(&db, &cfg, 1000.0, 5000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 100.0); // 5000 × 2% = 100 = inst_cap
}

#[test]
fn calc_leech_from_db_blocked_by_cannot_flag() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeLeech", ModType::Base, 2.0));
    db.add_mod(make_flag_modifier("CannotLeechLife"));
    let r = calc_leech_from_db(&db, &cfg, 1000.0, 5000.0, LeechResource::Life);
    assert_eq!(r.instance_total, 0.0);
    assert_eq!(r.display_rate_per_second, 0.0);
}

#[test]
fn calc_leech_from_db_cannot_mana_does_not_block_life() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeLeech", ModType::Base, 1.0));
    db.add_mod(make_flag_modifier("CannotLeechMana"));
    let r = calc_leech_from_db(&db, &cfg, 1000.0, 5000.0, LeechResource::Life);
    // CannotLeechMana doesn't affect Life leech
    assert!(r.instance_total > 0.0);
}

// Recoup constants

#[test]
fn recoup_duration_constants_match_pob2() {
    // PoB2 CalcPerform.lua: default 8s, 4SecondRecoup -> 4s
    assert_eq!(RECOUP_DURATION_DEFAULT, 8.0);
    assert_eq!(RECOUP_DURATION_4S, 4.0);
}

// calc_recoup, the pure function

#[test]
fn calc_recoup_basic_8s() {
    // damage_taken=800, recoup_pct=10%, duration=8s, rate_mod=1.0
    // total = 80; rate = 80/8 = 10/s
    let r = calc_recoup(800.0, 10.0, 8.0, 1.0);
    assert_eq!(r.rate_per_second, 10.0);
    assert_eq!(r.duration, 8.0);
}

#[test]
fn calc_recoup_4s_flag_doubles_rate() {
    // damage_taken=800, recoup_pct=10%, duration=4s → total=80; rate=80/4=20/s
    let r = calc_recoup(800.0, 10.0, 4.0, 1.0);
    assert_eq!(r.rate_per_second, 20.0);
    assert_eq!(r.duration, 4.0);
}

#[test]
fn calc_recoup_zero_when_no_damage() {
    let r = calc_recoup(0.0, 15.0, 8.0, 1.0);
    assert_eq!(r.rate_per_second, 0.0);
}

#[test]
fn calc_recoup_zero_when_no_pct() {
    let r = calc_recoup(500.0, 0.0, 8.0, 1.0);
    assert_eq!(r.rate_per_second, 0.0);
}

#[test]
fn calc_recoup_applies_recovery_rate_mod() {
    // rate=10/s × recovery_rate_mod=1.5 = 15/s
    let r = calc_recoup(800.0, 10.0, 8.0, 1.5);
    assert_eq!(r.rate_per_second, 15.0);
}

// calc_recoup_from_db

#[test]
fn calc_recoup_from_db_no_mods_returns_zero() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    let r = calc_recoup_from_db(&db, &cfg, 500.0, RecoupResource::Life);
    assert_eq!(r.rate_per_second, 0.0);
}

#[test]
fn calc_recoup_from_db_life_recoup() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    // LifeRecoup = 15% → total = 1000 × 15% = 150; rate = 150/8 ≈ 18.75/s
    db.add_mod(make_modifier("LifeRecoup", ModType::Base, 15.0));
    let r = calc_recoup_from_db(&db, &cfg, 1000.0, RecoupResource::Life);
    assert!((r.rate_per_second - 18.75).abs() < 0.001);
    assert_eq!(r.duration, 8.0);
}

#[test]
fn calc_recoup_from_db_global_4s_flag() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRecoup", ModType::Base, 10.0));
    db.add_mod(make_flag_modifier("4SecondRecoup")); // global 4s flag
    let r = calc_recoup_from_db(&db, &cfg, 800.0, RecoupResource::Life);
    assert_eq!(r.duration, 4.0);
    // total=80; rate=80/4=20/s
    assert_eq!(r.rate_per_second, 20.0);
}

#[test]
fn calc_recoup_from_db_single_resource_4s_flag() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRecoup", ModType::Base, 10.0));
    db.add_mod(make_flag_modifier("4SecondLifeRecoup")); // single-resource 4s flag
    let r = calc_recoup_from_db(&db, &cfg, 800.0, RecoupResource::Life);
    assert_eq!(r.duration, 4.0);
}

#[test]
fn calc_recoup_from_db_4s_flag_does_not_affect_other_resource() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("ManaRecoup", ModType::Base, 10.0));
    db.add_mod(make_flag_modifier("4SecondLifeRecoup")); // Life-only flag
    let r = calc_recoup_from_db(&db, &cfg, 800.0, RecoupResource::Mana);
    // Life's 4s flag doesn't affect Mana -> still 8s
    assert_eq!(r.duration, 8.0);
}

#[test]
fn calc_recoup_from_db_recovery_rate_applies() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("LifeRecoup", ModType::Base, 10.0));
    // LifeRecoveryRate +100% → ×2
    db.add_mod(make_modifier("LifeRecoveryRate", ModType::Inc, 100.0));
    let r = calc_recoup_from_db(&db, &cfg, 800.0, RecoupResource::Life);
    // total=80; rate=80/8=10/s; ×2=20/s
    assert_eq!(r.rate_per_second, 20.0);
}

#[test]
fn calc_recoup_from_db_es_recoup() {
    let mut db = ModDb::new();
    let cfg = CalcConfig::new();
    db.add_mod(make_modifier("EnergyShieldRecoup", ModType::Base, 20.0));
    let r = calc_recoup_from_db(&db, &cfg, 500.0, RecoupResource::EnergyShield);
    // total = 500*20%=100; rate=100/8=12.5/s
    assert!((r.rate_per_second - 12.5).abs() < 0.001);
    assert_eq!(r.duration, 8.0);
}
