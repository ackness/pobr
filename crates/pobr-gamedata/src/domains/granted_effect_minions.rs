//! `overlay/granted_effect_minions.json` loader — the granted-effect →
//! minion foreign-key sidecar (owned per decision §4-10; vendor
//! `Data/Skills/*.lua`'s `minionList`/`minionUses`/`minionHasItemSet`,
//! extracted via `extract-lua --what minion-list`).
//!
//! Merging this into `GrantedEffectDef` (filling in `minion_list` and
//! other fields in the in-memory shape) is an A3 wiring concern; this
//! loader lands with the data first, zero wiring.

use pobr_data::catalog::actors::GrantedEffectMinionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the foreign-key sidecar (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (the consumer behaves
    /// as "the edge fields stay empty", backward compatible); other
    /// errors still propagate as usual.
    pub fn granted_effect_minions(&self) -> Result<Option<GrantedEffectMinionsDef>, LoadError> {
        match self.load_json_at::<GrantedEffectMinionsDef>(
            self.overlay_path("granted_effect_minions.json"),
        ) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
