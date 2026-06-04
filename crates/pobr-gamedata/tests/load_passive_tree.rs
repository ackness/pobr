use pobr_data::catalog::PassiveNodeKind;
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
}

#[test]
fn passive_nodes_load_with_reasonable_count() {
    let nodes = game_data().passive_nodes().expect("passive_tree 可加载");
    // PoE2 0.5.0 主树 + 飞升约数千节点；过滤掉无 slug 的布局占位后仍应 >3000。
    assert!(nodes.len() > 3000, "节点数应为数千，实得 {}", nodes.len());

    // 每个节点都有稳定数值 skill id 与字符串 slug。
    assert!(nodes.iter().all(|n| !n.id.is_empty()));
}

#[test]
fn passive_nodes_sorted_by_skill_for_stable_diffs() {
    let nodes = game_data().passive_nodes().unwrap();
    let mut sorted = nodes.clone();
    sorted.sort_by_key(|n| n.skill);
    assert_eq!(nodes, sorted, "passive_tree.json 应按 skill id 排序");
    // skill id 唯一（map key 反范式化后无碰撞）。
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
    // 至少存在分组字段（布局）。
    assert!(
        nodes.iter().any(|n| n.group.is_some()),
        "应保留 group 分组字段"
    );
    // 连线指向有效的 skill id（无悬空引用）。
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
    // 无名占位飞升槽已过滤（id/name 非空）。
    assert!(
        meta.classes
            .iter()
            .flat_map(|c| &c.ascendancies)
            .all(|a| !a.id.is_empty() && !a.name.is_empty()),
        "飞升摘要不应含空占位"
    );
}
