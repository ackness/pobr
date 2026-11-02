//! `overlay/catalysts.json` loader — the catalyst quality-tag matching
//! table (PoB2-derived compatibility data from vendor `Classes/Item.lua`'s
//! three local table literals, imported from the RePoE owner package;
//! schema in [`pobr_data::catalog::item_overlay`]).
//!
//! Consumer (equivalent to `getCatalystScalar`: a catalystTags ∩ mod tags
//! match grants `(100+quality)/100`) injects it via RuleSet `ItemRules`,
//! zero wiring here.

use pobr_data::catalog::item_overlay::CatalystsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the catalyst table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (missing-table
    /// tolerance); other errors still propagate as usual.
    pub fn catalysts(&self) -> Result<Option<CatalystsDef>, LoadError> {
        match self.load_json_at::<CatalystsDef>(self.overlay_path("catalysts.json")) {
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
