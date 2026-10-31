//! Explicit prepared compatibility, target identity and reviewed native context.

use super::super::stat_set_codec::policy::hex;
use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) const VARIABLES: [&str; 6] = [
    "Id",
    "Name",
    "Icon_DDSFile",
    "PassiveSkillGraphId",
    "Stats",
    "Stat1Value",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    pub catalog: Catalog,
    pub source: Source,
    pub legacy_metadata: Metadata,
    pub legacy_id_bindings: Vec<Binding>,
    #[serde(default)]
    pub legacy_connection_exceptions: Vec<Connection>,
    pub qualification: Qualification,
    pub evidence: Evidence,
    pub statement: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    domain: String,
    tree_file: String,
    meta_file: String,
    pub tree_compatibility_tag: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub path: String,
    pub schema: String,
    pub sha256: String,
    pub snapshot_version: String,
    pub snapshot_manifest_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Metadata {
    pub value: Value,
    pub source_declaration: Declaration,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Declaration {
    path: String,
    schema: String,
    sha256: String,
    snapshot_version: String,
    role: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub legacy_id: String,
    pub skill: u32,
    pub target_passive_id: String,
    pub expected_name: String,
    pub source_entry_sha256: String,
    role: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Connection {
    pub source_row: usize,
    pub skill: u32,
    pub id: String,
    pub source_entry_sha256: String,
    pub source_pointer: String,
    pub position: usize,
    pub target_skill: u32,
    pub target_passive_id: String,
    pub expected_target_name: String,
    pub target_row_evidence: usize,
    role: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Qualification {
    schema: String,
    pub reference_passive_id: String,
    pub variable_fields: Vec<String>,
    pub context_sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    existing_source_pointers: Reference,
    graph_structure_inspection: Reference,
    prepared_source_check: Reference,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    path: String,
    sha256: String,
}
impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-passive-tree-policy/v1"
            || self.catalog.domain != "passive-tree"
            || self.catalog.tree_file != "passive_tree.json"
            || self.catalog.meta_file != "passive_tree_meta.json"
            || self.catalog.tree_compatibility_tag != "0_5"
            || self.legacy_metadata.value["tree"] != "Default"
        {
            return Err("unsupported passive-tree policy schema/catalog/tree tag".into());
        }
        hex(
            &self.target.facts_manifest_sha256,
            64,
            "target manifest SHA256",
        )?;
        if self.target.facts_manifest_sha256 != sha256_hex(&input.manifest_bytes)
            || input.manifest["source_identity"]["game_patch"].as_str()
                != Some(self.target.game_patch.as_str())
        {
            return Err("passive-tree policy target patch/manifest mismatch".into());
        }
        path(
            &self.source.snapshot_version,
            &self.source.path,
            "passive_tree.json",
        )?;
        hex(&self.source.sha256, 64, "prepared tree SHA256")?;
        hex(
            &self.source.snapshot_manifest_sha256,
            64,
            "declared snapshot manifest SHA256",
        )?;
        if self.source.schema != "pobr_data::catalog::PassiveNodeDef[]" {
            return Err("unsupported prepared tree schema".into());
        }
        let meta = &self.legacy_metadata.source_declaration;
        path(&meta.snapshot_version, &meta.path, "passive_tree_meta.json")?;
        hex(&meta.sha256, 64, "declared metadata SHA256")?;
        if meta.snapshot_version != self.source.snapshot_version
            || meta.schema != "pobr_data::catalog::PassiveTreeMeta"
            || meta.role
                != "policy-owned compatibility metadata; authoring file declared, not producer-provided or file-hash-verified"
        {
            return Err("unsupported prepared metadata declaration".into());
        }
        let q = &self.qualification;
        let fields: BTreeSet<_> = q.variable_fields.iter().map(String::as_str).collect();
        if q.schema != "pobr-ordinary-passive-native-context/whole-row-reference-v1"
            || q.reference_passive_id != "maximum_resistances1"
            || fields.len() != q.variable_fields.len()
            || fields != VARIABLES.into_iter().collect()
        {
            return Err("unsupported native qualification schema/reference/variable fields".into());
        }
        hex(&q.context_sha256, 64, "reference context SHA256")?;
        let mut ids = BTreeSet::new();
        let mut skills = BTreeSet::new();
        for b in &self.legacy_id_bindings {
            if b.legacy_id.is_empty()
                || b.target_passive_id.is_empty()
                || b.expected_name.is_empty()
                || !ids.insert(&b.legacy_id)
                || !skills.insert(b.skill)
                || b.role
                    != "explicit legacy numeric-slug identity binding only; keep output id/fields; no native packet for this alias"
            {
                return Err("invalid/duplicate prepared identity binding".into());
            }
            hex(&b.source_entry_sha256, 64, "binding source-entry SHA256")?;
        }
        let mut occurrences = BTreeSet::new();
        let mut pointers = BTreeSet::new();
        for c in &self.legacy_connection_exceptions {
            if c.id.is_empty()
                || c.target_passive_id.is_empty()
                || c.expected_target_name.is_empty()
                || !occurrences.insert((c.source_row, c.position))
                || !pointers.insert(&c.source_pointer)
                || c.source_pointer != format!("/{}/connections/{}", c.source_row, c.position)
                || c.role
                    != "existing prepared reference outside emitted skills and variant aliases; preserve exact edge, do not emit target or certify reachability/eligibility"
            {
                return Err("invalid/duplicate prepared connection exception".into());
            }
            hex(&c.source_entry_sha256, 64, "connection source-entry SHA256")?;
        }
        for r in [
            &self.evidence.existing_source_pointers,
            &self.evidence.graph_structure_inspection,
            &self.evidence.prepared_source_check,
        ] {
            if r.path.is_empty() {
                return Err("empty evidence declaration path".into());
            }
            hex(&r.sha256, 64, "evidence declaration SHA256")?;
        }
        if self.statement.trim().is_empty() {
            return Err("empty passive-tree compatibility statement".into());
        }
        Ok(())
    }
}
fn path(version: &str, path: &str, file: &str) -> Result<(), String> {
    if version.is_empty()
        || version.trim() != version
        || version.contains(['/', '\\'])
        || matches!(version, "." | "..")
        || path != format!("data/{version}/base/{file}")
    {
        return Err("invalid prepared source version/path declaration".into());
    }
    Ok(())
}
