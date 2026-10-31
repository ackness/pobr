//! Explicit legacy codec provenance and target raw-membership exception decisions.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    pub source: Source,
    pub evidence: Evidence,
    pub statement: String,
    pub raw_ownership_exceptions: Vec<Exception>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}

#[derive(Deserialize, Serialize)]
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
    legacy_base: Reference,
    target_base: Reference,
    target_producer_receipt_sha256: String,
    pub bound_before_prepared_merges_in_both: usize,
    pub unbound_before_prepared_merges_in_both: Vec<Key>,
    scope: String,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(super) struct Key {
    pub skill: String,
    pub set_id: String,
    pub set_index: u32,
}

impl Key {
    pub fn checked(&self) -> Result<(), String> {
        if self.skill.is_empty() || self.set_id.is_empty() || self.set_index == 0 {
            return Err(
                "codec identity needs nonempty skill/set IDs and positive u32 index".into(),
            );
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Exception {
    pub key: Key,
    pub reason: String,
}

impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-stat-set-codec-policy/v1" {
            return Err("unsupported stat-set codec policy schema".into());
        }
        hex(
            &self.target.facts_manifest_sha256,
            64,
            "target manifest SHA256",
        )?;
        if input.manifest["source_identity"]["game_patch"].as_str()
            != Some(self.target.game_patch.as_str())
            || self.target.facts_manifest_sha256 != sha256_hex(&input.manifest_bytes)
        {
            return Err("codec policy target patch / facts manifest mismatch".into());
        }
        hex(&self.source.sha256, 64, "codec source SHA256")?;
        if self.source.snapshot_version.trim().is_empty()
            || self.source.path.trim().is_empty()
            || self.statement.trim().is_empty()
            || self.evidence.scope.trim().is_empty()
        {
            return Err(
                "codec policy needs legacy version, source and compatibility evidence".into(),
            );
        }
        validate_metadata(&self.source.metadata)?;
        // Snapshot version is an archived provenance label, not the target patch.
        // Its original output path is present in the preserved extraction command.
        if self.source.path
            != format!(
                "data/{}/overlay/stat_set_labels.json",
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

        for reference in [
            &self.evidence.source_check,
            &self.evidence.legacy_runtime_manifest,
            &self.evidence.legacy_base,
            &self.evidence.target_base,
        ] {
            if reference.path.trim().is_empty() {
                return Err("empty codec evidence reference path".into());
            }
            hex(&reference.sha256, 64, "codec evidence reference SHA256")?;
        }
        hex(
            &self.evidence.target_producer_receipt_sha256,
            64,
            "target producer receipt SHA256",
        )?;
        let mut exceptions = BTreeSet::new();
        for exception in &self.raw_ownership_exceptions {
            exception.key.checked()?;
            if exception.reason.trim().is_empty() || !exceptions.insert(&exception.key) {
                return Err("duplicate codec exception or empty reason".into());
            }
        }
        let mut unbound = BTreeSet::new();
        for key in &self.evidence.unbound_before_prepared_merges_in_both {
            key.checked()?;
            if !unbound.insert(key) {
                return Err("duplicate historical unbound codec key".into());
            }
        }
        Ok(())
    }
}

pub(super) fn validate_metadata(meta: &Value) -> Result<(), String> {
    if meta["schema"] != "stat_set_labels/v1"
        || meta["generator"] != "sync-pob-catalog extract-lua"
        || meta["vendor"] != "PathOfBuilding-PoE2"
    {
        return Err("unsupported codec source metadata schema/generator/vendor".into());
    }
    hex(
        meta["vendor_commit"]
            .as_str()
            .ok_or("missing codec vendor commit")?,
        40,
        "codec vendor commit",
    )?;
    for field in ["vendor_commit_subject", "regen_command"] {
        if !meta[field].as_str().is_some_and(|v| !v.trim().is_empty()) {
            return Err(format!("codec metadata needs {field}"));
        }
    }
    let files = meta["extracted_files"]
        .as_array()
        .ok_or("codec metadata needs extracted_files")?;
    if files.is_empty()
        || files
            .iter()
            .any(|v| !v.as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err("invalid codec metadata extracted_files".into());
    }
    Ok(())
}

pub(super) fn hex(value: &str, length: usize, field: &str) -> Result<(), String> {
    if value.len() != length
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(format!(
            "{field}: expected {length} lowercase hexadecimal characters"
        ));
    }
    Ok(())
}
