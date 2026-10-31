//! Source-preserving player context and the separate, bounded PoBR applicability policy.
//!
//! Deserializing these DTOs does not validate or admit an actor. `NativeData::load`
//! is the immutable catalog boundary; build admission is a separate operation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::native_runtime::{NativeGame, NativeRealm};

/// Sealed source context location.
pub const NATIVE_PLAYER_CONTEXT_PATH: &str = "base/native_player_context.json";
/// Sealed PoBR interpretation location, separate from source facts.
pub const NATIVE_ACTOR_BINDING_PATH: &str = "base/native_actor_binding.json";
/// Stable selected actor; sharing its base class never selects another character.
pub const NATIVE_PLAYER_ID: &str = "Metadata/Characters/Int/IntFourb";
/// Case-exact AO root, independent of the generic OT root.
pub const NATIVE_PLAYER_AO: &str = "Metadata/Characters/Int/IntFourB.ao";
/// Explicit generic OT root; there is no inferred AO-to-OT edge.
pub const NATIVE_GENERIC_OT: &str = "Metadata/Characters/Character.ot";

/// Archive namespace for a physical reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NativePlayerOrigin {
    Player,
    Parent,
}

/// Exact physical row evidence, scoped by the containing context's archive pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerReference {
    pub origin: NativePlayerOrigin,
    pub table: String,
    pub payload: String,
    pub pointer: String,
    pub row: usize,
    /// Required nullable source field, not a generated row-based stable ID.
    #[serde(deserialize_with = "Option::deserialize")]
    pub id: Option<String>,
}

/// Ordered starting choices; neither allocated nor granted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStartingRelation {
    pub source: NativePlayerReference,
    pub options: Vec<NativePlayerReference>,
}

/// Selected row with every physical column retained, including unknown columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerCharacter {
    pub source: NativePlayerReference,
    pub values: BTreeMap<String, Value>,
}

/// Distinct literal metadata roots declared by the source lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerMetadataRoots {
    pub character_ao: String,
    pub generic_stats_ot: String,
}

/// Ordered lexical scope identity; repeated blocks are not collapsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadataScope {
    pub id: usize,
    pub name_raw: String,
    pub open_line: usize,
}

/// One lexical assignment; offsets address the original BOM UTF-16LE resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadataAssignment {
    pub key_raw: String,
    pub value_raw: String,
    pub line: usize,
    pub scope: Vec<NativeMetadataScope>,
    pub plain_stats_scope: bool,
    pub value_byte_start: usize,
    pub value_byte_end: usize,
}

/// Complete raw line with its physical UTF-16 byte span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadataRecord {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: usize,
    pub literal: String,
    pub scope: Vec<NativeMetadataScope>,
}

/// Literal same-extension parent; no effective inheritance merge is performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadataExtends {
    pub line: usize,
    pub literal: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target: Option<String>,
}

/// Source metadata file, retaining scoped/unknown assignments and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadataFile {
    pub format: String,
    pub path: String,
    pub bytes: usize,
    pub encoding: String,
    pub merge_policy: String,
    pub records: Vec<NativeMetadataRecord>,
    pub assignments: Vec<NativeMetadataAssignment>,
    pub extends: Vec<NativeMetadataExtends>,
    /// Opaque source diagnostics, never executable policy or a validity marker.
    pub diagnostics: Vec<Value>,
}

/// Complete source metadata archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMetadataArchive {
    pub format: String,
    pub files: BTreeMap<String, NativeMetadataFile>,
}

/// Source join from a plain Stats assignment to an optional exact native stat ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerMetadataStat {
    pub path: String,
    pub assignment: usize,
    pub key_raw: String,
    pub value_raw: String,
    pub scope: Vec<NativeMetadataScope>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub stat: Option<NativePlayerReference>,
}

/// Exact source projection, without PoBR actor applicability or numerical defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerFacts {
    pub format: String,
    pub character: NativePlayerCharacter,
    pub starting_relation: NativeStartingRelation,
    pub all_starting_relations: Vec<NativeStartingRelation>,
    pub selected_class_overrides: Vec<NativePlayerReference>,
    pub metadata_roots: NativePlayerMetadataRoots,
    pub literal_closure_heights: BTreeMap<String, usize>,
    pub metadata_stats: Vec<NativePlayerMetadataStat>,
    pub binding_policy: String,
    pub allocation_policy: String,
}

/// Physical source table; schema metadata and unknown columns remain unmodified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerPhysicalTable {
    pub table: String,
    pub raw_resource: String,
    pub row_key: String,
    pub physical_fields: Vec<String>,
    pub columns: Vec<Value>,
    pub validation: Value,
    pub rows: Vec<NativePlayerPhysicalRow>,
}

/// One physical row; `_row` is evidence, never an actor selection key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerPhysicalRow {
    #[serde(rename = "_row")]
    pub row: usize,
    pub values: BTreeMap<String, Value>,
}

/// Source context artifact. Raw inventory/provenance are retained without approval claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerContextDef {
    pub schema_version: u32,
    pub game: NativeGame,
    pub realm: NativeRealm,
    pub game_patch: String,
    pub source_manifest_sha256: String,
    pub source_lock_sha256: String,
    pub parent_manifest_sha256: String,
    pub parent_source_lock_sha256: String,
    pub facts: NativePlayerFacts,
    pub metadata: NativeMetadataArchive,
    pub starting_passive_skills: NativePlayerPhysicalTable,
    pub keyword_popups: NativePlayerPhysicalTable,
    /// All eight original JSON files as exact UTF-8 text, including the byte-pinned manifest.
    /// This is archival evidence only; typed fields above are independently checked.
    pub source_archive: BTreeMap<String, String>,
}

