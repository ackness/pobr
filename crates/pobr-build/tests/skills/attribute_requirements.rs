use pobr_build::{Build, BuildData, DataOrchestratorOptions, SocketGroup, calculate_with_data};
use pobr_data::item::{EquipmentSlot, Item, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn item_local_and_global_requirement_modifiers_stay_separate() {
    let mut data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    data.base_items.insert(
        "RequirementBoots".into(),
        serde_json::from_value(serde_json::json!({
            "id":"RequirementBoots", "name":"Requirement Boots", "item_class":"Boots",
            "drop_level":1, "width":2, "height":2, "tags":[], "implicits":[], "mod_domain":0,
            "req_str":200, "req_dex":100
        }))
        .unwrap(),
    );
    let mut build = Build::new();
    build.items.insert(
        EquipmentSlot::Boots,
        Item {
            base: "RequirementBoots".into(),
            rarity: ItemRarity::Normal,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["40% reduced Attribute Requirements".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            socketed: vec![],
            parsed_stats: vec![],
        },
    );
    let result = calculate_with_data(
        &build,
        &data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![
                "Equipment and Skill Gems have 20% reduced Attribute Requirements".into(),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.req_str, 96.0);
    assert_eq!(result.req_dex, 48.0);
    assert_eq!(result.req_int, 0.0);
    build
        .items
        .get_mut(&EquipmentSlot::Boots)
        .unwrap()
        .enchant_texts
        .push("Convert 50% of Requirements to Dexterity".into());
    let converted = calculate_with_data(
        &build,
        &data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![
                "Equipment and Skill Gems have 20% reduced Attribute Requirements".into(),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(converted.req_str, 48.0);
    assert_eq!(converted.req_dex, 96.0);
}

#[test]
fn gem_requirements_follow_more_and_highest_attribute_text() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let build =
        Build::new().add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    let calc = |lines: &[&str]| {
        calculate_with_data(
            &build,
            &data,
            &DataOrchestratorOptions {
                extra_modifier_texts: lines.iter().map(|line| (*line).into()).collect(),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let plain = calc(&[]);
    assert!(plain.req_int > 100.0);
    assert_eq!(
        calc(&["Skill Gems have 50% more Attribute Requirements"]).req_int,
        (plain.req_int * 1.5).floor()
    );
    assert_eq!(
        calc(&["Skill Gems have no Attribute Requirements"]).req_int,
        0.0
    );
    assert_eq!(
        calc(&[
            "+1000 to Strength",
            "Attribute Requirements of Gems can be satisified by your highest Attribute"
        ])
        .req_int,
        0.0
    );
}

#[test]
fn corruption_level_bonus_does_not_increase_gem_requirements() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let build = pobr_build::parse_build(r#"<PathOfBuilding2><Build level="90" className="Witch"/>
        <Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="18" corruptLevel="1" enabled="true"/></Skill></Skills>
        </PathOfBuilding2>"#).unwrap();
    assert_eq!(build.socket_groups[0].gem_skills[0].gem_level, 19);
    assert_eq!(build.socket_groups[0].gem_requirement_levels, [18]);
    let result = calculate_with_data(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(result.req_int, 137.0);
}
