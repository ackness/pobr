//! `base/monster_scaling.json` loader (the monster per-level tables,
//! sourced from PoB2 `Data/Misc.lua`'s monster tables).
//!
//! Schema in [`pobr_data::catalog::monster_scaling`]; located via
//! `load_domain`'s existing mechanism (`base/` first, falling back to the
//! version root).

use pobr_data::catalog::monster_scaling::MonsterScalingTable;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the monster per-level scaling table
    /// (accuracy/evasion/armour/life/the ally family/damage/
    /// ailment-threshold/poise-threshold, 100 entries each, indexed by
    /// level - 1).
    pub fn monster_scaling(&self) -> Result<MonsterScalingTable, LoadError> {
        self.load_domain("monster_scaling.json")
    }
}
