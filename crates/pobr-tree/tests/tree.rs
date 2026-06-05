//! 集成测试：PassiveTree 的 JSON 解析、节点查找、allocated mod 收集与 JewelSocket/Mastery gating。
//!
//! 节点 fixture 为自构造（catalog `PassiveNodeDef` schema），仅用于验证逻辑，
//! 不代表真实 PoE2 天赋树拓扑（见 blocked_by_missing_data）。

use pobr_data::prelude::*;
use pobr_tree::{AllocatedNodeMods, PassiveTree, collect_allocated_mods};
use std::collections::HashMap;

/// 自构造 6 节点 fixture 的 JSON（`PassiveNodeDef` 数组）。
///
/// skill 1 Normal(有词条) / 2 Notable(2 条) / 3 JewelSocket(应被 gating) /
/// 4 Keystone(2 条) / 5 Normal(无词条) / 6 Mastery(应被 gating)。
fn fixture_json() -> &'static str {
    r#"[
      {
        "skill": 1,
        "id": "node_strength",
        "name": "Strength Node",
        "kind": "normal",
        "stats": ["+10 to Strength"],
        "connections": [2]
      },
      {
        "skill": 2,
        "id": "node_fire_mastery",
        "name": "Fire Mastery",
        "kind": "notable",
        "stats": ["20% increased Fire Damage", "+5% to Fire Resistance"],
        "connections": [1, 3]
      },
      {
        "skill": 3,
        "id": "node_jewel_socket",
        "name": "Jewel Socket",
        "kind": "jewel_socket",
        "stats": ["this should be ignored"],
        "connections": [2]
      },
      {
        "skill": 4,
        "id": "node_resolute_technique",
        "name": "Resolute Technique",
        "kind": "keystone",
        "stats": ["Your hits can't be Evaded", "Never deal Critical Strikes"],
        "connections": []
      },
      {
        "skill": 5,
        "id": "node_empty",
        "name": "Empty Node",
        "kind": "normal",
        "stats": [],
        "connections": []
      },
      {
        "skill": 6,
        "id": "node_mastery",
        "name": "Some Mastery",
        "kind": "mastery",
        "stats": ["mastery effect a", "mastery effect b"],
        "connections": []
      }
    ]"#
}

#[test]
fn from_json_parses_all_nodes() {
    let tree = PassiveTree::from_json(fixture_json()).expect("fixture should parse");

    assert_eq!(tree.len(), 6);
    assert!(!tree.is_empty());
    for skill in [1u32, 2, 3, 4, 5, 6] {
        assert!(tree.node(NodeId(skill)).is_some());
    }
    assert!(tree.node(NodeId(999)).is_none());
}

#[test]
fn from_json_returns_error_on_invalid_json() {
    let bad = "{ this is not valid json ]";
    let result = PassiveTree::from_json(bad);
    assert!(result.is_err());
}

#[test]
fn node_lookup_returns_correct_data() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();

    let node = tree.node(NodeId(2)).expect("node 2 exists");

    assert_eq!(node.name.as_deref(), Some("Fire Mastery"));
    assert_eq!(node.kind, PassiveNodeKind::Notable);
    assert_eq!(node.stats.len(), 2);
    // node_by_skill mirrors NodeId lookup.
    assert_eq!(tree.node_by_skill(2).unwrap().id, "node_fire_mastery");
}

#[test]
fn compute_node_mods_collects_only_allocated() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(2)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert_eq!(mods.len(), 2);
    let ids: Vec<u32> = mods.iter().map(|m| m.node_id.0).collect();
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
}

#[test]
fn compute_node_mods_skips_jewel_socket_stats() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(3)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    let jewel = mods.iter().find(|m| m.node_id == NodeId(3));
    assert!(
        jewel.is_none(),
        "JewelSocket stats must be gated out entirely"
    );
    let strength = mods.iter().find(|m| m.node_id == NodeId(1)).unwrap();
    assert_eq!(strength.modifier_texts, vec!["+10 to Strength".to_string()]);
}

#[test]
fn compute_node_mods_skips_mastery_without_selection() {
    // Backwards-compatible: Mastery node with no entry in mastery_effects is gated.
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(6)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert!(mods.iter().all(|m| m.node_id != NodeId(6)));
    assert_eq!(mods.len(), 1);
}

#[test]
fn compute_node_mods_ignores_unknown_allocated_ids() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(404)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].node_id, NodeId(1));
}

#[test]
fn compute_node_mods_skips_nodes_with_no_stats() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(4), NodeId(5)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].node_id, NodeId(4));
    assert_eq!(mods[0].modifier_texts.len(), 2);
}

#[test]
fn compute_node_mods_preserves_allocation_order() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(4), NodeId(2), NodeId(1)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    let ids: Vec<u32> = mods.iter().map(|m| m.node_id.0).collect();
    assert_eq!(ids, vec![4, 2, 1]);
}

#[test]
fn allocated_mods_carry_passive_node_source_id() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(2)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    let m = &mods[0];
    assert_eq!(m.source_id.kind, SourceKind::PassiveNode);
    assert_eq!(m.source_id.id, "2");
}

