use serde_json::{Value, json};

fn init() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
}
fn decode(file: &Value) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_file_json(&file.to_string()).unwrap()).unwrap()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/pob2-planner.build")).unwrap()
}
fn code(build: &Value) -> String {
    pobr_wasm::encode_build_json(&json!({
        "character":build["character"], "allocated_nodes":build["tree"]["allocated_nodes"],
        "attribute_choices":build["tree"]["attribute_choices"], "weapon_swap":build["weapon_swap"],
        "items":build["items"]["equipped"], "socket_groups":build["socket_groups"].as_array().unwrap().iter().map(|group| {
            let mut group = group.clone(); group.as_object_mut().unwrap().remove("active_skill_id"); group
        }).collect::<Vec<_>>()
    }).to_string()).unwrap()
}
fn export(code: &str, source: Option<&Value>, all: bool) -> Value {
    serde_json::from_str(&pobr_wasm::encode_build_file_json(&json!({"code":code,"name":"Export test", "all_loadouts":all, "source":source.map(Value::to_string)}).to_string()).unwrap()).unwrap()
}

#[test]
fn reads_recorded_pob2_writer_output_without_inventing_equipment() {
    init();
    let build = decode(&fixture());
    assert_eq!(build["character"]["class_name"], "Ranger");
    assert_eq!(build["character"]["level"], 1);
    assert_eq!(build["tree"]["allocated_nodes"], json!([3936]));
    assert_eq!(
        build["weapon_swap"]["exclusive_nodes"],
        json!([[], [46325]])
    );
    assert_eq!(build["items"]["equipped"], json!([]));
    assert_eq!(
        build["socket_groups"][0]["gems"][0]["skill_id"],
        "IceShotPlayer"
    );
    assert_eq!(build["socket_groups"][0]["gems"][0]["level"], 17);
    assert_eq!(build["socket_groups"][0]["gems"][0]["quality"], 12);
    assert_eq!(build["socket_groups"][0]["gems"][1]["level"], 1);
    assert!(build["notes"].as_str().unwrap().contains("hints only"));
    assert_eq!(
        serde_json::from_str::<Value>(build["planner_source"].as_str().unwrap()).unwrap(),
        fixture()
    );
}

#[test]
fn roundtrip_retains_carried_fields_and_updates_edited_gem_hints() {
    init();
    let mut source = fixture();
    source["link"] = json!("https://example.invalid/guide");
    source["passives"].as_array_mut().unwrap().push(
        json!({"id":"attributes1","additional_text":"Attribute: dex", "level_interval":[10,100]}),
    );
    source["inventory_slots"][0]["unique_name"] = json!("Example hint");
    let mut build = decode(&source);
    build["socket_groups"][0]["gems"][0]["level"] = json!(19);
    let result = export(&code(&build), Some(&source), false);
    assert_eq!(result["files"].as_array().unwrap().len(), 1);
    let file: Value =
        serde_json::from_str(result["files"][0]["content"].as_str().unwrap()).unwrap();
    for key in [
        "author",
        "link",
        "description",
        "ascendancy",
        "inventory_slots",
    ] {
        assert_eq!(file[key], source[key], "{key}");
    }
    assert_eq!(
        file["skills"][0]["additional_text"],
        "Level 19, 12% Quality"
    );
    let restored = decode(&file);
    for key in ["character", "tree", "weapon_swap", "socket_groups", "items"] {
        assert_eq!(restored[key], build[key], "{key}");
    }
    let passive = file["passives"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "attributes1")
        .unwrap();
    assert_eq!(passive["level_interval"], json!([10, 100]));
}

#[test]
fn official_shorthands_and_optional_collections_are_accepted() {
    init();
    let build = decode(&json!({"name":"Empty planner", "ascendancy":"Ranger1"}));
    assert_eq!(build["items"]["equipped"], json!([]));
    let build = decode(
        &json!({"name":"Shorthands", "passives":["melee17"], "skills":["Metadata/Items/Gem/SkillGemIceShot"]}),
    );
    assert_eq!(build["tree"]["allocated_nodes"], json!([3936]));
    assert_eq!(build["socket_groups"][0]["gems"][0]["level"], 1);
    for invalid in [
        json!({"name":"Invalid", "passives":[{"id":"melee17","weapon_set":3}]}),
        json!({"name":"Invalid", "inventory_slots":[{"inventory_id":"Weapon1", "slot_x":-1}]}),
        json!({"name":"Invalid", "state":{}}),
    ] {
        assert!(pobr_wasm::decode_build_file_json(&invalid.to_string()).is_err());
    }
}

#[test]
fn all_loadouts_get_distinct_files_and_keep_each_tree() {
    init();
    let xml = r#"<PathOfBuilding2><Build level="50" className="Ranger" ascendClassName="Deadeye"/>
<Tree activeSpec="1"><Spec title="A" nodes="3936" treeVersion="0_5"/><Spec title="B" nodes="46325" treeVersion="0_5"/></Tree><Skills/><Items/></PathOfBuilding2>"#;
    let code = pobr_build::encode_pob_code(xml).unwrap();
    let out = export(&code, None, true);
    let files = out["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_ne!(files[0]["filename"], files[1]["filename"]);
    for (i, id) in ["melee17", "melee18"].iter().enumerate() {
        let file: Value = serde_json::from_str(files[i]["content"].as_str().unwrap()).unwrap();
        assert_eq!(file["passives"][0]["id"], *id);
    }
}

#[test]
fn unsupported_meta_gems_are_diagnosed_and_old_trees_are_rejected() {
    init();
    let source = json!({"name":"Meta", "ascendancy":"Ranger1", "skills":["Metadata/Items/Gems/SkillGemCastOnCritical"]});
    let out = export(&code(&decode(&source)), None, false);
    let file: Value = serde_json::from_str(out["files"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(file["skills"], json!([]));
    assert!(
        out["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("Meta gem"))
    );
    let code = pobr_build::encode_pob_code(r#"<PathOfBuilding2><Build level="1" className="Ranger"/><Tree activeSpec="1"><Spec treeVersion="0_1" nodes="3936"/></Tree><Skills/><Items/></PathOfBuilding2>"#).unwrap();
    assert!(
        pobr_wasm::encode_build_file_json(&json!({"code":code,"name":"Old"}).to_string())
            .unwrap_err()
            .contains("historical")
    );
}
