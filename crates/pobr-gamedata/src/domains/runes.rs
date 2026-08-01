//! `overlay/runes.json` loader — the rune / soul-core socketed-mod table
//! (extracted from vendor `Data/ModRunes.lua` via
//! `extract-lua --what runes`, schema in
//! [`pobr_data::catalog::item_overlay`]).
//!
//! The consumer loads it separately on demand — it isn't part of ItemRules.

use pobr_data::catalog::item_overlay::RunesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the rune table (always resolved under `overlay/`). Returns
    /// `Ok(None)` when the file is missing (missing-table tolerance);
    /// other errors still propagate as usual.
    pub fn runes(&self) -> Result<Option<RunesDef>, LoadError> {
        match self.load_json_at::<RunesDef>(self.overlay_path("runes.json")) {
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
