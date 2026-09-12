//! Integration tests for rune socket editing (real data): the catalog
//! carries the Chinese name sidecar; re-socketing rewrites text (named
//! lines plus `{rune}` mod lines plus the Implicits count), and the result is directly consumable by the engine.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn ensure_data() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
}

/// A 3-rune-socket body armour (PoB2 export shape: Rune named lines plus
/// `{rune}` mod lines within the Implicits window).
const ARMOUR_WITH_RUNES: &str = "\
Rarity: RARE
Dread Mantle
Death Mail
Item Level: 82
Quality: 30
Sockets: S S S
Rune: Iron Rune
LevelReq: 75
Implicits: 1
{rune}20% increased Armour, Evasion and Energy Shield
+167 to Armour
+45% to Fire Resistance";

#[test]
fn rune_catalog_has_names_and_zh() {
    ensure_data();
    let json = pobr_wasm::rune_catalog_json(ARMOUR_WITH_RUNES).expect("catalog");
    let entries: Vec<Value> = serde_json::from_str(&json).expect("parse");
    assert!(
        entries.len() > 200,
        "the rune catalog should have the full entry set, got {}",
        entries.len()
    );
    let iron = entries
        .iter()
        .find(|e| e["name"] == "Iron Rune")
        .expect("Iron Rune should be in the catalog");
    assert!(
        iron["name_zh_cn"].as_str().is_some_and(|s| !s.is_empty()),
        "Iron Rune should have a Simplified Chinese name, got {:?}",
        iron["name_zh_cn"]
    );
    // With an item context given, the effect lines applicable to that base (armour) should be attached.
    assert!(
        iron["lines"][0]
            .as_str()
            .is_some_and(|s| s.contains("Armour")),
        "Iron Rune should have an Armour effect line for armour, got {:?}",
        iron["lines"]
    );

    // No item context: the catalog still works, with lines all empty.
    let json = pobr_wasm::rune_catalog_json("").expect("catalog no item");
    let entries: Vec<Value> = serde_json::from_str(&json).expect("parse");
    assert!(
        entries
            .iter()
            .all(|e| e["lines"].as_array().is_some_and(|a| a.is_empty())),
        "lines should all be empty with no item context"
    );
}

#[test]
fn reforge_replaces_rune_lines_and_fixes_implicit_count() {
    ensure_data();
    let request = json!({
        "text": ARMOUR_WITH_RUNES,
        "runes": ["Greater Iron Rune", "Adept Rune"],
    });
    let out = pobr_wasm::reforge_runes_json(&request.to_string()).expect("reforge");
    let response: Value = serde_json::from_str(&out).expect("parse");
    let text = response["text"].as_str().unwrap();

    // The old rune named line is stripped, new named lines inserted in order after Sockets.
    assert!(
        !text.contains("Rune: Iron Rune\n"),
        "the old named line should have been replaced:\n{text}"
    );
    assert!(text.contains("Rune: Greater Iron Rune"));
    assert!(text.contains("Rune: Adept Rune"));
    // The old {rune} mod line (20%) is stripped, new mod lines carry the
    // {rune} prefix (Adept Rune's armour effect = +9 Dexterity).
    assert!(
        !text.contains("{rune}20% increased Armour"),
        "the old rune mod line should have been stripped:\n{text}"
    );
    assert!(
        text.contains("{rune}+9 to Dexterity"),
        "the new rune mod line should have been written:\n{text}"
    );
    // PoB2 ce566eac (0.5.5): both runes have one armour line and two
    // bonded bonuses. Replace the old rune implicit with all six lines.
    assert!(text.contains("{rune}Bonded: +20 to maximum Life"));
    assert!(
        text.contains("Implicits: 6"),
        "the Implicits count should have been fixed up:\n{text}"
    );
    // Non-rune mod lines are preserved as-is.
    assert!(text.contains("+167 to Armour"));
}

