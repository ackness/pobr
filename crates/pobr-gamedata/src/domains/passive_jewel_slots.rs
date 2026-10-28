//! Game-sourced jewel socket slots, from a shared curated file.

use pobr_data::catalog::passive_jewel_slots::PassiveJewelSlotsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the version's overlay first, then `overlay-common/`.
    pub fn passive_jewel_slots(&self) -> Result<PassiveJewelSlotsDef, LoadError> {
        self.load_overlay_or_common("passive_jewel_slots.json")
    }
}
