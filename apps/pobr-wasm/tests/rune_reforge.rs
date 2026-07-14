//! 符文槽编辑集成测试（真实数据）：目录带中文名边车；重插重写文本
//! （命名行 + `{rune}` 词条行 + Implicits 计数）且引擎可直接消费。

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

fn ensure_data() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
}

/// 三符文位胸甲（PoB2 导出形状：Rune 命名行 + Implicits 窗口内 {rune} 词条行）。
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
    let json = pobr_wasm::rune_catalog_json().expect("catalog");
    let entries: Vec<Value> = serde_json::from_str(&json).expect("parse");
    assert!(
        entries.len() > 200,
        "符文目录应有全量条目，实得 {}",
        entries.len()
    );
    let iron = entries
        .iter()
        .find(|e| e["name"] == "Iron Rune")
        .expect("Iron Rune 应在目录");
    assert!(
        iron["name_zh_cn"].as_str().is_some_and(|s| !s.is_empty()),
        "Iron Rune 应有简中名，实得 {:?}",
        iron["name_zh_cn"]
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

    // 旧符文命名行剔除、新命名行按序插入 Sockets 之后。
    assert!(
        !text.contains("Rune: Iron Rune\n"),
        "旧命名行应被替换：\n{text}"
    );
    assert!(text.contains("Rune: Greater Iron Rune"));
    assert!(text.contains("Rune: Adept Rune"));
    // 旧 {rune} 词条（20%）剔除，新词条带 {rune} 前缀（Adept Rune armour = +9 敏捷）。
    assert!(
        !text.contains("{rune}20% increased Armour"),
        "旧符文词条应剔除：\n{text}"
    );
    assert!(
        text.contains("{rune}+9 to Dexterity"),
        "新符文词条应写入：\n{text}"
    );
    // Implicits 计数 = 旧 1 - 旧 rune 行 1 + 新 rune 行 4（Greater Iron Rune 在
    // armour + body armour 两键共 3 行，broad/specific 双命中同 PoB2；Adept 1 行）。
    assert!(text.contains("{rune}Bonded: +20 to maximum Life"));
    assert!(
        text.contains("Implicits: 4"),
        "Implicits 计数应修正：\n{text}"
    );
    // 非符文词条原样保留。
    assert!(text.contains("+167 to Armour"));
}

#[test]
fn reforge_resizes_sockets() {
    ensure_data();
    // 3 孔 → 2 孔（带 1 颗符文），Sockets 行重写。
    let shrink = json!({ "text": ARMOUR_WITH_RUNES, "runes": ["Iron Rune"], "sockets": 2 });
    let out = pobr_wasm::reforge_runes_json(&shrink.to_string()).expect("shrink");
    let text: Value = serde_json::from_str(&out).unwrap();
    let text = text["text"].as_str().unwrap();
    assert!(text.contains("Sockets: S S\n"), "孔数应重写为 2：\n{text}");

    // 无 Sockets 行的物品直接加 1 孔：新行插入且可镶嵌。
    let no_sockets =
        "Rarity: RARE\nApocalypse Pelt\nFalconer's Jacket\nItem Level: 81\n+190 to maximum Life";
    let grow = json!({ "text": no_sockets, "runes": ["Iron Rune"], "sockets": 1 });
    let out = pobr_wasm::reforge_runes_json(&grow.to_string()).expect("grow");
    let text: Value = serde_json::from_str(&out).unwrap();
    let text = text["text"].as_str().unwrap();
    assert!(text.contains("Sockets: S\n"), "应新增 Sockets 行：\n{text}");
    assert!(text.contains("Rune: Iron Rune"));

    // 减到 0 孔：Sockets 行与符文全部移除。
    let zero = json!({ "text": ARMOUR_WITH_RUNES, "runes": [], "sockets": 0 });
    let out = pobr_wasm::reforge_runes_json(&zero.to_string()).expect("zero");
    let text: Value = serde_json::from_str(&out).unwrap();
    let text = text["text"].as_str().unwrap();
    assert!(
        !text.contains("Sockets:"),
        "0 孔应移除 Sockets 行：\n{text}"
    );
    assert!(!text.contains("Rune:"));
    assert!(
        text.contains("Implicits: 0"),
        "rune 词条清空后计数归零：\n{text}"
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
