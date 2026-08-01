//! `overlay/mirage_configs.json` loader — the 5 kinds of mirage configs
//! (a hand-transcription of vendor `Modules/CalcMirages.lua`'s five
//! branches, generated embedded in `sync-pob-catalog gen-mirage-configs`;
//! schema in [`pobr_data::catalog::triggers`]).
//!
//! Consumer: once the orchestrator recognizes the trigger condition, it
//! calls the mirage sub-environment recompute framework; genuinely special
//! branches are looked up in the rules registry by `handler_id`.

use pobr_data::catalog::triggers::MirageConfigsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the mirage config table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (missing-table
    /// tolerance); other errors still propagate as usual.
    pub fn mirage_configs(&self) -> Result<Option<MirageConfigsDef>, LoadError> {
        match self.load_json_at::<MirageConfigsDef>(self.overlay_path("mirage_configs.json")) {
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
