//! Minion / spectre overlay domain schema (`overlay/minions.json` /
//! `overlay/spectres.json` / `overlay/granted_effect_minions.json`).
//!
//! Data source: vendor PoB2 `Data/Minions.lua` (32 entries) /
//! `Data/Spectres.lua` (593 entries) — both files are headed
//! "automatically generated" and are themselves a deterministic projection
//! of the `.dat` files, the actual input to PoB2's calc engine; faithfully
//! extracted field by field by `sync-pob-catalog extract-lua --what
//! minions|spectres` running under luajit in a minimal stub environment
//! (schema id `minions/v1` / `spectres/v1`). Physically they live under
//! `overlay/` (decision 1: production tooling owns the layer — extract-lua
//! output goes to overlay, and migrates to base once the pipeline gains
//! `.dat` table support, as a byte-equivalent migration commit).
//!
//! Relationship to the existing `pobr_data::minion::MinionDef`
//! (hand-transcribed constants + calc-consumption shape): this module is
//! the **storage serde shape** (the full v2 field set, including the
//! structured `mod_list` modList); the consumer side will eventually
//! migrate entirely to this type and the hand-transcribed constants will be
//! deleted (A6). For now the two coexist, with a load-time unit test
//! locking them value-equal (a migration invariant). This module has zero
//! logic, zero I/O.
//!
//! `overlay/granted_effect_minions.json` is the gem → minion foreign-key
//! **sidecar** (owned per decision §4-10): `minionList` isn't in the
//! `.dat`s — it comes from PoB2's Export template's hand-written
//! `#minionList` directive (`Export/Scripts/skills.lua:771-776`), extracted
//! by extract-lua alongside `Data/Skills/*.lua`; merging it into
//! `GrantedEffectDef` is a wiring concern — this module only defines the
//! sidecar's shape.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A faithful transcription of a Lua scalar value (a mod's construction
/// argument / tag field value can be a bool, number, or string; the stub
/// records the argument as-is, and the consumer explicitly marks shapes it
/// doesn't recognize as Unsupported).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LuaValueDef {
    /// A boolean (e.g. the value=true from a `flag(...)` construction).
    Bool(bool),
    /// A number (a Lua number, uniformly f64).
    Number(f64),
    /// A string (e.g. a ModFlag name self-mapped through `__index` in the
    /// stub environment).
    Text(String),
}

/// A faithful serialization of one `mod(...)` / `flag(...)` construction
/// inside `modList` (corresponds to the arguments vendor
/// `Modules/Data.lua:56 makeSkillMod` takes; dropped-argument warning:
/// construction arguments must be serialized in full — dropping one
/// silently corrupts the data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinionModDef {
    /// Mod name (e.g. `StunDuration`).
    pub name: String,
    /// Mod type literal (`BASE`/`INC`/`MORE`/`OVERRIDE`/`FLAG`).
    #[serde(rename = "type")]
    pub mod_type: String,
    /// Numeric/boolean value (`flag(...)` gives true).
    pub value: LuaValueDef,
    /// The flags bits (a literal number in vendor's data files; may be a
    /// name string under the stub's self-mapping environment — transcribed
    /// faithfully, with bit-semantics interpretation left to the consumer's
    /// wiring).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<LuaValueDef>,
    /// The keywordFlags bits (same as above).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword_flags: Option<LuaValueDef>,
    /// Trailing tag tables (e.g.
    /// `{ effectName = "ArmourBreak", effectType = "Buff" }`), each tag a
    /// flat `key → scalar` map with a fixed key order (BTreeMap).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<BTreeMap<String, LuaValueDef>>,
}

