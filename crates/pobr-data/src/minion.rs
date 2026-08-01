//! `MinionDef` storage schema — pure data, zero logic, zero I/O.
//!
//! Aligned with the other domains in `catalog.rs`; written offline to
//! `data/<poe_version>/minions.json` by `pobr-data-adapter`, deserialized at
//! runtime by `pobr-gamedata`.
//!
//! Design mirrors PoB2 `src/Data/Minions.lua`: each `MinionDef` corresponds
//! to one `minions["<Id>"]` entry in Lua, with field names following the
//! Lua originals (converted to snake_case).
//!
//! Sources:
//! - PoB2 `src/Data/Minions.lua` (each minion's normalization multipliers +
//!   limit + reservation amount).
//! - agent-docs/minions.md §1 (monster-style scaling), §4.1 (count caps),
//!   §4.2 (reservation).
//! - PoB2 `src/Modules/CalcActiveSkill.lua` (virtualWeapon / limit /
//!   hiddenDamageFixup).
//! - PoB2 `src/Modules/CalcPerform.lua` (limit → Multiplier:SummonedMinion).

use serde::{Deserialize, Serialize};

// Minion-count-cap stable IDs (correspond to PoB2 Lua's
// `data.minionLimitNames` / `Active<X>Limit`)

/// Stable ID for a minion count cap.
///
/// Each minion-summoning skill has its own limit; the count cap is
/// aggregated through the `base_number_of_<x>_allowed` stat and exposed as
/// `Multiplier:SummonedMinion` (referenced by per-minion mods).
///
/// Source: PoB2 `src/Data/SkillStatMap.lua` / `CalcPerform.lua`;
/// agent-docs/minions.md §4.1.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MinionLimitId {
    /// Raised Zombie cap.
    ActiveZombieLimit,
    /// Skeleton-family cap (shared by all Raise Skeleton variants).
    ActiveSkeletonLimit,
    /// Raging Spirit cap.
    ActiveRagingSpiritLimit,
    /// Spectre cap.
    ActiveSpectreLimit,
    /// Living Lightning cap.
    ActiveLivingLightningLimit,
    /// Infernal Hound cap (a separate companion slot doesn't count against
    /// this cap).
    ActiveHellhoundLimit,
    /// Companion cap for Infernal Hound (used when the skill data has its
    /// own separate companion limit).
    ActiveCompanionLimit,
    /// Bone Construct (Unearth family) cap.
    ActiveUnearthBoneConstructLimit,
    /// Wardbound cap (a new 0.5.0 summon skill).
    WardboundLimit,
    /// Hyena cap.
    HyenaLimit,
    /// Wolf cap (shared by Wolfpack / Companion).
    WolfLimit,
    /// Beetle cap.
    BeetleLimit,
    /// Azmerian Swarm cap.
    AzmerianSwarmLimit,
    /// No cap (e.g. Companion, Spirit Walker's Bear).
    None,
    /// A custom cap (a limit ID outside the enum, kept as the raw string so
    /// it can still be extended).
    Custom(String),
}

