//! Integration tests: `PassiveTree` JSON parsing, node lookup, allocated-mod
//! collection, and JewelSocket/Mastery gating.
//!
//! Node fixtures are hand-constructed (following the catalog `PassiveNodeDef`
//! schema) purely to exercise the logic, and don't represent real PoE2
//! passive tree topology (see blocked_by_missing_data).

use pobr_data::prelude::*;
use pobr_tree::{
    AllocatedNodeMods, ClassContext, PassiveTree, collect_allocated_mods,
    collect_allocated_mods_for_class,
};
use std::collections::HashMap;

/// Hand-constructed JSON for a 7-node fixture (an array of `PassiveNodeDef`).
///
/// skill 1 Normal (has stats) / 2 Notable (2 stats) / 3 JewelSocket (should
/// be gated) / 4 Keystone (2 stats) / 5 Normal (no stats) / 6 Mastery
/// (should be gated) / 7 attribute-choice small node
/// (`+5 to any Attribute`, one of three).
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
      },
      {
        "skill": 7,
        "id": "node_attribute",
        "name": "Attribute",
        "kind": "normal",
        "stats": ["+5 to any [Attributes|Attribute]"],
        "connections": []
      }
    ]"#
}

#[test]
fn from_json_parses_all_nodes() {
    let tree = PassiveTree::from_json(fixture_json()).expect("fixture should parse");

    assert_eq!(tree.len(), 7);
    assert!(!tree.is_empty());
    for skill in [1u32, 2, 3, 4, 5, 6, 7] {
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
fn compute_node_mods_splits_multiline_stats_into_separate_texts() {
    // Some PoE2 keystones (e.g. CI) pack a single stats element with `\n`.
    // Feeding the whole thing to parse_mod always fails and gets silently
    // dropped; splitting into lines lets each parse independently. This is a
    // generic fix shared by every multi-line node.
    let json = r#"[
      {
        "skill": 7,
        "id": "node_ci",
        "name": "Chaos Inoculation",
        "kind": "keystone",
        "stats": ["Maximum Life is 1\nImmune to Chaos Damage and Bleeding"],
        "connections": []
      }
    ]"#;
    let tree = PassiveTree::from_json(json).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(7)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert_eq!(mods.len(), 1);
    assert_eq!(
        mods[0].modifier_texts,
        vec![
            "Maximum Life is 1".to_string(),
            "Immune to Chaos Damage and Bleeding".to_string(),
        ],
        "multi-line stat must split per physical line (trimmed, non-empty)"
    );
}

#[test]
fn from_nodes_round_trips_with_positions() {
    let nodes = vec![
        PassiveNodeDef {
            apply_to_armour: false,
            skill: 1,
            id: "a".into(),
            name: None,
            kind: PassiveNodeKind::Normal,
            stats: vec!["+10 to Strength".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        },
        PassiveNodeDef {
            apply_to_armour: false,
            skill: 2,
            id: "b".into(),
            name: None,
            kind: PassiveNodeKind::Normal,
            stats: vec![],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        },
    ];
    let mut positions = HashMap::new();
    positions.insert(1u32, (0.0, 0.0));
    positions.insert(2u32, (50.0, 0.0));

    let tree = PassiveTree::from_nodes(nodes).with_positions(positions);

    assert_eq!(tree.len(), 2);
    assert_eq!(tree.nodes_in_radius(NodeId(1), 100.0), vec![NodeId(2)]);
}

// Mastery selection tests

/// When `mastery_effects` has a selection for the node, only that single chosen effect is injected.
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
        attribute_overrides: HashMap::new(),
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert: mastery node 6 must now appear with the selected effect only
    let mastery_mod = mods.iter().find(|m| m.node_id == NodeId(6)).unwrap();
    assert_eq!(mastery_mod.modifier_texts, vec!["mastery effect a"]);
    assert_eq!(mods.len(), 2);
}

/// When `mastery_effects` selects effect b, effect b is injected instead of a.
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
        attribute_overrides: HashMap::new(),
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert
    let mastery_mod = mods.iter().find(|m| m.node_id == NodeId(6)).unwrap();
    assert_eq!(mastery_mod.modifier_texts, vec!["mastery effect b"]);
    assert_eq!(mods.len(), 1);
}

/// When `mastery_effects` has no selection for the node, the Mastery node is gated entirely (backwards-compatible).
#[test]
fn mastery_without_selection_is_still_gated() {
    // Arrange
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(1), NodeId(6)],
        mastery_effects: HashMap::new(), // no selection at all
        attribute_overrides: HashMap::new(),
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert: mastery node 6 is gated out
    assert!(mods.iter().all(|m| m.node_id != NodeId(6)));
    assert_eq!(mods.len(), 1);
}

/// Multiple Mastery nodes each pick independently, without interfering with each other.
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
        attribute_overrides: HashMap::new(),
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

