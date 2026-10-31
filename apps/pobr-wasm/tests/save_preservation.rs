//! Ordinary edit-state export must not turn absent catalog defaults into false.

use serde_json::{Value, json};

const INFERNALIST_CODE: &str = include_str!(
    "../../../examples/reference-builds/ninja-top10-2026-10-26/builds/infernalist-01/code.txt"
);

fn init() {
    pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy()).unwrap();
}

fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}

fn calculate(request: &Value) -> Value {
    serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap()).unwrap()
}

// Host contract projection. The companion TS/WASM regression uses the actual
// materialize/paramsFromDecoded/toRequest functions, including weaponSetRequest.
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
        "full_dps": groups.iter().any(|group| group["include_in_full_dps"] == true),
        "items": decoded["items"]["equipped"],
        "flasks": decoded["items"]["flasks"],
        "limbs": decoded["items"]["limbs"],
        "jewels": decoded["items"]["socket_jewels"],
        "weapon_swap": decoded["weapon_swap"],
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": decoded["config_inputs"],
        "custom_modifier_blocks": decoded["custom_modifier_blocks"],
        "party": decoded["party"],
        "breakdowns": [],
    })
}

fn assert_roundtrip(request: &Value) -> Value {
    let before = calculate(request);
    let saved = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let reopened = decode(&saved);
    assert_eq!(
        calculate(&materialized(&reopened)),
        before,
        "save/reopen changed the complete calculation response"
    );
    assert_eq!(
        calculate(&json!({"pob_code": saved, "full_dps": request["full_dps"], "breakdowns": []})),
        before,
        "the saved code itself must have the same calculation semantics"
    );
    assert_eq!(
        reopened["config_inputs"], request["config_inputs"],
        "preserve absent keys and explicit scalar types/values"
    );
    reopened
}

#[test]
fn ordinary_infernalist_save_and_level_edit_preserve_complete_results() {
    init();
    let decoded = decode(INFERNALIST_CODE);
    let original = materialized(&decoded);
    assert!(original["config_inputs"].get("inDemonForm").is_none());
    let direct = calculate(&original);
    assert!(
        direct["energy_trigger"]["events_per_second"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(
        direct,
        calculate(
            &json!({"pob_code": INFERNALIST_CODE.trim(), "full_dps": original["full_dps"], "breakdowns": []})
        ),
        "the edit-state projection must agree with the imported source"
    );
    let level = decoded["character"]["level"].as_u64().unwrap();
    for current_level in [level, level - 1] {
        for with_base in [true, false] {
            let mut request = original.clone();
            request["character"]["level"] = json!(current_level);
            if with_base {
                request["base_code"] = json!(INFERNALIST_CODE.trim());
            }
            let reopened = assert_roundtrip(&request);
            assert_eq!(reopened["character"]["level"], current_level);
        }
    }
}

#[test]
fn explicit_condition_edits_and_reset_preserve_the_current_calculation() {
    init();
    let original = materialized(&decode(INFERNALIST_CODE));
    let mut rates = Vec::new();
    for enabled in [false, true] {
        let mut request = original.clone();
        request["config_inputs"]["inDemonForm"] = json!(enabled);
        rates.push(calculate(&request)["energy_trigger"]["events_per_second"].clone());
        for with_base in [true, false] {
            if with_base {
                request["base_code"] = json!(INFERNALIST_CODE.trim());
            } else {
                request.as_object_mut().unwrap().remove("base_code");
            }
            assert_roundtrip(&request);
        }
        // Reopen a source with an explicit setting, edit it in the opposite
        // direction, then reset to absence. The old base must not revive it.
        let saved = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
        request = materialized(&decode(&saved));
        request["base_code"] = json!(saved);
        request["config_inputs"]["inDemonForm"] = json!(!enabled);
        assert_roundtrip(&request);
        request["config_inputs"]
            .as_object_mut()
            .unwrap()
            .remove("inDemonForm");
        assert_roundtrip(&request);
    }
    assert_ne!(
        rates[0], rates[1],
        "the fixture must exercise the condition"
    );
}

#[test]
fn catalog_defaults_and_unknown_config_preserve_active_and_inactive_sets() {
    init();
    let inactive = r#"<ConfigSet id="3" title="Inactive"><Input name="inDemonForm" boolean="false"/><Input name="future" string="0042"/><Future keep="verbatim"/></ConfigSet>"#;
    let unknown = [
        r#"<Input name="futureFalse" boolean="false" extension="keep"/>"#,
        r#"<Input name="futureZero" number="0"/>"#,
        r#"<Input name="futureText" string="0042 &amp; raw"/>"#,
        r#"<Input name="opaque" future="body"/>"#,
        r#"<Future><Nested value="keep"/></Future>"#,
    ];
    for source_value in [None, Some(false), Some(true)] {
        let inputs: String = pobr_build::default_true_condition_keys()
            .filter_map(|key| {
                source_value.map(|value| format!(r#"<Input name="{key}" boolean="{value}"/>"#))
            })
            .collect();
        let unknown_xml = unknown.concat();
        let code = pobr_build::encode_pob_code(&format!(
            r#"<PathOfBuilding2><Build level="30" className="Witch"/>
            <Config activeConfigSet="7" extension="keep"><ConfigSet id="7" title="Active" extension="keep">{inputs}{unknown_xml}</ConfigSet>{inactive}</Config>
            </PathOfBuilding2>"#
        )).unwrap();
        let decoded = decode(&code);
        for edited_value in [source_value, Some(false), Some(true), None] {
            let mut config = decoded["config_inputs"].clone();
            for key in pobr_build::default_true_condition_keys() {
                match edited_value {
                    Some(value) => {
                        config[key] = json!(value);
                    }
                    None => {
                        config.as_object_mut().unwrap().remove(key);
                    }
                }
            }
            let saved = pobr_wasm::encode_build_json(
                &json!({
                    "base_code": code, "character": decoded["character"], "tree_version": "0_5",
                    "config_inputs": config,
                })
                .to_string(),
            )
            .unwrap();
            let reopened = decode(&saved);
            assert_eq!(
                reopened["config_inputs"], config,
                "source {source_value:?}, edit {edited_value:?}"
            );
            assert_eq!(reopened["active_config_set"], 7);
            assert_eq!(reopened["config_sets"], decoded["config_sets"]);
            let xml = pobr_build::decode_pob_code(&saved).unwrap();
            assert!(
                xml.contains(inactive),
                "inactive set must remain byte-for-byte intact"
            );
            for element in unknown {
                assert!(
                    xml.contains(element),
                    "unknown input/payload must survive: {element}"
                );
            }
            assert!(xml.contains(r#"<Config activeConfigSet="7" extension="keep">"#));
        }
    }
}
