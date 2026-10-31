//! Required metadata for guarded native candidates, independent of calculation admission.
//!
//! Pins identify bytes and declarations. They do not authenticate an acquisition,
//! promote a source, or certify a calculation. Full-build calculation stays guarded;
//! a separate native consumer must admit each supported bounded request explicitly.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Original query-only contract version; its serialization and assembly keys stay stable.
pub const NATIVE_RUNTIME_CONTRACT_VERSION: u32 = 1;
/// Player-context contract version, still outside ordinary calculation admission.
pub const NATIVE_PLAYER_CONTRACT_VERSION: u32 = 2;
/// Immutable contract location, never merged with a user patch.
pub const NATIVE_RUNTIME_PATH: &str = "base/native_runtime.json";
/// Coverage declarations retained by the assembler, without a completeness claim.
pub const NATIVE_COVERAGE_PATH: &str = "base/native_coverage.json";
/// Canonical archive reference index retained by the assembler.
pub const NATIVE_REFERENCE_INDEX_PATH: &str = "base/native_reference_index.json";
/// Selected skill identities/levels and archival projection evidence.
pub const NATIVE_SKILL_PROJECTION_PATH: &str = "base/native_skill_projection.json";
/// Multi-primary-spell projection, distinct from the historical Spark-only artifact.
pub const NATIVE_PRIMARY_SPELL_PROJECTION_FORMAT: &str = "pobr-native-skill-projection/v2";

/// Minimum projected domains for the fixed input profile, not proof of semantic closure.
/// Constants/rules and exact build binding still need coordinated calculation admission.
pub const NATIVE_REQUIRED_DOMAINS: &[&str] = &[
    "passive_tree",
    "passive_tree_meta",
    "base_items",
    "skill_gems",
    "granted_effects",
    "granted_effect_levels",
    "granted_effect_stat_sets",
    "cost_types",
];

/// Game supported by this contract version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeGame {
    #[serde(rename = "poe2")]
    Poe2,
}

/// Numerical source realm; a display language never implies another realm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeRealm {
    #[serde(rename = "INTERNATIONAL")]
    International,
}

/// Bounded selected-player and primary-spell input scopes.
/// All exclude equipment, supports, ascendancy, infusion and event conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeInputProfile {
    /// Original scope without any passive allocations.
    #[serde(rename = "sorceress-spark-level1-q0-v1")]
    SorceressSparkLevel1Q0V1,
    /// Explicit ordinary damage-passive selections, assumed active without legality certification.
    #[serde(rename = "sorceress-spark-level1-q0-ordinary-damage-passives-v1")]
    SorceressSparkLevel1Q0OrdinaryDamagePassivesV1,
    /// A pinned primary-spell catalog with exact authored gem levels, quality zero,
    /// and explicit ordinary damage passives under the fixed actor assumption.
    #[serde(rename = "selected-player-primary-spell-hit-q0-passives-v1")]
    SelectedPlayerPrimarySpellHitQ0PassivesV1,
}

/// Requested output scope, not an assertion that these outputs are implemented.
/// This excludes projectile-hit counts, encounter rates and general build totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeOutputProfile {
    #[serde(rename = "spark-single-hit-cast-cost-v1")]
    SparkSingleHitCastCostV1,
    #[serde(rename = "conditional-skill-hit-v1")]
    ConditionalSkillHitV1,
    /// Optional intrinsic quantities never imply a fabricated projectile count/lifetime.
    #[serde(rename = "conditional-skill-hit-v2")]
    ConditionalSkillHitV2,
}

/// Named PoBR interpretation scope; unknown profiles must fail, never fall back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeProfile {
    #[serde(rename = "spark-level1-q0")]
    SparkLevel1Q0,
    #[serde(rename = "spark-level1-q0-ordinary-damage-passives")]
    SparkLevel1Q0OrdinaryDamagePassives,
    #[serde(rename = "primary-spell-hit-q0-passives")]
    PrimarySpellHitQ0Passives,
}

