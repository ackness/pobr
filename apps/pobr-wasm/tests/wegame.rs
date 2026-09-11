//! Synthetic WeGame Profile payloads. No account IDs, share tokens or player exports.
use pobr_gamedata::{data_version, repo_data_root};
use serde_json::{Value, json};

fn init() {
    let dir = repo_data_root().join(data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).unwrap();
}

fn groups(build: &Value) -> Vec<Value> {
    build["socket_groups"].as_array().unwrap().iter().map(|g| {
        json!({"enabled":g["enabled"],"slot":g["slot"],"source":g["source"],"gems":g["gems"]})
    }).collect()
}

fn fixture() -> Value {
    json!({
        "format":"wegame", "version":1,
        "role":{"level":59,"class_name":"Deadeye"},
        "equipments":[
            {"baseType":"Linen Belt","name":"Test Belt","frameType":2,"inventoryId":"Belt",
             "implicitMods":[{"description":"+20 生命上限"}],
             "explicitMods":[{"description":"[Resistances|火焰抗性] +33%"}],
             "runeMods":["[ShamanOnlyMods|羁绊]： +20 生命上限"]},
            {"baseType":"Twig Focus","name":"Test Focus","frameType":2,"inventoryId":"Offhand",
             "properties":[{"type":18,"values":[["100",1]]}],
             "explicitMods":["[EnergyShield|能量护盾]提高 50%"]},
            {"baseType":"Lesser Life Flask","frameType":0,"inventoryId":"Flask","x":0},
            {"baseType":"Crude Bow","frameType":0,"inventoryId":"Weapon2"}
        ],
        "talent_tree":{"hashes":[7960], "skill_overrides":{"7960":{"grantedDexterity":5}},
            "specialisations":{}, "quest_stats":["+20 生命上限", "+100 [Spirit|精魂]"]},
        "jewel_data":json!([{"socket_id":"jewel_slot1969","jewel":{
            "id":"Metadata/Items/Jewels/JewelDex", "name":"Emerald", "display_name":"Test Jewel", "rarity":2,
            "mod_descriptions":[{"values":["[Projectile|投射物]速度提高 4 (4-8)%"]}]
        }}]).to_string().replace('"',"\\\""),
        "skills":[{"baseType":"冰霜射击","support":false,
            "properties":[{"type":5,"values":[["12",0]]},{"type":6,"values":[["+20%",1]]}],
            "socketedItems":[{"baseType":"Unknown Test Support","support":true}]}]
    })
}

#[test]
fn wegame_preserves_calculation_inputs_and_roundtrips() {
    init();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_file_json(&fixture().to_string()).unwrap())
            .unwrap();
    assert_eq!(
        decoded["character"],
        json!({"level":59,"class_name":"Ranger","ascendancy_name":"Deadeye"})
    );
    assert_eq!(decoded["tree"]["attribute_choices"]["7960"], "dex");
    assert_eq!(decoded["items"]["equipped"].as_array().unwrap().len(), 2);
    assert_eq!(decoded["items"]["flasks"][0]["slot"], "Flask 1");
    assert_eq!(decoded["items"]["socket_jewels"][0]["socket_node"], 7960);
    let belt = decoded["items"]["equipped"][0]["text"].as_str().unwrap();
    assert!(belt.contains("+20 to maximum Life"));
    assert!(!belt.contains("羁绊"));
    let gem = &decoded["socket_groups"][0]["gems"][0];
    assert_eq!(gem["skill_id"], "IceShotPlayer");
    assert_eq!(gem["level"], 12);
    assert_eq!(gem["quality"], 20);
    let notes = decoded["notes"].as_str().unwrap();
    assert!(notes.contains("Unknown Test Support"));
    assert!(notes.contains("Weapon2"));
    assert_eq!(
        decoded["config_inputs"]["questAct 1Ogham ManorCandlemass"],
        false
    );
    assert_eq!(
        decoded["config_inputs"]["questWeGame"],
        "+20 to maximum Life\n+100 to Spirit"
    );
    let request = json!({
        "character":decoded["character"], "allocated_nodes":decoded["tree"]["allocated_nodes"],
        "items":decoded["items"]["equipped"], "flasks":decoded["items"]["flasks"],
        "jewels":decoded["items"]["socket_jewels"], "socket_groups":groups(&decoded),
        "config_inputs":decoded["config_inputs"]
    });
    let calculated: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap())
            .unwrap();
    assert!(
        calculated["stats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "Life" && s["value"].as_f64().unwrap() > 0.0)
    );
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let roundtrip: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_eq!(roundtrip["socket_groups"][0]["gems"][0], *gem);
}

#[test]
fn wegame_rejects_incomplete_or_unknown_payloads() {
    for input in [
        json!({"format":"wegame"}),
        json!({"format":"other"}),
        json!({"result":{"error_code":1},"format":"wegame"}),
    ] {
        assert!(pobr_wasm::decode_build_file_json(&input.to_string()).is_err());
    }
    init();
    let mut input = fixture();
    input["jewel_data"] = json!("not json");
    assert!(pobr_wasm::decode_build_file_json(&input.to_string()).is_err());
}

#[test]
#[ignore = "Local live-share smoke; set POBR_WEGAME_FILE to a fetched private bundle"]
fn wegame_live_bundle_calculates() {
    init();
    let input = std::fs::read_to_string(std::env::var("POBR_WEGAME_FILE").unwrap()).unwrap();
    let build: Value =
        serde_json::from_str(&pobr_wasm::decode_build_file_json(&input).unwrap()).unwrap();
    let main = build["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .position(|g| g["gems"][0]["skill_id"] == "IceShotPlayer")
        .unwrap_or(0);
    let request = json!({"character":build["character"], "allocated_nodes":build["tree"]["allocated_nodes"],
        "attribute_choices":build["tree"]["attribute_choices"], "items":build["items"]["equipped"],
        "jewels":build["items"]["socket_jewels"], "flasks":build["items"]["flasks"],
        "socket_groups":groups(&build), "main_socket_group":main, "config_inputs":build["config_inputs"]});
    let output = pobr_wasm::calculate_build_json(&request.to_string()).unwrap();
    let parent = std::path::PathBuf::from(std::env::var("POBR_WEGAME_FILE").unwrap());
    std::fs::write(parent.with_file_name("decoded.json"), build.to_string()).unwrap();
    std::fs::write(parent.with_file_name("calculated.json"), output).unwrap();
    let quiver = build["items"]["equipped"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["slot"] == "weapon2")
        .unwrap()["text"]
        .as_str()
        .unwrap();
    let variants = json!({"request":request,"stats":["TotalDPS","Life"],"variants":[
        {"label":"Current quiver","set_items":[{"slot":"weapon2","text":quiver}]},
        {"label":"Synthetic damage upgrade","set_items":[{"slot":"weapon2","text":format!("{quiver}\nAdds 20 to 40 Physical Damage to Attacks")}]}
    ]});
    let result: Value =
        serde_json::from_str(&pobr_wasm::optimize_variants_json(&variants.to_string()).unwrap())
            .unwrap();
    let base = result["baseline"]["TotalDPS"].as_f64().unwrap();
    assert!(base > 0.0);
    assert_eq!(
        result["variants"][0]["stats"]["TotalDPS"].as_f64().unwrap(),
        base
    );
    assert!(result["variants"][1]["stats"]["TotalDPS"].as_f64().unwrap() > base);
    std::fs::write(
        parent.with_file_name("quiver-comparison.json"),
        result.to_string(),
    )
    .unwrap();
}
