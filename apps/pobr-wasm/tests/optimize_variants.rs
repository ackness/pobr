//! Integration test for generic variant evaluation (a real ninja build):
//! the gem/mod-text/equipment/node channels, plus single-variant error
//! isolation, plus the include_baseline toggle.

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

    // Candidate support gem: pick one from the catalog that's not already in the main group.
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
            // 0: the gem channel.
            { "label": "gem", "add_gems": { "group_index": group_index,
                "gems": [{ "skill_id": support_id, "level": 1, "quality": 0 }] } },
            // 1: the mod-text catch-all channel.
            { "label": "mod", "extra_modifiers": ["+100 to maximum Life"] },
            // 2: the equipment channel (unequip the helmet).
            { "label": "unequip", "set_items": [{ "slot": "helmet", "text": "" }] },
            // 3: invalid equipment text -> a single-variant error, without taking down the whole batch.
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
    // index/label echoed back.
    assert_eq!(variants[0]["index"], 0);
    assert_eq!(variants[1]["label"], "mod");
    // The mod-text channel: +100 base life must raise final Life (>100 when there's an inc multiplier).
    let mod_life = variants[1]["stats"]["Life"].as_f64().unwrap();
    assert!(
        mod_life >= base_life + 100.0,
        "expected Life to rise by >= 100: {base_life} -> {mod_life}"
    );
    // The gem and unequip channels produce normal numbers.
    assert!(variants[0]["stats"]["TotalDPS"].as_f64().unwrap() > 0.0);
    assert!(variants[2]["error"].is_null());
    // Error isolation: only variant 3 carries an error and empty stats.
    assert!(variants[3]["error"].as_str().unwrap().contains("helmet"));
    assert!(variants[3]["stats"].as_object().unwrap().is_empty());

    // include_baseline=false: skips one baseline calculation.
    let no_base = json!({
        "request": { "pob_code": code },
        "stats": ["Life"],
        "variants": [{ "label": "noop" }],
        "include_baseline": false,
    });
    let out = pobr_wasm::optimize_variants_json(&no_base.to_string()).expect("optimize");
    let resp: Value = serde_json::from_str(&out).unwrap();
    assert!(resp["baseline"].is_null());
    // An empty variant = recomputing the baseline, so Life should match the baseline.
    let noop_life = resp["variants"][0]["stats"]["Life"].as_f64().unwrap();
    assert!((noop_life - base_life).abs() < 0.01);
}

#[test]
fn trade_replacements_match_normal_edits_and_keep_socket_gating() {
    setup();
    let request = json!({
        "character": {"class_name": "Witch", "level": 90},
        "allocated_nodes": [7960],
        "config_inputs": {"questInterlude 2Khari CrossingMolten Shrine": false},
        "jewels": [{"socket_node": 7960, "text": "Rarity: RARE\nOld Jewel\nEmerald\n+20 to maximum Life"}],
    });
    let replacements = json!({
        "jewels": [{"socket_node": 7960, "text": "Rarity: RARE\nNew Jewel\nEmerald\n+80 to maximum Life"}],
        "flasks": [{"slot": "Charm 1", "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm"}],
        "socket_groups": [{"enabled": true, "gems": [{"skill_id": "FireballPlayer", "level": 10, "quality": 20}]}],
    });
    let mut edited = request.clone();
    edited
        .as_object_mut()
        .unwrap()
        .extend(replacements.as_object().unwrap().clone());
    let normal: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&edited.to_string()).unwrap())
            .unwrap();
    let output: Value = serde_json::from_str(&pobr_wasm::optimize_variants_json(&json!({
        "request": request, "stats": ["Life", "TotalDPS", "FireResist"],
        "variants": [replacements, {"jewels": [{"socket_node": 7960, "text": "???"}]},
            {"jewels": [{"socket_node": 999999, "text": "Rarity: RARE\nUnused\nEmerald\n+900 to maximum Life"}]}],
    }).to_string()).unwrap()).unwrap();
    assert!(output["variants"][0]["error"].is_null(), "{output}");
    for stat in ["Life", "TotalDPS", "FireResist"] {
        let expected = normal["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == stat)
            .unwrap();
        assert_eq!(
            output["variants"][0]["stats"][stat], expected["value"],
            "{stat}"
        );
    }
    assert!(
        output["variants"][0]["stats"]["Life"].as_f64().unwrap()
            > output["baseline"]["Life"].as_f64().unwrap()
    );
    assert!(output["variants"][1]["error"].is_string());
    assert!(
        output["variants"][2]["stats"]["Life"].as_f64().unwrap()
            < output["baseline"]["Life"].as_f64().unwrap()
    );
}
