use serde_json::{Value, json};

fn fixture() -> String {
    pobr_build::encode_pob_code(r#"<PathOfBuilding2>
<Build level="30" className="Witch" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec title="Mapping {map}" nodes="" treeVersion="0_5"/><Spec title="Boss {boss}" nodes="" treeVersion="0_5"/></Tree>
<Skills activeSkillSet="1"><SkillSet id="1" title="Shared"><Skill enabled="true" mainActiveSkill="1" enabled1="true" enabled2="false"><Gem skillId="SparkPlayer" level="5" enabled="true"/></Skill></SkillSet></Skills>
<Config activeConfigSet="7" future="keep"><ConfigSet id="7" title="Mapping {map}" extension="keep"><Input name="enemyIsBoss" string="None"/><Input name="futureKey" number="0" extension="raw"/><Input name="futureSwitch" boolean="false"/><Input name="futureText" string="a &amp; b"/><Input name="opaque" future="body"/><Future><Nested value="keep"/></Future><CustomModifierBlock title="Optional" enabled="false">+100 to maximum Life</CustomModifierBlock></ConfigSet><ConfigSet id="3" title="Boss {boss}"><Input name="enemyIsBoss" string="Pinnacle"/></ConfigSet></Config>
<Notes>unchanged</Notes></PathOfBuilding2>"#).unwrap()
}

fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}
fn manage(code: &str, op: &str, id: usize, name: &str) -> String {
    serde_json::from_str(
        &pobr_wasm::manage_loadout_json(
            &json!({"code": code, "kind": "config", "op": op, "config": id, "name": name})
                .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn select(code: &str, id: usize) -> Value {
    serde_json::from_str(
        &pobr_wasm::decode_build_loadout_json(&json!({"code": code, "config": id}).to_string())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn config_sets_switch_independently_and_bind_to_loadouts() {
    let code = fixture();
    let first = decode(&code);
    assert_eq!(
        first["config_sets"],
        json!([{"id":7,"title":"Mapping {map}"},{"id":3,"title":"Boss {boss}"}])
    );
    assert_eq!(first["loadouts"][1]["config"], 3);
    assert_eq!(first["active_loadout"], 0);
    let second = select(&code, 3);
    for key in [
        "character",
        "tree",
        "items",
        "socket_groups",
        "main_socket_group",
        "weapon_swap",
        "notes",
    ] {
        assert_eq!(first[key], second[key], "switch changed {key}");
    }
    assert_eq!(second["config_inputs"]["enemyIsBoss"], "Pinnacle");
    assert_eq!(second["active_config_set"], 3);
    assert!(second["active_loadout"].is_null());
    let mut selection = first["loadouts"][1].clone();
    selection["code"] = json!(code);
    let grouped: Value = serde_json::from_str(
        &pobr_wasm::decode_build_loadout_json(&selection.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(grouped["active_loadout"], 1);
    assert_eq!(grouped["config_inputs"]["enemyIsBoss"], "Pinnacle");
    assert!(
        pobr_wasm::decode_build_loadout_json(&json!({"code":code,"config":99}).to_string())
            .is_err()
    );
}

#[test]
fn config_set_management_keeps_ids_and_unknown_inputs_through_export() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let code = fixture();
    let copied = manage(&code, "duplicate", 7, "Copy & test");
    let decoded = decode(&copied);
    assert_eq!(decoded["active_config_set"], 8);
    assert_eq!(decoded["config_inputs"]["futureKey"].as_f64(), Some(0.0));
    let mut groups = decoded["socket_groups"].clone();
    for group in groups.as_array_mut().unwrap() {
        group.as_object_mut().unwrap().remove("active_skill_id");
    }
    let exported = pobr_wasm::encode_build_json(&json!({
        "base_code": copied, "character": decoded["character"], "allocated_nodes": [],
        "socket_groups": groups, "main_socket_group": decoded["main_socket_group"],
        "config_inputs": decoded["config_inputs"], "custom_modifier_blocks": decoded["custom_modifier_blocks"],
        "items": [], "notes": "unchanged"
    }).to_string()).unwrap();
    let xml = pobr_build::decode_pob_code(&exported).unwrap();
    assert_eq!(xml.matches("<ConfigSet ").count(), 3);
    assert_eq!(xml.matches(r#"extension="raw""#).count(), 2);
    assert_eq!(
        xml.matches(r#"<Input name="opaque" future="body"/>"#)
            .count(),
        2
    );
    assert_eq!(
        xml.matches(r#"<Future><Nested value="keep"/></Future>"#)
            .count(),
        2
    );
    let restored = decode(&exported);
    assert_eq!(restored["active_config_set"], 8);
    assert_eq!(restored["config_inputs"]["futureText"], "a & b");
    assert_eq!(restored["config_inputs"]["futureSwitch"], false);
    assert_eq!(restored["custom_modifier_blocks"][0]["enabled"], false);
    let renamed = manage(&exported, "rename", 8, "Renamed");
    assert_eq!(decode(&renamed)["config_sets"][2]["title"], "Renamed");
    let removed = manage(&renamed, "remove", 7, "");
    assert_eq!(decode(&removed)["active_config_set"], 8);
    let removed = manage(&removed, "remove", 8, "");
    assert_eq!(decode(&removed)["active_config_set"], 3);
    assert_eq!(decode(&removed)["config_inputs"]["enemyIsBoss"], "Pinnacle");
    let created = manage(&removed, "create", 3, "Empty");
    assert!(
        decode(&created)["config_inputs"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert!(
        pobr_wasm::manage_loadout_json(
            &json!({"code":removed,"kind":"config","op":"remove","config":3}).to_string()
        )
        .is_err()
    );
}

#[test]
fn flat_legacy_configuration_becomes_the_first_named_set() {
    let code = pobr_build::encode_pob_code(r#"<PathOfBuilding2><Build level="1" className="Witch"/><Config><Input name="legacy" string="keep"/></Config></PathOfBuilding2>"#).unwrap();
    let copied = manage(&code, "duplicate", 1, "Copy");
    assert_eq!(decode(&copied)["config_sets"].as_array().unwrap().len(), 2);
    assert_eq!(select(&copied, 1)["config_inputs"]["legacy"], "keep");
    assert_eq!(decode(&copied)["config_inputs"]["legacy"], "keep");
}
