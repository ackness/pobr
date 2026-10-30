//! Optional native facts for an entire ordinary passive, separate from parser rules.

use serde::{Deserialize, Serialize};

/// An evidence address within a fixed source resource; never opened by the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeStatReference {
    pub resource: String,
    pub sha256: String,
    pub pointer: String,
}

/// Source identity of a raw passive stat, retained on the resulting modifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeStatProvenance {
    pub game_patch: String,
    pub source_manifest_sha256: String,
    pub passive_id: String,
    pub passive_row: u32,
    pub stat_row: u32,
    pub references: Vec<NativeStatReference>,
}

/// A signed game value with its stable stat ID and source evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeStat {
    pub id: String,
    pub value: i64,
    pub provenance: NativeStatProvenance,
}

/// Reviewed source context, not an interpreter for native numeric enum fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePassiveContext {
    /// Entire ordinary passive: unconditional, non-local, player-only stats.
    UnconditionalPlayer,
    /// Any context outside this bounded contract must use the original text.
    #[serde(other)]
    Unsupported,
}

/// Complete native payload bound to one version, node definition and original text.
///
/// Producers must attest the whole source context, including absence of choices,
/// requirements, grants and alternate actors. No partial stat bundles are eligible.
/// The current consumer supports only a single stat on the default tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePassiveStats {
    pub data_version: String,
    /// The build's tree version, if known; `None` only matches an unversioned build.
    pub tree_version: Option<String>,
    pub node_id: u32,
    pub node_slug: String,
    pub modifier_texts: Vec<String>,
    pub context: NativePassiveContext,
    pub stats: Vec<NativeStat>,
}
