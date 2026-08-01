//! `base/weapon_types.json` loader (weapon type → one_hand/melee/flag/label,
//! sourced from PoB2's `data.weaponTypeInfo`, `Modules/Data.lua:532-551`).
//!
//! Located via the `base/`-first, version-root-fallback domain location
//! (see [`crate::paths`]); no i18n sidecar (`label` is PoB's English
//! display alias — localization goes through `pobr-i18n` later).

use pobr_data::catalog::weapon_types::WeaponTypeDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the weapon type table (sorted by `id`, diff-friendly; `None`
    /// = the unarmed entry).
    pub fn weapon_types(&self) -> Result<Vec<WeaponTypeDef>, LoadError> {
        self.load_json_at(self.domain_path("weapon_types.json"))
    }
}
