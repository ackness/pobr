//! 前端 mock fixture 生成器（`#[ignore]`，手动运行）：
//! `cargo test -p pobr-wasm --test gen_fixtures -- --ignored`
//!
//! 用真实契约入口产出 `web/src/fixtures/*.json`，保证 mock 后端形状与
//! 真实 wasm 后端零漂移。契约变更后重跑一次并提交产物。

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
#[ignore = "手动 fixture 再生成入口，不进常规测试"]
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

    // 树 fixture：全量 1.6MB 过大，mock 只需渲染子集——取已加点节点 + 其邻接。
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

    // 宝石目录（手动技能编辑选择器）。
    let catalog = pobr_wasm::gem_catalog_json().expect("gem catalog");
    std::fs::write(out.join("gem_catalog.json"), pretty(&catalog)).unwrap();

    // 职业/升华元数据（新建 build 选择器用）：直接镜像数据文件。
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
