//! MH/OH hand pass integration tests.
//!
//! Focus = I3 single-hand equivalence (test plan ①: a single HandSource's output must be
//! **value-for-value identical** to "fold the weapon base into MinimalInput and run once" —
//! this is the correctness proof for the dual-pass fallback switch) + hand-calculated
//! dual-wield fixture checks (②) + doubleHits not halved (④).
//! Vendor reference: CalcOffence.lua:2369-2449 (passList) / :2451-2545 (combineStat).

use pobr_core::calc::{
    HandSource, MinimalInput, WeaponBase, calculate_minimal_vs_enemy, combine_mode_for,
    run_hand_passes,
};
use pobr_core::{CalcConfig, CombineMode, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn attack_db() -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0));
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 20.0));
    db.add_mod(Modifier::number("Accuracy", ModType::Base, 500.0));
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        30.0,
    ));
    db.add_mod(Modifier::number("MaximumLife", ModType::Base, 40.0));
    db
}

fn base_input() -> MinimalInput {
    MinimalInput {
        base_life: 100.0,
        base_mana: 50.0,
        base_accuracy: 100.0,
        enemy_evasion: 400.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    }
}

/// I3-empty: `passes = []` must be value-for-value identical to calling
/// `calculate_minimal_vs_enemy` directly; per-hand sub-tables are None (a single "Skill" pass
/// for non-attack skills).
#[test]
fn empty_passes_passthrough_is_identity() {
    let db = attack_db();
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();

    let direct = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &input);
    let via_pass = run_hand_passes(&db, &enemy, &cfg, &[], &input, false);

    assert_eq!(
        via_pass.combined, direct,
        "empty passes must pass through value-for-value"
    );
    assert!(via_pass.main_hand.is_none());
    assert!(via_pass.off_hand.is_none());
}

/// I3-single-hand: a single MainHand HandSource must be value-for-value identical to
/// "fold the weapon into input and run once" (vendor `not bothWeaponAttack` → every stat
/// passes through via OR, :2453).
#[test]
fn single_hand_source_equals_legacy_input_fold_per_value() {
    let db = attack_db();
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();

    let weapon = WeaponBase {
        hit_min: 12.0,
        hit_max: 30.0,
        attack_rate: Some(1.4),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };

    // Legacy fold semantics: write the weapon base directly into MinimalInput and run once.
    let mut folded = input;
    folded.base_hit_min += weapon.hit_min;
    folded.base_hit_max += weapon.hit_max;
    folded.base_action_rate = 1.4;
    let legacy = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &folded);

    let out = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );

    // Value-for-value equal (MinimalOutput derives PartialEq, including breakdown and damage_components).
    assert_eq!(
        out.combined, legacy,
        "single-hand OR pass-through must equal the legacy fold value-for-value"
    );
    let mh = out.main_hand.expect("MainHand sub-table");
    assert!(out.off_hand.is_none());
    assert_eq!(mh.total_dps, legacy.dps);
    assert_eq!(mh.speed, legacy.action_rate);
    assert_eq!(mh.crit_chance, legacy.crit_chance);
    assert_eq!(mh.average_hit, legacy.total_hit_avg);
    assert_eq!(mh.damage_components, legacy.damage_components);
    // Stored family: combined = crit×c + hit×(1−c), consistent with the leg values.
    assert_eq!(mh.stored_combined_avg.len(), mh.stored_hit_avg.len());
    for (((ty, combined), (_, hit)), (_, crit)) in mh
        .stored_combined_avg
        .iter()
        .zip(mh.stored_hit_avg.iter())
        .zip(mh.stored_crit_avg.iter())
    {
        let c = legacy.crit_chance;
        assert!(
            (combined - (crit * c + hit * (1.0 - c))).abs() < 1e-9,
            "{ty:?} StoredCombinedAvg weighted identity"
        );
    }
}

/// Single hand + per-hand conditional mod: a `MainHandAttack` conditional mod takes effect
/// inside the MainHand pass (equivalent to PoB2's weapon1Cfg).
#[test]
fn main_hand_condition_mod_applies_inside_main_hand_pass() {
    let mut db = attack_db();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("MainHandAttack", false)),
    );
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();
    let weapon = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };

    let without_condition = run_hand_passes(
        &attack_db(),
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );
    let with_condition = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );
    assert!(
        with_condition.combined.total_hit_avg > without_condition.combined.total_hit_avg,
        "the MainHandAttack conditional mod must take effect inside the main-hand pass"
    );
}

