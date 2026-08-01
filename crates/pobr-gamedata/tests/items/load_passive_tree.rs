use pobr_data::catalog::PassiveNodeKind;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

#[test]
fn passive_nodes_load_with_reasonable_count() {
    let nodes = game_data()
        .passive_nodes()
        .expect("passive_tree should load");
    // PoE2 0.5.0's main tree + ascendancies is roughly a few thousand
    // nodes; even after filtering out slug-less layout placeholders, it
    // should still be >3000.
    assert!(
        nodes.len() > 3000,
        "node count should be in the thousands, got {}",
        nodes.len()
    );

    // Every node has a stable numeric skill id and a string slug.
    assert!(nodes.iter().all(|n| !n.id.is_empty()));
}

#[test]
fn passive_nodes_sorted_by_skill_for_stable_diffs() {
    let nodes = game_data().passive_nodes().unwrap();
    let mut sorted = nodes.clone();
    sorted.sort_by_key(|n| n.skill);
    assert_eq!(
        nodes, sorted,
        "passive_tree.json should be sorted by skill id"
    );
    // skill id is unique (no collision after denormalizing the map key).
    let mut skills: Vec<u32> = nodes.iter().map(|n| n.skill).collect();
    skills.dedup();
    assert_eq!(skills.len(), nodes.len(), "skill id should be unique");
}

#[test]
fn keystone_node_resolved_with_stats() {
    let nodes = game_data().passive_nodes().unwrap();
    let avatar = nodes
        .iter()
        .find(|n| n.id == "passive_keystone_avatar_of_fire")
        .expect("Avatar of Fire keystone should exist");
    assert_eq!(avatar.kind, PassiveNodeKind::Keystone);
    assert_eq!(avatar.name.as_deref(), Some("Avatar of Fire"));
    assert!(
        avatar
            .stats
            .iter()
            .any(|s| s.contains("Converted to Fire Damage")),
        "keystone should retain the English mod text, got {:?}",
        avatar.stats
    );
}

#[test]
fn node_kinds_cover_expected_variants() {
    let nodes = game_data().passive_nodes().unwrap();
    let has = |k: PassiveNodeKind| nodes.iter().any(|n| n.kind == k);
    assert!(has(PassiveNodeKind::Normal), "should have Normal nodes");
    assert!(has(PassiveNodeKind::Notable), "should have Notable nodes");
    assert!(has(PassiveNodeKind::Keystone), "should have Keystone nodes");
    assert!(has(PassiveNodeKind::Mastery), "should have Mastery nodes");
}

#[test]
fn connections_and_group_fields_present() {
    let nodes = game_data().passive_nodes().unwrap();
    // At least the group field (layout) is present.
    assert!(
        nodes.iter().any(|n| n.group.is_some()),
        "the group field should be retained"
    );
    // A connection points to a valid skill id (no dangling reference).
    let valid: std::collections::HashSet<u32> = nodes.iter().map(|n| n.skill).collect();
    let connected = nodes.iter().find(|n| !n.connections.is_empty()).unwrap();
    assert!(
        connected.connections.iter().all(|c| valid.contains(c)),
        "connections should point at valid nodes in the store"
    );
}

#[test]
fn tree_meta_lists_classes_and_ascendancies() {
    let meta = game_data()
        .passive_tree_meta()
        .expect("passive_tree_meta should load");
    assert_eq!(meta.tree, "Default");
    assert_eq!(meta.classes.len(), 12, "PoE2 has 12 classes");

    let warrior = meta
        .classes
        .iter()
        .find(|c| c.name == "Warrior")
        .expect("Warrior class should exist");
    assert!(
        warrior
            .ascendancies
            .iter()
            .any(|a| a.name == "Smith of Kitava" && a.id == "Warrior3"),
        "Warrior should include the Smith of Kitava ascendancy"
    );
    // Unnamed placeholder ascendancy slots are already filtered out (id/name non-empty).
    assert!(
        meta.classes
            .iter()
            .flat_map(|c| &c.ascendancies)
            .all(|a| !a.id.is_empty() && !a.name.is_empty()),
        "ascendancy summary should not contain empty placeholders"
    );
}

/// The anointment notable pool (backfilled by `--tree-anoints`, from
/// vendor tree.lua's top-level nodes block): GGG's data.json doesn't
/// include anoint-exclusive notables that aren't on the main graph; once
/// backfilled they should be consumable via granting by name (the
/// `Allocates <name>` enchant → GrantedPassive, CalcSetup.lua:1322-1331).
#[test]
fn anoint_pool_notables_backfilled_off_graph() {
    let nodes = game_data().passive_nodes().unwrap();
    // A representative node: Paragon (20686, the target of the gemling
    // "Allocates Paragon" amulet enchant).
    let paragon = nodes
        .iter()
        .find(|n| n.skill == 20686)
        .expect("anoint-pool node Paragon(20686) should be in the store");
    assert_eq!(paragon.kind, PassiveNodeKind::Notable);
    assert_eq!(paragon.name.as_deref(), Some("Paragon"));
    // The new data's mod line carries an internal `[Quality]` annotation,
    // so the match is relaxed to two separate substrings.
    assert!(
        paragon
            .stats
            .iter()
            .any(|s| s.contains("Quality") && s.contains("of all Skills")),
        "Paragon should carry a quality mod, got {:?}",
        paragon.stats
    );
    // An anointment-pool node isn't on the main graph: no connections, no
    // coordinates (doesn't participate in tree topology/pathfinding).
    assert!(paragon.connections.is_empty());
    assert!(paragon.x.is_none() && paragon.y.is_none());
}
