//! Explicit charge counts through the real native host JSON/WASM contract.
//! Synthetic XML and ring text are fixture inputs, not mocked game data or calculations.

use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Charge {
    kind: &'static str,
    use_key: &'static str,
    override_key: &'static str,
    legacy_key: &'static str,
    current_stat: &'static str,
    maximum_stat: &'static str,
}

const CHARGES: &[Charge] = &[
    Charge {
        kind: "Power",
        use_key: "usePowerCharges",
        override_key: "overridePowerCharges",
        legacy_key: "multiplierPowerCharge",
        current_stat: "ChargePowerCurrent",
        maximum_stat: "ChargePowerMaximum",
    },
    Charge {
        kind: "Frenzy",
        use_key: "useFrenzyCharges",
        override_key: "overrideFrenzyCharges",
        legacy_key: "multiplierFrenzyCharge",
        current_stat: "ChargeFrenzyCurrent",
        maximum_stat: "ChargeFrenzyMaximum",
    },
    Charge {
        kind: "Endurance",
        use_key: "useEnduranceCharges",
        override_key: "overrideEnduranceCharges",
        legacy_key: "multiplierEnduranceCharge",
        current_stat: "ChargeEnduranceCurrent",
        maximum_stat: "ChargeEnduranceMaximum",
    },
];

fn init() {
    pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy()).unwrap();
}

