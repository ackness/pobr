//! Unresolved source slots survive export without acquiring calculation effects.

use serde_json::{Value, json};

const COMET_CODE: &str =
    include_str!("../../../examples/demo-bd-test/builds/sorceress-stormweaver-comet/code.txt");

fn init() {
    pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy()).unwrap();
}

fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}

fn materialized(decoded: &Value) -> Value {
    let groups: Vec<Value> = decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| {
            let mut group = group.clone();
            group.as_object_mut().unwrap().remove("active_skill_id");
            group
        })
        .collect();
    json!({
        "character": decoded["character"],
        "tree_version": decoded["tree"]["tree_version"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": groups,
        "items": decoded["items"]["equipped"],
        "flasks": decoded["items"]["flasks"],
        "jewels": decoded["items"]["socket_jewels"],
        "weapon_swap": decoded["weapon_swap"],
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": decoded["config_inputs"],
        "custom_modifier_blocks": decoded["custom_modifier_blocks"],
    })
}

fn calculate(request: &Value) -> Value {
    serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap()).unwrap()
}

fn export(request: &Value) -> (String, Value) {
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let xml = pobr_build::decode_pob_code(&code).unwrap();
    (xml, decode(&code))
}

fn fireball_request() -> Value {
    json!({
        "character": { "level": 85, "class_name": "Sorceress" },
        "main_socket_group": 0,
        "socket_groups": [{
            "enabled": true,
            "gems": [
                { "skill_id": "FireballPlayer", "level": 20, "quality": 0 },
                { "skill_id": "SupportForkPlayer", "level": 1, "quality": 0 },
            ],
        }],
    })
}

#[test]
fn committed_comet_decode_preserves_the_named_zero_level_slot() {
    let decoded = decode(COMET_CODE);
    assert_eq!(
        decoded["socket_groups"][8]["gems"][1],
        json!({ "skill_id": "", "level": 0, "quality": 0, "name_spec": "Unleash" })
    );
    assert_eq!(
        decoded["socket_groups"][8]["gems"][0]["skill_id"],
        "FirestormPlayer"
    );
    assert_eq!(
        decoded["socket_groups"][8]["gems"][2]["skill_id"],
        "SupportSpellCascadePlayer"
    );
    for group in decoded["socket_groups"].as_array().unwrap() {
        for gem in group["gems"].as_array().unwrap() {
            if gem["skill_id"] != "" {
                assert!(
                    gem.get("name_spec").is_none(),
                    "known gem metadata stays absent"
                );
            }
        }
    }
}

#[test]
fn comet_export_reimport_preserves_slots_and_complete_direct_calculation() {
    init();
    let original = decode(COMET_CODE);
    let mut request = materialized(&original);
    request["base_code"] = json!(COMET_CODE.trim());
    let before = calculate(&request);
    let (_, exported) = export(&request);
    assert_eq!(exported["socket_groups"], original["socket_groups"]);
    assert_eq!(calculate(&materialized(&exported)), before);
}

