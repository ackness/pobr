//! `extract-lua --what stat-map`: extracts vendor PoB2's
//! `Data/SkillStatMap.lua` (954 global stat -> modifier-constructor mappings)
//! plus the `statMap` field of each statSet in `Data/Skills/{act_*,sup_*,other}.lua`
//! (per-set overrides) into `data/<version>/overlay/skill_stat_map.json` (the data plane).
//!
//! Responsibility split matches [`crate::extract_lua`]: the Lua bootstrap
//! script (`extract_skill_stat_map.lua`, embedded at compile time) only does
//! **faithful extraction** and emits JSONL (one line per
//! `{"scope":"global"|"set",...,"entry":{...}}`); the Rust side handles
//! sorting ([`std::collections::BTreeMap`] lexicographic order),
//! shortest-round-trip number representation, and whole-document
//! serialization, guaranteeing **byte-stable** output on repeated runs with the same input.
//!
//! The single source of truth for the consumption-side schema is
//! [`pobr_data::catalog::stat_map`] (shared serde shape between generation
//! and consumption, so fields can't drift); semantic filtering (which tags /
//! constructors are actually supported) belongs to
//! `pobr-core::rules::stat_map_engine`, not this module.

use std::io;

use pobr_data::catalog::stat_map::{SkillStatMapDef, StatMapEntry};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory).
const BOOTSTRAP_LUA: &str = include_str!("extract_skill_stat_map.lua");

/// Current overlay document schema identifier (bumped when fields evolve).
pub const SKILL_STAT_MAP_SCHEMA: &str = "skill_stat_map/v1";

/// One JSONL line emitted by the bootstrap script: one statMap mapping (global or per-set scope).
#[derive(Debug, Clone, Deserialize)]
pub struct StatMapRow {
    /// Scope: `"global"` (SkillStatMap.lua) or `"set"` (a statSet in Data/Skills).
    pub scope: String,
    /// The stable stat id.
    pub stat: String,
    /// The granted effect id (only when `scope == "set"`).
    #[serde(default)]
    pub effect: Option<String>,
    /// The statSet index (1-based index into vendor's `statSets` array; only when `scope == "set"`).
    #[serde(default)]
    pub stat_set: Option<u32>,
    /// The mapping entry (same schema shape as the consumption-side [`StatMapEntry`], a faithful table dump).
    pub entry: StatMapEntry,
}

/// The full overlay document (generation side; `_meta` header plus the flattened consumption-side [`SkillStatMapDef`]).
#[derive(Debug, Serialize, Deserialize)]
pub struct StatMapDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file).
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The mapping body (global + per_stat_set; BTreeMap lexicographic order guarantees determinism).
    #[serde(flatten)]
    pub def: SkillStatMapDef,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
pub fn run_extract_stat_map(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_luajit_jsonl::<StatMapRow>(args, BOOTSTRAP_LUA)?;
    let meta = build_meta(args)?;
    assemble_stat_map_document(meta, rows)
}

/// Assemble the final document: JSONL rows -> `global` / `per_stat_set`
/// sections (all keys go through BTreeMap lexicographic order) + serde_json
/// serialization (identical input always yields identical output).
///
/// Duplicate-key guard: the same (scope, stat) appearing twice is an error
/// (vendor's table is a dict, so a duplicate signals a bootstrap-script or
/// vendor-data anomaly — better to fail than silently let the later write win).
pub fn assemble_stat_map_document(meta: OverlayMeta, rows: Vec<StatMapRow>) -> io::Result<String> {
    let mut def = SkillStatMapDef::default();
    for row in rows {
        match row.scope.as_str() {
            "global" => {
                if def.global.insert(row.stat.clone(), row.entry).is_some() {
                    return Err(io::Error::other(format!(
                        "stat-map extraction has a duplicate global key: {}",
                        row.stat
                    )));
                }
            }
            "set" => {
                let (Some(effect), Some(set)) = (row.effect.clone(), row.stat_set) else {
                    return Err(io::Error::other(format!(
                        "stat-map extraction: set row is missing effect/stat_set field: stat={}",
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
                        "stat-map extraction has a duplicate per-set key: {effect}#{set} {}",
                        row.stat
                    )));
                }
            }
            other => {
                return Err(io::Error::other(format!(
                    "stat-map extraction: unknown scope: {other}"
                )));
            }
        }
    }
    let doc = StatMapDoc { meta, def };
    let mut json = serde_json::to_string_pretty(&doc).map_err(io::Error::other)?;
    json.push('\n');
    Ok(json)
}

/// Read the vendor version file and build `_meta` (same convention as
/// `extract_lua::build_meta`: `regen_command` writes a canonical relative
/// path, so output is byte-identical when rerun on a different machine).
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;

    // Extraction sources = the global table plus each skill data file (unlike skill-overrides: one extra global table).
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

    /// The global / set scopes each go into their own section; keys are lexicographic (BTreeMap guarantees this).
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
        // Lexicographic order: a_stat's text appears before b_stat's (visible evidence of byte-stable sorting).
        assert!(json.find("a_stat").unwrap() < json.find("b_stat").unwrap());
    }

    /// A duplicate key (same scope, same stat) errors rather than silently overwriting.
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

    /// A set row missing effect / stat_set errors.
    #[test]
    fn set_row_missing_fields_errors_out() {
        assert!(assemble_stat_map_document(meta(), vec![row("set", "s", None, None)]).is_err());
    }

    /// An unknown scope errors.
    #[test]
    fn unknown_scope_errors_out() {
        assert!(assemble_stat_map_document(meta(), vec![row("woot", "s", None, None)]).is_err());
    }
}
