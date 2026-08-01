//! Per-skill override-value overlay domain schema
//! (`overlay/skill_overrides.json`).
//!
//! Data source: a hand-curated layer over vendor PoB2 `Data/Skills/*.lua`
//! for per-level columns that **don't exist** in the GGG `.dat` export
//! (pipeline/tables) — `critChance` / `attackSpeedMultiplier` /
//! `baseMultiplier` — plus a statSet's inherent `baseMods` Speed MORE.
//! Deterministically extracted by `sync-pob-catalog extract-lua` (schema id
//! `skill_overrides/v1`; see that tool's `extract_lua` module for the
//! generation side).
//!
//! Consumer: `pobr-gamedata` merges this table on top of the plain base
//! data while loading `base/granted_effect_levels.json` /
//! `base/granted_effect_stat_sets.json` (merge semantics and unit tests in
//! `pobr-gamedata::domains::skill_overrides`). This module only defines the
//! serde shape, zero logic.

use serde::{Deserialize, Serialize};

/// A value for [`SkillOverrideEntry::stat`]: the skill's base crit chance
/// (percentage points, corresponds to `SkillLevelDef::crit_chance`).
pub const OVERRIDE_STAT_CRIT_CHANCE: &str = "crit_chance";
/// A value for [`SkillOverrideEntry::stat`]: the attack speed multiplier
/// (percentage points, can be negative, corresponds to
/// `SkillLevelDef::attack_speed_multiplier`).
pub const OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER: &str = "attack_speed_multiplier";
/// A value for [`SkillOverrideEntry::stat`]: the skill's base damage
/// multiplier (corresponds to `SkillLevelDef::base_multiplier`).
pub const OVERRIDE_STAT_BASE_MULTIPLIER: &str = "base_multiplier";
/// A value for [`SkillOverrideEntry::stat`]: the statSet's inherent attack
/// speed MORE (percentage points, corresponds to
/// `SkillStatSetDef::skill_attack_speed_more`).
pub const OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE: &str = "skill_attack_speed_more";
/// A value for [`SkillOverrideEntry::stat`]: a skill DoT config boolean
/// (vendor statSet `baseMods`'s `skill("dotIs*", true)`; value 1.0 = true,
/// corresponds to the matching bit of `StatSetDef::dot_flags`). A
/// statSet-level entry (always carries `stat_set`).
pub const OVERRIDE_STAT_DOT_IS_AREA: &str = "dot_is_area";
/// Same as [`OVERRIDE_STAT_DOT_IS_AREA`] (dotIsProjectile).
pub const OVERRIDE_STAT_DOT_IS_PROJECTILE: &str = "dot_is_projectile";
/// Same as [`OVERRIDE_STAT_DOT_IS_AREA`] (dotIsSpell).
pub const OVERRIDE_STAT_DOT_IS_SPELL: &str = "dot_is_spell";
/// Same as [`OVERRIDE_STAT_DOT_IS_AREA`] (dotIsAttack).
pub const OVERRIDE_STAT_DOT_IS_ATTACK: &str = "dot_is_attack";
/// Same as [`OVERRIDE_STAT_DOT_IS_AREA`] (dotIsHit).
pub const OVERRIDE_STAT_DOT_IS_HIT: &str = "dot_is_hit";
/// A value for [`SkillOverrideEntry::stat`]: the corpse-explosion gate
/// boolean (vendor statSet `baseMods`'s `skill("explodeCorpse", true)`;
/// CalcOffence.lua:2213 uses it to inject
/// `monsterLife × corpseExplosionLifeMultiplier` into physical base
/// damage; value 1.0 = true, corresponds to `StatSetDef::explode_corpse`).
/// A statSet-level entry (always carries `stat_set`).
pub const OVERRIDE_STAT_EXPLODE_CORPSE: &str = "explode_corpse";
/// A value for [`SkillOverrideEntry::stat`]: a statSet implicit stat
/// (entries from vendor statSet's `stats` list where no level row has a
/// value = the `.dat`'s `ImplicitStats` column, not downloaded by the
/// adapter; vendor always consumes value 1,
/// `CalcTools.lua:152`'s `statSetLevel[index] or 1`). A statSet-level entry
/// (always carries `stat_set`); the stat id lives in
/// [`SkillOverrideEntry::stat_id`], corresponding to
/// `StatSetDef::implicit_stats`. Extracted as a **curated whitelist** on
/// the generation side (see `extract_skill_overrides.lua`'s header comment).
pub const OVERRIDE_STAT_IMPLICIT_STAT: &str = "implicit_stat";

/// All statSet-level dotIs* stat names (a shared list for both the
/// consumer's merge and the generator's extraction).
pub const OVERRIDE_DOT_FLAG_STATS: &[&str] = &[
    OVERRIDE_STAT_DOT_IS_AREA,
    OVERRIDE_STAT_DOT_IS_PROJECTILE,
    OVERRIDE_STAT_DOT_IS_SPELL,
    OVERRIDE_STAT_DOT_IS_ATTACK,
    OVERRIDE_STAT_DOT_IS_HIT,
];

/// A single per-skill override value. `value` and `per_level` are mutually
/// exclusive: when the stat appears at **every level** in vendor with the
/// same value, it's compressed into `value` (the consumer applies it to
/// every level row of that skill); otherwise the `per_level` breakdown is
/// kept (only the listed levels are overridden — missing levels aren't
/// filled in, staying faithful to vendor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillOverrideEntry {
    /// Vendor skill id (= `GrantedEffects.Id`, e.g. `FlickerStrikePlayer`).
    pub skill: String,
    /// The stored stat name (see this module's `OVERRIDE_STAT_*` constants).
    pub stat: String,
    /// statSet index (only present for statSet-level overrides, e.g.
    /// baseMods's Speed MORE).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_set: Option<u32>,
    /// The single value, when it's the same at every level (or
    /// level-independent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Per-level breakdown: `[[level, value], ...]`, ascending by level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_level: Option<Vec<(u32, f64)>>,
    /// The stat id for an implicit stat (only present for
    /// [`OVERRIDE_STAT_IMPLICIT_STAT`] entries, e.g.
    /// `attacks_roll_crits_twice`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_id: Option<String>,
}

/// Top level of `overlay/skill_overrides.json` (from the consumer's
/// perspective: the `_meta` header is provenance info, ignored by default
/// via serde along with other unknown fields; the consumer just takes the
/// `overrides` list).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkillOverridesDef {
    /// Override list, sorted by `(skill, stat, stat_set)`.
    pub overrides: Vec<SkillOverrideEntry>,
}
