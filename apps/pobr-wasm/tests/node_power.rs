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
    assert!(response["base"].as_f64().unwrap() > 0.0, "基线 Life 应为正");
    let entries = response["entries"].as_array().expect("entries");
    assert!(!entries.is_empty(), "深度 2 内应有候选节点");
    assert!(
        entries.iter().all(|e| e["distance"].as_u64().unwrap() <= 2),
        "全部候选应在深度门控内"
    );
    assert!(
        entries.iter().any(|e| e["delta"].as_f64().unwrap() > 0.0),
        "前沿附近应存在提升 Life 的节点"
    );
}