#[test]
fn reforge_resizes_sockets() {
    ensure_data();
    // 3 sockets -> 2 sockets (with 1 rune), the Sockets line is rewritten.
    let shrink = json!({ "text": ARMOUR_WITH_RUNES, "runes": ["Iron Rune"], "sockets": 2 });
    let out = pobr_wasm::reforge_runes_json(&shrink.to_string()).expect("shrink");
    let text: Value = serde_json::from_str(&out).unwrap();
    let text = text["text"].as_str().unwrap();
    assert!(
        text.contains("Sockets: S S\n"),
        "socket count should be rewritten to 2:\n{text}"
    );

    // An item with no Sockets line directly gains 1 socket: the new line is inserted, and it accepts a rune.
    let no_sockets =
        "Rarity: RARE\nApocalypse Pelt\nFalconer's Jacket\nItem Level: 81\n+190 to maximum Life";
    let grow = json!({ "text": no_sockets, "runes": ["Iron Rune"], "sockets": 1 });
    let out = pobr_wasm::reforge_runes_json(&grow.to_string()).expect("grow");
    let text: Value = serde_json::from_str(&out).unwrap();
    let text = text["text"].as_str().unwrap();
    assert!(
        text.contains("Sockets: S\n"),
        "a new Sockets line should have been added:\n{text}"
    );
    assert!(text.contains("Rune: Iron Rune"));

    // Reduced to 0 sockets: the Sockets line and every rune are removed.
    let zero = json!({ "text": ARMOUR_WITH_RUNES, "runes": [], "sockets": 0 });
    let out = pobr_wasm::reforge_runes_json(&zero.to_string()).expect("zero");
    let text: Value = serde_json::from_str(&out).unwrap();
    let text = text["text"].as_str().unwrap();
    assert!(
        !text.contains("Sockets:"),
        "0 sockets should remove the Sockets line:\n{text}"
    );
    assert!(!text.contains("Rune:"));
    assert!(
        text.contains("Implicits: 0"),
        "count should reset to zero once the rune mods are cleared:\n{text}"
    );
}

#[test]
fn reforge_rejects_over_capacity_and_unknown_rune() {
    ensure_data();
    let over = json!({ "text": ARMOUR_WITH_RUNES, "runes": vec!["Iron Rune"; 4] });
    assert!(pobr_wasm::reforge_runes_json(&over.to_string()).is_err());
    let unknown = json!({ "text": ARMOUR_WITH_RUNES, "runes": ["No Such Rune"] });
    assert!(pobr_wasm::reforge_runes_json(&unknown.to_string()).is_err());
}

fn info(text: &str) -> Value {
    serde_json::from_str(&pobr_wasm::item_augment_info_json(text).unwrap()).unwrap()
}

