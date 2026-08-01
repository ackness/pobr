//! `base/unarmed_data.json` loader (per-class unarmed base, sourced from
//! PoB2's `data.unarmedWeaponData`).

use pobr_data::catalog::unarmed_data::UnarmedWeaponDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the per-class unarmed weapon base (an array ascending by
    /// `class_id`; the weaponData source for attack skills when there's no
    /// main-hand weapon, PoB2's `data.unarmedWeaponData[classId]`).
    pub fn unarmed_data(&self) -> Result<Vec<UnarmedWeaponDef>, LoadError> {
        self.load_domain("unarmed_data.json")
    }
}