/// Dual-wield composite fixture (test plan ②): MH mace + OH sword, per-hand mods only route
/// to their own hand, and the combined value is checked by hand against each combine mode
/// (AVERAGE for hit chance / HARMONICMEAN for speed / DPS for damage / CRIT for crit chance).
#[test]
fn dual_wield_combines_per_vendor_modes_hand_calculated() {
    let mut db = attack_db();
    // Per-hand mod: boosts only the main hand.
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("MainHandAttack", false)),
    );
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();

    let mace = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };
    let sword = WeaponBase {
        hit_min: 30.0,
        hit_max: 40.0,
        attack_rate: Some(2.0),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };

    let out = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(mace), HandSource::off_hand(sword)],
        &input,
        false,
    );
    let mh = out.main_hand.as_ref().expect("MH sub-table");
    let oh = out.off_hand.as_ref().expect("OH sub-table");

    // Per-hand mod routing: the MH leg gets the extra +100% inc (150%+100%), the OH leg only 150%.
    // MH base avg = 15 × 2.5 = 37.5… (×crit factor); OH base avg = 35 × 1.5.
    assert!(
        mh.average_hit / 15.0 > oh.average_hit / 35.0,
        "a MainHandAttack mod must only enter the MH leg (MH={} OH={})",
        mh.average_hit,
        oh.average_hit
    );

    // Combined value hand-calculated (vendor :2451-2545):
    let avg = |a: f64, b: f64| (a + b) / 2.0;
    let eps = 1e-9;
    // HitChance: AVERAGE (:3024).
    assert!((out.combined.hit_chance - avg(mh.hit_chance, oh.hit_chance)).abs() < eps);
    // Speed: HARMONICMEAN (:3026).
    let harmonic = 2.0 / (1.0 / mh.speed + 1.0 / oh.speed);
    assert!((out.combined.action_rate - harmonic).abs() < 1e-6);
    // AverageDamage / TotalDPS: DPS, non-doubleHits → (MH+OH)/2 (:2541-2545).
    assert!((out.combined.total_hit_avg - avg(mh.average_hit, oh.average_hit)).abs() < eps);
    assert!((out.combined.dps - avg(mh.total_dps, oh.total_dps)).abs() < eps);
    // CritChance: CRIT, non-doubleHits → (MH+OH)/2 (:2459-2464; fraction conversion unchanged).
    assert!((out.combined.crit_chance - avg(mh.crit_chance, oh.crit_chance)).abs() < eps);
    // CritMultiplier: AVERAGE (:4557).
    assert!(
        (out.combined.crit_multiplier - avg(mh.crit_multiplier, oh.crit_multiplier)).abs() < eps
    );
    // Defence stats are hand-agnostic: match the single-pass run.
    assert_eq!(
        out.combined.life, 140.0,
        "100+40, the defence family is unrelated to hand pass"
    );
}

/// doubleHits (test plan ④): for `doubleHitsWhenDualWielding` skills, DPS = MH + OH,
/// not halved (:2541-2545); CritChance uses the doubleHits cross-term formula (:2459-2461).
#[test]
fn double_hits_dps_is_sum_not_halved() {
    let db = attack_db();
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();
    let mace = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };
    let sword = WeaponBase {
        hit_min: 30.0,
        hit_max: 40.0,
        attack_rate: Some(2.0),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };
    let passes = [HandSource::main_hand(mace), HandSource::off_hand(sword)];

    let normal = run_hand_passes(&db, &enemy, &cfg, &passes, &input, false);
    let double = run_hand_passes(&db, &enemy, &cfg, &passes, &input, true);

    let mh = double.main_hand.as_ref().unwrap();
    let oh = double.off_hand.as_ref().unwrap();
    let eps = 1e-9;
    assert!((double.combined.dps - (mh.total_dps + oh.total_dps)).abs() < eps);
    assert!(
        (normal.combined.dps - (mh.total_dps + oh.total_dps) / 2.0).abs() < eps,
        "non-doubleHits divides by 2"
    );
    // CritChance doubleHits: MH% + OH% − MH%×OH%/100 (in percentage space).
    let mh_pct = mh.crit_chance * 100.0;
    let oh_pct = oh.crit_chance * 100.0;
    let expected = (mh_pct + oh_pct - mh_pct * oh_pct / 100.0) / 100.0;
    assert!((double.combined.crit_chance - expected).abs() < 1e-9);
}