fn fixture_code(
    charge: Charge,
    enabled: bool,
    input: Option<(&str, f64)>,
    modifier: &str,
) -> String {
    let input = input
        .map(|(key, value)| format!(r#"<Input name="{key}" number="{value}"/>"#))
        .unwrap_or_default();
    let use_key = charge.use_key;
    pobr_build::encode_pob_code(&format!(
        r#"<PathOfBuilding2>
          <Build level="85" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
          <Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
          <Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true">
            <Gem skillId="FireballPlayer" gemId="Metadata/Items/Gems/SkillGemFireball" level="16" quality="0" enabled="true"/>
          </Skill></SkillSet></Skills>
          <Items activeItemSet="1"><Item id="1">Rarity: RARE
Synthetic Charge Consumer
Sapphire Ring
Implicits: 0
{modifier}</Item><ItemSet id="1"><Slot name="Ring 1" itemId="1"/></ItemSet></Items>
          <Config><Input name="{use_key}" boolean="{enabled}"/>{input}
            <Input name="enemyIsBoss" string="None"/>
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

fn per_charge_modifier(charge: Charge) -> String {
    format!("8% increased Damage per {} Charge", charge.kind)
}

fn code(charge: Charge, enabled: bool, count: Option<u32>) -> String {
    fixture_code(
        charge,
        enabled,
        count.map(|count| (charge.override_key, f64::from(count))),
        &per_charge_modifier(charge),
    )
}

fn source(code: &str) -> Value {
    json!({"pob_code": code, "enemy_tier": "none"})
}

fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}

fn materialized(decoded: &Value) -> Value {
    json!({
        "character": decoded["character"],
        "tree_version": decoded["tree"]["tree_version"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": decoded["socket_groups"].as_array().unwrap().iter().map(|group| json!({
            "enabled": group["enabled"], "gems": group["gems"],
        })).collect::<Vec<_>>(),
        "main_socket_group": decoded["main_socket_group"],
        "items": decoded["items"]["equipped"],
        "flasks": [], "jewels": [],
        "config_inputs": decoded["config_inputs"],
        "enemy_tier": "none",
    })
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

fn assert_counts(response: &Value, charge: Charge, count: u32) {
    for sibling in CHARGES {
        let expected = if sibling.kind == charge.kind {
            count
        } else {
            0
        };
        assert_eq!(
            stat(response, sibling.current_stat),
            f64::from(expected),
            "{} count {count}: {} current count",
            charge.kind,
            sibling.kind
        );
        assert_eq!(stat(response, sibling.maximum_stat), 3.0);
    }
    assert_eq!(response["item_errors"], json!([]));
    assert!(
        !response["unsupported_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str() == Some(per_charge_modifier(charge).as_str())),
        "{}: the synthetic per-charge item must be modeled",
        charge.kind
    );
}

// This control bypasses overrideXCharges and its interpreter effect entirely:
// XML's existing multiplierXCharge lane sets the already-supported cfg multiplier.
// The identical ring tests the same real consumer at an independently fixed count.
fn independently_counted(charge: Charge, count: f64) -> Value {
    let control = fixture_code(
        charge,
        true,
        Some((charge.legacy_key, count)),
        &per_charge_modifier(charge),
    );
    calculate(&source(&control))
}

fn supported_counts(charge: Charge) {
    init();
    let full_code = code(charge, true, None);
    let full = calculate(&source(&full_code));
    assert_counts(&full, charge, 3);
    assert_eq!(full, calculate(&materialized(&decode(&full_code))));
    let off = calculate(&source(&code(charge, false, None)));
    assert_counts(&off, charge, 0);
    assert!(stat(&full, "TotalDPS") > stat(&off, "TotalDPS"));

    for count in [1, 2] {
        let control = independently_counted(charge, f64::from(count));
        assert_counts(&control, charge, count);
        let on_code = code(charge, true, Some(count));
        let actual = calculate(&source(&on_code));
        assert_counts(&actual, charge, count);
        assert_eq!(
            actual, control,
            "{}: count {count} source consumer",
            charge.kind
        );
        assert_eq!(
            calculate(&materialized(&decode(&on_code))),
            control,
            "{}: count {count} direct consumer",
            charge.kind
        );

        // Independently expand the known item modifier, not an inherent charge bonus.
        let flat_code = fixture_code(
            charge,
            true,
            Some((charge.legacy_key, f64::from(count))),
            &format!("{}% increased Damage", 8 * count),
        );
        let flat = calculate(&source(&flat_code));
        assert_eq!(stat(&control, "TotalDPS"), stat(&flat, "TotalDPS"));
        assert!(stat(&actual, "TotalDPS") > stat(&off, "TotalDPS"));
        assert!(stat(&actual, "TotalDPS") < stat(&full, "TotalDPS"));

        let off_code = code(charge, false, Some(count));
        assert_eq!(calculate(&source(&off_code)), off);
        assert_eq!(calculate(&materialized(&decode(&off_code))), off);
    }
}

#[test]
fn power_explicit_counts_reach_reported_state_and_existing_item_consumer() {
    supported_counts(CHARGES[0]);
}

#[test]
fn frenzy_explicit_counts_reach_reported_state_and_existing_item_consumer() {
    supported_counts(CHARGES[1]);
}

#[test]
fn endurance_explicit_counts_reach_reported_state_and_existing_item_consumer() {
    supported_counts(CHARGES[2]);
}

fn assert_export(request: &Value, original_code: &str, expected: &Value, charge: Charge) {
    let mut export = request.clone();
    export["base_code"] = json!(original_code);
    let exported_code = pobr_wasm::encode_build_json(&export.to_string()).unwrap();
    let exported = decode(&exported_code);
    for (key, value) in request["config_inputs"].as_object().unwrap() {
        let roundtrip = &exported["config_inputs"][key];
        if value.is_number() {
            assert_eq!(
                roundtrip.as_f64(),
                value.as_f64(),
                "{}: exported numeric value of {key}",
                charge.kind
            );
        } else {
            assert_eq!(
                roundtrip, value,
                "{}: exported raw type/value of {key}",
                charge.kind
            );
        }
    }
    assert_eq!(
        exported["config_inputs"].get(charge.override_key).is_some(),
        request["config_inputs"].get(charge.override_key).is_some(),
        "{}: exported count presence",
        charge.kind
    );
    assert_eq!(calculate(&source(&exported_code)), *expected);
    assert_eq!(calculate(&materialized(&exported)), *expected);
}

#[test]
fn imported_count_edits_and_use_transitions_match_final_import_and_export() {
    init();
    for &charge in CHARGES {
        for imported_use in [false, true] {
            for imported_count in [None, Some(1), Some(2)] {
                let original_code = code(charge, imported_use, imported_count);
                let original = decode(&original_code);
                for enabled in [false, true] {
                    for count in [1, 2] {
                        let control = calculate(&source(&code(charge, enabled, Some(count))));
                        assert_counts(&control, charge, if enabled { count } else { 0 });
                        let source_edit = json!({
                            "pob_code": original_code, "enemy_tier": "none",
                            "config_inputs": {charge.use_key: enabled, charge.override_key: count},
                        });
                        assert_eq!(
                            calculate(&source_edit),
                            control,
                            "{}: source import {imported_count:?}/{imported_use}, edit {count}/{enabled}",
                            charge.kind
                        );
                        let mut direct = materialized(&original);
                        direct["config_inputs"][charge.use_key] = json!(enabled);
                        direct["config_inputs"][charge.override_key] = json!(count);
                        assert_eq!(
                            calculate(&direct),
                            control,
                            "{}: direct import {imported_count:?}/{imported_use}, edit {count}/{enabled}",
                            charge.kind
                        );
                        assert_export(&direct, &original_code, &control, charge);
                    }
                }
            }
        }
    }
}

#[test]
fn absent_count_empty_source_override_and_reset_keep_imported_semantics() {
    init();
    for &charge in CHARGES {
        for enabled in [false, true] {
            let absent_code = code(charge, enabled, None);
            let absent = decode(&absent_code);
            assert!(absent["config_inputs"].get(charge.override_key).is_none());
            let default = calculate(&source(&absent_code));
            assert_counts(&default, charge, if enabled { 3 } else { 0 });
            for count in [1, 2] {
                let original_code = code(charge, enabled, Some(count));
                let original = decode(&original_code);
                let baseline = calculate(&source(&original_code));
                assert_counts(&baseline, charge, if enabled { count } else { 0 });
                assert_eq!(
                    calculate(
                        &json!({"pob_code": original_code, "enemy_tier": "none", "config_inputs": {}})
                    ),
                    baseline,
                    "{}: empty source override preserves imported count",
                    charge.kind
                );
                let mut direct = materialized(&original);
                direct["config_inputs"][charge.override_key] = json!(3 - count);
                let edited = calculate(&direct);
                assert_counts(&edited, charge, if enabled { 3 - count } else { 0 });
                if enabled {
                    assert_ne!(edited, baseline, "{}: deliberate count edit", charge.kind);
                } else {
                    assert_eq!(
                        edited, baseline,
                        "{}: use-off suppresses count edit",
                        charge.kind
                    );
                }
                direct["config_inputs"] = original["config_inputs"].clone();
                assert_eq!(
                    calculate(&direct),
                    baseline,
                    "{}: imported reset",
                    charge.kind
                );
                assert_export(&direct, &original_code, &baseline, charge);

                // Removing a direct count is the absent/full control, not numeric zero.
                direct["config_inputs"]
                    .as_object_mut()
                    .unwrap()
                    .remove(charge.override_key);
                assert_eq!(
                    calculate(&direct),
                    default,
                    "{}: direct count removal",
                    charge.kind
                );
                assert_export(&direct, &original_code, &default, charge);
            }
        }
    }
}

#[test]
fn count_boundaries_preserve_reference_current_state_and_existing_multiplier_floor() {
    init();
    // Accepted coordinator contract msg_ca3da5bd175e: zero/unset selects max,
    // negative floors at minimum, fractions and above-max counts stay in output.
    // Ordinary per-charge Multiplier tags separately floor fractional values.
    for &charge in CHARGES {
        for (minimum, maximum, cases) in [
            (
                0,
                3,
                vec![(0.0, 3.0), (-1.0, 0.0), (1.75, 1.75), (5.0, 5.0)],
            ),
            (
                2,
                5,
                vec![(0.0, 5.0), (-1.0, 2.0), (1.0, 2.0), (1.75, 2.0), (7.0, 7.0)],
            ),
        ] {
            let bounds = format!(
                "+{minimum} to Minimum {} Charges\n+{} to Maximum {} Charges",
                charge.kind,
                maximum - 3,
                charge.kind
            );
            let item = format!("{}\n{bounds}", per_charge_modifier(charge));
            for (input, expected_on) in cases {
                for enabled in [false, true] {
                    let expected = if enabled {
                        expected_on
                    } else {
                        f64::from(minimum)
                    };
                    let boundary_code =
                        fixture_code(charge, enabled, Some((charge.override_key, input)), &item);
                    let response = calculate(&source(&boundary_code));
                    assert_eq!(
                        stat(&response, charge.current_stat),
                        expected,
                        "{}: override {input}, use {enabled}, min {minimum}, max {maximum}",
                        charge.kind
                    );
                    assert_eq!(stat(&response, charge.maximum_stat), f64::from(maximum));
                    let direct = materialized(&decode(&boundary_code));
                    assert_eq!(calculate(&direct), response);
                    assert_export(&direct, &boundary_code, &response, charge);

                    let flat = fixture_code(
                        charge,
                        enabled,
                        Some((charge.override_key, input)),
                        &format!("{}% increased Damage\n{bounds}", 8.0 * expected.floor()),
                    );
                    assert_eq!(
                        stat(&response, "TotalDPS"),
                        stat(&calculate(&source(&flat)), "TotalDPS"),
                        "{}: per-charge item consumes floored {expected}",
                        charge.kind
                    );
                    // Within the old legacy count bounds, the identical-item control
                    // bypasses the override interpreter, including fractional output.
                    if enabled && expected > 0.0 && expected <= f64::from(maximum) {
                        let legacy =
                            fixture_code(charge, true, Some((charge.legacy_key, expected)), &item);
                        assert_eq!(response, calculate(&source(&legacy)));
                    }
                }
            }
        }
    }
}

#[test]
fn count_bool_and_text_inputs_keep_existing_interpreter_and_raw_export_contract() {
    init();
    for &charge in CHARGES {
        for enabled in [false, true] {
            let original_code = code(charge, enabled, None);
            let original = decode(&original_code);
            for (raw, expected_on) in [
                (json!(true), 1),
                (json!(false), 3),
                (json!("2"), 3),
                (json!("raw & untouched"), 3),
            ] {
                let expected = if enabled { expected_on } else { 0 };
                let control = calculate(&source(&code(
                    charge,
                    enabled,
                    if expected_on == 1 { Some(1) } else { None },
                )));
                let source_edit = json!({
                    "pob_code": original_code, "enemy_tier": "none",
                    "config_inputs": {charge.override_key: raw},
                });
                let mut direct = materialized(&original);
                direct["config_inputs"][charge.override_key] = raw;
                assert_counts(&control, charge, expected);
                assert_eq!(calculate(&source_edit), control);
                assert_eq!(calculate(&direct), control);
                assert_export(&direct, &original_code, &control, charge);
            }
        }
    }
}
