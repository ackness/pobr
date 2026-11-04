//! Hand-written `MinionDef` fixtures for tests (behind the `test-fixtures`
//! feature, enabled only by dev-dependencies, plus this crate's own tests).
//!
//! Production reads minions from the imported `overlay/minions.json`; these
//! five definitions let unit tests build a minion without loading data.
//! Values are from PoB2 src/Data/Minions.lua (dev branch, 2025-06).

use super::{MinionCategory, MinionDef, MinionLimitId};

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
        weapon_type1: None,
        weapon_type2: None,
        monster_tags: vec![
            "animal_claw_weapon".into(),
            "flesh_armour".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
        mod_list: Vec::new(),
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
        weapon_type1: None,
        weapon_type2: None,
        monster_tags: vec![
            "bone_armour".into(),
            "construct".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
        mod_list: Vec::new(),
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
        weapon_type1: None,
        weapon_type2: None,
        monster_tags: vec![
            "bone_armour".into(),
            "bones".into(),
            "melee".into(),
            "undead".into(),
        ],
        skill_list: vec!["MinionMeleeStep".into()],
        mod_list: Vec::new(),
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
        weapon_type1: None,
        weapon_type2: None,
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
        mod_list: Vec::new(),
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
        weapon_type1: None,
        weapon_type2: None,
        monster_tags: vec![],
        skill_list: vec![],
        mod_list: Vec::new(),
    }
}