impl MinionLimitId {
    /// Parses a PoB2 Lua limit-name string into the enum value.
    pub fn from_pob2(s: &str) -> Self {
        match s {
            "ActiveZombieLimit" => Self::ActiveZombieLimit,
            "ActiveSkeletonLimit" => Self::ActiveSkeletonLimit,
            "ActiveRagingSpiritLimit" => Self::ActiveRagingSpiritLimit,
            "ActiveSpectreLimit" => Self::ActiveSpectreLimit,
            "ActiveLivingLightningLimit" => Self::ActiveLivingLightningLimit,
            "ActiveHellhoundLimit" => Self::ActiveHellhoundLimit,
            "ActiveCompanionLimit" => Self::ActiveCompanionLimit,
            "ActiveUnearthBoneConstructLimit" => Self::ActiveUnearthBoneConstructLimit,
            "WardboundLimit" => Self::WardboundLimit,
            "HyenaLimit" => Self::HyenaLimit,
            "WolfLimit" => Self::WolfLimit,
            "BeetleLimit" => Self::BeetleLimit,
            "AzmerianSwarmLimit" => Self::AzmerianSwarmLimit,
            "" => Self::None,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Returns the raw PoB2 Lua string (for serialization / debugging).
    pub fn to_pob2_str(&self) -> &str {
        match self {
            Self::ActiveZombieLimit => "ActiveZombieLimit",
            Self::ActiveSkeletonLimit => "ActiveSkeletonLimit",
            Self::ActiveRagingSpiritLimit => "ActiveRagingSpiritLimit",
            Self::ActiveSpectreLimit => "ActiveSpectreLimit",
            Self::ActiveLivingLightningLimit => "ActiveLivingLightningLimit",
            Self::ActiveHellhoundLimit => "ActiveHellhoundLimit",
            Self::ActiveCompanionLimit => "ActiveCompanionLimit",
            Self::ActiveUnearthBoneConstructLimit => "ActiveUnearthBoneConstructLimit",
            Self::WardboundLimit => "WardboundLimit",
            Self::HyenaLimit => "HyenaLimit",
            Self::WolfLimit => "WolfLimit",
            Self::BeetleLimit => "BeetleLimit",
            Self::AzmerianSwarmLimit => "AzmerianSwarmLimit",
            Self::None => "",
            Self::Custom(s) => s.as_str(),
        }
    }
}

// Minion category (corresponds to PoB2 Lua's `monsterCategory`)

/// A minion's monster category (affects condition checks for ailments /
/// mechanic-tag matching).
///
/// Source: PoB2 `src/Data/Minions.lua`'s `monsterCategory` field.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MinionCategory {
    Undead,
    Construct,
    Demon,
    Beast,
    Humanoid,
    /// An unknown/other category.
    Other(String),
}

impl MinionCategory {
    pub fn from_pob2(s: &str) -> Self {
        match s {
            "Undead" => Self::Undead,
            "Construct" => Self::Construct,
            "Demon" => Self::Demon,
            "Beast" => Self::Beast,
            "Humanoid" => Self::Humanoid,
            other => Self::Other(other.to_string()),
        }
    }
}

// MinionDef — the stored minion definition (pure data, deserialized by pobr-gamedata)

/// A minion's normalization-multiplier definition (corresponds to one entry
/// of PoB2's `src/Data/Minions.lua`).
///
/// **What each field means**:
/// - `life` / `damage` / `armour` / `evasion` / `energy_shield`: multiplied
///   against the monster-level baseline tables (`monsterAllyLifeTable`,
///   etc.) to get the base value. Defaults to 1.0.
/// - `damage_spread`: width of the min/max damage range. 0.0 means no range
///   (min == max == avg).
/// - `attack_time`: base attack interval (seconds); the virtual weapon's
///   `attack_rate = 1 / attack_time`.
/// - `crit_chance`: the virtual weapon's crit chance (default 5.0).
/// - `*_resist`: base resistance (%), injected directly as the minion's
///   `modDB.<Type>Resist BASE`.
/// - `base_damage_ignores_attack_speed`: whether base damage ignores
///   `attack_time`; `true` (Zombie / RagingSpirit, etc.) means attack speed
///   only affects DPS, not per-hit damage.
/// - `limit`: stable ID for the minion count cap; `None` means no explicit
///   cap (the Companion family).
/// - `spectre_reservation` / `companion_reservation`: the Spirit/companion
///   reservation a single minion consumes.
///
/// Source: PoB2 `src/Data/Minions.lua`; agent-docs/minions.md §1 / §4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinionDef {
    /// Stable ID (the key of Lua's `minions["<Id>"]`, e.g. `RaisedZombie`).
    pub id: String,
    /// English canonical name (Lua's `name`).
    pub name: String,
    /// Monster category (Lua's `monsterCategory`); `None` if the field is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<MinionCategory>,
    /// Life normalization multiplier (Lua's `life`; default 1.0).
    #[serde(default = "default_one")]
    pub life: f64,
    /// Damage normalization multiplier (Lua's `damage`; default 1.0).
    #[serde(default = "default_one")]
    pub damage: f64,
    /// Damage range width (Lua's `damageSpread`; default 0.0).
    #[serde(default)]
    pub damage_spread: f64,
    /// Base attack interval in seconds (Lua's `attackTime`; default 1.0).
    #[serde(default = "default_one")]
    pub attack_time: f64,
    /// Virtual weapon crit chance (Lua's `critChance`; default 5.0).
    #[serde(default = "default_five")]
    pub crit_chance: f64,
    /// Armour normalization multiplier (Lua's `armour`; default 1.0; some
    /// minions omit this field, which also means 1).
    #[serde(default = "default_one")]
    pub armour: f64,
    /// Evasion normalization multiplier (Lua's `evasion`; default 1.0).
    #[serde(default = "default_one")]
    pub evasion: f64,
    /// Life-to-ES conversion ratio (Lua's `energyShield`; 0.15 →
    /// LifeConvertToEnergyShield BASE=15; default 0.0).
    #[serde(default)]
    pub energy_shield: f64,
    /// Base fire resistance (%; Lua's `fireResist`; default 0.0).
    #[serde(default)]
    pub fire_resist: f64,
    /// Base cold resistance (%; default 0.0).
    #[serde(default)]
    pub cold_resist: f64,
    /// Base lightning resistance (%; default 0.0).
    #[serde(default)]
    pub lightning_resist: f64,
    /// Base chaos resistance (%; default 0.0).
    #[serde(default)]
    pub chaos_resist: f64,
    /// Whether base damage ignores attack speed (Lua's
    /// `baseDamageIgnoresAttackSpeed`; default false).
    #[serde(default)]
    pub base_damage_ignores_attack_speed: bool,
    /// Stable ID for the count cap (Lua's `limit`).
    #[serde(default = "default_limit_none")]
    pub limit: MinionLimitId,
    /// Spirit reservation a single minion consumes (Lua's
    /// `spectreReservation`; default 50.0).
    #[serde(default = "default_spectre_reservation")]
    pub spectre_reservation: f64,
    /// Companion reservation a single minion consumes (Lua's
    /// `companionReservation`; default 30.0).
    #[serde(default = "default_companion_reservation")]
    pub companion_reservation: f64,
    /// Whether this is a hostile minion (Lua's `hostile`; affects whether
    /// MinionModifier or EnemyModifier mods are forwarded to it).
    #[serde(default)]
    pub hostile: bool,
    /// Monster tag list (Lua's `monsterTags`; affects condition checks and
    /// keyword matching).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monster_tags: Vec<String>,
    /// Skill list (Lua's `skillList`; spell-casting minions get their base
    /// skill damage through this list).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_list: Vec<String>,
}

