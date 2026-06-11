//! `extract-lua --what stat-map`：vendor PoB2 `Data/SkillStatMap.lua`（954 条
//! 全局 stat → modifier 构造器映射）+ `Data/Skills/{act_*,sup_*,other}.lua` 各
//! statSet 的 `statMap` 字段（per-set 覆盖）→
//! `data/<版本>/overlay/skill_stat_map.json`（M1-T2.1，缺口 18-G3 / 15-G2 的
//! 数据面）。
//!
//! 职责切分与 [`crate::extract_lua`] 一致：Lua 引导脚本
//! （`extract_skill_stat_map.lua`，编译期内嵌）只负责**忠实抽取**并以 JSONL
//! 输出（每行 `{"scope":"global"|"set",...,"entry":{...}}`）；Rust 侧统一做
//! 排序（[`std::collections::BTreeMap`] 字典序）、数字最短往返表示与整体文档
//! 序列化，保证同输入重跑 **byte-stable**。
//!
//! 消费侧 schema 单一来源 = [`pobr_data::catalog::stat_map`]（生成 / 消费两侧
//! 共用同一 serde 形状，防字段漂移）；语义筛选（哪些 tag / 构造器受支持）是
//! `pobr-core::rules::stat_map_engine` 的职责，本模块不做。

use std::io;

use pobr_data::catalog::stat_map::{SkillStatMapDef, StatMapEntry};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）。
const BOOTSTRAP_LUA: &str = include_str!("extract_skill_stat_map.lua");

/// 当前 overlay 文档 schema 标识（字段演化时递增）。
pub const SKILL_STAT_MAP_SCHEMA: &str = "skill_stat_map/v1";

/// 引导脚本输出的单行 JSONL：一条 statMap 映射（global 或 per-set 作用域）。
#[derive(Debug, Clone, Deserialize)]
pub struct StatMapRow {
    /// 作用域：`"global"`（SkillStatMap.lua）或 `"set"`（Data/Skills 各 statSet）。
    pub scope: String,
    /// stat 稳定 id。
    pub stat: String,
    /// granted effect id（仅 `scope == "set"`）。
    #[serde(default)]
    pub effect: Option<String>,
    /// statSet 序号（vendor `statSets` 数组 1-based 下标；仅 `scope == "set"`）。
    #[serde(default)]
    pub stat_set: Option<u32>,
    /// 映射条目（schema 形状 = 消费侧 [`StatMapEntry`]，忠实纯表化）。
    pub entry: StatMapEntry,
}

/// 完整 overlay 文档（生成侧；`_meta` 头部 + 消费侧 [`SkillStatMapDef`] 平铺）。
#[derive(Debug, Serialize, Deserialize)]
pub struct StatMapDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）。
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 映射本体（global + per_stat_set，BTreeMap 字典序保证确定性）。
    #[serde(flatten)]
    pub def: SkillStatMapDef,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_stat_map(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_luajit_jsonl::<StatMapRow>(args, BOOTSTRAP_LUA)?;
    let meta = build_meta(args)?;
    assemble_stat_map_document(meta, rows)
}

/// 组装最终文档：JSONL 行 → `global` / `per_stat_set` 两段（键全部经 BTreeMap
/// 字典序）+ serde_json 统一序列化（同输入必然同输出）。
///
/// 重复键防御：同一 (作用域, stat) 出现两条 → 报错（vendor 表为字典，重复说明
/// 引导脚本或 vendor 数据异常，宁可失败不可静默后写覆盖）。
pub fn assemble_stat_map_document(meta: OverlayMeta, rows: Vec<StatMapRow>) -> io::Result<String> {
    let mut def = SkillStatMapDef::default();
    for row in rows {
        match row.scope.as_str() {
            "global" => {
                if def.global.insert(row.stat.clone(), row.entry).is_some() {
                    return Err(io::Error::other(format!(
                        "stat-map 抽取重复 global 键：{}",
                        row.stat
                    )));
                }
            }
            "set" => {
                let (Some(effect), Some(set)) = (row.effect.clone(), row.stat_set) else {
                    return Err(io::Error::other(format!(
                        "stat-map 抽取 set 行缺 effect/stat_set 字段：stat={}",
                        row.stat
                    )));
                };
                let slot = def
                    .per_stat_set
                    .entry(effect.clone())
                    .or_default()
                    .entry(set.to_string())
                    .or_default();
                if slot.insert(row.stat.clone(), row.entry).is_some() {
                    return Err(io::Error::other(format!(
                        "stat-map 抽取重复 per-set 键：{effect}#{set} {}",
                        row.stat
                    )));
                }
            }
            other => {
                return Err(io::Error::other(format!(
                    "stat-map 抽取未知 scope：{other}"
                )));
            }
        }
    }
    let doc = StatMapDoc { meta, def };
    let mut json = serde_json::to_string_pretty(&doc).map_err(io::Error::other)?;
    json.push('\n');
    Ok(json)
}

/// 读取 vendor 版本文件并构建 `_meta`（与 `extract_lua::build_meta` 同约定：
/// `regen_command` 写 canonical 相对路径，跨机器重跑产物 byte 一致）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;

    // 抽取源 = 全局表 + 各技能数据文件（区别于 skill-overrides：多一个全局表）。
    let mut extracted_files = vec!["Data/SkillStatMap.lua".to_string()];
    extracted_files.extend(
        args.files
            .iter()
            .map(|name| format!("Data/Skills/{name}.lua")),
    );

    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what stat-map --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: SKILL_STAT_MAP_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> OverlayMeta {
        OverlayMeta {
            schema: SKILL_STAT_MAP_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "test".to_string(),
            extracted_files: vec![],
            regen_command: "test".to_string(),
        }
    }

    fn row(scope: &str, stat: &str, effect: Option<&str>, set: Option<u32>) -> StatMapRow {
        StatMapRow {
            scope: scope.to_string(),
            stat: stat.to_string(),
            effect: effect.map(str::to_string),
            stat_set: set,
            entry: StatMapEntry::default(),
        }
    }

    /// global / set 两个作用域各归各段；键按字典序（BTreeMap 保证）。
    #[test]
    fn assembles_global_and_per_set_sections() {
        let json = assemble_stat_map_document(
            meta(),
            vec![
                row("global", "b_stat", None, None),
                row("global", "a_stat", None, None),
                row("set", "x_stat", Some("CometPlayer"), Some(1)),
            ],
        )
        .unwrap();
        let doc: StatMapDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(
            doc.def.global.keys().collect::<Vec<_>>(),
            vec!["a_stat", "b_stat"]
        );
        assert!(doc.def.per_stat_set["CometPlayer"]["1"].contains_key("x_stat"));
        // 字典序：a_stat 文本位置先于 b_stat（byte-stable 排序的可见证据）。
        assert!(json.find("a_stat").unwrap() < json.find("b_stat").unwrap());
    }

    /// 重复键（同作用域同 stat）→ 报错不静默覆盖。
    #[test]
    fn duplicate_keys_error_out() {
        let result = assemble_stat_map_document(
            meta(),
            vec![
                row("global", "dup", None, None),
                row("global", "dup", None, None),
            ],
        );
        assert!(result.is_err());
    }

    /// set 行缺 effect / stat_set → 报错。
    #[test]
    fn set_row_missing_fields_errors_out() {
        assert!(assemble_stat_map_document(meta(), vec![row("set", "s", None, None)]).is_err());
    }

    /// 未知 scope → 报错。
    #[test]
    fn unknown_scope_errors_out() {
        assert!(assemble_stat_map_document(meta(), vec![row("woot", "s", None, None)]).is_err());
    }
}
