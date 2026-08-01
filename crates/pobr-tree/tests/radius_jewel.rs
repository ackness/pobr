//! Integration tests: radius jewel effect-range calculation.
//!
//! Constant baseline: PoB2 `src/Modules/Data.lua` `data.jewelRadii["0_1"]`
//! (outer values); scaling factor: PoB2 `Data/Misc.lua`
//! `data.gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2`.
//!
//! Effective radius = outer * 1.2:
//!   Small 1000 -> 1200, Medium 1150 -> 1380, Large 1300 -> 1560, VeryLarge 1500 -> 1800.
//!
//! Fixture nodes are laid out by distance from the socket:
//!   11 -> 600, 12 -> 1000, 13 -> 1300, 14 -> 1400, 15 -> 2000 (outside every band).

use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::prelude::*;
use pobr_tree::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JEWEL_RADIUS_VERY_LARGE,
    JewelRadius, PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER, PassiveTree, RadiusJewelEffect, TreeError,
    compute_radius_jewel_effect, compute_radius_jewel_effect_with_radii,
};
use std::collections::HashMap;

/// socket (skill 10) sits at the origin; nodes are placed at increasing
/// distances from it.
///
/// | skill | distance  | falls within...  |
/// |-------|-----------|-------------------|
/// |  11   |   600     | Small (1200)      |
/// |  12   |  1000     | Small (1200)      |
/// |  13   |  1300     | Medium (1380)     |
/// |  14   |  1400     | Large (1560)      |
/// |  15   |  2000     | outside all bands |
fn fixture_positions() -> HashMap<u32, (f64, f64)> {
    let mut map = HashMap::new();
    map.insert(10u32, (0.0, 0.0)); // socket
    map.insert(11u32, (600.0, 0.0)); // dist 600
    map.insert(12u32, (0.0, 1000.0)); // dist 1000
    map.insert(13u32, (1300.0, 0.0)); // dist 1300
    map.insert(14u32, (1400.0, 0.0)); // dist 1400
    map.insert(15u32, (2000.0, 0.0)); // dist 2000
    map
}

fn node(skill: u32, kind: PassiveNodeKind) -> PassiveNodeDef {
    PassiveNodeDef {
        apply_to_armour: false,
        skill,
        id: format!("node_{skill}"),
        name: Some(format!("Node {skill}")),
        kind,
        stats: vec![format!("stat for {skill}")],
        group: None,
        orbit: None,
        orbit_index: None,
        x: None,
        y: None,
        connections: vec![],
        ascendancy_id: None,
        variants: vec![],
    }
}

fn fixture_tree() -> PassiveTree {
    let nodes = vec![
        node(10, PassiveNodeKind::JewelSocket),
        node(11, PassiveNodeKind::Normal),
        node(12, PassiveNodeKind::Normal),
        node(13, PassiveNodeKind::Notable),
        node(14, PassiveNodeKind::Normal),
        node(15, PassiveNodeKind::Notable),
    ];
    PassiveTree::from_nodes(nodes).with_positions(fixture_positions())
}

/// PoE2 source: PoB2 Data/Misc.lua gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2
#[test]
fn jewel_distance_multiplier_is_1_2() {
    assert_eq!(
        PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER, 1.2,
        "PassiveTreeJewelDistanceMultiplier must be 1.2 per PoB2 Data/Misc.lua GameConstants.dat"
    );
}

/// PoE2 source: PoB2 Data.lua jewelRadii["0_1"] outer values times the 1.2 scaling factor.
#[test]
fn radius_constants_match_pob2_outer_times_multiplier() {
    // PoB2 Data.lua: { inner=0, outer=1000, label="Small"  }  → 1000 * 1.2 = 1200
    // PoB2 Data.lua: { inner=0, outer=1150, label="Medium" }  → 1150 * 1.2 = 1380
    // PoB2 Data.lua: { inner=0, outer=1300, label="Large"  }  → 1300 * 1.2 = 1560
    // PoB2 Data.lua: { inner=0, outer=1500, label="Very Large" } → 1500 * 1.2 = 1800
    assert_eq!(
        JEWEL_RADIUS_SMALL, 1200.0,
        "Small effective radius = 1000 * 1.2"
    );
    assert_eq!(
        JEWEL_RADIUS_MEDIUM, 1380.0,
        "Medium effective radius = 1150 * 1.2"
    );
    assert_eq!(
        JEWEL_RADIUS_LARGE, 1560.0,
        "Large effective radius = 1300 * 1.2"
    );
    assert_eq!(
        JEWEL_RADIUS_VERY_LARGE, 1800.0,
        "VeryLarge effective radius = 1500 * 1.2"
    );
}

#[test]
fn radius_constants_are_ordered() {
    let small = JewelRadius::Small.units();
    let medium = JewelRadius::Medium.units();
    let large = JewelRadius::Large.units();
    let very_large = JewelRadius::VeryLarge.units();
    assert!(
        small < medium,
        "small ({small}) should be < medium ({medium})"
    );
    assert!(
        medium < large,
        "medium ({medium}) should be < large ({large})"
    );
    assert!(
        large < very_large,
        "large ({large}) should be < very_large ({very_large})"
    );
}

