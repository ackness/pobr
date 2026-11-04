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
        .expect("monster_scaling should load");
    assert_eq!(t.accuracy.len(), MONSTER_TABLE_LEN, "accuracy length");
    assert_eq!(t.evasion.len(), MONSTER_TABLE_LEN, "evasion length");
    assert_eq!(t.armour.len(), MONSTER_TABLE_LEN, "armour length");
    assert_eq!(t.life.len(), MONSTER_TABLE_LEN, "life length");
    assert_eq!(t.ally_life.len(), MONSTER_TABLE_LEN, "ally_life length");
    assert_eq!(t.damage.len(), MONSTER_TABLE_LEN, "damage length");
    assert_eq!(t.ally_damage.len(), MONSTER_TABLE_LEN, "ally_damage length");
    assert_eq!(
        t.ailment_threshold.len(),
        MONSTER_TABLE_LEN,
        "ailment_threshold length"
    );
    assert_eq!(
        t.poise_threshold.len(),
        MONSTER_TABLE_LEN,
        "poise_threshold length"
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
            "lv{lv} accuracy differs from MONSTER_ACCURACY_TABLE"
        );
        assert_eq!(
            t.evasion[i], MONSTER_EVASION_TABLE[i],
            "lv{lv} evasion differs from MONSTER_EVASION_TABLE"
        );
        assert_eq!(
            t.armour[i], MONSTER_ARMOUR_TABLE[i],
            "lv{lv} armour differs from MONSTER_ARMOUR_TABLE"
        );
        assert_eq!(
            t.life[i], MONSTER_LIFE_TABLE[i],
            "lv{lv} life differs from MONSTER_LIFE_TABLE"
        );
        assert_eq!(
            t.ailment_threshold[i], MONSTER_AILMENT_THRESHOLD_TABLE[i],
            "lv{lv} ailment_threshold differs from MONSTER_AILMENT_THRESHOLD_TABLE"
        );
        assert_eq!(
            t.poise_threshold[i], MONSTER_POISE_THRESHOLD_TABLE[i],
            "lv{lv} poise_threshold differs from MONSTER_POISE_THRESHOLD_TABLE"
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
            "lv{lv} damage differs from MONSTER_DAMAGE_TABLE"
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
            "ally_life lv{} -> lv{lv} should increase",
            lv - 1
        );
        assert!(
            t.ally_damage[i] > t.ally_damage[i - 1],
            "ally_damage lv{} -> lv{lv} should increase",
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
            "lv{lv} hiddenDamageFixup derivation inputs should be finite, got {fixup}"
        );
    }
    // Spot check: lv100's ratio = 4661.93 / 584.05 ≈ 7.982, fixup = round(9.98, 2) - 1 ≈ 8.98
    let lv100_fixup =
        (t.ally_damage[99] / t.damage[99] * SPECTRE_BEAST_DAMAGE_FIXUP * 100.0).round() / 100.0
            - 1.0;
    assert!(
        (lv100_fixup - 8.98).abs() < 0.01,
        "lv100 hiddenDamageFixup ≈ 8.98, got {lv100_fixup}"
    );
}

/// Switch-over invariant for the engine consumers that still call
/// `pobr_data::monster`'s lookup functions directly (minion baselines,
/// poise/ailment thresholds): the shipped JSON gives the same value as every
/// one of the nine lookups at every level, including the clamp below 1 and
/// above 100, so reading the imported table instead is value-preserving.
#[test]
fn shipped_json_equals_every_hardcoded_lookup_at_every_level() {
    use pobr_data::monster;
    let t = game_data().monster_scaling().unwrap();
    let at = |lv: u32| (lv.clamp(1, MONSTER_TABLE_LEN as u32) - 1) as usize;
    for lv in 0..=120u32 {
        let i = at(lv);
        assert_eq!(
            t.accuracy[i],
            monster::monster_accuracy(lv),
            "lv{lv} accuracy"
        );
        assert_eq!(t.evasion[i], monster::monster_evasion(lv), "lv{lv} evasion");
        assert_eq!(t.armour[i], monster::monster_armour(lv), "lv{lv} armour");
        assert_eq!(t.life[i], monster::monster_life(lv), "lv{lv} life");
        assert_eq!(
            t.ally_life[i],
            monster::monster_ally_life(lv),
            "lv{lv} ally_life"
        );
        assert_eq!(t.damage[i], monster::monster_damage(lv), "lv{lv} damage");
        assert_eq!(
            t.ally_damage[i],
            monster::monster_ally_damage(lv),
            "lv{lv} ally_damage"
        );
        assert_eq!(
            t.ailment_threshold[i],
            monster::enemy_ailment_threshold(lv),
            "lv{lv} ailment_threshold"
        );
        assert_eq!(
            t.poise_threshold[i],
            monster::enemy_poise_threshold(lv),
            "lv{lv} poise_threshold"
        );
        // The accessors the consumers switch to.
        assert_eq!(t.accuracy_at(lv), monster::monster_accuracy(lv));
        assert_eq!(t.evasion_at(lv), monster::monster_evasion(lv));
        assert_eq!(t.armour_at(lv), monster::monster_armour(lv));
        assert_eq!(t.life_at(lv), monster::monster_life(lv));
        assert_eq!(t.damage_at(lv), monster::monster_damage(lv));
        assert_eq!(
            t.ailment_threshold_at(lv),
            monster::enemy_ailment_threshold(lv)
        );
        assert_eq!(t.poise_threshold_at(lv), monster::enemy_poise_threshold(lv));
    }
    // The whole table, both ally tables included, equals the legacy arrays.
    assert_eq!(t.ally_life, monster::MONSTER_ALLY_LIFE_TABLE.to_vec());
    assert_eq!(t.ally_damage, monster::MONSTER_ALLY_DAMAGE_TABLE.to_vec());
}
