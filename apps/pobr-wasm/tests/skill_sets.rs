use serde_json::{Value, json};

fn fixture() -> String {
    pobr_build::encode_pob_code(r#"<PathOfBuilding2>
<Build level="30" className="Witch" mainSocketGroup="2"/>
<Tree activeSpec="1"><Spec title="Mapping {map}" nodes="" treeVersion="0_5"/><Spec title="Boss {boss}" nodes="" treeVersion="0_5"/></Tree>
<Skills activeSkillSet="7"><SkillSet id="7" title="Mapping {map}" future="keep">
<Skill enabled="true" mainActiveSkill="1" set1="true" set2="true" label="Spark label"><Gem skillId="SparkPlayer" nameSpec="Spark" level="5" quality="7" enabled="true" future="gem" enableGlobal1="true" enableGlobal2="false" count="3"/></Skill>
<Skill enabled="true" mainActiveSkill="1" set1="true" set2="false" includeInFullDPS="true" groupCount="2"><Gem skillId="FireballPlayer" level="8" quality="0" enabled="true"/></Skill>
</SkillSet><SkillSet id="3" title="Boss {boss}" mainSocketGroup="1"><Skill enabled="true" set1="true" set2="true"><Gem skillId="IceNovaPlayer" level="10" quality="0" enabled="true"/></Skill></SkillSet></Skills>
<Config activeConfigSet="9"><ConfigSet id="9"><Input name="futureConfig" number="0"/></ConfigSet></Config><Notes>Keep</Notes></PathOfBuilding2>"#).unwrap()
}
fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}
fn select(code: &str, id: usize) -> Value {
    serde_json::from_str(
        &pobr_wasm::decode_build_loadout_json(&json!({"code":code,"skill":id}).to_string())
            .unwrap(),
    )
    .unwrap()
}
fn manage(code: &str, op: &str, id: usize, name: &str) -> String {
    serde_json::from_str(
        &pobr_wasm::manage_loadout_json(
            &json!({"code":code,"kind":"skill","skill":id,"op":op,"name":name}).to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn skill_sets_switch_independently_with_main_group_and_weapon_bindings() {
    let code = fixture();
    let first = decode(&code);
    assert_eq!(first["main_socket_group"], 1);
    assert_eq!(first["skill_sets"][0]["id"], 7);
    assert_eq!(first["loadouts"][1]["skill"], 3);
    let second = select(&code, 3);
    assert_eq!(second["main_socket_group"], 0);
    assert_eq!(
        second["socket_groups"][0]["gems"][0]["skill_id"],
        "IceNovaPlayer"
    );
    for key in [
        "character",
        "tree",
        "items",
        "config_inputs",
        "custom_modifier_blocks",
        "active_config_set",
        "weapon_swap",
        "notes",
    ] {
        assert_eq!(first[key], second[key], "switch changed {key}");
    }
    let back = select(second["code"].as_str().unwrap(), 7);
    assert_eq!(back["main_socket_group"], 1);
    assert_eq!(first["socket_groups"], back["socket_groups"]);
    assert_eq!(back["socket_groups"][1]["weapon_set"], 1);
    assert_eq!(
        back["socket_groups"][0]["gems"][0]["enable_global_2"],
        false
    );
    let mut selection = first["loadouts"][1].clone();
    selection["code"] = json!(code);
    let grouped: Value = serde_json::from_str(
        &pobr_wasm::decode_build_loadout_json(&selection.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(grouped["active_loadout"], 1);
    assert_eq!(grouped["active_skill_set"], 3);
    assert!(
        pobr_wasm::decode_build_loadout_json(&json!({"code":code,"skill":99}).to_string()).is_err()
    );
}

#[test]
fn skill_set_copy_edit_export_and_management_preserve_other_sets() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let code = fixture();
    let copied = manage(&code, "duplicate", 7, "Copy");
    let decoded = decode(&copied);
    assert_eq!(decoded["active_skill_set"], 8);
    assert_eq!(decoded["main_socket_group"], 1);
    let mut groups = decoded["socket_groups"].clone();
    for group in groups.as_array_mut().unwrap() {
        group.as_object_mut().unwrap().remove("active_skill_id");
    }
    groups[0]["gems"][0]["level"] = json!(12);
    let exported = pobr_wasm::encode_build_json(&json!({
        "base_code":copied,"character":decoded["character"],"allocated_nodes":[],"items":[],
        "socket_groups":groups,"main_socket_group":0,"config_inputs":decoded["config_inputs"],"notes":"Keep"
    }).to_string()).unwrap();
    let xml = pobr_build::decode_pob_code(&exported).unwrap();
    assert_eq!(xml.matches("<SkillSet ").count(), 3);
    assert_eq!(xml.matches(r#"label="Spark label""#).count(), 2);
    assert_eq!(xml.matches(r#"future="gem""#).count(), 2);
    let result = decode(&exported);
    assert_eq!(result["active_skill_set"], 8);
    assert_eq!(result["main_socket_group"], 0);
    assert_eq!(result["socket_groups"][0]["gems"][0]["level"], 12);
    assert_eq!(result["socket_groups"][1]["weapon_set"], 1);
    assert_eq!(result["socket_groups"][1]["include_in_full_dps"], true);
    assert_eq!(
        result["socket_groups"][1]["group_count"].as_f64(),
        Some(2.0)
    );
    let original = select(&exported, 7);
    assert_eq!(original["main_socket_group"], 1);
    assert_eq!(original["socket_groups"][0]["gems"][0]["level"], 5);
    let back = select(original["code"].as_str().unwrap(), 8);
    assert_eq!(back["main_socket_group"], 0);
    let renamed = manage(back["code"].as_str().unwrap(), "rename", 8, "Renamed");
    assert_eq!(decode(&renamed)["skill_sets"][2]["title"], "Renamed");
    let removed = manage(&renamed, "remove", 7, "");
    assert_eq!(decode(&removed)["active_skill_set"], 8);
    let removed = manage(&removed, "remove", 8, "");
    assert_eq!(decode(&removed)["active_skill_set"], 3);
    let created = manage(&removed, "create", 3, "Empty");
    assert_eq!(decode(&created)["socket_groups"], json!([]));
    assert!(decode(&created)["main_socket_group"].is_null());
    assert!(
        pobr_wasm::manage_loadout_json(
            &json!({"code":removed,"kind":"skill","op":"remove","skill":3}).to_string()
        )
        .is_err()
    );
}

#[test]
fn legacy_flat_skills_become_a_named_set_with_the_main_group() {
    let code = pobr_build::encode_pob_code(r#"<PathOfBuilding2><Build level="1" className="Witch" mainSocketGroup="2"/><Skills><Skill enabled="true"><Gem skillId="SparkPlayer" level="5"/></Skill><Skill enabled="true"><Gem skillId="FireballPlayer" level="5"/></Skill></Skills></PathOfBuilding2>"#).unwrap();
    assert_eq!(
        select(&code, 1)["socket_groups"].as_array().unwrap().len(),
        2
    );
    let copied = manage(&code, "duplicate", 1, "Copy");
    assert_eq!(decode(&copied)["skill_sets"].as_array().unwrap().len(), 2);
    assert_eq!(decode(&copied)["main_socket_group"], 1);
    assert_eq!(select(&copied, 1)["main_socket_group"], 1);
}

#[test]
fn exporting_legacy_skills_flushes_edits_before_creating_a_set() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let base = pobr_build::encode_pob_code(r#"<PathOfBuilding2><Build level="1" className="Witch"/><Skills defaultGemQuality="5"><Skill enabled="true" label="Legacy"><Gem skillId="SparkPlayer" level="5"/></Skill></Skills></PathOfBuilding2>"#).unwrap();
    let code = pobr_wasm::encode_build_json(&json!({"base_code":base,"character":{"class_name":"Witch","level":1},
        "socket_groups":[{"enabled":true,"gems":[{"skill_id":"SparkPlayer","level":9,"quality":0}]}],"main_socket_group":0}).to_string()).unwrap();
    let copied = manage(&code, "duplicate", 1, "Copy");
    let restored = decode(&copied);
    assert_eq!(restored["main_socket_group"], 0);
    assert_eq!(restored["socket_groups"][0]["gems"][0]["level"], 9);
    assert_eq!(
        select(&copied, 1)["socket_groups"][0]["gems"][0]["level"],
        9
    );
    let xml = pobr_build::decode_pob_code(&copied).unwrap();
    assert!(xml.contains(r#"defaultGemQuality="5""#));
    assert_eq!(xml.matches(r#"label="Legacy""#).count(), 2);
}

#[test]
fn an_unmarked_sparse_skill_set_uses_the_first_imported_set() {
    let xml = pobr_build::decode_pob_code(&fixture())
        .unwrap()
        .replace(" activeSkillSet=\"7\"", "");
    let decoded = decode(&pobr_build::encode_pob_code(&xml).unwrap());
    assert_eq!(decoded["active_skill_set"], 7);
    assert_eq!(decoded["main_socket_group"], 1);
    assert_eq!(decoded["socket_groups"].as_array().unwrap().len(), 2);
}
