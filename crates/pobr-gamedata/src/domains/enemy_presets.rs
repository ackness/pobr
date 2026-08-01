//! `base/enemy_presets.json` loader (the four `enemyIsBoss` tier presets,
//! sourced from PoB2 `Modules/ConfigOptions.lua`'s enemy section +
//! `Modules/Data.lua`'s multiplier constants).
//!
//! Schema in [`pobr_data::catalog::enemy_presets`]; located via
//! `load_domain`'s existing mechanism (`base/` first, falling back to the
//! version root).

use pobr_data::catalog::enemy_presets::EnemyPresetsTable;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the enemy tier preset table (the None/Boss/Pinnacle/Uber four
    /// tiers' mod groups plus per-type defaults for
    /// resistance/armour-evasion multipliers/penetration/DPS multiplier, etc.).
    pub fn enemy_presets(&self) -> Result<EnemyPresetsTable, LoadError> {
        self.load_domain("enemy_presets.json")
    }
}
