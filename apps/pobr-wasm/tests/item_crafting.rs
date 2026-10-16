use pobr_item::ItemDraft;
use serde_json::Value;

fn init() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
}

fn keys(value: &Value, expected: &[&str]) {
    let mut actual: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn crafting_contract_uses_game_mods_and_inherited_base_tags() {
    init();
    let catalog: Value =
        serde_json::from_str(&pobr_wasm::item_crafting_catalog_json().unwrap()).unwrap();
    keys(&catalog, &["bases", "mods"]);
    let ring = catalog["bases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|base| base["name"] == "Iron Ring")
        .unwrap();
    keys(ring, &["id", "name", "item_class", "tags", "mod_domain"]);
    assert_eq!(ring["item_class"], "Ring");
    assert_eq!(ring["mod_domain"], 1);
    assert!(
        ring["tags"]
            .as_array()
            .unwrap()
            .contains(&Value::from("ring"))
    );
    let strength = catalog["mods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "Strength1")
        .unwrap();
    keys(
        strength,
        &["id", "domain", "kind", "level", "group", "spawn_weights"],
    );
    let game = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir());
    let source = game
        .mods()
        .unwrap()
        .into_iter()
        .find(|m| m.id == "Strength1")
        .unwrap();
    assert_eq!(
        strength["spawn_weights"],
        serde_json::to_value(source.spawn_weights).unwrap()
    );
    assert_eq!(strength["group"], "Strength");
    assert_eq!(strength["kind"], "suffix");
    keys(&strength["spawn_weights"][0], &["tag", "weight"]);
}

#[test]
fn crafted_text_round_trips_through_item_draft_and_build_export() {
    init();
    for rarity in ["NORMAL", "MAGIC", "RARE"] {
        let title = if rarity == "RARE" { "Draft Ring\n" } else { "" };
        let explicit = if rarity == "NORMAL" {
            ""
        } else {
            "\n+92 to maximum Life\n+8 to Strength"
        };
        let input = format!(
            "Rarity: {rarity}\n{title}Sapphire Ring\nItem Level: 85\nQuality: 20\nLevelReq: 12\nImplicits: 1\n+30% to Cold Resistance{explicit}"
        );
        let normalized: Value =
            serde_json::from_str(&pobr_wasm::normalize_item_draft_json(&input).unwrap()).unwrap();
        keys(&normalized, &["text"]);
        let text = normalized["text"].as_str().unwrap();
        assert_eq!(
            ItemDraft::parse(&input).unwrap(),
            ItemDraft::parse(text).unwrap()
        );
        let again: Value =
            serde_json::from_str(&pobr_wasm::normalize_item_draft_json(text).unwrap()).unwrap();
        assert_eq!(normalized, again);
        let code = pobr_wasm::encode_build_json(&serde_json::json!({
            "character": { "level": 85, "class_name": "Sorceress" }, "items": [{ "slot": "ring1", "text": text }]
        }).to_string()).unwrap();
        let decoded: Value =
            serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
        let restored = decoded["items"]["equipped"][0]["text"].as_str().unwrap();
        assert_eq!(
            ItemDraft::parse(restored).unwrap(),
            ItemDraft::parse(text).unwrap()
        );
    }
    let error: Value = serde_json::from_str(
        &pobr_wasm::normalize_item_draft_json(
            "Rarity: RARE\nDraft\nSapphire Ring\nItem Level: 101",
        )
        .unwrap_err(),
    )
    .unwrap();
    assert_eq!(error["code"], "bad_request");
}
