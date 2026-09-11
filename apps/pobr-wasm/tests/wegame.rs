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
    assert!(!notes.contains("Equipment slot omitted: Weapon2"));
    assert_eq!(
        decoded["weapon_swap"]["alternate_items"][0]["slot"],
        "weapon1"
    );
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
        "config_inputs":decoded["config_inputs"], "weapon_swap":decoded["weapon_swap"]
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
    assert_swap(&roundtrip["weapon_swap"], &decoded["weapon_swap"]);
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

#[test]
fn trade_item_objects_preserve_rolls_and_isolate_bad_entries() {
    init();
    let input = json!([
        {"name": "Synthetic Ward", "baseType": "Linen Belt", "frameType": 2, "ilvl": 82,
         "implicitMods": [{"description": "+20 to maximum Life"}],
         "explicitMods": [{"description": "+100 to maximum [Life|Life]"}, {"description": "+30% to Fire Resistance"}],
         "properties": [{"type": 6, "values": [["+20%", 1]]}], "corrupted": true},
        {"frameType": 2}
    ]);
    let result: Value =
        serde_json::from_str(&pobr_wasm::import_trade_items_json(&input.to_string()).unwrap())
            .unwrap();
    let text = result[0]["text"].as_str().unwrap();
    for expected in [
        "Linen Belt",
        "Item Level: 82",
        "Implicits: 1",
        "+100 to maximum Life",
        "+30% to Fire Resistance",
        "Quality: 20",
        "Corrupted",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(result[1]["error"].is_string());
}

#[test]
fn weapon_sets_roundtrip_both_pairs_passives_and_skill_bindings() {
    init();
    let request = json!({
        "character": {"level":80,"class_name":"Witch"},
        "allocated_nodes": [100, 222],
        "items": [{"slot":"weapon1","text":"Rarity: NORMAL\nAshen Staff"}, {"slot":"ring1","text":"Rarity: NORMAL\nSapphire Ring"}],
        "weapon_swap": {"active":2, "alternate_items":[
            {"slot":"weapon1","text":"Rarity: NORMAL\nCrude Bow"},
            {"slot":"weapon2","text":"Rarity: NORMAL\nPrimed Quiver"}
        ], "exclusive_nodes":[[111],[222]]},
        "socket_groups":[
            {"weapon_set":1,"enabled":true,"gems":[{"skill_id":"IceShotPlayer","level":12}]},
            {"weapon_set":2,"enabled":true,"gems":[{"skill_id":"FireballPlayer","level":12}]}
        ], "main_socket_group":1
    });
    let code = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
    assert_swap(&decoded["weapon_swap"], &request["weapon_swap"]);
    assert_eq!(decoded["tree"]["allocated_nodes"], json!([100, 222]));
    assert_eq!(decoded["socket_groups"][0]["weapon_set"], 1);
    assert_eq!(decoded["socket_groups"][1]["weapon_set"], 2);
    let equipped = decoded["items"]["equipped"].as_array().unwrap();
    assert_eq!(equipped.len(), 2);
    assert!(!equipped.iter().any(|item| item["slot"] == "weapon2"));
    assert!(
        equipped
            .iter()
            .any(|item| item["text"].as_str().unwrap().contains("Ashen Staff"))
    );
}

#[test]
fn ring_added_damage_respects_attack_and_spell_skill_contexts() {
    init();
    for (skill, applicable, irrelevant) in [
        (
            "FireballPlayer",
            "Adds 20 to 40 Fire Damage to Spells",
            "Adds 20 to 40 Fire Damage to Attacks",
        ),
        (
            "IceShotPlayer",
            "Adds 20 to 40 Fire Damage to Attacks",
            "Adds 20 to 40 Fire Damage to Spells",
        ),
    ] {
        let request = json!({
            "character":{"level":71,"class_name":"Ranger"},
            "items":[{"slot":"weapon1","text":"Rarity: NORMAL\nCrude Bow"}],
            "socket_groups":[{"enabled":true,"gems":[{"skill_id":skill,"level":12}]}],
            "main_socket_group":0
        });
        let input = json!({"request":request, "stats":["TotalDPS"], "variants":[
            {"set_items":[{"slot":"ring1","text":format!("Rarity: RARE\nSynthetic Ring\nSapphire Ring\n{applicable}")}]},
            {"set_items":[{"slot":"ring1","text":format!("Rarity: RARE\nSynthetic Ring\nSapphire Ring\n{irrelevant}")}]}
        ]});
        let result: Value =
            serde_json::from_str(&pobr_wasm::optimize_variants_json(&input.to_string()).unwrap())
                .unwrap();
        let base = result["baseline"]["TotalDPS"].as_f64().unwrap();
        assert!(base > 0.0, "{skill}");
        assert!(
            result["variants"][0]["stats"]["TotalDPS"].as_f64().unwrap() > base,
            "{skill}: relevant damage"
        );
        assert_eq!(
            result["variants"][1]["stats"]["TotalDPS"].as_f64().unwrap(),
            base,
            "{skill}: irrelevant damage"
        );
    }
}

fn assert_swap(actual: &Value, expected: &Value) {
    let normalize = |value: &Value| {
        let mut value = value.clone();
        for item in value["alternate_items"].as_array_mut().unwrap() {
            item["text"] = json!(item["text"].as_str().unwrap().trim());
        }
        value
    };
    assert_eq!(normalize(actual), normalize(expected));
}

#[test]
fn arrow_chance_changes_expected_projectiles_only_for_arrow_skill_sets() {
    init();
    for (skill, set_index, delta) in [
        ("IceShotPlayer", 1, 0.4),
        ("IceShotPlayer", 2, 0.0),
        ("FireballPlayer", 1, 0.0),
    ] {
        let code = pobr_build::encode_pob_code(&format!(
            r#"<PathOfBuilding2><Build level="71" className="Ranger"/><Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="{skill}" gemId="SyntheticGem" level="12" statSetIndex="{set_index}" enabled="true"/></Skill></SkillSet></Skills></PathOfBuilding2>"#
        )).unwrap();
        let input = json!({
            "request": {
                "pob_code":code,
                "character":{"level":71,"class_name":"Ranger"},
                "items":[{"slot":"weapon1","text":"Rarity: NORMAL\nCrude Bow"}],
                "main_socket_group":0
            },
            "stats":["TotalDPS","ProjectileCount"],
            "variants":[{"set_items":[{"slot":"weapon2","text":"Rarity: RARE\nSynthetic Quiver\nPrimed Quiver\n+40% Surpassing chance to fire an additional Arrow"}]}]
        });
        let result: Value =
            serde_json::from_str(&pobr_wasm::optimize_variants_json(&input.to_string()).unwrap())
                .unwrap();
        let base = &result["baseline"];
        let variant = &result["variants"][0];
        assert!(variant["error"].is_null(), "{variant}");
        assert!(
            base["ProjectileCount"].as_f64().unwrap() >= 1.0,
            "{skill}: {base}"
        );
        assert!(
            (variant["stats"]["ProjectileCount"].as_f64().unwrap()
                - base["ProjectileCount"].as_f64().unwrap()
                - delta)
                .abs()
                < 1e-9,
            "{skill}/{set_index}: {result}"
        );
        assert_eq!(
            variant["stats"]["TotalDPS"], base["TotalDPS"],
            "Extra projectiles do not imply same-target shotgun damage"
        );
    }
}
