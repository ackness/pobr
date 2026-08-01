//! `extract-lua --what stat-descriptions`：vendor PoB2
//! `Data/StatDescriptions/*.lua` → 每个 stat_id 的 canonical 显示文本 →
//! `data/<版本>/overlay/stat_descriptions.json`（「stat_id → Modifier
//! 第二通道」的数据面）。
//!
//! 职责切分与 [`crate::extract_stat_map`] 一致：Lua 引导脚本
//! （`extract_stat_descriptions.lua`，编译期内嵌）在最小环境下加载描述表、对
//! 每个 stat_id 喂代表值（V=1）渲染文本，以 JSONL 输出（每行一条 single 文本
//! 行或一条 compound 模板）；Rust 侧统一做 scope 分段、行序归并（[`BTreeMap`]
//! 字典序）与整体序列化，保证同输入重跑 **byte-stable**。
//!
//! 消费侧 schema 单一来源 = [`pobr_data::catalog::stat_descriptions`]（生成 /
//! 消费两侧共用同一 serde 形状，防字段漂移）；precedence（child scope 覆盖
//! parent）与「哪条 stat_id 受支持」的判定是消费侧 §B 生成器 / `parse_mod_engine`
//! 的职责，本模块不做。

use std::collections::BTreeMap;
use std::io;

use pobr_data::catalog::stat_descriptions::{CompoundDescription, StatDescriptionsDef};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）。
const BOOTSTRAP_LUA: &str = include_str!("extract_stat_descriptions.lua");

/// 当前 overlay 文档 schema 标识（字段演化时递增）。
pub const STAT_DESCRIPTIONS_SCHEMA: &str = "stat_descriptions/v1";

/// 默认抽取 scope（tree 通道最相关：root + passive + presence/aura）。
/// 其余 StatDescriptions 文件（skill/gem/monster/advanced_mod）按需经 `--files` 加。
pub const DEFAULT_STAT_DESC_FILES: &[&str] = &[
    "stat_descriptions",
    "passive_skill_stat_descriptions",
    "passive_skill_aura_stat_descriptions",
];

/// 引导脚本输出的单行 JSONL：一条 single 文本行 或 一条 compound 模板。
///
/// - single 渲染：`{stat, scope, text, line, compound:false}`
/// - single 无变体：`{stat, scope, compound:false, unrendered:true}`
/// - compound：`{stat, scope, compound:true, member_stats, template}`
#[derive(Debug, Clone, Deserialize)]
pub struct StatDescRow {
    /// stat 稳定 id（compound 行 = descriptor 首个 stat_id）。
    pub stat: String,
    /// 来源 scope 名（StatDescriptions 文件名，不含 `.lua`）。
    pub scope: String,
    /// 是否多 stat（compound）描述符。
    pub compound: bool,
    /// 渲染出的单行文本（仅 single 且可渲染）。
    #[serde(default)]
    pub text: Option<String>,
    /// 行序号（1-based；同一 stat 的多行描述按此排序）。
    #[serde(default)]
    pub line: Option<u32>,
    /// 无可渲染变体标记（仅 single）。
    #[serde(default)]
    pub unrendered: bool,
    /// compound 绑定的全部 stat_id（仅 compound）。
    #[serde(default)]
    pub member_stats: Vec<String>,
    /// compound 原样模板（仅 compound）。
    #[serde(default)]
    pub template: Option<String>,
}

/// 完整 overlay 文档（生成侧；`_meta` 头部 + 消费侧 [`StatDescriptionsDef`] 平铺）。
#[derive(Debug, Serialize, Deserialize)]
pub struct StatDescriptionsDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）。
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 描述本体（按 scope 分段，BTreeMap 字典序保证确定性）。
    #[serde(flatten)]
    pub def: StatDescriptionsDef,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_stat_descriptions(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_luajit_jsonl::<StatDescRow>(args, BOOTSTRAP_LUA)?;
    let meta = build_meta(args)?;
    assemble_stat_descriptions_document(meta, rows)
}

/// 组装最终文档：JSONL 行 → 各 scope 的 single / compound / unrendered 三段。
///
/// single 文本行先归并到 `BTreeMap<line, text>`（行序确定，与输入次序无关），再
/// 展平为 `Vec<String>`——保证同输入 byte-stable。重复键（同 scope 同 stat 同
/// line，或 compound 重复）报错不静默覆盖。
pub fn assemble_stat_descriptions_document(
    meta: OverlayMeta,
    rows: Vec<StatDescRow>,
) -> io::Result<String> {
    // 中间态：scope → stat → line → text（行序归并）。
    let mut single_lines: BTreeMap<String, BTreeMap<String, BTreeMap<u32, String>>> =
        BTreeMap::new();
    let mut def = StatDescriptionsDef::default();

    for row in rows {
        let scope = def.scopes.entry(row.scope.clone()).or_default();
        if row.compound {
            let template = row.template.clone().ok_or_else(|| {
                io::Error::other(format!(
                    "stat-descriptions compound 行缺 template：{}",
                    row.stat
                ))
            })?;
            let entry = CompoundDescription {
                member_stats: row.member_stats.clone(),
                template,
            };
            if scope.compound.insert(row.stat.clone(), entry).is_some() {
                return Err(io::Error::other(format!(
                    "stat-descriptions 抽取重复 compound 键：{}::{}",
                    row.scope, row.stat
                )));
            }
        } else if row.unrendered {
            scope.unrendered.insert(row.stat.clone());
        } else {
            let text = row.text.clone().ok_or_else(|| {
                io::Error::other(format!(
                    "stat-descriptions single 行缺 text：{}::{}",
                    row.scope, row.stat
                ))
            })?;
            let line = row.line.unwrap_or(1);
            let slot = single_lines
                .entry(row.scope.clone())
                .or_default()
                .entry(row.stat.clone())
                .or_default();
            if slot.insert(line, text).is_some() {
                return Err(io::Error::other(format!(
                    "stat-descriptions 抽取重复 single 行：{}::{} line {line}",
                    row.scope, row.stat
                )));
            }
        }
    }

    // 行序归并展平到消费侧形状。
    for (scope_name, stats) in single_lines {
        let scope = def.scopes.entry(scope_name).or_default();
        for (stat, lines) in stats {
            scope.single.insert(stat, lines.into_values().collect());
        }
    }

    let doc = StatDescriptionsDoc { meta, def };
    let mut json = serde_json::to_string_pretty(&doc).map_err(io::Error::other)?;
    json.push('\n');
    Ok(json)
}

