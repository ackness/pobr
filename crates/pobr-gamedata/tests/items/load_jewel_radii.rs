//! `base/jewel_radii.json` load tests.
//!
//! Migration-invariant checks: the 4 named bands
//! (Small/Medium/Large/Very Large) plus the distance multiplier are
//! **value-equal** to pobr's existing Rust source-of-truth consts in
//! `crates/pobr-tree/src/radius_jewel.rs`; the vendor-only parts (inner /
//! colour / the 8 Variable bands) are spot-checked against hardcoded
//! expected values per vendor PoB2 `src/Modules/Data.lua:595-611`.

use pobr_data::catalog::jewel_radii::{JewelRadiiDef, JewelRadiusBandDef};
use pobr_gamedata::{GameData, repo_data_root};
use pobr_tree::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JEWEL_RADIUS_VERY_LARGE,
    JewelRadius, PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER,
};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> JewelRadiiDef {
    GameData::new(repo_data_root().join(version()))
        .jewel_radii()
        .expect("jewel_radii 可加载")
}

/// Gets a named band from the `0_1` tree version (a named band's label is unique).
fn named_band<'a>(def: &'a JewelRadiiDef, label: &str) -> &'a JewelRadiusBandDef {
    def.tree_versions["0_1"]
        .iter()
        .find(|b| b.label == label)
        .unwrap_or_else(|| panic!("存在 {label} 档"))
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
        "Default fallback 必须与 JSON 逐值相等（搬迁不变式）"
    );
}

/// The distance multiplier is value-equal to pobr-tree's source-of-truth
/// const (= 1.2, sourced from GameConstants.dat).
#[test]
fn distance_multiplier_matches_pobr_tree_constant() {
    let def = load();
    assert_eq!(
        def.distance_multiplier,
        PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER
    );
    assert_eq!(def.distance_multiplier, 1.2);
}

/// The 4 named bands' outer × multiplier is value-equal to pobr-tree's
/// `JEWEL_RADIUS_*` consts, and matches `JewelRadius::units()`'s behavior
/// (a migration invariant).
#[test]
fn named_bands_match_pobr_tree_radius_constants() {
    let def = load();
    let m = def.distance_multiplier;
    let cases: [(&str, f64, JewelRadius); 4] = [
        ("Small", JEWEL_RADIUS_SMALL, JewelRadius::Small),
        ("Medium", JEWEL_RADIUS_MEDIUM, JewelRadius::Medium),
        ("Large", JEWEL_RADIUS_LARGE, JewelRadius::Large),
        (
            "Very Large",
            JEWEL_RADIUS_VERY_LARGE,
            JewelRadius::VeryLarge,
        ),
    ];
    for (label, expect_units, radius) in cases {
        let band = named_band(&def, label);
        assert_eq!(
            f64::from(band.outer) * m,
            expect_units,
            "{label} 档 outer×乘数应等于 pobr-tree 常量"
        );
        assert_eq!(f64::from(band.outer) * m, radius.units());
        // Named bands are solid circles, inner=0 (vendor Data.lua:597-600 is also 0).
        assert_eq!(band.inner, 0, "{label} 档 inner 应为 0");
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
    assert_eq!(bands.len(), 12, "0_1 共 4 具名档 + 8 Variable 档");

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
        "Variable 档 inner/outer 应与 vendor Data.lua:602-609 逐值一致（保持书写顺序）"
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
