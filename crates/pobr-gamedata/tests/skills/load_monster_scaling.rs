//! `base/monster_scaling.json` load tests.
//!
//! Migration-invariant checks: the seven tables with a Rust source of
//! truth (`pobr_data::monster`) are **checked value-for-value in a loop
//! across all 100 levels**; the two vendor-only ally tables are
//! spot-checked against vendor `Misc.lua` line numbers (values hardcoded).

use pobr_data::monster::{
    MONSTER_ACCURACY_TABLE, MONSTER_AILMENT_THRESHOLD_TABLE, MONSTER_ARMOUR_TABLE,
    MONSTER_DAMAGE_TABLE, MONSTER_EVASION_TABLE, MONSTER_LIFE_TABLE, MONSTER_POISE_THRESHOLD_TABLE,
    MONSTER_TABLE_LEN,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// All nine tables should always be 100 entries (level 1..=100).
#[test]
fn all_tables_have_100_levels() {
    let t = game_data()
        .monster_scaling()
        .expect("monster_scaling 可加载");
    assert_eq!(t.accuracy.len(), MONSTER_TABLE_LEN, "accuracy 长度");
    assert_eq!(t.evasion.len(), MONSTER_TABLE_LEN, "evasion 长度");
    assert_eq!(t.armour.len(), MONSTER_TABLE_LEN, "armour 长度");
    assert_eq!(t.life.len(), MONSTER_TABLE_LEN, "life 长度");
    assert_eq!(t.ally_life.len(), MONSTER_TABLE_LEN, "ally_life 长度");
    assert_eq!(t.damage.len(), MONSTER_TABLE_LEN, "damage 长度");
    assert_eq!(t.ally_damage.len(), MONSTER_TABLE_LEN, "ally_damage 长度");
    assert_eq!(
        t.ailment_threshold.len(),
        MONSTER_TABLE_LEN,
        "ailment_threshold 长度"
    );
    assert_eq!(
        t.poise_threshold.len(),
        MONSTER_TABLE_LEN,
        "poise_threshold 长度"
    );
}

/// Migration invariant: the six integer tables are value-equal to
/// `pobr_data::monster`'s existing Rust tables (asserted in a loop across
/// all 100 levels).
#[test]
fn integer_tables_match_rust_source_at_every_level() {
    let t = game_data().monster_scaling().unwrap();
    for lv in 1..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        assert_eq!(
            t.accuracy[i], MONSTER_ACCURACY_TABLE[i],
            "lv{lv} accuracy 与 MONSTER_ACCURACY_TABLE 不等"
        );
        assert_eq!(
            t.evasion[i], MONSTER_EVASION_TABLE[i],
            "lv{lv} evasion 与 MONSTER_EVASION_TABLE 不等"
        );
        assert_eq!(
            t.armour[i], MONSTER_ARMOUR_TABLE[i],
            "lv{lv} armour 与 MONSTER_ARMOUR_TABLE 不等"
        );
        assert_eq!(
            t.life[i], MONSTER_LIFE_TABLE[i],
            "lv{lv} life 与 MONSTER_LIFE_TABLE 不等"
        );
        assert_eq!(
            t.ailment_threshold[i], MONSTER_AILMENT_THRESHOLD_TABLE[i],
            "lv{lv} ailment_threshold 与 MONSTER_AILMENT_THRESHOLD_TABLE 不等"
        );
        assert_eq!(
            t.poise_threshold[i], MONSTER_POISE_THRESHOLD_TABLE[i],
            "lv{lv} poise_threshold 与 MONSTER_POISE_THRESHOLD_TABLE 不等"
        );
    }
}

/// Migration invariant: the damage table is value-equal (bit-level) to the
/// Rust table (asserted in a loop across all 100 levels).
///
/// The Rust table and the JSON both use the 2-decimal-place convention
/// (vendor's noisy f32 values rounded to 2 places), so they should be
/// bit-level equal — hence `==` rather than a tolerance comparison.
#[test]
fn damage_table_matches_rust_source_at_every_level() {
    let t = game_data().monster_scaling().unwrap();
    for lv in 1..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        assert_eq!(
            t.damage[i], MONSTER_DAMAGE_TABLE[i],
            "lv{lv} damage 与 MONSTER_DAMAGE_TABLE 不等"
        );
    }
}

