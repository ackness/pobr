//! keystone 派生 special 表生成（M5b 蓝图 C-1）。
//!
//! 从 `data/<patch>/base/passive_tree.json` 的 keystone 节点确定性派生
//! `generated/special_derived.json`（schema = `special_derived/v1`，与
//! `special_mods/v1` 同结构）：每个 keystone 名 → 一条整行锚定 special 条目，产出
//! `Keystone LIST` mod（字面量值 = 节点 canonical 名，proper-case 保留——与
//! `pobr-core::mod_parser::parse_keystone_grant` 通用路径逐字段等价，避免大小写
//! 失配；keystone 实际 mods 由 env_finalize `merge_keystones` 查注入表展开）。
//!
//! 对照 vendor `ModParser.lua:6151-6158`：`data.keystones` 每名注册为
//! specialModList 整行 key。本步骤以 passive_tree.json keystone 节点为全集
//! （超集无害：多识别几行；差异记 `_meta`）。
//!
//! **byte-stable 纪律**：条目按 keystone 名字典序排序，序列化走统一 pretty
//! 写入（[`crate::write_pretty`]）；同输入重跑 byte-diff 零（纳入 M0 regen-check）。
//!
//! **M6 衔接**：本步骤产物迁入 `tools/precompile-mods` 时 keystone 段须 byte 等价。

use std::path::PathBuf;

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};
use serde::Serialize;

use crate::write_pretty;

pub struct SpecialDerivedArgs {
    /// 已入库的 `passive_tree.json`（节点数组）路径。
    pub tree_json: PathBuf,
    /// 数据根（`data/`）；产物落 `<out>/<patch>/generated/special_derived.json`。
    pub out: PathBuf,
    pub patch: String,
}

// ── 产物 schema（序列化侧；与 pobr-data SpecialModsDef 同结构，独立定义避免
//    给纯数据 crate 加序列化依赖耦合）──

#[derive(Serialize)]
struct DerivedDoc {
    #[serde(rename = "_meta")]
    meta: DerivedMeta,
    entries: Vec<DerivedEntry>,
}

#[derive(Serialize)]
struct DerivedMeta {
    schema: &'static str,
    generator: &'static str,
    note: String,
}

#[derive(Serialize)]
struct DerivedEntry {
    id: String,
    pattern: String,
    mods: Vec<DerivedMod>,
    verified: bool,
    batch: &'static str,
    source_note: String,
}

#[derive(Serialize)]
struct DerivedMod {
    name: &'static str,
    #[serde(rename = "type")]
    mod_type: &'static str,
    value: String,
}

/// snake_case 化 keystone 名做稳定 id（与现有 special_mods.json id 风格一致）。
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_us = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// regex 元字符转义（keystone 名含 `'`/空格等；整行锚定由解释器加 `^...$`）。
fn regex_escape_lower(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    for c in lower.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn run(args: SpecialDerivedArgs) -> Result<String, String> {
    let bytes = std::fs::read(&args.tree_json)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_json.display()))?;
    let nodes: Vec<PassiveNodeDef> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("解析 {} 失败：{e}", args.tree_json.display()))?;

    let mut entries: Vec<DerivedEntry> = nodes
        .iter()
        .filter(|n| n.kind == PassiveNodeKind::Keystone)
        .filter_map(|n| n.name.as_deref())
        .map(|name| DerivedEntry {
            id: format!("keystone_{}", slug(name)),
            // 整行小写 + 转义；解释器编译期包 ^...$（对照 vendor :6155-6158）。
            pattern: regex_escape_lower(name),
            mods: vec![DerivedMod {
                name: "Keystone",
                mod_type: "LIST",
                // proper-case 字面量值——与通用 parse_keystone_grant 逐字段等价。
                value: name.to_string(),
            }],
            verified: false,
            batch: "S0",
            source_note: format!("passive_tree keystone「{name}」（C-1 派生）"),
        })
        .collect();

    // byte-stable：按 id 字典序排序。
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let count = entries.len();

    let doc = DerivedDoc {
        meta: DerivedMeta {
            schema: "special_derived/v1",
            generator: "pobr-data-adapter --emit-special-derived",
            note: format!(
                "{count} 个 keystone（passive_tree kind=Keystone）派生；\
                 对照 vendor ModParser.lua:6151-6158 data.keystones（超集无害）"
            ),
        },
        entries,
    };

    let path = args
        .out
        .join(&args.patch)
        .join("generated")
        .join("special_derived.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建 {} 失败：{e}", parent.display()))?;
    }
    write_pretty(&path, &doc)?;
    Ok(format!(
        "special_derived: {count} keystone 条目 → {}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("Unwavering Stance"), "unwavering_stance");
        assert_eq!(slug("Zealot's Oath"), "zealot_s_oath");
        assert_eq!(slug("Mind Over Matter"), "mind_over_matter");
    }

    #[test]
    fn escape_lowers_and_escapes() {
        assert_eq!(regex_escape_lower("Zealot's Oath"), "zealot's oath");
        assert_eq!(regex_escape_lower("Eldritch Battery"), "eldritch battery");
    }
}
