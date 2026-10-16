//! Source identity at the public JSON decoder: unsupported saves must not
//! become empty characters, while structurally empty legacy exports remain valid.
use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

fn init() {
    let dir = pobr_gamedata::current_data_dir();
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
}

fn decode(input: &Value) -> Value {
    serde_json::from_str(
        &pobr_wasm::decode_build_file_json(&input.to_string()).expect("decode build file"),
    )
    .expect("decoded JSON")
}

fn assert_decode_error(content: &str) {
    let error = pobr_wasm::decode_build_file_json(content).expect_err("reject unsupported source");
    let error: Value = serde_json::from_str(&error).expect("structured API error");
    assert_eq!(error["code"], "decode_error", "input: {content}");
}

fn empty_legacy() -> Value {
    json!({"passives": [], "items": [], "skills": []})
}

#[test]
fn unsupported_roots_and_saved_envelopes_are_decode_errors() {
    init();
    for input in [
        Value::Null,
        json!(false),
        json!(42),
        json!("not a build"),
        json!([]),
        // Derived structs can deserialize positional sequences; this is not
        // the named-collection identity of a legacy .build object.
        json!(["Character Lv.98", null, [], [], []]),
        json!({}),
        json!({"version": 1}),
        json!({
            "version": 1,
            "state": {
                "character": {"level": 98, "class_name": "Monk", "ascendancy_name": "Invoker"},
                "allocated_nodes": [3251],
                "socket_groups": [],
                "items": []
            },
            "notes": "Saved session, not a legacy export"
        }),
        json!({"version": 1, "workspace": {"builds": [], "activeBuildId": null}}),
        json!({"format": "pobr-build", "version": 1, "build": {"state": {}}}),
    ] {
        assert_decode_error(&input.to_string());
    }
}

#[test]
fn all_three_legacy_collections_are_required_and_typed() {
    init();
    for field in ["passives", "items", "skills"] {
        let mut missing = empty_legacy();
        missing.as_object_mut().unwrap().remove(field);
        assert_decode_error(&missing.to_string());
        for invalid in [Value::Null, json!({}), json!("[]"), json!(false), json!(1)] {
            let mut input = empty_legacy();
            input[field] = invalid;
            assert_decode_error(&input.to_string());
        }
    }
}

#[test]
fn collection_identity_does_not_bypass_inner_validation() {
    init();
    for (field, invalid) in [
        ("passives", json!([{}])),
        ("passives", json!([{"id": 3251}])),
        ("items", json!([{}])),
        ("items", json!([{"inventory_id": false}])),
        (
            "items",
            json!([{"inventory_id": "Boots1", "additional_text": []}]),
        ),
        ("skills", json!([{}])),
        ("skills", json!([{"id": null}])),
        ("skills", json!([{"id": "Unknown", "support_skills": {}}])),
        ("skills", json!([{"id": "Unknown", "support_skills": [{}]}])),
        ("name", json!(null)),
        ("ascendancy", json!(7)),
        ("ascendancy", json!("UnknownAscendancy")),
    ] {
        let mut input = empty_legacy();
        input[field] = invalid;
        assert_decode_error(&input.to_string());
    }
}

#[test]
fn malformed_json_is_a_decode_error() {
    for content in [
        "",
        "{",
        "{\"passives\":[],\"items\":[],\"skills\":[]",
        "{} trailing",
    ] {
        assert_decode_error(content);
    }
}

#[test]
fn format_marker_never_falls_back_to_legacy_collections() {
    init();
    let complete = json!({
        "format": "wegame", "version": 1,
        "role": {"level": 59, "class_name": "Deadeye"},
        "equipments": [],
        "talent_tree": {"hashes": [], "quest_stats": []},
        "jewel_data": "[]", "skills": [],
        "passives": [], "items": []
    });
    for invalid_format in [json!("other"), json!("pobr-build"), Value::Null, json!(1)] {
        let mut input = complete.clone();
        input["format"] = invalid_format;
        assert_decode_error(&input.to_string());
    }
    let mut unsupported_version = complete.clone();
    unsupported_version["version"] = json!(2);
    assert_decode_error(&unsupported_version.to_string());
    let mut missing_role = complete;
    missing_role.as_object_mut().unwrap().remove("role");
    assert_decode_error(&missing_role.to_string());
}

