//! SkillStatMap overlay domain schema (`overlay/skill_stat_map.json`,
//! schema `skill_stat_map/v1`).
//!
//! Data source: vendor PoB2 `Data/SkillStatMap.lua` (954 global stat →
//! modifier-constructor mappings) plus the `statMap` field of each statSet
//! in `Data/Skills/{act_*,sup_*,other}.lua` (per-set overrides;
//! minion/spectre are deferred). Deterministically extracted by
//! `sync-pob-catalog extract-lua --what stat-map`.
//!
//! **Extraction-fidelity principle**: a mod constructor's tags / nested
//! tables are turned into plain tables and stored in JSON **as-is** —
//! extraction does no semantic filtering; deciding "which tags/fields are
//! supported" is the engine's job
//! (`pobr-core::rules::stat_map_engine`). This is what makes the overlay's
//! drift diff meaningful when vendor updates. When a field can't be
//! serialized (e.g. a Lua function value), the whole entry is marked
//! `_unextractable: true` and reported, not silently dropped.
//!
//! Merge semantics (consumer side, matching vendor
//! `Modules/CalcActiveSkill.lua:112` line for line):
//! `injected value = entry.value or stat value × (entry.mult or 1) × scalar / (entry.div or 1) + (entry.base or 0)`
//! (a group element uses the group-level parameters instead of the
//! entry-level ones; scalar is fixed at 1.0). This module only defines the
//! serde shape, zero logic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A faithful JSON representation of an arbitrary plain Lua value (used for
/// tag fields / a mod constructor's literal value).
///
/// `untagged`: serializes to a bare JSON value; object keys are sorted
/// deterministically via [`BTreeMap`]'s dictionary order. Lua function
/// values can't be represented — the extractor marks the whole entry
/// `_unextractable` when it hits one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatMapValue {
    /// A boolean (e.g. a flag constructor's `true`, or a tag's
    /// `limitTotal = true`).
    Bool(bool),
    /// A number (Lua numbers are always f64).
    Number(f64),
    /// A string (e.g. a tag's `var` / `stat` name).
    Text(String),
    /// An array (a Lua table with only consecutive integer keys 1..n, e.g.
    /// `DistanceRamp`'s ramp-point list).
    List(Vec<StatMapValue>),
    /// An object (a Lua table with string keys, keys in dictionary order).
    Table(BTreeMap<String, StatMapValue>),
}

/// A single mod-constructor call captured (vendor's `mod()` / `flag()` /
/// `skill()`) or a group.
///
/// Vendor's three constructors share the same shape (the `makeSkillMod`
/// family in `Modules/Data.lua`): `flag(name, ...)` =
/// `mod(name, "FLAG", true, 0, 0, ...)`; `skill(key, value, ...)` =
/// `mod("SkillData", "LIST", {key, value}, 0, 0, ...)`. The extractor
/// records which constructor was called as [`Self::kind`], so the engine
/// doesn't have to re-infer it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatMapMod {
    /// Element kind: `"mod"` / `"flag"` / `"skill_data"` / `"group"`
    /// (group = vendor's nameless nested mod list, carrying group-level
    /// merge parameters).
    pub kind: String,
    /// PoB2's internal ModName, **kept as-is** (e.g. `FireDamage` / `Speed`
    /// / `CritChance`); translation to PoBR's ModName lives in the engine's
    /// Rust constant table (framework semantics, the L4 brake). Absent for a group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Aggregation type, raw text: `BASE` / `INC` / `MORE` / `FLAG` /
    /// `LIST` / `OVERRIDE`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<String>,
    /// The constructor's literal value (most mods have `nil` here — the
    /// value is filled in at runtime by the merge formula; a flag is
    /// always `true`; skill_data is a `{key, value}` table; a LIST mod is a
    /// nested table).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<StatMapValue>,
    /// ModFlag token names (guaranteed to be names thanks to the stub's
    /// self-mapping; `0` → empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag token names (same as above; `OR64(a, b)` expands into
    /// multiple names).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// Tag tables, **turned into plain tables as-is** (no filtering; the
    /// engine adds support batch by batch, per tag `type`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<BTreeMap<String, StatMapValue>>,
    /// Group-level scalar variable name (vendor's
    /// `checkForScalarMultiplier` reverse-looks-up `Multiplier:<scalar>`;
    /// the engine fixes scalar=1.0 and puts entries that use a scalar into
    /// Unsupported).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar: Option<String>,
    /// Group-level merge parameter (only for `kind == "group"`; overrides
    /// the entry-level parameter of the same name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub div: Option<f64>,
    /// See [`Self::div`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mult: Option<f64>,
    /// See [`Self::div`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<f64>,
    /// The group's nested mod list (only for `kind == "group"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<StatMapMod>,
}

/// A single stat → modifier mapping entry (one value of vendor's statMap table).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatMapEntry {
    /// Entry-level merge parameter `div` (e.g. `total_cast_time_+_ms`'s
    /// 1000, milliseconds → seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub div: Option<f64>,
    /// Entry-level merge parameter `mult`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mult: Option<f64>,
    /// Entry-level merge parameter `base`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<f64>,
    /// Entry-level value override (e.g. `global_bleed_on_hit`'s 100 —
    /// always injects 100 regardless of the stat's value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// A skill flag name (e.g. `skill_can_fire_arrows` → `"arrow"`;
    /// consumed by PoB2's statSet flags path rather than the merge formula
    /// — the engine's first batch puts these in Unsupported).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_flag: Option<String>,
    /// Mod-constructor / group list (vendor table's array part, order preserved).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<StatMapMod>,
    /// Extraction-fidelity marker: the entry has an unserializable field
    /// like a Lua function value (the engine puts it in Unsupported).
    #[serde(
        default,
        rename = "_unextractable",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub unextractable: bool,
}

/// Top level of `overlay/skill_stat_map.json` (from the consumer's
/// perspective: the `_meta` header is provenance info, ignored by default
/// via serde along with other unknown fields; the consumer just takes
/// `global` / `per_stat_set`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SkillStatMapDef {
    /// Global mapping: stat id → entry (vendor `Data/SkillStatMap.lua`, keys in dictionary order).
    pub global: BTreeMap<String, StatMapEntry>,
    /// Per-statSet overrides: granted effect id → statSet index (a decimal
    /// string, the 1-based index into vendor's `statSets` array) → stat id
    /// → entry. Lookup semantics: a per-set hit takes priority, a miss
    /// falls back to [`Self::global`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_stat_set: BTreeMap<String, BTreeMap<String, BTreeMap<String, StatMapEntry>>>,
}
