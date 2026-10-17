//! The monster each totem skill places, curated from the official game tables
//! (`ActiveSkills.SkillTotemId` → `SkillTotemVariations` → `MonsterVarieties`).
//! Provenance and export hashes live in `overlay-common/totem_varieties.json`.

use serde::{Deserialize, Serialize};

/// Top level of `overlay-common/totem_varieties.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TotemVarietiesDef {
    pub totems: Vec<TotemVarietyDef>,
}

/// One totem-summoning granted effect and the monster it places.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TotemVarietyDef {
    /// `GrantedEffects.Id` of the skill that summons the totem.
    pub granted_effect: String,
    /// `ActiveSkills.Id` (provenance).
    #[serde(default)]
    pub active_skill: String,
    /// `ActiveSkills.SkillTotemId` (provenance).
    #[serde(default)]
    pub skill_totem_id: u32,
    /// `MonsterVarieties.Id` of the skin-0 variation.
    pub monster_variety: String,
    /// `MonsterVarieties.LifeMultiplier`, in percent (100 = 1x).
    pub life_multiplier: u32,
    /// The variety's `MonsterTypes.Armour` / `Evasion`: percent of the level's
    /// `DefaultMonsterStats.Armour` / `Evasion` (100 = the table value).
    #[serde(default)]
    pub armour: u32,
    #[serde(default)]
    pub evasion: u32,
    /// `MonsterTypes.EnergyShieldFromLife`: maximum Energy Shield as a percent
    /// of maximum Life.
    #[serde(default)]
    pub energy_shield_from_life: u32,
}

impl Default for TotemVarietiesDef {
    /// The patch 0.5.5.4.2 rows, for a data pack without the file.
    fn default() -> Self {
        // MonsterTypes.Armour is 100 for every totem type but the Shockwave
        // and Dark Effigy totems (0); Evasion and EnergyShieldFromLife are 0.
        let row = |effect: &str, active: &str, id: u32, variety: &str, life: u32| TotemVarietyDef {
            granted_effect: effect.into(),
            active_skill: active.into(),
            skill_totem_id: id,
            monster_variety: format!("Metadata/Monsters/Totems/{variety}"),
            life_multiplier: life,
            armour: if matches!(variety, "ShockwaveTotem" | "DarkEffigyTotem_") {
                0
            } else {
                100
            },
            evasion: 0,
            energy_shield_from_life: 0,
        };
        Self {
            totems: vec![
                row(
                    "ShockwaveTotemPlayer",
                    "shockwave_totem",
                    5,
                    "ShockwaveTotem",
                    100,
                ),
                row(
                    "SummonMetaTotemBallistaPlayer",
                    "summon_meta_totem_ballista",
                    12,
                    "SnipeTotem",
                    100,
                ),
                row(
                    "AncestralWarriorTotemPlayer",
                    "ancestral_warrior_totem",
                    15,
                    "SlamTotem",
                    120,
                ),
                row(
                    "SiegeBallistaPlayer",
                    "siege_ballista",
                    19,
                    "UpwardBallista",
                    100,
                ),
                row(
                    "ArtilleryBallistaPlayer",
                    "artillery_ballista",
                    22,
                    "UpwardBallista",
                    100,
                ),
                row(
                    "DarkEffigyPlayer",
                    "dark_effigy",
                    24,
                    "DarkEffigyTotem_",
                    100,
                ),
                row(
                    "SummonMetaTotemSpellTotemPlayer",
                    "summon_meta_totem_spell",
                    25,
                    "SpellTotem",
                    100,
                ),
                row(
                    "MetaMortarCannonPlayer",
                    "mortar_cannon",
                    26,
                    "GrenadeTotem",
                    100,
                ),
            ],
        }
    }
}