#[test]
fn empty_legacy_collections_allow_optional_identity_and_unknown_metadata() {
    init();
    let baseline = decode(&empty_legacy());
    assert_eq!(
        baseline["character"],
        json!({"level": 1, "class_name": "", "ascendancy_name": ""})
    );
    assert_eq!(baseline["tree"]["allocated_nodes"], json!([]));
    assert_eq!(baseline["items"]["equipped"], json!([]));
    assert_eq!(baseline["socket_groups"], json!([]));
    assert_eq!(baseline["notes"], Value::Null);

    let mut metadata = empty_legacy();
    metadata["ascendancy"] = Value::Null;
    metadata["author"] = Value::Null;
    metadata["export_metadata"] = json!({"revision": 3, "tags": ["external"]});
    assert_eq!(decode(&metadata), baseline);

    metadata["name"] = json!("Character Lv.98");
    metadata["ascendancy"] = json!("Sorceress3");
    let named = decode(&metadata);
    assert_eq!(
        named["character"],
        json!({"level": 98, "class_name": "Sorceress", "ascendancy_name": "Disciple of Varashta"})
    );
    assert_eq!(named["tree"], baseline["tree"]);
    assert_eq!(named["items"], baseline["items"]);
    assert_eq!(named["socket_groups"], baseline["socket_groups"]);
    assert_eq!(named["notes"], "Character Lv.98");
}

