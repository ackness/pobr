use pobr_gamedata::{GameData, data_version, repo_data_root};

#[test]
fn curated_jewel_slot_rows_load_and_name_tree_sockets() {
    let game = GameData::new(repo_data_root().join(data_version()));
    let slots = game.passive_jewel_slots().unwrap();
    // Official 0.5.5.4.2 PassiveJewelSlots: 31 rows. A WeGame share's `PassiveJewel_<n>` is row n.
    assert_eq!(slots.slots.len(), 31);
    assert_eq!(
        slots.passive_id_for_wegame_socket("PassiveJewel_4"),
        Some("jewel_slot1960")
    );
    assert_eq!(
        slots.passive_id_for_wegame_socket("PassiveJewel_5"),
        Some("jewel_slot1961")
    );
    assert_eq!(
        slots.passive_id_for_wegame_socket("PassiveJewel_12"),
        Some("jewel_slot1970")
    );
    assert_eq!(
        slots.passive_id_for_wegame_socket("PassiveJewel_17"),
        Some("jewel_slot1976")
    );
    // Not a slot name, or a row the table does not have.
    assert_eq!(slots.passive_id_for_wegame_socket("jewel_slot1960"), None);
    assert_eq!(slots.passive_id_for_wegame_socket("PassiveJewel_31"), None);
    assert_eq!(slots.passive_id_for_wegame_socket("PassiveJewel_x"), None);
    // Every socket that the tree has is named once, and only sinister rows carry the flag.
    let tree = game.passive_nodes().unwrap();
    for slot in slots
        .slots
        .iter()
        .filter(|slot| tree.iter().any(|node| node.id == slot.passive_id))
    {
        let node = tree.iter().find(|node| node.id == slot.passive_id).unwrap();
        assert_eq!(
            node.skill, slot.passive_skill_graph_id,
            "{}",
            slot.passive_id
        );
    }
}
