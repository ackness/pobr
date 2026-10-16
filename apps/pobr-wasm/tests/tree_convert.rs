use pobr_build::{decode_pob_code, encode_pob_code};
use pobr_gamedata::current_data_dir;
use serde_json::{Value, json};
const SYNTHETIC: &str =
    include_str!("../../../crates/pobr-build/tests/fixtures/synthetic-tree-0_4.xml");
fn setup() {
    if !pobr_wasm::is_data_ready() {
        pobr_wasm::init_data_from_dir(current_data_dir().to_str().unwrap()).unwrap();
    }
}
fn preview(code: &str, all: bool) -> Value {
    serde_json::from_str(
        &pobr_wasm::preview_tree_conversion_json(&json!({"code":code,"all_specs":all}).to_string())
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn synthetic_historical_preview_and_export_keep_jewel_and_both_weapon_sets() {
    setup();
    let code = encode_pob_code(SYNTHETIC).unwrap();
    let one = preview(&code, false);
    assert_eq!(one["specs"].as_array().unwrap().len(), 1);
    assert_eq!(
        one["specs"][0]["disconnected_by_set"],
        json!([[], [], [56118]])
    );
    assert_eq!(one["specs"][0]["dropped"][0]["id"], 8194);
    assert_eq!(decode_pob_code(&code).unwrap(), SYNTHETIC);
    let all = preview(&code, true);
    assert_eq!(all["specs"].as_array().unwrap().len(), 2);
    let converted = all["code"].as_str().unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(converted).unwrap()).unwrap();
    assert_eq!(decoded["tree"]["tree_version"], "0_5");
    assert_eq!(
        decoded["weapon_swap"]["exclusive_nodes"],
        json!([[], [56118]])
    );
    assert_eq!(decoded["items"]["socket_jewels"][0]["socket_node"], 60735);
    let jewels: Vec<_> = decoded["items"]["socket_jewels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| json!({"socket_node": j["socket_node"], "text":j["text"]}))
        .collect();
    let exported = pobr_wasm::encode_build_json(&json!({
        "base_code":converted, "character":decoded["character"],
        "tree_version":decoded["tree"]["tree_version"], "allocated_nodes":decoded["tree"]["allocated_nodes"],
        "attribute_choices":decoded["tree"]["attribute_choices"], "weapon_swap":decoded["weapon_swap"], "jewels":jewels,
    }).to_string()).unwrap();
    let xml = decode_pob_code(&exported).unwrap();
    assert!(!xml.contains("treeVersion=\"0_4\""));
    assert!(xml.contains("56118"));
    let round: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&exported).unwrap()).unwrap();
    for key in ["tree", "weapon_swap"] {
        assert_eq!(round[key], decoded[key], "{key}");
    }
    assert_eq!(round["items"]["socket_jewels"][0]["socket_node"], 60735);
    assert_eq!(
        round["items"]["socket_jewels"][0]["text"]
            .as_str()
            .unwrap()
            .trim(),
        decoded["items"]["socket_jewels"][0]["text"]
            .as_str()
            .unwrap()
            .trim()
    );
    assert!(
        pobr_wasm::preview_tree_conversion_json(r#"{"code":"bad","unexpected":true}"#).is_err()
    );
}

#[test]
fn current_real_corpus_build_conversion_preserves_allocations_and_jewels() {
    setup();
    let code = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt").trim();
    let before: Value = serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap();
    let result = preview(code, true);
    assert!(
        result["specs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["dropped"].as_array().unwrap().is_empty())
    );
    assert_eq!(
        decode_pob_code(result["code"].as_str().unwrap()).unwrap(),
        decode_pob_code(code).unwrap()
    );
    let after: Value = serde_json::from_str(
        &pobr_wasm::decode_build_json(result["code"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    for key in [
        "tree",
        "weapon_swap",
        "items",
        "socket_groups",
        "config_inputs",
    ] {
        assert_eq!(before[key], after[key], "{key}");
    }
}
