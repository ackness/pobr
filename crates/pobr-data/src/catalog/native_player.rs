//! Source-preserving player context facts (character row, starting relations and
//! metadata archive) as produced by the RePoE adapter.
//!
//! Deserializing these DTOs validates only their shape; they carry no PoBR actor
//! applicability policy. The adapter's ordinary unarmed and runtime-constant
//! projections read the generic character metadata through these types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Game supported by the player context format.
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
}