fn reforge(text: &str, runes: &[&str], sockets: u32) -> String {
    let out = pobr_wasm::reforge_runes_json(
        &json!({"text": text, "runes": runes, "sockets": sockets}).to_string(),
    )
    .unwrap();
    serde_json::from_str::<Value>(&out).unwrap()["text"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn augment_info_uses_base_socket_limits_and_preserves_empty_positions() {
    ensure_data();
    let bow = "Rarity: RARE\nTest\nTwin Bow\nSockets: S S\nRune: None\nRune: Greater Glacial Rune\n{rune}Adds 9 to 15 Cold Damage";
    let meta = info(bow);
    assert_eq!(meta["sockets"], 2);
    assert_eq!(meta["max_sockets"], 4);
    assert_eq!(meta["runes"], json!(["", "Greater Glacial Rune"]));
    assert_eq!(meta["editable"], true);
    let options = meta["options"].as_array().unwrap();
    let glacial = options
        .iter()
        .find(|entry| entry["name"] == "Greater Glacial Rune")
        .unwrap();
    assert_eq!(glacial["kind"], "Rune");
    assert!(glacial["required_level"].is_number());
    assert!(
        glacial["lines"][0]
            .as_str()
            .unwrap()
            .contains("Cold Damage")
    );
    assert!(
        !options
            .iter()
            .any(|entry| entry["name"] == "Hayoxi's Soul Core of Heatproofing")
    );
    let empty = reforge(bow, &["", "Greater Glacial Rune"], 2);
    assert_eq!(info(&empty)["runes"], meta["runes"]);
    assert_eq!(empty.matches("Rune: None").count(), 1);
    assert_eq!(empty.matches("{rune}Adds 9 to 15 Cold Damage").count(), 1);
}

#[test]
fn localized_and_magic_market_bases_can_be_resocketed() {
    ensure_data();
    for text in [
        "Rarity: RARE\n测试\n分层背心\nSockets: S\n+20 生命上限",
        "Item Class: Body Armours\nRarity: MAGIC\nAgile Layered Vest of the Fox\nLayered Vest\n--------\nSockets: S\n--------\n+20 to maximum Life",
        "Rarity: MAGIC\nAgile Layered Vest of the Fox\nSockets: S\n+20 to maximum Life",
    ] {
        assert_eq!(info(text)["editable"], true, "{text}");
        let out = reforge(text, &["Greater Iron Rune"], 1);
        assert!(out.contains("\nLayered Vest\n"));
        assert!(out.contains("+20 to maximum Life"));
        assert!(out.contains("{rune}18% increased Armour, Evasion and Energy Shield"));
    }
}

#[test]
fn clipboard_separator_and_notes_do_not_consume_implicit_count() {
    ensure_data();
    let text = "Rarity: RARE\nTest\nLayered Vest\n--------\nSockets: S\nRune: Iron Rune\nNote: retained metadata\n--------\nImplicits: 2\n--------\n{rune}16% increased Armour, Evasion and Energy Shield\n+10 to maximum Life\n--------\n+30% to Fire Resistance\nNote: Sum 123.45";
    let out = reforge(text, &["Storm Rune"], 1);
    let draft = pobr_item::ItemDraft::parse(&out).unwrap();
    assert!(
        draft
            .lines
            .iter()
            .any(|line| line.text == "+10 to maximum Life"
                && line.bucket == pobr_item::LineBucket::Implicit)
    );
    assert!(
        draft
            .lines
            .iter()
            .any(|line| line.text == "+30% to Fire Resistance"
                && line.bucket == pobr_item::LineBucket::Explicit)
    );
    assert!(out.contains("Note: retained metadata"));
    assert!(out.contains("Note: Sum 123.45"));
    assert!(out.contains("Implicits: 4"));
    assert!(!out.contains("{rune}16%"));
}

#[test]
fn corrupt_items_keep_extra_sockets_but_cannot_gain_more() {
    ensure_data();
    for state in ["Corrupted", "Mirrored", "Sanctified", "Twice Corrupted"] {
        let text = format!("Rarity: RARE\nTest\nTwin Bow\nSockets: S S S S S\n{state}");
        let meta = info(&text);
        assert_eq!(meta["editable"], true, "{meta}");
        assert_eq!(meta["max_sockets"], 5);
        let out = reforge(&text, &["Iron Rune"], 5);
        assert!(out.contains(state));
        assert!(
            pobr_wasm::reforge_runes_json(&json!({"text":text,"runes":[],"sockets":6}).to_string())
                .is_err()
        );
    }
    let ordinary = "Rarity: RARE\nTest\nTwin Bow";
    assert!(
        pobr_wasm::reforge_runes_json(&json!({"text":ordinary,"runes":[],"sockets":5}).to_string())
            .is_err()
    );
    assert!(
        pobr_wasm::reforge_runes_json(
            &json!({"text":ordinary,"runes":[],"sockets":u64::MAX}).to_string()
        )
        .is_err()
    );
}

#[test]
fn ambiguous_and_special_augments_are_read_only() {
    ensure_data();
    for (text, reason) in [
        (
            "Rarity: RARE\nTest\nUnknown Bow\nSockets: S\nRune: Iron Rune",
            "unknown_base",
        ),
        (
            "Rarity: RARE\nTest\nLayered Vest\nSockets: J",
            "jewel_sockets",
        ),
        (
            "Rarity: RARE\nTest\nTwin Bow\nSockets: S\n{rune}Adds 9 to 15 Cold Damage",
            "unknown_augments",
        ),
        (
            "Rarity: RARE\nTest\nTwin Bow\nSockets: S\nRune: Missing Augment",
            "unknown_augments",
        ),
        (
            "Rarity: RARE\nTest\nTwin Bow\nSockets: S\nRune: Greater Iron Rune\n100% increased Effect of Socketed Runes",
            "special_augment_rules",
        ),
        (
            "Rarity: RARE\nTest\nTwin Bow\nThis Item gains bonuses from Socketed Items as though it was Body Armour",
            "special_augment_rules",
        ),
    ] {
        let meta = info(text);
        assert_eq!(meta["editable"], false, "{text}");
        assert_eq!(meta["reason"], reason, "{text}");
        assert!(
            pobr_wasm::reforge_runes_json(&json!({"text":text,"runes":[],"sockets":0}).to_string())
                .is_err()
        );
    }
}

#[test]
fn soul_core_limits_and_idol_categories_are_data_driven() {
    ensure_data();
    let helmet = "Rarity: RARE\nTest\nChain Tiara\nSockets: S S";
    let meta = info(helmet);
    let core = meta["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "Hayoxi's Soul Core of Heatproofing")
        .unwrap();
    assert_eq!(core["kind"], "SoulCore");
    assert_eq!(core["limit"], 1);
    assert!(
        pobr_wasm::reforge_runes_json(
            &json!({"text":helmet,"runes":[core["name"],core["name"]]}).to_string()
        )
        .is_err()
    );
    let out = reforge(helmet, &["Hayoxi's Soul Core of Heatproofing"], 2);
    assert!(out.contains("Rune: Hayoxi's Soul Core of Heatproofing"));
    assert!(out.contains("LevelReq: 50"));
    let sceptre = info("Rarity: RARE\nTest\nRattling Sceptre");
    assert!(
        sceptre["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["kind"] == "Idol")
    );
    let soul_only = info(&format!(
        "{helmet}\nOnly Soul Cores can be Socketed in this Item"
    ));
    assert!(
        soul_only["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["kind"] == "SoulCore")
    );
}

#[test]
fn differently_named_ancient_augments_share_one_limit() {
    ensure_data();
    let text = "Rarity: RARE\nTest\nLayered Vest\nSockets: S S";
    let names = ["Guatelitzi's Thesis", "Citaqualotl's Thesis"];
    let catalog: Vec<Value> =
        serde_json::from_str(&pobr_wasm::rune_catalog_json("").unwrap()).unwrap();
    for name in names {
        let entry = catalog.iter().find(|entry| entry["name"] == name).unwrap();
        assert_eq!(entry["limit"], 1);
        assert_eq!(entry["limit_id"], "AncientAugment");
        assert!(entry["lines"].as_array().unwrap().is_empty());
        assert!(
            info(text)["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| { entry["name"] == name && entry["limit_id"] == "AncientAugment" })
        );
        let _ = reforge(text, &[name], 2);
    }
    for runes in [names, [names[1], names[0]]] {
        let error = pobr_wasm::reforge_runes_json(&json!({"text":text,"runes":runes}).to_string())
            .unwrap_err();
        assert!(error.contains("augment_limit"));
    }
}

#[test]
fn swapping_defence_runes_recalculates_from_the_known_base_once() {
    ensure_data();
    let text = "Rarity: RARE\nTest\nLayered Vest\nQuality: 20\nSockets: S\nRune: Greater Iron Rune\nEvasion: 9999\nWard: 123\nSpirit: 17\nImplicits: 1\n{rune}18% increased Armour, Evasion and Energy Shield\n+75 to Evasion Rating\n107% increased Evasion Rating\n+25 to maximum Life";
    let out = reforge(text, &["Storm Rune"], 1);
    assert!(!out.contains("Evasion: 9999"));
    assert!(out.contains("Ward: 123"));
    assert!(out.contains("Spirit: 17"));
    let reference = text
        .replace(
            "{rune}18% increased Armour, Evasion and Energy Shield",
            "{rune}+14% to Lightning Resistance",
        )
        .replace("Evasion: 9999\n", "");
    let calculate = |item: &str| -> BTreeMap<String, Value> {
        let response: Value = serde_json::from_str(&pobr_wasm::calculate_build_json(&json!({"character":{"class_name":"Ranger","level":75},"items":[{"slot":"bodyarmour","text":item}]}).to_string()).unwrap()).unwrap();
        response["stats"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|stat| {
                [
                    "Evasion",
                    "EnergyShield",
                    "Life",
                    "Ward",
                    "Spirit",
                    "TotalEHP",
                    "LightningResist",
                ]
                .contains(&stat["id"].as_str().unwrap())
            })
            .map(|stat| (stat["id"].as_str().unwrap().into(), stat["value"].clone()))
            .collect()
    };
    assert_eq!(calculate(&out), calculate(&reference));
    assert!(
        calculate(&out)["Evasion"].as_f64().unwrap() < calculate(text)["Evasion"].as_f64().unwrap()
    );
    let again = reforge(&out, &["Storm Rune"], 1);
    assert_eq!(calculate(&again), calculate(&out));
}