/// A single minion / spectre entry (corresponds to Lua's
/// `minions["<Id>"]`; field names are vendor's keys converted to
/// snake_case). An optional field being absent means the key is absent in
/// vendor (transcribed faithfully — no defaults materialized here; see
/// `pobr_data::minion::MinionDef`'s doc for the consumer-side default
/// semantics: armour/evasion default to 1.0, energyShield defaults to 0.0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinionEntryDef {
    /// Stable ID (the Lua key; for a spectre this is the full metadata
    /// path, e.g.
    /// `Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre`).
    pub id: String,
    /// English canonical name (`name`).
    pub name: String,
    /// Monster tags (`monsterTags`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monster_tags: Vec<String>,
    /// Life normalization multiplier (`life`).
    pub life: f64,
    /// Damage normalization multiplier (`damage`).
    pub damage: f64,
    /// Damage range width (`damageSpread`).
    pub damage_spread: f64,
    /// Base attack interval in seconds (`attackTime`).
    pub attack_time: f64,
    /// Attack range (`attackRange`).
    pub attack_range: f64,
    /// Accuracy normalization multiplier (`accuracy`).
    pub accuracy: f64,
    /// Crit chance (`critChance`, percentage points).
    pub crit_chance: f64,
    /// Base movement speed (`baseMovementSpeed`).
    pub base_movement_speed: f64,
    /// Fire resistance (`fireResist`, percentage points).
    pub fire_resist: f64,
    /// Cold resistance (`coldResist`).
    pub cold_resist: f64,
    /// Lightning resistance (`lightningResist`).
    pub lightning_resist: f64,
    /// Chaos resistance (`chaosResist`).
    pub chaos_resist: f64,
    /// Base damage ignores attack speed (`baseDamageIgnoresAttackSpeed`;
    /// absent in vendor = false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base_damage_ignores_attack_speed: bool,
    /// Raw string for the count-cap stat name (`limit`, e.g.
    /// `ActiveZombieLimit`; mapping it to an enum is the consumer's
    /// concern, see `pobr_data::minion::MinionLimitId::from_pob2`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    /// Spirit reservation (`spectreReservation`).
    pub spectre_reservation: f64,
    /// Companion reservation (`companionReservation`).
    pub companion_reservation: f64,
    /// Fire-resist override for the companion form (`companionFireResist`,
    /// added in PoB2 0.5.4; absent = falls back to the base form).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_fire_resist: Option<f64>,
    /// Cold-resist override for the companion form (`companionColdResist`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_cold_resist: Option<f64>,
    /// Lightning-resist override for the companion form
    /// (`companionLightningResist`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_lightning_resist: Option<f64>,
    /// Chaos-resist override for the companion form
    /// (`companionChaosResist`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_chaos_resist: Option<f64>,
    /// Monster category (`monsterCategory`, e.g. `Undead`/`Demon`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monster_category: Option<String>,
    /// Armour normalization multiplier (`armour`; absent in vendor →
    /// consumer treats it as 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armour: Option<f64>,
    /// Evasion normalization multiplier (`evasion`; absent in vendor →
    /// consumer treats it as 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evasion: Option<f64>,
    /// Life-to-ES conversion ratio (`energyShield`; absent in vendor = 0.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_shield: Option<f64>,
    /// Virtual weapon type 1 (`weaponType1`, used to derive ModFlags).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type1: Option<String>,
    /// Virtual weapon type 2 (`weaponType2`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type2: Option<String>,
    /// Spawn location (`spawnLocation`, spectre-only; empty for minions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawn_location: Vec<String>,
    /// Skill list (`skillList`, input to createMinionSkills).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_list: Vec<String>,
    /// Extra flags (keys with value true in `extraFlags`, ascending; e.g.
    /// `recommendedSpectre`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_flags: Vec<String>,
    /// Structured modList (a full transcription of each `mod(...)`/
    /// `flag(...)` construction).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mod_list: Vec<MinionModDef>,
}

/// Top level of `overlay/minions.json` / `overlay/spectres.json` (from the
/// consumer's perspective: the `_meta` header is provenance info, ignored
/// by serde along with other unknown fields — only the `minions` list is used).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MinionsDef {
    /// Entry list, ascending by `id`.
    pub minions: Vec<MinionEntryDef>,
}

/// A single record in the gem-granted-effect → minion foreign-key sidecar
/// (keyed by `effect_id`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrantedEffectMinionDef {
    /// Granted effect id (= `GrantedEffects.Id`, e.g. `RagingSpiritsPlayer`).
    pub effect_id: String,
    /// Ids of minions this skill summons (`minionList`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_list: Vec<String>,
    /// Minion ids a support adds (`addMinionList`; current vendor data has
    /// no such key — the field is reserved, `#[serde(default)]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_minion_list: Vec<String>,
    /// Player equipment slots the minion borrows from (keys with value true
    /// in `minionUses`, ascending; e.g. Manifest Weapon's `Weapon 1`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minion_uses: Vec<String>,
    /// Whether the minion uses its own separate item set
    /// (`minionHasItemSet`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minion_has_item_set: bool,
}

/// Top level of `overlay/granted_effect_minions.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GrantedEffectMinionsDef {
    /// Sidecar records, ascending by `effect_id`.
    pub entries: Vec<GrantedEffectMinionDef>,
}
