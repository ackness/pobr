//! Integration test for the tree node power heatmap (a real ninja build):
//! BFS depth gating plus non-all-zero single-node trial-allocation deltas.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

#[test]
fn node_power_within_depth_and_finds_positive_nodes() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    let repo_root = repo_data_root();
    let repo_root = repo_root.parent().expect("repo root");
    let code = std::fs::read_to_string(repo_root.join("examples/poe-ninja/2.txt"))
        .expect("ninja code")
        .trim()
        .to_string();

    let request = json!({
        "request": { "pob_code": code },
        "power_stat": "Life",
        "max_depth": 2,
    });
    let out = pobr_wasm::node_power_json(&request.to_string()).expect("node power");
    let response: Value = serde_json::from_str(&out).expect("parse");
    assert!(
        response["base"].as_f64().unwrap() > 0.0,
        "baseline Life should be positive"
    );
    let entries = response["entries"].as_array().expect("entries");
    assert!(
        !entries.is_empty(),
        "there should be candidate nodes within depth 2"
    );
    assert!(
        entries.iter().all(|e| e["distance"].as_u64().unwrap() <= 2),
        "all candidates should be within the depth gate"
    );
    assert!(
        entries.iter().any(|e| e["delta"].as_f64().unwrap() > 0.0),
        "there should be a Life-raising node near the frontier"
    );
}

#[test]
fn identical_passive_text_keeps_node_specific_radius_grants() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).unwrap();
    let request = json!({
        "character": { "class_name": "Witch", "level": 80 },
        "allocated_nodes": [7960],
        "jewels": [{ "socket_node": 7960,
            "text": "Rarity: RARE\nRadius Test\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\nSmall Passive Skills in Radius also grant +10 to maximum Mana" }],
    });
    let output: Value = serde_json::from_str(
        &pobr_wasm::node_power_json(
            &json!({
                "request": request, "power_stat": "Mana", "max_depth": 5,
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let mana = |nodes: Vec<u32>| {
        let mut trial = request.clone();
        trial["allocated_nodes"] = json!(nodes);
        let result: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&trial.to_string()).unwrap())
                .unwrap();
        result["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stat| stat["id"] == "Mana")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let baseline = mana(vec![7960]);
    let mut deltas = Vec::new();
    // Both nodes grant 12% increased Fire Damage. Only the first is inside the radius.
    for node in [9884, 23091] {
        let delta = output["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["skill"] == node)
            .unwrap()["delta"]
            .as_f64()
            .unwrap();
        assert!((delta - (mana(vec![7960, node]) - baseline)).abs() < 1e-8);
        deltas.push(delta);
    }
    assert!(
        deltas[0] > deltas[1],
        "in-radius grant must distinguish identical node text"
    );
}
