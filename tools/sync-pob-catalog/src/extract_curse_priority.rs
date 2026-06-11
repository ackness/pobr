//! `extract-lua --what curse-priority`：`data.cursePriority` 纯数据表抽取
//! （M3 S1-C，蓝图 §6.3 C3 数据项）。
//!
//! 职责切分（P13 抽取约定）：
//! - Lua 引导脚本（`extract_curse_priority.lua`，编译期内嵌）截取
//!   `Modules/Data.lua:274` 的表字面量并经 luajit 求值，平铺 `k=v` 以
//!   JSONL 原样转发；
//! - 本模块负责分类（per-curse 基值 / SocketPriorityBase / 槽名权重 /
//!   CurseFromAura/CurseFromEquipment）、量级哨兵校验、`_meta` 组装与
//!   byte-stable 序列化（BTreeMap 键序 + serde_json 统一格式）。

use std::io;

use pobr_data::catalog::curse_priority::{CURSE_PRIORITY_SCHEMA, CursePriorityDef};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// 引导脚本内容（经 stdin 注入 luajit）。
const BOOTSTRAP_LUA: &str = include_str!("extract_curse_priority.lua");

/// vendor 表中装备槽名段的闭集（撰写时 commit `2df5a74` 全量 10 槽）。
/// vendor 新增槽名时须同步扩充此表——否则该槽的大权重值会落进 `curse_base`
/// 并触发 [`classify`] 的量级哨兵报错（防静默错分类）。
const SLOT_NAMES: &[&str] = &[
    "Amulet",
    "Body Armour",
    "Boots",
    "Gloves",
    "Helmet",
    "Ring 1",
    "Ring 2",
    "Ring 3",
    "Weapon 1",
    "Weapon 2",
];

/// 引导脚本 JSONL 行：vendor 平铺 `k=v` 原样转发（分类在 Rust 侧）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPriorityEntry {
    /// vendor 表键（curse 名 / 槽名 / 特殊权重名）。
    pub name: String,
    /// 优先级整数值。
    pub priority: i64,
}

/// 完整 overlay 文档（生产侧；消费侧用 [`CursePriorityDef`] 忽略 `_meta`）。
#[derive(Debug, Serialize, Deserialize)]
pub struct CursePriorityDoc {
    /// 头部元信息。
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 分类后的四段数据（与消费侧 schema 同形，顶层平铺）。
    #[serde(flatten)]
    pub table: CursePriorityDef,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_curse_priority(args: &ExtractLuaArgs) -> io::Result<String> {
    if args.files != ["Data"] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "--what curse-priority 的抽取文件固定为 [\"Data\"]，不接受 --files 自定义（收到 {:?}）",
                args.files
            ),
        ));
    }
    let entries: Vec<RawPriorityEntry> = invoke_luajit_jsonl(args, BOOTSTRAP_LUA)?;
    let table = classify(entries)?;
    let meta = build_meta(args)?;
    Ok(assemble_document(meta, table))
}

/// 平铺条目 → 四段分类。特殊键按名精确匹配；槽名走 [`SLOT_NAMES`] 闭集；
/// 其余落 per-curse 基值。量级哨兵：curse 基值必须落在
/// `0..socket_priority_base` 区间——vendor 新增槽名（大权重）误入 curse 段
/// 时显式报错提示扩充闭集，而非静默产出错表。
pub fn classify(entries: Vec<RawPriorityEntry>) -> io::Result<CursePriorityDef> {
    let mut def = CursePriorityDef::default();
    let mut socket_priority_base = None;
    let mut curse_from_aura = None;
    let mut curse_from_equipment = None;
    for entry in entries {
        let duplicated = match entry.name.as_str() {
            "SocketPriorityBase" => socket_priority_base.replace(entry.priority).is_some(),
            "CurseFromAura" => curse_from_aura.replace(entry.priority).is_some(),
            "CurseFromEquipment" => curse_from_equipment.replace(entry.priority).is_some(),
            name if SLOT_NAMES.contains(&name) => def
                .slot_weights
                .insert(name.to_string(), entry.priority)
                .is_some(),
            _ => def
                .curse_base
                .insert(entry.name.clone(), entry.priority)
                .is_some(),
        };
        if duplicated {
            return Err(io::Error::other(format!(
                "cursePriority 键 `{}` 重复（引导脚本输出异常）",
                entry.name
            )));
        }
    }
    let missing = |key: &str| {
        io::Error::other(format!(
            "cursePriority 缺少特殊键 `{key}`（vendor 表结构变化）"
        ))
    };
    def.socket_priority_base = socket_priority_base.ok_or_else(|| missing("SocketPriorityBase"))?;
    def.curse_from_aura = curse_from_aura.ok_or_else(|| missing("CurseFromAura"))?;
    def.curse_from_equipment = curse_from_equipment.ok_or_else(|| missing("CurseFromEquipment"))?;
    if def.curse_base.is_empty() || def.slot_weights.is_empty() {
        return Err(io::Error::other(
            "cursePriority 分类后 curse_base / slot_weights 为空（vendor 表结构变化）",
        ));
    }
    for (name, value) in &def.curse_base {
        if *value < 0 || *value >= def.socket_priority_base {
            return Err(io::Error::other(format!(
                "cursePriority 条目 `{name}`={value} 超出 curse 基值量级（应 < SocketPriorityBase={}）；\
                 若为 vendor 新增槽名，请扩充 extract_curse_priority.rs 的 SLOT_NAMES 闭集",
                def.socket_priority_base
            )));
        }
    }
    Ok(def)
}

