//! Rules for the skills that items grant (`overlay/item_granted_skills.json`).
//!
//! The game's table of the skills a base item grants (`ItemInherentSkills`)
//! flags some bases `NoReservation`: the skill such an item grants reserves no
//! Spirit, although the same skill reserves Spirit as a gem. The Lament,
//! Portent and Absent Amulets are those bases; the skills they grant reserve
//! 30, 60 and 100 Spirit as gems.

use serde::{Deserialize, Serialize};

/// Top level of `overlay/item_granted_skills.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemGrantedSkillsDef {
    /// Names of the base items whose granted skills reserve nothing.
    #[serde(default)]
    pub reservation_free_bases: Vec<String>,
}