#[test]
fn unresolved_name_metadata_never_changes_direct_calculation_or_diagnostics() {
    init();
    let original = materialized(&decode(COMET_CODE));
    let mut absent = original.clone();
    absent["socket_groups"][8]["gems"][1]
        .as_object_mut()
        .unwrap()
        .remove("name_spec");
    let baseline = calculate(&absent);
    for name in [
        Value::Null,
        json!("Unleash"),
        json!("Atziri's Communion"),
        json!("Fireball"),
    ] {
        let mut request = original.clone();
        request["socket_groups"][8]["gems"][1]["name_spec"] = name;
        assert_eq!(calculate(&request), baseline);
    }
    let mut removed = original;
    removed["socket_groups"][8]["gems"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(calculate(&removed), baseline);
}

#[test]
fn unresolved_export_keeps_current_properties_group_metadata_and_escaped_name() {
    init();
    let name = "Unknown <slot> & \"quoted\" 'source'";
    let mut request = fireball_request();
    request["socket_groups"][0]["enabled"] = json!(false);
    request["socket_groups"][0]["source"] = json!("Item:7:Copied & source");
    request["socket_groups"][0]["slot"] = json!("weapon1");
    request["socket_groups"][0]["weapon_set"] = json!(2);
    request["socket_groups"][0]["main_active_skill"] = json!(1);
    request["socket_groups"][0]["gems"].as_array_mut().unwrap().insert(
        1,
        json!({ "skill_id": "", "level": 0, "quality": 17, "stat_set_index": 2, "name_spec": name }),
    );
    let (_, decoded) = export(&request);
    assert_eq!(
        decoded["socket_groups"][0]["gems"],
        request["socket_groups"][0]["gems"]
    );
    for field in [
        "enabled",
        "source",
        "slot",
        "weapon_set",
        "main_active_skill",
    ] {
        assert_eq!(
            decoded["socket_groups"][0][field],
            request["socket_groups"][0][field]
        );
    }
    request["socket_groups"][0]["gems"][1]["level"] = json!(7);
    request["socket_groups"][0]["gems"][1]["quality"] = json!(3);
    request["socket_groups"][0]["gems"][1]["stat_set_index"] = json!(4);
    let (_, edited) = export(&request);
    assert_eq!(
        edited["socket_groups"][0]["gems"][1],
        request["socket_groups"][0]["gems"][1]
    );
}

#[test]
fn removed_unresolved_slot_is_not_resurrected_from_the_base_code() {
    init();
    let original = decode(COMET_CODE);
    let mut request = materialized(&original);
    request["base_code"] = json!(COMET_CODE.trim());
    request["socket_groups"][8]["gems"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    let (xml, decoded) = export(&request);
    assert_eq!(
        decoded["socket_groups"][8]["gems"],
        request["socket_groups"][8]["gems"]
    );
    assert!(!xml.contains("nameSpec=\"Unleash\""));
}

#[test]
fn missing_null_and_empty_names_reject_lossy_export_without_changing_calculation() {
    init();
    for name in [None, Some(Value::Null), Some(json!(""))] {
        let mut request = fireball_request();
        let mut gem = json!({ "skill_id": "", "level": 0, "quality": 0 });
        if let Some(name) = name {
            gem["name_spec"] = name;
        }
        request["socket_groups"][0]["gems"]
            .as_array_mut()
            .unwrap()
            .insert(1, gem);
        let error: Value =
            serde_json::from_str(&pobr_wasm::encode_build_json(&request.to_string()).unwrap_err())
                .unwrap();
        assert_eq!(error["code"], "bad_request");
        assert!(error["message"].as_str().unwrap().contains("name_spec"));
        let baseline = calculate(&fireball_request());
        assert_eq!(calculate(&request), baseline);
    }
}

#[test]
fn known_and_unknown_nonempty_ids_ignore_names_without_losing_unknown_rows() {
    init();
    let mut request = fireball_request();
    request["socket_groups"][0]["gems"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "skill_id": "UnrecognizedSourceEffect", "level": 1, "quality": 0,
        }));
    let baseline = calculate(&request);
    let original_gems = request["socket_groups"][0]["gems"].clone();
    for gem in request["socket_groups"][0]["gems"].as_array_mut().unwrap() {
        gem["name_spec"] = json!("SupportForkPlayer");
    }
    assert_eq!(calculate(&request), baseline);
    let (xml, decoded) = export(&request);
    assert!(!xml.contains("nameSpec="));
    assert_eq!(decoded["socket_groups"][0]["gems"], original_gems);
    assert_eq!(calculate(&materialized(&decoded)), baseline);
}

