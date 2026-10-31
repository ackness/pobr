//! Closed selected-file identities and explicit target numeric bindings.

use super::super::stat_set_codec::policy::hex;
use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(super) const FILES: [(&str, &str); 3] = [
    (
        "game_constants.json",
        "pobr_data::catalog::GameConstantsDef",
    ),
    (
        "character_constants.json",
        "pobr_data::catalog::CharacterConstantsDef",
    ),
    ("jewel_radii.json", "pobr_data::catalog::JewelRadiiDef"),
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    normalization: String,
    target: Target,
    pub source_snapshot: Snapshot,
    pub source_files: Vec<SourceFile>,
    pub bindings: Vec<Binding>,
    pub evidence: Evidence,
    pub statement: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub path: String,
    pub schema_version: u32,
    pub sha256: String,
    pub snapshot_version: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceFile {
    pub file: String,
    pub relative_path: String,
    pub schema: String,
    pub sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub file: String,
    pub source_id: String,
    pub target_pointer: String,
    pub additional_divisors: Vec<f64>,
    #[serde(default)]
    pub allow_missing_leaf: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    existing_dto_semantics: Vec<Reference>,
    source_check: Reference,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    path: String,
    sha256: String,
}

impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-runtime-constants-policy/v1"
            || self.normalization != "value_over_divisor_then_divide_in_order/v1"
        {
            return Err("unsupported runtime-constants policy schema/normalization".into());
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
            return Err("runtime-constants target patch/manifest mismatch".into());
        }
        let s = &self.source_snapshot;
        if s.schema_version != 3
            || s.snapshot_version.is_empty()
            || s.snapshot_version.trim() != s.snapshot_version
            || s.snapshot_version.contains(['/', '\\'])
            || matches!(s.snapshot_version.as_str(), "." | "..")
            || s.path != format!("data/{}/manifest.json", s.snapshot_version)
        {
            return Err("invalid source snapshot schema/version/path".into());
        }
        hex(&s.sha256, 64, "source snapshot SHA256")?;
        let mut files = BTreeSet::new();
        for f in &self.source_files {
            let schema = FILES
                .iter()
                .find(|(name, _)| *name == f.file)
                .map(|(_, schema)| *schema)
                .ok_or("unknown selected constants file")?;
            if !files.insert(f.file.as_str())
                || f.relative_path != format!("base/{}", f.file)
                || f.schema != schema
            {
                return Err("duplicate/invalid selected constants path/schema".into());
            }
            hex(&f.sha256, 64, "selected constants SHA256")?;
        }
        if files.len() != FILES.len() {
            return Err("incomplete selected constants file set".into());
        }
        let mut destinations = BTreeSet::new();
        if self.bindings.is_empty() {
            return Err("empty constants bindings".into());
        }
        for b in &self.bindings {
            if !files.contains(b.file.as_str())
                || b.source_id.is_empty()
                || !b.target_pointer.starts_with('/')
                || !destinations.insert((&b.file, &b.target_pointer))
            {
                return Err(
                    "invalid/duplicate constants binding destination or source identity".into(),
                );
            }
            if b.additional_divisors
                .iter()
                .any(|v| !v.is_finite() || *v == 0.0)
            {
                return Err("additional divisors must be finite and nonzero".into());
            }
        }
        for r in self
            .evidence
            .existing_dto_semantics
            .iter()
            .chain([&self.evidence.source_check])
        {
            if r.path.is_empty() {
                return Err("empty evidence declaration path".into());
            }
            hex(&r.sha256, 64, "evidence declaration SHA256")?;
        }
        if self.statement.trim().is_empty() {
            return Err("empty compatibility statement".into());
        }
        Ok(())
    }
}