/// 组装最终文档：BTreeMap 键序 + serde_json 统一序列化（同输入必然同输出）。
pub fn assemble_document(meta: OverlayMeta, table: CursePriorityDef) -> String {
    let doc = CursePriorityDoc { meta, table };
    let mut json = serde_json::to_string_pretty(&doc).expect("curse priority 文档序列化不应失败");
    json.push('\n');
    json
}

/// 构建 `_meta`（vendor commit + canonical 再生成命令）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = String::from(
        "cargo run -p sync-pob-catalog -- extract-lua --vendor-root vendor/PathOfBuilding-PoE2/src --what curse-priority",
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: CURSE_PRIORITY_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua --what curse-priority".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files: vec!["Modules/Data.lua".to_string()],
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> OverlayMeta {
        OverlayMeta {
            schema: CURSE_PRIORITY_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "test".to_string(),
            extracted_files: vec!["Modules/Data.lua".to_string()],
            regen_command: "test".to_string(),
        }
    }

    fn entry(name: &str, priority: i64) -> RawPriorityEntry {
        RawPriorityEntry {
            name: name.to_string(),
            priority,
        }
    }

    /// 乱序平铺输入 → 四段正确分类（vendor 数值样例，Data.lua:274-300）。
    #[test]
    fn classify_splits_four_sections() {
        let def = classify(vec![
            entry("CurseFromAura", 20000),
            entry("Weapon 1", 1000),
            entry("Temporal Chains", 1),
            entry("SocketPriorityBase", 100),
            entry("Ring 3", 10000),
            entry("Warlord's Mark", 10),
            entry("CurseFromEquipment", 11000),
        ])
        .unwrap();
        assert_eq!(def.curse_base["Temporal Chains"], 1);
        assert_eq!(def.curse_base["Warlord's Mark"], 10);
        assert_eq!(def.socket_priority_base, 100);
        assert_eq!(def.slot_weights["Weapon 1"], 1000);
        assert_eq!(def.slot_weights["Ring 3"], 10000);
        assert_eq!(def.curse_from_equipment, 11000);
        assert_eq!(def.curse_from_aura, 20000);
    }

    /// 闭集外的大权重键（vendor 新增槽名）触发量级哨兵报错而非静默错分类。
    #[test]
    fn classify_rejects_unknown_slot_magnitude() {
        let error = classify(vec![
            entry("SocketPriorityBase", 100),
            entry("CurseFromAura", 20000),
            entry("CurseFromEquipment", 11000),
            entry("Enfeeble", 2),
            entry("Weapon 1", 1000),
            entry("Belt", 9500),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("SLOT_NAMES"), "{error}");
    }

    /// 重复键（引导脚本异常）显式报错。
    #[test]
    fn classify_rejects_duplicate_key() {
        let error = classify(vec![
            entry("SocketPriorityBase", 100),
            entry("SocketPriorityBase", 100),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("重复"), "{error}");
    }

    /// 缺特殊键（vendor 表结构变化）显式报错。
    #[test]
    fn classify_rejects_missing_special_key() {
        let error = classify(vec![
            entry("Enfeeble", 2),
            entry("Weapon 1", 1000),
            entry("CurseFromAura", 20000),
            entry("CurseFromEquipment", 11000),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("SocketPriorityBase"), "{error}");
    }

    /// 组装 byte-stable（同输入两次组装逐字节一致）且 `_meta` 在顶层。
    #[test]
    fn assemble_is_byte_stable() {
        let table = classify(vec![
            entry("SocketPriorityBase", 100),
            entry("CurseFromAura", 20000),
            entry("CurseFromEquipment", 11000),
            entry("Enfeeble", 2),
            entry("Temporal Chains", 1),
            entry("Weapon 1", 1000),
        ])
        .unwrap();
        let one = assemble_document(meta(), table.clone());
        let two = assemble_document(meta(), table);
        assert_eq!(one, two);
        let doc: CursePriorityDoc = serde_json::from_str(&one).unwrap();
        assert_eq!(doc.meta.schema, CURSE_PRIORITY_SCHEMA);
        assert_eq!(doc.table.curse_base.len(), 2);
        // 消费侧 schema 直接读同一文档（`_meta` 被忽略）
        let def: CursePriorityDef = serde_json::from_str(&one).unwrap();
        assert_eq!(def, doc.table);
    }
}