/// Small's effective radius is 1200: captures the nodes at distance 600 and
/// 1000, excludes the ones at 1300/1400/2000.
#[test]
fn small_radius_captures_nodes_within_1200() {
    let positions = fixture_positions();
    let mods = vec!["10% increased Damage".to_string()];

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::Small, &positions, mods.clone()).unwrap();

    assert_eq!(effect.socket, 10);
    // dist 600 and 1000 are both within 1200; dist 1300/1400/2000 are outside.
    assert_eq!(effect.affected_nodes, vec![11, 12]);
    assert_eq!(effect.mod_texts, mods);
}

/// Medium's effective radius is 1380: captures one more node than Small, at distance 1300.
#[test]
fn medium_radius_captures_nodes_within_1380() {
    let positions = fixture_positions();

    let effect = compute_radius_jewel_effect(10, JewelRadius::Medium, &positions, vec![]).unwrap();

    assert_eq!(effect.affected_nodes, vec![11, 12, 13]);
}

/// Large's effective radius is 1560: captures one more node than Medium, at distance 1400.
#[test]
fn large_radius_captures_nodes_within_1560() {
    let positions = fixture_positions();

    let effect = compute_radius_jewel_effect(10, JewelRadius::Large, &positions, vec![]).unwrap();

    assert_eq!(effect.affected_nodes, vec![11, 12, 13, 14]);
}

/// VeryLarge's effective radius is 1800: still doesn't capture the node at distance 2000.
#[test]
fn very_large_radius_captures_nodes_within_1800() {
    let positions = fixture_positions();

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::VeryLarge, &positions, vec![]).unwrap();

    // dist 2000 is still outside 1800.
    assert_eq!(effect.affected_nodes, vec![11, 12, 13, 14]);
}

#[test]
fn socket_node_is_excluded_from_affected() {
    let positions = fixture_positions();

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::VeryLarge, &positions, vec![]).unwrap();

    assert!(!effect.affected_nodes.contains(&10));
}

#[test]
fn custom_radius_captures_farthest_node() {
    let positions = fixture_positions();

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::Custom(2500.0), &positions, vec![]).unwrap();

    let mut ids = effect.affected_nodes.clone();
    ids.sort_unstable();
    assert_eq!(ids, vec![11, 12, 13, 14, 15]);
}

#[test]
fn missing_socket_position_returns_error() {
    let positions = fixture_positions();

    let result = compute_radius_jewel_effect(999, JewelRadius::Small, &positions, vec![]);

    match result {
        Err(TreeError::NodePositionMissing(id)) => assert_eq!(id, 999),
        other => panic!("expected NodePositionMissing(999), got {other:?}"),
    }
}

#[test]
fn negative_custom_radius_is_invalid() {
    let positions = fixture_positions();

    let result = compute_radius_jewel_effect(10, JewelRadius::Custom(-1.0), &positions, vec![]);

    match result {
        Err(TreeError::InvalidRadius(r)) => assert_eq!(r, -1.0),
        other => panic!("expected InvalidRadius(-1.0), got {other:?}"),
    }
}

#[test]
fn radius_to_units_matches_constants() {
    assert_eq!(JewelRadius::Small.units(), JEWEL_RADIUS_SMALL);
    assert_eq!(JewelRadius::Medium.units(), JEWEL_RADIUS_MEDIUM);
    assert_eq!(JewelRadius::Large.units(), JEWEL_RADIUS_LARGE);
    assert_eq!(JewelRadius::VeryLarge.units(), JEWEL_RADIUS_VERY_LARGE);
    assert_eq!(JewelRadius::Custom(123.0).units(), 123.0);
}

/// Migration invariant: the `JewelRadiiDef::default()` injection path and the
/// legacy hardcoded-constant path are **value-for-value equal** —
/// `units_with_radii(Default)` == `units()` (all 4 named bands plus Custom).
/// This test anchors pobr-data's `Default` fallback to this crate's old
/// Rust-source constants (pobr-data's dependency direction can't reference
/// this crate's constants directly, so this is pinned here instead).
#[test]
fn default_radii_data_matches_legacy_constants() {
    let radii = JewelRadiiDef::default();
    for r in [
        JewelRadius::Small,
        JewelRadius::Medium,
        JewelRadius::Large,
        JewelRadius::VeryLarge,
        JewelRadius::Custom(123.0),
    ] {
        assert_eq!(
            r.units_with_radii(&radii),
            r.units(),
            "{r:?} 注入 Default 数据与旧常量路径必须逐值相等"
        );
    }
    assert_eq!(
        radii.distance_multiplier,
        PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER
    );
}

