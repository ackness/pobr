//! StatDescriptions overlay domain schema
//! (`overlay/stat_descriptions.json`, schema `stat_descriptions/v1`).
//!
//! Data source: vendor PoB2 `Data/StatDescriptions/*.lua` — the canonical
//! display-text template for each stat_id (compiled from GGG's
//! `stat_descriptions.txt`). Deterministically extracted by
//! `sync-pob-catalog extract-lua --what stat-descriptions`: luajit loads
//! the description tables in a minimal environment, feeds each stat_id a
//! representative value (V=1) and renders a line of text; the Rust side
//! groups the result by scope and serializes with a BTreeMap for
//! byte-stable dictionary order.
//!
//! Purpose (the gap this fills — "a second stat_id → Modifier channel"): a
//! lot of the game's tree nodes / item implicits / gem mods are given in
//! **stat_id** form (rather than English text) in the game data. This
//! table turns a stat_id back into canonical English text, which then goes
//! through `parse_mod_engine` to become a Modifier — a second injection
//! path running in parallel with the existing English-text channel; once
//! the two paths' output converges, consumers can switch over domain by domain.
//!
//! **Extraction-fidelity principle** (matches [`crate::catalog::stat_map`]):
//! - Rendered text is stored in JSON **as-is** — extraction does no
//!   semantic filtering; deciding "which stat_id is supported" is the
//!   engine's (`parse_mod_engine`'s) job.
//! - Multiple scopes (the root `stat_descriptions` plus
//!   `passive_skill_*`, etc.) are **each kept in their own section**;
//!   precedence isn't decided at extraction time — "child overrides
//!   parent" is decided by the consumer's §B generator, so the overlay's
//!   drift diff stays readable scope by scope when vendor updates.
//! - A compound descriptor spanning multiple stats **isn't force-parsed**
//!   (one sentence tied to several stat_ids) — the template and its member
//!   list are kept as-is for §B to handle.
//! - A stat_id with no renderable variant (e.g. only a negative limit) is
//!   recorded in the `unrendered` diagnostic set, not silently dropped.
//!
//! This module only defines the serde shape, zero logic.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The body of the StatDescriptions overlay document (the flat part
/// outside the `_meta` header).
///
/// Grouped by scope name (the root `stat_descriptions` plus each dedicated
/// domain like `passive_skill_stat_descriptions`); each section
/// independently holds the single / compound / unrendered three
/// categories, with precedence decided by the consumer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatDescriptionsDef {
    /// scope name → that scope's description set (BTreeMap for deterministic dictionary order).
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeDescriptions>,
}

/// The description set for a single scope (the "own keys" of one
/// StatDescriptions file).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopeDescriptions {
    /// Single-stat descriptor → the rendered canonical text lines (a
    /// multi-line description keeps line order). This is the primary input
    /// to the §B generator: feed each line into `parse_mod_engine`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub single: BTreeMap<String, Vec<String>>,
    /// A multi-stat (compound) descriptor → the template as-is plus its
    /// member list (not force-parsed). Key = the descriptor's first
    /// stat_id (vendor's `stats[1]`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compound: BTreeMap<String, CompoundDescription>,
    /// stat_ids with no renderable variant (for diagnostics; §B skips
    /// these, left for a manual overlay addition).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unrendered: BTreeSet<String>,
}

/// A multi-stat descriptor kept as-is (one template sentence bound to
/// several stat_ids).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompoundDescription {
    /// All the stat_ids this descriptor is bound to (vendor's
    /// `descriptor.stats`, original order).
    pub member_stats: Vec<String>,
    /// The template text as-is (includes `{0}`/`{1}` placeholders and
    /// literal `\n` newlines; §B handles it on its own).
    pub template: String,
}
