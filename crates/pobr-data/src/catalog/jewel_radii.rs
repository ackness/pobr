//! Radius-jewel ring-band domain schema (`base/jewel_radii.json`).
//!
//! Data source:
//! - The band table: vendor PoB2 `src/Modules/Data.lua:595-611`'s
//!   `data.jewelRadii["0_1"]` (4 named bands
//!   Small/Medium/Large/Very Large + 8 Variable ring bands with
//!   `inner > 0`);
//! - The distance multiplier: vendor PoB2 `src/Data/Misc.lua:36`'s
//!   `gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2`
//!   (transcribed from `GameConstants.dat`).
//!
//! pobr's existing Rust source of truth: `crates/pobr-tree/src/radius_jewel.rs`'s
//! `PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` and
//! `JEWEL_RADIUS_{SMALL,MEDIUM,LARGE,VERY_LARGE}` (the 4 named bands'
//! outer and the multiplier already match value-for-value); `inner`/`colour`
//! and the 8 Variable bands are a vendor-only addition (pobr-tree's first
//! pass covers Variable with `JewelRadius::Custom`, without inner semantics).
//!
//! Distance-check semantics (matching PoB2
//! `Modules/Data.lua:584-586`'s `setJewelRadiiGlobally`): a node falls in a
//! ring band ⇔ `inner² × m² <= dx² + dy² <= outer² × m²`, where
//! `m = distance_multiplier`; i.e. both inner/outer are base radii before
//! the scaling factor is applied (tree units).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Top level of `base/jewel_radii.json`: the distance multiplier plus a
/// per-tree-version ring-band table.
///
/// Tree version keys look like `"0_1"` (major_minor); at runtime, per PoB2
/// `setJewelRadiiGlobally`'s semantics, the latest set `<=` the target tree
/// version is selected (currently there's only one set, `0_1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JewelRadiiDef {
    /// The passive tree's jewel distance scaling factor
    /// (`PassiveTreeJewelDistanceMultiplier`, currently 1.2).
    ///
    /// Source: `Data/Misc.lua:36` (GameConstants.dat). pobr's source of
    /// truth: `pobr-tree::PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` (matches value-for-value).
    pub distance_multiplier: f64,
    /// `tree version -> ring-band array` (keeps vendor `Data.lua`'s
    /// writing order: the 4 named bands first, then the 8 Variable bands).
    pub tree_versions: BTreeMap<String, Vec<JewelRadiusBandDef>>,
}

impl Default for JewelRadiiDef {
    /// `Default` = the fallback used when injection is missing,
    /// **value-equal** to `base/jewel_radii.json` field by field (a
    /// migration invariant: both the injected and fallback paths produce
    /// the same output).
    ///
    /// This domain's pobr Rust source of truth
    /// (`pobr-tree::PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` /
    /// `JEWEL_RADIUS_*`) lives in a crate that's **unreachable in the
    /// dependency direction** here (pobr-tree depends on pobr-data), so
    /// it's transcribed as literals from vendor PoB2
    /// `Modules/Data.lua:595-611` + `Data/Misc.lua:36`; value-for-value
    /// agreement is locked by two tests:
    /// - `pobr-tree`'s `tests/radius_jewel.rs`: Default's named bands ×
    ///   multiplier == the old consts;
    /// - `pobr-gamedata`'s `tests/load_jewel_radii.rs`: Default == the JSON, fully.
    fn default() -> Self {
        // A convenience constructor for a band row (Default only).
        fn band(label: &str, inner: u32, outer: u32, colour: &str) -> JewelRadiusBandDef {
            JewelRadiusBandDef {
                label: label.to_string(),
                inner,
                outer,
                colour: colour.to_string(),
            }
        }
        // vendor `Modules/Data.lua:595-611`'s `data.jewelRadii["0_1"]`:
        // the 4 named bands (inner=0, solid circles) first, then the 8
        // Variable ring bands (in their original writing order).
        let bands = vec![
            band("Small", 0, 1000, "^xBB6600"),
            band("Medium", 0, 1150, "^x66FFCC"),
            band("Large", 0, 1300, "^x2222CC"),
            band("Very Large", 0, 1500, "^xC100FF"),
            band("Variable", 650, 950, "^xD35400"),
            band("Variable", 800, 1100, "^x66FFCC"),
            band("Variable", 950, 1250, "^x2222CC"),
            band("Variable", 1100, 1400, "^xC100FF"),
            band("Variable", 1250, 1550, "^x0B9300"),
            band("Variable", 1400, 1700, "^xFFCC00"),
            band("Variable", 1650, 1950, "^xFF6600"),
            band("Variable", 1800, 2100, "^x0099FF"),
        ];
        Self {
            // `Data/Misc.lua:36`'s PassiveTreeJewelDistanceMultiplier (GameConstants.dat).
            distance_multiplier: 1.2,
            tree_versions: BTreeMap::from([("0_1".to_string(), bands)]),
        }
    }
}

/// A single ring band (corresponds to one row of PoB2's `data.jewelRadii`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JewelRadiusBandDef {
    /// Band label: `Small` / `Medium` / `Large` / `Very Large` / `Variable`.
    pub label: String,
    /// Base inner radius (tree units, before the scaling factor); 0 (a
    /// solid circle) for the named bands. A vendor-only field (pobr-tree's
    /// first pass has no inner semantics), sourced from `Modules/Data.lua:602-609`.
    pub inner: u32,
    /// Base outer radius (tree units, before the scaling factor). For the
    /// named bands this matches pobr-tree's `JEWEL_RADIUS_*`
    /// (= outer × 1.2) value-for-value.
    pub outer: u32,
    /// UI highlight color code (PoB2's `col` field, `^xRRGGBB` format). A
    /// vendor-only display field.
    pub colour: String,
}

#[cfg(test)]
mod tests {
    use super::JewelRadiiDef;

    /// A spot check on the Default fallback's structure (the full
    /// value-by-value comparisons live elsewhere: against pobr-tree's old
    /// consts in `pobr-tree/tests/radius_jewel.rs`, and full equality with
    /// the JSON in `pobr-gamedata/tests/load_jewel_radii.rs`).
    #[test]
    fn default_has_vendor_zero_one_table() {
        let def = JewelRadiiDef::default();
        assert_eq!(def.distance_multiplier, 1.2);
        let bands = &def.tree_versions["0_1"];
        assert_eq!(bands.len(), 12, "4 具名档 + 8 Variable 档");
        assert_eq!(bands[0].label, "Small");
        assert_eq!(bands[0].outer, 1000);
        assert_eq!(bands[3].label, "Very Large");
        assert_eq!(bands[3].outer, 1500);
        // A Variable band's ring width is always 300 (structural to vendor's table).
        assert!(
            bands
                .iter()
                .filter(|b| b.label == "Variable")
                .all(|b| b.outer - b.inner == 300)
        );
    }
}
