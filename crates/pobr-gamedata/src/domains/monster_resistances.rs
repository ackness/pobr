//! Official base resistances keyed by monster variety id.

use pobr_data::catalog::MonsterResistancesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads `base/monster_resistances.json`. Older snapshots without this
    /// domain report a missing file; existing calculations do not require it.
    pub fn monster_resistances(&self) -> Result<MonsterResistancesDef, LoadError> {
        self.load_domain("monster_resistances.json")
    }
}
