//! `overlay/special_mods.json` 的 pattern 编译校验（pre-M5b）。
//!
//! schema 契约（M5b 蓝图 §2.1）：pattern 是 Rust regex 语法子集，载入期编译
//! 失败 = fail fast。本 crate 持有 regex 依赖，故编译校验落在这里；
//! 解释器（`SpecialModRules::compile` 的完整错误路径）归 M5b 主波 B-2。

use std::path::Path;

use regex::Regex;

/// 仓库 data 根（tools/sync-pob-catalog/ → 上两级）。
fn special_mods_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/4.5.0.3.4/overlay/special_mods.json")
}

/// 全部 pattern 可被 regex crate 编译（整行锚定由引擎统一加 `^...$`，
/// 此处按同口径编译）；捕获组数 ≥ mods 内引用的最大 `$n`。
#[test]
fn all_patterns_compile_and_captures_cover_refs() {
    let text = std::fs::read_to_string(special_mods_path()).expect("special_mods.json 在库");
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let entries = doc["entries"].as_array().expect("entries 数组");
    assert!(!entries.is_empty());
    for entry in entries {
        let id = entry["id"].as_str().unwrap();
        let pattern = entry["pattern"].as_str().unwrap();
        let re = Regex::new(&format!("^{pattern}$"))
            .unwrap_or_else(|e| panic!("{id}: pattern 编译失败：{e}"));
        // mods JSON 文本里出现的最大 $n 不得超过捕获组数
        let mods_text = entry["mods"].to_string();
        let max_ref = (1..=9)
            .rev()
            .find(|n| {
                mods_text.contains(&format!("\"${n}\"")) || mods_text.contains(&format!("${n}\""))
            })
            .unwrap_or(0);
        assert!(
            re.captures_len() > max_ref,
            "{id}: 捕获组 {} 个，引用了 ${max_ref}",
            re.captures_len() - 1
        );
    }
}

/// 数值捕获形态统一：`(\d+)` / `(\d+(?:\.\d+)?)` / `([+-]\d+)` / `(\+?\d+)`
/// 之外的捕获必须是显式闭集（不含 `\d`）——DSL 硬边界的「禁开放捕获」。
#[test]
fn captures_are_numeric_or_closed_sets() {
    let text = std::fs::read_to_string(special_mods_path()).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    const NUMERIC_FORMS: [&str; 4] = [r"(\d+)", r"(\d+(?:\.\d+)?)", r"([+-]\d+)", r"(\+?\d+)"];
    for entry in doc["entries"].as_array().unwrap() {
        let id = entry["id"].as_str().unwrap();
        let pattern = entry["pattern"].as_str().unwrap();
        // 逐捕获组扫描（顶层括号，忽略 (?: 非捕获组）
        let bytes = pattern.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'(' && !pattern[i..].starts_with("(?:") {
                // 找平衡右括号
                let mut depth = 1;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'(' {
                        depth += 1;
                    } else if bytes[j] == b')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let group = &pattern[i..j];
                let is_numeric = NUMERIC_FORMS.contains(&group);
                assert!(
                    is_numeric || !group.contains("\\d"),
                    "{id}: 捕获组 {group:?} 既非统一数值形态也非闭集"
                );
                i = j;
            } else {
                i += 1;
            }
        }
    }
}