/// 读取 vendor 版本文件并构建 `_meta`（与 `extract_lua::build_meta` 同约定）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;

    let extracted_files = args
        .files
        .iter()
        .map(|name| format!("Data/StatDescriptions/{name}.lua"))
        .collect();

    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what stat-descriptions --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: STAT_DESCRIPTIONS_SCHEMA.to_string(),
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
            schema: STAT_DESCRIPTIONS_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "test".to_string(),
            extracted_files: vec![],
            regen_command: "test".to_string(),
        }
    }

    fn single(scope: &str, stat: &str, text: &str, line: u32) -> StatDescRow {
        StatDescRow {
            stat: stat.to_string(),
            scope: scope.to_string(),
            compound: false,
            text: Some(text.to_string()),
            line: Some(line),
            unrendered: false,
            member_stats: vec![],
            template: None,
        }
    }

    /// single 多行按 line 序展平；多 scope 各归各段；键字典序（BTreeMap 保证）。
    #[test]
    fn assembles_single_lines_in_order() {
        let json = assemble_stat_descriptions_document(
            meta(),
            vec![
                single(
                    "stat_descriptions",
                    "additional_strength",
                    "+1 to Strength",
                    1,
                ),
                // 故意乱序输入：line 2 在 line 1 之前。
                single("stat_descriptions", "two_line", "second", 2),
                single("stat_descriptions", "two_line", "first", 1),
                single(
                    "passive_skill_stat_descriptions",
                    "z_passive",
                    "passive text",
                    1,
                ),
            ],
        )
        .unwrap();
        let doc: StatDescriptionsDoc = serde_json::from_str(&json).unwrap();
        let root = &doc.def.scopes["stat_descriptions"];
        assert_eq!(root.single["additional_strength"], vec!["+1 to Strength"]);
        // 乱序输入仍按 line 序展平。
        assert_eq!(root.single["two_line"], vec!["first", "second"]);
        assert!(
            doc.def
                .scopes
                .contains_key("passive_skill_stat_descriptions")
        );
    }

    /// compound 行原样保留 template + member_stats。
    #[test]
    fn keeps_compound_template_verbatim() {
        let row = StatDescRow {
            stat: "a_stat".to_string(),
            scope: "stat_descriptions".to_string(),
            compound: true,
            text: None,
            line: None,
            unrendered: false,
            member_stats: vec!["a_stat".to_string(), "b_stat".to_string()],
            template: Some("Deal {0} damage to {1} targets".to_string()),
        };
        let json = assemble_stat_descriptions_document(meta(), vec![row]).unwrap();
        let doc: StatDescriptionsDoc = serde_json::from_str(&json).unwrap();
        let c = &doc.def.scopes["stat_descriptions"].compound["a_stat"];
        assert_eq!(c.member_stats, vec!["a_stat", "b_stat"]);
        assert_eq!(c.template, "Deal {0} damage to {1} targets");
    }

    /// unrendered 行进诊断集。
    #[test]
    fn records_unrendered_stats() {
        let row = StatDescRow {
            stat: "weird_stat".to_string(),
            scope: "stat_descriptions".to_string(),
            compound: false,
            text: None,
            line: None,
            unrendered: true,
            member_stats: vec![],
            template: None,
        };
        let json = assemble_stat_descriptions_document(meta(), vec![row]).unwrap();
        let doc: StatDescriptionsDoc = serde_json::from_str(&json).unwrap();
        assert!(
            doc.def.scopes["stat_descriptions"]
                .unrendered
                .contains("weird_stat")
        );
    }

    /// 重复 single 行（同 scope 同 stat 同 line）→ 报错不静默覆盖。
    #[test]
    fn duplicate_single_line_errors_out() {
        let result = assemble_stat_descriptions_document(
            meta(),
            vec![
                single("stat_descriptions", "dup", "a", 1),
                single("stat_descriptions", "dup", "b", 1),
            ],
        );
        assert!(result.is_err());
    }

    /// 字典序：a_stat 文本位置先于 z_stat（byte-stable 排序的可见证据）。
    #[test]
    fn output_is_lexicographically_sorted() {
        let json = assemble_stat_descriptions_document(
            meta(),
            vec![
                single("stat_descriptions", "z_stat", "z", 1),
                single("stat_descriptions", "a_stat", "a", 1),
            ],
        )
        .unwrap();
        assert!(json.find("a_stat").unwrap() < json.find("z_stat").unwrap());
    }
}
