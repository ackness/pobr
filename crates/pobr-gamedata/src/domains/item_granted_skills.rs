//! `overlay/item_granted_skills.json` loader: the rules for the skills that
//! items grant, schema in [`pobr_data::catalog::item_granted_skills`].

use pobr_data::catalog::item_granted_skills::ItemGrantedSkillsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the rules for item-granted skills: the version's `overlay/`
    /// first, `overlay-common/` otherwise ([`Self::load_overlay_or_common`]).
    /// Errors when neither has the file; what a data pack without it means is
    /// the consumer's call.
    pub fn item_granted_skills(&self) -> Result<ItemGrantedSkillsDef, LoadError> {
        self.load_overlay_or_common("item_granted_skills.json")
    }
}