/// A Mastery node's SourceId matches a regular node's (PassiveNode, skill id as a string).
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
        attribute_overrides: HashMap::new(),
    };

    // Act
    let mods = tree.compute_node_mods(&spec);

    // Assert
    let m = &mods[0];
    assert_eq!(m.source_id.kind, SourceKind::PassiveNode);
    assert_eq!(m.source_id.id, "6");
}

/// When `attribute_overrides` has a choice for an attribute-choice node, its
/// text is rewritten to the chosen attribute.
#[test]
fn attribute_choice_node_rewrites_to_selected_attribute() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let mut attribute_overrides = HashMap::new();
    attribute_overrides.insert(NodeId(7), AttributeChoice::Dexterity);
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(7)],
        attribute_overrides,
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].modifier_texts, vec!["+5 to Dexterity".to_string()]);
}

/// With no choice made for an attribute-choice node, the original text is
/// kept as-is (mod_parser downstream doesn't recognize `any attribute`, so it
/// naturally ends up Unsupported — matching PoB2's empty mapping, and
/// contributes no attribute).
#[test]
fn attribute_choice_node_without_override_keeps_original_text() {
    let tree = PassiveTree::from_json(fixture_json()).unwrap();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(7)],
        ..Default::default()
    };

    let mods = tree.compute_node_mods(&spec);

    assert_eq!(mods.len(), 1);
    assert_eq!(
        mods[0].modifier_texts,
        vec!["+5 to any [Attributes|Attribute]".to_string()]
    );
}

// isSwitchable variants selected by class/ascendancy (mirrors PoB2 PassiveSpec.lua:1251-1256)

/// Hand-constructed isSwitchable node fixture: base stats + a Witch class
/// variant + an Abyssal Lich ascendancy variant.
fn switchable_fixture() -> HashMap<u32, PassiveNodeDef> {
    let json = r#"[
      {
        "skill": 51335,
        "id": "fire32",
        "name": "Affliction Enforcer",
        "kind": "notable",
        "stats": ["40% increased Flammability Magnitude", "20% increased chance to Shock"],
        "connections": [],
        "variants": [
          {
            "class": "Witch",
            "variant_skill": 64801,
            "name": "Jagged Shards",
            "stats": [
              "20% increased Critical Hit Chance for Spells",
              "20% increased Physical Damage"
            ]
          },
          {
            "class": "Abyssal Lich",
            "variant_skill": 99999,
            "name": "Ascend Variant",
            "stats": ["+10 to Intelligence"]
          }
        ]
      }
    ]"#;
    PassiveTree::from_json(json).unwrap().nodes
}

/// A class-name match on a variant wholesale replaces the base stats (PoB
/// `ReplaceNode` semantics, not a merge).
#[test]
fn switchable_node_uses_class_variant_stats() {
    let nodes = switchable_fixture();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(51335)],
        ..Default::default()
    };
    let class = ClassContext {
        class_name: "Witch",
        ascendancy_name: "Blood Mage",
    };

    let mods = collect_allocated_mods_for_class(&spec, &nodes, class);

    assert_eq!(mods.len(), 1);
    assert_eq!(
        mods[0].modifier_texts,
        vec![
            "20% increased Critical Hit Chance for Spells".to_string(),
            "20% increased Physical Damage".to_string(),
        ],
        "the Witch variant mods must wholesale replace the base mods"
    );
    // Attribution still points at the base node's skill id (the stable key
    // for tree connections / Build Code).
    assert_eq!(mods[0].source_id.id, "51335");
}

/// When class name doesn't match, falls back to ascendancy name matching
/// (mirrors PoB's `options[curAscendClassName]` fallback).
#[test]
fn switchable_node_falls_back_to_ascendancy_variant() {
    let nodes = switchable_fixture();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(51335)],
        ..Default::default()
    };
    let class = ClassContext {
        class_name: "Sorceress",
        ascendancy_name: "Abyssal Lich",
    };

    let mods = collect_allocated_mods_for_class(&spec, &nodes, class);

    assert_eq!(mods.len(), 1);
    assert_eq!(
        mods[0].modifier_texts,
        vec!["+10 to Intelligence".to_string()]
    );
}

/// With no class or ascendancy match (or no context at all), base stats are
/// kept — zero behaviour change for the legacy entry point.
#[test]
fn switchable_node_without_matching_class_keeps_base_stats() {
    let nodes = switchable_fixture();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(51335)],
        ..Default::default()
    };
    let base = vec![
        "40% increased Flammability Magnitude".to_string(),
        "20% increased chance to Shock".to_string(),
    ];

    let unmatched = collect_allocated_mods_for_class(
        &spec,
        &nodes,
        ClassContext {
            class_name: "Druid",
            ascendancy_name: "Oracle",
        },
    );
    assert_eq!(unmatched[0].modifier_texts, base);

    // The no-context entry point (collect_allocated_mods) is equivalent to an empty ClassContext.
    let legacy = collect_allocated_mods(&spec, &nodes);
    assert_eq!(legacy[0].modifier_texts, base);
}
