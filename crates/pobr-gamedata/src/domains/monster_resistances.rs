//! Official base resistances keyed by monster variety id.

use pobr_data::catalog::MonsterResistancesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads `base/monster_resistances.json`. Older snapshots without this
    /// domain return an empty table, retaining their overlay resistances.
    pub fn monster_resistances(&self) -> Result<MonsterResistancesDef, LoadError> {
        match self.load_domain("monster_resistances.json") {
            Ok(table) => Ok(table),
            Err(LoadError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Ok(MonsterResistancesDef::default())
            }
            Err(error) => Err(error),
        }
    }
}