#[test]
fn known_active_and_support_roundtrip_keeps_complete_calculation_and_diagnostics() {
    init();
    let request = fireball_request();
    let baseline = calculate(&request);
    assert!(
        baseline["unsupported_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| { line.as_str().unwrap().contains("SupportForkPlayer") })
    );
    let (_, decoded) = export(&request);
    assert_eq!(
        decoded["socket_groups"][0]["gems"],
        request["socket_groups"][0]["gems"]
    );
    assert_eq!(calculate(&materialized(&decoded)), baseline);
}

fn named_slot(name: &str) -> Value {
    json!({
        "skill_id": "",
        "level": 0,
        "quality": 0,
        "name_spec": name,
    })
}

#[test]
fn export_normalization_preserves_leading_shifted_and_multiple_unresolved_positions() {
    init();
    let known = fireball_request()["socket_groups"][0]["gems"].clone();
    let active = known[0].clone();
    let fork = known[1].clone();
    let mastery = json!({
        "skill_id": "SupportFireMasteryPlayer", "level": 1, "quality": 0,
    });
    let first = named_slot("Leading <source> & \"name\"");
    let second = named_slot("Second source");
    let third = named_slot("Third source");
    let last = named_slot("Last source");
    let cases = [
        (
            "leading",
            json!([first, active, fork]),
            json!([first, active, fork]),
        ),
        (
            "shifted",
            json!([fork, first, active]),
            json!([active, first, fork]),
        ),
        (
            "multiple",
            json!([first, fork, second, mastery, third, active, last]),
            json!([first, active, second, fork, third, mastery, last]),
        ),
        (
            "all identified",
            json!([fork, mastery, active]),
            json!([active, fork, mastery]),
        ),
    ];
    for (label, gems, expected) in cases {
        let mut request = fireball_request();
        request["socket_groups"][0]["gems"] = gems;
        let before = calculate(&request);
        let (_, decoded) = export(&request);
        assert_eq!(decoded["socket_groups"][0]["gems"], expected, "{label}");
        assert_eq!(calculate(&materialized(&decoded)), before, "{label}");
    }
}

#[test]
fn normalization_keeps_current_unresolved_names_properties_and_removal_after_export() {
    init();
    let known = fireball_request()["socket_groups"][0]["gems"].clone();
    let active = known[0].clone();
    let fork = known[1].clone();
    let first = named_slot("Original first");
    let removed = named_slot("Remove this source");
    let last = named_slot("Original last");
    let mut request = fireball_request();
    request["socket_groups"][0]["gems"] = json!([first, fork, removed, active, last]);
    let (xml, decoded) = export(&request);
    assert_eq!(
        decoded["socket_groups"][0]["gems"],
        json!([first, active, removed, fork, last])
    );
    let mut edited = materialized(&decoded);
    edited["base_code"] = json!(pobr_build::encode_pob_code(&xml).unwrap());
    let current = json!({
        "skill_id": "",
        "level": 4,
        "quality": 9,
        "stat_set_index": 3,
        "name_spec": "Current <edited> & \"source\"",
    });
    edited["socket_groups"][0]["gems"][0] = current.clone();
    edited["socket_groups"][0]["gems"]
        .as_array_mut()
        .unwrap()
        .remove(2);
    let before = calculate(&edited);
    let (_, reimported) = export(&edited);
    assert_eq!(
        reimported["socket_groups"][0]["gems"],
        json!([current, active, fork, last])
    );
    assert_eq!(calculate(&materialized(&reimported)), before);
}

#[test]
fn unresolved_only_group_survives_export_without_shifting_the_selected_later_group() {
    init();
    let mut request = fireball_request();
    let known_group = request["socket_groups"][0].clone();
    let unknown = named_slot("Unresolved first group");
    request["socket_groups"] = json!([
        { "enabled": true, "gems": [unknown] },
        known_group,
    ]);
    request["main_socket_group"] = json!(1);
    let before = calculate(&request);
    assert_eq!(before["main_skill"]["group_index"], 1);
    let (_, decoded) = export(&request);
    assert_eq!(decoded["socket_groups"][0]["gems"], json!([unknown]));
    assert_eq!(decoded["socket_groups"][1]["gems"], known_group["gems"]);
    assert_eq!(decoded["main_socket_group"], 1);
    assert_eq!(calculate(&materialized(&decoded)), before);
}