/// vendor-only: spot checks on ally_life (pobr has no Rust source of truth for it).
///
/// Source: `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L8's
/// `data.monsterAllyLifeTable` (Lua 1-indexed = monster level).
#[test]
fn ally_life_spot_checks_against_vendor() {
    let t = game_data().monster_scaling().unwrap();
    // Misc.lua L8: [1]=51, [10]=382, [20]=886, [50]=3709
    assert_eq!(t.ally_life[0], 51, "lv1 ally_life");
    assert_eq!(t.ally_life[9], 382, "lv10 ally_life");
    assert_eq!(t.ally_life[19], 886, "lv20 ally_life");
    assert_eq!(t.ally_life[49], 3709, "lv50 ally_life");
    // Misc.lua L8: [65]=6282, [85]=11708, [100]=17980
    assert_eq!(t.ally_life[64], 6282, "lv65 ally_life");
    assert_eq!(t.ally_life[84], 11708, "lv85 ally_life");
    assert_eq!(t.ally_life[99], 17980, "lv100 ally_life");
}

/// vendor-only: spot checks on ally_damage (pobr has no Rust source of
/// truth for it; 2-decimal-place convention).
///
/// Source: `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L10's
/// `data.monsterAllyDamageTable`, with noisy f32 values rounded to 2
/// decimal places (e.g. [1]=3.1099998950958 → 3.11), matching the
/// `damage` table's convention.
#[test]
fn ally_damage_spot_checks_against_vendor() {
    let t = game_data().monster_scaling().unwrap();
    // Misc.lua L10: [1]≈3.11, [10]≈18.73, [20]≈50.39, [50]≈367.27
    assert_eq!(t.ally_damage[0], 3.11, "lv1 ally_damage");
    assert_eq!(t.ally_damage[9], 18.73, "lv10 ally_damage");
    assert_eq!(t.ally_damage[19], 50.39, "lv20 ally_damage");
    assert_eq!(t.ally_damage[49], 367.27, "lv50 ally_damage");
    // Misc.lua L10: [65]≈828.94, [85]≈2271.56, [100]≈4661.93
    assert_eq!(t.ally_damage[64], 828.94, "lv65 ally_damage");
    assert_eq!(t.ally_damage[84], 2271.56, "lv85 ally_damage");
    assert_eq!(t.ally_damage[99], 4661.93, "lv100 ally_damage");
}

/// The ally tables' monotonicity: minion life/damage strictly increase
/// with level (a structural sanity backstop).
#[test]
fn ally_tables_strictly_increasing() {
    let t = game_data().monster_scaling().unwrap();
    for lv in 2..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        assert!(
            t.ally_life[i] > t.ally_life[i - 1],
            "ally_life lv{} -> lv{lv} 应递增",
            lv - 1
        );
        assert!(
            t.ally_damage[i] > t.ally_damage[i - 1],
            "ally_damage lv{} -> lv{lv} 应递增",
            lv - 1
        );
    }
}

/// The hiddenDamageFixup derivation formula can be recomputed from this
/// table + SpectreBeastDamageFixup (PoB2 `CalcActiveSkill.lua:907`):
/// `fixup = round(allyDamage[lv] / damage[lv] * 1.25, 2) - 1`.
/// This verifies the derivation's inputs are all present and the result is
/// finite (the actual constant belongs to the game_constants domain;
/// wiring up the consumer is later work).
#[test]
fn hidden_damage_fixup_inputs_are_derivable() {
    let t = game_data().monster_scaling().unwrap();
    // SpectreBeastDamageFixup = 1.25 (vendor Modules/Data.lua L249, belongs
    // to the game_constants domain)
    const SPECTRE_BEAST_DAMAGE_FIXUP: f64 = 1.25;
    for lv in 1..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        let ratio = t.ally_damage[i] / t.damage[i];
        let fixup = (ratio * SPECTRE_BEAST_DAMAGE_FIXUP * 100.0).round() / 100.0 - 1.0;
        assert!(
            fixup.is_finite(),
            "lv{lv} hiddenDamageFixup 派生输入应有限，得 {fixup}"
        );
    }
    // Spot check: lv100's ratio = 4661.93 / 584.05 ≈ 7.982, fixup = round(9.98, 2) - 1 ≈ 8.98
    let lv100_fixup =
        (t.ally_damage[99] / t.damage[99] * SPECTRE_BEAST_DAMAGE_FIXUP * 100.0).round() / 100.0
            - 1.0;
    assert!(
        (lv100_fixup - 8.98).abs() < 0.01,
        "lv100 hiddenDamageFixup ≈ 8.98，得 {lv100_fixup}"
    );
}