#[test]
fn collect_allocated_mods_free_function_matches_tree_method() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let nodes: HashMap<u32, PassiveNodeDef> = tree.nodes.clone();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(2), NodeId(3)],
        ..Default::default()
    };

    let via_fn: Vec<AllocatedNodeMods> = collect_allocated_mods(&spec, &nodes);
    let via_method = tree.compute_node_mods(&spec);

    let mut a: Vec<u32> = via_fn.iter().map(|m| m.node_id.0).collect();
    let mut b: Vec<u32> = via_method.iter().map(|m| m.node_id.0).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
    assert!(!a.contains(&3));
}

#[test]
fn from_nodes_round_trips_with_positions() {
    let nodes = vec![
        PassiveNodeDef {
            skill: 1,
            id: "a".into(),
            name: None,
            kind: PassiveNodeKind::Normal,
            stats: vec!["+10 to Strength".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            connections: vec![],
            ascendancy_id: None,
        },
        PassiveNodeDef {
            skill: 2,
            id: "b".into(),
            name: None,
            kind: PassiveNodeKind::Normal,
            stats: vec![],
            group: None,
            orbit: None,
            orbit_index: None,
            connections: vec![],
            ascendancy_id: None,
        },
    ];
    let mut positions = HashMap::new();
    positions.insert(1u32, (0.0, 0.0));
    positions.insert(2u32, (50.0, 0.0));

    let tree = PassiveTree::from_nodes(nodes).with_positions(positions);

    assert_eq!(tree.len(), 2);
    assert_eq!(tree.nodes_in_radius(NodeId(1), 100.0), vec![NodeId(2)]);
}

// --- Mastery selection tests ---

/// 当 mastery_effects 中包含对应节点的选择时，应只注入选定的单条词条。
#[test]
fn mastery_with_selection_injects_chosen_effect() {
    // Arrange
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let mut mastery_effects = HashMap::new();
    mastery_effects.insert(
        NodeId(6),
        MasterySelection {
            effect_text: "mastery effect a".to_string(),
        },
    );
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(6)],
        mastery_effects,
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert: mastery node 6 must now appear with the selected effect only
    let mastery_mod = mods.iter().find(|m| m.node_id == NodeId(6)).unwrap();
    assert_eq!(mastery_mod.modifier_texts, vec!["mastery effect a"]);
    assert_eq!(mods.len(), 2);
}

/// 当 mastery_effects 中选择了 effect b，应注入 effect b 而不是 a。
#[test]
fn mastery_selection_injects_second_effect() {
    // Arrange
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let mut mastery_effects = HashMap::new();
    mastery_effects.insert(
        NodeId(6),
        MasterySelection {
            effect_text: "mastery effect b".to_string(),
        },
    );
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(6)],
        mastery_effects,
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert
    let mastery_mod = mods.iter().find(|m| m.node_id == NodeId(6)).unwrap();
    assert_eq!(mastery_mod.modifier_texts, vec!["mastery effect b"]);
    assert_eq!(mods.len(), 1);
}

/// 当 mastery_effects 中没有对应节点的选择时，Mastery 节点整体 gating（向后兼容）。
#[test]
fn mastery_without_selection_is_still_gated() {
    // Arrange
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(6)],
        mastery_effects: HashMap::new(), // 无任何选择
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert: mastery node 6 is gated out
    assert!(mods.iter().all(|m| m.node_id != NodeId(6)));
    assert_eq!(mods.len(), 1);
}

/// 多个 Mastery 节点各自独立选择，互不干扰。
#[test]
fn multiple_mastery_selections_are_independent() {
    // Arrange: fixture only has node 6 as mastery; build a tree with two mastery nodes
    let nodes_json = r#"[
      { "skill": 10, "id": "mastery_a", "kind": "mastery", "stats": ["effect_a1", "effect_a2"] },
      { "skill": 11, "id": "mastery_b", "kind": "mastery", "stats": ["effect_b1", "effect_b2"] }
    ]"#;
    let tree = PassiveTree::from_json(nodes_json).unwrap();

    let mut mastery_effects = HashMap::new();
    mastery_effects.insert(
        NodeId(10),
        MasterySelection {
            effect_text: "effect_a2".to_string(),
        },
    );
    mastery_effects.insert(
        NodeId(11),
        MasterySelection {
            effect_text: "effect_b1".to_string(),
        },
    );
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(10), NodeId(11)],
        mastery_effects,
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert
    assert_eq!(mods.len(), 2);
    let mod_a = mods.iter().find(|m| m.node_id == NodeId(10)).unwrap();
    let mod_b = mods.iter().find(|m| m.node_id == NodeId(11)).unwrap();
    assert_eq!(mod_a.modifier_texts, vec!["effect_a2"]);
    assert_eq!(mod_b.modifier_texts, vec!["effect_b1"]);
}

/// Mastery 节点的 SourceId 与普通节点保持一致（PassiveNode, skill id 字符串）。
#[test]
fn mastery_selection_source_id_is_passive_node() {
    // Arrange
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let mut mastery_effects = HashMap::new();
    mastery_effects.insert(
        NodeId(6),
        MasterySelection {
            effect_text: "mastery effect a".to_string(),
        },
    );
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(6)],
        mastery_effects,
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert
    let m = &mods[0];
    assert_eq!(m.source_id.kind, SourceKind::PassiveNode);
    assert_eq!(m.source_id.id, "6");
}
