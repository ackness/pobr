//! `overlay/minions.json` loader — minion entries (extracted from vendor
//! `Data/Minions.lua` via
//! `sync-pob-catalog extract-lua --what minions`, schema in
//! [`pobr_data::catalog::actors`], /A5).
//!
//! Consumer (zero wiring): once the orchestrator recognizes a summon gem,
//! it looks up the `MinionEntryDef` by id and builds the input for
//! `env.add_minion_from_def`; the hand-transcribed constants
//! (`pobr_data::minion::minion_def_*`) are locked value-equal by a load
//! unit test and will be deleted at A6.

use pobr_data::catalog::actors::MinionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the minion entry table (always resolved under `overlay/`).
    /// Returns `Ok(None)` when the file is missing (an old data pack
    /// without this overlay domain) — missing-table tolerance; other
    /// I/O / parse errors still propagate, not silenced.
    pub fn minions(&self) -> Result<Option<MinionsDef>, LoadError> {
        match self.load_json_at::<MinionsDef>(self.overlay_path("minions.json")) {
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
