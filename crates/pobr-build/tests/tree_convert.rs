use pobr_build::{
    BuildData, SetSelection, parse_build, select_sets, tree_convert::preview_conversion,
};
use pobr_gamedata::{GameData, current_data_dir};

const SYNTHETIC: &str = include_str!("fixtures/synthetic-tree-0_4.xml");
fn setup() -> (BuildData, pobr_data::catalog::PassiveTreeMeta) {
    let game = GameData::new(current_data_dir());
    (
        BuildData::load(&game).unwrap(),
        game.passive_tree_meta().unwrap(),
    )
}

#[test]
fn synthetic_historical_conversion_reports_diff_and_retains_disconnected_weapon_leaf() {
    let (data, meta) = setup();
    let preview = preview_conversion(SYNTHETIC, &data, &meta, "0_5", &[1]).unwrap();
    let spec = &preview.specs[0];
    let original = parse_build(SYNTHETIC).unwrap();
    let old = &data.versioned_passive_nodes["0_4"];
    // The fixture comment records both connected historical paths and their
    // topology cross-check. Every allocation is an actual shipped 0_4 node.
    assert!(
        original
            .tree
            .allocated_nodes
            .iter()
            .all(|id| old.contains_key(&id.0))
    );
    assert_eq!(spec.dropped.len(), 1);
    assert_eq!(spec.dropped[0].id, 8194);
    assert_eq!(spec.dropped[0].name, old[&8194].name.as_deref().unwrap());
    assert_eq!(spec.dropped[0].stats, old[&8194].stats);
    assert_eq!(spec.dropped[0].reason.as_deref(), Some("missing"));
    assert_eq!(
        spec.kept,
        vec![
            1140, 10909, 11825, 13828, 15507, 16489, 21280, 26786, 28050, 35901, 42250, 48401,
            48568, 50459, 56118, 60735, 61312, 63888
        ]
    );
    assert!(spec.allocation.exclusive[0].is_empty());
    assert_eq!(
        spec.allocation.exclusive[1]
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [56118]
    );
    assert_eq!(
        spec.disconnected.iter().map(|n| n.id).collect::<Vec<_>>(),
        [56118]
    );
    assert!(spec.disconnected_by_set[0].is_empty());
    assert!(spec.disconnected_by_set[1].is_empty());
    assert_eq!(
        spec.disconnected_by_set[2]
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [56118]
    );
    assert!(preview.xml.contains("nodeId=\"60735\""));
    assert!(preview.xml.contains("itemId=\"1\""));
    assert!(preview.xml.contains("dexNodes=\"1140,15507,48401\""));
    assert!(preview.xml.contains("+10 to Dexterity"));
    assert_eq!(
        parse_build(&preview.xml).unwrap().tree_version.as_deref(),
        Some("0_5")
    );
    let second = select_sets(
        &preview.xml,
        &SetSelection {
            tree: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(
        parse_build(&second).unwrap().tree_version.as_deref(),
        Some("0_4")
    );
    // Preview never mutates the source; the second spec and global item pool
    // remain byte-identical when converting just the active spec.
    assert_eq!(
        SYNTHETIC
            .split("  <Spec title=\"Synthetic 0_4 alternate")
            .nth(1),
        preview
            .xml
            .split("  <Spec title=\"Synthetic 0_4 alternate")
            .nth(1)
    );
    assert_eq!(original.tree_version.as_deref(), Some("0_4"));
    let all = preview_conversion(SYNTHETIC, &data, &meta, "0_5", &[1, 2]).unwrap();
    assert_eq!(all.specs.len(), 2);
    assert!(!all.xml.contains("treeVersion=\"0_4\""));
    let code = pobr_build::encode_pob_code(&all.xml).unwrap();
    assert_eq!(pobr_build::decode_pob_code(&code).unwrap(), all.xml);
}

#[test]
fn identity_changes_drop_nodes_but_markup_and_balance_changes_keep_them() {
    let (data, meta) = setup();
    // In 0_1 id 40 was Connected Chemistry; 0_5 reuses it for Sustainable Practices.
    let xml = "<PathOfBuilding2><Build className=\"Ranger\"/><Tree><Spec treeVersion=\"0_1\" nodes=\"4,40,52,60735\"/></Tree></PathOfBuilding2>";
    let result = preview_conversion(xml, &data, &meta, "0_5", &[1]).unwrap();
    assert_eq!(result.specs[0].kept, vec![4, 52, 60735]);
    assert_eq!(result.specs[0].dropped[0].id, 40);
    assert_eq!(
        result.specs[0].dropped[0].reason.as_deref(),
        Some("changed_identity")
    );
    assert!(
        preview_conversion(
            &xml.replace("0_1", "unavailable"),
            &data,
            &meta,
            "0_5",
            &[1]
        )
        .unwrap_err()
        .contains("unavailable")
    );
    assert!(preview_conversion(xml, &data, &meta, "0_5", &[0]).is_err());
    assert!(preview_conversion(xml, &data, &meta, "0_5", &[2]).is_err());
}

#[test]
fn removed_socket_loses_assignment_but_keeps_item_and_opaque_xml() {
    let (mut data, meta) = setup();
    data.passive_nodes.remove(&60735);
    let input = SYNTHETIC.replace("<Notes>", "<!-- <Spec treeVersion=\"fake\"/> --><Notes>");
    let result = preview_conversion(&input, &data, &meta, "0_5", &[1]).unwrap();
    assert!(result.xml.contains("<!-- <Spec treeVersion=\"fake\"/> -->"));
    let dropped = result.specs[0]
        .dropped
        .iter()
        .find(|n| n.id == 60735)
        .unwrap();
    assert_eq!(dropped.jewel_item_id, Some(1));
    assert!(
        result
            .xml
            .contains("Synthetic Jewel\nEmerald\n+10 to Dexterity")
    );
    let active = result.xml.split("Synthetic 0_4 alternate").next().unwrap();
    assert!(!active.contains("nodeId=\"60735\""));
    assert_eq!(result.xml.matches("nodeId=\"60735\"").count(), 1);
}
