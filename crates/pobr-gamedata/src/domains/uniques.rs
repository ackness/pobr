//! `overlay/uniques.json` loader — unique-item raw text blocks + a
//! pre-parsed index (two layers; PoB2-derived compatibility data from
//! vendor `Data/Uniques/*.lua`, imported from the RePoE owner package;
//! schema in [`pobr_data::catalog::item_overlay`]).
//!
//! The consumer loads it separately on demand — it isn't part of
//! ItemRules; parsing the mod template lines is done at runtime by
//! pobr-item.

use pobr_data::catalog::item_overlay::UniquesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the unique-item table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (missing-table
    /// tolerance); other errors still propagate as usual.
    pub fn uniques(&self) -> Result<Option<UniquesDef>, LoadError> {
        match self.load_json_at::<UniquesDef>(self.overlay_path("uniques.json")) {
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
