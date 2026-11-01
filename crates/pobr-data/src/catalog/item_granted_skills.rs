//! Rules for the skills that items grant (`overlay/item_granted_skills.json`).
//!
//! A versioned community interpretation of `ItemInherentSkills` marks bases
//! whose item skills reserve no Spirit. The raw schema calls its third boolean
//! `IsWeapon`; the pinned community schema calls it `NoReservation`. The producer
//! records both identities instead of claiming an official field definition.

use serde::{Deserialize, Serialize};

/// Top level of `overlay/item_granted_skills.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemGrantedSkillsDef {
    /// Names of the base items whose granted skills reserve nothing.
    #[serde(default)]
    pub reservation_free_bases: Vec<String>,
    /// Item-specific effects whose display names also name ordinary gems.
    #[serde(default)]
    pub variants: Vec<ItemGrantedSkillVariant>,
}

/// A game `ModGrantedSkills → SkillGems → GemEffects` attachment, with the
/// ordinary effect ID used for this item skill by older character exporters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemGrantedSkillVariant {
    pub item_name: String,
    pub base_names: Vec<String>,
    pub skill_name: String,
    pub granted_effect_id: String,
    pub exported_effect_id: String,
    /// Stable `Mods.Id` establishing the native effect, retained for audits.
    pub game_mod_id: String,
}
