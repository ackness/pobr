//! `extract-lua --what stat-set-labels`：vendor statSet 形态 label + 导出序号 →
//! `data/<版本>/overlay/stat_set_labels.json`。
//!
//! **双源 join**（`.dat` `GrantedEffectStatSets.Label` 的 FK 目标表
//! `GrantedEffectLabels` 在钉定补丁不可下载，label 文本只存在于 vendor 导出产物）：
//! - **label 文本**：`Data/Skills/<file>.lua` 的 `statSets[i].label`
//!   （vendor `Export/Scripts/skills.lua:478` 已解析 `LabelType.Label`，
//!   缺省回退技能显示名），经 luajit 引导脚本抽取（JSONL）；
//! - **set 稳定 id**：`Export/Skills/<file>.txt` 模板的 `#skill` / `#set` 行——
//!   模板内 `#set` 顺序 = 数据文件 statSets 数字键 = PoB2 `<Gem statSetIndex>`
//!   的索引语义，按 (skill, 序号) join 得 `(skill, set_id, set_index, label)`。
//!
//! 注意 vendor 模板会**策展跳过**个别 `.dat` 附加 set（如 IceNovaPlayerOnFrostbolt，
//! 与主 set 数值同形的位置变体）——这些 set 不在导出产物中，无 label / 无导出序号，
//! 消费侧（`StatSetDef::vendor_set_index = None`）不可被 statSetIndex 选中，忠实
//! 转录 PoB2 行为。
//!
//! 确定性：按 `(skill, set_index)` 排序 + serde_json 统一序列化，同输入 byte-stable。

use std::fs;
use std::io;

use pobr_data::catalog::StatSetLabelDef;
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）
const BOOTSTRAP_LUA: &str = include_str!("extract_stat_set_labels.lua");

/// 当前 overlay 文档 schema 标识（字段演化时递增）
pub const STAT_SET_LABELS_SCHEMA: &str = "stat_set_labels/v1";

/// 引导脚本输出的单行 JSONL：一条 (技能, 导出序号, label)。
#[derive(Debug, Clone, Deserialize)]
pub struct LabelRow {
    /// 授予效果 id。
    pub skill: String,
    /// statSets 1-based 导出序号。
    pub set_index: u32,
    /// label 文本。
    pub label: String,
}

/// 完整 overlay 文档（生成侧；消费侧 schema 见
/// [`pobr_data::catalog::StatSetLabelsDef`]，serde 形状一致防字段漂移）。
#[derive(Debug, Serialize, Deserialize)]
pub struct StatSetLabelsDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// label 表，按 `(skill, set_index)` 升序。
    pub labels: Vec<StatSetLabelDef>,
}

/// 执行抽取：luajit 抽 label + Rust 读模板抽 set id，join 后返回 byte-stable JSON。
pub fn run_extract_stat_set_labels(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows: Vec<LabelRow> = invoke_luajit_jsonl(args, BOOTSTRAP_LUA)?;
    let template_sets = parse_templates(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_labels_document(meta, rows, &template_sets))
}

/// 解析 `Export/Skills/<file>.txt` 模板：`#skill <id>` 开块、其后的 `#set <id>`
/// 依序编号（1-based）。返回 `(skill, set_index) → set_id`。
fn parse_templates(
    args: &ExtractLuaArgs,
) -> io::Result<std::collections::BTreeMap<(String, u32), String>> {
    let mut map = std::collections::BTreeMap::new();
    for name in &args.files {
        let path = args
            .vendor_root
            .join("Export/Skills")
            .join(format!("{name}.txt"));
        let text = fs::read_to_string(&path).map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("无法读取 vendor 模板 {}：{error}", path.display()),
            )
        })?;
        let mut current_skill: Option<String> = None;
        let mut set_index = 0u32;
        for line in text.lines() {
            let line = line.trim();
            if let Some(skill) = line.strip_prefix("#skill ") {
                current_skill = Some(skill.trim().to_string());
                set_index = 0;
            } else if let Some(set_id) = line.strip_prefix("#set ")
                && let Some(skill) = &current_skill
            {
                set_index += 1;
                map.insert((skill.clone(), set_index), set_id.trim().to_string());
            }
        }
    }
    Ok(map)
}

/// 组装最终文档：join 模板 set id（无对应模板行的 label 条目丢弃并告警——
/// 数据/模板漂移信号）+ 排序 + serde_json 统一序列化。
pub fn assemble_labels_document(
    meta: OverlayMeta,
    rows: Vec<LabelRow>,
    template_sets: &std::collections::BTreeMap<(String, u32), String>,
) -> String {
    let mut labels: Vec<StatSetLabelDef> = rows
        .into_iter()
        .filter_map(
            |row| match template_sets.get(&(row.skill.clone(), row.set_index)) {
                Some(set_id) => Some(StatSetLabelDef {
                    skill: row.skill,
                    set_id: set_id.clone(),
                    set_index: row.set_index,
                    label: row.label,
                }),
                None => {
                    eprintln!(
                        "stat_set_labels: 技能 {} 的 statSets[{}] 无对应模板 #set 行（丢弃）",
                        row.skill, row.set_index
                    );
                    None
                }
            },
        )
        .collect();
    labels.sort_by(|a, b| {
        a.skill
            .cmp(&b.skill)
            .then_with(|| a.set_index.cmp(&b.set_index))
    });
    let doc = StatSetLabelsDoc { meta, labels };
    let mut json = serde_json::to_string_pretty(&doc).expect("stat set labels 文档序列化不应失败");
    json.push('\n');
    json
}

/// 构建 `_meta`（与公共层同约定：regen_command 写 canonical 相对路径）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let extracted_files: Vec<String> = args
        .files
        .iter()
        .flat_map(|name| {
            [
                format!("Data/Skills/{name}.lua"),
                format!("Export/Skills/{name}.txt"),
            ]
        })
        .collect();
    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what stat-set-labels --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: STAT_SET_LABELS_SCHEMA.to_string(),
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
            schema: STAT_SET_LABELS_SCHEMA.into(),
            generator: "test".into(),
            vendor: "PathOfBuilding-PoE2".into(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "subject".into(),
            extracted_files: vec![],
            regen_command: "cargo run …".into(),
        }
    }

    /// join：模板有对应行 → 带 set_id 落库；无对应行 → 丢弃告警。排序确定。
    #[test]
    fn joins_template_set_ids_and_sorts() {
        let mut templates = std::collections::BTreeMap::new();
        templates.insert(
            ("IceNovaPlayer".to_string(), 1),
            "IceNovaPlayer".to_string(),
        );
        templates.insert(
            ("IceNovaPlayer".to_string(), 2),
            "IceNovaColdInfusedPlayer".to_string(),
        );
        let rows = vec![
            LabelRow {
                skill: "IceNovaPlayer".into(),
                set_index: 2,
                label: "Cold-Infused".into(),
            },
            LabelRow {
                skill: "IceNovaPlayer".into(),
                set_index: 1,
                label: "Ice Nova".into(),
            },
            LabelRow {
                skill: "IceNovaPlayer".into(),
                set_index: 3,
                label: "Orphan".into(), // 模板无第 3 set → 丢弃
            },
        ];
        let json = assemble_labels_document(meta(), rows, &templates);
        let def: pobr_data::catalog::StatSetLabelsDef = serde_json::from_str(&json).unwrap();
        assert_eq!(def.labels.len(), 2);
        assert_eq!(def.labels[0].set_index, 1);
        assert_eq!(def.labels[1].set_id, "IceNovaColdInfusedPlayer");
        assert_eq!(def.labels[1].label, "Cold-Infused");
    }
}
