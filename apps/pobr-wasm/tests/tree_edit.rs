use pobr_gamedata::{GameData, current_data_dir};
use serde_json::{Value, json};
fn setup() -> u32 {
    if !pobr_wasm::is_data_ready() {
        pobr_wasm::init_data_from_dir(current_data_dir().to_str().unwrap()).unwrap();
    }
    let data = pobr_build::BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let root_id = &data.passive_jewels.class_starts["warrior"];
    let root = data
        .passive_nodes
        .values()
        .find(|n| &n.id == root_id)
        .unwrap();
    let mut adjacent: Vec<_> = data
        .passive_nodes
        .values()
        .filter(|n| {
            n.ascendancy_id.is_none()
                && (n.connections.contains(&root.skill) || root.connections.contains(&n.skill))
        })
        .map(|n| n.skill)
        .collect();
    adjacent.sort();
    adjacent[0]
}
fn request() -> Value {
    json!({"character":{"level":80,"class_name":"Warrior","ascendancy_name":"None"},"allocated_nodes":[],"weapon_swap":{"active":1,"alternate_items":[],"exclusive_nodes":[[],[]]}})
}
fn preview(request: Value, node: u32, mode: u8, action: &str) -> Value {
    serde_json::from_str(
        &pobr_wasm::preview_tree_edit_json(
            &json!({"request":request,"node":node,"mode":mode,"action":action}).to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn set_two_round_trips_while_inactive_and_refunds_only_its_branch() {
    let node = setup();
    let mut req = request();
    let edit = preview(req.clone(), node, 2, "allocate");
    assert_eq!(edit["valid"], true, "{edit}");
    let mut keys: Vec<_> = edit
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["reason", "result", "valid"]);
    assert_eq!(
        edit["result"]["allocation"]["exclusive"],
        json!([[], [node]])
    );
    req["weapon_swap"]["exclusive_nodes"] = edit["result"]["allocation"]["exclusive"].clone();
    let code = pobr_wasm::encode_build_json(&req.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(
        decoded["weapon_swap"]["exclusive_nodes"],
        json!([[], [node]])
    );
    assert_eq!(decoded["tree"]["allocated_nodes"], json!([]));
    req["weapon_swap"]["active"] = json!(2);
    req["allocated_nodes"] = json!([node]);
    let refund = preview(req, node, 2, "refund");
    assert_eq!(refund["valid"], true, "{refund}");
    assert_eq!(refund["result"]["removed"], json!([node]));
    assert_eq!(refund["result"]["allocation"]["exclusive"], json!([[], []]));
}
#[test]
fn budget_and_bad_materialized_set_are_rejected() {
    let node = setup();
    let mut req = request();
    req["character"]["level"] = json!(1);
    let result: Value = serde_json::from_str(
        &pobr_wasm::preview_tree_edit_json(
            &json!({"request":req,"node":node,"mode":2,"action":"allocate","quest_points":0})
                .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result["valid"], false);
    assert_eq!(result["reason"], "passive_point_budget");
    let mut req = request();
    req["weapon_swap"]["exclusive_nodes"] = json!([[node], []]);
    assert!(
        pobr_wasm::preview_tree_edit_json(
            &json!({"request":req,"node":node,"mode":1,"action":"refund"}).to_string()
        )
        .is_err()
    );
}
