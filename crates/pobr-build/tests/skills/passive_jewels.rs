use pobr_build::jewel_tree::passive_jewel_state;
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, calculate_with_data_session,
    radius_jewel_from_text,
};
use pobr_core::parse_pob_xml_item;
use pobr_data::catalog::PassiveNodeDef;
use pobr_data::item::EquipmentSlot;
use pobr_data::passive_tree::{AttributeChoice, NodeId, PassiveTreeSpec};
use pobr_gamedata::GameData;
use serde_json::json;

fn data() -> BuildData {
    BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).unwrap()
}
fn node(skill: u32, id: &str, name: &str, kind: &str, x: f64, stats: &[&str]) -> PassiveNodeDef {
    serde_json::from_value(
        json!({"skill":skill,"id":id,"name":name,"kind":kind,"x":x,"y":0.0,"stats":stats}),
    )
    .unwrap()
}
fn build(text: &str, allocated: &[u32]) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 85,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .with_tree(PassiveTreeSpec {
            allocated_nodes: allocated.iter().copied().map(NodeId).collect(),
            ..Default::default()
        })
        .with_jewels(vec![parse_pob_xml_item(text).unwrap()])
        .with_radius_jewels(vec![radius_jewel_from_text(900001, text).unwrap()])
}
fn jewel(mods: &str) -> String {
    format!("Rarity: UNIQUE\nSynthetic Jewel\nDiamond\nRadius: Very Large\nImplicits: 0\n{mods}")
}

#[test]
fn allocation_uses_selected_tree_ring_boundaries_and_stable_class_ids() {
    let mut data = data();
    let entries = [
        node(900001, "socket", "Jewel Socket", "jewel_socket", 0.0, &[]),
        node(
            900002,
            "inside",
            "Inside",
            "normal",
            199.0,
            &["+1 to maximum Life"],
        ),
        node(
            900003,
            "inner",
            "Inner",
            "normal",
            200.0,
            &["+1 to maximum Life"],
        ),
        node(
            900004,
            "outer",
            "Outer",
            "notable",
            400.0,
            &["+1 to maximum Life"],
        ),
        node(
            900005,
            "outside",
            "Outside",
            "normal",
            401.0,
            &["+1 to maximum Life"],
        ),
        node(
            900006,
            "keystone",
            "Future Keystone",
            "keystone",
            300.0,
            &["+1 to maximum Life"],
        ),
        node(
            900007,
            "another_socket",
            "Jewel Socket",
            "jewel_socket",
            300.0,
            &[],
        ),
        node(900008, "ranger596", "RANGER", "normal", 300.0, &[]),
    ];
    data.versioned_passive_nodes.insert(
        "9_10".into(),
        entries.into_iter().map(|n| (n.skill, n)).collect(),
    );
    let mut bands = data
        .jewel_radii
        .tree_versions
        .values()
        .next()
        .unwrap()
        .clone();
    let index = data.passive_jewels.ring_sizes["only affects passives in medium ring"] - 1;
    bands[index].inner = 100;
    bands[index].outer = 200;
    data.jewel_radii
        .tree_versions
        .insert("9_10".into(), bands.clone());
    bands[index].outer = 900;
    data.jewel_radii.tree_versions.insert("9_11".into(), bands);
    data.jewel_radii.distance_multiplier = 2.0;
    let raw = jewel(
        "Only affects Passives in Medium Ring\nPassives in Radius can be Allocated without being connected to your tree\nCan Allocate Passive Skills from the Ranger's starting point",
    );
    let build = build(&raw, &[900001]).with_tree_version(Some("9_10".into()));
    let state = passive_jewel_state(&build, &data);
    assert_eq!(
        state.allocation_grants[0].nodes,
        vec![900003, 900004, 900006]
    );
    assert_eq!(state.allocation_grants[0].roots, vec![900008]);
    assert_eq!(state.rings[0].inner, 200.0);
    assert_eq!(state.rings[0].outer, 400.0);
    assert_eq!(state.handled_modifiers.len(), 3);
    data.versioned_passive_nodes
        .get_mut("9_10")
        .unwrap()
        .get_mut(&900001)
        .unwrap()
        .x = None;
    let missing = passive_jewel_state(&build, &data);
    assert!(missing.allocation_grants[0].nodes.is_empty());
    assert_eq!(missing.allocation_grants[0].roots, vec![900008]);
    data.jewel_radii.tree_versions.clear();
    let no_radii = passive_jewel_state(&build, &data);
    assert_eq!(no_radii.allocation_grants[0].roots, vec![900008]);
    assert!(no_radii.allocation_grants[0].nodes.is_empty());
}

