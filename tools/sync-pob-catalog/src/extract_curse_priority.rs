//! `extract-lua --what curse-priority`: extraction of the plain `data.cursePriority` data table
//!
//! Responsibility split (the deterministic-extraction convention):
//! - The Lua bootstrap script (`extract_curse_priority.lua`, embedded at
//!   compile time) slices out the table literal at `Modules/Data.lua:274`,
//!   evaluates it via luajit, and forwards the flattened `k=v` pairs as-is over JSONL;
//! - This module handles classification (per-curse base values /
//!   SocketPriorityBase / slot-name weights / CurseFromAura /
//!   CurseFromEquipment), magnitude sentinel checks, `_meta` assembly, and
//!   byte-stable serialization (BTreeMap key order + uniform serde_json formatting).

use std::io;

use pobr_data::catalog::curse_priority::{CURSE_PRIORITY_SCHEMA, CursePriorityDef};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// Bootstrap script content (piped into luajit via stdin).
const BOOTSTRAP_LUA: &str = include_str!("extract_curse_priority.lua");

/// The closed set of equipment slot names in the vendor table (all 10 slots,
/// as of commit `2df5a74` when this was written). When vendor adds a new
/// slot name, this table must be extended too — otherwise that slot's large
/// weight value would fall into `curse_base` and trip [`classify`]'s
/// magnitude sentinel check (guarding against silent misclassification).
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

/// One JSONL line from the bootstrap script: a flattened vendor `k=v` pair, forwarded as-is (classification happens in Rust).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPriorityEntry {
    /// The vendor table key (curse name / slot name / special weight name).
    pub name: String,
    /// The priority integer value.
    pub priority: i64,
}

/// The full overlay document (production side; the consumption side uses [`CursePriorityDef`] and ignores `_meta`).
#[derive(Debug, Serialize, Deserialize)]
pub struct CursePriorityDoc {
    /// Header metadata.
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The four classified sections (same shape as the consumption-side schema, flattened at the top level).
    #[serde(flatten)]
    pub table: CursePriorityDef,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
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

/// Flattened entries -> four classified sections. Special keys match by
/// exact name; slot names go through the [`SLOT_NAMES`] closed set;
/// everything else falls into per-curse base values. Magnitude sentinel:
/// curse base values must fall in `0..socket_priority_base` — if a new
/// vendor slot name (with a large weight) accidentally lands in the curse
/// section, this errors explicitly to prompt extending the closed set
/// instead of silently producing a wrong table.
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

/// Assemble the final document: BTreeMap key order + serde_json serialization (identical input always yields identical output).
pub fn assemble_document(meta: OverlayMeta, table: CursePriorityDef) -> String {
    let doc = CursePriorityDoc { meta, table };
    let mut json = serde_json::to_string_pretty(&doc).expect("curse priority 文档序列化不应失败");
    json.push('\n');
    json
}

/// Build `_meta` (vendor commit + canonical regen command).
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

    /// Shuffled flattened input -> correct four-way classification (vendor sample values, Data.lua:274-300).
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

    /// A large-weight key outside the closed set (a new vendor slot name)
    /// trips the magnitude sentinel error instead of silently misclassifying.
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

    /// A duplicate key (bootstrap script malfunction) errors explicitly.
    #[test]
    fn classify_rejects_duplicate_key() {
        let error = classify(vec![
            entry("SocketPriorityBase", 100),
            entry("SocketPriorityBase", 100),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("重复"), "{error}");
    }

    /// A missing special key (vendor table structure changed) errors explicitly.
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

    /// Assembly is byte-stable (two assemblies of the same input are byte-identical) and `_meta` sits at the top level.
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
        // The consumption-side schema reads the same document directly (`_meta` is ignored)
        let def: CursePriorityDef = serde_json::from_str(&one).unwrap();
        assert_eq!(def, doc.table);
    }
}