/// Per-hand weapon-flag routing: a mod carrying a weapon flag (the MACE flag produced by
/// dual-writing `with Maces`) only routes to the hand whose weapon matches — the MH mace leg
/// gets the damage bonus, the OH sword leg does not (vendor weapon1Cfg/weapon2Cfg flags are
/// isolated per hand).
#[test]
fn weapon_flag_mod_routes_to_matching_hand_only() {
    let mace_flags = ModFlags::weapon_flags("One Hand Mace", "Mace", true, true);
    let sword_flags = ModFlags::weapon_flags("One Hand Sword", "Sword", true, true);

    let mut db = attack_db();
    // Weapon-flag mod (the flag channel for "100% increased Physical Damage with Maces").
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 100.0).with_flags(ModFlags::MACE));
    let enemy = ModDb::new();
    // Global cfg carries the MH (mace) weapon flag — the global supply from the dual-write
    // channel; the per-hand substitution must swap the OH leg's mace flag for the sword flag
    // (the global flag must not leak into the OH pass).
    let mut cfg = CalcConfig::attack();
    cfg.flags |= mace_flags;
    let input = base_input();

    let mace = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
        flags: mace_flags,
    };
    let sword = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
        flags: sword_flags,
    };

    let out = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(mace), HandSource::off_hand(sword)],
        &input,
        false,
    );
    let mh = out.main_hand.as_ref().expect("MH sub-table");
    let oh = out.off_hand.as_ref().expect("OH sub-table");

    // Both hands share the same base (10-20, average 15): MH leg inc = 50% (flagless base)
    // + 100% (MACE flag), OH leg only 50% → hit ratio = 2.5/1.5.
    let ratio = mh.average_hit / oh.average_hit;
    let expected = 2.5 / 1.5;
    assert!(
        (ratio - expected).abs() < 1e-9,
        "a MACE-flagged mod must only enter the MH leg: MH/OH = {ratio} (expected {expected})"
    );
}

/// A HandSource with empty weapon flags (e.g. Shield Wall-style non-weapon attack sources):
/// per-hand substitution is a no-op — the cfg's upstream flags pass through into the leg
/// unchanged (the "empty supply doesn't clear upstream flags" equivalence branch).
#[test]
fn empty_weapon_flags_keep_upstream_cfg_flags() {
    let mut db = attack_db();
    // A mod that only reads the ATTACK flag: the cfg's upstream flag isn't cleared by the empty weapon-flag supply.
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0).with_flags(ModFlags::ATTACK),
    );
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();
    let weapon = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
        flags: ModFlags::NONE,
    };

    let baseline = run_hand_passes(
        &attack_db(),
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    )
    .combined
    .total_hit_avg;
    let boosted = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );
    assert!(
        boosted.combined.total_hit_avg > baseline,
        "an ATTACK-flagged mod must still take effect when the weapon-flag supply is empty"
    );
}

/// Pins the COMBINE_TABLE (mirrors the vendor call sites verbatim; see the table comments in hand_pass.rs for line numbers).
#[test]
fn combine_table_modes_match_vendor_call_sites() {
    assert_eq!(
        combine_mode_for("HitChance", false),
        Some(CombineMode::Average)
    );
    assert_eq!(
        combine_mode_for("Speed", false),
        Some(CombineMode::HarmonicMean)
    );
    assert_eq!(combine_mode_for("HitSpeed", false), Some(CombineMode::Or));
    assert_eq!(
        combine_mode_for("CritChance", true),
        Some(CombineMode::Crit { double_hits: true })
    );
    assert_eq!(
        combine_mode_for("CritMultiplier", true),
        Some(CombineMode::Average)
    );
    assert_eq!(
        combine_mode_for("TotalDPS", true),
        Some(CombineMode::Dps { double_hits: true })
    );
    assert_eq!(
        combine_mode_for("AverageDamage", false),
        Some(CombineMode::Dps { double_hits: false })
    );
    assert_eq!(combine_mode_for("NotAStat", false), None);
}
