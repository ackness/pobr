//! Bind the target and archival catalog to the supported field projection.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Catalog, prepared, projection::OWNED_FIELDS};
use crate::native_facts::reader::VerifiedFacts;
use crate::ordinary_facts::skill_stat_map::policy::{Source, identity};
use crate::ordinary_facts::stat_set_codec::policy::hex;
use crate::raw_source::sha256_hex;

pub(super) const PROJECTION: &str = "pobr-actor-fields/legacy-bb52-v1";
const SKILL_PROJECTION: &str = "pobr-actor-fields/target-skills-v2";
const REFERENCE_COMMIT: &str = "bb52d6b368307457eb9c54bb13f1829993d390b1";
const REFERENCES: [(&str, &str); 2] = [
    (
        "vendor/PathOfBuilding-PoE2/src/Export/Scripts/minions.lua",
        "d24b1e3eb9b3f730eaed2dafeaff2c3d23cc0427a108c8a04853018ac502da59",
    ),
    (
        "vendor/PathOfBuilding-PoE2/src/Modules/Common.lua",
        "6e416b1f1a2454c5ed785aa7f349eb5a68e13b41fa9150ab010d5ec2b7bd7ad5",
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    schema: String,
    target: Target,
    catalog: CatalogSpec,
    pub source: Source,
    pub projection_schema: String,
    pub projection_references: Vec<ProjectionReference>,
    owned_fields: Vec<String>,
    pub retained_compatibility_fields_observed: Vec<String>,
    #[serde(default)]
    skill_list: Option<SkillList>,
    #[serde(default)]
    pub prepared_fields: Option<prepared::Policy>,
    pub evidence: Evidence,
    pub statement: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillList {
    projection: String,
    authored_additions: Vec<String>,
    authoring_reference: ProjectionReference,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    game_patch: String,
    facts_manifest_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSpec {
    domain: String,
    output_file: String,
    output_schema: String,
    variety_identity_field: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectionReference {
    path: String,
    sha256: String,
    vendor_commit: String,
    role: String,
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

impl Policy {
    pub fn target_skills(&self) -> bool {
        self.skill_list.is_some()
    }

    pub fn owned_fields(&self) -> Vec<&'static str> {
        let mut fields = OWNED_FIELDS.to_vec();
        if self.target_skills() {
            fields.push("skill_list");
        }
        if self.prepared_fields.is_some() {
            fields.extend(prepared::FIELDS);
        }
        fields
    }

    pub fn validate(&self, input: &VerifiedFacts, catalog: Catalog) -> Result<(), String> {
        let v1 = self.schema == "pobr-ordinary-actor-catalog-policy/v1"
            && self.projection_schema == PROJECTION
            && self.skill_list.is_none()
            && self.prepared_fields.is_none();
        let v2 = self.schema == "pobr-ordinary-actor-catalog-policy/v2"
            && self.projection_schema == SKILL_PROJECTION
            && self.skill_list.is_some()
            && self.prepared_fields.is_none();
        let v3 = self.schema == "pobr-ordinary-actor-catalog-policy/v3"
            && self.projection_schema == "pobr-actor-fields/target-prepared-v3"
            && self.skill_list.is_some()
            && self.prepared_fields.is_some();
        if !v1 && !v2 && !v3 {
            return Err("unsupported actor policy/projection schema".into());
        }
        if let Some(prepared) = &self.prepared_fields {
            prepared.validate()?;
        }
        if let Some(skills) = &self.skill_list {
            let (path, sha) = match catalog {
                Catalog::Minions => (
                    "vendor/PathOfBuilding-PoE2/src/Export/Minions/Minions.txt",
                    "47ef370dad6cf7251d43f7edc5a190c88a46c263357995f488b74b9c22e6cb10",
                ),
                Catalog::Spectres => (
                    "vendor/PathOfBuilding-PoE2/src/Export/Minions/Spectres.txt",
                    "8231e8819a106e204d29a0cbe23d678f59cbf6dcd6431c00f5beafaafc7fbecf",
                ),
            };
            let reference = &skills.authoring_reference;
            if skills.projection != "MonsterVarieties.GrantedEffects/ordered-v1"
                || !skills.authored_additions.is_empty()
                || reference.path != path
                || reference.sha256 != sha
                || reference.vendor_commit != REFERENCE_COMMIT
                || reference.role
                    != "archived declaration of no extra skills; never a runtime/producer read or execution input"
            {
                return Err("unsupported actor skill-list ownership/declaration".into());
            }
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
            return Err("actor policy target patch / manifest mismatch".into());
        }
        if self.catalog.domain != catalog.name()
            || self.catalog.output_file != catalog.file()
            || self.catalog.output_schema != catalog.schema()
            || self.catalog.variety_identity_field != catalog.identity_field()
        {
            return Err("actor policy catalog domain/file/schema/identity mismatch".into());
        }
        self.source.validate(catalog.file(), catalog.schema())?;
        let owned: BTreeSet<_> = self.owned_fields.iter().map(String::as_str).collect();
        if owned.len() != self.owned_fields.len()
            || owned != self.owned_fields().into_iter().collect()
        {
            return Err("actor policy owned fields differ from the supported projection".into());
        }
        let mut retained = BTreeSet::new();
        for field in &self.retained_compatibility_fields_observed {
            identity(field, "retained field name")?;
            if owned.contains(field.as_str()) || !retained.insert(field) {
                return Err("invalid/duplicate retained compatibility field".into());
            }
        }
        // This list is historical evidence, not a whitelist of fields to preserve.
        identity(
            &self.evidence.source_check.path,
            "source-check reference path",
        )?;
        hex(
            &self.evidence.source_check.sha256,
            64,
            "source-check reference SHA256",
        )?;
        if self.statement.trim().is_empty() {
            return Err("empty compatibility statement".into());
        }
        let mut references = BTreeSet::new();
        for reference in &self.projection_references {
            if reference.vendor_commit != REFERENCE_COMMIT
                || reference.role
                    != "archived projection semantics only; never a runtime/producer read or execution input"
                || !REFERENCES.contains(&(reference.path.as_str(), reference.sha256.as_str()))
                || !references.insert(&reference.path)
            {
                return Err(
                    "actor projection reference differs from the supported archived recipe".into(),
                );
            }
        }
        if references.len() != REFERENCES.len() {
            return Err("incomplete actor projection reference evidence".into());
        }
        Ok(())
    }
}
