//! 一次性探针（审计用）：枚举树节点词条 + demo build 装备/珠宝词条，
//! 跑 en→zh 显示翻译，dump 未命中行到 target/zh_gap_report.txt。
//! 跑法：cargo test -p pobr-wasm --test zh_gap_probe -- --nocapture --ignored

use std::collections::BTreeMap;

use pobr_gamedata::repo_data_root;
use serde_json::Value;

/// 前端同款标注剥离：`[A|B]` 取 B、`[A]` 取 A、`{tag}` 删除。
fn strip_markup(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                }
            }
            '[' => {
                let mut inner = String::new();
                for d in chars.by_ref() {
                    if d == ']' {
                        break;
                    }
                    inner.push(d);
                }
                match inner.split_once('|') {
                    Some((_, b)) => out.push_str(b),
                    None => out.push_str(&inner),
                }
            }
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

const STRUCT_PREFIXES: &[&str] = &[
    "Rarity:",
    "Unique ID:",
    "Item Level:",
    "Quality:",
    "Sockets:",
    "Rune:",
    "Soul Core:",
    "LevelReq:",
    "Requires Level",
    "Implicits:",
    "Energy Shield:",
    "Armour:",
    "Evasion:",
    "Spirit:",
    "Charm Slots:",
    "Radius:",
    "Limited to:",
    "League:",
    "Corrupted",
    "Ward:",
];

#[test]
#[ignore = "audit probe, run on demand"]
fn dump_untranslated_lines() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");

    // 来源 → 待翻行集合（去重，记来源类别）。
    let mut lines: BTreeMap<String, &'static str> = BTreeMap::new();

    // 1. 树节点词条（全量）。
    let tree: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("base/passive_tree.json")).expect("tree"),
    )
    .expect("tree json");
    for node in &tree {
        for stat in node["stats"].as_array().into_iter().flatten() {
            let s = strip_markup(stat.as_str().unwrap_or(""));
            if !s.is_empty() && s.chars().any(|c| c.is_ascii_alphabetic()) {
                lines.entry(s).or_insert("tree");
            }
        }
    }

    // 2. demo build 装备/珠宝词条行（真实物品语料）。
    let repo_root = repo_data_root();
    let builds_dir = repo_root
        .parent()
        .expect("root")
        .join("examples/demo-bd-test/builds");
    for entry in std::fs::read_dir(&builds_dir).expect("builds dir") {
        let xml_path = entry.expect("entry").path().join("decoded.xml");
        let Ok(xml) = std::fs::read_to_string(&xml_path) else {
            continue;
        };
        let items = pobr_build::parse_raw_items_view(&xml).expect("items view");
        let all_texts = items
            .equipped
            .iter()
            .map(|(_, t)| t)
            .chain(items.jewels.iter())
            .chain(items.flasks.iter().map(|(_, t)| t));
        for text in all_texts {
            for raw in text.lines() {
                let s = strip_markup(raw);
                if s.is_empty()
                    || !s.chars().any(|c| c.is_ascii_alphabetic())
                    || STRUCT_PREFIXES.iter().any(|p| s.starts_with(p))
                {
                    continue;
                }
                lines.entry(s).or_insert("item");
            }
        }
    }

    // 3. 批量翻译，收集未命中（返回原文）。
    let inputs: Vec<&String> = lines.keys().collect();
    let payload = serde_json::to_string(&inputs).unwrap();
    let out = pobr_wasm::translate_lines_to_zh_cn_json(&payload).expect("translate");
    let translated: Vec<String> = serde_json::from_str(&out).expect("parse out");

    let mut misses: Vec<(&str, &str)> = Vec::new();
    for (i, key) in lines.keys().enumerate() {
        if translated[i] == *key {
            misses.push((lines[key], key));
        }
    }
    misses.sort();

    let report: String = misses
        .iter()
        .map(|(src, line)| format!("[{src}] {line}\n"))
        .collect();
    let report_path = std::env::temp_dir().join("zh_gap_report.txt");
    std::fs::write(&report_path, &report).expect("write report");
    let tree_miss = misses.iter().filter(|(s, _)| *s == "tree").count();
    let item_miss = misses.iter().filter(|(s, _)| *s == "item").count();
    println!(
        "total unique lines: {}, misses: {} (tree {}, item {}) -> {}",
        lines.len(),
        misses.len(),
        tree_miss,
        item_miss,
        report_path.display()
    );
}