// serde default-value helper functions (`#[serde(default = "...")]` needs a fn() signature)

#[inline]
fn default_one() -> f64 {
    1.0
}

#[inline]
fn default_five() -> f64 {
    5.0
}

#[inline]
fn default_limit_none() -> MinionLimitId {
    MinionLimitId::None
}

#[inline]
fn default_spectre_reservation() -> f64 {
    50.0
}

#[inline]
fn default_companion_reservation() -> f64 {
    30.0
}

// Representative minion constants (built-in data, for use by unit tests /
// quick prototyping without JSON)
// Values are from PoB2 src/Data/Minions.lua (dev branch, 2025-06)

/// Default `MinionDef` constant for the Raised Zombie.
///
/// Source: PoB2 Minions.lua `minions["RaisedZombie"]`.
pub fn minion_def_zombie() -> MinionDef {
    MinionDef {
        id: "RaisedZombie".into(),
        name: "Raised Zombie".into(),
        category: Some(MinionCategory::Undead),
        life: 0.7,
        damage: 0.75,
        damage_spread: 0.3,
        attack_time: 1.25,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveZombieLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "animal_claw_weapon".into(),
            "flesh_armour".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
    }
}

/// Default `MinionDef` constant for the Raging Spirit.
///
/// Source: PoB2 Minions.lua `minions["SummonedRagingSpirit"]`.
pub fn minion_def_raging_spirit() -> MinionDef {
    MinionDef {
        id: "SummonedRagingSpirit".into(),
        name: "Raging Spirit".into(),
        category: Some(MinionCategory::Construct),
        life: 0.25,
        damage: 0.7,
        damage_spread: 0.2,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveRagingSpiritLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "bone_armour".into(),
            "construct".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
    }
}

/// Default `MinionDef` constant for the Skeletal Warrior.
///
/// Source: PoB2 Minions.lua `minions["RaisedSkeletonWarriors"]`.
pub fn minion_def_skeletal_warrior() -> MinionDef {
    MinionDef {
        id: "RaisedSkeletonWarriors".into(),
        name: "Skeletal Warrior".into(),
        category: Some(MinionCategory::Undead),
        life: 0.88,
        damage: 0.7,
        damage_spread: 0.3,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 0.5,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveSkeletonLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "bone_armour".into(),
            "bones".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
    }
}

