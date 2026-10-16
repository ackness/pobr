//! `overlay-common/totem_varieties.json` loader: the monster each totem skill
//! places, schema in [`pobr_data::catalog::totem_varieties`].

use pobr_data::catalog::totem_varieties::TotemVarietiesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the version's overlay first, then `overlay-common/`.
    pub fn totem_varieties(&self) -> Result<TotemVarietiesDef, LoadError> {
        self.load_overlay_or_common("totem_varieties.json")
    }
}
