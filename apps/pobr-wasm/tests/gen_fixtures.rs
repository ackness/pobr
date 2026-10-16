//! Frontend mock fixture generator (`#[ignore]`, run manually):
//! `cargo test -p pobr-wasm --test gen_fixtures -- --ignored`
//!
//! Produces `web/src/fixtures/*.json` from the real contract entry points,
//! guaranteeing the mock backend's shape has zero drift from the real wasm
//! backend. Rerun once and commit the output after any contract change.

use pobr_gamedata::repo_data_root;

#[path = "support/utility.rs"]
mod utility_fixture;

fn demo_code() -> String {
    let path =
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt");
    std::fs::read_to_string(path).expect("read demo code")
}

fn fixtures_dir() -> std::path::PathBuf {
    repo_data_root().join("../web/src/fixtures")
}

#[test]
#[ignore = "manual Calcs fixture regeneration"]
fn generate_stat_detail_fixtures() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let descriptors = pobr_build::breakdown::detail::detail_descriptors();
    let ids: Vec<_> = descriptors.keys().collect();
    let request = serde_json::json!({"pob_code": demo_code(), "breakdowns": ids, "full_dps": true});
    let response: serde_json::Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    let mut inputs: std::collections::BTreeSet<String> = descriptors
        .keys()
        .flat_map(|id| pobr_core::calc::session::detail::source_names(id))
        .collect();
    let fixtures = repo_data_root().join("../examples/demo-bd-test/builds");
    let mut responses = vec![
        response.clone(),
        serde_json::from_str(
            &pobr_wasm::calculate_build_json(
                &serde_json::json!({"pob_code": utility_fixture::code(), "breakdowns": ids})
                    .to_string(),
            )
            .unwrap(),
        )
        .unwrap(),
    ];
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let code = entry.unwrap().path().join("code.txt");
        if !code.is_file() {
            continue;
        }
        let request = serde_json::json!({"pob_code": std::fs::read_to_string(code).unwrap(), "breakdowns": ids, "full_dps": true});
        responses.push(
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
                .unwrap(),
        );
    }
    for response in &responses {
        for detail in response["breakdowns"].as_object().unwrap().values() {
            for key in ["inputs", "context"] {
                for input in detail["derived"][key].as_array().into_iter().flatten() {
                    inputs.insert(input["id"].as_str().unwrap().into());
                }
            }
        }
    }
    let out = fixtures_dir();
    std::fs::write(
        out.join("stat_details.json"),
        serde_json::to_string_pretty(&response["breakdowns"]).unwrap() + "\n",
    )
    .unwrap();
    std::fs::write(
        out.join("detail_inputs.json"),
        serde_json::to_string_pretty(&inputs).unwrap() + "\n",
    )
    .unwrap();
    let calculate =
        pobr_wasm::calculate_build_json(&serde_json::json!({"pob_code": demo_code()}).to_string())
            .unwrap();
    std::fs::write(out.join("calculate.json"), pretty(&calculate)).unwrap();
}

#[test]
#[ignore = "manual fixture regeneration entry point, not part of regular tests"]
fn generate_web_fixtures() {
    let dir = pobr_gamedata::current_data_dir();
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    let out = fixtures_dir();
    std::fs::create_dir_all(&out).expect("mkdir fixtures");

    let code = demo_code();
    let decode = pobr_wasm::decode_build_json(&code).expect("decode");
    std::fs::write(out.join("decode.json"), pretty(&decode)).unwrap();

    let calc_req = serde_json::json!({ "pob_code": code }).to_string();
    let calculate = pobr_wasm::calculate_build_json(&calc_req).expect("calculate");
    std::fs::write(out.join("calculate.json"), pretty(&calculate)).unwrap();
    generate_stat_detail_fixtures();

    let attr_req = serde_json::json!({
        "request": { "pob_code": code },
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
    let minions = pobr_wasm::minion_catalog_json().expect("minion catalog");
    std::fs::write(out.join("minion_catalog.json"), pretty(&minions)).unwrap();

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
