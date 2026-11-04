//! Ordinary edit-state export must not turn absent catalog defaults into false.

use serde_json::{Value, json};

const INFERNALIST_CODE: &str = include_str!(
    "../../../examples/reference-builds/ninja-top10-2026-10-26/builds/infernalist-01/code.txt"
);
const EQUIPMENT_JEWEL_CODE: &str = include_str!(
    "../../../examples/wasm-benchmark/builds/ninja-2026-10-11-druid-oracle-lightning-bolt-950c9ad4451d/code.txt"
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
        "equipment_jewels": decoded["items"]["equipment_jewels"],
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

fn utility_code() -> String {
    pobr_build::encode_pob_code(include_str!("fixtures/utility-selection.xml")).unwrap()
}

fn stat(response: &Value, id: &str) -> f64 {
    response["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap()["value"]
        .as_f64()
        .unwrap()
}

#[test]
fn equipped_utility_selection_survives_materialization_and_export() {
    init();
    let code = utility_code();
    let decoded = decode(&code);
    let utilities = decoded["items"]["flasks"].as_array().unwrap();
    assert_eq!(utilities.len(), 3);
    for (slot, active) in [("Flask 1", false), ("Flask 2", true), ("Charm 1", false)] {
        assert_eq!(
            utilities.iter().find(|item| item["slot"] == slot).unwrap()["active"],
            active
        );
    }
    let original = materialized(&decoded);
    let calculated = calculate(&original);
    assert_eq!(
        calculated,
        calculate(&json!({"pob_code":code, "full_dps":false, "breakdowns":[]}))
    );
    for id in ["Flask1Recovery", "Flask1ManaRecovery"] {
        assert!((stat(&calculated, id) - 506.0).abs() < 1e-8, "{id}");
    }
    assert_eq!(stat(&calculated, "FireResist"), -10.0);
    let usage: Value =
        serde_json::from_str(&pobr_wasm::line_usage_json(&original.to_string()).unwrap()).unwrap();
    assert!(
        usage["usage"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|line| line["modifiers"].as_array().unwrap())
            .any(|modifier| modifier["name"] == "LifeFlaskAppliesToMana"
                && modifier["status"] == "applied")
    );
    for with_base in [false, true] {
        let mut request = original.clone();
        if with_base {
            request["base_code"] = json!(code);
        }
        let reopened = assert_roundtrip(&request);
        let restored = reopened["items"]["flasks"].as_array().unwrap();
        assert_eq!(restored.len(), utilities.len());
        for item in utilities {
            let after = restored
                .iter()
                .find(|entry| entry["slot"] == item["slot"])
                .unwrap();
            assert_eq!(after["active"], item["active"]);
            assert_eq!(
                after["text"].as_str().unwrap().trim(),
                item["text"].as_str().unwrap().trim()
            );
        }
    }
}

#[test]
fn utility_omission_clear_and_replacement_cover_both_baseline_collections() {
    init();
    let code = utility_code();
    let original = materialized(&decode(&code));
    for with_base in [false, true] {
        let mut request = original.clone();
        request["pob_code"] = json!(code);
        if with_base {
            request["base_code"] = json!(code);
        }
        request.as_object_mut().unwrap().remove("flasks");
        assert_eq!(calculate(&request), calculate(&original));
        let inherited = decode(&pobr_wasm::encode_build_json(&request.to_string()).unwrap());
        assert_eq!(inherited["items"]["flasks"].as_array().unwrap().len(), 3);
        assert_eq!(calculate(&materialized(&inherited)), calculate(&original));
        // Exercise an export whose only baseline is base_code as well.
        if with_base {
            let mut base_only = request.clone();
            base_only.as_object_mut().unwrap().remove("pob_code");
            assert_eq!(
                decode(&pobr_wasm::encode_build_json(&base_only.to_string()).unwrap())["items"]["flasks"],
                inherited["items"]["flasks"]
            );
        }
        for replacement in [
            json!([]),
            json!([{
                "slot":"Flask 1", "text":"Rarity: NORMAL\nUltimate Life Flask\nImplicits: 0\n+12% to Fire Resistance during effect", "active":false
            }]),
            json!([{"slot":"Charm 1", "text":"Rarity: NORMAL\nRuby Charm", "active":true}]),
        ] {
            request["flasks"] = replacement.clone();
            let mut fresh = request.clone();
            fresh.as_object_mut().unwrap().remove("pob_code");
            assert_eq!(
                calculate(&request),
                calculate(&fresh),
                "baseline must not retain stale utilities"
            );
            let variant: Value = serde_json::from_str(
                &pobr_wasm::optimize_variants_json(
                    &json!({
                        "request": {"pob_code":code}, "stats":["FireResist"],
                        "variants":[{"flasks":replacement}]
                    })
                    .to_string(),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(variant["variants"][0]["error"].is_null(), "{variant}");
            assert_eq!(
                variant["variants"][0]["stats"]["FireResist"],
                stat(&calculate(&fresh), "FireResist")
            );
            let exported = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
            let parsed =
                pobr_build::parse_build(&pobr_build::decode_pob_code(&exported).unwrap()).unwrap();
            let entries = replacement.as_array().unwrap();
            assert_eq!(
                parsed.utility_slots.len(),
                entries.iter().filter(|item| item["active"] == true).count()
            );
            assert_eq!(
                parsed.inactive_utility_slots.len(),
                entries
                    .iter()
                    .filter(|item| item["active"] == false)
                    .count()
            );
            let reopened = assert_roundtrip(&request);
            assert_eq!(
                reopened["items"]["flasks"].as_array().unwrap().len(),
                entries.len()
            );
        }
    }
}

#[test]
fn utility_selection_keeps_engine_gates_and_validates_legacy_inputs() {
    init();
    let mut request = materialized(&decode(&utility_code()));
    let fire = |request: &Value| stat(&calculate(request), "FireResist");
    let life = request["flasks"]
        .as_array()
        .unwrap()
        .iter()
        .position(|item| item["slot"] == "Flask 1")
        .unwrap();
    let charm = request["flasks"]
        .as_array()
        .unwrap()
        .iter()
        .position(|item| item["slot"] == "Charm 1")
        .unwrap();
    let mut unique = request.clone();
    unique["flasks"][life]["text"] = json!(
        request["flasks"][life]["text"]
            .as_str()
            .unwrap()
            .replace("Rarity: NORMAL", "Rarity: UNIQUE\nFixture Unique")
    );
    assert_eq!(fire(&unique), -50.0, "Reverie excludes unique flasks");
    assert_roundtrip(&unique);
    request["items"]
        .as_array_mut()
        .unwrap()
        .retain(|item| item["slot"] != "bodyarmour");
    assert_eq!(
        fire(&request),
        -50.0,
        "without Reverie, the unselected flask stays off"
    );
    assert_eq!(stat(&calculate(&request), "Flask1Recovery"), 920.0);
    assert_roundtrip(&request);
    request["flasks"][life]["active"] = json!(true);
    assert_eq!(fire(&request), -10.0);
    request["flasks"][life]
        .as_object_mut()
        .unwrap()
        .remove("active");
    assert_eq!(fire(&request), -10.0, "legacy omission means selected");
    request["flasks"][charm]["active"] = json!(true);
    assert_eq!(
        fire(&request),
        15.0,
        "the equipped Ruby Charm has budget but needs selection"
    );
    assert_roundtrip(&request);
    for invalid in [json!(null), json!("false"), json!(0), json!({})] {
        request["flasks"][life]["active"] = invalid;
        assert!(pobr_wasm::calculate_build_json(&request.to_string()).is_err());
        assert!(pobr_wasm::encode_build_json(&request.to_string()).is_err());
    }
    request["flasks"][life]["active"] = json!(false);
    for slot in ["Flask 3", "Charm 4", "Flask 2"] {
        request["flasks"][life]["slot"] = json!(slot);
        assert!(
            pobr_wasm::calculate_build_json(&request.to_string()).is_err(),
            "invalid/duplicate {slot}"
        );
        assert!(pobr_wasm::encode_build_json(&request.to_string()).is_err());
    }
}

#[test]
fn equipment_jewels_survive_materialization_tree_edits_and_save() {
    init();
    let decoded = decode(EQUIPMENT_JEWEL_CODE);
    let jewels = &decoded["items"]["equipment_jewels"];
    assert_eq!(jewels.as_array().unwrap().len(), 1);
    assert_eq!(jewels[0]["slot"], "Gloves Jewel Socket 1");
    assert!(jewels[0]["text"].as_str().unwrap().contains("Kraken Eye"));
    let original = materialized(&decoded);
    assert_eq!(
        calculate(&original),
        calculate(&json!({
            "pob_code": EQUIPMENT_JEWEL_CODE.trim(), "full_dps": original["full_dps"], "breakdowns": []
        }))
    );
    for with_base in [false, true] {
        let mut request = original.clone();
        if with_base {
            request["base_code"] = json!(EQUIPMENT_JEWEL_CODE.trim());
        }
        let reopened = assert_roundtrip(&request);
        let restored = &reopened["items"]["equipment_jewels"];
        assert_eq!(restored.as_array().unwrap().len(), 1);
        assert_eq!(restored[0]["slot"], jewels[0]["slot"]);
        // XML pretty-printing adds outer whitespace; the item's text is unchanged.
        assert_eq!(
            restored[0]["text"].as_str().unwrap().trim(),
            jewels[0]["text"].as_str().unwrap().trim()
        );
        request["equipment_jewels"] = json!([]);
        assert_eq!(
            assert_roundtrip(&request)["items"]["equipment_jewels"],
            json!([])
        );
        assert_ne!(calculate(&request)["stats"], calculate(&original)["stats"]);
    }
    // Reapplying a tree-jewel override to a parsed build must retain the
    // equipment channel, including when all tree jewels are removed.
    let mut no_tree = original;
    no_tree["jewels"] = json!([]);
    assert_eq!(
        calculate(&no_tree),
        calculate(&json!({
            "pob_code": EQUIPMENT_JEWEL_CODE.trim(), "jewels": [],
            "full_dps": no_tree["full_dps"], "breakdowns": []
        }))
    );
    for slot in ["Jewel 1", "Gloves", "Flask 1"] {
        no_tree["equipment_jewels"] = json!([{ "slot": slot, "text": jewels[0]["text"] }]);
        assert!(pobr_wasm::calculate_build_json(&no_tree.to_string()).is_err());
        assert!(pobr_wasm::encode_build_json(&no_tree.to_string()).is_err());
    }
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
