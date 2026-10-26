use pobr_build::{Build, DataOrchestratorOptions, SocketGroup, calculate_with_data};
use pobr_data::item::{EquipmentSlot, Item, ItemRarity, RolledDefence};

#[test]
fn item_local_and_global_requirement_modifiers_stay_separate() {
    let mut data = crate::test_data().clone();
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

    let diagnosis =
        pobr_build::diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
    for text in [
        "40% reduced Attribute Requirements",
        "Convert 50% of Requirements to Dexterity",
    ] {
        let line = usage
            .lines
            .iter()
            .find(|line| line.text.as_deref() == Some(text))
            .unwrap();
        assert!(
            line.modifiers
                .iter()
                .all(|modifier| { modifier.status == pobr_build::usage::ModifierStatus::Outside })
        );
    }
}

#[test]
fn local_flat_requirement_is_added_after_base_requirement_conversion() {
    let data = crate::test_data();
    let build = pobr_build::parse_build(
        r#"<PathOfBuilding2><Build level="90" className="Huntress"/>
        <Items><Item id="1">Rarity: UNIQUE
Chober Chaber
Runeforged Leaden Greathammer
Implicits: 2
{enchant}Convert 20% of Requirements to Dexterity
+81 Intelligence Requirement
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
        </PathOfBuilding2>"#,
    )
    .unwrap();
    let result = calculate_with_data(
        &build,
        &data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["Triple Attribute requirements of Martial Weapons".into()],
            ..Default::default()
        },
    )
    .unwrap();
    // The base needs 60 Str and 50 Int. Convert 20% of those, then add
    // 81 Int: 48 Str, 22 Dex, 121 Int. Triple weapon requirements last.
    assert_eq!(result.req_str, 144.0);
    assert_eq!(result.req_dex, 66.0);
    assert_eq!(result.req_int, 363.0);
}

#[test]
fn gem_requirements_follow_more_and_highest_attribute_text() {
    let data = crate::test_data();
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
    let data = crate::test_data();
    let build = pobr_build::parse_build(r#"<PathOfBuilding2><Build level="90" className="Witch"/>
        <Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="18" corruptLevel="1" enabled="true"/></Skill></Skills>
        </PathOfBuilding2>"#).unwrap();
    assert_eq!(build.socket_groups[0].gem_skills[0].gem_level, 19);
    assert_eq!(build.socket_groups[0].gem_requirement_levels, [18]);
    let result = calculate_with_data(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(result.req_int, 137.0);
}

#[test]
fn a_separate_gem_keeps_requirements_beside_the_same_item_grant() {
    let data = crate::test_data();
    let build = pobr_build::parse_build(
        r#"<PathOfBuilding2><Build level="90" className="Witch"/>
        <Items><Item id="1">Rarity: RARE
Test Amulet
Absent Amulet
Implicits: 1
Grants Skill: Level 20 Cast on Dodge
</Item><ItemSet id="1"><Slot name="Amulet" itemId="1"/></ItemSet></Items>
        <Skills>
        <Skill enabled="true"><Gem skillId="MetaCastOnDodgePlayer" level="20" quality="0" enabled="true"/></Skill>
        <Skill enabled="true"><Gem skillId="MetaCastOnDodgePlayer" level="20" corruptLevel="1" corrupted="true" quality="23" enabled="true"/></Skill>
        <Skill enabled="true" slot="Amulet" source="Item:1:Test Amulet, Absent Amulet"><Gem skillId="MetaCastOnDodgePlayer" level="20" quality="0" enabled="true"/></Skill>
        </Skills></PathOfBuilding2>"#,
    )
    .unwrap();
    let result = calculate_with_data(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    // The first group represents the amulet's grant. The second is an actual
    // level-20 gem, with a corruption bonus that does not raise its requirements.
    assert_eq!(result.req_dex, 86.0);
    assert_eq!(result.req_int, 86.0);

    let mut only_granted = build;
    only_granted.socket_groups.remove(1);
    let result =
        calculate_with_data(&only_granted, &data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(result.req_dex, 0.0);
    assert_eq!(result.req_int, 0.0);
}
