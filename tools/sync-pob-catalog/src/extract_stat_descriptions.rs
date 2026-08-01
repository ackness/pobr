//! `extract-lua --what stat-descriptions`: extracts the canonical display
//! text for every stat_id from vendor PoB2's `Data/StatDescriptions/*.lua`
//! into `data/<version>/overlay/stat_descriptions.json` (the data plane for
//! the "stat_id -> Modifier, second channel" pipeline).
//!
//! Responsibility split matches [`crate::extract_stat_map`]: the Lua
//! bootstrap script (`extract_stat_descriptions.lua`, embedded at compile
//! time) loads the description tables in a minimal environment, feeds each
//! stat_id a representative value (V=1) to render its text, and emits JSONL
//! (one line per single text line or per compound template); the Rust side
//! handles scope segmentation, line-order merging ([`BTreeMap`]
//! lexicographic order), and whole-document serialization, guaranteeing
//! **byte-stable** output on repeated runs with the same input.
//!
//! The single source of truth for the consumption-side schema is
//! [`pobr_data::catalog::stat_descriptions`] (shared serde shape between
//! generation and consumption, so fields can't drift); precedence (child
//! scope overrides parent) and deciding which stat_id is actually supported
//! belong to the consumption side's §B generator / `parse_mod_engine`, not this module.

use std::collections::BTreeMap;
use std::io;

use pobr_data::catalog::stat_descriptions::{CompoundDescription, StatDescriptionsDef};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory).
const BOOTSTRAP_LUA: &str = include_str!("extract_stat_descriptions.lua");

/// Current overlay document schema identifier (bumped when fields evolve).
pub const STAT_DESCRIPTIONS_SCHEMA: &str = "stat_descriptions/v1";

/// Default extraction scope (most relevant to the tree channel: root + passive + presence/aura).
/// Other StatDescriptions files (skill/gem/monster/advanced_mod) are added as needed via `--files`.
pub const DEFAULT_STAT_DESC_FILES: &[&str] = &[
    "stat_descriptions",
    "passive_skill_stat_descriptions",
    "passive_skill_aura_stat_descriptions",
];

/// One JSONL line emitted by the bootstrap script: either a single text line or a compound template.
///
/// - single, rendered: `{stat, scope, text, line, compound:false}`
/// - single, no variant: `{stat, scope, compound:false, unrendered:true}`
/// - compound: `{stat, scope, compound:true, member_stats, template}`
#[derive(Debug, Clone, Deserialize)]
pub struct StatDescRow {
    /// The stable stat id (for a compound row, this is the descriptor's first stat_id).
    pub stat: String,
    /// The source scope name (the StatDescriptions file name, without `.lua`).
    pub scope: String,
    /// Whether this is a multi-stat (compound) descriptor.
    pub compound: bool,
    /// The rendered single-line text (single only, when renderable).
    #[serde(default)]
    pub text: Option<String>,
    /// Line number (1-based; multi-line descriptions for the same stat are ordered by this).
    #[serde(default)]
    pub line: Option<u32>,
    /// Marks a variant with no renderable text (single only).
    #[serde(default)]
    pub unrendered: bool,
    /// All stat_ids bound to a compound descriptor (compound only).
    #[serde(default)]
    pub member_stats: Vec<String>,
    /// The compound template, verbatim (compound only).
    #[serde(default)]
    pub template: Option<String>,
}

/// The full overlay document (generation side; `_meta` header plus the flattened consumption-side [`StatDescriptionsDef`]).
#[derive(Debug, Serialize, Deserialize)]
pub struct StatDescriptionsDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file).
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The description body (segmented by scope; BTreeMap lexicographic order guarantees determinism).
    #[serde(flatten)]
    pub def: StatDescriptionsDef,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
pub fn run_extract_stat_descriptions(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_luajit_jsonl::<StatDescRow>(args, BOOTSTRAP_LUA)?;
    let meta = build_meta(args)?;
    assemble_stat_descriptions_document(meta, rows)
}

/// Assemble the final document: JSONL rows -> each scope's single / compound / unrendered sections.
///
/// Single text lines are first merged into a `BTreeMap<line, text>` (line
/// order is deterministic, independent of input order), then flattened into
/// a `Vec<String>` — guaranteeing byte-stable output for the same input.
/// Duplicate keys (same scope, same stat, same line; or a duplicate
/// compound) error out rather than silently overwriting.
pub fn assemble_stat_descriptions_document(
    meta: OverlayMeta,
    rows: Vec<StatDescRow>,
) -> io::Result<String> {
    // Intermediate state: scope -> stat -> line -> text (line-order merging).
    let mut single_lines: BTreeMap<String, BTreeMap<String, BTreeMap<u32, String>>> =
        BTreeMap::new();
    let mut def = StatDescriptionsDef::default();

    for row in rows {
        let scope = def.scopes.entry(row.scope.clone()).or_default();
        if row.compound {
            let template = row.template.clone().ok_or_else(|| {
                io::Error::other(format!(
                    "stat-descriptions compound row is missing template: {}",
                    row.stat
                ))
            })?;
            let entry = CompoundDescription {
                member_stats: row.member_stats.clone(),
                template,
            };
            if scope.compound.insert(row.stat.clone(), entry).is_some() {
                return Err(io::Error::other(format!(
                    "stat-descriptions extraction has a duplicate compound key: {}::{}",
                    row.scope, row.stat
                )));
            }
        } else if row.unrendered {
            scope.unrendered.insert(row.stat.clone());
        } else {
            let text = row.text.clone().ok_or_else(|| {
                io::Error::other(format!(
                    "stat-descriptions single row is missing text: {}::{}",
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
                    "stat-descriptions extraction has a duplicate single row: {}::{} line {line}",
                    row.scope, row.stat
                )));
            }
        }
    }

    // Flatten the line-order-merged state into the consumption-side shape.
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

/// Read the vendor version file and build `_meta` (same convention as `extract_lua::build_meta`).
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

    /// Multi-line single entries flatten in line order; multiple scopes go into their own sections; keys are lexicographic (BTreeMap guarantees this).
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
                // Deliberately shuffled input: line 2 comes before line 1.
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
        // Shuffled input still flattens in line order.
        assert_eq!(root.single["two_line"], vec!["first", "second"]);
        assert!(
            doc.def
                .scopes
                .contains_key("passive_skill_stat_descriptions")
        );
    }

    /// A compound row keeps template + member_stats verbatim.
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

    /// An unrendered row goes into the diagnostic set.
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

    /// A duplicate single row (same scope, same stat, same line) errors rather than silently overwriting.
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

    /// Lexicographic order: a_stat's text appears before z_stat's (visible evidence of byte-stable sorting).
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
