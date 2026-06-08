//! 集成测试：radius jewel 影响范围计算。
//!
//! 常量基准：PoB2 `src/Modules/Data.lua` `data.jewelRadii["0_1"]`（outer 值）；
//! 缩放因子：PoB2 `Data/Misc.lua` `data.gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2`。
//!
//! 实际有效半径 = outer * 1.2：
//!   Small 1000 → 1200，Medium 1150 → 1380，Large 1300 → 1560，VeryLarge 1500 → 1800。
//!
//! fixture 节点按与 socket 距离排列：
//!   11 → 600，12 → 1000，13 → 1300，14 → 1400，15 → 2000（均在 Small/Medium/Large 范围之外）。

use pobr_data::prelude::*;
use pobr_tree::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JEWEL_RADIUS_VERY_LARGE,
    JewelRadius, PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER, PassiveTree, RadiusJewelEffect, TreeError,
    compute_radius_jewel_effect,
};
use std::collections::HashMap;

/// socket(skill 10) 在原点；节点按与 socket 距离递增排布。
///
/// | skill | 距离      | 落在...         |
/// |-------|-----------|-----------------|
/// |  11   |   600     | Small(1200)内   |
/// |  12   |  1000     | Small(1200)内   |
/// |  13   |  1300     | Medium(1380)内  |
/// |  14   |  1400     | Large(1560)内   |
/// |  15   |  2000     | 全部范围外       |
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

/// PoE2 出处：PoB2 Data/Misc.lua gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2
#[test]
fn jewel_distance_multiplier_is_1_2() {
    assert_eq!(
        PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER, 1.2,
        "PassiveTreeJewelDistanceMultiplier must be 1.2 per PoB2 Data/Misc.lua GameConstants.dat"
    );
}

/// PoE2 出处：PoB2 Data.lua jewelRadii["0_1"] outer 值乘以 1.2 缩放因子。
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

/// Small 有效半径 1200：捕获距离 600 和 1000 的节点，排除距离 1300/1400/2000 的节点。
#[test]
fn small_radius_captures_nodes_within_1200() {
    let positions = fixture_positions();
    let mods = vec!["10% increased Damage".to_string()];

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::Small, &positions, mods.clone()).unwrap();

    assert_eq!(effect.socket, 10);
    // dist 600 和 1000 均在 1200 内；dist 1300/1400/2000 在外
    assert_eq!(effect.affected_nodes, vec![11, 12]);
    assert_eq!(effect.mod_texts, mods);
}

/// Medium 有效半径 1380：比 Small 多捕获距离 1300 的节点。
#[test]
fn medium_radius_captures_nodes_within_1380() {
    let positions = fixture_positions();

    let effect = compute_radius_jewel_effect(10, JewelRadius::Medium, &positions, vec![]).unwrap();

    assert_eq!(effect.affected_nodes, vec![11, 12, 13]);
}

/// Large 有效半径 1560：比 Medium 多捕获距离 1400 的节点。
#[test]
fn large_radius_captures_nodes_within_1560() {
    let positions = fixture_positions();

    let effect = compute_radius_jewel_effect(10, JewelRadius::Large, &positions, vec![]).unwrap();

    assert_eq!(effect.affected_nodes, vec![11, 12, 13, 14]);
}

/// VeryLarge 有效半径 1800：仍不捕获距离 2000 的节点。
#[test]
fn very_large_radius_captures_nodes_within_1800() {
    let positions = fixture_positions();

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::VeryLarge, &positions, vec![]).unwrap();

    // dist 2000 仍在 1800 外
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
    // Large 有效半径 1560：捕获 dist 600/1000/1300/1400 的节点（11/12/13/14）
    assert_eq!(via_method.affected_nodes, vec![11, 12, 13, 14]);
}
