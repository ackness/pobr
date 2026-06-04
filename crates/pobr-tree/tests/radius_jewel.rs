//! 集成测试：radius jewel 影响范围计算。
//!
//! 坐标系/半径常数为自构造，**可能与真实 PoE2 树不符**（见 blocked_by_missing_data）。
//! 这里只验证欧氏距离筛选逻辑：以 socket 坐标为圆心，半径内（不含 socket 本身）的节点。

use pobr_data::prelude::*;
use pobr_tree::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JewelRadius, PassiveTree,
    RadiusJewelEffect, TreeError, compute_radius_jewel_effect,
};
use std::collections::HashMap;

/// socket(skill 10) 在原点；其它节点按距离递增排布。
/// 11 距离 600；12 距离 1000；13 距离 2000。
fn fixture_positions() -> HashMap<u32, (f64, f64)> {
    let mut map = HashMap::new();
    map.insert(10u32, (0.0, 0.0)); // socket
    map.insert(11u32, (600.0, 0.0)); // dist 600
    map.insert(12u32, (0.0, 1000.0)); // dist 1000
    map.insert(13u32, (2000.0, 0.0)); // dist 2000
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
    ];
    PassiveTree::from_nodes(nodes).with_positions(fixture_positions())
}

#[test]
fn radius_constants_are_ordered() {
    let small = JewelRadius::Small.units();
    let medium = JewelRadius::Medium.units();
    let large = JewelRadius::Large.units();
    assert!(
        small < medium,
        "small ({small}) should be < medium ({medium})"
    );
    assert!(
        medium < large,
        "medium ({medium}) should be < large ({large})"
    );
}

#[test]
fn small_radius_finds_only_near_neighbor() {
    let positions = fixture_positions();
    let mods = vec!["10% increased Damage".to_string()];

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::Small, &positions, mods.clone()).unwrap();

    assert_eq!(effect.socket, 10);
    assert_eq!(effect.affected_nodes, vec![11]);
    assert_eq!(effect.mod_texts, mods);
}

#[test]
fn socket_node_is_excluded_from_affected() {
    let positions = fixture_positions();

    let effect = compute_radius_jewel_effect(10, JewelRadius::Large, &positions, vec![]).unwrap();

    assert!(!effect.affected_nodes.contains(&10));
}

#[test]
fn large_radius_finds_all_within_range() {
    let positions = fixture_positions();

    let effect = compute_radius_jewel_effect(10, JewelRadius::Large, &positions, vec![]).unwrap();

    let mut ids = effect.affected_nodes.clone();
    ids.sort_unstable();
    assert_eq!(ids, vec![11, 12]);
}

#[test]
fn custom_radius_captures_farthest_node() {
    let positions = fixture_positions();

    let effect =
        compute_radius_jewel_effect(10, JewelRadius::Custom(2500.0), &positions, vec![]).unwrap();

    let mut ids = effect.affected_nodes.clone();
    ids.sort_unstable();
    assert_eq!(ids, vec![11, 12, 13]);
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
    assert_eq!(via_method.affected_nodes, vec![11, 12]);
}