/// Default `MinionDef` constant for the Skeletal Storm Mage.
///
/// Source: PoB2 Minions.lua `minions["RaisedSkeletonStormMage"]`.
pub fn minion_def_skeletal_storm_mage() -> MinionDef {
    MinionDef {
        id: "RaisedSkeletonStormMage".into(),
        name: "Skeletal Storm Mage".into(),
        category: Some(MinionCategory::Undead),
        life: 0.53,
        damage: 0.65,
        damage_spread: 0.3,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.15,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 50.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: true,
        limit: MinionLimitId::ActiveSkeletonLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![
            "bone_armour".into(),
            "bones".into(),
            "caster".into(),
            "undead".into(),
        ],
        skill_list: vec![
            "ArcSkeletonMageMinion".into(),
            "DeathStormSkeletonStormMageMinion".into(),
        ],
    }
}

/// Placeholder `MinionDef` constant for the Spectre (to be replaced with a
/// real Spectre entry once full data ingestion lands).
///
/// In PoB2, the Spectre is a special minion whose level is locked to the
/// area level rather than the character level; this only gives the schema
/// shape with placeholder values — be aware the level source differs when
/// computing with it (see agent-docs §1.1).
pub fn minion_def_spectre_placeholder() -> MinionDef {
    MinionDef {
        id: "Spectre".into(),
        name: "Raised Spectre (placeholder)".into(),
        category: None,
        life: 1.0,
        damage: 1.0,
        damage_spread: 0.2,
        attack_time: 1.0,
        crit_chance: 5.0,
        armour: 1.0,
        evasion: 1.0,
        energy_shield: 0.0,
        fire_resist: 0.0,
        cold_resist: 0.0,
        lightning_resist: 0.0,
        chaos_resist: 0.0,
        base_damage_ignores_attack_speed: false,
        limit: MinionLimitId::ActiveSpectreLimit,
        spectre_reservation: 50.0,
        companion_reservation: 30.0,
        hostile: false,
        monster_tags: vec![],
        skill_list: vec![],
    }
}

/// A snapshot of normalization multipliers extracted from a `MinionDef` (no
/// serde, used internally in memory).
///
/// Returned by [`MinionDef::scaling`], consumed by
/// `pobr-core::calc::minion::MinionData::from_def`. Doesn't introduce a
/// dependency on `pobr-core` (the data layer doesn't depend on core).
///
/// Source: agent-docs/minions.md §1; PoB2 CalcActiveSkill.lua `minionData.*`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionScaling {
    pub life: f64,
    pub damage: f64,
    pub damage_spread: f64,
    pub attack_time: f64,
    pub crit_chance: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    pub base_damage_ignores_attack_speed: bool,
}

// impl MinionDef

impl MinionDef {
    /// Extracts the normalization multipliers from a `MinionDef`, returning
    /// [`MinionScaling`].
    ///
    /// The caller (`pobr-core` / `pobr-build`) is responsible for copying
    /// the struct's fields into `MinionData`.
    ///
    /// Source: agent-docs/minions.md §1; PoB2 CalcActiveSkill.lua `minionData.*`.
    pub fn scaling(&self) -> MinionScaling {
        MinionScaling {
            life: self.life,
            damage: self.damage,
            damage_spread: self.damage_spread,
            attack_time: self.attack_time,
            crit_chance: self.crit_chance,
            armour: self.armour,
            evasion: self.evasion,
            energy_shield: self.energy_shield,
            fire_resist: self.fire_resist,
            cold_resist: self.cold_resist,
            lightning_resist: self.lightning_resist,
            chaos_resist: self.chaos_resist,
            base_damage_ignores_attack_speed: self.base_damage_ignores_attack_speed,
        }
    }

    /// Whether this minion has an energy shield (`energy_shield > 0`).
    pub fn has_energy_shield(&self) -> bool {
        self.energy_shield > 0.0
    }

    /// Whether this minion has an explicit count cap (`limit != None`).
    pub fn has_limit(&self) -> bool {
        !matches!(self.limit, MinionLimitId::None)
    }