/// Closed normalized selection inventory. Numerical catalogs retain their existing
/// domain types; the evidence map is archival and is never a calculation interpreter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePrimarySpellProjection {
    pub format: String,
    pub primary_only: bool,
    pub quality: u32,
    pub selections: Vec<NativePrimarySpellSelection>,
    pub retained_evidence: BTreeMap<String, serde_json::Value>,
}

/// Exact stable forward joins and the complete authored-level inventory for one gem.
/// Source row numbers and scaling coordinates are retained separately as evidence;
/// neither they nor gem levels stand in for requirements or the selected actor level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePrimarySpellSelection {
    pub gem_id: String,
    pub gem_effect_id: String,
    pub effect_id: String,
    pub active_skill_id: String,
    pub stat_set_id: String,
    pub gem_levels: Vec<u32>,
}

/// Versioned normalization or rule declaration, pinned independently from game facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProfilePin {
    pub profile: NativeProfile,
    pub version: u32,
    /// Digest of the explicit interpretation/evidence artifact, not a source approval.
    pub sha256: String,
}

/// Exact Default graph source identity, separate from the PoB compatibility tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTreeIdentity {
    pub source_manifest_sha256: String,
    /// Canonical relative raw resource address in the pinned archive.
    pub resource: String,
    pub resource_sha256: String,
    /// Exactly `0_5` for this contract; never interpreted as a game patch.
    pub compatibility_tag: String,
}

/// Acquisition declaration only; even a reviewed declaration is not authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeAcquisitionStatus {
    #[serde(rename = "unreviewed-acquisition-draft")]
    UnreviewedAcquisitionDraft,
    #[serde(rename = "externally-reviewed-observation")]
    ExternallyReviewedObservation,
}

/// Explicit evidence pin for the acquisition declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAcquisition {
    pub status: NativeAcquisitionStatus,
    pub evidence_sha256: String,
}

/// Only non-numeric official tree supplements are expressible in this contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTreeSupplementField {
    Coordinates,
    RendererMetadata,
}

/// Field-scoped official tree supplement; an unknown patch remains explicitly null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTreeSupplement {
    pub sha256: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub game_patch: Option<String>,
    pub fields: BTreeSet<NativeTreeSupplementField>,
    pub match_evidence_sha256: String,
}

/// Mandatory schema-4 native candidate contract; no fields imply factual defaults.
///
/// The assembly key hashes this typed contract with an empty `assembly_key` and a
/// versioned format tag (see `pobr-gamedata::manifest::native_assembly_key`). The final
/// runtime byte identity is computed separately, avoiding a self-referential hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRuntimeContract {
    pub schema_version: u32,
    pub game: NativeGame,
    pub realm: NativeRealm,
    pub game_patch: String,
    pub assembly_key: String,
    /// Exact immutable transitive input lock, never a mutable registry's latest entry.
    pub source_lock_sha256: String,
    pub descriptions_manifest_sha256: String,
    pub compiler_revision: String,
    pub tree: NativeTreeIdentity,
    pub allowed_source_manifests: BTreeSet<String>,
    pub normalization: NativeProfilePin,
    pub rules: NativeProfilePin,
    pub input_profile: NativeInputProfile,
    pub output_profile: NativeOutputProfile,
    pub coverage_sha256: String,
    pub reference_index_sha256: String,
    pub acquisition: NativeAcquisition,
    /// An empty list explicitly declares no supplements. Shared overlays are not inputs.
    pub supplements: Vec<NativeTreeSupplement>,
    /// Absent in v1, mandatory in v2. Omission preserves exact v1 canonical serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<NativePlayerArtifacts>,
}

/// Required sealed artifacts and transitive source pins for contract v2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerArtifacts {
    pub context_sha256: String,
    pub actor_binding_sha256: String,
    pub source_manifest_sha256: String,
    pub source_lock_sha256: String,
    pub parent_manifest_sha256: String,
    pub parent_source_lock_sha256: String,
}
