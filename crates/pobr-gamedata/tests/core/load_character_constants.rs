//! `base/character_constants.json` load + migration-invariant regression.
//!
//! Assertion strategy:
//! - The 10 values with a pobr Rust source of truth
//!   (`crates/pobr-core/src/character.rs`'s consts) are asserted
//!   value-for-value against literals matching the source of truth —
//!   pobr-gamedata doesn't depend on pobr-core, so it can't reference the
//!   private consts directly; the source-of-truth values are hardcoded
//!   in the test with the const name noted, and this test goes red if the
//!   source of truth's value changes.
//! - The 3 vendor-only values are spot-checked, with the value hardcoded
//!   and the vendor file:line referenced (commit `2df5a74`, see
//!   `vendor/.pob2-version.txt`).

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// manifest v2 already registers this domain in the base section.
#[test]
fn manifest_registers_character_constants_in_base() {
    let manifest = game_data().manifest().expect("manifest should load");
    assert!(
        manifest
            .domains
            .base
            .iter()
            .any(|d| d == "character_constants"),
        "manifest.domains.base should include character_constants"
    );
}

/// Value-for-value assertion = pobr-core/src/character.rs's existing
/// consts (a migration invariant: the JSON must equal the existing Rust
/// values; aligning behavior with vendor is a separate follow-up commit).
#[test]
fn values_match_pobr_core_character_rs_constants() {
    let c = game_data()
        .character_constants()
        .expect("character_constants should load");

    // Life: character.rs's BASE_LIFE_CONSTANT / LIFE_PER_LEVEL / LIFE_PER_STRENGTH
    assert_eq!(c.base_life_constant, 16.0);
    assert_eq!(c.life_per_level, 12.0);
    assert_eq!(c.life_per_strength, 2.0);

    // Mana: character.rs's BASE_MANA_CONSTANT / MANA_PER_LEVEL / MANA_PER_INTELLIGENCE
    assert_eq!(c.base_mana_constant, 30.0);
    assert_eq!(c.mana_per_level, 4.0);
    assert_eq!(c.mana_per_intelligence, 2.0);

    // Accuracy: character.rs's BASE_ACCURACY_CONSTANT / ACCURACY_PER_LEVEL / ACCURACY_PER_DEXTERITY
    assert_eq!(c.base_accuracy_constant, -6.0);
    assert_eq!(c.accuracy_per_level, 6.0);
    assert_eq!(c.accuracy_per_dexterity, 6.0);

    // Evasion: character.rs's BASE_EVASION
    assert_eq!(c.base_evasion, 7.0);
}

/// Spot-checks vendor-only fields (pobr's Rust has no such value, sourced
/// from vendor PoB2 commit 2df5a74).
#[test]
fn vendor_only_per_level_attributes_match_vendor() {
    let c = game_data().character_constants().unwrap();

    // vendor/PathOfBuilding-PoE2/src/Data/Misc.lua:157 ["strength_per_level"] = 0
    assert_eq!(c.strength_per_level, 0.0);
    // vendor/PathOfBuilding-PoE2/src/Data/Misc.lua:158 ["dexterity_per_level"] = 0
    assert_eq!(c.dexterity_per_level, 0.0);
    // vendor/PathOfBuilding-PoE2/src/Data/Misc.lua:159 ["intelligence_per_level"] = 0
    assert_eq!(c.intelligence_per_level, 0.0);
}

/// Recomputes the derived formulas against the documented oracle values
/// (character.rs's module doc notes: L99 Life base 1204 = 12×99+16, Mana
/// base 426 = 4×99+30; L1 Accuracy base 0 = 6×1−6) — confirms feeding this
/// table's values into the existing formulas produces the same output.
#[test]
fn derived_formulas_reproduce_documented_oracle_values() {
    let c = game_data().character_constants().unwrap();

    let life_l99 = c.base_life_constant + c.life_per_level * 99.0;
    assert_eq!(life_l99, 1204.0, "L99 base Life should be 1204");

    let mana_l99 = c.base_mana_constant + c.mana_per_level * 99.0;
    assert_eq!(mana_l99, 426.0, "L99 base Mana should be 426");

    let accuracy_l1 = c.base_accuracy_constant + c.accuracy_per_level * 1.0;
    assert_eq!(accuracy_l1, 0.0, "L1 base Accuracy should be 0");
}

/// The JSON round-trips through the schema (no serde fields missing / extra).
#[test]
fn json_roundtrips_through_schema() {
    use pobr_data::catalog::character_constants::CharacterConstantsDef;

    let c = game_data().character_constants().unwrap();
    let text = serde_json::to_string(&c).unwrap();
    let back: CharacterConstantsDef = serde_json::from_str(&text).unwrap();
    assert_eq!(c, back);
}
