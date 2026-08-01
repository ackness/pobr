//! StatId→Modifier mapping-table overlay domain schema
//! (`overlay/stat_id_map.json`, schema `stat_id_map/v1`).
//!
//! The output of the "second stat_id → Modifier channel" (stage B): feeds
//! stage A's extracted `stat_descriptions.json` (stat_id → canonical text)
//! through `parse_mod_engine` line by line, and bakes the stat_ids that
//! parse successfully into modifier templates. Generated offline by
//! `sync-pob-catalog gen-stat-id-map` (consuming two overlays:
//! stat_descriptions + mod_parser_rules).
//!
//! Runtime purpose: when the game data gives a `(stat_id, raw value)` pair,
//! look it up in this table to get the pre-parsed modifier structure, and
//! inject it with `runtime value = raw value × coefficient` — skipping the
//! text round trip entirely (rendering text needs luajit, so it's only
//! available at build time). This is the second injection path running in
//! parallel with the English-text channel.
//!
//! **Design conventions**:
//! - A template separates **structure** (name / mod_type / tags / flags)
//!   from **coefficient** (the per-unit value obtained by feeding stage A a
//!   V=1 — always 1 for a linear, generic mod) — the structure is for
//!   dual-run field-by-field comparison, the coefficient is for runtime
//!   scaling. Tags use the single-source string from
//!   `pobr_core::mod_parser::canonical_tags` (no second serialization
//!   scheme is allowed); an empty string = no tag (the common case for
//!   generic mods).
//! - Multiple scopes are each kept in their own section (matching
//!   [`crate::catalog::stat_descriptions`]), with precedence left to the
//!   consumer.
//! - A stat_id that can't be parsed is recorded in `unsupported` (that
//!   channel falls back to text / special handling), not silently dropped.
//!
//! This module only defines the serde shape, zero logic.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The body of the StatId→Modifier mapping document (the flat part outside
/// the `_meta` header).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatIdMapDef {
    /// scope name → that scope's mapping (BTreeMap for deterministic dictionary order).
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeStatIdMap>,
}

/// A single scope's stat_id → modifier-template mapping.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopeStatIdMap {
    /// stat_id → the parsed modifier templates (mods from every text line
    /// of this stat_id, merged in order).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mapped: BTreeMap<String, Vec<StatIdModTemplate>>,
    /// stat_ids whose text couldn't be parsed (handled by falling back to
    /// text / special processing).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unsupported: BTreeSet<String>,
}

/// A single modifier template (structure and coefficient kept separate).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatIdModTemplate {
    /// ModName (PoBR's internal stable ID).
    pub name: String,
    /// Aggregation type: `Base` / `Inc` / `More` / `Flag` / `Override` / `List`.
    pub mod_type: String,
    /// The per-unit value obtained by feeding stage A a V=1 (runtime
    /// modifier value = raw stat value × coefficient; always 1.0 for a
    /// linear, generic mod). `None` for a non-numeric payload — see `value_kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coefficient: Option<f64>,
    /// Marks a non-numeric payload (`flag:true` / `text:...` / `nested`);
    /// omitted for a numeric one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_kind: Option<String>,
    /// The canonical tag string (`pobr_core::mod_parser::canonical_tags`;
    /// empty = no tag).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tags: String,
    /// ModFlag bits (0 = none).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub flags: u64,
    /// KeywordFlag bits (0 = none).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub keyword_flags: u64,
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}