#[test]
fn from_nothing_uses_named_keystone_center_and_rejects_unknown_centers() {
    let mut data = data();
    data.passive_nodes = [
        node(900001, "socket", "Socket", "jewel_socket", 10000.0, &[]),
        node(2, "key", "Future Keystone", "keystone", 0.0, &[]),
        node(3, "near_key", "Near Key", "normal", 100.0, &[]),
        node(4, "near_socket", "Near Socket", "normal", 10100.0, &[]),
    ]
    .into_iter()
    .map(|n| (n.skill, n))
    .collect();
    let text = jewel(
        "Passives in Radius of Future Keystone can be Allocated without being connected to your tree",
    );
    let state = passive_jewel_state(&build(&text, &[900001]), &data);
    assert_eq!(state.allocation_grants[0].nodes, vec![3]);
    assert_eq!(state.rings[0].center, 2);
    let mut missing_catalog = data.clone();
    missing_catalog.passive_jewels = Default::default();
    assert!(
        passive_jewel_state(&build(&text, &[900001]), &missing_catalog)
            .allocation_grants
            .is_empty()
    );
    assert!(
        passive_jewel_state(
            &build(
                &text.replace("Future Keystone", "Missing Keystone"),
                &[900001]
            ),
            &data
        )
        .allocation_grants
        .is_empty()
    );
}

#[test]
fn variants_and_rolls_are_normalized_before_jewel_tree_directives() {
    let text = "Rarity: UNIQUE\nControlled Metamorphosis\nDiamond\nVariant: Small\nVariant: Large\nSelected Variant: 2\nRadius: Variable\nImplicits: 0\n{variant:1}Only affects Passives in Small Ring\n{variant:2}Only affects Passives in Large Ring\nPassives in Radius can be Allocated without being connected to your tree\n{range:0.5}-(20-10)% to all Elemental Resistances";
    let radius = radius_jewel_from_text(1, text).unwrap();
    assert!(!radius.tree_texts.iter().any(|s| s.contains("Small Ring")));
    assert!(radius.tree_texts.iter().any(|s| s.contains("Large Ring")));
    assert!(radius.tree_texts.iter().any(|s| s.starts_with("-15%")));
}

