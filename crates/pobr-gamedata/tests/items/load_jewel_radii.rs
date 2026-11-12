//! Jewel bands remain usable by geometry consumers when balance values change.

use pobr_gamedata::{GameData, current_data_dir};
use pobr_tree::JewelRadius;

#[test]
fn tree_versions_have_valid_bands_and_named_radii_resolve() {
    let def = GameData::new(current_data_dir())
        .jewel_radii()
        .expect("jewel_radii should load");
    assert!(def.distance_multiplier.is_finite() && def.distance_multiplier > 0.0);
    assert!(!def.tree_versions.is_empty());
    for (version, bands) in &def.tree_versions {
        assert!(!version.is_empty() && !bands.is_empty());
        for band in bands {
            assert!(!band.label.is_empty());
            assert!(band.outer > band.inner);
            let colour = band.colour.strip_prefix("^x").expect("hex colour prefix");
            assert!(colour.len() == 6 && colour.bytes().all(|b| b.is_ascii_hexdigit()));
        }
        let variables: Vec<_> = bands
            .iter()
            .filter(|band| band.label == "Variable")
            .collect();
        assert!(!variables.is_empty(), "{version}: variable bands");
        assert!(
            variables.iter().all(|band| band.inner > 0),
            "ring interiors"
        );
        let mut previous = 0;
        for (label, radius) in [
            ("Small", JewelRadius::Small),
            ("Medium", JewelRadius::Medium),
            ("Large", JewelRadius::Large),
            ("Very Large", JewelRadius::VeryLarge),
        ] {
            let named: Vec<_> = bands.iter().filter(|band| band.label == label).collect();
            assert_eq!(named.len(), 1, "{version}: {label}");
            assert_eq!(named[0].inner, 0, "named radii are solid circles");
            assert!(named[0].outer > previous, "ordered named radii");
            previous = named[0].outer;
            assert_eq!(
                radius.units_for_tree(&def, Some(version)),
                f64::from(named[0].outer) * def.distance_multiplier,
            );
        }
    }
}