    /// Builds the calc-consumption shape `MinionDef` from the stored
    /// overlay shape [`crate::catalog::actors::MinionEntryDef`] (a bridge:
    /// the loader produces `MinionEntryDef`, calc consumes `MinionDef`).
    ///
    /// Field alignment: `armour`/`evasion` default to 1.0 and
    /// `energy_shield` defaults to 0.0 when absent (`MinionEntryDef`
    /// faithfully transcribes vendor's "field absent" semantics; this
    /// function is where the consumer-side defaults get materialized — see
    /// `MinionEntryDef`'s doc). The `limit` string is mapped through
    /// [`MinionLimitId::from_pob2`]; `monster_category` through
    /// [`MinionCategory::from_pob2`]. `mod_list` isn't carried over for now
    /// (the calc-side C3 assembly reads it directly from the entry, to
    /// avoid modeling it twice).
    pub fn from_entry(e: &crate::catalog::actors::MinionEntryDef) -> Self {
        Self {
            id: e.id.clone(),
            name: e.name.clone(),
            category: e.monster_category.as_deref().map(MinionCategory::from_pob2),
            life: e.life,
            damage: e.damage,
            damage_spread: e.damage_spread,
            attack_time: e.attack_time,
            crit_chance: e.crit_chance,
            armour: e.armour.unwrap_or(1.0),
            evasion: e.evasion.unwrap_or(1.0),
            energy_shield: e.energy_shield.unwrap_or(0.0),
            fire_resist: e.fire_resist,
            cold_resist: e.cold_resist,
            lightning_resist: e.lightning_resist,
            chaos_resist: e.chaos_resist,
            base_damage_ignores_attack_speed: e.base_damage_ignores_attack_speed,
            limit: e
                .limit
                .as_deref()
                .map(MinionLimitId::from_pob2)
                .unwrap_or(MinionLimitId::None),
            spectre_reservation: e.spectre_reservation,
            companion_reservation: e.companion_reservation,
            hostile: false,
            monster_tags: e.monster_tags.clone(),
            skill_list: e.skill_list.clone(),
        }
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zombie_def_matches_pob2() {
        // PoB2 minions["RaisedZombie"]: life=0.7, damage=0.75, attackTime=1.25
        let def = minion_def_zombie();
        assert_eq!(def.id, "RaisedZombie");
        assert_eq!(def.life, 0.7);
        assert_eq!(def.damage, 0.75);
        assert!((def.damage_spread - 0.3).abs() < 1e-9);
        assert!((def.attack_time - 1.25).abs() < 1e-9);
        assert_eq!(def.crit_chance, 5.0);
        assert!(def.base_damage_ignores_attack_speed);
        assert_eq!(def.limit, MinionLimitId::ActiveZombieLimit);
        assert_eq!(def.energy_shield, 0.0);
        assert!(!def.hostile);
    }

    #[test]
    fn raging_spirit_def_matches_pob2() {
        // PoB2 minions["SummonedRagingSpirit"]: life=0.25, damage=0.7, damageSpread=0.2
        let def = minion_def_raging_spirit();
        assert_eq!(def.id, "SummonedRagingSpirit");
        assert_eq!(def.life, 0.25);
        assert_eq!(def.damage, 0.7);
        assert!((def.damage_spread - 0.2).abs() < 1e-9);
        assert_eq!(def.limit, MinionLimitId::ActiveRagingSpiritLimit);
    }

    #[test]
    fn skeletal_warrior_def_armour_normalizer() {
        // PoB2 minions["RaisedSkeletonWarriors"]: armour=0.5
        let def = minion_def_skeletal_warrior();
        assert!((def.armour - 0.5).abs() < 1e-9);
        assert_eq!(def.limit, MinionLimitId::ActiveSkeletonLimit);
    }

    #[test]
    fn storm_mage_has_energy_shield() {
        // PoB2 minions["RaisedSkeletonStormMage"]: energyShield=0.15, lightningResist=50
        let def = minion_def_skeletal_storm_mage();
        assert!(def.has_energy_shield());
        assert!((def.energy_shield - 0.15).abs() < 1e-9);
        assert!((def.lightning_resist - 50.0).abs() < 1e-9);
    }

    #[test]
    fn limit_id_roundtrip_from_pob2() {
        let cases = [
            ("ActiveZombieLimit", MinionLimitId::ActiveZombieLimit),
            ("ActiveSkeletonLimit", MinionLimitId::ActiveSkeletonLimit),
            (
                "ActiveRagingSpiritLimit",
                MinionLimitId::ActiveRagingSpiritLimit,
            ),
            ("ActiveSpectreLimit", MinionLimitId::ActiveSpectreLimit),
            (
                "ActiveLivingLightningLimit",
                MinionLimitId::ActiveLivingLightningLimit,
            ),
            (
                "ActiveUnearthBoneConstructLimit",
                MinionLimitId::ActiveUnearthBoneConstructLimit,
            ),
            ("WardboundLimit", MinionLimitId::WardboundLimit),
            ("WolfLimit", MinionLimitId::WolfLimit),
            ("BeetleLimit", MinionLimitId::BeetleLimit),
            ("AzmerianSwarmLimit", MinionLimitId::AzmerianSwarmLimit),
        ];
        for (s, expected) in cases {
            let got = MinionLimitId::from_pob2(s);
            assert_eq!(got, expected, "from_pob2({s})");
            assert_eq!(got.to_pob2_str(), s, "to_pob2_str({s}) roundtrip");
        }
    }

    #[test]
    fn limit_id_none_from_empty() {
        assert_eq!(MinionLimitId::from_pob2(""), MinionLimitId::None);
        assert_eq!(MinionLimitId::None.to_pob2_str(), "");
    }

    #[test]
    fn limit_id_custom_unknown() {
        let custom = MinionLimitId::from_pob2("SomeUnknownLimit");
        assert!(matches!(custom, MinionLimitId::Custom(ref s) if s == "SomeUnknownLimit"));
    }

    #[test]
    fn def_has_limit_works() {
        assert!(minion_def_zombie().has_limit());
        let placeholder = minion_def_spectre_placeholder();
        assert!(placeholder.has_limit()); // Spectre has ActiveSpectreLimit
        let no_limit = MinionDef {
            id: "NoLimitMinion".into(),
            name: "No Limit".into(),
            category: None,
            life: 1.0,
            damage: 1.0,
            damage_spread: 0.0,
            attack_time: 1.0,
            crit_chance: 5.0,
            armour: 1.0,
            evasion: 1.0,
            energy_shield: 0.0,
            fire_resist: 0.0,
            cold_resist: 0.0,
            lightning_resist: 0.0,
            chaos_resist: 0.0,
            base_damage_ignores_attack_speed: false,
            limit: MinionLimitId::None,
            spectre_reservation: 50.0,
            companion_reservation: 30.0,
            hostile: false,
            monster_tags: vec![],
            skill_list: vec![],
        };
        assert!(!no_limit.has_limit());
    }

    #[test]
    fn scaling_returns_correct_fields() {
        let def = minion_def_zombie();
        let s = def.scaling();
        assert_eq!(s.life, 0.7);
        assert_eq!(s.damage, 0.75);
        assert!((s.damage_spread - 0.3).abs() < 1e-9);
        assert!((s.attack_time - 1.25).abs() < 1e-9);
        assert_eq!(s.crit_chance, 5.0);
        assert_eq!(s.armour, 1.0);
        assert_eq!(s.evasion, 1.0);
        assert_eq!(s.energy_shield, 0.0);
        assert_eq!(s.fire_resist, 0.0);
        assert_eq!(s.cold_resist, 0.0);
        assert_eq!(s.lightning_resist, 0.0);
        assert_eq!(s.chaos_resist, 0.0);
        assert!(s.base_damage_ignores_attack_speed);
    }

    #[test]
    fn category_roundtrip() {
        let cases = [
            ("Undead", MinionCategory::Undead),
            ("Construct", MinionCategory::Construct),
            ("Demon", MinionCategory::Demon),
            ("Beast", MinionCategory::Beast),
            ("Humanoid", MinionCategory::Humanoid),
        ];
        for (s, expected) in cases {
            assert_eq!(MinionCategory::from_pob2(s), expected, "{s}");
        }
    }

    #[test]
    fn serde_roundtrip_zombie() {
        // Verify serde traits are derivable: serialize and deserialize the def.
        // We can't use serde_json here (no dev-dep), but we can verify Clone + PartialEq work.
        let def = minion_def_zombie();
        let cloned = def.clone();
        assert_eq!(cloned.id, def.id);
        assert_eq!(cloned.life, def.life);
        assert_eq!(cloned.limit, def.limit);
        assert!((cloned.attack_time - def.attack_time).abs() < 1e-12);
    }
}
