//! `gen-stat-id-map`：把段 A 的 `overlay/stat_descriptions.json`（stat_id →
//! canonical 文本）逐条喂 `parse_mod_engine`，固化成 `overlay/stat_id_map.json`
//! （M6 E/F 段 B：stat_id → Modifier 第二通道的 modifier 模板）。
//!
//! 与 luajit 抽取目标不同：本命令**不执行 luajit**，纯消费两份已生成的 overlay
//! （stat_descriptions + mod_parser_rules），跑引擎离线派生。引擎变更后需 regen
//! （产物随引擎能力演化）。
//!
//! 职责切分：模板把**结构**（name/mod_type/tags/flags）与**系数**（coefficient，
//! V=1 解析值）分列；tag 串走 `pobr_core::mod_parser::canonical_tags` 单源序列化。
//! 一条 stat_id 的全部文本行须**全部**解析成功才入 `mapped`，否则整条入
//! `unsupported`（通道回退文本 / special）。排序 / byte-stable 由 BTreeMap +
//! serde_json 保证。

use std::fs;
use std::io;
use std::path::Path;

use pobr_core::mod_parser::{CompiledParserRules, ParseStatus, canonical_tags, parse_mod_engine};
use pobr_core::{ModValue, Modifier};
use pobr_data::catalog::parser_rules::ModParserRulesDoc;
use pobr_data::catalog::stat_id_map::{ScopeStatIdMap, StatIdMapDef, StatIdModTemplate};
use serde::{Deserialize, Serialize};

use crate::extract_lua::OverlayMeta;
use crate::extract_stat_descriptions::StatDescriptionsDoc;

/// 当前 overlay 文档 schema 标识。
pub const STAT_ID_MAP_SCHEMA: &str = "stat_id_map/v1";

/// 完整 overlay 文档（`_meta` 头部 + [`StatIdMapDef`] 平铺）。
#[derive(Debug, Serialize, Deserialize)]
pub struct StatIdMapDoc {
    /// 头部元信息（serde 落为 `_meta`）。
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 映射本体（按 scope 分段）。
    #[serde(flatten)]
    pub def: StatIdMapDef,
}

/// 把单条 [`Modifier`] 转成可序列化模板（结构 + 系数分列）。
fn template(m: &Modifier) -> StatIdModTemplate {
    let (coefficient, value_kind) = match &m.value {
        ModValue::Number(n) => (Some(*n), None),
        ModValue::Bool(b) => (None, Some(format!("flag:{b}"))),
        ModValue::Text(s) => (None, Some(format!("text:{s}"))),
        ModValue::NestedMods(_) => (None, Some("nested".to_string())),
    };
    StatIdModTemplate {
        name: m.name.as_str().to_string(),
        mod_type: format!("{:?}", m.mod_type),
        coefficient,
        value_kind,
        tags: canonical_tags(&m.tags),
        flags: m.flags.bits(),
        keyword_flags: m.keyword_flags.bits(),
    }
}

/// 执行生成，返回最终（byte-stable 的）JSON 文本。
pub fn run_gen_stat_id_map(overlay_dir: &Path, out_for_meta: Option<String>) -> io::Result<String> {
    // 源 1：stat_descriptions.json（含 _meta：vendor commit 透传）。
    let descs_text = fs::read_to_string(overlay_dir.join("stat_descriptions.json"))?;
    let descs: StatDescriptionsDoc = serde_json::from_str(&descs_text).map_err(io::Error::other)?;

    // 源 2：mod_parser_rules.json（serde 忽略 _meta，编译为引擎规则）。
    let rules_text = fs::read_to_string(overlay_dir.join("mod_parser_rules.json"))?;
    let rules_doc: ModParserRulesDoc =
        serde_json::from_str(&rules_text).map_err(io::Error::other)?;
    let rules = CompiledParserRules::compile(&rules_doc)
        .map_err(|e| io::Error::other(format!("编译 mod_parser_rules 失败：{e:?}")))?;

    let def = build_map(&descs.def, &rules);
    let meta = build_meta(&descs.meta, out_for_meta);

    let doc = StatIdMapDoc { meta, def };
    let mut json = serde_json::to_string_pretty(&doc).map_err(io::Error::other)?;
    json.push('\n');
    Ok(json)
}

