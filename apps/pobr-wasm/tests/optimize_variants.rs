//! 通用变体评估集成测试（真实 ninja build）：宝石/词条/装备/节点四通道 +
//! 单变体错误隔离 + include_baseline 开关。

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

fn setup() -> String {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    let repo_root = repo_data_root();
    let repo_root = repo_root.parent().expect("repo root");
    std::fs::read_to_string(repo_root.join("examples/poe-ninja/2.txt"))
        .expect("ninja code")
        .trim()
        .to_string()
}

#[test]
fn evaluates_all_mutation_channels_and_isolates_errors() {
    let code = setup();
    let build: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("decode")).unwrap();
    let group_index = build["main_socket_group"].as_u64().unwrap_or(0) as usize;

    // 候选辅助：目录里挑一个不在主组内的。
    let in_group: Vec<String> = build["socket_groups"][group_index]["gems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["skill_id"].as_str().unwrap().to_string())
        .collect();
    let catalog: Value =
        serde_json::from_str(&pobr_wasm::gem_catalog_json().expect("catalog")).unwrap();
    let support_id = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["is_support"] == true && !in_group.contains(&e["skill_id"].as_str().unwrap().into())
        })
        .expect("a support outside the group")["skill_id"]
        .clone();

    let request = json!({
        "request": { "pob_code": code },
        "stats": ["TotalDPS", "Life", "ManaCost", "ActionRate"],
        "variants": [
            // 0: 宝石通道。
            { "label": "gem", "add_gems": { "group_index": group_index,
                "gems": [{ "skill_id": support_id, "level": 1, "quality": 0 }] } },
            // 1: 词条文本兜底通道。
            { "label": "mod", "extra_modifiers": ["+100 to maximum Life"] },
            // 2: 装备通道（摘下头盔）。
            { "label": "unequip", "set_items": [{ "slot": "helmet", "text": "" }] },
            // 3: 非法装备文本 → 单变体报错，不拖垮整批。
            { "label": "bad", "set_items": [{ "slot": "helmet", "text": "???" }] },
        ],
    });
    let out = pobr_wasm::optimize_variants_json(&request.to_string()).expect("optimize");
    let resp: Value = serde_json::from_str(&out).unwrap();

    let baseline = resp["baseline"].as_object().expect("baseline present");
    let base_life = baseline["Life"].as_f64().unwrap();
    assert!(base_life > 0.0);
    assert!(baseline["TotalDPS"].as_f64().unwrap() > 0.0);

    let variants = resp["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 4);
    // index/label 回显。
    assert_eq!(variants[0]["index"], 0);
    assert_eq!(variants[1]["label"], "mod");
    // 词条通道：+100 基础生命必然抬高最终 Life（有 inc 时 >100）。
    let mod_life = variants[1]["stats"]["Life"].as_f64().unwrap();
    assert!(
        mod_life >= base_life + 100.0,
        "expected Life to rise by >= 100: {base_life} -> {mod_life}"
    );
    // 宝石与摘装通道正常出数。
    assert!(variants[0]["stats"]["TotalDPS"].as_f64().unwrap() > 0.0);
    assert!(variants[2]["error"].is_null());
    // 错误隔离：只有 3 号带 error 且空 stats。
    assert!(variants[3]["error"].as_str().unwrap().contains("helmet"));
    assert!(variants[3]["stats"].as_object().unwrap().is_empty());

    // include_baseline=false：省一次基线计算。
    let no_base = json!({
        "request": { "pob_code": code },
        "stats": ["Life"],
        "variants": [{ "label": "noop" }],
        "include_baseline": false,
    });
    let out = pobr_wasm::optimize_variants_json(&no_base.to_string()).expect("optimize");
    let resp: Value = serde_json::from_str(&out).unwrap();
    assert!(resp["baseline"].is_null());
    // 空变体 = 基线复算，Life 应与基线一致。
    let noop_life = resp["variants"][0]["stats"]["Life"].as_f64().unwrap();
    assert!((noop_life - base_life).abs() < 0.01);
}