/// The injection path and fallback path produce the same result (under
/// Default data, the whole effect range is value-for-value equal).
#[test]
fn with_radii_default_matches_fallback_function() {
    let positions = fixture_positions();
    let radii = JewelRadiiDef::default();
    for r in [
        JewelRadius::Small,
        JewelRadius::Medium,
        JewelRadius::Large,
        JewelRadius::VeryLarge,
    ] {
        let injected =
            compute_radius_jewel_effect_with_radii(10, r, &radii, &positions, vec![]).unwrap();
        let fallback = compute_radius_jewel_effect(10, r, &positions, vec![]).unwrap();
        assert_eq!(injected, fallback, "{r:?} 注入/回退两条路径输出必须一致");
    }
}

/// The injected data is actually consumed: tampering with the Small band's
/// outer value shifts the affected set (proving the calculation reads the
/// injected data, not hardcoded constants).
#[test]
fn injected_radii_data_is_actually_consumed() {
    let positions = fixture_positions();
    let mut radii = JewelRadiiDef::default();
    // Small outer 1000 -> 1100: effective radius 1200 -> 1320, which should
    // now capture node 13 at dist 1300 (node 14 at dist 1400 is still outside).
    let bands = radii.tree_versions.get_mut("0_1").unwrap();
    bands.iter_mut().find(|b| b.label == "Small").unwrap().outer = 1100;

    let effect =
        compute_radius_jewel_effect_with_radii(10, JewelRadius::Small, &radii, &positions, vec![])
            .unwrap();
    assert_eq!(
        effect.affected_nodes,
        vec![11, 12, 13],
        "篡改后的 Small 档（1320）应捕获 dist 600/1000/1300"
    );
}

/// When the data is missing the named band (malformed/truncated data), this
/// falls back to the hardcoded constant, matching fallback behaviour.
#[test]
fn missing_band_falls_back_to_legacy_constant() {
    let positions = fixture_positions();
    let mut radii = JewelRadiiDef::default();
    radii
        .tree_versions
        .get_mut("0_1")
        .unwrap()
        .retain(|b| b.label != "Small");

    let effect =
        compute_radius_jewel_effect_with_radii(10, JewelRadius::Small, &radii, &positions, vec![])
            .unwrap();
    // Falls back to JEWEL_RADIUS_SMALL (1200): captures dist 600/1000.
    assert_eq!(effect.affected_nodes, vec![11, 12]);
}

/// Tree version selection: with multiple version groups present, the max key
/// (the newest group) is used.
#[test]
fn latest_tree_version_bands_are_used() {
    let positions = fixture_positions();
    let mut radii = JewelRadiiDef::default();
    // Add a newer "0_2" version: Small's outer changed to 1100 (effective
    // 1320, now also captures node 13).
    let mut newer = radii.tree_versions["0_1"].clone();
    newer.iter_mut().find(|b| b.label == "Small").unwrap().outer = 1100;
    radii.tree_versions.insert("0_2".to_string(), newer);

    let effect =
        compute_radius_jewel_effect_with_radii(10, JewelRadius::Small, &radii, &positions, vec![])
            .unwrap();
    assert_eq!(
        effect.affected_nodes,
        vec![11, 12, 13],
        "应消费最新树版本（0_2）的档位表"
    );
}

/// The `PassiveTree` injection-aware method matches the free function.
#[test]
fn tree_method_with_radii_matches_free_function() {
    let tree = fixture_tree();
    let radii = JewelRadiiDef::default();

    let via_method = tree
        .radius_jewel_effect_with_radii(
            NodeId(10),
            JewelRadius::Large,
            &radii,
            vec!["m".to_string()],
        )
        .unwrap();
    let via_fn = compute_radius_jewel_effect_with_radii(
        10,
        JewelRadius::Large,
        &radii,
        &tree.positions,
        vec!["m".to_string()],
    )
    .unwrap();

    assert_eq!(via_method, via_fn);
    assert_eq!(via_method.affected_nodes, vec![11, 12, 13, 14]);
}

#[test]
fn effect_is_serializable() {
    let effect = RadiusJewelEffect {
        socket: 10,
        affected_nodes: vec![11],
        mod_texts: vec!["x".to_string()],
    };

    let json = serde_json::to_string(&effect).unwrap();
    let back: RadiusJewelEffect = serde_json::from_str(&json).unwrap();

    assert_eq!(back, effect);
}

#[test]
fn tree_method_matches_free_function() {
    let tree = fixture_tree();

    let via_method = tree
        .radius_jewel_effect(NodeId(10), JewelRadius::Large, vec!["m".to_string()])
        .unwrap();
    let via_fn = compute_radius_jewel_effect(
        10,
        JewelRadius::Large,
        &tree.positions,
        vec!["m".to_string()],
    )
    .unwrap();

    assert_eq!(via_method, via_fn);
    // Large's effective radius is 1560: captures the nodes at dist
    // 600/1000/1300/1400 (11/12/13/14).
    assert_eq!(via_method.affected_nodes, vec![11, 12, 13, 14]);
}