#[test]
fn kalguur_keystones_replace_old_stats_and_remove_inherent_attribute_bonuses() {
    let mut data = data();
    data.passive_nodes = [
        node(900001, "socket", "Socket", "jewel_socket", 0.0, &[]),
        node(
            2,
            "key",
            "Original",
            "keystone",
            100.0,
            &["+100 to maximum Life"],
        ),
    ]
    .into_iter()
    .map(|n| (n.skill, n))
    .collect();
    let plain = jewel("+0 to maximum Life");
    let base=build(&plain,&[900001,2]).set_item(EquipmentSlot::Helmet,
        parse_pob_xml_item("Rarity: RARE\nReference\nCirclet\nArmour: 100\nEvasion: 100\nEnergy Shield: 100\nImplicits: 0\n+100 to Strength\n+100 to Dexterity\n+100 to Intelligence").unwrap());
    let options = DataOrchestratorOptions {
        inject_character_base: true,
        mode_effective: false,
        ..Default::default()
    };
    let baseline = calculate_with_data_session(&base, &data, &options).unwrap();
    let attrs = data.class_attributes("Witch").unwrap();
    for (conqueror, flag, name) in [
        ("Vorana", "NoStrBonusToLife", "Black Scythe Training"),
        ("Medved", "NoDexBonusToAccuracy", "Circular Teachings"),
        ("Olroth", "NoIntBonusToMana", "Knightly Tenets"),
    ] {
        let text = jewel(&format!(
            "Remembrancing 1234 songworthy deeds by the line of {conqueror}\nPassives in radius are Conquered by the Kalguur\nHistoric"
        ));
        let mut changed = base.clone();
        changed.jewels = vec![parse_pob_xml_item(&text).unwrap()];
        changed.radius_jewels = vec![radius_jewel_from_text(900001, &text).unwrap()];
        assert_eq!(passive_jewel_state(&changed, &data).nodes[&2].name, name);
        let calc = calculate_with_data_session(&changed, &data, &options).unwrap();
        assert!(
            calc.has_flag(flag),
            "{conqueror}: {:?}",
            calc.unsupported_modifier_texts()
        );
        let removed_life = 100.0
            + if conqueror == "Vorana" {
                (f64::from(attrs.strength) + 100.0)
                    * data.constants.character_constants.life_per_strength
            } else {
                0.0
            };
        assert!((baseline.output().life - calc.output().life - removed_life).abs() < 1e-8);
        if conqueror == "Vorana" {
            assert!(calc.output().energy_shield > baseline.output().energy_shield);
        }
        if conqueror == "Medved" {
            assert!(calc.output().armour > baseline.output().armour);
            assert!(calc.base_sum("Accuracy") < baseline.base_sum("Accuracy"));
        }
        if conqueror == "Olroth" {
            assert!(calc.output().evasion > baseline.output().evasion);
            assert!(calc.output().mana < baseline.output().mana);
        }
    }
}

#[test]
fn abyss_small_passives_replace_stats_preserve_attribute_choice_and_expose_missing_seeds() {
    let mut data = data();
    data.passive_nodes = [
        node(900001, "socket", "Socket", "jewel_socket", 0.0, &[]),
        node(
            2,
            "small",
            "Life",
            "normal",
            100.0,
            &["+100 to maximum Life"],
        ),
        node(
            3,
            "attr",
            "Attribute",
            "normal",
            100.0,
            &["+5 to any [Attributes|Attribute]"],
        ),
        node(
            4,
            "notable",
            "Notable",
            "notable",
            100.0,
            &["+30 to maximum Life"],
        ),
    ]
    .into_iter()
    .map(|n| (n.skill, n))
    .collect();
    let text = jewel(
        "Glorifying the defilement of 8000 souls in tribute to Amanamu\nPassives in radius are Conquered by the Abyssals\nHistoric",
    );
    let mut build = build(&text, &[900001, 2, 3, 4]);
    build
        .tree
        .attribute_overrides
        .insert(NodeId(3), AttributeChoice::Strength);
    let state = passive_jewel_state(&build, &data);
    assert_eq!(state.nodes[&2].stats, vec!["+5 to Tribute"]);
    assert_eq!(state.nodes[&3].stats, vec!["+3 to Tribute"]);
    assert_eq!(state.unresolved.into_iter().collect::<Vec<_>>(), vec![4]);
    let options = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let calc = calculate_with_data_session(&build, &data, &options).unwrap();
    assert_eq!(calc.base_sum("Tribute"), 8.0);
    assert!(
        calc.unsupported_modifier_texts()
            .iter()
            .any(|s| s.contains("Tree:4: missing timeless"))
    );
    assert!(
        calc.mods_named("MaximumLife")
            .iter()
            .all(|m| m.source.as_deref() != Some("+100 to maximum Life"))
    );
    assert!(calc.base_sum("Strength") >= 5.0);
    data.passive_nodes.get_mut(&2).unwrap().stats.clear();
    let empty_original = calculate_with_data_session(&build, &data, &options).unwrap();
    assert_eq!(empty_original.base_sum("Tribute"), 8.0);
}