/// 逐 scope / stat_id 解析文本行 → modifier 模板，组装 [`StatIdMapDef`]。
///
/// 一条 stat_id 的全部行须全部 Parsed 且各产出非空 mod，才入 `mapped`；任一行
/// Unsupported / 空 → 整条入 `unsupported`（保守：通道宁回退不可错算）。
pub fn build_map(
    descs: &pobr_data::catalog::stat_descriptions::StatDescriptionsDef,
    rules: &CompiledParserRules,
) -> StatIdMapDef {
    let mut def = StatIdMapDef::default();
    for (scope_name, scope) in &descs.scopes {
        let mut out_scope = ScopeStatIdMap::default();
        for (stat_id, lines) in &scope.single {
            let mut templates = Vec::new();
            let mut all_ok = true;
            for line in lines {
                let outcome = parse_mod_engine(line, rules);
                if outcome.status != ParseStatus::Parsed || outcome.mods.is_empty() {
                    all_ok = false;
                    break;
                }
                templates.extend(outcome.mods.iter().map(template));
            }
            if all_ok && !templates.is_empty() {
                out_scope.mapped.insert(stat_id.clone(), templates);
            } else {
                out_scope.unsupported.insert(stat_id.clone());
            }
        }
        def.scopes.insert(scope_name.clone(), out_scope);
    }
    def
}

/// 构建 `_meta`：vendor commit 从源 stat_descriptions 的 _meta 透传（本表派生自
/// 那次抽取）；extracted_files 记两份源 overlay；regen_command 记本命令。
fn build_meta(source_meta: &OverlayMeta, out_for_meta: Option<String>) -> OverlayMeta {
    let mut regen =
        "cargo run -p sync-pob-catalog -- gen-stat-id-map --overlay-dir data/<version>/overlay"
            .to_string();
    if let Some(out) = &out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    OverlayMeta {
        schema: STAT_ID_MAP_SCHEMA.to_string(),
        generator: "sync-pob-catalog gen-stat-id-map".to_string(),
        vendor: source_meta.vendor.clone(),
        vendor_commit: source_meta.vendor_commit.clone(),
        vendor_commit_subject: source_meta.vendor_commit_subject.clone(),
        extracted_files: vec![
            "overlay/stat_descriptions.json".to_string(),
            "overlay/mod_parser_rules.json".to_string(),
        ],
        regen_command: regen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_descriptions::{ScopeDescriptions, StatDescriptionsDef};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn descs_with(scope: &str, entries: &[(&str, &str)]) -> StatDescriptionsDef {
        let mut single = BTreeMap::new();
        for (id, text) in entries {
            single.insert(id.to_string(), vec![text.to_string()]);
        }
        let mut scopes = BTreeMap::new();
        scopes.insert(
            scope.to_string(),
            ScopeDescriptions {
                single,
                ..Default::default()
            },
        );
        StatDescriptionsDef { scopes }
    }

    /// 加载真实 overlay 规则（免 test-rules feature；与 stat_desc_parse_rate 同法）。
    fn real_rules() -> CompiledParserRules {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/4.5.0.3.4/overlay/mod_parser_rules.json");
        let doc: ModParserRulesDoc =
            serde_json::from_str(&fs::read_to_string(path).expect("read rules")).expect("doc");
        CompiledParserRules::compile(&doc).expect("compile")
    }

    /// 可解析文本 → mapped；不可解析 → unsupported（保守整条回退）。
    #[test]
    fn maps_parseable_and_quarantines_rest() {
        let rules = real_rules();
        let descs = descs_with(
            "stat_descriptions",
            &[
                ("additional_strength", "+1 to Strength"),
                ("gibberish_stat", "Walk the Paths Not Taken"),
            ],
        );
        let def = build_map(&descs, &rules);
        let scope = &def.scopes["stat_descriptions"];
        // strength 必进 mapped（标准通用词条）。
        assert!(scope.mapped.contains_key("additional_strength"));
        let tmpl = &scope.mapped["additional_strength"][0];
        assert_eq!(tmpl.name, "Strength");
        assert_eq!(tmpl.mod_type, "Base");
        // gibberish 必进 unsupported（无规则命中）。
        assert!(scope.unsupported.contains("gibberish_stat"));
    }

    /// 模板系数：V=1 数值词条 coefficient = 1.0。
    #[test]
    fn number_template_coefficient_is_one() {
        let m = Modifier::number("Strength", pobr_data::prelude::ModType::Base, 1.0);
        let t = template(&m);
        assert_eq!(t.name, "Strength");
        assert_eq!(t.mod_type, "Base");
        assert_eq!(t.coefficient, Some(1.0));
        assert!(t.value_kind.is_none());
    }
}
