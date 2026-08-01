//! `extract-lua --what stat-set-labels`: vendor statSet shape's label plus
//! export index -> `data/<version>/overlay/stat_set_labels.json`.
//!
//! **Two-source join** (the FK target table for `.dat` `GrantedEffectStatSets.Label`
//! — `GrantedEffectLabels` — isn't downloadable at the pinned patch, so the
//! label text only exists in vendor's export artifacts):
//! - **label text**: `statSets[i].label` from `Data/Skills/<file>.lua`
//!   (vendor `Export/Scripts/skills.lua:478` already resolves `LabelType.Label`,
//!   defaulting back to the skill's display name), extracted via a luajit bootstrap script (JSONL);
//! - **stable set id**: the `#skill` / `#set` lines of the
//!   `Export/Skills/<file>.txt` template — the `#set` order in the template
//!   matches the data file's statSets numeric keys, which is the same index
//!   semantics as PoB2's `<Gem statSetIndex>`, so joining by (skill, index)
//!   yields `(skill, set_id, set_index, label)`.
//!
//! Note that vendor's template **deliberately skips** a few `.dat`
//! additional sets (e.g. IceNovaPlayerOnFrostbolt, a positional variant with
//! the same values as the main set) — these sets aren't in the export
//! artifacts, so they have no label / no export index, and on the
//! consumption side (`StatSetDef::vendor_set_index = None`) they can't be
//! selected by statSetIndex; this is a faithful transcription of PoB2's behavior.
//!
//! Determinism: sorted by `(skill, set_index)` + serde_json serialization; byte-stable for the same input.

use std::fs;
use std::io;

use pobr_data::catalog::StatSetLabelDef;
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory)
const BOOTSTRAP_LUA: &str = include_str!("extract_stat_set_labels.lua");

/// Current overlay document schema identifier (bumped when fields evolve)
pub const STAT_SET_LABELS_SCHEMA: &str = "stat_set_labels/v1";

/// One JSONL line emitted by the bootstrap script: one (skill, export index, label) triple.
#[derive(Debug, Clone, Deserialize)]
pub struct LabelRow {
    /// The granted effect id.
    pub skill: String,
    /// The statSets 1-based export index.
    pub set_index: u32,
    /// The label text.
    pub label: String,
}

/// The full overlay document (generation side; see
/// [`pobr_data::catalog::StatSetLabelsDef`] for the consumption-side schema
/// — matching serde shapes guard against field drift).
#[derive(Debug, Serialize, Deserialize)]
pub struct StatSetLabelsDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file)
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The label table, ascending by `(skill, set_index)`.
    pub labels: Vec<StatSetLabelDef>,
}

/// Run the extraction: luajit extracts labels, Rust reads templates to
/// extract set ids, then joins them and returns byte-stable JSON.
pub fn run_extract_stat_set_labels(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows: Vec<LabelRow> = invoke_luajit_jsonl(args, BOOTSTRAP_LUA)?;
    let template_sets = parse_templates(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_labels_document(meta, rows, &template_sets))
}

/// Parse an `Export/Skills/<file>.txt` template: `#skill <id>` opens a
/// block, and the `#set <id>` lines that follow are numbered in order
/// (1-based). Returns `(skill, set_index) -> set_id`.
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
                format!("failed to read vendor template {}: {error}", path.display()),
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

/// Assemble the final document: join in the template's set id (a label
/// entry with no matching template line is dropped with a warning — a
/// data/template drift signal) + sort + serde_json serialization.
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
                        "stat_set_labels: skill {}'s statSets[{}] has no matching template #set line (dropped)",
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
    let mut json = serde_json::to_string_pretty(&doc)
        .expect("stat set labels document serialization should not fail");
    json.push('\n');
    json
}

/// Build `_meta` (same convention as the shared layer: regen_command writes a canonical relative path).
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

    /// Join: a matching template line -> stored with its set_id; no match -> dropped with a warning. Sort order is deterministic.
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
                label: "Orphan".into(), // no set 3 in the template -> dropped
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
