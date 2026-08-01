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
    let nodes = game_data().passive_nodes().expect("passive_tree 可加载");
    // PoE2 0.5.0's main tree + ascendancies is roughly a few thousand
    // nodes; even after filtering out slug-less layout placeholders, it
    // should still be >3000.
    assert!(nodes.len() > 3000, "节点数应为数千，实得 {}", nodes.len());

    // Every node has a stable numeric skill id and a string slug.
    assert!(nodes.iter().all(|n| !n.id.is_empty()));
}

#[test]
fn passive_nodes_sorted_by_skill_for_stable_diffs() {
    let nodes = game_data().passive_nodes().unwrap();
    let mut sorted = nodes.clone();
    sorted.sort_by_key(|n| n.skill);
    assert_eq!(nodes, sorted, "passive_tree.json 应按 skill id 排序");
    // skill id is unique (no collision after denormalizing the map key).
    let mut skills: Vec<u32> = nodes.iter().map(|n| n.skill).collect();
    skills.dedup();
    assert_eq!(skills.len(), nodes.len(), "skill id 应唯一");
}

#[test]
fn keystone_node_resolved_with_stats() {
    let nodes = game_data().passive_nodes().unwrap();
    let avatar = nodes
        .iter()
        .find(|n| n.id == "passive_keystone_avatar_of_fire")
        .expect("存在 Avatar of Fire 基石");
    assert_eq!(avatar.kind, PassiveNodeKind::Keystone);
    assert_eq!(avatar.name.as_deref(), Some("Avatar of Fire"));
    assert!(
        avatar
            .stats
            .iter()
            .any(|s| s.contains("Converted to Fire Damage")),
        "基石应保留英文词条文本，实得 {:?}",
        avatar.stats
    );
}

#[test]
fn node_kinds_cover_expected_variants() {
    let nodes = game_data().passive_nodes().unwrap();
    let has = |k: PassiveNodeKind| nodes.iter().any(|n| n.kind == k);
    assert!(has(PassiveNodeKind::Normal), "应有小天赋节点");
    assert!(has(PassiveNodeKind::Notable), "应有大天赋节点");
    assert!(has(PassiveNodeKind::Keystone), "应有基石节点");
    assert!(has(PassiveNodeKind::Mastery), "应有精通节点");
}

#[test]
fn connections_and_group_fields_present() {
    let nodes = game_data().passive_nodes().unwrap();
    // At least the group field (layout) is present.
    assert!(
        nodes.iter().any(|n| n.group.is_some()),
        "应保留 group 分组字段"
    );
    // A connection points to a valid skill id (no dangling reference).
    let valid: std::collections::HashSet<u32> = nodes.iter().map(|n| n.skill).collect();
    let connected = nodes.iter().find(|n| !n.connections.is_empty()).unwrap();
    assert!(
        connected.connections.iter().all(|c| valid.contains(c)),
        "连线应指向库内有效节点"
    );
}

#[test]
fn tree_meta_lists_classes_and_ascendancies() {
    let meta = game_data()
        .passive_tree_meta()
        .expect("passive_tree_meta 可加载");
    assert_eq!(meta.tree, "Default");
    assert_eq!(meta.classes.len(), 12, "PoE2 有 12 个职业");

    let warrior = meta
        .classes
        .iter()
        .find(|c| c.name == "Warrior")
        .expect("存在 Warrior 职业");
    assert!(
        warrior
            .ascendancies
            .iter()
            .any(|a| a.name == "Smith of Kitava" && a.id == "Warrior3"),
        "Warrior 应含 Smith of Kitava 飞升"
    );
    // Unnamed placeholder ascendancy slots are already filtered out (id/name non-empty).
    assert!(
        meta.classes
            .iter()
            .flat_map(|c| &c.ascendancies)
            .all(|a| !a.id.is_empty() && !a.name.is_empty()),
        "飞升摘要不应含空占位"
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
        .expect("油涂池节点 Paragon(20686) 应在库");
    assert_eq!(paragon.kind, PassiveNodeKind::Notable);
    assert_eq!(paragon.name.as_deref(), Some("Paragon"));
    // The new data's mod line carries an internal `[Quality]` annotation,
    // so the match is relaxed to two separate substrings.
    assert!(
        paragon
            .stats
            .iter()
            .any(|s| s.contains("Quality") && s.contains("of all Skills")),
        "Paragon 应携带品质词条，实得 {:?}",
        paragon.stats
    );
    // An anointment-pool node isn't on the main graph: no connections, no
    // coordinates (doesn't participate in tree topology/pathfinding).
    assert!(paragon.connections.is_empty());
    assert!(paragon.x.is_none() && paragon.y.is_none());
}
