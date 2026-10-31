//! Pinned compatibility decisions, separate from target relationship facts.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    pub source: Source,
    pub evidence: Evidence,
    pub statement: String,
    pub owner_precedence: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
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
    target_skill_gems: Reference,
    target_producer_receipt_sha256: String,
    scope: String,
}

impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-gem-effects-policy/v1" {
            return Err("unsupported gem-effects policy schema".into());
        }
        hex(&self.target.facts_manifest_sha256, 64)?;
        if input.manifest["source_identity"]["game_patch"].as_str()
            != Some(self.target.game_patch.as_str())
            || self.target.facts_manifest_sha256 != sha256_hex(&input.manifest_bytes)
        {
            return Err("codec policy target patch / facts manifest mismatch".into());
        }
        if self.owner_precedence != "base-first-legacy-owner-overlay-last/v1" {
            return Err("unsupported gem-effects owner precedence policy".into());
        }
        hex(&self.source.sha256, 64)?;
        validate_metadata(&self.source.metadata)?;
        if self.source.snapshot_version.trim().is_empty()
            || self.source.path
                != format!(
                    "data/{}/overlay/gem_effects.json",
                    self.source.snapshot_version
                )
            || !self.source.metadata["regen_command"]
                .as_str()
                .is_some_and(|cmd| cmd.ends_with(&format!("--out {}", self.source.path)))
        {
            return Err(
                "codec legacy version/path differs from original extraction metadata".into(),
            );
        }
        if self.statement.trim().is_empty() || self.evidence.scope.trim().is_empty() {
            return Err(
                "gem-effects policy needs a compatibility statement and evidence scope".into(),
            );
        }
        for reference in [
            &self.evidence.source_check,
            &self.evidence.legacy_runtime_manifest,
            &self.evidence.target_skill_gems,
        ] {
            if reference.path.trim().is_empty() {
                return Err("empty codec evidence reference path".into());
            }
            hex(&reference.sha256, 64)?;
        }
        hex(&self.evidence.target_producer_receipt_sha256, 64)?;
        Ok(())
    }
}

pub(super) fn validate_metadata(meta: &Value) -> Result<(), String> {
    if meta["schema"] != "gem_effects/v1"
        || meta["generator"] != "sync-pob-catalog extract-lua"
        || meta["vendor"] != "PathOfBuilding-PoE2"
        || meta["extracted_files"] != json!(["Data/Gems.lua"])
    {
        return Err("unsupported codec source metadata schema/generator/vendor/files".into());
    }
    hex(
        meta["vendor_commit"]
            .as_str()
            .ok_or("missing codec vendor commit")?,
        40,
    )?;
    for field in ["vendor_commit_subject", "regen_command"] {
        if !meta[field].as_str().is_some_and(|s| !s.trim().is_empty()) {
            return Err(format!("codec metadata needs {field}"));
        }
    }
    Ok(())
}

fn hex(value: &str, length: usize) -> Result<(), String> {
    if value.len() != length
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(format!(
            "codec policy expected {length} lowercase hexadecimal characters"
        ));
    }
    Ok(())
}
