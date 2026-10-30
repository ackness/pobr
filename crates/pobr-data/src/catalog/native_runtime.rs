//! Required metadata for guarded native candidates, independent of calculation admission.
//!
//! Pins identify bytes and declarations. They do not authenticate an acquisition,
//! promote a source, or certify a calculation. No native profile is admitted yet.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Only this contract version is understood by the current loader.
pub const NATIVE_RUNTIME_CONTRACT_VERSION: u32 = 1;
/// Immutable contract location, never merged with a user patch.
pub const NATIVE_RUNTIME_PATH: &str = "base/native_runtime.json";
/// Coverage declarations retained by the assembler, without a completeness claim.
pub const NATIVE_COVERAGE_PATH: &str = "base/native_coverage.json";
/// Canonical archive reference index retained by the assembler.
pub const NATIVE_REFERENCE_INDEX_PATH: &str = "base/native_reference_index.json";

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

/// Fixed initial input scope: level-1 Sorceress, Spark primary branch at level 1/q0,
/// without equipment, supports, allocations, ascendancy, infusion or event conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeInputProfile {
    #[serde(rename = "sorceress-spark-level1-q0-v1")]
    SorceressSparkLevel1Q0V1,
}

/// Requested output scope, not an assertion that these outputs are implemented.
/// This excludes projectile-hit counts, encounter rates and general build totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeOutputProfile {
    #[serde(rename = "spark-single-hit-cast-cost-v1")]
    SparkSingleHitCastCostV1,
}

/// Named PoBR interpretation scope; unknown profiles must fail, never fall back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeProfile {
    #[serde(rename = "spark-level1-q0")]
    SparkLevel1Q0,
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
}
