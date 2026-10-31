//! Explicit legacy interpretation identity and exact target absence decisions.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::stat_set_codec::policy::{hex, validate_extraction_metadata};
use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    pub source: Source,
    pub branch_codec: Source,
    pub absent_target_stat_ids: Vec<String>,
    pub unbound_scopes: Vec<Unbound>,
    pub evidence: Evidence,
    pub statement: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::ordinary_facts) struct Source {
    pub snapshot_version: String,
    pub path: String,
    pub sha256: String,
    pub metadata: Value,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    path: String,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    source_check: Reference,
    legacy_runtime_manifest: Reference,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Unbound {
    pub skill: String,
    pub set_index: u32,
    pub codec_present: bool,
    pub target_effect_present: bool,
    pub action: String,
}

impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-skill-stat-map-policy/v1" {
            return Err("unsupported skill-stat-map policy schema".into());
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
            return Err("skill-stat-map policy target patch / manifest mismatch".into());
        }
        self.source
            .validate("skill_stat_map.json", "skill_stat_map/v1")?;
        self.branch_codec
            .validate("stat_set_labels.json", "stat_set_labels/v1")?;
        if self.statement.trim().is_empty() {
            return Err("empty compatibility statement".into());
        }
        for reference in [
            &self.evidence.source_check,
            &self.evidence.legacy_runtime_manifest,
        ] {
            identity(&reference.path, "evidence path")?;
            hex(&reference.sha256, 64, "evidence SHA256")?;
        }
        let mut absent = BTreeSet::new();
        for stat in &self.absent_target_stat_ids {
            identity(stat, "absent stat Id")?;
            if !absent.insert(stat) {
                return Err("duplicate absent stat exception".into());
            }
        }
        let mut unbound = BTreeSet::new();
        for row in &self.unbound_scopes {
            identity(&row.skill, "unbound skill Id")?;
            if row.set_index == 0 || !unbound.insert((&row.skill, row.set_index)) {
                return Err("duplicate/invalid unbound scope exception".into());
            }
            // This schema expresses missing-both archival scopes, not an ownership
            // override. A future different exception needs an explicit adaptation.
            if row.codec_present
                || row.target_effect_present
                || row.action != "preserve unchanged/unbound; do not infer an index or retarget"
            {
                return Err("unsupported unbound scope exception action/presence".into());
            }
        }
        Ok(())
    }
}

impl Source {
    pub(in crate::ordinary_facts) fn validate(
        &self,
        filename: &str,
        schema: &str,
    ) -> Result<(), String> {
        identity(&self.snapshot_version, "legacy snapshot version")?;
        if self.snapshot_version.contains('/')
            || self.snapshot_version.contains('\\')
            || matches!(self.snapshot_version.as_str(), "." | "..")
        {
            return Err("invalid legacy snapshot version".into());
        }
        hex(&self.sha256, 64, "legacy source SHA256")?;
        validate_extraction_metadata(&self.metadata, schema)?;
        if self.path != format!("data/{}/overlay/{filename}", self.snapshot_version)
            || !self.metadata["regen_command"]
                .as_str()
                .is_some_and(|cmd| cmd.ends_with(&format!("--out {}", self.path)))
        {
            return Err("legacy version/path differs from original extraction metadata".into());
        }
        Ok(())
    }
}

pub(in crate::ordinary_facts) fn identity(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(format!("{field}: nonempty canonical identity required"));
    }
    Ok(())
}

pub(super) fn index(value: &str) -> Result<u32, String> {
    let index = value
        .parse::<u32>()
        .map_err(|_| "scope index: positive canonical decimal u32 required")?;
    if index == 0 || index.to_string() != value {
        return Err("scope index: positive canonical decimal u32 required".into());
    }
    Ok(index)
}
