//! Imported condition precedence through the normal host JSON/WASM contract.
//! Synthetic XML is input only; all parsing, data and calculation are real.

use serde_json::{Value, json};

const CASES: &[(&str, &str, Option<&str>)] = &[
    ("usePowerCharges", "Fireball", Some("ChargePowerCurrent")),
    ("useFrenzyCharges", "Fireball", Some("ChargeFrenzyCurrent")),
    (
        "useEnduranceCharges",
        "Fireball",
        Some("ChargeEnduranceCurrent"),
    ),
    ("conditionEnemyFireExposure", "Fireball", None),
    ("conditionEnemyColdExposure", "Comet", None),
    ("conditionEnemyLightningExposure", "Spark", None),
];

fn init() {
    pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy()).unwrap();
}

fn code(key: &str, value: Option<bool>, skill: &str) -> String {
    let input = value
        .map(|value| format!(r#"<Input name="{key}" boolean="{value}"/>"#))
        .unwrap_or_default();
    pobr_build::encode_pob_code(&format!(
        r#"<PathOfBuilding2>
          <Build level="85" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
          <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
          <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true">
            <Gem skillId="{skill}Player" gemId="Metadata/Items/Gems/SkillGem{skill}" level="16" quality="0" enabled="true"/>
          </Skill></SkillSet></Skills>
          <Items activeItemSet="1"><ItemSet id="1"/></Items>
          <Config>{input}<Input name="enemyIsBoss" string="None"/>
            <Input name="syntheticFalse" boolean="false"/>
            <Input name="syntheticZero" number="0"/>
            <Input name="syntheticNumber" number="7.5"/>
            <Input name="syntheticString" string="0042 &amp; raw"/>
            <Input name="enemyFireResist" string="23"/>
          </Config>
        </PathOfBuilding2>"#
    ))
    .unwrap()
}

fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}

fn calculate(request: &Value) -> Value {
    serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap()).unwrap()
}

fn stat(response: &Value, id: &str) -> f64 {
    response["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stat| stat["id"] == id)
        .unwrap_or_else(|| panic!("missing calculated stat {id}"))["value"]
        .as_f64()
        .unwrap()
}

fn materialized(decoded: &Value) -> Value {
    json!({
        "character": decoded["character"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": decoded["socket_groups"].as_array().unwrap().iter().map(|group| json!({
            "enabled": group["enabled"], "gems": group["gems"],
        })).collect::<Vec<_>>(),
        "main_socket_group": decoded["main_socket_group"],
        "items": [], "flasks": [], "jewels": [],
        "config_inputs": decoded["config_inputs"],
        "enemy_tier": "none",
    })
}

#[test]
fn imported_condition_source_and_materialized_edits_match_final_import_both_directions() {
    init();
    for &(key, skill, charge_stat) in CASES {
        let off =
            calculate(&json!({"pob_code": code(key, Some(false), skill), "enemy_tier": "none"}));
        let on =
            calculate(&json!({"pob_code": code(key, Some(true), skill), "enemy_tier": "none"}));
        if let Some(id) = charge_stat {
            assert_eq!(stat(&off, id), 0.0, "{key}: disabled charges");
            assert_eq!(stat(&on, id), 3.0, "{key}: existing full-stack consumer");
        } else {
            assert!(
                stat(&on, "TotalDPS") > stat(&off, "TotalDPS"),
                "{key}: real exposure consumer"
            );
        }
        for imported in [false, true] {
            let original_code = code(key, Some(imported), skill);
            let original = decode(&original_code);
            for current in [!imported, imported] {
                let control_code = code(key, Some(current), skill);
                let control = calculate(&json!({"pob_code": control_code, "enemy_tier": "none"}));
                let source_edit = json!({
                    "pob_code": original_code, "enemy_tier": "none",
                    "config_inputs": {key: current},
                });
                assert_eq!(
                    calculate(&source_edit),
                    control,
                    "{key}: source import {imported}, edit {current}"
                );
                let mut direct_edit = materialized(&original);
                direct_edit["config_inputs"][key] = json!(current);
                assert_eq!(
                    calculate(&direct_edit),
                    control,
                    "{key}: materialized edit {current}"
                );
                let mut export = direct_edit;
                export["base_code"] = json!(original_code);
                let exported = pobr_wasm::encode_build_json(&export.to_string()).unwrap();
                let exported_inputs = decode(&exported)["config_inputs"].clone();
                for (raw_key, raw_value) in export["config_inputs"].as_object().unwrap() {
                    assert_eq!(
                        &exported_inputs[raw_key], raw_value,
                        "{key}: raw type/value of {raw_key}"
                    );
                }
                assert_eq!(
                    calculate(&json!({"pob_code": exported, "enemy_tier": "none"})),
                    control,
                    "{key}: exported final import"
                );
            }
        }
    }
}

#[test]
fn imported_condition_absent_override_and_reset_preserve_imported_and_raw_values() {
    init();
    for &(key, skill, _) in CASES {
        let absent = code(key, None, skill);
        let absent_request = json!({"pob_code": absent, "enemy_tier": "none"});
        assert_eq!(
            calculate(&absent_request),
            calculate(&json!({"pob_code": code(key, Some(false), skill), "enemy_tier": "none"})),
            "{key}: absent remains disabled"
        );
        assert!(decode(&absent)["config_inputs"].get(key).is_none());
        for current in [false, true] {
            let mut edited_absent = absent_request.clone();
            edited_absent["config_inputs"] = json!({key: current});
            assert_eq!(
                calculate(&edited_absent),
                calculate(
                    &json!({"pob_code": code(key, Some(current), skill), "enemy_tier": "none"})
                ),
                "{key}: explicit edit on an absent imported assumption"
            );
        }
        for imported in [false, true] {
            let original_code = code(key, Some(imported), skill);
            let original = decode(&original_code);
            let source = json!({"pob_code": original_code, "enemy_tier": "none"});
            let baseline = calculate(&source);
            let mut no_override = source.clone();
            no_override["config_inputs"] = json!({});
            assert_eq!(
                calculate(&no_override),
                baseline,
                "{key}: empty override is not false"
            );
            let mut direct = materialized(&original);
            direct["config_inputs"][key] = json!(!imported);
            let final_control = calculate(
                &json!({"pob_code": code(key, Some(!imported), skill), "enemy_tier": "none"}),
            );
            assert_eq!(calculate(&direct), final_control);
            direct["config_inputs"] = original["config_inputs"].clone();
            assert_eq!(
                calculate(&direct),
                baseline,
                "{key}: reset restores imported state"
            );
            direct["base_code"] = json!(original_code);
            let restored_code = pobr_wasm::encode_build_json(&direct.to_string()).unwrap();
            let restored_inputs = decode(&restored_code)["config_inputs"].clone();
            for (raw_key, raw_value) in original["config_inputs"].as_object().unwrap() {
                assert_eq!(
                    &restored_inputs[raw_key], raw_value,
                    "{key}: reset/export raw type/value of {raw_key}"
                );
            }
            assert_eq!(
                calculate(&json!({"pob_code": restored_code, "enemy_tier": "none"})),
                baseline
            );
        }
    }
}
