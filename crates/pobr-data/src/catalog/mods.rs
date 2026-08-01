//! Mod pool and stat registry domain schema (`base/mods.json` /
//! `base/stats.json`, sourced from `Mods.dat` / `Stats.dat`).

use serde::{Deserialize, Serialize};

/// A stat registry entry (from `Stats.dat`).
///
/// `id` is GGG's stable string stat key (e.g. `additional_strength`) — the
/// target that a Mod's `Stat1..4` integer foreign keys resolve to, and also
/// the future primary key for i18n stat descriptions. `semantic` /
/// `category` are raw GGG integer enums (no separate lookup table, kept as
/// their raw values).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDef {
    /// Stable stat ID, i.e. `Stats.dat`'s `Id` (e.g. `additional_strength`).
    pub id: String,
    /// Whether this is a local stat (only affects the item it's on).
    pub is_local: bool,
    /// Raw GGG `Semantic` enum value (display semantics like sign /
    /// percentage / duration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<u32>,
    /// Raw GGG `Category` enum value (stat classification, can be absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<u32>,
}

/// A mod spawn-weight entry (from `Mods.SpawnWeight_Tags` +
/// `SpawnWeight_Values`'s parallel arrays).
///
/// To decide whether a base can roll this mod: scan in order for the first
/// entry whose tag matches one of the base's tags, and take its weight;
/// weight = 0 means it can't roll (same semantics as PoB2's
/// `Item:GetModSpawnWeight`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnWeight {
    /// The base tag this matches (resolves `Tags.Id`, e.g. `ring` /
    /// `str_armour`; `default` is the catch-all).
    pub tag: String,
    /// Weight value (0 = can't roll under this tag).
    pub weight: u32,
}

/// The roll range for one stat slot of a mod (from `Mods.StatNValue`, a
/// `[min, max]` pair).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModStat {
    /// The stable stat ID this slot affects (resolves the `StatN` foreign
    /// key → `Stats.Id`).
    pub stat_id: String,
    /// Roll lower bound.
    pub min: i64,
    /// Roll upper bound.
    pub max: i64,
}

/// A mod pool definition (from `Mods.dat` plus foreign-key resolution).
///
/// `name` is the English canonical mod name (prefix/suffix name, e.g. `of
/// the Brute`); other languages go through the `i18n/<lang>/mods.json`
/// sidecar (`id -> localized name`). `Stat1..4` + `Stat1Value..4Value` are
/// merged into the `stats` array (with the stat foreign key resolved and
/// empty slots skipped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDef {
    /// Stable ID, i.e. `Mods.dat`'s `Id` (e.g. `Strength1`).
    pub id: String,
    /// English canonical mod name (can be absent: a lot of internal mods
    /// have no display name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Raw GGG `ModType` enum value (no separate lookup table, kept as its
    /// raw value; can be absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_type: Option<u32>,
    /// Mod domain (a raw GGG enum value, used to decide which items this
    /// mod can apply to).
    pub domain: u32,
    /// Raw GGG `GenerationType` enum value (generation type: prefix /
    /// suffix / implicit, etc.; can be absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_type: Option<u32>,
    /// Mod generation level.
    pub level: u32,
    /// This mod's stat slots (the Stat foreign key plus its roll range
    /// already merged in, empty slots skipped).
    pub stats: Vec<ModStat>,
    /// Tags (resolves `Tags.Id`).
    pub tags: Vec<String>,
    /// Mod group (resolves the `ModType` foreign key → `ModType.Name`,
    /// i.e. PoB2's exported `group`). Mods on the same strength line (e.g.
    /// Strength1..9) share a group — this is the grouping key for tier
    /// ranking. Absent (serde defaults to `None`) in older data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Spawn-weight table (tag → weight, order-sensitive: the first
    /// matching entry wins). Absent (serde defaults to empty) in older data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawn_weights: Vec<SpawnWeight>,
}
