//! Frontend mock fixture generator (`#[ignore]`, run manually):
//! `cargo test -p pobr-wasm --test gen_fixtures -- --ignored`
//!
//! Produces `web/src/fixtures/*.json` from the real contract entry points,
//! guaranteeing the mock backend's shape has zero drift from the real wasm
//! backend. Rerun once and commit the output after any contract change.

use pobr_gamedata::repo_data_root;

fn demo_code() -> String {
    let path =
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt");
    std::fs::read_to_string(path).expect("read demo code")
}

fn fixtures_dir() -> std::path::PathBuf {
    repo_data_root().join("../web/src/fixtures")
}

#[test]
#[ignore = "manual fixture regeneration entry point, not part of regular tests"]
fn generate_web_fixtures() {
    let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    let out = fixtures_dir();
    std::fs::create_dir_all(&out).expect("mkdir fixtures");

    let code = demo_code();
    let decode = pobr_wasm::decode_build_json(&code).expect("decode");
    std::fs::write(out.join("decode.json"), pretty(&decode)).unwrap();

    let calc_req = serde_json::json!({ "pob_code": code }).to_string();
    let calculate = pobr_wasm::calculate_build_json(&calc_req).expect("calculate");
    std::fs::write(out.join("calculate.json"), pretty(&calculate)).unwrap();

    let attr_req = serde_json::json!({
        "pob_code": code,
        "fields": ["TotalDPS", "Life", "EnergyShield", "TotalEHP"],
    })
    .to_string();
    let attribution = pobr_wasm::attribution_json(&attr_req).expect("attribution");
    std::fs::write(out.join("attribution.json"), pretty(&attribution)).unwrap();

    // Tree fixture: the full 1.6MB is too large, and mock rendering only
    // needs a subset — take the allocated nodes plus their neighbors.
    let decode_json: serde_json::Value = serde_json::from_str(&decode).unwrap();
    let allocated: std::collections::BTreeSet<u64> = decode_json["tree"]["allocated_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_u64())
        .collect();
    let tree_path = dir.join("base/passive_tree.json");
    let tree: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(tree_path).unwrap()).unwrap();
    let keep: Vec<&serde_json::Value> = tree
        .iter()
        .filter(|n| {
            let skill = n["skill"].as_u64().unwrap_or(0);
            allocated.contains(&skill)
                || n["connections"].as_array().is_some_and(|c| {
                    c.iter()
                        .any(|t| allocated.contains(&t.as_u64().unwrap_or(0)))
                })
        })
        .collect();
    std::fs::write(
        out.join("tree.json"),
        serde_json::to_string_pretty(&keep).unwrap(),
    )
    .unwrap();

    // The gem catalog (used by the manual skill-editing picker).
    let catalog = pobr_wasm::gem_catalog_json().expect("gem catalog");
    std::fs::write(out.join("gem_catalog.json"), pretty(&catalog)).unwrap();

    // The built-in config catalog (Config page): mock only needs a subset —
    // take the first 5 entries per section.
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("overlay/config_options.json")).unwrap(),
    )
    .unwrap();
    let mut per_section: std::collections::BTreeMap<String, usize> = Default::default();
    let slim: Vec<&serde_json::Value> = config["options"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| {
            let section = o["section"].as_str().unwrap_or("General").to_string();
            let count = per_section.entry(section).or_insert(0);
            *count += 1;
            *count <= 5
        })
        .collect();
    std::fs::write(
        out.join("config_options.json"),
        serde_json::to_string_pretty(&slim).unwrap(),
    )
    .unwrap();

    // Class/ascendancy metadata (used by the new-build picker): mirrors the data file directly.
    std::fs::copy(
        dir.join("base/passive_tree_meta.json"),
        out.join("tree_meta.json"),
    )
    .expect("copy tree meta");

    eprintln!("fixtures written to {}", out.display());
}

fn pretty(json: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    serde_json::to_string_pretty(&value).unwrap()
}
