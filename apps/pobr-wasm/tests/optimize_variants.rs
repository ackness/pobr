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

fn assert_variant_matches_final(base: &Value, variant: Value, final_request: &Value) -> Value {
    let output: Value = serde_json::from_str(
        &pobr_wasm::optimize_variants_json(
            &json!({
                "request": base,
                "stats": ["Life", "Mana"],
                "variants": [variant],
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let row = &output["variants"][0];
    assert!(row["error"].is_null(), "{output}");
    let final_calc: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&final_request.to_string()).unwrap())
            .unwrap();
    for stat in ["Life", "Mana"] {
        let expected = final_calc["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["id"] == stat)
            .unwrap();
        assert_eq!(row["stats"][stat], expected["value"], "{stat}: {output}");
    }
    assert_eq!(row["unsupported"], final_calc["unsupported_modifiers"]);
    row.clone()
}

#[test]
fn socket_jewel_variants_use_the_final_allocation() {
    setup();
    let radius = "Rarity: RARE\nRadius Test\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\n+50 to maximum Life\nSmall Passive Skills in Radius also grant +10 to maximum Mana";
    let upgraded = radius.replace("+50 to maximum Life", "+80 to maximum Life");
    let inactive = json!({
        "character": {"class_name": "Witch", "level": 80},
        "allocated_nodes": [9884],
        "jewels": [{"socket_node": 7960, "text": radius}],
    });
    let mut activated = inactive.clone();
    activated["allocated_nodes"] = json!([9884, 7960]);
    let gained =
        assert_variant_matches_final(&inactive, json!({"allocate_nodes": [7960]}), &activated);
    assert!(gained["stats"]["Life"].as_f64().unwrap() > 0.0);

    let mut replaced = activated.clone();
    replaced["jewels"] = json!([{"socket_node": 7960, "text": upgraded}]);
    let replacement = assert_variant_matches_final(
        &inactive,
        json!({
            "allocate_nodes": [7960], "jewels": replaced["jewels"],
        }),
        &replaced,
    );
    assert!(
        replacement["stats"]["Life"].as_f64().unwrap() > gained["stats"]["Life"].as_f64().unwrap()
    );

    let mut refunded = activated.clone();
    refunded["allocated_nodes"] = json!([9884]);
    let lost =
        assert_variant_matches_final(&activated, json!({"deallocate_nodes": [7960]}), &refunded);
    assert!(lost["stats"]["Life"].as_f64().unwrap() < gained["stats"]["Life"].as_f64().unwrap());
    assert!(lost["stats"]["Mana"].as_f64().unwrap() < gained["stats"]["Mana"].as_f64().unwrap());

    let mut removed = activated.clone();
    removed["jewels"] = json!([]);
    let taken_out = assert_variant_matches_final(&activated, json!({"jewels": []}), &removed);
    assert_eq!(taken_out["stats"], lost["stats"]);

    let plain = json!({
        "character": {"class_name": "Witch", "level": 80},
        "allocated_nodes": [7960],
        "jewels": [{"socket_node": 7960,
            "text": "Rarity: RARE\nPlain Jewel\nEmerald\nImplicits: 0\n+70 to maximum Life"}],
    });
    let mut plain_refunded = plain.clone();
    plain_refunded["allocated_nodes"] = json!([]);
    let plain_lost =
        assert_variant_matches_final(&plain, json!({"deallocate_nodes": [7960]}), &plain_refunded);
    let plain_base: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&plain.to_string()).unwrap())
            .unwrap();
    let plain_base_life = plain_base["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["id"] == "Life")
        .unwrap()["value"]
        .as_f64()
        .unwrap();
    assert!(plain_lost["stats"]["Life"].as_f64().unwrap() < plain_base_life);
}

#[test]
fn repeated_node_in_one_variant_is_allocated_once() {
    setup();
    let base = json!({
        "character": {"class_name": "Witch", "level": 80},
        "allocated_nodes": [],
    });
    let mut final_request = base.clone();
    final_request["allocated_nodes"] = json!([57110]);
    assert_variant_matches_final(
        &base,
        json!({"allocate_nodes": [57110, 57110]}),
        &final_request,
    );
}

#[test]
fn imported_socket_edits_preserve_itemset_jewels_and_shared_item_ids() {
    setup();
    for shared in [false, true] {
        for initially_allocated in [false, true] {
            let nodes = if initially_allocated {
                "7960,9884"
            } else {
                "9884"
            };
            let shared_slot = if shared {
                "<Slot name=\"Jewel 1\" itemId=\"1\"/>"
            } else {
                ""
            };
            let xml = format!(
                r#"<PathOfBuilding2><Build level="80" className="Witch"/>
                <Tree activeSpec="1"><Spec nodes="{nodes}"><Sockets>
                <Socket nodeId="7960" itemId="1"/></Sockets></Spec></Tree>
                <Items activeItemSet="1">
                <Item id="1">Rarity: RARE
Radius Test
Time-Lost Ruby
Radius: Small
Implicits: 0
+50 to maximum Life
Small Passive Skills in Radius also grant +10 to maximum Mana</Item>
                <Item id="2">Rarity: MAGIC
Ruby
Implicits: 0
+40 to maximum Life</Item>
                <ItemSet id="1">{shared_slot}<Slot name="Jewel 2" itemId="2"/></ItemSet>
                </Items></PathOfBuilding2>"#
            );
            let code = pobr_build::encode_pob_code(&xml).unwrap();
            let base = json!({"pob_code": code});
            let mut final_request = base.clone();
            let variant = if initially_allocated {
                final_request["allocated_nodes"] = json!([9884]);
                json!({"deallocate_nodes": [7960]})
            } else {
                final_request["allocated_nodes"] = json!([9884, 7960]);
                json!({"allocate_nodes": [7960]})
            };
            let row = assert_variant_matches_final(&base, variant, &final_request);
            let base_calc: Value =
                serde_json::from_str(&pobr_wasm::calculate_build_json(&base.to_string()).unwrap())
                    .unwrap();
            let base_mana = base_calc["stats"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["id"] == "Mana")
                .unwrap()["value"]
                .as_f64()
                .unwrap();
            let final_mana = row["stats"]["Mana"].as_f64().unwrap();
            assert_eq!(
                final_mana > base_mana,
                !initially_allocated,
                "shared={shared}"
            );
            let base_life = base_calc["stats"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["id"] == "Life")
                .unwrap()["value"]
                .as_f64()
                .unwrap();
            let final_life = row["stats"]["Life"].as_f64().unwrap();
            if shared {
                assert_eq!(final_life, base_life, "shared item ID must remain active");
            } else {
                assert_eq!(final_life > base_life, !initially_allocated);
            }
        }
    }
}
