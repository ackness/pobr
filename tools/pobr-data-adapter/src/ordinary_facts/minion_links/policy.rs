//! Fixed relationship/alias curation, with explicitly unprovided authoring evidence.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::skill_stat_map::policy::{Source, identity};
use super::super::stat_set_codec::policy::hex;
use crate::native_facts::reader::VerifiedFacts;
use crate::raw_source::sha256_hex;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    catalog: Catalog,
    pub source: Source,
    projection_schema: String,
    owned_fields: Vec<String>,
    pub alias_catalog: AliasCatalog,
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
struct Catalog {
    domain: String,
    output_file: String,
    output_schema: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AliasCatalog {
    pub aliases: Vec<Alias>,
    pub source_declaration: Declaration,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Alias {
    pub id: String,
    pub monster_variety_id: String,
    pub source_catalog_row: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Declaration {
    snapshot_version: String,
    path: String,
    sha256: String,
    metadata: Value,
    role: String,
}

impl Policy {
    pub fn validate(&self, input: &VerifiedFacts) -> Result<(), String> {
        if self.schema != "pobr-ordinary-minion-links-policy/v1"
            || self.projection_schema != "pobr-minion-links/target-boolean-fixed-curation-v1"
            || self.catalog.domain != "granted-effect-minions"
            || self.catalog.output_file != super::FILE
            || self.catalog.output_schema != super::SCHEMA
            || self.owned_fields != ["ignore_minion_types"]
        {
            return Err(
                "unsupported minion-links policy schema/domain/projection/owned fields".into(),
            );
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
            return Err("minion-links policy target patch / manifest mismatch".into());
        }
        self.source.validate(super::FILE, super::SCHEMA)?;
        let declaration = &self.alias_catalog.source_declaration;
        // Validate declaration shape only: never read the unprovided authoring catalog.
        Source {
            snapshot_version: declaration.snapshot_version.clone(),
            path: declaration.path.clone(),
            sha256: declaration.sha256.clone(),
            metadata: declaration.metadata.clone(),
        }
        .validate("minions.json", "minions/v1")?;
        if declaration.role
            != "parent-verified policy authoring only; not a future producer-read input"
        {
            return Err("unsupported alias authoring declaration role".into());
        }
        let mut ids = BTreeSet::new();
        for alias in &self.alias_catalog.aliases {
            if alias.id.is_empty() || alias.monster_variety_id.is_empty() || !ids.insert(&alias.id)
            {
                return Err("empty/duplicate policy alias or empty variety identity".into());
            }
        }
        identity(
            &self.evidence.source_check.path,
            "source-check declaration path",
        )?;
        hex(
            &self.evidence.source_check.sha256,
            64,
            "source-check declaration SHA256",
        )?;
        if self.statement.trim().is_empty() {
            return Err("empty minion-links compatibility statement".into());
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    source_check: Reference,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    path: String,
    sha256: String,
}
