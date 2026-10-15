use pobr_item::ItemDraft;
use serde_json::{Value, json};

fn init() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
}
fn catalogue() -> Vec<Value> {
    serde_json::from_str(&pobr_wasm::unique_catalog_json().unwrap()).unwrap()
}
fn entry(name: &str) -> Value {
    catalogue()
        .into_iter()
        .find(|entry| entry["name"] == name)
        .unwrap()
}
fn draft(request: Value) -> Value {
    serde_json::from_str(&pobr_wasm::unique_item_draft_json(&request.to_string()).unwrap()).unwrap()
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
fn unique_catalog_contract_and_variant_dependent_bases() {
    init();
    let entries = catalogue();
    let game = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir());
    assert_eq!(
        entries.len(),
        game.uniques().unwrap().unwrap().uniques.len()
    );
    keys(&entries[0], &["id", "name", "item_type", "raw", "bases"]);
    keys(&entries[0]["bases"][0], &["name", "item_class"]);
    let howa = entry("Hand of Wisdom and Action");
    assert_eq!(howa["bases"][1]["name"], "Spiral Wraps");
    assert_eq!(draft(json!({ "id": howa["id"] }))["base"], "Spiral Wraps");
    assert_eq!(
        draft(json!({ "id": howa["id"], "variant": 1 }))["base"],
        "Furtive Wraps"
    );
    assert_eq!(
        draft(json!({ "id": entry("Pillar of the Caged God")["id"] }))["base"],
        "Long Quarterstaff"
    );
}

#[test]
fn variants_and_roll_values_round_trip_through_library_text_and_export() {
    init();
    let astramentis = entry("Astramentis");
    let middle = draft(json!({ "id": astramentis["id"] }));
    keys(&middle, &["text", "base", "required_level"]);
    let text = middle["text"].as_str().unwrap();
    assert!(text.contains("Selected Variant: 2"));
    assert!(text.contains("+75 to all Attributes"));
    assert!(text.contains("+6 to all Attributes"));
    assert!(!text.contains("+90 to all Attributes"));
    let roll_line = astramentis["raw"]
        .as_str()
        .unwrap()
        .lines()
        .position(|line| line.contains("+(80-100)"))
        .unwrap();
    let high = draft(
        json!({ "id": astramentis["id"], "variant": 1, "rolls": { roll_line.to_string(): 1.0 } }),
    );
    let high_text = high["text"].as_str().unwrap();
    assert!(high_text.contains("+100 to all Attributes"));
    assert!(!high_text.contains("+75 to all Attributes"));
    let original = ItemDraft::parse(high_text).unwrap();
    assert_eq!(ItemDraft::parse(&original.build_raw()).unwrap(), original);
    let encoded = pobr_wasm::encode_build_json(
        &json!({
            "character": { "level": 85, "class_name": "Sorceress" },
            "items": [{ "slot": "amulet", "text": high_text }]
        })
        .to_string(),
    )
    .unwrap();
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&encoded).unwrap()).unwrap();
    assert_eq!(
        ItemDraft::parse(decoded["items"]["equipped"][0]["text"].as_str().unwrap()).unwrap(),
        original
    );
}

#[test]
fn version_and_group_selection_reuse_calculation_semantics() {
    init();
    let idol = entry("Idol of Uldurn");
    let current = draft(json!({ "id": idol["id"] }));
    let old = draft(json!({ "id": idol["id"], "version": 1 }));
    assert!(
        current["text"]
            .as_str()
            .unwrap()
            .contains("reduced Presence Area")
    );
    assert!(
        !old["text"]
            .as_str()
            .unwrap()
            .contains("reduced Presence Area")
    );
    let atziri = entry("Atziri's Splendour");
    let selected = draft(
        json!({ "id": atziri["id"], "variant": 1, "alts": [5], "groups": { "1": 4, "2": 6 } }),
    );
    let text = selected["text"].as_str().unwrap();
    assert!(text.contains("as though it was also a Shield"), "{text}");
    assert!(!text.contains("as though it was also Boots"));
    assert!(!text.contains("{group:"));
    assert!(text.contains("Selected Variant: 4"));
    assert!(text.contains("Selected Alt Variant: 6"));
    assert!(
        pobr_wasm::unique_item_draft_json(
            &json!({ "id": atziri["id"], "alts": [5], "groups": { "1": 11 } }).to_string()
        )
        .is_err()
    );
}

#[test]
fn every_default_template_is_constructible_or_explicitly_diagnosed() {
    init();
    let mut unavailable = Vec::new();
    for entry in catalogue() {
        let raw = entry["raw"].as_str().unwrap();
        let variants = raw
            .lines()
            .filter(|line| line.starts_with("Variant:"))
            .count() as u32;
        let alts: Vec<u32> = ["", " Two", " Three", " Four", " Five"]
            .iter()
            .filter_map(|suffix| {
                if !raw
                    .lines()
                    .any(|line| line == format!("Has Alt Variant{suffix}: true"))
                {
                    return None;
                }
                Some(
                    raw.lines()
                        .find_map(|line| {
                            line.strip_prefix(&format!("Selected Alt Variant{suffix}: "))
                                .and_then(|value| value.parse().ok())
                        })
                        .unwrap_or(variants),
                )
            })
            .collect();
        match pobr_wasm::unique_item_draft_json(
            &json!({ "id": entry["id"], "alts": alts }).to_string(),
        ) {
            Ok(value) => {
                let value: Value = serde_json::from_str(&value).unwrap();
                let text = value["text"].as_str().unwrap();
                let parsed = ItemDraft::parse(text).unwrap();
                assert_eq!(ItemDraft::parse(&parsed.build_raw()).unwrap(), parsed);
                assert!(!text.contains("{variant:"));
            }
            Err(error) => {
                let error: Value = serde_json::from_str(&error).unwrap();
                assert_eq!(
                    error["message"], "unique_base_unavailable",
                    "{}: {error}",
                    entry["name"]
                );
                unavailable.push(entry["name"].as_str().unwrap().to_string());
            }
        }
    }
    // The pinned Current record gates its two bases to variants 1 and 2 only.
    assert_eq!(unavailable, vec!["Voll's Protector"]);
}
