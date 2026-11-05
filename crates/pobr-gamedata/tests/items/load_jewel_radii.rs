//! `base/jewel_radii.json` load tests.
//!
//! The 4 named bands (Small/Medium/Large/Very Large), the distance
//! multiplier and the vendor-only parts (inner / colour / the 8 Variable
//! bands) are spot-checked against vendor PoB2 `src/Modules/Data.lua:595-611`,
//! and the built-in `JewelRadiiDef::default` must equal the file.

use pobr_data::catalog::jewel_radii::{JewelRadiiDef, JewelRadiusBandDef};
use pobr_gamedata::{GameData, repo_data_root};
use pobr_tree::JewelRadius;

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> JewelRadiiDef {
    GameData::new(repo_data_root().join(version()))
        .jewel_radii()
        .expect("jewel_radii should load")
}

/// Gets a named band from the `0_1` tree version (a named band's label is unique).
fn named_band<'a>(def: &'a JewelRadiiDef, label: &str) -> &'a JewelRadiusBandDef {
    def.tree_versions["0_1"]
        .iter()
        .find(|b| b.label == label)
        .unwrap_or_else(|| panic!("{label} tier should exist"))
}

/// Migration invariant: `JewelRadiiDef::default()` (the fallback used when
/// injection is missing) is **wholly equal** to `base/jewel_radii.json` —
/// guaranteeing "no data → Default" and "data present → injected" produce
/// value-identical output on both paths.
#[test]
fn default_fallback_equals_loaded_json() {
    assert_eq!(
        load(),
        JewelRadiiDef::default(),
        "Default fallback must equal the JSON value-for-value (migration invariant)"
    );
}

/// The distance multiplier (= 1.2, sourced from GameConstants.dat).
#[test]
fn distance_multiplier_is_the_game_constant() {
    assert_eq!(load().distance_multiplier, 1.2);
}

/// The 4 named bands' outer × multiplier is PoB2's effective radius, and
/// what `JewelRadius::units_with_radii` resolves from the file.
#[test]
fn named_bands_resolve_to_pob2_effective_radii() {
    let def = load();
    let m = def.distance_multiplier;
    let cases: [(&str, f64, JewelRadius); 4] = [
        ("Small", 1200.0, JewelRadius::Small),
        ("Medium", 1380.0, JewelRadius::Medium),
        ("Large", 1560.0, JewelRadius::Large),
        ("Very Large", 1800.0, JewelRadius::VeryLarge),
    ];
    for (label, expect_units, radius) in cases {
        let band = named_band(&def, label);
        assert_eq!(
            f64::from(band.outer) * m,
            expect_units,
            "{label} tier's outer × multiplier should equal PoB2's effective radius"
        );
        assert_eq!(f64::from(band.outer) * m, radius.units_with_radii(&def));
        // Named bands are solid circles, inner=0 (vendor Data.lua:597-600 is also 0).
        assert_eq!(band.inner, 0, "{label} tier's inner should be 0");
    }
}

/// A spot check on the named bands' base outer values (vendor
/// `Modules/Data.lua:597-600`; matches the 1000/1150/1300/1500 defined in
/// pobr-tree's consts).
#[test]
fn named_band_outer_base_values() {
    let def = load();
    assert_eq!(named_band(&def, "Small").outer, 1000);
    assert_eq!(named_band(&def, "Medium").outer, 1150);
    assert_eq!(named_band(&def, "Large").outer, 1300);
    assert_eq!(named_band(&def, "Very Large").outer, 1500);
}

/// vendor-only: the 8 Variable ring bands' inner/outer are value-equal to
/// vendor `Modules/Data.lua:602-609` (pobr has no Rust source of truth for
/// these currently, expected values hardcoded).
#[test]
fn variable_bands_match_vendor_data_lua() {
    let def = load();
    let bands = &def.tree_versions["0_1"];
    assert_eq!(bands.len(), 12, "0_1 has 4 named tiers + 8 Variable tiers");

    let variables: Vec<(u32, u32)> = bands
        .iter()
        .filter(|b| b.label == "Variable")
        .map(|b| (b.inner, b.outer))
        .collect();
    assert_eq!(
        variables,
        vec![
            (650, 950),
            (800, 1100),
            (950, 1250),
            (1100, 1400),
            (1250, 1550),
            (1400, 1700),
            (1650, 1950),
            (1800, 2100),
        ],
        "Variable tier inner/outer should match vendor Data.lua:602-609 value-for-value (in source order)"
    );
    // A Variable band's ring width is always 300 (structural to vendor's
    // table, guards against a fat-fingered value change).
    assert!(variables.iter().all(|(inner, outer)| outer - inner == 300));
}

/// vendor-only: a spot check on color codes (`Modules/Data.lua:597`'s
/// Small `^xBB6600`, `:609`'s last Variable band `^x0099FF`).
#[test]
fn colour_codes_sampled_from_vendor() {
    let def = load();
    let bands = &def.tree_versions["0_1"];
    assert_eq!(named_band(&def, "Small").colour, "^xBB6600");
    assert_eq!(named_band(&def, "Very Large").colour, "^xC100FF");
    assert_eq!(bands.last().unwrap().colour, "^x0099FF");
}
