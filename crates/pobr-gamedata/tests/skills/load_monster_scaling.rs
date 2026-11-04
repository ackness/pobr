//! `base/monster_scaling.json` load tests.
//!
//! The JSON is the only copy of these tables (the engine's former Rust copy
//! was deleted after a level-by-level equality check); these tests pin the
//! shape and spot values the calculation and its golden tests rely on.

use pobr_data::catalog::monster_scaling::MONSTER_SCALING_TABLE_LEN as MONSTER_TABLE_LEN;
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

/// Spot values of the seven enemy tables at levels 1 / 20 / 65 / 85 / 100
/// (`DefaultMonsterStats`; the same values the former Rust copy held).
#[test]
fn enemy_tables_spot_checks() {
    let t = game_data().monster_scaling().unwrap();
    let at = |lv: usize| lv - 1;
    let levels = [1, 20, 65, 85, 100];
    let rows: [(&str, Vec<u32>, [u32; 5]); 6] = [
        ("accuracy", t.accuracy.clone(), [32, 140, 1114, 2357, 4011]),
        ("evasion", t.evasion.clone(), [24, 160, 663, 996, 1304]),
        ("armour", t.armour.clone(), [3, 89, 2023, 6355, 14441]),
        ("life", t.life.clone(), [15, 249, 6555, 36012, 56106]),
        (
            "ailment_threshold",
            t.ailment_threshold.clone(),
            [15, 275, 9649, 79967, 123287],
        ),
        (
            "poise_threshold",
            t.poise_threshold.clone(),
            [30, 542, 26703, 271810, 446335],
        ),
    ];
    for (name, table, expected) in rows {
        for (lv, want) in levels.into_iter().zip(expected) {
            assert_eq!(table[at(lv)], want, "lv{lv} {name}");
        }
    }
    for (lv, want) in levels
        .into_iter()
        .zip([9.16, 37.29, 212.36, 385.42, 584.05])
    {
        assert_eq!(t.damage[at(lv)], want, "lv{lv} damage");
    }
}

/// The accessors clamp below 1 and above 100 to the table edges.
#[test]
fn accessors_clamp_to_table_edges() {
    let t = game_data().monster_scaling().unwrap();
    assert_eq!(t.accuracy_at(0), t.accuracy[0]);
    assert_eq!(t.accuracy_at(999), t.accuracy[99]);
    assert_eq!(t.ally_life_at(0), 51);
    assert_eq!(t.ally_life_at(120), 17980);
    assert_eq!(t.ally_damage_at(40), t.ally_damage[39]);
    assert_eq!(t.poise_threshold_at(85), 271810);
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