/// Bounded applicability policies; neither asserts source inheritance or allocation legality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeActorPolicy {
    #[serde(rename = "selected-player-generic-ot-v1")]
    SelectedPlayerGenericOtV1,
    #[serde(rename = "selected-player-generic-ot-ordinary-damage-passives-v1")]
    SelectedPlayerGenericOtOrdinaryDamagePassivesV1,
}

/// Required predicate for explicitly selected passive sources in the conditional model.
pub const NATIVE_PASSIVES_ACTIVE_CONDITION: &str =
    "selected-ordinary-damage-passives-assumed-active";
/// Every allocation outside the complete ordinary damage-packet scope stays excluded.
pub const NATIVE_OTHER_ALLOCATIONS_EXCLUSION: &str =
    "allocations-other-than-explicit-ordinary-damage-passives";
/// Required limitation of the passive policy, also retained in consumer result assumptions.
pub const NATIVE_ALLOCATION_ASSUMPTION: &str = "Explicitly selected eligible ordinary damage passives are assumed active for conditional comparison; allocation-legality (point budget, reachability and progression) is unavailable. Starting choices never grant nodes automatically.";

/// Explicit evaluation contract, independent of any particular skill's implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeEvaluationKind {
    #[serde(rename = "conditional-skill-hit-v1")]
    ConditionalSkillHitV1,
}

/// Honest status of applying generic OT facts to the selected player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeModelStatus {
    #[serde(rename = "conditional-model")]
    ConditionalModel,
}

/// Address of one source assignment, preserving path and order rather than collapsing keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAssignmentSelector {
    pub path: String,
    pub assignment: usize,
}

/// Explicit finite bonus in percentage points, never a total multiplier or default bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCriticalBonus {
    pub bonus_percent: f64,
    pub source: NativeAssignmentSelector,
    pub stat: NativePlayerReference,
    /// Raw glossary meaning remains retained in the physical table and archive.
    pub meaning: Vec<NativePlayerReference>,
}

/// Finite dispositions are local to this exact profile, never globally effectless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeFactDispositionKind {
    Consumed,
    Inactive,
    OutsideOutputScope,
    Unresolved,
}

/// Per-fact interpretation evidence; unresolved relevant input blocks later admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFactDisposition {
    /// JSON pointer into the typed context artifact.
    pub selector: String,
    pub disposition: NativeFactDispositionKind,
    pub relevant_to_conditional_hit: bool,
    pub reason: String,
}

/// Separate PoBR applicability policy and normalized actor quantity.
/// Public DTO fields carry declarations, not admission authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActorBinding {
    pub schema_version: u32,
    pub policy: NativeActorPolicy,
    pub evaluation: NativeEvaluationKind,
    pub model_status: NativeModelStatus,
    pub context_sha256: String,
    pub actor_id: String,
    pub actor_level: u32,
    pub critical_bonus: NativeCriticalBonus,
    pub required_conditions: Vec<String>,
    pub excluded_sources: Vec<String>,
    pub dispositions: Vec<NativeFactDisposition>,
    pub assumptions: Vec<String>,
}

impl NativeActorBinding {
    /// Declared applicability model; validity comes from the containing NativeData.
    pub fn policy(&self) -> NativeActorPolicy {
        self.policy
    }
    /// Declared bounded evaluation contract.
    pub fn evaluation(&self) -> NativeEvaluationKind {
        self.evaluation
    }
    /// Exact selected character stable ID.
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }
    /// Explicit selected actor level.
    pub fn actor_level(&self) -> u32 {
        self.actor_level
    }
    /// Explicit source-derived extra-damage bonus and its provenance.
    pub fn critical_bonus(&self) -> &NativeCriticalBonus {
        &self.critical_bonus
    }
    /// Complete retained per-fact interpretation record.
    pub fn dispositions(&self) -> &[NativeFactDisposition] {
        &self.dispositions
    }
    /// Later admission must reject a packet with unresolved relevant inputs.
    pub fn has_unresolved_hit_inputs(&self) -> bool {
        self.dispositions.iter().any(|fact| {
            fact.relevant_to_conditional_hit
                && fact.disposition == NativeFactDispositionKind::Unresolved
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_references_keep_namespace_case_and_required_nullable_id() {
        let source = json!({"origin":"parent","table":"Characters","row":7,
            "id":NATIVE_PLAYER_ID,"payload":"tables/Characters.json","pointer":"/rows/7/values"});
        let reference: NativePlayerReference = serde_json::from_value(source.clone()).unwrap();
        assert_eq!(serde_json::to_value(reference).unwrap(), source);
        let mut missing = source;
        missing.as_object_mut().unwrap().remove("id");
        assert!(serde_json::from_value::<NativePlayerReference>(missing).is_err());
        assert_ne!(NATIVE_PLAYER_ID, "Metadata/Characters/Int/IntFourB");
    }

    #[test]
    fn explicit_bonus_and_finite_disposition_contracts_have_no_implicit_defaults() {
        let missing = json!({"source":{"path":NATIVE_GENERIC_OT,"assignment":15},
            "stat":{"origin":"parent","table":"Stats","row":595,"id":"base_critical_hit_damage_bonus",
                "payload":"tables/Stats.json","pointer":"/rows/595/values"},"meaning":[]});
        assert!(serde_json::from_value::<NativeCriticalBonus>(missing).is_err());
        for name in ["effectless", "ignored", "default"] {
            assert!(serde_json::from_value::<NativeFactDispositionKind>(json!(name)).is_err());
        }
        assert!(
            serde_json::from_value::<NativeActorPolicy>(json!("infer-ao-ot-inheritance")).is_err()
        );
    }
}