#[test]
fn committed_legacy_export_preserves_character_tree_items_and_skills() {
    init();
    let path = repo_data_root().join("../examples/build-file/召唤奥黛丽黑本.build");
    let source: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let decoded = decode(&source);
    assert_eq!(
        decoded["character"],
        json!({"level": 98, "class_name": "Sorceress", "ascendancy_name": "Disciple of Varashta"})
    );
    // Exact node identities from this committed export in the pinned golden
    // data, not merely a nonempty/tree-size check.
    assert_eq!(
        decoded["tree"]["allocated_nodes"],
        serde_json::to_value(vec![
            95, 116, 517, 1218, 1433, 1506, 2254, 3251, 4345, 4810, 5314, 5335, 6006, 6715, 7104,
            7338, 7782, 7960, 8616, 8737, 8831, 8957, 8983, 9642, 10131, 10561, 11248, 11376,
            11672, 11679, 11838, 12778, 13289, 14258, 14598, 14666, 14686, 14945, 15408, 15782,
            15809, 15885, 16466, 17602, 18160, 19318, 21568, 21984, 22783, 23265, 24721, 25893,
            25927, 26196, 26563, 26926, 26945, 29009, 29148, 30265, 30346, 30523, 30720, 31175,
            31238, 32705, 33240, 33612, 34006, 34199, 34840, 35560, 36746, 37594, 37974, 38105,
            39037, 39567, 40200, 40313, 40691, 40894, 41372, 41394, 41511, 43633, 43979, 44299,
            44871, 44872, 45333, 45343, 45885, 46091, 46554, 46628, 46819, 47168, 48030, 48552,
            48805, 49512, 50104, 50483, 50498, 50720, 50816, 50837, 51169, 53795, 54036, 54733,
            55938, 56349, 56857, 57021, 57710, 58363, 59362, 59798, 59799, 61403, 61419, 61842,
            61905, 62210, 62230, 62677, 63400, 64223
        ])
        .unwrap()
    );
    let expected_slots = [
        (0, "boots"),
        (1, "helmet"),
        (2, "gloves"),
        (3, "weapon1"),
        (4, "bodyarmour"),
        (5, "ring1"),
        (6, "weapon2"),
        (8, "belt"),
        (9, "ring2"),
        (10, "amulet"),
    ];
    let equipped = decoded["items"]["equipped"].as_array().unwrap();
    assert_eq!(equipped.len(), expected_slots.len());
    for (item, (source_index, slot)) in equipped.iter().zip(expected_slots) {
        assert_eq!(item["slot"], slot);
        let raw_mods = source["items"][source_index]["additional_text"]
            .as_str()
            .unwrap();
        let imported_mods = item["text"]
            .as_str()
            .unwrap()
            .splitn(3, '\n')
            .nth(2)
            .unwrap();
        assert_eq!(imported_mods, raw_mods, "slot: {slot}");
    }
    let expected_gems: &[&[&str]] = &[
        &[
            "SummonSandDjinnPlayer",
            "SupportMusterPlayer",
            "SupportBiddingPlayerThree",
            "SupportHulkingMinionsPlayer",
            "SupportUulNetolsEmbracePlayer",
            "SupportVoranasSiege",
        ],
        &[
            "SummonWaterDjinnPlayer",
            "SupportChillingIcePlayer",
            "SupportEncroachingGroundPlayer",
            "SupportBiddingPlayerThree",
            "SupportMusterPlayer",
            "SupportTulsStillnessPlayer",
        ],
        // The Djinn's commands are gems of their own in the export; the
        // game's `GemEffects` links them to their skills.
        &["CommandWaterDjinnGroundBurstPlayer"],
        &["CommandSandDjinnExplosiveTeleportPlayer"],
        &[
            "PurityOfLightningPlayer",
            "SupportUpwellingPlayerTwo",
            "SupportClarityPlayerTwo",
            "SupportSeraphsHeartPlayer",
            "SupportHerDeclarationPlayer",
        ],
        &[
            "HyenaCacklePlayer",
            "SupportMinionInstabilityPlayer",
            "SupportLastGaspPlayer",
            "SupportUruksSmeltingPlayer",
            "SupportAmanamusTithePlayer",
            "SupportRomirasRequitalPlayer",
        ],
        &[
            "RefutationPlayer",
            "ProlongedDurationSupportPlayerTwo",
            "SupportCooldownRecoveryPlayerTwo",
            "SupportRapidCastingPlayerTwo",
            "SupportMobilityPlayer",
        ],
        &[
            "SummonSkeletalStormMagesPlayer",
            "SupportBiddingPlayerTwo",
            "SupportLastGaspPlayer",
            "SupportRapidCastingPlayerTwo",
        ],
        &[
            "SummonSkeletalSnipersPlayer",
            "SupportArmourBreakPlayerThree",
            "SupportArmourDemolisherPlayerTwo",
            "SupportInfernalLegionPlayer",
            "SupportMusterPlayer",
        ],
        &[
            "MetaCastOnMinionDeathPlayer",
            "PoweredByVerisiumPlayer",
            "SupportCooldownRecoveryPlayerTwo",
            "SupportMagnifiedAreaPlayerTwo",
            "SupportEncroachingGroundPlayer",
            "IceNovaPlayer",
        ],
        &[
            "SummonSkeletalArsonistsPlayer",
            "SupportBrutusBrainPlayer",
            "SupportRapidAttacksPlayerThree",
            "SupportMagnifiedAreaPlayerTwo",
            "SupportBiddingPlayerThree",
            "SupportEfficiencyPlayerTwo",
        ],
        &[
            "SummonSkeletalReaversPlayer",
            "SupportMinionInstabilityPlayer",
            "SupportArmourBreakPlayerThree",
            "SupportInfernalLegionPlayer",
        ],
        &["SummonSkeletalFrostMagesPlayer", "SupportLastGaspPlayer"],
    ];
    let groups = decoded["socket_groups"].as_array().unwrap();
    assert_eq!(groups.len(), expected_gems.len());
    for (group, expected) in groups.iter().zip(expected_gems) {
        assert_eq!(group["active_skill_id"], expected[0]);
        let gems = group["gems"].as_array().unwrap();
        let ids: Vec<_> = gems
            .iter()
            .map(|gem| gem["skill_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, *expected);
    }
}
