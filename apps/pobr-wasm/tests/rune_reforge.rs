//! Integration tests for rune socket editing (real data): the catalog
//! carries the Chinese name sidecar; re-socketing rewrites text (named
//! lines plus `{rune}` mod lines plus the Implicits count), and the result is directly consumable by the engine.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

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
    // Implicits count = old 1 - old rune line 1 + new rune lines 4 (Greater
    // Iron Rune has 3 lines total across the armour + body armour keys,
    // both broad/specific hitting, matching PoB2; Adept has 1 line).
    assert!(text.contains("{rune}Bonded: +20 to maximum Life"));
    assert!(
        text.contains("Implicits: 4"),
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
